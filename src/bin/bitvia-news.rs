use anyhow::{Context, Result};
use feed_rs::model::Feed;
use reqwest::Client;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::{env, fs, path::Path, time::Duration};
use url::Url;
use chrono::{DateTime, Utc};

// Reuse your schema to guarantee tables exist
const SCHEMA: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/db/schema.sql"));

// ---- config ----
const FEEDS: &[&str] = &[
    "https://www.coindesk.com/arc/outboundfeeds/rss/",
    "https://decrypt.co/feed",
    "https://bitcoinops.org/feed.xml",          // Bitcoin Optech (high signal)
    "https://blog.blockstream.com/rss/",        // Blockstream blog
    "https://insights.glassnode.com/feed/",    // On-chain analytics
    "https://mempool.space/blog/index.xml",    // mempool.space blog
    "https://www.reddit.com/r/Bitcoin/.rss",   // Community pulse
];

fn db_path() -> String {
    env::var("BITVIA_DB").unwrap_or_else(|_| "./db/bitvia.db".to_string())
}

fn ensure_parent_dir(path: &str) -> Result<()> {
    if let Some(p) = Path::new(path).parent() {
        if !p.as_os_str().is_empty() {
            fs::create_dir_all(p).context("create DB parent dir")?;
        }
    }
    Ok(())
}

fn open_db() -> Result<Connection> {
    let path = db_path();
    ensure_parent_dir(&path)?;
    let conn = Connection::open(&path).with_context(|| format!("open sqlite at {path}"))?;
    conn.execute_batch(SCHEMA).context("apply schema.sql")?;
    Ok(conn)
}

fn outlet_from(feed_url: &str, feed: &Feed) -> String {
    if let Some(title) = feed.title.as_ref() {
        let t = title.content.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    Url::parse(feed_url)
        .ok()
        .and_then(|u| u.domain().map(|d| d.to_string()))
        .unwrap_or_else(|| "unknown".to_string())
}

fn sha256_bytes(s: &str) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    h.finalize().to_vec()
}

async fn fetch_feed(http: &Client, url: &str) -> Result<Feed> {
    let bytes = http
        .get(url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?
        .bytes()
        .await
        .context("read body")?;
    feed_rs::parser::parse(&bytes[..]).context("parse feed")
}

fn upsert_article(
    conn: &Connection,
    url: &str,
    title: &str,
    outlet: &str,
    author: Option<&str>,
    published_at: Option<&str>,
    text: &str,
) -> Result<usize> {
    let sha = sha256_bytes(text);
    let n = conn.execute(
        r#"
        INSERT INTO news_sources (url, title, outlet, author, published_at, text, sha256)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(url) DO UPDATE SET
          title=excluded.title,
          outlet=excluded.outlet,
          author=excluded.author,
          published_at=excluded.published_at,
          text=excluded.text,
          sha256=excluded.sha256
        "#,
        params![url, title, outlet, author, published_at, text, sha],
    )?;
    Ok(n)
}

fn prune_old(conn: &Connection, days: i64) -> Result<usize> {
    let n = conn.execute(
        "DELETE FROM news_sources WHERE fetched_at < datetime('now', ?1)",
        params![format!("-{} days", days)],
    )?;
    Ok(n)
}

// Try published, then updated; return both ISO string and parsed DateTime<Utc>
fn entry_pub_dt(entry: &feed_rs::model::Entry) -> (Option<String>, Option<DateTime<Utc>>) {
    // feed-rs gives DateTime<FixedOffset> or DateTime<Utc>; normalize to Utc
    fn to_iso(dt: DateTime<Utc>) -> String {
        dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    }

    if let Some(p) = entry.published {
        let dt = p.with_timezone(&Utc);
        return (Some(to_iso(dt)), Some(dt));
    }
    if let Some(u) = entry.updated {
        let dt = u.with_timezone(&Utc);
        return (Some(to_iso(dt)), Some(dt));
    }
    (None, None)
}

// Consider stale if strictly older than N hours
fn is_stale(pub_dt: Option<DateTime<Utc>>, max_age_hours: i64) -> bool {
    match pub_dt {
        Some(dt) => (Utc::now() - dt).num_hours() > max_age_hours,
        None => false, // keep unknown pub dates; we’ll further filter at read-time
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let http = Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("bitvia-news/0.1")
        .build()
        .context("build reqwest client")?;

    let conn = open_db()?;

    let mut total = 0usize;
    for feed_url in FEEDS {
        match fetch_feed(&http, feed_url).await {
            Ok(feed) => {
                let outlet = outlet_from(feed_url, &feed);
                for entry in feed.entries {
                    // Prefer an external link; fall back to entry.id
                    // Prefer an external link; fall back to entry.id
                    let link = entry
                        .links
                        .iter()
                        .find(|l| l.rel.as_deref().unwrap_or("") != "self")
                        .map(|l| l.href.clone())
                        .unwrap_or_else(|| entry.id.clone());

                    if link.is_empty() {
                        continue; // skip entries without a URL
                    }

                    let title = entry
                        .title
                        .as_ref()
                        .map(|t| t.content.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "Untitled".to_string());

                    // Prefer summary; fall back to first content body; else title.
                    // IMPORTANT: use as_ref() to BORROW, not MOVE.
                    let mut text = entry
                        .summary
                        .as_ref()
                        .map(|s| s.content.trim().to_string())
                        .unwrap_or_default();

                    if text.is_empty() {
                        if let Some(c) = entry.content.as_ref() {
                            if let Some(b) = c.body.as_ref() {
                                text = b.trim().to_string();
                            }
                        }
                    }
                    if text.is_empty() {
                        text = title.clone();
                    }

                    // Author (optional)
                    let author_owned: Option<String> = entry.authors.get(0).map(|p| p.name.clone());
                    let author_opt = author_owned.as_deref(); // Option<&str>

                    // Published at (prefer published, then updated); normalize to UTC
                    let (published_at_owned, published_dt) = entry_pub_dt(&entry);

                    // Skip obviously stale items (older than 48h) even if the feed re-published them
                    if is_stale(published_dt, 48) {
                        // eprintln!("skip stale: {} ({:?})", title, published_at_owned);
                        continue;
                    }

                    // Bind Option<&str> for the upsert
                    let published_opt = published_at_owned.as_deref();

                    // Upsert (dedup by URL as you already do)
                    let changed = upsert_article(
                        &conn,
                        &link,
                        &title,
                        &outlet,
                        author_opt,
                        published_opt,
                        &text,
                    )?;
                    if changed > 0 {
                        total += 1;
                    }
                }
            }
            Err(e) => eprintln!("WARN: failed {feed_url}: {e:#}"),
        }
    }

    let pruned = prune_old(&conn, 3).unwrap_or(0);
    println!("OK: upserted {} items; pruned {} old rows", total, pruned);
    Ok(())
}
