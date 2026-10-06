//all binaries can now access storage
pub mod server;
pub mod storage;
pub mod raft {
    tonic::include_proto!("raft");
}
pub mod node;
