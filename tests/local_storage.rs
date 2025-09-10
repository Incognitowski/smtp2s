use std::fs;

use lettre::{
    transport::smtp::authentication::{Credentials, Mechanism},
    AsyncSmtpTransport,
    AsyncTransport,
    Message,
    Tokio1Executor,
};
use smtp2s::run_server;
use smtp2s::storage::local::LocalFileStorage;
use tempfile::tempdir;
use tokio::net::TcpListener;

#[tokio::test]
async fn test_email_delivery_to_local_storage() {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    // 1. Set up a temporary directory for file-based storage
    let storage_dir = tempdir().unwrap();
    let storage_path = storage_dir.path().to_path_buf();

    // 2. Configure and run the server in a background task
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let allowed_addresses = vec!["test@example.com".to_string()];

    let server_storage_path = storage_path.clone();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let server_handle = tokio::spawn(async move {
        let storage = Box::new(LocalFileStorage {
            base_path: server_storage_path,
        });
        run_server(
            listener,
            storage,
            &allowed_addresses,
            shutdown_rx,
        )
        .await
        .unwrap();
    });

    // 3. Use an async SMTP client to connect and send an email
    let email = Message::builder()
        .from("test@example.com".parse().unwrap())
        .to("user@example.net".parse().unwrap())
        .subject("Test Email")
        .body("Hello, world!".to_string())
        .unwrap();

    let client = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&addr.ip().to_string())
        .port(addr.port())
        .credentials(Credentials::new("test@example.com".to_string(), "password".to_string()))
        .authentication(vec![Mechanism::Login])
        .build();

    client.send(email).await.unwrap();

    // 4. Shutdown the server and wait for it to complete
    let _ = shutdown_tx.send(());
    server_handle.await.unwrap();

    // 5. NOW, assert that the email was correctly saved to storage
    let entries: Vec<_> = fs::read_dir(&storage_path).unwrap().map(|r| r.unwrap()).collect();
    assert_eq!(entries.len(), 1, "Should be one new directory in the storage path");

    let message_dir_path = entries[0].path();
    assert!(message_dir_path.is_dir());

    let mut message_files: Vec<_> = fs::read_dir(&message_dir_path).unwrap().map(|r| r.unwrap()).collect();
    message_files.sort_by_key(|f| f.path());
    
    assert_eq!(message_files.len(), 3, "Should be three items: metadata, body, and attachments dir");

    let attachments_dir = &message_files[0];
    let body_file = &message_files[1];
    let metadata_file = &message_files[2];

    assert_eq!(attachments_dir.file_name(), "attachments");
    assert!(attachments_dir.path().is_dir());

    assert_eq!(body_file.file_name(), "body.html");
    assert_eq!(metadata_file.file_name(), "metadata.json");

    let metadata_content = fs::read_to_string(metadata_file.path()).unwrap();
    let metadata_json: serde_json::Value = serde_json::from_str(&metadata_content).unwrap();

    assert_eq!(metadata_json["from"], "test@example.com");
    assert_eq!(metadata_json["to"][0], "user@example.net");
    assert_eq!(metadata_json["subject"], "Test Email");

    let body_file_content = fs::read_to_string(body_file.path()).unwrap();
    assert!(body_file_content.contains("Hello, world!"));
}