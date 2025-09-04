use std::fs::File;
use std::io::BufReader;

use aws_config::meta::region::RegionProviderChain;
use aws_config::BehaviorVersion;
use aws_sdk_s3::{Client, Config};
use clap::{command, Parser};
use dotenvy::dotenv;
use serde::Deserialize;
use smtp2s::storage::local::LocalFileStorage;
use smtp2s::storage::s3::S3FileStorage;
use smtp2s::storage::Storage;
use smtp2s::{run_server, setup_logging};
use std::path::PathBuf;
use tracing::info;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Smpt2sArgs {
    #[arg(short, long)]
    config_file: String,
    #[arg(short, long, default_value = "INFO")]
    log_level : String,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
enum Strategy {
    Local {
        base_path: String,
    },
    S3 {
        bucket_name: String,
        override_aws_endpoint: Option<String>,
    },
}

#[derive(Deserialize, Debug)]
struct Smpt2sConfig {
    port: i32,
    strategy: Strategy,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();
    info!("Parsing args...");
    let args = Smpt2sArgs::parse();

    let _observability_guard = setup_logging(&args.log_level);

    info!("About to read config file from {}", args.config_file);
    let file = File::open(args.config_file)?;
    let reader = BufReader::new(file);
    info!("Parsing config file contents...");
    let config: Smpt2sConfig = serde_json::from_reader(reader)?;
    info!("Parsed config contents: {:?}", config);

    let storage_strategy: Box<dyn Storage> = match config.strategy {
        Strategy::Local { base_path } => {
            let storage_path = PathBuf::from(base_path);
            Box::new(LocalFileStorage {
                base_path: storage_path,
            })
        }
        Strategy::S3 {
            bucket_name,
            override_aws_endpoint,
        } => Box::new(build_s3_file_storage(bucket_name, override_aws_endpoint).await),
    };

    run_server(&format!("127.0.0.1:{}", config.port), storage_strategy).await
}

async fn build_s3_file_storage(
    bucket_name: String,
    override_aws_endpoint: Option<String>,
) -> S3FileStorage {
    let region_provider = RegionProviderChain::default_provider().or_else("us-east-1");
    // Gets the default AWS config from environment (~/.aws/config)
    let shared_config = aws_config::defaults(BehaviorVersion::v2025_08_07())
        .region(region_provider)
        .load()
        .await;
    let client = match override_aws_endpoint {
        Some(endpoint) => {
            let config = Config::builder()
                .credentials_provider(shared_config.credentials_provider().unwrap())
                .region(shared_config.region().cloned())
                .endpoint_url(endpoint)
                .force_path_style(true)
                .build();
            Client::from_conf(config)
        }
        None => Client::new(&shared_config),
    };

    return S3FileStorage::new(client, bucket_name);
}
