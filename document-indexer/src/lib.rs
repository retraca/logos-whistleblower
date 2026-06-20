//! Document-indexer: reusable upload → broadcast → anchor pipeline.
//!
//! Other Logos apps include this crate to get censorship-resistant document
//! publication without depending on the Whistleblower app itself.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use document_indexer::{Indexer, IndexerConfig, MetadataEnvelope, StorageBackend};
//!
//! # async fn run() -> anyhow::Result<()> {
//! let config = IndexerConfig {
//!     storage_url: "http://127.0.0.1:8080".to_string(),
//!     delivery_url: "http://127.0.0.1:9090".to_string(),
//!     sequencer_url: "http://127.0.0.1:3040".to_string(),
//!     delivery_topic: "whistleblower/v1/documents".to_string(),
//!     // production: drive the real storage_module + delivery_module
//!     backend: StorageBackend::from_env(),
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

pub mod ffi;

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

/// Where the indexer puts bytes and broadcasts envelopes.
///
/// `LogosCore` is the production path: it drives the real Logos Core
/// `storage_module` (Codex-backed, via the `agent_module` `storage.upload`
/// skill proven in LP-0008) and `delivery_module` (`send`), through the
/// `logoscore` CLI. The `lssa/logos_storage_service` / `lssa/logos_delivery_service`
/// HTTP images referenced by older drafts do not exist — `Http` is kept only
/// for local mock-server tests.
#[derive(Debug, Clone)]
pub enum StorageBackend {
    LogosCore {
        /// Path to the `logoscore` CLI binary (from the `logos-logoscore-cli` nix build).
        logoscore_bin: String,
        /// Directory holding the built module `.so`s (storage/delivery/agent).
        modules_dir: String,
    },
    Http,
}

