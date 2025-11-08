use anyhow::{Context, Result};
use clap::Parser;
use regex::Regex;
use reqwest::Client;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{env, fs, path::Path, time::Duration as StdDur};

const SCHEMA: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/db/schema.sql"));

#[derive(Parser, Debug)]
#[command(name="bitvia-ai", about="Generate Bitvia daily digest (metrics + news)")]
struct Args {
    /// Print output and skip DB writes
    #[arg(long)]
    dry_run: bool,

    /// OpenAI model (default: gpt-5-mini)
    #[arg(long, default_value = "gpt-5-mini")]
    model: String,

    /// Max news items to feed the model (to keep context bounded)
    #[arg(long, default_value_t = 10)]
    max_news: usize,
}

fn extract_output_text(resp: &serde_json::Value) -> Option<String> {
    // Common path you used before
    if let Some(s) = resp.pointer("/output/0/content/0/text").and_then(|v| v.as_str()) {
        return Some(s.to_string());
    }
    // Scan all content parts; accept either "text" or a structured "json"
    if let Some(items) = resp.get("output").and_then(|v| v.as_array()) {
        for item in items {
            if let Some(parts) = item.get("content").and_then(|v| v.as_array()) {
                for part in parts {
                    if let Some(s) = part.get("text").and_then(|v| v.as_str()) {
                        return Some(s.to_string());
                    }
                    if let Some(j) = part.get("json") {
                        return Some(j.to_string()); // stringify JSON object
                    }
                }
            }
        }
    }
    None
}

async fn post_json(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    body: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    use tokio::time::{sleep, Duration as TokioDur};

    let mut delay_ms = 500u64;

    // Read optional headers from .env (dotenvy already ran in main)
    let project = std::env::var("OPENAI_PROJECT").ok();           // e.g., proj_xxxxx
    let beta    = std::env::var("OPENAI_BETA").unwrap_or_else(|_| "use=responses".to_string());

    for attempt in 1..=5 {
        // Build request with required + optional headers
        let mut req = client.post(url)
            .bearer_auth(api_key)
            .header("OpenAI-Beta", beta.as_str())
            .json(body);

        if let Some(p) = project.as_ref() {
            req = req.header("OpenAI-Project", p);
        }

        match req.send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    return Ok(resp.json::<serde_json::Value>().await?);
                } else {
                    let status = resp.status();
                    let retry_after = resp
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|h| h.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok());
                    let text = resp.text().await.unwrap_or_default();

                    let retryable_status = status == reqwest::StatusCode::REQUEST_TIMEOUT
                        || status == reqwest::StatusCode::CONFLICT
                        || status == reqwest::StatusCode::TOO_EARLY
                        || status == reqwest::StatusCode::TOO_MANY_REQUESTS
                        || status.is_server_error();

                    if retryable_status && attempt < 5 {
                        let wait_ms = retry_after.map(|s| s * 1000).unwrap_or(delay_ms);
                        eprintln!(
                            "OpenAI {} error ({}). Attempt {}/5. Retrying in {} ms. Body: {}",
                            url, status, attempt, wait_ms, text
                        );
                        sleep(TokioDur::from_millis(wait_ms)).await;
                        delay_ms = (delay_ms * 2).min(8_000);
                        continue;
                    }

                    anyhow::bail!("OpenAI {} error: {} — {}", url, status, text);
                }
            }
            Err(e) => {
                let retryable = e.is_timeout() || e.is_connect() || e.is_request();
                if retryable && attempt < 5 {
                    eprintln!(
                        "Network error on attempt {}/5: {}. Retrying in {} ms…",
                        attempt, e, delay_ms
                    );
                    sleep(TokioDur::from_millis(delay_ms)).await;
                    delay_ms = (delay_ms * 2).min(8_000);
                    continue;
                }
                return Err(anyhow::anyhow!("Network error: {}", e));
            }
        }
    }

    anyhow::bail!("Exhausted retries calling {}", url);
}


fn db_path() -> String {
    env::var("BITVIA_DB").unwrap_or_else(|_| "./db/bitvia.db".to_string())
}
fn ensure_parent_dir(path: &str) -> Result<()> {
    if let Some(p) = Path::new(path).parent() {
        if !p.as_os_str().is_empty() {
            fs::create_dir_all(p)?;
        }
    }
    Ok(())
}
fn open_db() -> Result<Connection> {
    let path = db_path();
    ensure_parent_dir(&path)?;
    let conn = Connection::open(&path).with_context(|| format!("open sqlite at {path}"))?;
    conn.execute_batch(SCHEMA).ok();
    Ok(conn)
}

