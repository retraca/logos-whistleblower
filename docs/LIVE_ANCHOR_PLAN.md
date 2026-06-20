# LP-0017 — live on-chain anchor plan (ready to execute)

The cid-registry program is built + deployed (program_id `c4ea30cd…` / ImageID `cd30eac4…`)
and its logic is verified by 7/7 unit tests. To land a **live** anchor tx with a real proof
(closing on-chain F5 + the P1 CU benchmark + the video's proof requirement), use the spel CLI —
it already does discriminator/instruction-index encoding, PDA derivation, signer/nonces, and
`send_transaction`. The only gap is its arg parser can't handle `entries_cids: Vec<Vec<u8>>`.

## Step 1 — patch spel-cli to parse `Vec<Vec<u8>>` (one match arm)

In `spel-cli/src/parse.rs`, function `parse_vec`, add this arm **before** the final
`_ => Ok(ParsedValue::Raw(raw.to_string()))`:

```rust
// Vec<Vec<u8>> — comma-separated list of hex byte-vectors (variable length each)
IdlType::Vec { vec } if matches!(vec.as_ref(), IdlType::Primitive(p) if p == "u8") => {
    if raw.is_empty() {
        return Ok(ParsedValue::ByteArrayVec(vec![]));
    }
    let parts: Vec<&str> = raw.split(',').map(|s| s.trim()).collect();
    let mut result = Vec::with_capacity(parts.len());
    for (i, part) in parts.iter().enumerate() {
        let hex = part.strip_prefix("0x").or_else(|| part.strip_prefix("0X")).unwrap_or(part);
        let bytes = hex_decode(hex).map_err(|e| format!("Element [{}]: {}", i, e))?;
        result.push(bytes);
    }
    Ok(ParsedValue::ByteArrayVec(result))
}
```

`serialize.rs` already maps `ByteArrayVec` for an inner `Vec<u8>` (the
`(IdlType::Vec { vec: elem_ty }, ParsedValue::ByteArrayVec(vecs))` arm → `Seq(Seq(U8))`),
so no serializer change is needed. Fork spel locally (don't edit the cargo checkout in place),
apply the arm, `cargo build --release -p spel-cli`.

## Step 2 — initialize + anchor + verify (real proofs, RISC0_DEV_MODE=0)

```bash
# standalone sequencer already up at :3040 (RISC0_DEV_MODE=0)
spel.toml: [program] idl = cid-registry/cid-registry.idl.json, program = <bin>, sequencer = http://127.0.0.1:3040/, signer = <funded key>

spel -- initialize  --authority <pubkey>           # creates the registry_state PDA (real proof)
spel pda registry --program <hex>                  # → registry_state PDA address
spel -- anchor-batch \                             # real-proof anchor (RISC0_INFO=1 → CU)
  --entries-cids <46B-hex>,<46B-hex> \
  --entries-meta-hashes <32B-hex>,<32B-hex> \
  --entries-timestamps 1700000000,1700000001
spel inspect <PDA> --idl <idl> --type RegistryState   # confirm CID registered (query-by-CID)
```

Capture the RISC0 cycle counts for a single-CID anchor and a 50-CID batch anchor → P1 benchmark.

## Then (remaining criteria)
- F1/F2: rewire `document-indexer` upload/broadcast from the nonexistent `lssa` HTTP services
  to the logoscore `storage_module` (real Codex CID, proven in LP-0008) + `delivery_module`.
- U2 module README, S4 README addresses (fill the deployed program_id + PDA).
- S2/S3 e2e-in-CI + make repo public.
- S6 record the criterion-walkthrough video (the recipe is in the lambda-prize-loop skill) → narration.
