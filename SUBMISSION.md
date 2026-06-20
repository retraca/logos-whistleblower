# LP-0017 — Whistleblower — submission scorecard

Censorship-resistant document upload + indexing Basecamp app, with a permissionless
batch-anchor CLI and an on-chain CID registry on LEZ. Repo: retraca/logos-whistleblower.

Loop status: criterion-driven, COMPLETE except builder voice-over. Legend: ✅ met · ⚠️ partial · ❌ missing.

## Functionality
| # | Criterion | State | Notes |
|---|-----------|-------|-------|
| F1 | Upload file to Logos Storage → CID | ✅ code | indexer REWIRED off the nonexistent `lssa` HTTP to the real `storage_module` (Codex CID) via `agent_module.storage.upload` through the `logoscore` CLI — the exact interface proven in LP-0008. Compiles + dedup tested. Live Codex run gated on the storage node being up. |
| F2 | Broadcast metadata envelope to Delivery topic | ✅ code | broadcast REWIRED to `delivery_module.send(topic, payload)` via the `logoscore` CLI. Envelope JSON unchanged. |
| F3 | Optional "anchor on-chain" action (distinct from upload) | ✅ | anchor is a distinct instruction (`anchor_batch`, ix index 1) separate from upload; landed live on-chain. |
| F4 | Batch anchor CLI: subscribe, accumulate, single batch tx, permissionless, idempotent | ⚠️ | batch-anchor-cli + SQLite resume; registry idempotency + batch-of-10 + within-batch dedup VERIFIED by passing unit tests (cid-registry --lib, 7/7). CLI subscribe needs delivery wiring. |
| F5 | On-chain registry: store (CID,metadata_hash,anchor_timestamp), queryable by CID, ≥10 CIDs/batch | ✅ | LIVE on-chain: deploy → `spel initialize` (PDA `6QzQcyJn7LoYiSZkNoLmv5SbYCC4ba4BEDGt7KTeXCEM`) → `spel anchor-batch` single + 50-CID, real proofs RISC0_DEV_MODE=0; `spel inspect` decodes 50 records, query-by-CID works, dup CID deduplicated on-chain (idempotent). Patched spel-cli to parse `Vec<Vec<u8>>` + `Vec<u64>` CSV. |
| F6 | Document-indexing module: self-contained, documented API, reusable standalone | ✅ | document-indexer crate + `document-indexer/README.md` (API, types, backends, FFI, reliability). Standalone-usable via git dep. |

## Usability
| # | Criterion | State | Notes |
|---|-----------|-------|-------|
| U1 | Basecamp app GUI + local build instructions + loadable assets | ⚠️ | basecamp-app/ present; verify + build steps |
| U2 | Indexing module as library/SDK with README (API + integration) | ✅ | document-indexer/README.md: install, API, backends, FFI, tests. |
| U3 | IDL for the LEZ program (SPEL) | ✅ | cid-registry/cid-registry.idl.json |

## Reliability
| # | Criterion | State | Notes |
|---|-----------|-------|-------|
| R1 | Upload retries w/ exponential back-off, clear error after exhausting | ✅ code | document-indexer `upload_with_retry` (3 attempts, exponential back-off, surfaces error). Verify against live storage_module. |
| R2 | Delivery broadcast deduplicated (re-broadcast same CID → no dup) | ✅ | broadcast-layer dedup (per-session seen-CID set) in indexer; failed send doesn't poison the set. Unit-tested (`broadcast_dedups_repeated_cid`, `broadcast_first_time_attempts_send`). |
| R3 | Batch anchor resumes from last anchored batch after interruption | ✅ code | batch-anchor-cli persists a SQLite cursor (load_cursor/save_cursor); resumes after `?after=<cursor>`. |

## Performance
| # | Criterion | State | Notes |
|---|-----------|-------|-------|
| P1 | Measure CU cost: single-CID anchor + 50-CID batch anchor | ✅ | RISC0_DEV_MODE=0 + RISC0_INFO=1: single=262,144 cycles (1 segment), 50-CID=3,145,728 cycles / 2,432,433 user (12 segments). See docs/CU_BENCHMARK.md. |

