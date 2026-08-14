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

## 13/08/2026 — TCP server: async Tokio server with concurrent clients

**What I did:**
- Created src/server.rs and added `pub mod server;` to lib.rs — server is library code, not a binary, so tests can call it directly rather than launching a process
- main.rs now creates the Storage (which replays the WAL), wraps it in `Arc<RwLock<>>`, and passes it to `server::run()`. main does wiring and config; server does logic
- `run()` binds a TcpListener to 127.0.0.1:7878 and loops on `accept().await` — parks until a client connects instead of busy-waiting
- Each accepted connection clones the Arc and spawns a task via `tokio::spawn`, passing the socket and storage handle into `process()`
- `process()` wraps the socket in a BufReader, loops reading lines, splits the first word as the command and the rest as the key, and pattern-matches to dispatch
- Implemented all five protocol responses: GET hit (`OK <len>\n<bytes>`), GET miss (`NOT_FOUND`), PUT/DELETE success (`OK`), missing key (`ERR missing key`), unknown command (`ERR unknown command`)
- Full round trip working end to end: PUT a key, GET it back, DELETE it, GET returns NOT_FOUND. Data persists to the WAL and is replayed on restart

**What I learned:**
- `tokio::spawn` returns immediately; it hands the task to the runtime rather than waiting for it. Calling `process(socket).await` directly instead would serialise clients — one connection served to completion before the next is accepted. The spawn is what makes the server concurrent
- Cloning an `Arc` doesn't copy the Storage. It increments a reference count, so every task holds a pointer to the same underlying data. The clone has to be inside the accept loop — one handle per connection, moved into that task
- `RwLock` over `Mutex`: many readers concurrently, or one writer alone. GETs don't block each other. A Mutex would serialise reads for no reason. Using tokio's RwLock rather than std's, because std's blocks the OS thread instead of yielding to the runtime
- PUT needs two reads. The protocol is `PUT key\nvalue\n`, so the command line only gives the key — the value is a separate `read_line` inside the arm. Read into a fresh String rather than reusing `line`, because the key borrows from `line` and reusing it would upset the borrow checker
- `read_line` returns the byte count; 0 means the client disconnected. Checked in two places — the main loop, and inside the PUT arm for a client that sends a key then hangs up
- A task parked on `.await` waiting for client input costs almost nothing. Thread-per-connection would tie up an OS thread per idle client; async tasks are cheap enough that idle connections are effectively free

**What broke:**
- Saw a phantom `ERR unknown command` in the middle of a working session. Turned out not to be a bug: the GET response is `OK <len>\n` followed by raw bytes with no trailing newline, by design — the length already tells the client how many bytes to read. That leaves the terminal cursor mid-line, so pressing Enter to tidy up sent an empty line, which parsed as an empty command and correctly fell through to the unknown-command arm. Worth knowing the length-prefixed format makes the server awkward to drive by hand with `nc`, even though it's the right format for a real client
- Nearly hit a lock-scope problem in the GET arm. `storage.read().await` returns a guard, and the read lock is held for as long as that guard lives. Matching directly on `storage.read().await.get(k)` keeps the guard alive for the whole match block — meaning the read lock is held while writing to the client's socket. Reads don't block each other, but they do block writers, so a single slow client would stall every PUT and DELETE on a network operation unrelated to storage. Binding the result to a variable first drops the guard at the end of that statement, before any socket writes. General rule: never hold a lock across an `.await` that might block on I/O. `put` already breaks this by holding the write lock across an fsync — deliberate, documented, and the thing group commit would fix
- Lost time to brace nesting — three levels of match inside a loop inside a function. The compiler catches it instantly; eyeballing it does not

**Known costs (in DESIGN.md):**
- `put` and `delete` hold the write lock across an fsync, so every reader and writer is blocked for the duration of a physical disk write (~2ms measured). Concurrent write throughput will be poor and the Phase 4 benchmark will show it. Fixes are group commit, or moving the disk write outside the lock — neither implemented
- Request framing is still newline-delimited, so a value containing a newline can't be transmitted. Responses are length-prefixed and don't have this problem. Accepted for Phase 1; gRPC replaces the wire protocol in Phase 2

