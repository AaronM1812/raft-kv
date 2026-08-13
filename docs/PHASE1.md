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

## Wire protocol

### Requests

PUT key\nvalue\n
GET key\n
DELETE key\n


Newline-delimited, text-oriented. PUT is two lines; GET and DELETE are one.

### Responses

GET hit → OK <len>\n<value bytes>
GET miss → NOT_FOUND\n
PUT ok → OK\n
DELETE ok → OK\n
malformed → ERR <reason>\n


### Why a status prefix

Every response leads with a status token — `OK`, `NOT_FOUND`, or `ERR`. The client branches on that token before looking at anything else, and each case is unambiguous: a missing key is distinguishable from a stored empty value, and both are distinguishable from a malformed request.

The alternative — a distinct format per case with no shared prefix — is harder to parse and harder to extend. The prefix leaves room for statuses Phase 2 will need, such as redirecting a client that contacted a follower instead of the leader.

### Why the GET response is length-prefixed

Values are `Vec<u8>` — arbitrary bytes, which may include newlines. If the response were terminated by a newline, a value containing one would make the client stop reading mid-value and treat the remainder as the next response, desynchronising the connection permanently.

Length-prefixing removes the problem: the client reads the status line, parses the byte count, then reads exactly that many bytes. The content of those bytes is irrelevant. This is the same reasoning as the WAL entry format — length prefixes over delimiters wherever the payload is arbitrary bytes.

### Known limitation: requests are still delimiter-framed

The response side is length-prefixed, but the request side is not. `PUT key\nvalue\n` reads the value as a line, so a value containing a newline cannot be transmitted in the first place.

Accepted for Phase 1 rather than fixed. The wire protocol is replaced by gRPC in Phase 2, where protobuf handles framing with length-delimited fields, so investment in hardening this format would be thrown away. Documented here so the limitation is a known choice rather than an oversight.

### DELETE on a missing key returns OK

Delete is idempotent. The post-condition is "this key is absent," which holds whether or not the key existed beforehand, so both cases return `OK`.

Considered: returning `NOT_FOUND` when the key was absent, as Redis does with a count. Rejected because it leaks internal state without giving the client anything actionable — there is no different action to take on either outcome. It also matters for Phase 2: after a leader crash, a client retries a delete it may have already committed, and that retry must not look like a failure.

## Concurrency model

`Arc<RwLock<Storage>>`, one Tokio task per connected client.

- **Arc** — multiple tasks need to point at the same Storage; Arc is the shared-ownership pointer that allows it.
- **RwLock, not Mutex** — many readers concurrently, or one writer alone. GETs don't block each other. A Mutex would serialise reads unnecessarily.
- **One task per client** — a client that connects and sends nothing parks on an `.await` and costs almost nothing. Thread-per-client would tie up an OS thread per idle connection; async tasks are cheap enough that idle clients are effectively free.

**Known cost:** `put` and `delete` hold the write lock across an fsync. Every reader and writer is blocked for the duration of a physical disk write, which the crash test measured at roughly 2ms. Throughput under concurrent writes will be poor and this will show in the Phase 4 benchmark. The standard fixes are group commit, or restructuring so the disk write happens outside the lock — neither is implemented in Phase 1.