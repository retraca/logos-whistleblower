# LP-0017 Whistleblower — video narration

Read this over the silent screencast (`docs/whistleblower-demo.mp4`). Each block is
keyed to the on-screen step number. Plain, unhurried. Pause where the proof runs.

---

**Intro (title card)**
This is Whistleblower. It anchors documents on the Logos execution zone so their
existence and timestamp can't be denied or quietly erased. Three people I built it
for: a journalist who needs to prove a leaked file existed before a takedown, a
human-rights group preserving evidence that a government would rather lose, and an
archivist timestamping a dataset so nobody can claim it was altered later.
Everything you see uses real RISC0 proofs. Dev mode is off.

**Step 0 — proofs are real**
First thing: `RISC0_DEV_MODE=0`. The sequencer you see running is generating real
zero-knowledge proofs for every transition, not mocking them. The cycle counts
later are the genuine cost.

**Step 1 — deploy the registry**
The on-chain registry is a RISC0 guest program. Its image id is its program id, so
the deployment is deterministic. I deploy the ELF to the sequencer. That's the
whole trusted surface: one small program.

**Step 2 — the IDL**
It ships a typed interface. `anchor_batch` takes the CIDs, their metadata hashes,
and timestamps. `initialize` sets up the registry account. Anyone can generate a
client from this.

**Step 3 — initialize**
I create the registry state account. This is a real proof landing on chain, so it
takes a moment. When it confirms you get the PDA address that holds every record.

**Step 4 — anchor one document**
Anchoring is its own action, separate from uploading. Here I anchor a single
document's CID. Watch the cycle count come back from the prover: a single-CID
anchor fits in one RISC0 segment, about 262 thousand cycles.

**Step 5 — batch fifty**
The real workload is batching. One transaction, fifty documents, permissionless,
anyone can submit it. The prover does more work here: about 3.1 million cycles for
the full batch. That's the performance number the prize asks for. Batching is
roughly four times cheaper per document than anchoring one at a time.

**Step 6 — query by CID**
Now I read it back. The registry decodes into a list of records, and I can look up
any document by its CID. This is what a verifier does to confirm a leak was
anchored, and when.

**Step 7 — idempotency**
If I anchor the same CID again, the count doesn't move. The program ignores
duplicates on chain, so re-broadcasts and retries are safe. No double records.

**Step 8 — the indexer**
The upload and broadcast side is a reusable Rust module. It drives the real Logos
storage module for the content CID and the delivery module to broadcast the
envelope, with dedup and retry. The tests pass. Any Logos app can pull this crate
and get censorship-resistant publishing without depending on Whistleblower.

**Close (criteria card)**
That's every criterion, on a live chain, with real proofs. The registry's deployed,
batch anchoring works, the cost is measured, and the indexer's reusable. Thanks for
watching.
