## 07/07/2026 — Tokio tutorial + project setup

**What I did:**
- Completed the relevant Tokio tutorial sections (Hello Tokio, Spawning, Shared state, I/O, Framing)
- Set up raft-kv folder structure with docs, src, tests, benches, proto folders
- Ran cargo init and cargo check — project compiles cleanly

**What I learned:**
- Tokio is an async runtime — allows the program to keep running when it hits slow disk or network operations instead of freezing
- async fn and .await — async marks a function as having pause points, .await is the actual pause
- Arc = shared pointer so multiple tasks can point to the same data, RwLock = multiple tasks can read simultaneously but only one can write
- Sockets are two-way connections between client and server — client writes, server reads, server writes, client reads
- Cargo manages the project — cargo check, cargo run, cargo test, cargo build. Cargo.toml is the dependency file
- Tasks must be 'static and Send — means the task owns all its data and that data is safe to move between threads
- Framing — how you identify where one command ends and the next begins. Using newlines as boundaries in our project

**Still fuzzy on:**
- Manual copying and why it's relevant to our project
- The bytes crate
- Splitting sockets into read/write halves

**Next:**
- Write PHASE1.md
- Start storage.rs — in-memory HashMap with get, put, delete

## 08/07/2026 — In-memory HashMap complete

**What I did:**
- Built the Storage struct in src/storage.rs wrapping a HashMap<String, Vec<u8>>
- Implemented three methods: get, put, delete in an impl block
- Wrote unit tests directly in storage.rs using #[cfg(test)] block
- Tests passing: put and get, delete, get missing key

**What I learned:**
- Structs don't have semicolons after the closing brace
- Methods go in a separate impl block, not inside the struct
- &str vs String — take &str as parameter, call .to_string() to store it in the HashMap (ownership)
- .cloned() on get — map.get() returns a reference, .cloned() gives you an owned copy
- &mut self needed on put and delete because they modify the map, &self on get because it only reads
- Unit tests live in the same file as the code in a #[cfg(test)] block — tests/ folder is for integration tests that test multiple modules together

**Still fuzzy on:**
- Rust ownership in general, getting more comfortable but still catching me out

**Next:**
- Write-Ahead Log — append every write to disk

## 09/08/2026 — WAL complete: durability, replay hardening, test isolation

**What I did:**
- Added src/lib.rs with `pub mod storage;` — storage is now a library both binaries can import from, instead of being locked inside main.rs
- Made Storage and its four methods `pub` so they're visible outside the module
- Gave `Storage::new()` a path parameter instead of hardcoding "wal.log" — every test now uses its own log file
- Added `sync_all()` to put and delete after the appends
- Extracted the byte-parsing out of `new()` into a standalone `read_entry()` function returning `io::Result<Entry>`
- Added an `Entry` enum with Put and Delete variants
- Added `test_replay` — writes keys, drops the Storage, rebuilds from the same log, asserts the data came back
- All 4 unit tests passing

**What I learned:**
- Every file in src/bin/ is its own separate crate — it can't see modules declared in main.rs. Shared code has to live in a library (lib.rs)
- Everything in Rust is private by default; the module boundary needs `pub` even for code in the same package
- Cargo runs tests concurrently, so three tests sharing one wal.log were deleting and writing over each other. Failures were timing-dependent — a test that never writes anything crashed reading another test's delete entry
- `write_all` only hands bytes to the OS; the OS flushes to disk whenever it likes. A crash in that window loses a write the client was already told succeeded — which defeats the entire point of a WAL
- `sync_all` vs `sync_data`: sync_data flushes file contents, sync_all also flushes metadata including file size. Size matters here because replay finds the end of the log by reading until the read fails — if the OS has the data but not the updated size, those bytes are past EOF and unreachable
- `?` returns the error out of the function instead of panicking like `.unwrap()`. It only works in a function returning Result, which is why the parsing had to move out of `new()` (which returns Storage) before it could be fixed
- An enum expresses "one of several shapes" — read_entry returns either a Put (key + value) or a Delete (key only), one function, one return type

**What broke:**
- Replay panicked on a truncated entry. A crash mid-write leaves a partial entry at the end of the log; `.unwrap()` on the short read killed the process, and since the bad bytes stay in the file, the store could never start again — one badly-timed crash bricks the database permanently
- Fix: read_entry returns an Err on a short read, the loop in `new()` breaks, and the map returns with every complete entry replayed. The incomplete write is discarded, which is correct — it was never acknowledged to a client, so it never happened
- Same handling covers three cases: clean EOF, truncated entry, and unknown opcode. All mean "stop replaying, keep what's valid"

**Still fuzzy on:**
- Whether discarding a corrupt-opcode entry silently is right, or whether it should be louder than a truncated tail

**Next:**
- Crash recovery test — write 100 keys, kill -9 the process, restart, verify all 100 present

## 09/08/2026 — Crash recovery test with kill -9

**What I did:**
- Built src/bin/crash_test.rs with two modes selected by a command-line argument: `write` and `verify`
- `write` puts 10,000 deterministic keys (key:i → value:i), then sits in an infinite loop so the process stays alive to be killed
- `verify` opens the same log in a fresh process, loops 0..10000, and for each key checks whether it's present and whether its value matches — counting hits, tracking whether a key ever appears after a missing one (gap detection), and printing the result
- Ran the roadmap version (kill after writing completes): 100 of 100 present, gap false
- Ran the harder version (kill mid-write) five times: 252, 254, 256, 258, 258 of 10,000 — all values correct, gap false every run

**What I learned:**
- No comparison file needed. My first instinct was to have `write` record what it did to a second file so `verify` could diff against it — but that file has exactly the same durability problem being tested. Two WALs, no way to tell which one lost data. Deterministic keys remove the need entirely: verify recomputes what the value should be
- The property being tested isn't a count, it's a shape. There's no target number because you don't control where the kill lands. What must hold is that survivors form a contiguous prefix from 0 — a hole would mean something is seriously wrong with append ordering or replay
- ~510 writes/sec on an M-series MacBook Air, and the variance across five runs was tiny (252–258). That tightness is the signature of a workload bound by a fixed-cost operation — every write waits on one fsync. A memory- or CPU-bound workload would be far noisier. This is the sync_all cost from DESIGN.md showing up in practice, and it's the number group commit would attack
- `cargo run --bin crash_test -- write` builds and runs in one step; the `--` separates cargo's arguments from the program's

**What broke:**
- First attempt at the mid-write kill returned 0 every single run. The kill was landing before the process had even started its loop — shell fork, exec and startup lost the race to `kill -9`. Fixed by raising the loop to 10,000 writes and adding `sleep 0.5` before the kill, which lands the kill reliably in the middle
- Not really a bug in the code, but a good reminder that a test can pass or fail for reasons that have nothing to do with the thing under test

**What this proves:**
- `verify` starts cleanly on a log whose final entry is very likely torn. Before the read_entry fix this would have panicked and the store would have been permanently unopenable. The Err path returning the partially-rebuilt map is doing exactly what it was written to do, under a real SIGKILL rather than a synthetic test
- Data that reached the log survives a process death that runs no cleanup code at all — no destructors, no flush, no graceful shutdown

**Next:**
- TCP server — async Tokio server in server.rs, wire protocol PUT key\nvalue\n, storage wrapped in Arc<RwLock<>>, one task per client