// ----------------------- Data access -----------------------

#[derive(Debug, Clone, Serialize)]
struct Metrics {
    metric_date: String,
    mempool_tx: Option<i64>,
    mempool_bytes: Option<i64>,
    avg_block_interval_sec: Option<f64>,
    median_fee_sat_per_vb: Option<f64>,
}

fn load_today_metrics(conn: &Connection) -> Result<Metrics> {
    let mut st = conn.prepare(
        "SELECT metric_date, mempool_tx, mempool_bytes, avg_block_interval_sec, median_fee_sat_per_vb
         FROM metrics
         ORDER BY metric_date DESC LIMIT 1",
    )?;
    let row = st.query_row([], |r| {
        Ok(Metrics {
            metric_date: r.get::<_, String>(0)?,
            mempool_tx: r.get::<_, Option<i64>>(1)?,
            mempool_bytes: r.get::<_, Option<i64>>(2)?,
            avg_block_interval_sec: r.get::<_, Option<f64>>(3)?,
            median_fee_sat_per_vb: r.get::<_, Option<f64>>(4)?,
        })
    })?;
    Ok(row)
}

#[derive(Debug, Clone, Serialize)]
struct NewsItem {
    id: Option<i64>, // if you have IDs; otherwise None
    title: String,
    outlet: String,
    url: String,
    published_at: Option<String>,
}

