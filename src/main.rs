use std::fs::File;
use std::io::BufReader;
use std::str::FromStr;

use aws_config::meta::region::RegionProviderChain;
use aws_config::BehaviorVersion;
use aws_sdk_s3::{Client, Config};
use clap::{command, Parser, ValueEnum};
use dotenvy::dotenv;
use serde::Deserialize;
use smtp2s::run_server;
use smtp2s::storage::local::LocalFileStorage;
use smtp2s::storage::s3::S3FileStorage;
use smtp2s::storage::Storage;
use std::path::PathBuf;
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, Layer, Registry};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Smpt2sArgs {
    #[arg(long)]
    config_file: String,
    #[arg(long, default_value = "INFO")]
    log_level: String,
    #[arg(long, value_enum, default_value_t = LoggingType::Pretty)]
    stdout_log_kind: LoggingType,
    #[arg(long, value_enum, default_value_t = LoggingType::JSON)]
    file_log_kind: LoggingType,
    #[arg(long, default_value = "logs")]
    file_log_dir: String,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum LoggingType {
    None,
    Pretty,
    JSON,
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
    allowed_addresses: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();
    info!("Parsing args...");
    let args = Smpt2sArgs::parse();

    let _observability_guard = setup_logging(
        &args.log_level,
        &args.stdout_log_kind,
        &args.file_log_kind,
        &args.file_log_dir,
    );

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

    run_server(
        &format!("127.0.0.1:{}", config.port),
        storage_strategy,
        &config.allowed_addresses,
    )
    .await
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

pub fn setup_logging(
    log_level: &str,
    stdout_log_kind: &LoggingType,
    file_log_kind: &LoggingType,
    file_log_dir: &str,
) -> WorkerGuard {
    let (non_blocking_writer, _guard) = tracing_appender::non_blocking(
        tracing_appender::rolling::daily(file_log_dir, "smtp2s.log"),
    );

    let stdout_layer = match stdout_log_kind {
        LoggingType::Pretty => Some(fmt::layer().with_writer(std::io::stdout).pretty().boxed()),
        LoggingType::JSON => Some(fmt::layer().with_writer(std::io::stdout).json().boxed()),
        LoggingType::None => None,
    };

    let file_layer = match file_log_kind {
        LoggingType::Pretty => Some(
            fmt::layer()
                .with_writer(non_blocking_writer)
                .pretty()
                .boxed(),
        ),
        LoggingType::JSON => Some(fmt::layer().with_writer(non_blocking_writer).json().boxed()),
        LoggingType::None => None,
    };

    Registry::default()
        .with(LevelFilter::from_str(log_level).unwrap_or(LevelFilter::INFO))
        .with(stdout_layer)
        .with(file_layer)
        .init();
    _guard
}
