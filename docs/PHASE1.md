# Phase 1 — Single Node KV Store

## Struct
- Name: `Storage`
- Lives in: `src/storage.rs`
- Contains: `HashMap<String, Vec<u8>>` — key is a String, value is a byte array
- Wrapper in networking layer: `Arc<RwLock<Storage>>` — Arc for multiple pointers, RwLock for multiple reads but only one write at a time

## Methods
- `get(key: String)` — returns the value if it exists, error if it doesn't
- `put(key: String, value: Vec<u8>)` — inserts or updates the key, returns confirmation
- `delete(key: String)` — deletes the key if it exists, returns confirmation

## WAL
- A log of commands, replayed on startup to rebuild the HashMap
- Each entry contains: `[op][key_len][key][val_len][val]`
- Still not 100% sure on the exact byte format — will figure out when implementing

## Still unsure about
- Exact WAL byte format on disk
- How startup replay logic works in code