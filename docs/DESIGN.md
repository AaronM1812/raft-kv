# DESIGN.md — Architectural decisions

## Phase 1 — Single-node KV store

### WAL entry format: length-prefixed, not delimited

**Chose:** `[op:1][key_len:4][key][val_len:4][val]` — every variable-length field preceded by its size in bytes.

**Considered:** delimiter-based framing (write a marker byte after each field, read until the marker).

**Why:** values are arbitrary `Vec<u8>`. Any byte chosen as a delimiter can legitimately appear inside a value, so reading-until-marker would truncate mid-value. Avoiding that needs an escaping scheme, which adds complexity and makes entry size unpredictable. A length prefix says exactly how many bytes to consume and is indifferent to their content.

Note the wire protocol (`PUT key\nvalue\n`) uses the opposite approach. That's acceptable there because the protocol is text-oriented and newlines aren't expected in transmitted keys — but it's a weaker guarantee, and if the protocol ever carries binary values it will need length prefixes too.

### Length fields: big-endian u32

**Chose:** 4-byte lengths, big-endian.

**Considered:** u16 (2 bytes, 64 KB ceiling), u64 (8 bytes, effectively unlimited); little-endian.

**Why the width:** the prefix width trades per-entry overhead against maximum key/value size. u32 caps a key or value at ~4 GB and costs 9 bytes of overhead per entry (1 opcode + 4 + 4). u64 would double the length overhead to 17 bytes per entry for a ceiling no KV store of this kind needs. u16's 64 KB limit is too low to be comfortable. u32 is the right point on that curve.

**Why big-endian:** network byte order, so it's conventional for wire and file formats and consistent with the gRPC layer arriving in Phase 2. Also readable in a hex dump, which matters when debugging a corrupt log by eye. Endianness has no correctness implications as long as reads and writes agree — this is a convention and debuggability choice.

### Operation code: single byte

**Chose:** one byte, `0` = put, `1` = delete.

**Why:** 256 possible operations for one byte of overhead. Two are in use; the rest are free for future needs (compaction markers, transaction boundaries, Raft term metadata). Smaller isn't practical — bit-packing the opcode into a length field would save 1 byte per entry at the cost of making the format unreadable and unextendable.

### Durability: sync_all on every write

**Chose:** call `sync_all()` at the end of `put` and `delete`, after the appends and before returning.

**Considered:** `sync_data()`; no explicit sync at all.

**Why sync at all:** `write_all` only transfers bytes to the operating system's page cache. The OS flushes to physical disk on its own schedule, potentially seconds later. A crash in that window loses a write the client was already told had succeeded — which is precisely the failure the WAL exists to prevent.

**Why sync_all over sync_data:** `sync_data` flushes file contents only; `sync_all` also flushes metadata, including file length. Replay finds the end of the log by reading until a read fails, so the recorded file size defines what is readable. If appended data reached the disk but the updated size did not, those bytes sit past EOF and are unreachable — a durable write that recovery cannot see, which is indistinguishable from data loss. On most modern filesystems an append updates size in the same journal transaction, so `sync_data` would likely be sufficient in practice, but "likely, on most filesystems" is not a durability guarantee.

**Cost accepted:** one fsync per write. This is the dominant term in single-node write latency and will define the Phase 4 benchmark numbers. It is the standard tradeoff — Postgres, MySQL, SQLite and RocksDB all pay it. If throughput becomes the constraint, the recognised fix is group commit (batching multiple writes behind one fsync), which trades a small latency increase for a large throughput gain. Not implemented; noted as the known next step.

### Truncated tail: discard and continue

**Chose:** on a short read during replay, stop replaying and return the store rebuilt from every complete entry read so far.

**Considered:** refusing to start on a malformed log; attempting to repair or salvage the partial entry.

**Why:** a partial entry at the end of the log is the normal signature of a crash mid-write, not corruption. That write never completed, so it was never acknowledged to a client, so by the client's view it never happened — discarding it is correct, and the client will retry. Refusing to start would mean a single badly-timed crash permanently bricks the store, since the bad bytes remain in the file forever.

The earlier implementation did exactly that: `.unwrap()` on the short read panicked, and the store could never be opened again.

**Implementation:** parsing moved out of `Storage::new` into a standalone `read_entry` returning `io::Result<Entry>`, which allows `?` instead of `.unwrap()`. `new` calls it in a loop and breaks on any `Err`. This also separates concerns cleanly — `read_entry` knows the byte format and nothing about the map; `new` knows the map and nothing about bytes.

Three conditions produce an `Err` and are handled identically: clean EOF, truncated entry, and unrecognised opcode. All mean "everything before this point is valid, everything after is unusable." Open question: an unrecognised opcode indicates real corruption rather than a normal crash, and arguably deserves louder handling than a silent stop.

**Not implemented:** per-entry checksums. Without them, a partial entry is detected only if it is short. A torn write that produces a complete-looking but corrupt entry would be replayed as valid. A CRC per entry is the standard defence and is the natural extension of this format.

### Divergence from Bitcask

**Chose:** full values held in the in-memory `HashMap`; the log is used purely for recovery.

**Bitcask's design:** the log *is* the primary data structure. Memory holds only a "keydir" mapping each key to a file offset; reads seek into the log for the value.

**Why the divergence:** holding values in memory makes reads a single hash lookup with no disk I/O, and keeps the storage layer simple enough to stay out of the way of the distributed-systems work that is the actual point of this project.

**Cost:** memory scales with total data volume rather than key count, so the store cannot hold a dataset larger than RAM. Bitcask's approach scales to datasets far exceeding memory. For a project whose working set is a benchmark of ~10,000 keys, this is not a binding constraint — but the roadmap describes this as "Bitcask-style" and the difference is worth stating plainly rather than being caught by it.

**Also not implemented:** log compaction. The log grows without bound; repeated puts to the same key accumulate dead entries, and deletes never reclaim space. Bitcask merges old log files into compacted ones in the background. Startup time grows linearly with total writes ever made, not with live key count.
