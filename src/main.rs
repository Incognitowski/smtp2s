use aws_config::meta::region::RegionProviderChain;
use aws_config::BehaviorVersion;
use aws_sdk_s3::{Client, Config};
use dotenvy::dotenv;
// use smtp2s::storage::local::LocalFileStorage;
use smtp2s::storage::s3::S3FileStorage;
use smtp2s::{run_server, setup_logging};
// use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();

    let _observability_guard = setup_logging();

    // Build local storage strategy
    // TODO: parse storage strategy from config file
    // let storage_path = PathBuf::from("/home/incognitowski/Desktop/tmp-storage");
    // let local_file_storage = LocalFileStorage {
    //     base_path: storage_path,
    // };
    // Build S3 based storage strategy
    let s3_file_storage = build_s3_file_storage("smtp2s-data-storage".to_string()).await;

    // TODO: parse port from config file
    run_server("127.0.0.1:8080", s3_file_storage).await
}

async fn build_s3_file_storage(
    bucket_name: String,
) -> S3FileStorage {
    let region_provider = RegionProviderChain::default_provider().or_else("us-east-1");
    // Gets the default AWS config from environment (~/.aws/config)
    let shared_config = aws_config::defaults(BehaviorVersion::v2025_08_07())
        .region(region_provider)
        .load()
        .await;
    let client = match std::env::var("AWS_ENDPOINT_OVERRIDE") {
        Ok(endpoint) => {
            let config = Config::builder()
                .credentials_provider(shared_config.credentials_provider().unwrap())
                .region(shared_config.region().cloned())
                .endpoint_url(endpoint)
                .force_path_style(true)
                .build();
            Client::from_conf(config)
        }
        _ => Client::new(&shared_config),
    };

    return S3FileStorage::new(client, bucket_name)
}
