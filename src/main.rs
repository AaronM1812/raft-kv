use raft_kv::storage::Storage;
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() {
    let storage = Arc::new(RwLock::new(Storage::new("wal.log")));
    raft_kv::server::run("127.0.0.1:7878", storage).await;
}
