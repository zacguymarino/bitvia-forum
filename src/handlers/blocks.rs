use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde_json::json;

use crate::{
    models::{BlockHashResp, BlockPageQ, BlockView, GetBlockV1},
    rpc::rpc_call,
    state::AppState,
    utils::internalize,
};

pub async fn blockhash_by_height(
    State(st): State<Arc<AppState>>,
    Path(height): Path<u64>,
) -> Result<Json<BlockHashResp>, (StatusCode, String)> {
    let hash: String = rpc_call(&st, "getblockhash", json!([height]))
        .await
        .map_err(internalize)?;
    Ok(Json(BlockHashResp { height, hash }))
}

pub async fn block_by_hash(
    State(st): State<Arc<AppState>>,
    Path(hash): Path<String>,
    Query(q): Query<BlockPageQ>,
) -> Result<Json<BlockView>, (StatusCode, String)> {
    // v=1 → returns txids (strings), not full tx objects
    let gb: GetBlockV1 = rpc_call(&st, "getblock", json!([hash, 1]))
        .await
        .map_err(internalize)?;

    // txids already strings
    let all: Vec<String> = gb.tx; // already the txids

    // paging
    let total = all.len();
    let limit = q.limit.unwrap_or(20).clamp(1, 200);
    let offset = q.offset.unwrap_or(0).min(total);
    let end = (offset + limit).min(total);
    let txids = if offset < end { all[offset..end].to_vec() } else { Vec::new() };

    let out = BlockView {
        hash: gb.hash,
        height: gb.height,
        time: gb.time,
        mediantime: gb.mediantime,
        size: gb.size,
        weight: gb.weight,
        n_tx: gb.n_tx,
        prev: gb.prevblockhash,
        next: gb.nextblockhash,
        txids,
        more_tx: end < total,
        total_tx: total,
        offset,
        limit,
    };
    Ok(Json(out))
}

