use anyhow::{anyhow, Context, Result};
use rusqlite::{params, Connection};
use std::{collections::HashMap, env, fs, path::Path};
use std::time::Duration;

// ---- embed schema so the binary can init DB anywhere ----
const SCHEMA: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/db/schema.sql"));

// =====================
// DB helpers
// =====================

fn db_path() -> String {
    env::var("BITVIA_DB").unwrap_or_else(|_| "./db/bitvia.db".to_string())
}

fn ensure_parent_dir(path: &str) -> Result<()> {
    if let Some(parent) = Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).context("creating DB parent directory")?;
        }
    }
    Ok(())
}

fn open_db() -> Result<Connection> {
    let path = db_path();
    ensure_parent_dir(&path)?;
    let conn = Connection::open(&path).with_context(|| format!("opening sqlite db at {path}"))?;
    Ok(conn)
}

fn cmd_init() -> Result<()> {
    let conn = open_db()?;
    conn.execute_batch(SCHEMA).context("applying schema.sql")?;
    println!("OK: database initialized at {}", db_path());
    Ok(())
}

// =====================
// tiny arg parser
// =====================

/// Turns ["--k","v","--x","y"] into {"k":"v","x":"y"}
fn parse_kv(args: &[String]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut it = args.iter();
    while let Some(k) = it.next() {
        if k.starts_with("--") {
            if let Some(v) = it.next() {
                if !v.starts_with("--") {
                    map.insert(k.trim_start_matches("--").to_string(), v.to_string());
                } else {
                    // flag without value (not used here)
                    map.insert(k.trim_start_matches("--").to_string(), String::new());
                }
            }
        }
    }
    map
}

// =====================
// metrics-insert / metrics-show
// =====================

/// INSERT ... ON CONFLICT(metric_date) DO UPDATE
fn cmd_metrics_insert(rest: &[String]) -> Result<()> {
    let kv = parse_kv(rest);

    let date = kv
        .get("date")
        .ok_or_else(|| anyhow!("--date YYYY-MM-DD is required"))?;
    // Optional fields; parse if present
    let mempool_tx = kv.get("mempool-tx").and_then(|s| s.parse::<i64>().ok());
    let mempool_bytes = kv.get("mempool-bytes").and_then(|s| s.parse::<i64>().ok());
    let avg_block_interval_sec = kv
        .get("avg-block-interval-sec")
        .and_then(|s| s.parse::<f64>().ok());
    let median_fee_sat_per_vb = kv
        .get("median-fee-sat-per-vb")
        .and_then(|s| s.parse::<f64>().ok());

    let conn = open_db()?;
    conn.execute_batch(SCHEMA)?; // ensure tables exist

    let sql = r#"
        INSERT INTO metrics (
            metric_date, mempool_tx, mempool_bytes, avg_block_interval_sec, median_fee_sat_per_vb
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5
        )
        ON CONFLICT(metric_date) DO UPDATE SET
            mempool_tx = excluded.mempool_tx,
            mempool_bytes = excluded.mempool_bytes,
            avg_block_interval_sec = excluded.avg_block_interval_sec,
            median_fee_sat_per_vb = excluded.median_fee_sat_per_vb
    "#;

    let n = conn.execute(
        sql,
        params![
            date,
            mempool_tx,
            mempool_bytes,
            avg_block_interval_sec,
            median_fee_sat_per_vb
        ],
    )?;

    println!("OK: metrics upserted for {} ({} row affected)", date, n);
    Ok(())
}

fn cmd_metrics_show(rest: &[String]) -> Result<()> {
    let kv = parse_kv(rest);
    let limit = kv
        .get("limit")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(10);

    let conn = open_db()?;
    let mut stmt = conn.prepare(
        "SELECT metric_date, mempool_tx, mempool_bytes, avg_block_interval_sec, median_fee_sat_per_vb, created_at
         FROM metrics
         ORDER BY metric_date DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<i64>>(1)?,
            r.get::<_, Option<i64>>(2)?,
            r.get::<_, Option<f64>>(3)?,
            r.get::<_, Option<f64>>(4)?,
            r.get::<_, String>(5)?,
        ))
    })?;

    println!("metric_date | mempool_tx | mempool_bytes | avg_block_interval_sec | median_fee_sat_per_vb | created_at");
    for row in rows {
        let (d, tx, b, abi, fee, created) = row?;
        println!(
            "{} | {} | {} | {} | {} | {}",
            d,
            tx.map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
            b.map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
            abi.map(|v| format!("{:.2}", v)).unwrap_or_else(|| "-".into()),
            fee.map(|v| format!("{:.2}", v)).unwrap_or_else(|| "-".into()),
            created
        );
    }
    Ok(())
}

// =====================
// metrics-collect (RPC → DB)
// =====================

use reqwest::Client;
use serde::Deserialize;

#[derive(Deserialize)]
struct ChainInfo {
    blocks: u64,
    headers: u64,
    verificationprogress: f64,
    initialblockdownload: bool,
}

#[derive(Deserialize)]
struct MempoolInfo {
    size: u64,
    bytes: u64,
    usage: Option<u64>,
    #[serde(default)]
    mempoolminfee: f64, // BTC/kb
}

async fn rpc_call<T: for<'de> Deserialize<'de>>(
    http: &Client,
    url: &str,
    user: &str,
    pass: &str,
    method: &str,
) -> Result<T> {
    let body = serde_json::json!({
        "jsonrpc": "1.0",
        "id": "bitvia",
        "method": method,
        "params": []
    });

    let res = http
        .post(url)
        .basic_auth(user, Some(pass))
        .json(&body)
        .send()
        .await
        .context("RPC HTTP send failed")?;

    let status = res.status();
    let json = res
        .json::<serde_json::Value>()
        .await
        .with_context(|| format!("parse failed ({status})"))?;

    if let Some(err) = json.get("error") {
        if !err.is_null() {
            return Err(anyhow!("RPC error: {err:?}"));
        }
    }

    let result = json
        .get("result")
        .ok_or_else(|| anyhow!("missing 'result' field"))?;
    let typed: T = serde_json::from_value(result.clone())?;
    Ok(typed)
}

