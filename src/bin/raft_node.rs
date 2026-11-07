use raft_kv::node::Node;
use raft_kv::raft::raft_server::RaftServer;
use tonic::transport::Server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let id: u64 = std::env::args()
        .nth(1)
        .expect("usage: raft_node <id> [base_port]")
        .parse()?;
    // optional, so tests can run a cluster away from the default ports
    let base_port: u64 = match std::env::args().nth(2) {
        Some(p) => p.parse()?,
        None => 5000,
    };
    // node 1 -> 5001, node 2 -> 5002, node 3 -> 5003
    let port = base_port + id;
    let addr = format!("127.0.0.1:{port}").parse()?;

    let node = Node::new(id, base_port);

    // timer runs in the background; the clone shares the same state (Arc inside Node)
    tokio::spawn(node.clone().run_election_timer());

    // server runs in the foreground, which keeps the process alive
    println!("node {id} listening on {addr}");
    Server::builder()
        .add_service(RaftServer::new(node))
        .serve(addr)
        .await?;
    Ok(())
}