impl StorageBackend {
    /// Production default: read paths from `LOGOSCORE_BIN` / `LOGOS_MODULES_DIR`.
    pub fn from_env() -> Self {
        StorageBackend::LogosCore {
            logoscore_bin: std::env::var("LOGOSCORE_BIN").unwrap_or_else(|_| "logoscore".into()),
            modules_dir: std::env::var("LOGOS_MODULES_DIR")
                .unwrap_or_else(|_| "./modules".into()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct IndexerConfig {
    pub storage_url: String,
    pub delivery_url: String,
    pub sequencer_url: String,
    pub delivery_topic: String,
    /// Storage/delivery backend. Production = `StorageBackend::LogosCore`.
    pub backend: StorageBackend,
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            storage_url: "http://127.0.0.1:8080".into(),
            delivery_url: "http://127.0.0.1:9090".into(),
            sequencer_url: "http://127.0.0.1:3040".into(),
            delivery_topic: "whistleblower/v1/documents".into(),
            backend: StorageBackend::from_env(),
        }
    }
}

// ── Indexer ──────────────────────────────────────────────────────────────────

pub struct Indexer {
    config: IndexerConfig,
    client: reqwest::Client,
    /// CIDs already broadcast this session — re-broadcasting the same CID is a
    /// no-op (R2: delivery dedup).
    broadcasted: std::sync::Mutex<std::collections::HashSet<String>>,
}

impl Default for Indexer {
    fn default() -> Self {
        Self::new(IndexerConfig::default())
    }
}

impl Indexer {
    pub fn new(config: IndexerConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("HTTP client"),
            broadcasted: std::sync::Mutex::new(std::collections::HashSet::new()),
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

    /// Check whether a CID is already registered on the on-chain registry.
    pub async fn is_anchored(&self, cid: &str) -> Result<bool> {
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "queryCidRegistry",
            "params": { "cid": cid },
            "id": 1,
        });
        let resp: serde_json::Value = self.client
            .post(&format!("{}/", self.config.sequencer_url))
            .json(&payload)
            .send().await?
            .json().await?;
        Ok(resp["result"]["found"].as_bool().unwrap_or(false))
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
        match &self.config.backend {
            StorageBackend::LogosCore { logoscore_bin, modules_dir } => {
                self.upload_via_logoscore(data, logoscore_bin, modules_dir).await
            }
            StorageBackend::Http => self.upload_via_http(data).await,
        }
    }

    /// Real path: write the bytes to a temp file and drive the Logos Core
    /// `storage_module` (Codex) through `agent_module`'s `storage.upload` skill,
    /// which returns the Codex CID. This is the exact interface proven in LP-0008.
    async fn upload_via_logoscore(
        &self,
        data: &[u8],
        logoscore_bin: &str,
        modules_dir: &str,
    ) -> Result<String> {
        let tmp = std::env::temp_dir().join(format!(
            "whistleblower-upload-{}",
            hex::encode(Sha256::digest(data))
        ));
        tokio::fs::write(&tmp, data).await?;
        let tmp_str = tmp.to_str()
            .ok_or_else(|| anyhow::anyhow!("temp path is not valid UTF-8"))?;
        // The path is interpolated into the logoscore `-c` expression; reject any
        // path that could break out of the quoted string literal. Our filename is
        // hex(sha256), so this only guards a pathological temp_dir.
        if tmp_str.contains('"') || tmp_str.contains('\\') {
            bail!("temp path contains characters unsafe for the logoscore expression");
        }
        let expr = format!("storage.upload(\"{}\", \"whistleblower-doc\")", tmp_str);
        let out = tokio::process::Command::new(logoscore_bin)
            .args(["-m", modules_dir, "-l", "agent_module", "-c", &expr,
                   "--quit-on-finish", "--json-output"])
            .output()
            .await?;
        let _ = tokio::fs::remove_file(&tmp).await;
        if !out.status.success() {
            bail!("storage_module upload failed: {}", String::from_utf8_lossy(&out.stderr));
        }
        let body: serde_json::Value = serde_json::from_slice(&out.stdout)
            .map_err(|e| anyhow::anyhow!("storage_module response not JSON: {e}: {}",
                String::from_utf8_lossy(&out.stdout)))?;
        let cid = body["cid"].as_str()
            .or_else(|| body["result"]["cid"].as_str())
            .ok_or_else(|| anyhow::anyhow!("missing cid in storage_module response: {body}"))?;
        Ok(cid.to_string())
    }

    async fn upload_via_http(&self, data: &[u8]) -> Result<String> {
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

    /// Broadcast the envelope to the Logos Delivery topic. Re-broadcasting a CID
    /// already sent this session is a no-op (R2: delivery dedup).
    async fn broadcast(&self, payload: &BroadcastPayload) -> Result<()> {
        {
            let mut seen = self.broadcasted.lock().expect("broadcast set");
            if seen.contains(&payload.cid) {
                return Ok(()); // already broadcast — dedup
            }
            seen.insert(payload.cid.clone());
        }
        let result = match &self.config.backend {
            StorageBackend::LogosCore { logoscore_bin, modules_dir } => {
                self.broadcast_via_logoscore(payload, logoscore_bin, modules_dir).await
            }
            StorageBackend::Http => self.broadcast_via_http(payload).await,
        };
        if result.is_err() {
            // failed broadcast shouldn't poison the dedup set
            self.broadcasted.lock().expect("broadcast set").remove(&payload.cid);
        }
        result
    }

    async fn broadcast_via_logoscore(
        &self,
        payload: &BroadcastPayload,
        logoscore_bin: &str,
        modules_dir: &str,
    ) -> Result<()> {
        // Topic is interpolated into the logoscore expression — require a strict
        // allowlist so it can't break out of the string literal.
        if self.config.delivery_topic.is_empty()
            || !self.config.delivery_topic.bytes().all(|b|
                b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'-' | b'_'))
        {
            bail!("delivery_topic must be [A-Za-z0-9._/-]+");
        }
        // Hex-encode the envelope JSON so the payload contains only [0-9a-f] and
        // cannot contain a quote or backslash — no escaping guesswork, no breakout.
        // The delivery wire payload for the LogosCore backend is hex(envelope-json).
        let json = serde_json::to_string(payload)?;
        let expr = format!(
            "send(\"{}\", \"{}\")",
            self.config.delivery_topic,
            hex::encode(json.as_bytes())
        );
        let out = tokio::process::Command::new(logoscore_bin)
            .args(["-m", modules_dir, "-l", "delivery_module", "-c", &expr,
                   "--quit-on-finish", "--json-output"])
            .output()
            .await?;
        if !out.status.success() {
            bail!("delivery_module send failed: {}", String::from_utf8_lossy(&out.stderr));
        }
        Ok(())
    }

    async fn broadcast_via_http(&self, payload: &BroadcastPayload) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn http_indexer() -> Indexer {
        // Http backend with an unreachable URL — broadcast() should never reach
        // the network for an already-seen CID, which is what the dedup test checks.
        Indexer::new(IndexerConfig {
            backend: StorageBackend::Http,
            delivery_url: "http://127.0.0.1:1".into(),
            ..IndexerConfig::default()
        })
    }

    fn envelope() -> MetadataEnvelope {
        MetadataEnvelope {
            title: "t".into(),
            description: "d".into(),
            content_type: "text/plain".into(),
            size_bytes: 1,
            timestamp: 1_700_000_000,
            tags: vec![],
        }
    }

    #[tokio::test]
    async fn broadcast_dedups_repeated_cid() {
        let ix = http_indexer();
        let payload = BroadcastPayload { cid: "QmDeadBeef".into(), metadata: envelope() };
        // mark it as already broadcast
        ix.broadcasted.lock().unwrap().insert("QmDeadBeef".into());
        // second broadcast of the same CID is a no-op (returns Ok without hitting the net)
        ix.broadcast(&payload).await.expect("dedup broadcast must be a no-op Ok");
    }

    #[tokio::test]
    async fn broadcast_first_time_attempts_send() {
        let ix = http_indexer();
        let payload = BroadcastPayload { cid: "QmFresh".into(), metadata: envelope() };
        // first send hits the (unreachable) backend and errors; dedup set must NOT
        // retain the CID after a failed send, so a retry can still go out.
        assert!(ix.broadcast(&payload).await.is_err());
        assert!(!ix.broadcasted.lock().unwrap().contains("QmFresh"));
    }

    #[test]
    fn decode_cid_rejects_wrong_length() {
        assert!(decode_cid("Qm123").is_err());
    }
}