fn load_recent_news(conn: &Connection, limit: usize) -> Result<Vec<NewsItem>> {
    let mut st = conn.prepare(
        r#"
        SELECT id, title, outlet, url, published_at
        FROM news_sources
        WHERE fetched_at >= datetime('now','-1 day')
          AND (published_at IS NULL OR published_at >= datetime('now','-3 day'))
        ORDER BY COALESCE(published_at, fetched_at) DESC
        LIMIT ?1
        "#,
    )?;
    let rows = st
        .query_map([limit as i64], |r| {
            Ok(NewsItem {
                id: r.get(0).ok(),
                title: r.get(1)?,
                outlet: r.get(2)?,
                url: r.get(3)?,
                published_at: r.get(4).ok(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn upsert_digest(conn: &Connection, date: &str, title: &str, body_md: &str) -> Result<usize> {
    let n = conn.execute(
        r#"
        INSERT INTO news_digests (digest_date, title, body_md)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(digest_date) DO UPDATE SET
          title=excluded.title,
          body_md=excluded.body_md
        "#,
        params![date, title, body_md],
    )?;
    Ok(n)
}

// ----------------------- Model I/O types -----------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ClaimType {
    Metric,
    News,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Claim {
    #[serde(rename = "type")]
    ty: ClaimType,
    text: String,
    value: Option<serde_json::Value>,
    unit: Option<String>,
    source_id: Option<String>, // e.g., "metrics.mempool_tx" or a news row id as string
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Draft {
    facts_markdown: String,
    opinion_markdown: String,
    claims: Vec<Claim>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
struct VerifyResult {
    ok: bool,
    invalid_claim_indexes: Vec<usize>,
    reasons: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let args = Args::parse();

    let api_key = env::var("OPENAI_API_KEY").context("missing OPENAI_API_KEY (check your .env on Windows)")?;
    let conn = open_db()?;

    // 1) Gather inputs
    let metrics = load_today_metrics(&conn)
        .context("no metrics found — run bitvia-digest first")?;
    let news = load_recent_news(&conn, args.max_news).unwrap_or_default();

    fn is_bitcoin_relevant(title: &str, outlet: &str) -> bool {
        let t = title.to_lowercase();
        let o = outlet.to_lowercase();

        // Strong outlet allowlist (extend as you like)
        let outlet_ok = [
            "coindesk", "decrypt", "bitcoin optech", "blockstream",
            "glassnode", "mempool", "bitcoin magazine", "the mempool", "bitmex research",
        ].iter().any(|k| o.contains(k));

        // Keyword gate
        let kw = ["bitcoin", "btc", "lightning", "mempool", "hashrate", "miner", "ordinals", "taproot", "halving", "etf"];
        let kw_ok = kw.iter().any(|k| t.contains(k));

        outlet_ok && kw_ok || kw_ok
    }

    // Optional: cap how many from a single outlet so one feed can’t dominate
    use std::collections::HashMap;
    let mut per_outlet: HashMap<String, usize> = HashMap::new();
    let max_per_outlet = 3;

    let filtered_news: Vec<_> = news.into_iter()
        .filter(|n| is_bitcoin_relevant(&n.title, &n.outlet))
        .filter(|n| {
            let c = per_outlet.entry(n.outlet.to_lowercase()).or_insert(0);
            if *c >= max_per_outlet { return false; }
            *c += 1;
            true
        })
        .collect();

    // Use filtered set downstream
    let news = filtered_news;

    // 2) Build compact inputs
    let metrics_line = format!(
        "date={}; mempool_tx={:?}; mempool_bytes={:?}; avg_block_s={:?}; fee_sat_vb={:?}",
        metrics.metric_date,
        metrics.mempool_tx,
        metrics.mempool_bytes,
        metrics.avg_block_interval_sec,
        metrics.median_fee_sat_per_vb
    );
    let news_lines = news
        .iter()
        .map(|n| {
            let when = n
                .published_at
                .as_deref()
                .map(|d| format!(" — published {d}"))
                .unwrap_or_default();
            format!("- [{}] {} ({}){}", n.outlet, n.title, n.url, when)
        })
        .collect::<Vec<_>>()
        .join("\n");


    // 3) Compose primary prompt (structured output)
    let system = r#"You are Bitvia’s daily Bitcoin digest writer.

        OUTPUT (strict JSON fields):
        - facts_markdown: 200–500 words of sourced, factual reporting derived ONLY from today’s metrics and the provided news list.
        - opinion_markdown: up to 500 words of clearly labeled AI commentary on those facts. Must include: “This is AI-generated and not financial advice.”
        - claims[] covers ALL non-trivial numeric/dated statements appearing in facts_markdown only (NOT the opinion). For metrics use source_id='metrics.<field>'. For news use the exact URL from inputs. value is a simple string or null; unit may be null.
        - If a headline appears older than ~3 days by its visible date/wording, omit it or mark it as “previous coverage”; do not present it as today’s news.
        - Relevance gate: Only include news that is directly about Bitcoin or Bitcoin-adjacent core infra (Bitcoin Core, Lightning, mining, fees, ordinals, mempool, ETFs materially impacting BTC). Exclude general AI/tech/gaming items unless they explicitly reference Bitcoin’s network, economics, or regulation.
        - Freshness: If a headline appears older than ~3 days by its visible date/wording, omit it or mark as “previous coverage”—do not treat as today's news.
        - Links: Use proper Markdown links `[title](url)` for every cited item. Do not output bare URLs.
        - Top News each bullet must show outlet and (if provided) published date.
        - Word limits: Facts section ≤ 500 words. “AI Opinion” ≤ 500 words. Keep them in separate sections.
        - No investment advice or price predictions.

        STYLE & MARKDOWN
        - Tone: concise financial journalist—clear, approachable, no hype.
        - Use this section structure INSIDE facts_markdown:
        - **“### Key Metrics”** — short bullets with **bold** labels, e.g.:
            - **Mempool:** 7,058 tx (~2.3 MB)
            - **Avg. block interval:** 9.6 min
            - **Median fee:** 1 sat/vB
        - **“### Top News”** — 3–6 bullets. Each bullet ends with a Markdown link using ONLY the provided URL:
            - Brief headline summary … [Decrypt](https://example.com)
        - Use commas and light rounding for readability (≤1 decimal for seconds/minutes). No new numbers or links beyond inputs.
        - opinion_markdown: write like a short column—connect the dots, add context, avoid predictions. Include the non-advice line at the end.
        - Return JSON that matches the schema exactly: { facts_markdown, opinion_markdown, claims[] } (no extra keys)."#;


    let user = format!(r#"# Context
        Metrics (raw line):
        {metrics}

        News (last 24h, up to {maxn}):
        {news}

        # Task
        Produce:
        1) facts_markdown — 200–500 words with the exact Markdown layout:
        - Start with a 1–2 sentence summary paragraph.
        - Then add **“### Key Metrics”** bullets (bold labels).
        - Then **“### Top News”** bullets, each ending with a Markdown link to the provided URL.
        2) opinion_markdown — up to 500 words, labeled **AI Opinion**, conversational but professional. Connect metrics to headlines. No predictions. End with: “This is AI-generated and not financial advice.”
        3) claims[] for every non-trivial numeric/dated statement in facts_markdown only (NOT opinion). Use source_id='metrics.<field>' for metrics or the exact news URL.

        Return ONLY JSON matching the schema (no extra keys)."#,
            metrics = metrics_line,
            news = news_lines,
            maxn = args.max_news
        );


    let http = Client::builder()
        // Fail fast if TCP connect stalls
        .connect_timeout(StdDur::from_secs(15))
        // Overall request deadline (upload + server processing + download)
        .timeout(StdDur::from_secs(120))
        // Keep connections warm
        .tcp_keepalive(Some(StdDur::from_secs(30)))
        .build()?;

    // --- PASS 1: generate draft with structured outputs ---
    let body_generate = json!({
        "model": args.model,
        "input": [
            { "role": "system", "content": [{ "type": "input_text", "text": system }] },
            { "role": "user",   "content": [{ "type": "input_text", "text": user   }] }
        ],
        "text": {
            "format": {
            "type": "json_schema",
            "name": "BitviaDigestV2",
            "schema": {
                "type": "object",
                "properties": {
                "facts_markdown":   { "type": "string" },
                "opinion_markdown": { "type": "string" },
                "claims": {
                    "type": "array",
                    "items": {
                    "type": "object",
                    "properties": {
                        "type":      { "type": "string", "enum": ["metric", "news"] },
                        "text":      { "type": "string" },
                        "value":     { "type": ["string", "null"] },
                        "unit":      { "type": ["string", "null"] },
                        "source_id": { "type": "string" }
                    },
                    // API wants every property listed here:
                    "required": ["type", "text", "value", "unit", "source_id"],
                    "additionalProperties": false
                    }
                }
                },
                "required": ["facts_markdown", "opinion_markdown", "claims"],
                "additionalProperties": false
            },
            "strict": true
            }
        }
    });


    let resp1 = post_json(&http, "https://api.openai.com/v1/responses", &api_key, &body_generate)
        .await
        .context("OpenAI call (generate) failed")?;

    let draft_json = match extract_output_text(&resp1) {
        Some(s) => s,
        None => {
            eprintln!("FULL OpenAI (generate) response:\n{:#}", resp1);
            anyhow::bail!("empty model response (generate)")
        }
    };

    let draft: Draft = serde_json::from_str(&draft_json)
        .context("structured output didn't match schema")?;

    // --- PASS 2: verify claims against our raw inputs ---
    let verifier_system = r#"You are a strict fact verifier.

        Verify ONLY the claims[] against the provided raw inputs:
        - For metric claims: source_id MUST be 'metrics.<field>' and the text/value MUST match the raw value (allow commas and rounding to at most 1 decimal).
        - For news claims: source_id MUST equal one of the provided news URLs; the text MUST reflect that headline/description without adding new facts.
        - Ignore the opinion_markdown entirely (no verification required for it).
        - If any claim cannot be verified, mark it invalid.

        Return JSON strictly matching the VerifyResult schema."#;
    let verify_input = json!({
        "claims": draft.claims,
        "metrics_raw": metrics, // the exact row we loaded
        "news_raw": news,       // the exact list we loaded
    });

    let body_verify = json!({
        "model": args.model,
        "input": [
            { "role":"system", "content":[{ "type":"input_text", "text": verifier_system }] },
            { "role":"user",   "content":[{ "type":"input_text", "text": serde_json::to_string(&verify_input).unwrap() }] }
        ],
        "text": {
            "format": {
            "type":"json_schema",
            "name":"VerifyResult",
            "schema": {
                "type":"object",
                "properties":{
                "ok":{"type":"boolean"},
                "invalid_claim_indexes":{"type":"array","items":{"type":"integer"}},
                "reasons":{"type":"array","items":{"type":"string"}}
                },
                "required":["ok","invalid_claim_indexes","reasons"],
                "additionalProperties":false
            },
            "strict": true
            }
        }
    });

    let resp2 = post_json(&http, "https://api.openai.com/v1/responses", &api_key, &body_verify)
        .await
        .context("OpenAI call (verify) failed")?;

    let verify_json = match extract_output_text(&resp2) {
        Some(s) => s,
        None => {
            eprintln!("FULL OpenAI (verify) response:\n{:#}", resp2);
            anyhow::bail!("empty model response (verify)")
        }
    };

    let verify: VerifyResult = serde_json::from_str(&verify_json)
        .context("verify output didn't match schema")?;

    let mut final_md = format!(
        "# Bitvia Daily — {date}\n\n## Network & News (Facts)\n\n{facts}\n\n## AI Opinion\n\n{opinion}",
        date = metrics.metric_date,
        facts = draft.facts_markdown,
        opinion = draft.opinion_markdown
    );

    if !verify.ok && !verify.invalid_claim_indexes.is_empty() {
        // Ask the model to rewrite ONLY the factual section.
        let pruner_system = "Rewrite the factual section by removing or correcting ONLY the invalid claims. Do not introduce any new facts, URLs, or numbers. Maintain the same structure and length as before.";
        let pruner_user = json!({
            "facts_markdown": draft.facts_markdown,
            "opinion_markdown": draft.opinion_markdown, // keep unchanged
            "invalid_claim_indexes": verify.invalid_claim_indexes,
            "reasons": verify.reasons,
            "claims": draft.claims,
            "rule": "Return ONLY the corrected facts_markdown (plain markdown text)."
        });

        let body_prune = json!({
            "model": args.model,
            "input": [
                { "role":"system", "content":[{ "type":"input_text", "text": pruner_system }] },
                { "role":"user",   "content":[{ "type":"input_text", "text": pruner_user.to_string() }] }
            ]
        });

        let resp3 = post_json(&http, "https://api.openai.com/v1/responses", &api_key, &body_prune)
            .await
            .context("OpenAI call (prune) failed")?;

        // Extract the corrected facts section (plain markdown)
        let pruned_facts = match extract_output_text(&resp3) {
            Some(s) => s,
            None => {
                eprintln!("FULL OpenAI (prune) response:\n{:#}", resp3);
                String::new()
            }
        };

        if pruned_facts.is_empty() {
            eprintln!("WARN: prune produced empty text; keeping original facts section");
        } else {
            // Rebuild the final combined markdown:
            final_md = format!(
                "# Bitvia Daily — {date}\n\n## Network & News (Facts)\n\n{facts}\n\n## AI Opinion\n\n{opinion}",
                date = metrics.metric_date,
                facts = pruned_facts,
                opinion = draft.opinion_markdown
            );
        }
    }


    // Minimal programmatic checks (URLs must be from provided news; length cap)
    {
        use regex::Regex;

        // Build a whitelist of allowed URLs (normalize a bit)
        fn normalize(u: &str) -> String {
            // Strip tracking params (utm_*, ref, etc.) and fragments for equality checks
            let mut s = u.to_string();
            if let Ok(mut parsed) = url::Url::parse(u) {
                parsed.set_fragment(None);
                // Remove common tracking params
                let mut qp: Vec<(String, String)> = parsed.query_pairs()
                    .filter(|(k, _)| {
                        let k = k.to_string().to_lowercase();
                        !(k.starts_with("utm_") || k == "ref" || k == "source")
                    })
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                qp.sort();
                if qp.is_empty() { parsed.set_query(None); }
                else {
                    let q = qp.iter().map(|(k,v)| format!("{}={}", k, v)).collect::<Vec<_>>().join("&");
                    parsed.set_query(Some(&q));
                }
                s = parsed.to_string();
            }
            s
        }

        let allowed: std::collections::HashSet<String> = news
            .iter()
            .map(|n| normalize(&n.url))
            .collect();

        // Find ALL urls in the markdown: (markdown links), bare links, and bracketed links.
        let re_any_url = Regex::new(r#"https?://[^\s\)\]]+"#).unwrap();

        for m in re_any_url.find_iter(&final_md) {
            let u = normalize(m.as_str());
            if !allowed.contains(&u) {
                anyhow::bail!("Output contains URL not present in inputs: {}", m.as_str());
            }
        }

        if final_md.len() > 8000 {
            anyhow::bail!("Output too large; rejecting.");
        }
    }


    // Store or print
    let date = &metrics.metric_date;
    if args.dry_run {
        println!("=== DRAFT (final) for {date} ===\n{}\n", final_md);
        println!("(dry-run) not writing to DB.");
    } else {
        upsert_digest(&conn, date, "Bitvia Daily Bitcoin Digest", &final_md)?;
        println!("OK: stored digest for {date}");
    }

    Ok(())
}
