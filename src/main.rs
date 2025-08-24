use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tracing::{error, info, instrument};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{Registry, fmt};

mod smtp2s;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _logging_guard = setup_logging();
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    info!(address = %listener.local_addr()?, "Server listening");

    loop {
        let (socket, addr) = listener.accept().await?;
        tokio::spawn(handle_client(socket, addr));
    }
}

fn setup_logging() -> WorkerGuard {
    let (non_blocking_writer, _guard) =
        tracing_appender::non_blocking(tracing_appender::rolling::daily("logs", "smtp2s.log"));
    Registry::default()
        .with(LevelFilter::INFO)
        .with(fmt::layer().with_writer(std::io::stdout).pretty())
        .with(fmt::layer().with_writer(non_blocking_writer).json())
        .init();
    _guard
}

#[instrument(name = "client_handler", skip(socket), fields(client.addr = %addr))]
async fn handle_client(mut socket: TcpStream, addr: SocketAddr) {
    info!("Connection accepted");
    let mut buf = vec![0; 1024];
    let mut message_metadata = smtp2s::models::Metadata::new();
    let mut state = smtp2s::models::State::Initialized;
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

        let response = smtp2s::handle_message(&buf[0..n], &mut message_metadata, &mut state);

        let mut entire_response = response.join("\r\n".as_bytes());
        entire_response.extend_from_slice(b"\r\n");

        // Still have to learn how to handle broken pipe exceptions
        let _ = socket.write_all(&entire_response).await;
    }
}
