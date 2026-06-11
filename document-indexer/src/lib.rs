//! Document-indexer: reusable upload → broadcast → anchor pipeline.
//!
//! Other Logos apps include this crate to get censorship-resistant document
//! publication without depending on the Whistleblower app itself.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use document_indexer::{Indexer, IndexerConfig, MetadataEnvelope};
//!
//! # async fn run() -> anyhow::Result<()> {
//! let config = IndexerConfig {
//!     storage_url: "http://127.0.0.1:8080".to_string(),
//!     delivery_url: "http://127.0.0.1:9090".to_string(),
//!     sequencer_url: "http://127.0.0.1:3040".to_string(),
//!     delivery_topic: "whistleblower/v1/documents".to_string(),
//! };
//! let indexer = Indexer::new(config);
//! let result = indexer.upload_and_broadcast(
//!     b"document contents",
//!     MetadataEnvelope {
//!         title: "Test document".into(),
//!         description: "A test".into(),
//!         content_type: "text/plain".into(),
//!         size_bytes: 17,
//!         timestamp: 1_700_000_000,
//!         tags: vec![],
//!     },
//! ).await?;
//! println!("CID: {}", result.cid);
//! # Ok(())
//! # }
//! ```

use cid_registry::CID_BYTES;
use sha2::{Digest, Sha256};
use serde::{Deserialize, Serialize};
use anyhow::{bail, Result};

// ── Envelope ────────────────────────────────────────────────────────────────

/// Metadata broadcast over the Logos Delivery topic alongside a document CID.
/// The delivery envelope schema is the same across all Logos apps that use
/// this module; downstream tools parse it without knowing which app produced it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataEnvelope {
    /// Human-readable document title.
    pub title: String,
    /// Brief description of the document's content.
    pub description: String,
    /// MIME type (e.g. "application/pdf", "text/plain").
    pub content_type: String,
    /// Size in bytes of the raw document uploaded to storage.
    pub size_bytes: u64,
    /// Unix timestamp (seconds since epoch) when this envelope was created.
    pub timestamp: u64,
    /// Optional categorisation tags.
    pub tags: Vec<String>,
}

/// Full broadcast payload: the CID plus its metadata envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadcastPayload {
    /// Base58-encoded SHA256 multihash content identifier.
    pub cid: String,
    #[serde(flatten)]
    pub metadata: MetadataEnvelope,
}

/// Result returned by `upload_and_broadcast`.
#[derive(Debug, Clone)]
pub struct IndexResult {
    /// Base58-encoded CID assigned by Logos Storage.
    pub cid: String,
    /// SHA256 of the serialised metadata envelope (for on-chain anchoring).
    pub metadata_hash: [u8; 32],
}

// ── Config ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct IndexerConfig {
    pub storage_url: String,
    pub delivery_url: String,
    pub sequencer_url: String,
    pub delivery_topic: String,
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            storage_url: "http://127.0.0.1:8080".into(),
            delivery_url: "http://127.0.0.1:9090".into(),
            sequencer_url: "http://127.0.0.1:3040".into(),
            delivery_topic: "whistleblower/v1/documents".into(),
        }
    }
}

// ── Indexer ──────────────────────────────────────────────────────────────────

pub struct Indexer {
    config: IndexerConfig,
    client: reqwest::Client,
}

