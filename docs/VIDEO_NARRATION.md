# LP-0017 Whistleblower — video narration

Read this over the silent screencast (`docs/whistleblower-demo.mp4`). Each block is keyed
to the on-screen step number. Plain, unhurried. Pause where a proof runs.

---

**Intro (title card)**
This is Whistleblower. It anchors documents on the Logos execution zone so their existence
and timestamp can't be denied or quietly erased. Three people I built it for: a journalist
who needs to prove a leaked file existed before a takedown, a human-rights group preserving
evidence a government would rather lose, and an archivist timestamping a dataset so nobody
can claim it was altered later. Everything you see uses real RISC0 proofs. Dev mode is off.

**Step 0 — proofs are real**
First thing: `RISC0_DEV_MODE=0`. The sequencer running here generates real zero-knowledge
proofs for every state transition, not mocks. The cycle counts later are the genuine cost.

**Step 1 — deploy the registry**
The on-chain registry is a RISC0 guest program; its image id is its program id, so the
deploy is deterministic. One small program is the whole trusted surface.

**Step 2 — the IDL**
It ships a typed SPEL interface: `initialize` sets up the registry account, `anchor_batch`
takes the CIDs, metadata hashes, and timestamps. Anyone can generate a client from this.

**Step 3 — initialize**
I create the registry state account. A real proof landing on chain, so it takes a moment.
When it confirms you get the PDA address that holds every record.

**Step 4 — upload and broadcast**
Now the publishing side. I upload twelve documents to Logos Storage, each getting a content
CID, and broadcast a metadata envelope, cid plus title and type, to the Logos Delivery
topic. The moment it's broadcast, the document is findable by anyone subscribed, with no
on-chain fee and no coordination.

**Step 5 — the permissionless batch anchor tool**
This is the key idea. The publisher never has to touch the chain. Any third party, an NGO,
a journalist collective, an automated guardian, runs this tool. It subscribes to the
delivery topic, gathers the broadcasted CIDs, and commits all twelve in a single on-chain
transaction. Watch the tx confirm, one real proof for the whole batch.

**Step 6 — compute cost**
Here's the performance number the prize asks for: the real RISC0 cycles for that batch
anchor, straight from the prover. A single-CID anchor fits one segment at 262 thousand
cycles; batching amortises that across many documents.

**Step 7 — the registry confirms**
Now I read it back. The registry decodes into the list of records and I can look up any
document by its CID. This is what a verifier does to confirm a leak was anchored, and when.

**Step 8 — idempotent and resumable**
I run the tool again. Nothing new anchors: the CIDs are already registered, and the tool
resumes from its saved cursor. Re-broadcasts, retries, a crash mid-run, none of it produces
duplicates.

**Step 9 — the reusable module**
The upload-broadcast-anchor logic is a standalone Rust crate. It drives the real Logos
storage module for the content CID and the delivery module for the broadcast, with dedup
and retry, and its tests pass. Any Logos app can pull it in and get censorship-resistant
publishing without depending on Whistleblower.

**Close (criteria card)**
That's the whole pipeline on a live chain with real proofs: upload, broadcast, a
permissionless batch anchor, the registry confirming, all of it reproducible. Thanks for
watching.
