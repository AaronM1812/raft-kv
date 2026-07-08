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