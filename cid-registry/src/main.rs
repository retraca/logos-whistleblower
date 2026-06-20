//! SPEL program entry point for the LP-0017 CID registry.
//! Annotated with #[lez_program] to enable IDL generation via `spel generate`.

#![no_main]

use borsh::{BorshDeserialize, BorshSerialize};
use cid_registry::{
    CID_BYTES, DocumentRecord, ERR_BATCH_TOO_LARGE, ERR_INVALID_CID, MAX_BATCH_SIZE, RegistryState,
};
use nssa_core::account::{AccountWithMetadata, Data};
use spel_framework::error::SpelError;
use spel_framework::prelude::*;

risc0_zkvm::guest::entry!(main);

#[lez_program]
mod cid_registry_program {
    #[allow(unused_imports)]
    use super::*;

    /// Initialise an empty CID registry state account.
    #[instruction]
    pub fn initialize(
        #[account(init, pda = literal("registry_state"))]
        mut registry: AccountWithMetadata,
        #[account(signer)]
        authority: AccountWithMetadata,
    ) -> SpelResult {
        let state = RegistryState { records: vec![] };
        registry.account.data =
            Data::try_from(borsh::to_vec(&state).map_err(|e| SpelError::SerializationError {
                message: e.to_string(),
            })?)
            .map_err(|e| SpelError::SerializationError {
                message: format!("state too large: {e:?}"),
            })?;
        Ok(SpelOutput::execute(vec![registry, authority], vec![]))
    }

    /// Anchor a batch of CIDs (up to 50) on-chain. Idempotent: known CIDs are skipped.
    ///
    /// `entries_cids`: each entry is 46 bytes (base58 multihash).
    /// `entries_meta_hashes`: SHA256 of the delivery metadata envelope, one per CID.
    /// `entries_timestamps`: Unix timestamp (seconds) at anchor time, one per CID.
    #[instruction]
    pub fn anchor_batch(
        #[account(mut, pda = literal("registry_state"))]
        mut registry: AccountWithMetadata,
        entries_cids: Vec<Vec<u8>>, // each entry is a 46-byte CID (validated below)
        entries_meta_hashes: Vec<[u8; 32]>,
        entries_timestamps: Vec<u64>,
    ) -> SpelResult {
        if entries_cids.len() > MAX_BATCH_SIZE {
            return Err(SpelError::Custom {
                code: ERR_BATCH_TOO_LARGE,
                message: "batch exceeds 50-CID limit".to_string(),
            });
        }

        let mut state =
            RegistryState::try_from_slice(registry.account.data.as_ref()).map_err(|_| {
                SpelError::Custom {
                    code: ERR_BATCH_TOO_LARGE,
                    message: "state deserialise failed".to_string(),
                }
            })?;

        let existing: std::collections::HashSet<[u8; CID_BYTES]> =
            state.records.iter().map(|r| r.cid).collect();

        let n = entries_cids.len();
        for i in 0..n {
            let cid_bytes = &entries_cids[i];
            if cid_bytes.len() != CID_BYTES {
                return Err(SpelError::Custom {
                    code: ERR_INVALID_CID,
                    message: "CID must be 46 bytes".to_string(),
                });
            }
            let mut cid = [0u8; CID_BYTES];
            cid.copy_from_slice(cid_bytes);
            if existing.contains(&cid) {
                continue; // idempotent: skip already-registered CIDs
            }
            state.records.push(DocumentRecord {
                cid,
                metadata_hash: entries_meta_hashes[i],
                anchor_timestamp: entries_timestamps[i],
            });
        }

        registry.account.data =
            Data::try_from(borsh::to_vec(&state).map_err(|e| SpelError::SerializationError {
                message: e.to_string(),
            })?)
            .map_err(|e| SpelError::SerializationError {
                message: format!("state too large: {e:?}"),
            })?;

        Ok(SpelOutput::execute(vec![registry], vec![]))
    }
}
