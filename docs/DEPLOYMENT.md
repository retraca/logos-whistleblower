# Deployment guide: LP-0017 Whistleblower

## Prerequisites

- Rust stable
- Docker + docker compose
- `lez` CLI (from LP-0008 or built from `lez-build`)

## Build

```bash
cargo build --release --workspace
```

## Start the Logos stack

```bash
docker compose up -d
# Wait for sequencer health check (30s)
until curl -sf http://127.0.0.1:3040/ \
    -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","method":"hello","params":{},"id":1}'; do
  sleep 2
done
echo "sequencer ready"
```

## Deploy the cid-registry program

```bash
# Create an agent identity
lez ensure-account \
    --home ~/.whistleblower-deployer \
    --passphrase <your-passphrase>

# Deploy the program
PROGRAM_ID=$(lez program deploy \
    --home ~/.whistleblower-deployer \
    --passphrase <your-passphrase> \
    --binary ./target/release/libcid_registry.so)

echo "cid-registry program_id: $PROGRAM_ID"
# Save this for querying later
```

## Generate the IDL

```bash
spel generate --manifest-path cid-registry/Cargo.toml --output cid-registry/idl.json
```

## Run the Basecamp app

```bash
# Load modules into Logos Core (requires Logos app installed)
logoscore -D \
    -m ./target/release/libcid_registry_module.so \
    -m ./target/release/libdocument_indexer_module.so
# Open Logos app, navigate to Basecamp, load Whistleblower
```

## Run the batch anchor tool

```bash
./target/release/batch-anchor \
    --delivery-url http://127.0.0.1:9090 \
    --sequencer-url http://127.0.0.1:3040 \
    run --topic whistleblower/v1/documents --batch-size 50
```

## Query the registry

```bash
# By CID
curl -s http://127.0.0.1:3040/ \
    -H 'Content-Type: application/json' \
    -d "{
        \"jsonrpc\": \"2.0\",
        \"method\": \"queryProgram\",
        \"params\": {
            \"program\": \"$PROGRAM_ID\",
            \"query\": \"query_by_cid\",
            \"cid\": \"<base58-cid>\"
        },
        \"id\": 1
    }" | jq .
```

## End-to-end demo

```bash
RISC0_DEV_MODE=0 bash demo.sh
```
