pub mod smtp;
pub mod storage;

use std::net::SocketAddr;
use std::str::FromStr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tracing::{debug, error, info, instrument};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, Registry};

use crate::smtp::protocol::handle_message;
use crate::storage::Storage;

pub async fn run_server(
    addr: &str,
    storage_strategy: Box<dyn Storage>,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting TCP server...");
    let listener = TcpListener::bind(addr).await?;
    info!("Server listening on port {}", listener.local_addr()?.port());

    let storage = std::sync::Arc::new(storage_strategy);

    loop {
        let (socket, addr) = listener.accept().await?;
        let storage_strategy = storage.clone();
        tokio::spawn(handle_client(socket, addr, storage_strategy));
    }
}

#[instrument(name = "client_handler", skip(socket, storage), fields(client.addr = %addr))]
async fn handle_client(
    mut socket: TcpStream,
    addr: SocketAddr,
    storage: std::sync::Arc<Box<dyn Storage>>,
) {
    info!("Connection accepted");
    let mut buf = vec![0; 1024];
    let mut data_vec: Vec<u8> = vec![];
    let mut message_metadata = smtp::models::Metadata::default();
    let mut state = smtp::models::State::Initialized;
    let _ = socket
        .write_all(b"220 localhost ESMTP Service Ready\r\n")
        .await;
    loop {
        let n = match socket.read(&mut buf).await {
            Ok(0) => {
                info!("Connection closed by client");
                return;
            }
            Err(e) => {
                error!(error.message = %e, "Failed to read from socket");
                return;
            }
            Ok(n) => n,
        };

        let response = handle_message(
            &buf[0..n],
            &mut message_metadata,
            &mut state,
            &mut data_vec,
            &**storage,
        )
        .await;

        if matches!(state, smtp::models::State::ProvidingData) && response.is_empty() {
            debug!("Accepted data package, waiting for more or delimiter.");
            continue;
        }

        let mut entire_response = response.join("\r\n".as_bytes());
        entire_response.extend_from_slice(b"\r\n");

        debug!(
            "About to reply with: {}",
            String::from_utf8(entire_response.clone()).unwrap()
        );

        if socket.write_all(&entire_response).await.is_err() {
            error!("Failed to write to socket. Broken pipe.");
            return;
        }
    }
}

pub fn setup_logging(log_level: &str) -> WorkerGuard {
    let (non_blocking_writer, _guard) =
        tracing_appender::non_blocking(tracing_appender::rolling::daily("logs", "smtp2s.log"));
    Registry::default()
        .with(LevelFilter::from_str(log_level).unwrap_or(LevelFilter::INFO))
        .with(fmt::layer().with_writer(std::io::stdout).pretty())
        .with(fmt::layer().with_writer(non_blocking_writer).json())
        .init();
    _guard
}
