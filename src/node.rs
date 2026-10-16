use crate::raft::raft_server::Raft;
use crate::raft::{
    AppendEntriesRequest, AppendEntriesResponse, RequestVoteRequest, RequestVoteResponse,
};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::time::sleep;
use tonic::{Request, Response, Status};

#[derive(Clone)]
pub struct Node {
    pub id: u64,
    state: Arc<Mutex<State>>,
}

pub struct State {
    pub current_term: u64,
    pub voted_for: Option<u64>,
    pub log: Vec<LogEntry>,
    pub role: Role,
    pub commit_index: u64,
    pub last_applied: u64,
    pub last_heartbeat: Instant,
}

pub struct LogEntry {
    pub term: u64,
    pub command: Command,
}

#[derive(PartialEq)]
pub enum Role {
    Follower,
    Candidate,
    Leader,
}

pub enum Command {
    Put { key: String, value: Vec<u8> },
    Delete { key: String },
}

impl Node {
    pub fn new(id: u64) -> Self {
        let (current_term, voted_for) = load_state(id).expect("failed to read raft_state");
        let state = State {
            current_term,
            voted_for,
            log: Vec::new(),
            role: Role::Follower,
            commit_index: 0,
            last_applied: 0,
            last_heartbeat: Instant::now(),
        };
        Node {
            id,
            state: Arc::new(Mutex::new(state)),
        }
    }

    pub async fn run_election_timer(self) {
        loop {
            let timeout = Duration::from_millis(rand::random_range(150..=300));
            sleep(timeout).await;
            let state = self.state.lock().await;
            if state.role == Role::Leader {
                continue; // leader: ignore, go back to sleep
            }
            if state.last_heartbeat.elapsed() >= timeout {
                drop(state); // release the lock first
                self.start_election().await;
            }
        }
    }

    async fn start_election(&self) {
        todo!() // you write this next
    }
}

fn state_path(id: u64) -> String {
    format!("raft_state_{id}")
}

// Write to a temp file, fsync, then rename over the real one, so a crash never leaves it half-written.
fn save_state(id: u64, term: u64, voted_for: Option<u64>) -> io::Result<()> {
    let tmp = format!("{}.tmp", state_path(id));
    let mut f = File::create(&tmp)?;
    f.write_all(&term.to_be_bytes())?;
    f.write_all(&voted_for.unwrap_or(0).to_be_bytes())?; // 0 = no vote
    f.sync_all()?;
    fs::rename(&tmp, state_path(id))
}

fn load_state(id: u64) -> io::Result<(u64, Option<u64>)> {
    let mut buf = [0u8; 16];
    match File::open(state_path(id)) {
        Ok(mut f) => f.read_exact(&mut buf)?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok((0, None)), // first boot
        Err(e) => return Err(e),
    }
    let term = u64::from_be_bytes(buf[0..8].try_into().unwrap());
    let vote = u64::from_be_bytes(buf[8..16].try_into().unwrap());
    Ok((term, if vote == 0 { None } else { Some(vote) }))
}

#[tonic::async_trait]
impl Raft for Node {
    async fn request_vote(
        &self,
        req: Request<RequestVoteRequest>,
    ) -> Result<Response<RequestVoteResponse>, Status> {
        let req = req.into_inner(); // unwrap the gRPC envelope to get your struct
        let mut state = self.state.lock().await;

        // 0. newer term seen: adopt it and step down (universal rule)
        if req.term > state.current_term {
            state.current_term = req.term;
            state.voted_for = None;
            state.role = Role::Follower;
            save_state(self.id, state.current_term, state.voted_for)
                .map_err(|e| Status::internal(e.to_string()))?;
        }

        // up-to-date check: higher last term wins; equal terms, longer log wins
        let my_last_index = state.log.len() as u64;
        let my_last_term = state.log.last().map_or(0, |e| e.term);
        let log_ok = req.last_log_term > my_last_term
            || (req.last_log_term == my_last_term && req.last_log_index >= my_last_index);

        let vote_granted = if req.term < state.current_term {
            false // 1. stale candidate
        } else if state.voted_for.is_some_and(|v| v != req.candidate_id) {
            false // 2. already voted for someone else this term
        } else {
            log_ok // 3. reject if candidate's log is behind mine
        };

        // 4. record the vote on disk before replying
        if vote_granted {
            state.voted_for = Some(req.candidate_id);
            save_state(self.id, state.current_term, state.voted_for)
                .map_err(|e| Status::internal(e.to_string()))?;
        }

        println!(
            "node {} got RequestVote from {} term {} -> granted={vote_granted}",
            self.id, req.candidate_id, req.term
        );
        Ok(Response::new(RequestVoteResponse {
            term: state.current_term,
            vote_granted,
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
