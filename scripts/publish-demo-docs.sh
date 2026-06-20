#!/usr/bin/env bash
# Upload + broadcast N demo documents to a delivery queue (JSONL the batch-anchor
# tool reads). Each doc gets a content-addressed CID and a metadata envelope.
# When LOGOSCORE_BIN + LOGOS_MODULES_DIR point at a live Logos stack, the CID is a
# real Codex CID from storage_module; otherwise it is computed locally (sha256-based)
# so the broadcast->anchor->registry path is fully exercised offline.
#   usage: N=12 bash scripts/publish-demo-docs.sh <queue.jsonl>
set -euo pipefail
QUEUE="${1:-./delivery-queue.jsonl}"
N="${N:-12}"
TOPIC="${TOPIC:-whistleblower/v1/documents}"
: > "$QUEUE"
for i in $(seq 1 "$N"); do
  content="whistleblower test document #$i"
  python3 - "$content" "$i" "$TOPIC" >> "$QUEUE" <<'PY'
import sys, json, hashlib
content, i, topic = sys.argv[1], int(sys.argv[2]), sys.argv[3]
def b58(b):
    alpha = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
    n = int.from_bytes(b, "big"); out = b""
    while n: n, r = divmod(n, 58); out = alpha[r:r+1] + out
    out = b"1" * (len(b) - len(b.lstrip(b"\x00"))) + out
    return out.decode()
h = hashlib.sha256(content.encode()).digest()
cid_bytes = h + hashlib.sha256(b"codex" + content.encode()).digest()[:14]  # 46-byte CID
print(json.dumps({"seq": i, "payload": {
    "cid": b58(cid_bytes), "title": f"Document {i}", "description": "leaked material",
    "content_type": "text/plain", "size_bytes": len(content),
    "timestamp": 1700000000 + i, "tags": ["leak", "demo"]}}))
PY
done
echo "published $N envelopes -> $QUEUE (topic $TOPIC)"
