# document-indexer

Reusable Rust library/SDK for censorship-resistant document publication on the
Logos stack. Any Logos app can depend on this crate to get **upload → broadcast →
anchor** without depending on the Whistleblower app.

```toml
[dependencies]
document-indexer = { git = "https://github.com/retraca/logos-whistleblower", package = "document-indexer" }
```

## What it does

1. **Upload** bytes to Logos Storage → content CID.
2. **Broadcast** a metadata envelope to a Logos Delivery topic (so indexers/guardians find it).
3. **Anchor** the `(CID, metadata_hash, timestamp)` on the on-chain `cid-registry` LEZ program.

## Backends

`StorageBackend` selects where bytes and envelopes go:

| Variant | Use |
|---|---|
| `LogosCore { logoscore_bin, modules_dir }` | **Production.** Drives the real Logos Core `storage_module` (Codex CID, via `agent_module`'s `storage.upload` skill) and `delivery_module` (`send`) through the `logoscore` CLI. |
| `Http` | Local mock-server tests only. The `lssa/logos_storage_service` HTTP images do not exist — do not use in production. |

`StorageBackend::from_env()` reads `LOGOSCORE_BIN` and `LOGOS_MODULES_DIR`.

## API

```rust
use document_indexer::{Indexer, IndexerConfig, MetadataEnvelope};

let indexer = Indexer::new(IndexerConfig::default());

// 1+2: upload to storage, broadcast envelope to the delivery topic.
let result = indexer.upload_and_broadcast(file_bytes, MetadataEnvelope {
    title: "My Document".into(),
    description: "Important material".into(),
    content_type: "application/pdf".into(),
    size_bytes: file_bytes.len() as u64,
    timestamp: unix_now(),
    tags: vec!["leak".into()],
}).await?;
println!("CID: {}  metadata_hash: {:x?}", result.cid, result.metadata_hash);

// 3: anchor a batch on-chain (≤ 50 entries; already-anchored CIDs are idempotent no-ops).
// Anchoring drives the real `spel anchor-batch` path: SPEL_BIN / SPEL_IDL / SPEL_WORKDIR
// (and REGISTRY_PDA for `is_anchored`). For high volume, prefer the `batch-anchor` tool.
indexer.anchor_batch(vec![(result.cid.clone(), result.metadata_hash)], unix_now()).await?;

// query: is this CID already anchored?
let anchored = indexer.is_anchored(&result.cid).await?;
```

### Types

- `IndexerConfig { storage_url, delivery_url, sequencer_url, delivery_topic, backend }`
- `MetadataEnvelope { title, description, content_type, size_bytes, timestamp, tags }`
- `IndexResult { cid: String, metadata_hash: [u8; 32] }`
- `decode_cid(&str) -> [u8; 46]` — base58 CID → the 46-byte array the on-chain registry expects.

## Reliability

- **Upload retry** (`upload_with_retry`): 3 attempts, exponential back-off (200ms → 400ms → 800ms), surfaces the final error.
- **Broadcast dedup** (R2): re-broadcasting a CID already sent this session is a no-op; a *failed* send does not poison the dedup set, so it can be retried.

## FFI

`src/ffi.rs` exports `document_indexer_upload_and_anchor(...)` and
`document_indexer_is_anchored(...)` (C ABI) for the Qt Basecamp app
(`whistleblower_impl.cpp`).

## Tests

```bash
cargo test -p document-indexer    # dedup, retry-set hygiene, CID decode
```
