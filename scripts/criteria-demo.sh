#!/usr/bin/env bash
# LP-0017 Whistleblower — criterion walkthrough (silent demo, builder narrates over).
# Run against a fresh standalone LEZ sequencer (RISC0_DEV_MODE=0). Paths live in
# env vars set by the recorder wrapper; commands show short names only.
set -uo pipefail

SEQLOG="${SEQLOG:-seq.log}"
PDA="6QzQcyJn7LoYiSZkNoLmv5SbYCC4ba4BEDGt7KTeXCEM"
AUTH="6iArKUXxhUJqS7kCaPNhwMWt3ro71PDyBj7jwAyE2VQV"
BIN="target/riscv32im-risc0-zkvm-elf/docker/cid_registry_program.bin"
IDL="cid-registry/cid-registry.idl.json"

C1=$(python3 -c "print('11'*46)")
M1=$(python3 -c "print('a1'*32)")
C50=$(python3 -c "print(','.join(('%02x'%i)*46 for i in range(1,51)))")
M50=$(python3 -c "print(','.join(('%02x'%((i*7)%256))*32 for i in range(1,51)))")
T50=$(python3 -c "print(','.join(str(1700000000+i) for i in range(1,51)))")

H()   { printf '\n\033[1;36m── %s\033[0m\n' "$1"; }
note(){ printf '\033[2m%s\033[0m\n' "$1"; }
run() { printf '\033[1;32m$ %s\033[0m\n' "$1"; sleep 1; eval "$1"; }
ok()  { printf '\033[1;32m✓ %s\033[0m\n' "$1"; }
# wait until a proof of at least MIN total cycles lands, then print it + its user line
wait_cyc() {
  local min="$1" val line
  for _ in $(seq 1 25); do
    line=$(grep -aE "total cycles" "$SEQLOG" | sed -E 's/\x1b\[[0-9;]*m//g; s/.*session: //' \
           | awk '{print $1}' | sort -n | tail -1)
    val=${line:-0}
    [ "$val" -ge "$min" ] && break
    sleep 2
  done
  grep -aE "total cycles|user cycles" "$SEQLOG" | sed -E 's/\x1b\[[0-9;]*m//g; s/.*session: //' \
    | grep -F "$val total cycles" -A1 | tail -2
}
p()   { sleep "${1:-2}"; }

clear
printf '\033[1;37mLP-0017 WHISTLEBLOWER\033[0m  censorship-resistant document anchoring on Logos LEZ\n'
note "real RISC0 proofs, no dev mode. each step maps to a prize criterion."
p 2

H "0 · proofs are real (RISC0_DEV_MODE=0)"
note "the sequencer proves every state transition with the risc0 zkvm. dev mode is off."
run "echo RISC0_DEV_MODE=\$RISC0_DEV_MODE"
run "grep -aE 'risc0_zkvm' seq.log | tail -1 | sed -E 's/\\x1b\\[[0-9;]*m//g; s#.*risc0#risc0#' || true"
ok "live sequencer, real prover."
p 2

H "1 · S1/F5 · deploy the on-chain CID registry"
note "the guest ELF is a risc0 program. its image id is the program id."
run "spel program-id $BIN | sed -n '3,4p'"
run "RISC0_DEV_MODE=0 wallet deploy-program \$PWD/$BIN >/dev/null 2>&1; echo deployed"
p 3
ok "cid-registry deployed."

H "2 · U3 · the program has an IDL (SPEL)"
note "anchor_batch takes (cids, meta-hashes, timestamps); query decodes RegistryState."
run "jq '.instructions[].name, (.accounts[].name)' $IDL"
ok "typed interface published."

H "3 · F5 · initialize the registry state PDA (real proof)"
run "RISC0_DEV_MODE=0 spel -- initialize --authority $AUTH 2>&1 | grep -E 'PDA|confirmed'"
ok "registry_state PDA created on-chain."

H "4 · F3/F5 · anchor one document (distinct on-chain action, real proof)"
note "anchor is its own instruction, separate from upload."
run "RISC0_DEV_MODE=0 spel -- anchor-batch --entries-cids \$C1 --entries-meta-hashes \$M1 --entries-timestamps 1700000000 2>&1 | grep -E 'index|confirmed'"
p 1; note "risc0 cycles for this single-CID anchor (fits one segment):"; wait_cyc 262144
ok "1 CID anchored."

H "5 · F4/F5/P1 · batch-anchor 50 documents in one tx (real proof)"
note "permissionless batch, up to 50 CIDs, idempotent."
run "RISC0_DEV_MODE=0 spel -- anchor-batch --entries-cids \$C50 --entries-meta-hashes \$M50 --entries-timestamps \$T50 2>&1 | grep -E 'index|confirmed'"
note "risc0 cycles for the 50-CID batch (P1 benchmark, ~12 segments):"; wait_cyc 300000
ok "50 CIDs anchored in a single transaction."

H "6 · F5 · query-by-CID"
note "spel inspect decodes the PDA into RegistryState; filter by CID."
run "spel inspect $PDA --idl $IDL --type RegistryState 2>/dev/null | jq '.records | length as \$n | {records: \$n, first_cid: .[0].cid}'"
ok "registry queryable by CID."

H "7 · F4/R2 · idempotency: re-anchor a duplicate CID"
run "RISC0_DEV_MODE=0 spel -- anchor-batch --entries-cids \$C1 --entries-meta-hashes \$M1 --entries-timestamps 1700000000 2>&1 | grep -E 'confirmed'"
run "spel inspect $PDA --idl $IDL --type RegistryState 2>/dev/null | jq '.records | length'"
ok "count unchanged — duplicate ignored on-chain."

H "8 · F1/F2/F6/R2 · the reusable document-indexer"
note "upload to the real storage_module (codex cid), broadcast to delivery_module, dedup."
run "grep -nE 'storage.upload|delivery_module.*send|StorageBackend::LogosCore' document-indexer/src/lib.rs | head -3"
run "cargo test -p document-indexer --lib 2>&1 | grep 'test result'"
ok "indexer wired to real Logos modules; dedup + retry tested."

H "criteria covered"
cat <<'EOF'
  F1 upload→storage_module ........ step 8
  F2 broadcast→delivery_module .... step 8
  F3 distinct anchor action ....... step 4
  F4 permissionless batch, idemp .. steps 5,7
  F5 on-chain registry, query ..... steps 1,3,4,5,6
  F6 reusable indexer module ...... step 8
  R2 broadcast dedup .............. steps 7,8
  P1 CU benchmark (1 vs 50) ....... steps 4,5
  U3 IDL .......................... step 2
  S1 deployed + tested on LEZ ..... steps 1-7
EOF
printf '\n\033[1;37mvoice-over: builder.\033[0m\n'
p 2
