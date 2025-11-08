use anyhow::{Context, Result};
use axum::{routing::get, Router};
use dotenvy::dotenv;
use std::{net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;
use tower_http::services::ServeDir;

mod state;
mod rpc;
mod models;
mod supply;
mod utils;
mod handlers;

use state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    let rpc_url = std::env::var("RPC_URL").context("missing RPC_URL")?;
    let rpc_user = std::env::var("RPC_USER").context("missing RPC_USER")?;
    let rpc_pass = std::env::var("RPC_PASS").context("missing RPC_PASS")?;
    let bind_addr: SocketAddr = std::env::var("BIND_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8000".to_string())
        .parse()
        .context("BIND_ADDR must be host:port")?;

    let state = Arc::new(AppState::new(rpc_url, rpc_user, rpc_pass));

    let app = Router::new()
        // pages
        .route("/", get(handlers::pages::index))
        .route("/health", get(handlers::pages::health))
        // api
        .route("/api/mempoolinfo", get(handlers::mempool::mempoolinfo))
        .route("/api/network", get(handlers::network::network_summary))
        .route("/api/blockhash/{height}", get(handlers::blocks::blockhash_by_height))
        .route("/api/block/{hash}", get(handlers::blocks::block_by_hash))
        .route("/api/tx/{txid}", get(handlers::tx::tx_by_id))
        // static
        .nest_service("/static", ServeDir::new("static"))
        // shared state
        .with_state(state);

    println!("listening on http://{bind_addr}");
    let listener = TcpListener::bind(bind_addr).await?;
    axum::serve(listener, app).await.context("server crashed")
}

// use axum::{
//     extract::{Path, State},
//     response::Html,
//     http::StatusCode,
//     routing::get,
//     Json, Router,
// };
// use tokio::net::TcpListener;
// use anyhow::{Context, Result};
// use dotenvy::dotenv;
// use reqwest::Client;
// use serde::{Deserialize, Serialize};
// use std::{net::SocketAddr, sync::Arc};
// use tower_http::services::ServeDir;
// use axum::extract::Query;

// #[derive(Clone)]
// struct AppState {
//     http: Client,
//     rpc_url: String,
//     rpc_user: String,
//     rpc_pass: String,
// }

// #[derive(Deserialize, Serialize)]
// struct BlockHeaderLite {
//     time: u64,        // UNIX seconds
//     height: Option<u64>,
// }

// #[derive(Deserialize, Serialize)]
// struct NetworkSummary {
//     // Core metrics
//     height: u64,
//     difficulty: f64,
//     hashrate_ghps: f64,     // from getnetworkhashps (converted to GH/s for readability)
//     avg_block_interval_sec: f64,

//     // Difficulty adjustment estimate
//     blocks_into_epoch: u64,
//     blocks_to_next_adjust: u64,
//     est_diff_change_pct: f64, // rough projection for this epoch so far

//     // Supply & issuance
//     current_subsidy_btc: f64,
//     est_new_btc_per_day: f64,
//     est_circulating_btc: f64,

//     // Timestamps
//     tip_time: u64,
// }

// #[derive(Deserialize)]
// struct GetBlockV2 {
//     hash: String,
//     height: u64,
//     time: u64,
//     mediantime: Option<u64>,
//     size: u64,
//     weight: Option<u64>,
//     nTx: u64,
//     prevblockhash: Option<String>,
//     nextblockhash: Option<String>,
//     tx: Vec<serde_json::Value>, // tx objects (verbosity=2), we’ll turn into ids
// }

// #[derive(Serialize)]
// struct BlockView {
//     hash: String,
//     height: u64,
//     time: u64,
//     mediantime: Option<u64>,
//     size: u64,
//     weight: Option<u64>,
//     n_tx: u64,
//     prev: Option<String>,
//     next: Option<String>,
//     txids: Vec<String>,      // first N txids for quick rendering
//     more_tx: bool,           // true if there are more than N
// }

// #[derive(Deserialize, Serialize, Clone)]
// struct TxDecoded {
//     txid: String,
//     hash: Option<String>,
//     size: Option<u64>,
//     vsize: Option<u64>,
//     weight: Option<u64>,
//     version: Option<i64>,
//     locktime: Option<u64>,
//     vin: Vec<serde_json::Value>,
//     vout: Vec<serde_json::Value>,
//     hex: Option<String>,
//     time: Option<u64>,
//     blocktime: Option<u64>,
//     confirmations: Option<u64>,
//     blockhash: Option<String>,
// }

// #[derive(Serialize)]
// struct PrevoutResolved {
//     txid: String,
//     vout: u32,
//     value_btc: f64,
//     address: String,
// }

