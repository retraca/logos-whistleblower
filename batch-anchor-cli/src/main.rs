//! batch-anchor: permissionless CLI that gathers broadcasted Whistleblower CIDs
//! from the Logos Delivery topic and batch-anchors them on the on-chain registry.
//!
//! - Permissionless: anyone runs it; no coordination with the original publisher.
//! - Idempotent: already-registered CIDs are skipped (local dedup) and the on-chain
//!   program ignores duplicates too.
//! - Resumable: a SQLite cursor + anchored-set survive interruption, so a restart
//!   does not re-process already-anchored CIDs.
//!
//! Delivery source (`--delivery-source`):
//!   - `file:<path.jsonl>`  newline-delimited `{ "seq": <u64>, "payload": {envelope} }`.
//!     This is what the Logos Delivery topic is bridged to for the reproducible e2e
//!     (and what `demo.sh` writes). Works offline against a standalone sequencer.
//!   - `http:<url>`         a delivery HTTP bridge exposing `/messages/<topic>?after=&limit=`.
//!
//! Anchoring uses the real `spel` path (`spel -- anchor-batch ...`) against the LEZ
//! sequencer, so the on-chain tx is a genuine RISC0-proved registry update.
//!
//! Usage:
//!   batch-anchor run --topic whistleblower/v1/documents --batch-size 50
//!   batch-anchor status

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use document_indexer::{decode_cid, BroadcastPayload};
use rusqlite::Connection;
use serde_json::Value;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Parser)]
#[command(name = "batch-anchor")]
#[command(about = "Permissionless batch CID anchor tool for LP-0017 Whistleblower")]
struct Cli {
    /// Delivery source: `file:<path.jsonl>` (default) or `http:<url>`.
    #[arg(long, default_value = "file:./delivery-queue.jsonl")]
    delivery_source: String,
    /// `spel` binary used to submit the on-chain anchor transaction.
    #[arg(long, env = "SPEL_BIN", default_value = "spel")]
    spel_bin: String,
    /// IDL passed to spel (relative to --workdir).
    #[arg(long, default_value = "cid-registry/cid-registry.idl.json")]
    idl: String,
    /// Working directory for spel (must contain spel.toml + the IDL + program binary).
    #[arg(long, default_value = ".")]
    workdir: String,
    #[arg(long, default_value = "whistleblower-state.db")]
    state_db: String,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Gather CIDs from the delivery source and anchor accumulated CIDs on-chain.
    Run {
        #[arg(long, default_value = "whistleblower/v1/documents")]
        topic: String,
        #[arg(long, default_value = "50")]
        batch_size: usize,
        /// Stop after anchoring N batches (0 = run indefinitely).
        #[arg(long, default_value = "0")]
        max_batches: usize,
        /// Print what would be anchored without submitting any transactions.
        #[arg(long)]
        dry_run: bool,
        /// Process the currently-available messages once, then exit (e2e/demo mode).
        #[arg(long)]
        once: bool,
    },
    /// Show anchoring statistics from the local state DB.
    Status,
}

/// Anchor/delivery config (the non-subcommand part of the CLI).
struct Cfg {
    delivery_source: String,
    spel_bin: String,
    idl: String,
    workdir: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let db = open_db(&cli.state_db)?;
    let cfg = Cfg {
        delivery_source: cli.delivery_source,
        spel_bin: cli.spel_bin,
        idl: cli.idl,
        workdir: cli.workdir,
    };
    match cli.command {
        Commands::Run { topic, batch_size, max_batches, dry_run, once } => {
            run(&cfg, &db, &topic, batch_size, max_batches, dry_run, once).await
        }
        Commands::Status => status(&db),
    }
}

/// One delivery message: a monotonically-increasing `seq` (the cursor key) + the envelope.
struct DeliveryMsg {
    seq: u64,
    payload: BroadcastPayload,
}

/// Read messages with `seq > after`, up to `limit`, from the configured source.
async fn poll_delivery(source: &str, topic: &str, after: u64, limit: usize) -> Result<Vec<DeliveryMsg>> {
    if let Some(path) = source.strip_prefix("file:") {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(e).context("reading delivery queue"),
        };
        let mut out = vec![];
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }
            let v: Value = serde_json::from_str(line).context("malformed delivery line")?;
            let seq = v["seq"].as_u64().unwrap_or(0);
            if seq <= after { continue; }
            let payload: BroadcastPayload = serde_json::from_value(v["payload"].clone())
                .context("malformed envelope payload")?;
            out.push(DeliveryMsg { seq, payload });
            if out.len() >= limit { break; }
        }
        out.sort_by_key(|m| m.seq);
        Ok(out)
    } else if let Some(url) = source.strip_prefix("http:").map(|_| source.trim_start_matches("http:")) {
        let url = format!("{url}/messages/{topic}?after={after}&limit={limit}");
        let messages: Vec<Value> = reqwest::get(&url).await?.json().await.unwrap_or_default();
        let mut out = vec![];
        for msg in messages {
            let seq = msg["seq"].as_u64().unwrap_or(0);
            if let Ok(payload) = serde_json::from_value::<BroadcastPayload>(msg["payload"].clone()) {
                out.push(DeliveryMsg { seq, payload });
            }
        }
        Ok(out)
    } else {
        bail!("unknown --delivery-source '{source}' (use file:<path> or http:<url>)");
    }
}

