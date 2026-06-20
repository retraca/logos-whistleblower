//! batch-anchor: permissionless CLI that subscribes to the Logos Delivery topic,
//! accumulates (CID, metadata_hash) tuples, and batch-anchors them on-chain.
//!
//! Idempotent: already-registered CIDs are ignored by the on-chain program.
//! Resumes from the last anchored position after interruption via SQLite state.
//!
//! Usage:
//!   batch-anchor run --topic whistleblower/v1/documents --batch-size 10
//!   batch-anchor status

use anyhow::Result;
use clap::{Parser, Subcommand};
use document_indexer::{BroadcastPayload, Indexer, IndexerConfig};
use rusqlite::Connection;
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Parser)]
#[command(name = "batch-anchor")]
#[command(about = "Permissionless batch CID anchor tool for LP-0017 Whistleblower")]
struct Cli {
    #[arg(long, default_value = "http://127.0.0.1:9090")]
    delivery_url: String,
    #[arg(long, default_value = "http://127.0.0.1:3040")]
    sequencer_url: String,
    #[arg(long, default_value = "whistleblower-state.db")]
    state_db: String,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Subscribe to the delivery topic and anchor accumulated CIDs.
    Run {
        #[arg(long, default_value = "whistleblower/v1/documents")]
        topic: String,
        #[arg(long, default_value = "10")]
        batch_size: usize,
        /// Stop after anchoring N batches (0 = run indefinitely).
        #[arg(long, default_value = "0")]
        max_batches: usize,
        /// Print what would be anchored without submitting any transactions.
        #[arg(long)]
        dry_run: bool,
    },
    /// Show anchoring statistics from the local state DB.
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let db = open_db(&cli.state_db)?;
    match cli.command {
        Commands::Run {
            topic,
            batch_size,
            max_batches,
            dry_run,
        } => run(&cli.delivery_url, &cli.sequencer_url, &db, &topic, batch_size, max_batches, dry_run).await,
        Commands::Status => status(&db),
    }
}

async fn run(
    delivery_url: &str,
    sequencer_url: &str,
    db: &Connection,
    topic: &str,
    batch_size: usize,
    max_batches: usize,
    dry_run: bool,
) -> Result<()> {
    let config = IndexerConfig {
        delivery_url: delivery_url.into(),
        sequencer_url: sequencer_url.into(),
        ..Default::default()
    };
    let indexer = Indexer::new(config);
    let client = reqwest::Client::new();

    let mut batches_submitted = 0usize;
    let mut cursor = load_cursor(db)?;

    println!("Subscribing to {delivery_url}/subscribe/{topic} (cursor={cursor})");

    loop {
        if max_batches > 0 && batches_submitted >= max_batches {
            println!("Reached max_batches={max_batches}, stopping.");
            break;
        }

        // Poll the delivery endpoint for messages after cursor.
        let url = format!("{delivery_url}/messages/{topic}?after={cursor}&limit={batch_size}");
        let resp = client.get(&url).send().await;
        let messages: Vec<Value> = match resp {
            Ok(r) if r.status().is_success() => r.json().await.unwrap_or_default(),
            Ok(r) => {
                eprintln!("delivery poll failed: {}", r.status());
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
            Err(e) => {
                eprintln!("delivery poll error: {e}, retrying in 5s");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
        };

        if messages.is_empty() {
            // No new messages; wait and poll again.
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            continue;
        }

        // Collect entries not yet anchored.
        let mut entries: Vec<(String, [u8; 32])> = vec![];
        let mut new_cursor = cursor;

        for msg in &messages {
            let seq = msg["seq"].as_u64().unwrap_or(0);
            let payload: BroadcastPayload = match serde_json::from_value(msg["payload"].clone()) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("skipping malformed message seq={seq}: {e}");
                    new_cursor = new_cursor.max(seq);
                    continue;
                }
            };
            if is_anchored(db, &payload.cid)? {
                new_cursor = new_cursor.max(seq);
                continue;
            }
            // Recompute metadata hash: SHA256 over canonical JSON of the envelope.
            let hash = metadata_hash_from_payload(&payload);
            entries.push((payload.cid.clone(), hash));
            new_cursor = new_cursor.max(seq);
        }

        if entries.is_empty() {
            save_cursor(db, new_cursor)?;
            cursor = new_cursor;
            continue;
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if dry_run {
            println!(
                "[dry-run] would anchor {} CIDs (cursor {}→{}):",
                entries.len(),
                cursor,
                new_cursor
            );
            for (cid, hash) in &entries {
                println!("  cid={cid}  meta_hash={}", hex::encode(hash));
            }
            save_cursor(db, new_cursor)?;
            cursor = new_cursor;
            batches_submitted += 1;
            continue;
        }

        println!("Anchoring {} CIDs (cursor {}→{})...", entries.len(), cursor, new_cursor);
        match indexer.anchor_batch(entries.clone(), now).await {
            Ok(tx) => {
                println!("Anchored. tx={tx}");
                for (cid, _) in &entries {
                    mark_anchored(db, cid)?;
                }
                save_cursor(db, new_cursor)?;
                cursor = new_cursor;
                batches_submitted += 1;
            }
            Err(e) => {
                eprintln!("anchor_batch failed: {e}, will retry");
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            }
        }
    }
    Ok(())
}

fn status(db: &Connection) -> Result<()> {
    let count: i64 =
        db.query_row("SELECT COUNT(*) FROM anchored", [], |r| r.get(0))?;
    let cursor = load_cursor(db)?;
    println!("anchored: {count} CIDs, cursor: {cursor}");
    Ok(())
}

// ── SQLite state ─────────────────────────────────────────────────────────────

fn open_db(path: &str) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS anchored (cid TEXT PRIMARY KEY);
         CREATE TABLE IF NOT EXISTS state (key TEXT PRIMARY KEY, value TEXT);",
    )?;
    Ok(conn)
}

fn is_anchored(db: &Connection, cid: &str) -> Result<bool> {
    let n: i64 = db.query_row(
        "SELECT COUNT(*) FROM anchored WHERE cid = ?1",
        [cid],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

fn mark_anchored(db: &Connection, cid: &str) -> Result<()> {
    db.execute("INSERT OR IGNORE INTO anchored (cid) VALUES (?1)", [cid])?;
    Ok(())
}

fn load_cursor(db: &Connection) -> Result<u64> {
    let val: Option<String> = db
        .query_row(
            "SELECT value FROM state WHERE key = 'cursor'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    Ok(val.and_then(|v| v.parse().ok()).unwrap_or(0))
}

fn save_cursor(db: &Connection, cursor: u64) -> Result<()> {
    db.execute(
        "INSERT OR REPLACE INTO state (key, value) VALUES ('cursor', ?1)",
        [cursor.to_string()],
    )?;
    Ok(())
}

fn metadata_hash_from_payload(payload: &BroadcastPayload) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let canonical = serde_json::to_string(payload).unwrap_or_default();
    let mut h = Sha256::new();
    h.update(canonical.as_bytes());
    h.finalize().into()
}

trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
