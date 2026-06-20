# LP-0017 Whistleblower

Censorship-resistant document upload and indexing for the Logos stack.

Three components:

| Component | What it does |
|---|---|
| `cid-registry` | On-chain LEZ program (real RISC0 zkVM guest). Stores `(CID, metadata_hash, anchor_timestamp)`. Accepts batches up to 50. Idempotent. |
| `document-indexer` | Reusable Rust library. Wraps upload → Logos Storage (`storage_module`), broadcast → Logos Delivery (`delivery_module`), anchor → sequencer. |
| `batch-anchor` | Permissionless CLI. Subscribes to the Delivery topic, accumulates CIDs, submits batch anchor transactions. Resumes from SQLite state after interruption. |

## Deployed addresses (LEZ standalone, RISC0_DEV_MODE=0)

| Item | Value |
|---|---|
| Program id (hex) | `c4ea30cd,bf888e58,c019e2ce,3bdfb9eb,b98fc9e8,3c7c06ea,383b1175,913b425b` |
| ImageID | `cd30eac4588e88bfcee219c0ebb9df3be8c98fb9ea067c3c75113b385b423b91` |
| Registry state PDA | `6QzQcyJn7LoYiSZkNoLmv5SbYCC4ba4BEDGt7KTeXCEM` (seeds: `[program_id, "registry_state"]`) |

CU benchmark: single-CID anchor = 262,144 RISC0 cycles (1 segment); 50-CID batch = 3,145,728 cycles / 2,432,433 user cycles (12 segments). See [docs/CU_BENCHMARK.md](docs/CU_BENCHMARK.md).

## Build

```bash
cargo build --release --workspace                  # host crates + CLIs
cargo build --release -p cid-registry --target riscv32im-risc0-zkvm-elf  # zkVM guest (via risc0)
```

Requires: Rust stable, the RISC0 toolchain, a running LEZ sequencer.

## Deploy the on-chain registry (verified flow)

```bash
# 1. Start a standalone LEZ sequencer with real proofs
RISC0_DEV_MODE=0 sequencer_service sequencer_config.json   # :3040

# 2. Deploy the guest ELF (deterministic program_id = ImageID)
export NSSA_WALLET_HOME_DIR=<wallet-home>
RISC0_DEV_MODE=0 wallet deploy-program \
    target/riscv32im-risc0-zkvm-elf/docker/cid_registry_program.bin

# 3. Initialize the registry PDA + anchor a batch (real proofs) with spel
spel -- initialize  --authority <funded-pubkey>
spel -- anchor-batch \
    --entries-cids <46B-hex>[,<46B-hex>...] \
    --entries-meta-hashes <32B-hex>[,...] \
    --entries-timestamps <u64>[,...]
```

> Anchoring `Vec<Vec<u8>>` CIDs requires the spel-cli arg-parser patch in
> `spel/spel-cli/src/parse.rs` (+ a `Vec<u64>` CSV arm in `serialize.rs`).

## Upload and broadcast a document

The indexer drives the real Logos Core `storage_module` (Codex CID, via the
`agent_module` `storage.upload` skill) and `delivery_module` (`send`) through the
`logoscore` CLI. Set `LOGOSCORE_BIN` and `LOGOS_MODULES_DIR`.

```rust
use document_indexer::{Indexer, IndexerConfig, MetadataEnvelope};

let indexer = Indexer::new(IndexerConfig::default()); // backend = StorageBackend::from_env()
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

## Query the registry (query-by-CID)

`spel inspect` decodes the registry PDA account into `RegistryState` (the list of
anchored `DocumentRecord`s). Filter by CID to confirm a document is anchored:

```bash
spel inspect 6QzQcyJn7LoYiSZkNoLmv5SbYCC4ba4BEDGt7KTeXCEM \
    --idl cid-registry/cid-registry.idl.json --type RegistryState \
    | jq '.records[] | select(.cid == "<46B-hex-cid>")'
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
