use raft_kv::node::Node;
use raft_kv::raft::RequestVoteRequest;
use raft_kv::raft::raft_client::RaftClient;
use raft_kv::raft::raft_server::RaftServer;
use std::time::Duration;
use tonic::transport::Server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let id: u64 = std::env::args()
        .nth(1)
        .expect("usage: raft_node <id>")
        .parse()?;
    // node 1 -> 5001, node 2 -> 5002, node 3 -> 5003
    let port = 5000 + id;
    let addr = format!("127.0.0.1:{port}").parse()?;

    // --- server half: spawn so it runs in the background ---
    tokio::spawn(async move {
        Server::builder()
            .add_service(RaftServer::new(Node { id }))
            .serve(addr)
            .await
            .unwrap();
    });

    // give the other nodes time to start listening
    tokio::time::sleep(Duration::from_secs(3)).await;

    // --- client half: call every peer ---
    for peer in 1..=3u64 {
        if peer == id {
            continue;
        }
        let mut client = RaftClient::connect(format!("http://127.0.0.1:{}", 5000 + peer)).await?;
        let resp = client
            .request_vote(RequestVoteRequest {
                candidate_id: id,
                term: 3,
                last_log_index: 8,
                last_log_term: 3,
            })
            .await?;
        println!(
            "node {id} -> {peer}: vote_granted={}",
            resp.into_inner().vote_granted
        );
    }

    // keep the server alive so the others can still reach us
    tokio::time::sleep(Duration::from_secs(5)).await;
    Ok(())
}
