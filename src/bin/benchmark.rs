use raft_kv::storage::Storage;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() {
    // sequential
    let _ = std::fs::remove_file("bench_seq.log");
    let _ = std::fs::remove_file("bench_conc.log");
    let mut storage = Storage::new("bench_seq.log");
    let start = Instant::now();

    for i in 0..10000 {
        storage.put(&format!("key:{}", i), format!("value:{}", i).into_bytes());
    }
    let elapsed = start.elapsed();
    println!("sequential: {:.0} ops/sec", 10000.0 / elapsed.as_secs_f64());

    // concurrent
    // Arc<RwLock<Storage>>, spawn N tasks each doing 10000/N puts,
    // collect handles, await all, time the whole thing
    let storage = Arc::new(RwLock::new(Storage::new("bench_conc.log")));
    let mut handles = Vec::new();
    let start = Instant::now();
    for client_id in 0..5 {
        let storage = storage.clone();
        let handle = tokio::spawn(async move {
            for i in 0..2000 {
                storage.write().await.put(&format!("c{}:key:{}", client_id, i), format!("value:{}", i).into_bytes());
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }
    let elapsed = start.elapsed();
    println!("concurrent: {:.0} ops/sec", 10000.0 / elapsed.as_secs_f64());
}