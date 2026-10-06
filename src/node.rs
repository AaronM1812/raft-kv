use crate::raft::raft_server::Raft;
use crate::raft::{
    AppendEntriesRequest, AppendEntriesResponse, RequestVoteRequest, RequestVoteResponse,
};
use tonic::{Request, Response, Status};

pub struct Node {
    pub id: u64,
}

#[tonic::async_trait]
impl Raft for Node {
    async fn request_vote(
        &self,
        req: Request<RequestVoteRequest>,
    ) -> Result<Response<RequestVoteResponse>, Status> {
        let req = req.into_inner(); // unwrap th gRPC envelope to get your struct
        println!(
            "node {} got RequestVote from {} term {}",
            self.id, req.candidate_id, req.term
        );
        Ok(Response::new(RequestVoteResponse {
            term: req.term,
            vote_granted: true,
        }))
    }

    async fn append_entries(
        &self,
        req: Request<AppendEntriesRequest>,
    ) -> Result<Response<AppendEntriesResponse>, Status> {
        let req = req.into_inner(); // unwrap th gRPC envelope to get your struct
        println!(
            "node {} got AppendEntries from {} term {}",
            self.id, req.leader_id, req.term
        );
        Ok(Response::new(AppendEntriesResponse {
            term: req.term,
            success: true,
        }))
    }
}
