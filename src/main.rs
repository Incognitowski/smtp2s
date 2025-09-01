use smtp2s::storage::local::LocalFileStorage;
use smtp2s::{run_server, setup_logging};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _observability_guard = setup_logging();

    // TODO: parse storage strategy from config file
    let storage_path = PathBuf::from("/home/incognitowski/Desktop/tmp-storage");
    let storage = LocalFileStorage {
        base_path: storage_path,
    };

    // TODO: parse port from config file
    run_server("127.0.0.1:8080", storage).await
}
