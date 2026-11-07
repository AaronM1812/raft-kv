# Phase 2 — Raft Consensus

## RaftNode struct
- Name: `RaftNode`
- Lives in: `src/node.rs`
- One per process. Every node runs both a gRPC server (receives RPCs) and holds a gRPC client per peer (sends RPCs).
- State lives behind one `Arc<Mutex<>>` — the RPC handlers and the election timer all mutate it. Mutex over RwLock: nearly every access writes.

### Persistent — fsync'd before replying to any RPC
`current_term` + `voted_for` → own small file, overwritten on change. `log` → own append-only file. Phase 1 WAL untouched.

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
Sent by the leader to every follower, to replicate log entries. With `entries` empty it is the heartbeat, but it still runs the consistency check and carries `leader_commit`, so heartbeats and replication are the same code.

### Request
- `term: u64` — leader's `current_term`
- `leader_id: u64`
- `prev_log_index: u64` — index of the entry just before the new ones
- `prev_log_term: u64` — term of that entry
- `entries: Vec<LogEntry>` — new entries to store (empty for a heartbeat)
- `leader_commit: u64` — leader's `commit_index`, so followers learn what is committed

`next_index` and `match_index` are **not** sent. They're leader-only state, one per follower: `next_index` is the next entry to send that follower, `match_index` the highest entry known to be on it and matching the leader's log.

### Response
- `term: u64` — receiver's `current_term`
- `success: bool` — `true` if the follower matched `prev_log_index`/`prev_log_term` and stored the entries

`success: false` has two causes, and `term` tells them apart:
- reply `term` > leader's term → the leader is stale: step down (universal rule), don't retry
- otherwise → the `prev` entry didn't match: decrement `next_index` and retry from one entry further back

### Why `prev_log_index` / `prev_log_term`
Log Matching Property: if two logs have an entry with the same index and term, that entry holds the same command and every entry before it is identical. The follower only accepts new entries if it has the `prev` entry, so each accepted batch extends a log that already matches the leader's up to that point. That's why sending the new entries alone isn't enough: the follower would have no way to know its log matches the leader's before them.

### Receiver rules, in order
0. If `req.term > current_term`: set `current_term = req.term`, `voted_for = None`, `role = Follower`, fsync. *(Universal rule.)*
1. `req.term < current_term` → `false` *(stale leader; my term in the reply makes it step down)*
2. Valid leader for my term: if Candidate, back to Follower; reset the election timer. *(Already done for heartbeats.)*
3. No entry at `prev_log_index` with term `prev_log_term` → `false`. Covers both cases: my log is too short, or the entry there has a different term. *(`prev_log_index = 0` always matches: the empty start of the log.)*
4. For each new entry: if I already have an entry at that index with a **different** term, delete it and everything after it.
5. Append any new entries I don't already have.
6. fsync the log.
7. If `leader_commit > commit_index`: `commit_index = min(leader_commit, index of last new entry)`.
8. → `true`

### Why delete only on conflict, never truncate after `prev`
Messages can arrive late. If I already have entries 5–7 from the leader and a delayed message arrives with `prev_log_index = 4, entries = [5]`, truncating after `prev` would throw away 6 and 7, which the leader may already be counting in `match_index`. Deleting only from the first conflict (same index, different term) never removes an entry the leader has.

### Why `min` for `commit_index`
`leader_commit` can be ahead of what I hold. If `leader_commit = 10` but I only match the leader up to 7, I can't call 8–10 committed: I don't have them yet. `commit_index` = highest entry known committed (safe to apply); how far I've applied to `Storage` is `last_applied`, a separate counter.

### Why fsync before `true`
The leader counts my `true` as a stored copy when deciding what's committed. If I crash and lose the entry, that majority was never real, and a "committed" entry can disappear.

### What the leader does
**On becoming leader**, for each follower: `next_index = my last log index + 1`, `match_index = 0`.

**Sending to a follower** (every heartbeat, and straight away on a new entry):
- `prev_log_index = next_index - 1`
- `prev_log_term` = term of my entry at `prev_log_index` (0 if `prev_log_index` is 0)
- `entries` = my log from `next_index` onwards (empty if the follower is caught up)

**On the reply:**
- `success: true` → `match_index = prev_log_index + entries.len()`, `next_index = match_index + 1`
- `success: false`, reply term > mine → step down (universal rule)
- `success: false` otherwise → `next_index -= 1` (never below 1) and retry

**Commit rule:** entry N is committed when
1. a majority, counting me, have `match_index ≥ N` *(for 3 nodes: the middle of my last index and the two `match_index` values)*, **and**
2. `log[N].term == current_term`

then `commit_index = N`. Followers learn it from `leader_commit` in the next AppendEntries; there's no separate commit message.

**A client PUT/DELETE, in order:**
1. Append `{current_term, command}` to my log
2. fsync it
3. Send AppendEntries to followers, in parallel
4. Majority `success` → commit rule advances `commit_index`
5. Apply to `Storage`
6. Reply OK to the client

### Applying (every node)
While `last_applied < commit_index`: `last_applied += 1`, apply `log[last_applied]` to the Phase 1 `Storage`. Committing and applying are separate counters because they happen in different places: `commit_index` moves in RPC handling, applying runs after it.

### Why only current-term entries are committed by counting (§5.4.2)
An entry keeps the term it was created in. An old-term entry can be on a majority and still be overwritten. Example: S1 copies an old term-2 entry A to S3 while leading in term 4, so A is on 2 of 3. S2 holds a conflicting term-3 entry B in the same slot. If S1 crashes, S2's last term (3) beats S3's (2), so S3 votes for S2, which overwrites A. If S1 had already told the client OK, a committed write is lost.

Waiting until an entry from S1's own term 4 is on a majority blocks this: S3's last term is then 4, which beats S2's 3, so S2 can't win. Committing that entry also commits A, since everything before a committed entry is committed. A write that is lost before it commits is fine: the client was never told it succeeded.

## Replication decisions
- **No-op on becoming leader.** A new leader appends an empty entry with its `current_term` straight away. By §5.4.2 it can't commit older-term entries until an entry from its own term commits; the no-op makes that happen without waiting for a client write.
- **Raft log on disk.** Separate append-only file, same style as the Phase 1 WAL: `[term][op][key_len][key][val_len][val]`, fsync'd on append, replayed on startup. Truncating conflicting entries (receiver rule 4) rewrites the file from the conflict point.
- **Client write to a follower.** Rejected with an error naming the current leader; the client retries there. No forwarding.
- **Restarts apply entries twice — accepted for now.** `last_applied` is volatile and resets to 0, but `Storage` keeps its data through its own WAL, so replaying the log re-applies entries already in it. The final state is still correct (same entries, same order); while catching up, a key can briefly show an older value. A node only serves clients as leader, after applying up to `commit_index`. Proper fix: persist `last_applied` with `Storage`, or merge the Raft log and WAL (Phase 4, see DESIGN.md).

## Still unsure about
- Nothing blocking. Expect the details of rule 4 (finding the first conflict) and rewriting the log file to get clearer while implementing.
