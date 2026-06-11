# LP-0017 Whistleblower

Censorship-resistant document upload and indexing for the Logos stack.

Three components:

| Component | What it does |
|---|---|
| `cid-registry` | On-chain LEZ program. Stores `(CID, metadata_hash, anchor_timestamp)`. Accepts batches up to 50. Idempotent. |
| `document-indexer` | Reusable Rust library. Wraps upload → Logos Storage, broadcast → Logos Delivery, anchor → sequencer RPC. |
| `batch-anchor` | Permissionless CLI. Subscribes to the Delivery topic, accumulates CIDs, submits batch anchor transactions. Resumes from SQLite state after interruption. |

## Build

```bash
cargo build --release --workspace
```

Requires: Rust stable, Cargo.

## Deploy the on-chain registry

```bash
# Start local sequencer in standalone mode
cd lez-build && docker compose up -d

# Deploy cid-registry
./target/release/lez program deploy \
    --home ~/.whistleblower-agent \
    --passphrase <your-passphrase> \
    --binary ./target/release/cid_registry.so
# prints: program_id = <hex>
```

## Upload and broadcast a document

```rust
use document_indexer::{Indexer, IndexerConfig, MetadataEnvelope};

let indexer = Indexer::new(IndexerConfig::default());
let result = indexer.upload_and_broadcast(
    file_bytes,
    MetadataEnvelope {
        title: "My Document".into(),
        description: "Important leaked material".into(),
        content_type: "application/pdf".into(),
        size_bytes: file_bytes.len() as u64,
        timestamp: unix_now(),
        tags: vec!["leak".into()],
    },
).await?;
println!("CID: {}", result.cid);
```

## Run the batch anchor CLI

Any party can run this -- the original uploader, an NGO, a guardian service:

```bash
./target/release/batch-anchor \
    --delivery-url http://127.0.0.1:9090 \
    --sequencer-url http://127.0.0.1:3040 \
    run --topic whistleblower/v1/documents --batch-size 50

# Check status
./target/release/batch-anchor status
```

## Anchor a single document yourself

```bash
./target/release/batch-anchor run --max-batches 1
```

## Query the registry

```bash
# Via sequencer RPC
curl -s http://127.0.0.1:3040/ \
    -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","method":"queryProgram","params":{"program":"<program_id>","query":"query_by_cid","cid":"<cid>"},"id":1}'
```

## End-to-end demo

```bash
RISC0_DEV_MODE=0 bash demo.sh
```

## Tests

```bash
# Unit tests (no chain)
cargo test --workspace

# Integration tests (requires local Logos stack)
docker compose up -d
cargo test --workspace --test integration -- --include-ignored
```

## Delivery envelope schema

Every broadcast uses this JSON structure:

```json
{
  "cid": "Qm...",
  "title": "string",
  "description": "string",
  "content_type": "string",
  "size_bytes": 12345,
  "timestamp": 1700000000,
  "tags": ["optional", "list"]
}
```

Any app subscribing to the topic can parse this envelope without depending on Whistleblower.

## On-chain registry choice

LEZ program (not zone SDK), because:

- The zone SDK requires a single designated sequencer actor for consensus inscription, which adds a centralisation assumption.
- A LEZ program runs on the existing LEZ sequencer network, is callable by any transaction sender, and requires no trusted relay.
- Tradeoff: requires a running LEZ sequencer; the zone SDK approach could work without one. LP-0017's scope assumes a LEZ devnet is available.

## License

MIT
