use std::sync::Arc;

use axum::{extract::{Path, State}, Json};
use crate::{
    models::{BlockHashResp, BlockView, GetBlockV2},
    rpc::rpc_call,
    state::AppState,
    utils::internalize,
};

pub async fn blockhash_by_height(
    State(st): State<Arc<AppState>>,
    Path(height): Path<u64>,
) -> Result<Json<BlockHashResp>, (axum::http::StatusCode, String)> {
    let hash: String = rpc_call(&st, "getblockhash", serde_json::json!([height]))
        .await
        .map_err(internalize)?;
    Ok(Json(BlockHashResp { height, hash }))
}

pub async fn block_by_hash(
    State(st): State<Arc<AppState>>,
    Path(hash): Path<String>,
) -> Result<Json<BlockView>, (axum::http::StatusCode, String)> {
    let gb: GetBlockV2 = rpc_call(&st, "getblock", serde_json::json!([hash, 2]))
        .await
        .map_err(internalize)?;

    let mut txids = Vec::with_capacity(gb.tx.len());
    for t in &gb.tx {
        if let Some(id) = t.get("txid").and_then(|v| v.as_str()) {
            txids.push(id.to_string());
        }
    }

    let show = 12usize;
    let more = txids.len() > show;
    txids.truncate(show);

    Ok(Json(BlockView {
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
        more_tx: more,
    }))
}
