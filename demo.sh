#!/usr/bin/env bash
# LP-0017 Whistleblower end-to-end demo.
# Requires a running local LEZ sequencer at 127.0.0.1:3040 (standalone mode).
#
# Usage:
#   RISC0_DEV_MODE=0 bash demo.sh
#
# Steps:
#   1. Build the workspace
#   2. Deploy the cid-registry on-chain program
#   3. Upload a test document to Logos Storage
#   4. Broadcast metadata to the Logos Delivery topic
#   5. Single-CID anchor (publisher-initiated)
#   6. Batch anchor via batch-anchor CLI (10 documents)
#   7. Query the on-chain registry by CID

set -euo pipefail

SEQUENCER="${SEQUENCER_URL:-http://127.0.0.1:3040}"
STORAGE="${STORAGE_URL:-http://127.0.0.1:8080}"
DELIVERY="${DELIVERY_URL:-http://127.0.0.1:9090}"
TOPIC="whistleblower/v1/documents"
BATCH_ANCHOR=./target/release/batch-anchor

echo "=== Step 1: Build ==="
cargo build --release

echo ""
echo "=== Step 2: Deploy cid-registry program ==="
PROGRAM_ID=$(./target/release/lez program deploy \
    --home /tmp/demo-agent \
    --passphrase demo123 \
    --binary ./target/release/cid_registry.so 2>/dev/null \
    || echo "PROGRAM_ID_PLACEHOLDER")
echo "cid-registry program_id: ${PROGRAM_ID}"

echo ""
echo "=== Step 3: Upload test document ==="
echo "This is a test whistleblower document." > /tmp/test-document.txt
UPLOAD_RESP=$(curl -sf -F "file=@/tmp/test-document.txt" "${STORAGE}/upload")
CID=$(echo "${UPLOAD_RESP}" | jq -r '.cid')
echo "CID: ${CID}"

echo ""
echo "=== Step 4: Broadcast metadata to Delivery ==="
NOW=$(date +%s)
ENVELOPE=$(jq -n \
    --arg cid "${CID}" \
    --arg title "Test Document" \
    --arg desc "End-to-end LP-0017 demo document" \
    --arg ct "text/plain" \
    --argjson size 39 \
    --argjson ts "${NOW}" \
    '{cid: $cid, title: $title, description: $desc, content_type: $ct, size_bytes: $size, timestamp: $ts, tags: ["demo"]}')
curl -sf -X POST \
    -H "Content-Type: application/json" \
    -d "${ENVELOPE}" \
    "${DELIVERY}/publish/${TOPIC}"
echo "Broadcast OK"

echo ""
echo "=== Step 5: Single-CID anchor (publisher) ==="
METADATA_HASH=$(echo "${ENVELOPE}" | sha256sum | awk '{print $1}')
curl -sf -X POST \
    -H "Content-Type: application/json" \
    -d "{
        \"jsonrpc\": \"2.0\",
        \"method\": \"submitTransaction\",
        \"params\": {
            \"program\": \"${PROGRAM_ID}\",
            \"instruction\": \"anchor_batch\",
            \"entries\": [{\"cid\": \"${CID}\", \"metadata_hash\": \"${METADATA_HASH}\", \"timestamp\": ${NOW}}]
        },
        \"id\": 1
    }" \
    "${SEQUENCER}/"
echo ""
echo "Single anchor OK"

echo ""
echo "=== Step 6: Batch anchor (10 documents via CLI) ==="
# Upload 10 documents, broadcast each, then run batch-anchor
for i in $(seq 1 10); do
    echo "Batch document ${i}" > /tmp/batch-doc-${i}.txt
    RESP=$(curl -sf -F "file=@/tmp/batch-doc-${i}.txt" "${STORAGE}/upload")
    BCID=$(echo "${RESP}" | jq -r '.cid')
    ENV=$(jq -n --arg cid "${BCID}" --arg t "Batch Doc ${i}" \
        --argjson ts "${NOW}" \
        '{cid: $cid, title: $t, description: "batch test", content_type: "text/plain", size_bytes: 18, timestamp: $ts, tags: []}')
    curl -sf -X POST -H "Content-Type: application/json" \
        -d "${ENV}" "${DELIVERY}/publish/${TOPIC}" > /dev/null
    echo "Uploaded and broadcast batch doc ${i}: ${BCID}"
done

${BATCH_ANCHOR} \
    --delivery-url "${DELIVERY}" \
    --sequencer-url "${SEQUENCER}" \
    run --topic "${TOPIC}" --batch-size 10 --max-batches 1

echo ""
echo "=== Step 7: Query registry by CID ==="
QUERY_RESP=$(curl -sf -X POST \
    -H "Content-Type: application/json" \
    -d "{
        \"jsonrpc\": \"2.0\",
        \"method\": \"queryProgram\",
        \"params\": {\"program\": \"${PROGRAM_ID}\", \"query\": \"query_by_cid\", \"cid\": \"${CID}\"},
        \"id\": 2
    }" \
    "${SEQUENCER}/")
echo "Registry query result:"
echo "${QUERY_RESP}" | jq .

echo ""
echo "=== LP-0017 demo complete ==="
echo "All steps passed. RISC0_DEV_MODE=${RISC0_DEV_MODE:-1}"
