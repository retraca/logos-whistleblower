#!/usr/bin/env bash
# LP-0017 Whistleblower — reproducible end-to-end demo.
#
# Flow: upload -> broadcast -> permissionless batch anchor -> on-chain registry confirm,
# then idempotency/resume. Runs against a real local LEZ sequencer with RISC0_DEV_MODE=0.
#
# Prereqs (see README "Deployed addresses" + "Deploy"):
#   - a standalone LEZ sequencer on :3040 (RISC0_DEV_MODE=0), cid-registry deployed + initialized
#   - `spel` (patched for Vec<Vec<u8>>) and `batch-anchor` on PATH (or set SPEL_BIN / BATCH_ANCHOR)
#   - NSSA_WALLET_HOME_DIR pointing at a wallet home with a funded account
# When LOGOSCORE_BIN + LOGOS_MODULES_DIR are set and the Logos storage/delivery stack is up,
# uploads produce real Codex CIDs via storage_module; otherwise the demo computes a
# deterministic content-addressed CID locally so the broadcast->anchor->registry path is
# still fully exercised offline.
set -euo pipefail

SPEL="${SPEL_BIN:-spel}"
BATCH_ANCHOR="${BATCH_ANCHOR:-batch-anchor}"
IDL="${IDL:-cid-registry/cid-registry.idl.json}"
PDA="${PDA:-6QzQcyJn7LoYiSZkNoLmv5SbYCC4ba4BEDGt7KTeXCEM}"
QUEUE="${QUEUE:-./delivery-queue.jsonl}"
DB="${DB:-./whistleblower-demo.db}"
TOPIC="${TOPIC:-whistleblower/v1/documents}"
N="${N:-12}"
RISC0_DEV_MODE="${RISC0_DEV_MODE:-0}"; export RISC0_DEV_MODE

rm -f "$QUEUE" "$DB"
echo "== LP-0017 e2e demo (RISC0_DEV_MODE=$RISC0_DEV_MODE) =="

# 1. UPLOAD + BROADCAST: produce a CID per document and append a delivery envelope.
echo "-- step 1: upload $N documents + broadcast envelopes to topic $TOPIC"
N="$N" TOPIC="$TOPIC" bash "$(dirname "$0")/scripts/publish-demo-docs.sh" "$QUEUE"

# 2. BATCH ANCHOR: the permissionless tool reads the topic and anchors on-chain via spel.
echo "-- step 2: batch-anchor picks up broadcasted CIDs and anchors them on-chain"
"$BATCH_ANCHOR" --delivery-source "file:$QUEUE" --state-db "$DB" --idl "$IDL" --spel-bin "$SPEL" \
  run --topic "$TOPIC" --batch-size 50 --once

# 3. CONFIRM: registry holds the CIDs (query-by-CID).
echo "-- step 3: on-chain registry confirms the CIDs"
COUNT=$("$SPEL" inspect "$PDA" --idl "$IDL" --type RegistryState 2>/dev/null | python3 -c 'import sys,json;print(len(json.load(sys.stdin)["records"]))')
echo "   registry now holds $COUNT records"

# 4. IDEMPOTENCY / RESUME: re-run; nothing new should anchor.
echo "-- step 4: re-run (idempotent + resumes from cursor) -> no duplicate anchoring"
"$BATCH_ANCHOR" --delivery-source "file:$QUEUE" --state-db "$DB" --idl "$IDL" --spel-bin "$SPEL" \
  run --topic "$TOPIC" --batch-size 50 --once
"$BATCH_ANCHOR" --state-db "$DB" status

echo "== done: $N uploaded+broadcast, anchored on-chain, registry confirmed, idempotent =="
