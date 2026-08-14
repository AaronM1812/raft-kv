//here im creating integration test
//we first need to create a storage, with the name for our wal
//then run the server
//and then connect our client
//and run some tests

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use raft_kv::storage::Storage;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::net::TcpStream;

#[tokio::test]
async fn test_put_and_get_over_tcp() {
    let _ = std::fs::remove_file("test_tcp.log");
    let storage = Arc::new(RwLock::new(Storage::new("test_tcp.log")));

    tokio::spawn(async move {
        raft_kv::server::run("127.0.0.1:7879", storage).await;
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let stream = TcpStream::connect("127.0.0.1:7879").await.unwrap();
    // client work goes here
    let mut reader = BufReader::new(stream);
    let mut response = String::new();

    for i in 0..100{
        response.clear();
        let cmd = format!("PUT key:{}\nvalue:{}\n", i, i);
        reader.write_all(cmd.as_bytes()).await.unwrap();

        reader.read_line(&mut response).await.unwrap();
        assert_eq!(response, "OK\n");
        
        let cmd = format!("GET key:{}\n", i);
        reader.write_all(cmd.as_bytes()).await.unwrap();

        response.clear();
        reader.read_line(&mut response).await.unwrap();

        let len: usize = response.trim_end().splitn(2, ' ').nth(1).unwrap().parse().unwrap();

        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf).await.unwrap();

        let expected = format!("value:{}", i);
        assert_eq!(buf, expected.as_bytes());
    }
}

#[tokio::test]
async fn test_concurrent_clients(){
    let _ = std::fs::remove_file("test_tcp_concurrent.log");
    let storage = Arc::new(RwLock::new(Storage::new("test_tcp_concurrent.log")));

    tokio::spawn(async move {
        raft_kv::server::run("127.0.0.1:7880", storage).await;
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let mut handles = Vec::new();

    for client_id in 0..5 {
        let handle = tokio::spawn(async move {
            let stream = TcpStream::connect("127.0.0.1:7880").await.unwrap();
            let mut reader = BufReader::new(stream);
            let mut response = String::new();

            for i in 0..10 {
                response.clear();
                let cmd = format!("PUT c{}:key:{}\nvalue:{}\n", client_id, i, i);
                reader.write_all(cmd.as_bytes()).await.unwrap();

                reader.read_line(&mut response).await.unwrap();
                assert_eq!(response, "OK\n");
                
                let cmd = format!("GET c{}:key:{}\n", client_id, i);
                reader.write_all(cmd.as_bytes()).await.unwrap();

                response.clear();
                reader.read_line(&mut response).await.unwrap();

                let len: usize = response.trim_end().splitn(2, ' ').nth(1).unwrap().parse().unwrap();

                let mut buf = vec![0u8; len];
                reader.read_exact(&mut buf).await.unwrap();

                let expected = format!("value:{}", i);
                assert_eq!(buf, expected.as_bytes());
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

}