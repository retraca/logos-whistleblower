// Integration tests for LP-0017 Whistleblower.
// Requires the full Logos stack running locally:
//   docker compose up -d
//
// Run with:
//   cargo test --workspace --test integration -- --include-ignored

use document_indexer::{Indexer, IndexerConfig, MetadataEnvelope};

fn default_config() -> IndexerConfig {
    IndexerConfig {
        storage_url: std::env::var("STORAGE_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8080".into()),
        delivery_url: std::env::var("DELIVERY_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:9090".into()),
        sequencer_url: std::env::var("SEQUENCER_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:3040".into()),
        delivery_topic: "whistleblower/v1/documents/test".into(),
    }
}

#[tokio::test]
#[ignore = "requires local Logos stack"]
async fn test_upload_and_broadcast_returns_cid() {
    let indexer = Indexer::new(default_config());
    let data = b"integration test document";
    let meta = MetadataEnvelope {
        title: "Integration Test".into(),
        description: "LP-0017 integration test".into(),
        content_type: "text/plain".into(),
        size_bytes: data.len() as u64,
        timestamp: 1_700_000_000,
        tags: vec![],
    };
    let result = indexer.upload_and_broadcast(data, meta).await
        .expect("upload_and_broadcast must succeed");
    assert!(!result.cid.is_empty(), "CID must not be empty");
    assert_ne!(result.metadata_hash, [0u8; 32], "metadata hash must be non-zero");
}

#[tokio::test]
#[ignore = "requires local Logos stack"]
async fn test_anchor_batch_empty_is_noop() {
    let indexer = Indexer::new(default_config());
    let result = indexer.anchor_batch(vec![], 1_700_000_000).await
        .expect("empty anchor_batch must succeed");
    assert_eq!(result, "no-op");
}

#[tokio::test]
#[ignore = "requires local Logos stack"]
async fn test_anchor_batch_10_documents() {
    let indexer = Indexer::new(default_config());
    let mut entries = vec![];
    for i in 0..10u8 {
        let data = format!("batch document {i}");
        let meta = MetadataEnvelope {
            title: format!("Batch Doc {i}"),
            description: format!("batch test doc {i}"),
            content_type: "text/plain".into(),
            size_bytes: data.len() as u64,
            timestamp: 1_700_000_000 + i as u64,
            tags: vec![],
        };
        let result = indexer
            .upload_and_broadcast(data.as_bytes(), meta)
            .await
            .expect("upload must succeed");
        entries.push((result.cid, result.metadata_hash));
    }
    assert_eq!(entries.len(), 10);
    let tx = indexer
        .anchor_batch(entries, 1_700_000_000)
        .await
        .expect("anchor_batch must succeed for 10 documents");
    assert!(!tx.is_empty());
    println!("batch anchor tx: {tx}");
}

#[tokio::test]
#[ignore = "requires local Logos stack"]
async fn test_anchor_batch_idempotent_on_resubmit() {
    let indexer = Indexer::new(default_config());
    let data = b"idempotency test document";
    let meta = MetadataEnvelope {
        title: "Idempotency Test".into(),
        description: "Tests that re-anchoring a known CID does not fail".into(),
        content_type: "text/plain".into(),
        size_bytes: data.len() as u64,
        timestamp: 1_700_000_001,
        tags: vec![],
    };
    let result = indexer.upload_and_broadcast(data, meta).await.expect("upload");
    let entries = vec![(result.cid.clone(), result.metadata_hash)];
    // First anchor
    indexer.anchor_batch(entries.clone(), 1_700_000_001).await.expect("first anchor");
    // Second anchor: same CID, must not fail
    indexer.anchor_batch(entries, 1_700_000_002).await.expect("idempotent re-anchor");
}
