use crate::raft::raft_client::RaftClient;
use crate::raft::raft_server::Raft;
use crate::raft::{
    AppendEntriesRequest, AppendEntriesResponse, RequestVoteRequest, RequestVoteResponse,
};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use tokio::time::{sleep, timeout};
use tonic::{Request, Response, Status};

#[derive(Clone)]
pub struct Node {
    pub id: u64,
    peers: Vec<u64>,
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
            peers: (1..=3).filter(|&p| p != id).collect(), // 3-node cluster for now
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
        // 1. become a candidate, under the lock
        let (election_term, last_log_index, last_log_term) = {
            let mut state = self.state.lock().await;
            state.role = Role::Candidate;
            state.current_term += 1;
            state.voted_for = Some(self.id);

            // persist before asking anyone, or a restart could
            // vote twice this term
            if let Err(e) = save_state(self.id, state.current_term, state.voted_for) {
                eprintln!("node {} failed to save state: {e}", self.id);
                return;
            }

            state.last_heartbeat = Instant::now(); // restart own timer

            println!(
                "node {} starting election for term {}",
                self.id, state.current_term
            );

            (
                state.current_term, // snapshot: compared against each reply later
                state.log.len() as u64,
                state.log.last().map_or(0, |e| e.term),
            )
        }; // lock released: never hold it across network calls

        // 2. ask every peer at once, one task each, so a dead
        // peer only stalls its own task
        let mut tasks = JoinSet::new();

        for &peer in &self.peers {
            let req = RequestVoteRequest {
                term: election_term,
                candidate_id: self.id,
                last_log_index,
                last_log_term,
            };

            tasks.spawn(async move {
                let call = async move {
                    let addr = format!("http://127.0.0.1:{}", 5000 + peer);
                    let mut client = RaftClient::connect(addr).await.ok()?;

                    Some(client.request_vote(req).await.ok()?.into_inner())
                };

                // no reply within 100 ms counts as no vote
                timeout(Duration::from_millis(100), call)
                    .await
                    .ok()
                    .flatten()
            });
        }

        // 3. count replies as they arrive
        let majority = (self.peers.len() + 1) / 2 + 1; // whole cluster incl. me: 3 nodes -> 2
        let mut votes = 1; // my own vote

        while let Some(result) = tasks.join_next().await {
            let Ok(Some(resp)) = result else {
                continue;
            }; // dead/slow peer: no vote

            let mut state = self.state.lock().await;

            // someone is in a newer term: step down
            if resp.term > state.current_term {
                state.current_term = resp.term;
                state.voted_for = None;
                state.role = Role::Follower;

                if let Err(e) = save_state(self.id, state.current_term, state.voted_for) {
                    eprintln!("node {} failed to save state: {e}", self.id);
                }

                return;
            }

            // stale: a heartbeat made me Follower, or a newer
            // election started
            if state.role != Role::Candidate || state.current_term != election_term {
                return;
            }

            if resp.vote_granted {
                votes += 1;

                if votes >= majority {
                    state.role = Role::Leader;

                    println!(
                        "node {} became leader for term {election_term} ({votes} votes)",
                        self.id
                    );

                    return; // dropping the JoinSet cancels any replies still pending
                }
            }

            // no majority yet: the timer loop starts the next
            // election
        }
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