// #[derive(Serialize)]
// struct TxView {
//     // passthrough
//     txid: String,
//     size: Option<u64>,
//     vsize: Option<u64>,
//     weight: Option<u64>,
//     confirmations: Option<u64>,
//     blockhash: Option<String>,
//     is_coinbase: bool,

//     // resolved inputs (capped)
//     inputs_resolved: Vec<PrevoutResolved>,
//     inputs_total_btc: Option<f64>,
//     outputs_total_btc: f64,
//     fee_btc: Option<f64>,
//     feerate_sat_vb: Option<f64>,
//     // paging-ish flags
//     total_inputs: usize,
//     resolved_inputs: usize,
//     more_inputs: bool,

//     // keep raw outputs to render without extra parsing client-side
//     vout: Vec<serde_json::Value>,
// }

// #[derive(Deserialize)]
// struct ResolveQ {
//     resolve: Option<usize>, // how many inputs to resolve (cap enforced)
// }

// #[tokio::main]
// async fn main() -> Result<()> {
//     dotenv().ok();

//     let rpc_url = std::env::var("RPC_URL").context("missing RPC_URL")?;
//     let rpc_user = std::env::var("RPC_USER").context("missing RPC_USER")?;
//     let rpc_pass = std::env::var("RPC_PASS").context("missing RPC_PASS")?;
//     let bind_addr: SocketAddr = std::env::var("BIND_ADDR")
//         .unwrap_or_else(|_| "0.0.0.0:8000".to_string())
//         .parse()
//         .context("BIND_ADDR must be host:port")?;

//     let state = Arc::new(AppState {
//         http: Client::new(),
//         rpc_url,
//         rpc_user,
//         rpc_pass,
//     });

//     let app = Router::new()
//         .route("/", get(index))
//         .route("/health", get(health))
//         .route("/api/mempoolinfo", get(mempoolinfo))
//         .route("/api/blockhash/{height}", get(blockhash_by_height))
//         .route("/api/block/{hash}", get(block_by_hash))
//         .route("/api/tx/{txid}", get(tx_by_id)) 
//         .route("/api/network", get(network_summary))
//         .nest_service("/static", ServeDir::new("static"))
//         .with_state(state);

//     println!("listening on http://{bind_addr}");
//     let listener = TcpListener::bind(bind_addr).await?;
//     axum::serve(listener, app).await.context("server crashed")
// }

// async fn health() -> &'static str {
//     "ok"
// }

// #[derive(Serialize)]
// struct RpcRequest<'a> {
//     jsonrpc: &'static str,
//     id: &'static str,
//     method: &'a str,
//     params: serde_json::Value,
// }

// #[derive(Deserialize)]
// struct RpcResponse<T> {
//     result: Option<T>,
//     error: Option<RpcError>,
//     id: serde_json::Value,
// }

// #[derive(Deserialize, Debug)]
// struct RpcError {
//     code: i64,
//     message: String,
// }

// async fn rpc_call<T: for<'de> Deserialize<'de>>(
//     st: &AppState,
//     method: &str,
//     params: serde_json::Value,
// ) -> Result<T> {
//     let req = RpcRequest {
//         jsonrpc: "1.0",
//         id: "axum",
//         method,
//         params,
//     };

//     let res = st.http
//         .post(&st.rpc_url)
//         .basic_auth(&st.rpc_user, Some(&st.rpc_pass))
//         .json(&req)
//         .send()
//         .await
//         .context("rpc http send failed")?;

//     let status = res.status();
//     let body = res
//         .json::<RpcResponse<T>>()
//         .await
//         .with_context(|| format!("rpc parse failed (status {status})"))?;

//     if let Some(err) = body.error {
//         return Err(anyhow::anyhow!("rpc error {}: {}", err.code, err.message));
//     }
//     let result = body
//         .result
//         .ok_or_else(|| anyhow::anyhow!("rpc response missing result"))?;
//     Ok(result)
// }

// async fn rpc_batch_getraw<T: for<'de> Deserialize<'de>>(
//     st: &AppState,
//     method: &str,
//     params_list: Vec<serde_json::Value>,
// ) -> Result<Vec<T>> {
//     // Build a JSON-RPC batch array
//     let batch: Vec<RpcRequest> = params_list
//         .into_iter()
//         .enumerate()
//         .map(|(i, params)| RpcRequest {
//             jsonrpc: "1.0",
//             id: Box::leak(format!("b{}", i).into_boxed_str()), // stable &'static str for serde
//             method,
//             params,
//         })
//         .collect();