impl Indexer {
    pub fn new(config: IndexerConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("HTTP client"),
        }
    }

    /// Upload bytes to Logos Storage and broadcast the metadata envelope to the
    /// Logos Delivery topic. Returns the CID and metadata hash.
    ///
    /// Retries upload up to 3 times with exponential back-off on transient errors.
    pub async fn upload_and_broadcast(
        &self,
        data: &[u8],
        metadata: MetadataEnvelope,
    ) -> Result<IndexResult> {
        let cid = self.upload_with_retry(data, 3).await?;
        let metadata_hash = self.hash_envelope(&cid, &metadata);
        let payload = BroadcastPayload {
            cid: cid.clone(),
            metadata,
        };
        self.broadcast(&payload).await?;
        Ok(IndexResult { cid, metadata_hash })
    }

    /// Submit a batch anchor transaction to the on-chain CID registry via the
    /// sequencer RPC.
    ///
    /// Entries are (cid_string, metadata_hash) pairs. Already-registered CIDs
    /// are ignored by the on-chain program (idempotent).
    pub async fn anchor_batch(
        &self,
        entries: Vec<(String, [u8; 32])>,
        timestamp: u64,
    ) -> Result<String> {
        if entries.is_empty() {
            return Ok("no-op".into());
        }
        if entries.len() > cid_registry::MAX_BATCH_SIZE {
            bail!(
                "batch size {} exceeds MAX_BATCH_SIZE {}",
                entries.len(),
                cid_registry::MAX_BATCH_SIZE
            );
        }
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "submitTransaction",
            "params": {
                "program": "cid-registry",
                "instruction": "anchor_batch",
                "entries": entries.iter().map(|(cid, hash)| {
                    serde_json::json!({
                        "cid": cid,
                        "metadata_hash": hex::encode(hash),
                        "timestamp": timestamp,
                    })
                }).collect::<Vec<_>>()
            },
            "id": 1
        });
        let resp = self.client
            .post(&format!("{}/", self.config.sequencer_url))
            .json(&payload)
            .send()
            .await?;
        let text = resp.text().await?;
        Ok(text)
    }

    // ── private helpers ──────────────────────────────────────────────────────

    async fn upload_with_retry(&self, data: &[u8], max_attempts: u32) -> Result<String> {
        let mut delay = std::time::Duration::from_millis(200);
        for attempt in 1..=max_attempts {
            match self.upload_once(data).await {
                Ok(cid) => return Ok(cid),
                Err(e) if attempt < max_attempts => {
                    tokio::time::sleep(delay).await;
                    delay *= 2;
                    eprintln!("upload attempt {attempt} failed: {e}, retrying");
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    }

    async fn upload_once(&self, data: &[u8]) -> Result<String> {
        let part = reqwest::multipart::Part::bytes(data.to_vec())
            .file_name("document")
            .mime_str("application/octet-stream")?;
        let form = reqwest::multipart::Form::new().part("file", part);
        let resp = self.client
            .post(&format!("{}/upload", self.config.storage_url))
            .multipart(form)
            .send()
            .await?;
        if !resp.status().is_success() {
            bail!("storage upload failed: {}", resp.status());
        }
        let body: serde_json::Value = resp.json().await?;
        let cid = body["cid"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing cid in storage response"))?
            .to_string();
        Ok(cid)
    }

    async fn broadcast(&self, payload: &BroadcastPayload) -> Result<()> {
        let resp = self.client
            .post(&format!(
                "{}/publish/{}",
                self.config.delivery_url, self.config.delivery_topic
            ))
            .json(payload)
            .send()
            .await?;
        if !resp.status().is_success() {
            bail!("delivery broadcast failed: {}", resp.status());
        }
        Ok(())
    }

    fn hash_envelope(&self, cid: &str, metadata: &MetadataEnvelope) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(cid.as_bytes());
        hasher.update(metadata.title.as_bytes());
        hasher.update(metadata.description.as_bytes());
        hasher.update(metadata.content_type.as_bytes());
        hasher.update(metadata.size_bytes.to_le_bytes());
        hasher.update(metadata.timestamp.to_le_bytes());
        hasher.finalize().into()
    }
}

// ── CID helpers ──────────────────────────────────────────────────────────────

/// Decode a base58-encoded CID string into the 46-byte array expected by the
/// on-chain registry. Returns an error if the CID is not exactly 46 bytes.
pub fn decode_cid(cid_str: &str) -> Result<[u8; CID_BYTES]> {
    let bytes = bs58::decode(cid_str).into_vec()?;
    if bytes.len() != CID_BYTES {
        bail!(
            "CID decoded to {} bytes, expected {}",
            bytes.len(),
            CID_BYTES
        );
    }
    let mut arr = [0u8; CID_BYTES];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}