**Next:**
- Integration test — real TCP client connects, PUTs 10 keys, GETs them back, asserts values match. Plus concurrent clients

## 13/08/2026 — Integration tests over real TCP

**What I did:**
- Created tests/server_test.rs — integration tests, so they live outside src/ and can only touch the public API, exactly as a real consumer would
- Changed `server::run()` to take an address parameter instead of hardcoding 127.0.0.1:7878, so each test can bind its own port. Same reasoning as giving `Storage::new()` a path parameter earlier
- `test_put_and_get_over_tcp`: spawns a server in-process, connects a real TcpStream client, then loops 100 times — PUT key:i/value:i, assert `OK`, GET key:i, parse the length from the header, read exactly that many bytes, assert the value matches
- `test_concurrent_clients`: spawns 5 client tasks against one server, each doing 10 PUT/GET pairs on its own namespaced key range (`c{client_id}:key:{i}`), collects the JoinHandles and awaits all of them
- Both passing. Five test targets now run under `cargo test`: 4 unit tests in the lib, 2 integration tests, and three targets with none

**What I learned:**
- The integration test can spawn a server *inside the test process* only because `server::run` is library code. If the server logic lived in main.rs there'd be no way to call it — the test would have to launch a subprocess and coordinate with it. This pays off properly in Phase 2, where three Raft nodes need to run inside one test to watch an election happen
- `tokio::spawn` returns a JoinHandle. Without collecting them into a Vec and awaiting each one, the test function returns while the clients are still mid-flight and nothing gets asserted — the test would pass vacuously
- `read_exact`, not `read_line`, for the GET value. The response is `OK <len>\n` followed by raw bytes with no terminator, so the only way to know where the value ends is the length in the header. This is the first time the client half of the protocol design has actually been exercised — and it's the same length-prefix-over-delimiter reasoning as the WAL entry format
- What the concurrent test actually proves: 5 simultaneous connections don't corrupt each other and no client's data leaks into another's. Namespacing keys by client_id is what makes that meaningful — if the locking were broken you'd see wrong values, not just slow ones
- What it doesn't prove: any throughput characteristic. Each client is still sequential with itself — PUT, wait for OK, GET, wait for response. No client ever has two requests in flight. It's five sequential streams running alongside each other, not a flood. Real concurrency numbers are the benchmark's job

**What broke:**
- `response.clear()` was only being called once per loop iteration, before the GET read. `read_line` appends rather than replaces, so iteration two's PUT check saw `"OK 7\nOK\n"` and the assert failed. Needed clearing before *both* reads. Third time this exact trap has bitten — the server's process loop, the unit tests, and now here. `read_line` appending is the single most reliable source of bugs in this codebase so far
- First version of the loop used `b"PUT key:i\nvalue:i\n"` — byte-string literals don't interpolate, so all 100 iterations wrote the literal key `key:i`. The test would have passed while proving essentially nothing. Fixed with `format!` and `.as_bytes()`. A test that passes for the wrong reason is worse than one that fails
- Same mistake in the first concurrent version: all 5 clients wrote `key:0` through `key:9`, so they were overwriting each other's keys with identical values. Passing, but testing nothing about isolation

**Open question:**
- Timing doesn't add up. Both integration tests together did ~150 fsync'd writes in 0.36s — roughly 400/sec, which is in the right range. But the two tests run in parallel and use separate log files, so the disks writes are interleaved across two files. The crash test measured ~510 writes/sec to a single file. Need the baseline benchmark to get a clean number rather than inferring one from test timings

**Next:**
- Baseline benchmark — 10,000 sequential PUTs, measure ops/sec, then 10,000 concurrent. Record with date and machine spec