async fn rpc_call_params<T: for<'de> Deserialize<'de>>(
    http: &Client,
    url: &str,
    user: &str,
    pass: &str,
    method: &str,
    params_v: serde_json::Value,
) -> Result<T> {
    let body = serde_json::json!({
        "jsonrpc": "1.0",
        "id": "bitvia",
        "method": method,
        "params": params_v
    });
    let res = http.post(url).basic_auth(user, Some(pass)).json(&body).send().await?;
    let v = res.json::<serde_json::Value>().await?;
    if let Some(err) = v.get("error") { if !err.is_null() { return Err(anyhow!("RPC error: {err:?}")); } }
    let result = v.get("result").ok_or_else(|| anyhow!("missing result"))?;
    Ok(serde_json::from_value(result.clone())?)
}

#[derive(Deserialize)]
struct BlockHeader { time: u64 }

async fn get_block_time(
    http: &Client, url: &str, user: &str, pass: &str, height: u64
) -> Result<u64> {
    let hash: String = rpc_call_params(http, url, user, pass, "getblockhash", serde_json::json!([height])).await?;
    let hdr: BlockHeader = rpc_call_params(http, url, user, pass, "getblockheader", serde_json::json!([hash])).await?;
    Ok(hdr.time)
}

async fn cmd_metrics_collect() -> Result<()> {
    // load RPC env (dev: .env; prod: systemd EnvironmentFile)
    dotenvy::dotenv().ok();
    let rpc_url  = env::var("RPC_URL").context("missing RPC_URL")?;
    let rpc_user = env::var("RPC_USER").context("missing RPC_USER")?;
    let rpc_pass = env::var("RPC_PASS").context("missing RPC_PASS")?;

    let http = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .context("failed to build reqwest client")?;

    // Live RPC calls
    let chain: ChainInfo = rpc_call(&http, &rpc_url, &rpc_user, &rpc_pass, "getblockchaininfo").await?;
    let mempool: MempoolInfo = rpc_call(&http, &rpc_url, &rpc_user, &rpc_pass, "getmempoolinfo").await?;

    // Compute average block interval over last N blocks (N=72 is ~12h)
    let tip = chain.blocks;
    let n: u64 = 72;
    let start = tip.saturating_sub(n);

    let mut prev_t = get_block_time(&http, &rpc_url, &rpc_user, &rpc_pass, start).await?;
    let mut total: i64 = 0;
    let mut count: i64 = 0;

    for h in (start + 1)..=tip {
        let t = get_block_time(&http, &rpc_url, &rpc_user, &rpc_pass, h).await?;
        let dt = (t as i64) - (prev_t as i64);
        if dt > 0 && dt < 3600 { // ignore outliers >1h or negative
            total += dt;
            count += 1;
        }
        prev_t = t;
    }

    let avg_block_interval_sec = if count > 0 { (total as f64) / (count as f64) } else { 600.0 };

    // Convert BTC/kB -> sat/vB: sat_per_vb = BTC_per_kB * 1e8 / 1000 = * 1e5
    let mut fee_sat_per_vb = mempool.mempoolminfee * 100_000.0;
    if !fee_sat_per_vb.is_finite() || fee_sat_per_vb < 0.0 {
        fee_sat_per_vb = 0.0;
    }
    // Optional: round to 2 decimals (you can change to whole sats if you prefer)
    fee_sat_per_vb = (fee_sat_per_vb * 100.0).round() / 100.0;

    // Use UTC date string YYYY-MM-DD
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let conn = open_db()?;
    conn.execute_batch(SCHEMA)?; // ensure tables exist

    conn.execute(
        r#"
        INSERT INTO metrics (
            metric_date, mempool_tx, mempool_bytes, avg_block_interval_sec, median_fee_sat_per_vb
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5
        )
        ON CONFLICT(metric_date) DO UPDATE SET
            mempool_tx = excluded.mempool_tx,
            mempool_bytes = excluded.mempool_bytes,
            avg_block_interval_sec = excluded.avg_block_interval_sec,
            median_fee_sat_per_vb = excluded.median_fee_sat_per_vb
        "#,
        params![
            today,
            mempool.size as i64,
            mempool.bytes as i64,
            avg_block_interval_sec,
            fee_sat_per_vb,
        ],
    )?;

    println!(
        "OK: collected metrics for {} (IBD: {}, blocks: {}, mempool_tx: {}, fee_min: {:.2} sat/vB)",
        today, chain.initialblockdownload, chain.blocks, mempool.size, fee_sat_per_vb
    );
    Ok(())
}


// =====================
// entrypoint
// =====================

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        None | Some("init") => cmd_init(),
        Some("metrics-insert") => {
            let rest: Vec<String> = args.collect();
            cmd_metrics_insert(&rest)
        }
        Some("metrics-show") => {
            let rest: Vec<String> = args.collect();
            cmd_metrics_show(&rest)
        }
        Some("metrics-collect") => cmd_metrics_collect().await,
        Some(other) => Err(anyhow!(
            "unknown command: {other}\n\
             commands:\n  init\n  metrics-insert --date YYYY-MM-DD [--mempool-tx N --mempool-bytes N --avg-block-interval-sec X --median-fee-sat-per-vb X]\n  metrics-show [--limit N]\n  metrics-collect"
        )),
    }
}
