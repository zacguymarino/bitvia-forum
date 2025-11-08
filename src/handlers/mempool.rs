use std::sync::Arc;

use axum::{extract::State, Json};
use crate::{models::MempoolInfo, rpc::rpc_call, state::AppState, utils::internalize};

pub async fn mempoolinfo(
    State(st): State<Arc<AppState>>,
) -> Result<Json<MempoolInfo>, (axum::http::StatusCode, String)> {
    let params = serde_json::json!([]);
    rpc_call::<MempoolInfo>(&st, "getmempoolinfo", params)
        .await
        .map(Json)
        .map_err(internalize)
}
