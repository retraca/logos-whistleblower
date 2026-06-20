# LP-0017 — CU / RISC0 cycle benchmark (P1)

Measured on the standalone LEZ sequencer with **`RISC0_DEV_MODE=0`** (real proofs) and
`RISC0_INFO=1`, reading the `risc0_zkvm::host::server::session` cycle report for the
block that contains each `anchor_batch` transaction.

| Anchor | CIDs | Total cycles | User cycles | RISC0 segments |
|--------|------|--------------|-------------|----------------|
| Single-CID | 1 | 262,144 | ~97k | 1 (the minimum 2^18 segment) |
| Batch | 50 | 3,145,728 | 2,432,433 | 12 |

- A single-CID anchor fits inside one RISC0 segment (2^18 = 262,144 cycles), the floor.
- A full 50-CID batch needs 12 segments → 3,145,728 total cycles, 2,432,433 user cycles (77.3%).
- Marginal cost ≈ (3,145,728 − 262,144) / 49 ≈ **~58.8k cycles per additional CID**.
- Amortised per-CID in a full batch ≈ 3,145,728 / 50 ≈ **62.9k cycles/CID**, vs 262,144 for
  a single CID — batching is ~4.2× cheaper per CID.

Reproduce:

```bash
# sequencer started with: RISC0_DEV_MODE=0 RISC0_INFO=1 sequencer_service sequencer_config.json
spel -- anchor-batch --entries-cids <46B-hex> --entries-meta-hashes <32B-hex> --entries-timestamps <u64>
# then read the cycle report from the sequencer log for that block:
grep -E "total cycles|user cycles" sequencer.log
```

Tx hashes (this run): single = `3728295c…d68d4`, 50-CID = `f6aba617…f55c0`.
Program id `c4ea30cd…`, registry PDA `6QzQcyJn7LoYiSZkNoLmv5SbYCC4ba4BEDGt7KTeXCEM`.
