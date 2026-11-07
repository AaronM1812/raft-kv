// Leader failover: kill the leader of a real 3-node cluster and time how
// long the survivors take to elect a new one. Nodes run as subprocesses so
// the kill is a real SIGKILL, like the Phase 1 crash test.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

const BASE_PORT: u64 = 6000; // away from the demo's 5001-5003
const RUNS: usize = 10;
// max election timeout (300ms) plus a vote round trip
const FAILOVER_LIMIT: Duration = Duration::from_millis(350);
// a split vote costs one more election round: up to another max timeout
const ROUND_TIMEOUT: Duration = Duration::from_millis(300);

// (node id, term it won, when the line was read)
type LeaderEvent = (u64, u64, Instant);

// Kills the node when dropped, so a failed assert or panic can't leave
// processes running and holding the ports
struct NodeProcess(Child);

impl Drop for NodeProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn_node(id: u64, dir: &Path, tx: Sender<LeaderEvent>) -> NodeProcess {
    let mut child = Command::new(env!("CARGO_BIN_EXE_raft_node"))
        .args([id.to_string(), BASE_PORT.to_string()])
        .current_dir(dir) // own state files, not the repo's
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to start raft_node");

    // one reader thread per node: a quiet node only blocks its own thread
    let stdout = child.stdout.take().unwrap();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            // "node 2 became leader for term 3 (2 votes)"
            if let Some(rest) = line.split("became leader for term ").nth(1) {
                let term = rest.split(' ').next().unwrap().parse().unwrap();
                if tx.send((id, term, Instant::now())).is_err() {
                    return; // test finished
                }
            }
        }
    });

    NodeProcess(child)
}

// Wait for a leader in a term newer than `after`. One leader per term, so
// a higher term means a new election was won; older lines are skipped
fn wait_for_leader(rx: &Receiver<LeaderEvent>, after: u64) -> LeaderEvent {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok(event) if event.1 > after => return event,
            Ok(_) => continue,
            Err(_) => panic!("no leader newer than term {after} within 3s"),
        }
    }
}

#[test]
fn test_leader_failover() {
    let dir = std::env::temp_dir().join(format!("raftkv-failover-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let (tx, rx) = mpsc::channel();
    let mut nodes: Vec<NodeProcess> = (1..=3).map(|id| spawn_node(id, &dir, tx.clone())).collect();

    let (mut leader, mut term, _) = wait_for_leader(&rx, 0);

    for run in 1..=RUNS {
        let i = (leader - 1) as usize;
        nodes[i].0.kill().unwrap();
        let killed_at = Instant::now(); // clock starts once the leader is gone

        let (new_leader, new_term, elected_at) = wait_for_leader(&rx, term);
        let took = elected_at - killed_at;
        // terms go up by one per election, so each skipped term is an
        // election that nobody won; each one costs up to another 300ms timeout
        let failed_rounds = (new_term - term - 1) as u32;
        let limit = FAILOVER_LIMIT + ROUND_TIMEOUT * failed_rounds;
        println!(
            "run {run}: killed node {leader}, node {new_leader} won term {new_term} in {took:?}{}",
            if failed_rounds > 0 { format!(" ({failed_rounds} split vote(s))") } else { String::new() }
        );
        assert!(took < limit, "run {run}: failover {took:?} is over {limit:?}");

        // dead node rejoins as a follower; give it time to catch up on the
        // term before the next kill
        nodes[i] = spawn_node(leader, &dir, tx.clone());
        thread::sleep(Duration::from_millis(500));

        (leader, term) = (new_leader, new_term);
    }

    drop(nodes);
    let _ = std::fs::remove_dir_all(&dir);
}
