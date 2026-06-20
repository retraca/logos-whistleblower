//! On-chain CID registry for LP-0017 Whistleblower.
//!
//! Stores (CID, metadata_hash, anchor_timestamp) per document.
//! Accepts batch submissions of up to 50 CIDs per transaction.
//! Queries by CID. Idempotent: re-submitting a known CID is a no-op.

use borsh::{BorshDeserialize, BorshSerialize};
use nssa_core::account::{Account, AccountId, AccountWithMetadata, Data};
use spel_framework::account_type;
use spel_framework::error::SpelError;

// ── Error codes ─────────────────────────────────────────────────────────────

/// Batch exceeds the 50-CID limit per transaction.
pub const ERR_BATCH_TOO_LARGE: u32 = 4001;

/// CID is not 46 bytes (base58-encoded SHA256 multihash).
pub const ERR_INVALID_CID: u32 = 4002;

/// Registry state account is already initialised.
pub const ERR_ALREADY_INITIALIZED: u32 = 4003;

pub const MAX_BATCH_SIZE: usize = 50;
pub const CID_BYTES: usize = 46;

// ── On-chain state ──────────────────────────────────────────────────────────

/// Per-document record anchored on-chain.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct DocumentRecord {
    /// Content identifier -- base58-encoded SHA256 multihash, 46 bytes.
    pub cid: [u8; CID_BYTES],
    /// SHA256 hash of the delivery metadata envelope (32 bytes).
    pub metadata_hash: [u8; 32],
    /// Unix timestamp when this record was anchored (seconds since epoch).
    pub anchor_timestamp: u64,
}

/// Global registry state: list of anchored documents.
/// In practice the on-chain account is paginated per batch; this type
/// represents the contents of a single registry account shard.
#[account_type]
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RegistryState {
    /// Sequential list of anchored document records.
    pub records: Vec<DocumentRecord>,
}

// ── Instructions ────────────────────────────────────────────────────────────

/// Initialise an empty registry state account.
#[must_use]
pub fn initialize(
    registry_account: AccountWithMetadata,
) -> Vec<nssa_core::program::AccountPostState> {
    assert_eq!(
        registry_account.account,
        Account::default(),
        "Registry account must be uninitialised"
    );
    let state = RegistryState { records: vec![] };
    let mut account = registry_account.account;
    account.data = Data::try_from(
        borsh::to_vec(&state).expect("RegistryState must serialise"),
    )
    .expect("serialised state fits Data");
    vec![nssa_core::program::AccountPostState::new(account)]
}

/// Anchor a batch of CIDs.
///
/// Idempotent: CIDs already present in the registry are skipped without error.
/// Returns an error if the batch exceeds `MAX_BATCH_SIZE`.
///
/// # Errors
///
/// Returns `SpelError::Custom(ERR_BATCH_TOO_LARGE)` if `entries.len() > 50`.
/// Returns `SpelError::Custom(ERR_INVALID_CID)` if any CID is not 46 bytes.
pub fn anchor_batch(
    registry_account: AccountWithMetadata,
    entries: Vec<(Vec<u8>, [u8; 32], u64)>, // (cid_bytes, metadata_hash, timestamp)
) -> Result<Vec<nssa_core::program::AccountPostState>, SpelError> {
    let state = RegistryState::try_from_slice(registry_account.account.data.as_ref())
        .expect("RegistryState must deserialise from account data");

    let state = apply_anchor(state, entries)?;

    let mut account = registry_account.account;
    account.data = Data::try_from(
        borsh::to_vec(&state).map_err(|e| SpelError::SerializationError {
            message: e.to_string(),
        })?,
    )
    .map_err(|e| SpelError::SerializationError {
        message: format!("state too large: {e:?}"),
    })?;
    Ok(vec![nssa_core::program::AccountPostState::new(account)])
}