async fn run(
    cli: &Cfg,
    db: &Connection,
    topic: &str,
    batch_size: usize,
    max_batches: usize,
    dry_run: bool,
    once: bool,
) -> Result<()> {
    let mut batches_submitted = 0usize;
    let mut cursor = load_cursor(db)?;
    println!("delivery-source={} topic={topic} cursor={cursor}", cli.delivery_source);

    loop {
        if max_batches > 0 && batches_submitted >= max_batches {
            println!("reached max_batches={max_batches}, stopping.");
            break;
        }

        let messages = poll_delivery(&cli.delivery_source, topic, cursor, batch_size).await?;
        if messages.is_empty() {
            if once { break; }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            continue;
        }

        // (cid_bytes_46, meta_hash_32) for not-yet-anchored CIDs; advance cursor over all seen.
        let mut entries: Vec<([u8; 46], [u8; 32], String)> = vec![];
        let mut new_cursor = cursor;
        for m in &messages {
            new_cursor = new_cursor.max(m.seq);
            if is_anchored(db, &m.payload.cid)? { continue; }
            let cid = match decode_cid(&m.payload.cid) {
                Ok(c) => c,
                Err(e) => { eprintln!("skip invalid cid {}: {e}", m.payload.cid); continue; }
            };
            let hash = metadata_hash_from_payload(&m.payload);
            entries.push((cid, hash, m.payload.cid.clone()));
        }

        if entries.is_empty() {
            save_cursor(db, new_cursor)?;
            cursor = new_cursor;
            if once { break; }
            continue;
        }

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();

        if dry_run {
            println!("[dry-run] would anchor {} CIDs (cursor {}→{}):", entries.len(), cursor, new_cursor);
            for (_, hash, cid) in &entries { println!("  cid={cid}  meta_hash={}", hex::encode(hash)); }
        } else {
            println!("anchoring {} CIDs (cursor {}→{})...", entries.len(), cursor, new_cursor);
            anchor_via_spel(cli, &entries, now)?;
            for (_, _, cid) in &entries { mark_anchored(db, cid)?; }
            println!("anchored {} CIDs on-chain.", entries.len());
        }
        save_cursor(db, new_cursor)?;
        cursor = new_cursor;
        batches_submitted += 1;
        if once { break; }
    }
    Ok(())
}

/// Submit the batch anchor as a real on-chain transaction via `spel -- anchor-batch`.
fn anchor_via_spel(cli: &Cfg, entries: &[([u8; 46], [u8; 32], String)], timestamp: u64) -> Result<()> {
    let cids = entries.iter().map(|(c, _, _)| hex::encode(c)).collect::<Vec<_>>().join(",");
    let metas = entries.iter().map(|(_, h, _)| hex::encode(h)).collect::<Vec<_>>().join(",");
    let times = entries.iter().map(|_| timestamp.to_string()).collect::<Vec<_>>().join(",");
    let out = Command::new(&cli.spel_bin)
        .current_dir(&cli.workdir)
        .args([
            "--idl", &cli.idl, "--",
            "anchor-batch",
            "--entries-cids", &cids,
            "--entries-meta-hashes", &metas,
            "--entries-timestamps", &times,
        ])
        .output()
        .with_context(|| format!("spawning spel ({})", cli.spel_bin))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    // surface just the tx hash + confirmation, not spel's full instruction dump
    for line in stdout.lines() {
        let l = line.trim();
        if l.contains("tx_hash") || l.contains("confirmed") || l.starts_with('❌') {
            println!("  {l}");
        }
    }
    if !out.status.success() {
        bail!("spel anchor-batch failed (exit {:?}): {}", out.status.code(),
              String::from_utf8_lossy(&out.stderr));
    }
    Ok(())
}

fn status(db: &Connection) -> Result<()> {
    let count: i64 = db.query_row("SELECT COUNT(*) FROM anchored", [], |r| r.get(0))?;
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
    let n: i64 = db.query_row("SELECT COUNT(*) FROM anchored WHERE cid = ?1", [cid], |r| r.get(0))?;
    Ok(n > 0)
}

fn mark_anchored(db: &Connection, cid: &str) -> Result<()> {
    db.execute("INSERT OR IGNORE INTO anchored (cid) VALUES (?1)", [cid])?;
    Ok(())
}

fn load_cursor(db: &Connection) -> Result<u64> {
    let val: Option<String> = db
        .query_row("SELECT value FROM state WHERE key = 'cursor'", [], |r| r.get(0))
        .optional()?;
    Ok(val.and_then(|v| v.parse().ok()).unwrap_or(0))
}

fn save_cursor(db: &Connection, cursor: u64) -> Result<()> {
    db.execute("INSERT OR REPLACE INTO state (key, value) VALUES ('cursor', ?1)", [cursor.to_string()])?;
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