//     let res = st.http
//         .post(&st.rpc_url)
//         .basic_auth(&st.rpc_user, Some(&st.rpc_pass))
//         .json(&batch)
//         .send()
//         .await
//         .context("rpc batch http send failed")?;

//     let status = res.status();
//     let body = res
//         .json::<Vec<RpcResponse<T>>>()
//         .await
//         .with_context(|| format!("rpc batch parse failed (status {status})"))?;

//     // Extract results or error
//     let mut out = Vec::with_capacity(body.len());
//     for item in body {
//         if let Some(err) = item.error {
//             return Err(anyhow::anyhow!("rpc batch item error {}: {}", err.code, err.message));
//         }
//         out.push(item.result.ok_or_else(|| anyhow::anyhow!("rpc batch item missing result"))?);
//     }
//     Ok(out)
// }

// fn vout_value_btc(v: &serde_json::Value) -> f64 {
//     v.get("value").and_then(|x| x.as_f64()).unwrap_or(0.0)
// }
// fn vout_address(v: &serde_json::Value) -> String {
//     let spk = v.get("scriptPubKey").cloned().unwrap_or(serde_json::json!({}));
//     if let Some(addr) = spk.get("address").and_then(|a| a.as_str()) {
//         return addr.to_string();
//     }
//     if let Some(arr) = spk.get("addresses").and_then(|a| a.as_array()) {
//         if let Some(first) = arr.first().and_then(|x| x.as_str()) {
//             return first.to_string();
//         }
//     }
//     "(no address)".to_string()
// }

// // --- Typed result for getblockchaininfo (subset we care about) ---
// #[derive(Deserialize, Serialize)]
// struct ChainInfo {
//     blocks: u64,
//     difficulty: f64,
// }

// #[derive(Deserialize, Serialize)]
// struct MempoolInfo {
//     size: u64,
//     bytes: u64,
//     usage: u64,
//     #[serde(default)] fullrbf: bool,
//     #[serde(default)] unbroadcastcount: u64,
//     #[serde(default)] mempoolminfee: f64,
// }

// /// Compute theoretical BTC mined up to the given height (excludes genesis subsidy).
// /// Assumes halving every 210_000 blocks, initial subsidy 50 BTC.
// fn mined_supply_btc(height: u64) -> f64 {
//     let mut remaining = height; // exclude genesis at height 0
//     let mut subsidy_sats: u64 = 50_0000_0000; // 50 BTC in satoshis
//     let mut total_sats: u128 = 0;

//     for _epoch in 0..64 {
//         if remaining == 0 || subsidy_sats == 0 { break; }
//         let blocks_in_epoch = remaining.min(210_000);
//         total_sats += (blocks_in_epoch as u128) * (subsidy_sats as u128);
//         remaining -= blocks_in_epoch;
//         subsidy_sats >>= 1; // halve
//     }
//     (total_sats as f64) / 100_000_000.0
// }

// /// Current block subsidy in BTC for a given height.
// fn current_subsidy_btc(height: u64) -> f64 {
//     let halvings = (height / 210_000) as u32;
//     let sats: u64 = if halvings >= 64 { 0 } else { 50_0000_0000 >> halvings };
//     (sats as f64) / 100_000_000.0
// }

// /// ROUTE HANDLERS ///

// async fn index() -> Html<String> {
//     let html = include_str!("../templates/index.html");
//     Html(html.to_string())
// }

// async fn mempoolinfo(State(st): State<Arc<AppState>>,) -> Result<Json<MempoolInfo>, (StatusCode, String)> {
//     let params = serde_json::json!([]);
//     rpc_call::<MempoolInfo>(&st, "getmempoolinfo", params)
//         .await
//         .map(Json)
//         .map_err(internalize)
// }

// async fn network_summary(
//     State(st): State<Arc<AppState>>
// ) -> Result<Json<NetworkSummary>, (StatusCode, String)> {
//     // 1) Basic chain info (height + difficulty)
//     let ci: ChainInfo = rpc_call(&st, "getblockchaininfo", serde_json::json!([]))
//         .await
//         .map_err(internalize)?;
//     let height = ci.blocks;
//     let difficulty = ci.difficulty;

//     // 2) Tip header
//     let tip_hash: String = rpc_call(&st, "getblockhash", serde_json::json!([height]))
//         .await
//         .map_err(internalize)?;
//     let tip_hdr: BlockHeaderLite = rpc_call(&st, "getblockheader", serde_json::json!([tip_hash, true]))
//         .await
//         .map_err(internalize)?;

