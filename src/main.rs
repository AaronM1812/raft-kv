use std::sync::Arc;
use tokio::sync::RwLock;
use raft_kv::storage::Storage;

#[tokio::main]
async fn main() {
    let storage = Arc::new(RwLock::new(Storage::new("wal.log")));
    raft_kv::server::run(storage).await;
}
