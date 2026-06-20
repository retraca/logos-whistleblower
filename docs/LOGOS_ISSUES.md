# Logos issues encountered (LP-0017)

Drafts of GitHub issues to file against Logos repos for problems hit while building
Whistleblower. (Submission requirement: "GitHub issues filed for any problems encountered
with Logos technology.")

---

## 1. spel-cli cannot parse `Vec<Vec<u8>>` (or `Vec<u64>`) instruction arguments
**Repo:** logos-co/spel

`spel`'s raw-arg parser (`spel-cli/src/parse.rs::parse_vec`) handles `Vec<u8>`, `Vec<u32>`,
and `Vec<[u8; N]>`, but has no arm for `Vec<Vec<u8>>` (a list of variable-length byte
vectors) — it falls through to `ParsedValue::Raw`, and serialization then fails with
`type mismatch: expected Vec { vec: Vec { vec: Primitive("u8") } }, got Raw(...)`. There is
also no `Vec<u64>` arm, so a `Vec<u64>` arg with a single element fails the same way.

This blocks calling any instruction whose IDL arg is `vec: { vec: u8 }` — e.g. a batch of
CIDs. Repro: an instruction `anchor_batch(entries_cids: Vec<Vec<u8>>, entries_timestamps:
Vec<u64>)`; `spel -- anchor-batch --entries-cids <hex>,<hex> --entries-timestamps <n>`.

**Fix (verified locally):** add a `Vec<Vec<u8>>` arm to `parse_vec` (comma-separated hex
byte-vectors → `ParsedValue::ByteArrayVec`; the existing `(IdlType::Vec, ByteArrayVec)`
serialize arm already emits `Seq(Seq(U8))`), and a `Vec<u64>` CSV fallback in `serialize.rs`.
Happy to upstream the patch.

---

## 2. `spel generate-idl` omits `accounts`/`types`, so `spel inspect` can't decode account state
**Repo:** logos-co/spel

`spel inspect <account> --idl <idl> --type <T>` decodes account data using the IDL's
`accounts`/`types` definitions, but the IDL emitted by `spel -- generate-idl` for a program
with an `#[account_type] struct RegistryState { records: Vec<DocumentRecord> }` contained
only `instructions` — no `accounts`/`types`. `inspect` then reports `Type 'RegistryState'
not found in IDL`. We had to hand-add the `accounts`/`types` sections. Either `generate-idl`
should emit them for `#[account_type]` structs, or the docs should say it doesn't.

---

## 3. LEZ sequencer exposes no compute-unit / cycle data over RPC
**Repo:** logos-blockchain/logos-execution-zone (or the sequencer service)

The Performance criterion ("measure CU cost") has no RPC surface: `getTransaction`,
`getBlock`, `getAccount` return no compute-unit or cycle field. The only way to get the
real RISC0 cost is to run the sequencer with `RISC0_INFO=1` and scrape `total cycles` /
`user cycles` from the `risc0_zkvm::host::server::session` log lines. A per-tx CU value in
the RPC (or a documented way to query it) would let tools report cost without log scraping.

---

## 4. No published images for the Logos Storage / Delivery / Sequencer services
**Repo:** logos-co/lambda-prize (or the relevant service repos)

The Whistleblower scaffold's `docker-compose.yml` references `lssa/sequencer_service`,
`lssa/logos_storage_service`, and `lssa/logos_delivery_service` images, but these are not
published anywhere we could find, so the compose stack can't come up as written. The real
Logos Storage/Delivery are the logoscore Qt modules (`storage_module` Codex-backed +
`delivery_module`), reachable via the `logoscore` CLI — not standalone HTTP services. Either
publish the service images or update the scaffold to point at the module-based path so an
evaluator can run the e2e from a clean clone.