//     // 3) Epoch-based timing (since start of current 2016-block window)
//     let epoch_len: u64 = 2016;
//     let blocks_into_epoch: u64 = height % epoch_len;
//     let blocks_to_next_adjust: u64 = epoch_len - blocks_into_epoch;

//     // Guard the "boundary" case cleanly: if we're exactly at an epoch boundary,
//     // we can't form an average for the *new* epoch yet. Show target defaults.
//     let (avg_block_interval_sec, est_diff_change_pct) = if blocks_into_epoch == 0 {
//         // Just started a new epoch: no data yet
//         (600.0, 0.0)
//     } else {
//         let start_h: u64 = height.saturating_sub(blocks_into_epoch);
//         let start_hash: String = rpc_call(&st, "getblockhash", serde_json::json!([start_h]))
//             .await
//             .map_err(internalize)?;
//         let start_hdr: BlockHeaderLite = rpc_call(&st, "getblockheader", serde_json::json!([start_hash, true]))
//             .await
//             .map_err(internalize)?;

//         // Average block interval across the epoch so far
//         let blocks_so_far = blocks_into_epoch as f64; // safe (>=1 here)
//         let dt = (tip_hdr.time.saturating_sub(start_hdr.time)) as f64;
//         let avg_since_epoch = if dt > 0.0 { dt / blocks_so_far } else { 600.0 };

//         // Projected diff change relative to 600s target.
//         // ratio > 1 => faster than target => difficulty likely increases.
//         let ratio = 600.0 / avg_since_epoch;
//         let est_pct = ((ratio - 1.0) * 100.0).clamp(-50.0, 50.0); // keep UI sane

//         (avg_since_epoch, est_pct)
//     };

//     // 4) Hashrate (GH/s) from node (returns H/s)
//     let nhps_hps: f64 = rpc_call(&st, "getnetworkhashps", serde_json::json!([]))
//         .await
//         .map_err(internalize)?;
//     let hashrate_ghps = nhps_hps / 1e9;

//     // 5) Supply & issuance
//     let curr_subsidy = current_subsidy_btc(height);
//     let est_new_btc_per_day = curr_subsidy * 144.0;
//     let est_circulating_btc = mined_supply_btc(height);

//     let out = NetworkSummary {
//         height,
//         difficulty,
//         hashrate_ghps,
//         avg_block_interval_sec,
//         blocks_into_epoch,
//         blocks_to_next_adjust,
//         est_diff_change_pct,
//         current_subsidy_btc: curr_subsidy,
//         est_new_btc_per_day,
//         est_circulating_btc,
//         tip_time: tip_hdr.time,
//     };

//     Ok(Json(out))
// }

// #[derive(Serialize)] struct BlockHashResp {
//     height: u64,
//     hash: String,
// }

// async fn blockhash_by_height(
//     State(st): State<Arc<AppState>>,
//     Path(height): Path<u64>,
// ) -> Result<Json<BlockHashResp>, (StatusCode, String)> {
//     // getblockhash height -> "0000..."
//     let params = serde_json::json!([height]);
//     let hash: String = rpc_call(&st, "getblockhash", params)
//         .await.map_err(internalize)?;
//     Ok(Json(BlockHashResp { height, hash }))
// }

// async fn block_by_hash(
//     State(st): State<Arc<AppState>>,
//     Path(hash): Path<String>,
// ) -> Result<Json<BlockView>, (StatusCode, String)> {
//     // Ask Core for verbosity=2 block
//     let gb: GetBlockV2 = rpc_call(&st, "getblock", serde_json::json!([hash, 2]))
//         .await
//         .map_err(internalize)?;

//     // Extract txids from objects (verbosity=2 returns objects with `txid`)
//     let mut txids = Vec::with_capacity(gb.tx.len());
//     for t in &gb.tx {
//         if let Some(id) = t.get("txid").and_then(|v| v.as_str()) {
//             txids.push(id.to_string());
//         }
//     }

//     // Trim for UI (first N)
//     let show = 12usize;
//     let more = txids.len() > show;
//     txids.truncate(show);

//     let out = BlockView {
//         hash: gb.hash,
//         height: gb.height,
//         time: gb.time,
//         mediantime: gb.mediantime,
//         size: gb.size,
//         weight: gb.weight,
//         n_tx: gb.nTx,
//         prev: gb.prevblockhash,
//         next: gb.nextblockhash,
//         txids,
//         more_tx: more,
//     };
//     Ok(Json(out))
// }

// fn tx_is_coinbase(tx: &TxDecoded) -> bool {
//     if tx.vin.is_empty() { return false; }
//     tx.vin[0].get("coinbase").is_some()
// }