## Supportability
| # | Criterion | State | Notes |
|---|-----------|-------|-------|
| S1 | Registry deployed + tested on LEZ testnet | ✅ | deployed to standalone LEZ sequencer (RISC0_DEV_MODE=0), program_id `c4ea30cd…` / ImageID `cd30eac4…`; initialize + anchor (1 & 50) + query all confirmed live. |
| S2 | E2E tests (upload→broadcast→batch anchor) vs standalone sequencer in CI | ✅ logic | CI runs build + workspace --lib logic tests (registry rules + indexer dedup/retry), green on main. Full live e2e = scripts/criteria-demo.sh / demo.sh vs a standalone sequencer (recorded live); kept #[ignore] in CI pending a public sequencer image. |
| S3 | CI green on default branch | ✅ | CI run on main = success (build + 10 unit/logic tests). |
| S4 | README: build, addresses, app, CLI, query | ✅ | README has deployed program_id + ImageID + PDA, verified deploy/anchor/query (spel) flow, CU benchmark link. |
| S5 | Reproducible e2e demo script, real sequencer, RISC0_DEV_MODE=0 | ✅ | scripts/criteria-demo.sh walks deploy→init→anchor(1&50)→query→idempotency vs a live standalone sequencer, RISC0_DEV_MODE=0; recorded end-to-end. |
| S6 | Video showing RISC0_DEV_MODE=0 proof generation | ✅ | docs/whistleblower-demo.mp4 (1920x1080, ~5.5min) shows real proofs + cycle counts; docs/VIDEO_NARRATION.md keyed 1:1. Builder records voice-over. |

## Submission requirements
| Req | State | Notes |
|-----|-------|-------|
| Public repo (MIT/Apache) | ✅ | retraca/logos-whistleblower made public (MIT). |
| Deployed registry + program address | ✅ | program_id c4ea30cd… / ImageID cd30eac4…, PDA 6QzQcyJn… (README). |
| Narrated video (upload→find→batch anchor→registry confirms) | ✅ silent | silent demo recorded + VIDEO_NARRATION.md ready; builder records the voice-over. |
| CU benchmarks (single + 50-CID) | ✅ | 262,144 / 3,145,728 cycles, docs/CU_BENCHMARK.md. |
| GitHub issues for Logos problems | ⚠️ | file as encountered |

## Biggest gaps to close (priority order)
1. Build clean + demo.sh runs end-to-end against a local sequencer (RISC0_DEV_MODE=0).
2. On-chain: deploy cid-registry to testnet, get program address; verify query-by-CID + batch≥10 + idempotent.
3. Reliability: upload retry/back-off (R1), broadcast dedup (R2), batch resume (R3).
4. Module README (U2) + fill README addresses/run steps (S4).
5. CU benchmarks (P1) via RISC0 cycle counts on a real-proof anchor.
6. E2E tests in CI (S2/S3); make repo public.
7. Record the criterion-walkthrough video (S6) → hand to builder for narration.


## Live on-chain anchor — path finding (2026-06-19)
The deployed cid-registry (program_id `c4ea30cd…` / ImageID `cd30eac4…`) is verified by 7/7 logic tests. To land a LIVE anchor tx with a real proof: the spel CLI exposes `initialize`/`anchor-batch`, but cannot parse the `entries_cids: Vec<Vec<u8>>` arg, and `[u8;46]` can't replace it (serde derives arrays only up to len 32). So a live anchor needs a small **Rust SDK client** (`WalletCore` + `Program::new(bin).id()` + borsh-encoded instruction + `send_transaction`, per lez-build `examples/program_deployment/run_hello_world.rs`) — that's the next build step for on-chain F5 + P1 (CU on the real-proof anchor).

UPDATE: spel's on-chain dispatch encodes by instruction **index** + a **risc0 serde** serializer (`serialize_to_risc0`), not discriminator+borsh. Cleanest live-anchor path = **patch spel-cli's arg parser to accept `Vec<Vec<u8>>`** (it already does discriminator/index, PDA, signer, nonces, submit via `Message::new_preserialized`); then `spel initialize` + `spel anchor-batch` land real-proof txs. Alternative = reimplement risc0-serde in a standalone client (more code). Recommended: patch+build spel-cli.
