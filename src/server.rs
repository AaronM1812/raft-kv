use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::storage::Storage;

pub async fn run(storage: Arc<RwLock<Storage>>){
    // Bind the listener to the address
    let listener = TcpListener::bind("127.0.0.1:7878").await.unwrap();

    loop {
        // The second item contains the IP and port of the new connection.
        let (socket, _) = listener.accept().await.unwrap();
        let storage = storage.clone();
        tokio::spawn(async move {
            process(socket, storage).await;
        });
    }
}

async fn process(socket: TcpStream, storage: Arc<RwLock<Storage>>) {
    println!("client connected");
    let mut reader = BufReader::new(socket);
    let mut line = String::new();
    loop {
        let n = reader.read_line(&mut line).await.unwrap();
        if n == 0 { break; }
        let trimmed = line.trim_end();
        let mut parts = trimmed.splitn(2, ' ');
        let command = parts.next().unwrap();
        let key = parts.next();

        //split first word
        //match on it
        match command {
            "GET" => {match key {
                Some(k) => {
                    let result = storage.read().await.get(k);
                    match result {
                        Some(v) => {
                                    let header = format!("OK {}\n", v.len());
                                    reader.write_all(header.as_bytes()).await.unwrap();
                                    reader.write_all(&v).await.unwrap();
                        }
                        None => {reader.write_all(b"NOT_FOUND\n").await.unwrap();}
                    }
                }

                None => { reader.write_all(b"ERR missing key\n").await.unwrap();}
            }
            }
            "DELETE" => {match key {
                Some(k) => {
                    storage.write().await.delete(k);
                    reader.write_all(b"OK\n").await.unwrap();
                }
                None => { reader.write_all(b"ERR missing key\n").await.unwrap(); }
                }
            }

            "PUT" => {
                match key {
                    Some(k) => {
                        let mut value_line = String::new();
                        let n = reader.read_line(&mut value_line).await.unwrap();
                        if n == 0 { break; }
                        let value = value_line.trim_end();
                        storage.write().await.put(k, value.as_bytes().to_vec());
                        reader.write_all(b"OK\n").await.unwrap();
                    }
                    None => { reader.write_all(b"ERR missing key\n").await.unwrap(); }
                }
            }

            _ => { reader.write_all(b"ERR unknown command\n").await.unwrap(); }
        }
        line.clear();
    }
}


