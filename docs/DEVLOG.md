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