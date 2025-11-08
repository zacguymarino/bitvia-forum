use std::sync::Arc;

use axum::{extract::State, Json};
use crate::{
    models::{BlockHeaderLite, ChainInfo, NetworkSummary},
    rpc::rpc_call,
    state::AppState,
    supply::{current_subsidy_btc, mined_supply_btc},
    utils::internalize,
};

pub async fn network_summary(
    State(st): State<Arc<AppState>>,
) -> Result<Json<NetworkSummary>, (axum::http::StatusCode, String)> {
    // 1) height + difficulty
    let ci: ChainInfo = rpc_call(&st, "getblockchaininfo", serde_json::json!([]))
        .await
        .map_err(internalize)?;
    let height = ci.blocks;
    let difficulty = ci.difficulty;

    // 2) tip header
    let tip_hash: String = rpc_call(&st, "getblockhash", serde_json::json!([height]))
        .await
        .map_err(internalize)?;
    let tip_hdr: BlockHeaderLite = rpc_call(&st, "getblockheader", serde_json::json!([tip_hash, true]))
        .await
        .map_err(internalize)?;

    // 3) epoch stats
    let epoch_len: u64 = 2016;
    let blocks_into_epoch: u64 = height % epoch_len;
    let blocks_to_next_adjust: u64 = epoch_len - blocks_into_epoch;

    let (avg_block_interval_sec, est_diff_change_pct) = if blocks_into_epoch == 0 {
        (600.0, 0.0)
    } else {
        let start_h: u64 = height.saturating_sub(blocks_into_epoch);
        let start_hash: String = rpc_call(&st, "getblockhash", serde_json::json!([start_h]))
            .await
            .map_err(internalize)?;
        let start_hdr: BlockHeaderLite = rpc_call(&st, "getblockheader", serde_json::json!([start_hash, true]))
            .await
            .map_err(internalize)?;

        let blocks_so_far = blocks_into_epoch as f64;
        let dt = (tip_hdr.time.saturating_sub(start_hdr.time)) as f64;
        let avg_since_epoch = if dt > 0.0 { dt / blocks_so_far } else { 600.0 };

        let ratio = 600.0 / avg_since_epoch;
        let est_pct = ((ratio - 1.0) * 100.0).clamp(-50.0, 50.0);

        (avg_since_epoch, est_pct)
    };

    // 4) network hashrate (H/s -> GH/s)
    let nhps_hps: f64 = rpc_call(&st, "getnetworkhashps", serde_json::json!([]))
        .await
        .map_err(internalize)?;
    let hashrate_ghps = nhps_hps / 1e9;

    // 5) supply
    let curr_subsidy = current_subsidy_btc(height);
    let est_new_btc_per_day = curr_subsidy * 144.0;
    let est_circulating_btc = mined_supply_btc(height);

    Ok(Json(NetworkSummary {
        height,
        difficulty,
        hashrate_ghps,
        avg_block_interval_sec,
        blocks_into_epoch,
        blocks_to_next_adjust,
        est_diff_change_pct,
        current_subsidy_btc: curr_subsidy,
        est_new_btc_per_day,
        est_circulating_btc,
        tip_time: tip_hdr.time,
    }))
}
