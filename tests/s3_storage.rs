use aws_config::meta::region::RegionProviderChain;
use aws_config::BehaviorVersion;
use aws_sdk_s3::{config::Credentials as S3Credentials, types::Delete, Client, Config};
use lettre::{
    transport::smtp::authentication::{Credentials, Mechanism},
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use smtp2s::run_server;
use smtp2s::storage::s3::S3FileStorage;
use tokio::net::TcpListener;

const TEST_BUCKET_NAME: &str = "smtp2s-data-storage";

async fn get_s3_client() -> Client {
    let region_provider = RegionProviderChain::default_provider().or_else("us-east-1");
    let credentials = S3Credentials::new("test".to_string(), "test".to_string(), None, None, "test");
    let shared_config = aws_config::defaults(BehaviorVersion::v2025_08_07())
        .region(region_provider)
        .credentials_provider(credentials)
        .load()
        .await;
    let config = Config::builder()
        .credentials_provider(shared_config.credentials_provider().unwrap())
        .region(shared_config.region().cloned())
        .endpoint_url("http://localhost:4566")
        .force_path_style(true)
        .build();
    Client::from_conf(config)
}

async fn cleanup_bucket(s3_client: &Client) {
    let objects_output = s3_client
        .list_objects_v2()
        .bucket(TEST_BUCKET_NAME)
        .send()
        .await
        .unwrap();

    let objects = objects_output.contents();
    if objects.is_empty() {
        return;
    }

    let keys_to_delete: Vec<_> = objects
        .iter()
        .map(|o| {
            aws_sdk_s3::types::ObjectIdentifier::builder()
                .key(o.key().unwrap())
                .build()
                .unwrap()
        })
        .collect();

    let delete_list = Delete::builder()
        .set_objects(Some(keys_to_delete))
        .build()
        .unwrap();

    s3_client
        .delete_objects()
        .bucket(TEST_BUCKET_NAME)
        .delete(delete_list)
        .send()
        .await
        .unwrap();
}

#[tokio::test]
async fn test_email_delivery_to_s3_storage() {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();
    let s3_client = get_s3_client().await;

    // Ensure the bucket is clean before running the test
    cleanup_bucket(&s3_client).await;

    let _ = s3_client
        .create_bucket()
        .bucket(TEST_BUCKET_NAME)
        .send()
        .await;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let allowed_addresses = vec!["test@example.com".to_string()];

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let server_handle = tokio::spawn(async move {
        let storage = Box::new(S3FileStorage::new(
            get_s3_client().await,
            TEST_BUCKET_NAME.to_string(),
        ));
        run_server(listener, storage, &allowed_addresses, shutdown_rx)
            .await
            .unwrap();
    });

    let email = Message::builder()
        .from("test@example.com".parse().unwrap())
        .to("user@example.net".parse().unwrap())
        .subject("Test Email")
        .body("Hello, world!".to_string())
        .unwrap();

    let client = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&addr.ip().to_string())
        .port(addr.port())
        .credentials(Credentials::new(
            "test@example.com".to_string(),
            "password".to_string(),
        ))
        .authentication(vec![Mechanism::Login])
        .build();

    client.send(email).await.unwrap();

    // Shutdown the server BEFORE checking the results
    let _ = shutdown_tx.send(());
    server_handle.await.unwrap();

    // NOW check S3 for the results
    let objects = s3_client
        .list_objects_v2()
        .bucket(TEST_BUCKET_NAME)
        .send()
        .await
        .unwrap();

    assert_eq!(
        objects.key_count(),
        Some(2),
        "Should be two objects in the bucket"
    );

    let keys: Vec<String> = objects
        .contents()
        .iter()
        .map(|o| o.key().unwrap().to_string())
        .collect();
    let ulid_prefix = keys[0].split('/').next().unwrap();

    let metadata_key = format!("{}/metadata.json", ulid_prefix);
    let content_key = format!("{}/body.html", ulid_prefix);

    assert!(keys.contains(&metadata_key));
    assert!(keys.contains(&content_key));

    let metadata_object = s3_client
        .get_object()
        .bucket(TEST_BUCKET_NAME)
        .key(metadata_key)
        .send()
        .await
        .unwrap();

    let metadata_bytes = metadata_object.body.collect().await.unwrap().into_bytes();
    let metadata_json: serde_json::Value = serde_json::from_slice(&metadata_bytes).unwrap();

    assert_eq!(metadata_json["from"], "test@example.com");
    assert_eq!(metadata_json["to"][0], "user@example.net");
    assert_eq!(metadata_json["subject"], "Test Email");

    let content_object = s3_client
        .get_object()
        .bucket(TEST_BUCKET_NAME)
        .key(content_key)
        .send()
        .await
        .unwrap();

    let content_bytes = content_object.body.collect().await.unwrap().into_bytes();
    let content_str = String::from_utf8(content_bytes.to_vec()).unwrap();

    assert!(content_str.contains("Hello, world!"));
}