/// Pure state transition for a batch anchor: applies `(cid, metadata_hash, timestamp)`
/// entries to a `RegistryState`. Idempotent — CIDs already present (or repeated within
/// the batch) are skipped without error. Errors on oversized batch or wrong CID length.
/// Extracted from `anchor_batch` so the registry logic is unit-testable without a chain.
///
/// # Errors
/// `ERR_BATCH_TOO_LARGE` if `entries.len() > MAX_BATCH_SIZE`; `ERR_INVALID_CID` if any CID
/// is not `CID_BYTES` long.
pub fn apply_anchor(
    mut state: RegistryState,
    entries: Vec<(Vec<u8>, [u8; 32], u64)>,
) -> Result<RegistryState, SpelError> {
    if entries.len() > MAX_BATCH_SIZE {
        return Err(SpelError::Custom {
            code: ERR_BATCH_TOO_LARGE,
            message: "batch exceeds 50-CID limit".to_string(),
        });
    }
    let mut seen: std::collections::HashSet<[u8; CID_BYTES]> =
        state.records.iter().map(|r| r.cid).collect();
    for (cid_bytes, metadata_hash, anchor_timestamp) in entries {
        if cid_bytes.len() != CID_BYTES {
            return Err(SpelError::Custom {
                code: ERR_INVALID_CID,
                message: "CID must be 46 bytes".to_string(),
            });
        }
        let mut cid = [0u8; CID_BYTES];
        cid.copy_from_slice(&cid_bytes);
        if !seen.insert(cid) {
            continue; // idempotent: already registered or duplicate in batch
        }
        state.records.push(DocumentRecord {
            cid,
            metadata_hash,
            anchor_timestamp,
        });
    }
    Ok(state)
}

/// Look up a document record by CID. Returns `None` if not found.
#[must_use]
pub fn query_by_cid(
    registry_account: &AccountWithMetadata,
    cid: [u8; CID_BYTES],
) -> Option<DocumentRecord> {
    let state = RegistryState::try_from_slice(registry_account.account.data.as_ref())
        .expect("RegistryState must deserialise");
    state.records.into_iter().find(|r| r.cid == cid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nssa_core::account::{Account, AccountId, AccountWithMetadata};

    fn empty() -> RegistryState {
        RegistryState { records: vec![] }
    }

    #[test]
    fn anchor_adds_record() {
        let cid = [0x12u8; CID_BYTES];
        let s = apply_anchor(empty(), vec![(cid.to_vec(), [0xab; 32], 1_700_000_000)]).unwrap();
        assert_eq!(s.records.len(), 1);
        assert_eq!(s.records[0].cid, cid);
    }

    #[test]
    fn anchor_batch_of_10() {
        let entries: Vec<_> = (0..10u8)
            .map(|i| {
                let mut cid = [0u8; CID_BYTES];
                cid[0] = i;
                (cid.to_vec(), [0u8; 32], i as u64)
            })
            .collect();
        let s = apply_anchor(empty(), entries).unwrap();
        assert_eq!(s.records.len(), 10, "a batch of 10 distinct CIDs must all register");
    }

    #[test]
    fn anchor_idempotent_on_known_cid() {
        let cid = [0x12u8; CID_BYTES];
        let initial = RegistryState {
            records: vec![DocumentRecord { cid, metadata_hash: [0xab; 32], anchor_timestamp: 1_000 }],
        };
        let s = apply_anchor(initial, vec![(cid.to_vec(), [0xab; 32], 2_000)]).unwrap();
        assert_eq!(s.records.len(), 1, "re-anchoring a known CID must not append");
    }

    #[test]
    fn anchor_dedups_within_batch() {
        let cid = [0x55u8; CID_BYTES];
        let s = apply_anchor(empty(), vec![(cid.to_vec(), [0; 32], 1), (cid.to_vec(), [0; 32], 2)]).unwrap();
        assert_eq!(s.records.len(), 1, "a CID repeated within one batch must register once");
    }

    #[test]
    fn anchor_rejects_oversized_batch() {
        let entries: Vec<_> = (0..=MAX_BATCH_SIZE)
            .map(|i| {
                let mut cid = [0u8; CID_BYTES];
                cid[0] = i as u8;
                (cid.to_vec(), [0u8; 32], i as u64)
            })
            .collect();
        assert!(apply_anchor(empty(), entries).is_err(), "batch over the limit must error");
    }

    #[test]
    fn anchor_rejects_bad_cid_length() {
        assert!(apply_anchor(empty(), vec![(vec![0u8; 10], [0; 32], 1)]).is_err());
    }

    #[test]
    fn query_by_cid_finds_record() {
        let cid = [0x34u8; CID_BYTES];
        let state = RegistryState {
            records: vec![DocumentRecord { cid, metadata_hash: [0xcd; 32], anchor_timestamp: 9_999 }],
        };
        let registry = AccountWithMetadata {
            account_id: AccountId::default(),
            is_authorized: false,
            account: Account {
                data: Data::try_from(borsh::to_vec(&state).unwrap()).unwrap(),
                ..Account::default()
            },
        };
        let record = query_by_cid(&registry, cid).expect("must find record");
        assert_eq!(record.metadata_hash, [0xcd; 32]);
    }
}
