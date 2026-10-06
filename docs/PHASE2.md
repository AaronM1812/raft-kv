# Phase 2 — Raft Consensus

## RaftNode struct
- Name: `RaftNode`
- Lives in: `src/node.rs`
- One per process. Every node runs both a gRPC server (receives RPCs) and holds a gRPC client per peer (sends RPCs).

### Persistent — fsync'd before replying to any RPC
- `current_term: u64` — logical clock; increments on every election attempt, never decreases
- `voted_for: Option<u64>` — who I voted for in `current_term`; `None` if not yet voted; reset to `None` on term change
- `log: Vec<LogEntry>` — `LogEntry { term: u64, command: Command }`; index 0 is a dummy entry so indices match the paper (1-based)

### Volatile — rebuilt or reset on restart
- `role: Role` — `enum Role { Follower, Candidate, Leader }`; always Follower on startup
- `commit_index: u64` — highest log index known to be replicated on a majority *(understand loosely — revisit before Replication)*
- `last_applied: u64` — highest log index actually applied to `Storage`; trails `commit_index` *(revisit)*

### Volatile, leader only — reset on becoming leader
- `next_index: HashMap<u64, u64>` — per follower, next log index to send them *(revisit)*
- `match_index: HashMap<u64, u64>` — per follower, highest index confirmed replicated *(revisit)*

### Command
```rust
enum Command {
    Put { key: String, value: Vec<u8> },
    Delete { key: String },
}
```
No GET — reads don't change state so never enter the log. Applying an entry means matching on `Command` and calling the existing `Storage::put` / `Storage::delete`. `storage.rs` is unchanged.

### Why `u64` everywhere
Protobuf integers are `uint32`/`uint64`. Picking `u64` for terms, indices and node IDs means no casts between the in-memory struct and the proto messages.

## RequestVote RPC
Sent by a candidate to every peer when its election timer fires. Only exists during elections — it is *not* a heartbeat.

### Request
- `term: u64` — candidate's `current_term` (already incremented)
- `candidate_id: u64`
- `last_log_index: u64` — index of candidate's last log entry
- `last_log_term: u64` — term of that entry

### Response
- `term: u64` — receiver's `current_term`, so a stale candidate can update itself
- `vote_granted: bool`

### Receiver rules, in order
0. If `req.term > current_term`: set `current_term = req.term`, `voted_for = None`, `role = Follower`, fsync. *(Universal rule — applies to every RPC received, in every role.)*
1. `req.term < current_term` → `false`
2. `voted_for` is `Some(x)` and `x != candidate_id` → `false` *(same candidate retrying is fine)*
3. Candidate's log is behind mine → `false`
4. Otherwise: `voted_for = Some(candidate_id)`, fsync, → `true`

### Why `last_log_index` / `last_log_term`
Only committed entries are safe. A candidate with a shorter log may be missing committed entries; if it won, it would overwrite them on followers. So the vote doubles as a log check: "is your log at least as up-to-date as mine?"

**Up-to-date rule:** compare last terms — higher term wins. If equal, longer log wins. A follower never votes for a candidate that loses this comparison, regardless of term.

### What a stale candidate does
Receives `vote_granted: false` with a higher `term` → updates `current_term`, drops to Follower. The follower that replied does **not** become a candidate — it just answered a message. The only way to become a candidate is your own timer expiring.

## State machine

```
                 timer expires
   ┌─────────┐ ───────────────► ┌───────────┐  majority votes  ┌────────┐
   │Follower │                  │ Candidate │ ───────────────► │ Leader │
   └─────────┘ ◄─────────────── └───────────┘                  └────────┘
        ▲        higher term OR       │  ▲                          │
        │        valid AppendEntries  └──┘ timer expires,           │
        │        from a leader             no winner → new term     │
        └───────────────────────────────────────────────────────────┘
                              higher term seen
```

### Transitions
1. Follower → Candidate — election timer expires without hearing from a leader
2. Candidate → Leader — receives votes from a majority (own vote counts)
3. Candidate → Follower — sees a higher term, **or** receives AppendEntries from a leader with term ≥ own (someone else won)
4. Candidate → Candidate — timer expires with no winner (split vote); increment term, vote for self, retry
5. Leader → Follower — sees a higher term in any request or response
6. Any → Follower — on process restart

No path Follower → Leader. No path Leader → Candidate. Every leader was a candidate first.

### What each role does
- **Follower** — passive. Replies to RPCs. Resets election timer on every valid AppendEntries or when granting a vote. Never sends anything unprompted.
- **Candidate** — increments term, votes for self, resets timer, sends RequestVote to all peers, counts `vote_granted: true` replies.
- **Leader** — sends AppendEntries to every follower every heartbeat interval. Empty entries = heartbeat. Non-empty = replication. Never times out.

### Universal rule
Any message, any role: if the incoming `term > current_term`, adopt the new term, clear `voted_for`, become Follower. Do this *before* processing the message.

## Timeouts
- Election timeout: random in **150–300ms**, re-randomised every time it's reset
- Heartbeat interval: **50ms**, fixed

### Why heartbeat ≪ election timeout
The follower's timer resets on every heartbeat. If heartbeats arrived slower than the timeout, healthy followers would time out and start elections against a live leader. 50ms vs 150ms minimum means a follower tolerates two consecutive lost heartbeats before acting — one dropped packet doesn't cause an election.

### Why the election timeout is random
Fixed timeouts → all followers time out together → all become candidates in the same term → split vote → repeat forever. Randomising spreads them out so one node usually times out first, wins before the others fire, and its heartbeats reset their timers. Split votes still happen but the next round is randomised again, so they don't repeat.

### Why 150–300ms specifically
Paper's recommendation. Must be ≫ network round-trip (~1ms locally) so a vote can complete before others time out, and ≪ MTBF so downtime after a leader crash is short. Revisit in Phase 4 benchmarks if leader failover is slower than expected.

## AppendEntries RPC
*Deferred until the Replication step. For Election, an empty AppendEntries with `term` + `leader_id` is sufficient as a heartbeat. Read §5.3 and fill this section in before touching replication.*

## Still unsure about
- AppendEntries full semantics — `prev_log_index`/`prev_log_term` consistency check, how `entries[]` is reconciled, how `leader_commit` advances follower `commit_index`. Section 3 above.
- `commit_index` / `last_applied` / `next_index` / `match_index` — know what they hold, not yet how they move. Read §5.3 before section 3.
