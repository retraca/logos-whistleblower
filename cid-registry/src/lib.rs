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
) -> Vec<nssa_core::account::AccountPostState> {
    assert_eq!(
        registry_account.account,
        Account::default(),
        "Registry account must be uninitialised"
    );
    let state = RegistryState { records: vec![] };
    let mut account = registry_account.account;
    account.data = Data::from_borsh(&state);
    vec![nssa_core::account::AccountPostState::new(account)]
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
) -> Result<Vec<nssa_core::account::AccountPostState>, SpelError> {
    if entries.len() > MAX_BATCH_SIZE {
        return Err(SpelError::Custom {
            code: ERR_BATCH_TOO_LARGE,
        });
    }

    let mut state = RegistryState::try_from_slice(&registry_account.account.data.0)
        .expect("RegistryState must deserialise from account data");

    let existing: std::collections::HashSet<[u8; CID_BYTES]> =
        state.records.iter().map(|r| r.cid).collect();

    for (cid_bytes, metadata_hash, anchor_timestamp) in entries {
        if cid_bytes.len() != CID_BYTES {
            return Err(SpelError::Custom {
                code: ERR_INVALID_CID,
            });
        }
        let mut cid = [0u8; CID_BYTES];
        cid.copy_from_slice(&cid_bytes);
        if existing.contains(&cid) {
            continue; // idempotent
        }
        state.records.push(DocumentRecord {
            cid,
            metadata_hash,
            anchor_timestamp,
        });
    }

    let mut account = registry_account.account;
    account.data = Data::from_borsh(&state);
    Ok(vec![nssa_core::account::AccountPostState::new(account)])
}

/// Look up a document record by CID. Returns `None` if not found.
#[must_use]
pub fn query_by_cid(
    registry_account: &AccountWithMetadata,
    cid: [u8; CID_BYTES],
) -> Option<DocumentRecord> {
    let state = RegistryState::try_from_slice(&registry_account.account.data.0)
        .expect("RegistryState must deserialise");
    state.records.into_iter().find(|r| r.cid == cid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nssa_core::account::{Account, AccountId, AccountWithMetadata};

    fn blank_registry() -> AccountWithMetadata {
        AccountWithMetadata {
            account_id: AccountId::default(),
            account: Account::default(),
        }
    }

    #[test]
    fn initialize_creates_empty_registry() {
        let registry = blank_registry();
        let post = initialize(registry);
        assert_eq!(post.len(), 1);
        let state =
            RegistryState::try_from_slice(&post[0].account.data.0).expect("deserialise");
        assert!(state.records.is_empty());
    }

    #[test]
    fn anchor_batch_adds_records() {
        let mut registry = blank_registry();
        registry.account.data = Data::from_borsh(&RegistryState { records: vec![] });
        let cid = [0x12u8; CID_BYTES];
        let hash = [0xabu8; 32];
        let entries = vec![(cid.to_vec(), hash, 1_700_000_000u64)];
        let post = anchor_batch(registry, entries).unwrap();
        let state =
            RegistryState::try_from_slice(&post[0].account.data.0).expect("deserialise");
        assert_eq!(state.records.len(), 1);
        assert_eq!(state.records[0].cid, cid);
    }

    #[test]
    fn anchor_batch_idempotent() {
        let mut registry = blank_registry();
        let cid = [0x12u8; CID_BYTES];
        let hash = [0xabu8; 32];
        let initial = RegistryState {
            records: vec![DocumentRecord {
                cid,
                metadata_hash: hash,
                anchor_timestamp: 1_000,
            }],
        };
        registry.account.data = Data::from_borsh(&initial);
        let entries = vec![(cid.to_vec(), hash, 2_000u64)];
        let post = anchor_batch(registry, entries).unwrap();
        let state =
            RegistryState::try_from_slice(&post[0].account.data.0).expect("deserialise");
        assert_eq!(state.records.len(), 1, "duplicate CID must not be appended");
    }

    #[test]
    fn anchor_batch_rejects_oversized_batch() {
        let mut registry = blank_registry();
        registry.account.data = Data::from_borsh(&RegistryState { records: vec![] });
        let entries: Vec<_> = (0..=MAX_BATCH_SIZE)
            .map(|i| {
                let mut cid = [0u8; CID_BYTES];
                cid[0] = i as u8;
                (cid.to_vec(), [0u8; 32], i as u64)
            })
            .collect();
        let result = anchor_batch(registry, entries);
        assert!(result.is_err());
    }

    #[test]
    fn query_by_cid_finds_record() {
        let cid = [0x34u8; CID_BYTES];
        let hash = [0xcdu8; 32];
        let state = RegistryState {
            records: vec![DocumentRecord {
                cid,
                metadata_hash: hash,
                anchor_timestamp: 9_999,
            }],
        };
        let registry = AccountWithMetadata {
            account_id: AccountId::default(),
            account: Account {
                data: Data::from_borsh(&state),
                ..Account::default()
            },
        };
        let record = query_by_cid(&registry, cid).expect("must find record");
        assert_eq!(record.metadata_hash, hash);
    }
}