// async fn tx_by_id(
//     State(st): State<Arc<AppState>>,
//     Path(txid): Path<String>,
//     Query(q): Query<ResolveQ>,
// ) -> Result<Json<TxView>, (StatusCode, String)> {
//     // 1) Fetch main tx
//     let tx: TxDecoded = rpc_call(&st, "getrawtransaction", serde_json::json!([txid, true]))
//         .await
//         .map_err(|e| {
//             let msg = e.to_string();
//             if msg.to_lowercase().contains("no such mempool or blockchain transaction") {
//                 (StatusCode::NOT_FOUND, format!("tx not found: {msg}"))
//             } else {
//                 internalize(msg)
//             }
//         })?;

//     // 2) Compute outputs_total
//     let outputs_total_btc: f64 = tx.vout.iter().map(vout_value_btc).sum();

//     // 3) How many inputs to resolve?
//     let total_inputs = tx.vin.len();
//     let cap_max = 100usize;               // hard ceiling
//     let mut resolve_n = q.resolve.unwrap_or(20);
//     if resolve_n > cap_max { resolve_n = cap_max; }
//     if resolve_n > total_inputs { resolve_n = total_inputs; }

//     // 4) Gather prevouts to resolve
//     let mut prev_params = Vec::with_capacity(resolve_n);
//     let mut in_map: Vec<(String, u32)> = Vec::with_capacity(resolve_n);

//     for vin in tx.vin.iter().take(resolve_n) {
//         if let (Some(prev_txid), Some(vout_idx)) = (
//             vin.get("txid").and_then(|x| x.as_str()),
//             vin.get("vout").and_then(|x| x.as_u64()).map(|x| x as u32),
//         ) {
//             prev_params.push(serde_json::json!([prev_txid, true]));
//             in_map.push((prev_txid.to_string(), vout_idx));
//         }
//     }

//     // 5) Batch fetch previous transactions (if any)
//     let mut inputs_resolved = Vec::<PrevoutResolved>::new();
//     let mut inputs_total_btc = None::<f64>;

//     if !prev_params.is_empty() {
//         let prev_txs: Vec<TxDecoded> =
//             rpc_batch_getraw(&st, "getrawtransaction", prev_params)
//                 .await
//                 .map_err(internalize)?;

//         // Map txid -> tx for quick lookup
//         use std::collections::HashMap;
//         let mut by_id: HashMap<&str, &TxDecoded> = HashMap::with_capacity(prev_txs.len());
//         for t in &prev_txs {
//             by_id.insert(t.txid.as_str(), t);
//         }

//         let mut sum_inputs = 0.0_f64;
//         for (prev_txid, vout_idx) in in_map {
//             if let Some(prev) = by_id.get(prev_txid.as_str()) {
//                 if let Some(vout_val) = prev.vout.get(vout_idx as usize) {
//                     let val = vout_value_btc(vout_val);
//                     let addr = vout_address(vout_val);
//                     inputs_resolved.push(PrevoutResolved {
//                         txid: prev_txid.clone(),
//                         vout: vout_idx,
//                         value_btc: val,
//                         address: addr,
//                     });
//                     sum_inputs += val;
//                 }
//             }
//         }
//         inputs_total_btc = Some(sum_inputs);
//     }

//     // 6) Fee + feerate (if we have enough info)
//     let fee_btc = inputs_total_btc.map(|ins| (ins - outputs_total_btc).max(0.0));
//     let feerate_sat_vb = match (fee_btc, tx.vsize) {
//         (Some(fee_btc), Some(vsize)) if vsize > 0 => {
//             let fee_sats = fee_btc * 100_000_000.0;
//             Some(fee_sats / (vsize as f64))
//         }
//         _ => None,
//     };

//     let is_cb = tx_is_coinbase(&tx);

//     let view = TxView {
//         txid: tx.txid,
//         size: tx.size,
//         vsize: tx.vsize,
//         weight: tx.weight,
//         confirmations: tx.confirmations,
//         blockhash: tx.blockhash,
//         is_coinbase: is_cb,

//         inputs_resolved,
//         inputs_total_btc,
//         outputs_total_btc,
//         fee_btc,
//         feerate_sat_vb,

//         total_inputs,
//         resolved_inputs: resolve_n,
//         more_inputs: total_inputs > resolve_n,

//         vout: tx.vout,
//     };

//     Ok(Json(view))
// }

// // ---------- Error helper ----------
// fn internalize<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
//     (StatusCode::BAD_GATEWAY, format!("RPC failed: {e}"))
// }
