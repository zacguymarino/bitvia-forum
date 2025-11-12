use anyhow::Context;
use reqwest::StatusCode as HttpStatus;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::state::AppState;

#[derive(Serialize)]
struct RpcRequestOwned {
    jsonrpc: &'static str,
    id: String,
    method: String,
    params: serde_json::Value,
}

#[derive(Deserialize)]
pub struct RpcResponse<T> {
    pub result: Option<T>,
    pub error: Option<RpcError>,
}

#[derive(Deserialize, Debug)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

pub async fn rpc_call<T: DeserializeOwned>(
    st: &AppState,
    method: &str,
    params: serde_json::Value,
) -> anyhow::Result<T> {
    let req = RpcRequestOwned {
        jsonrpc: "1.0",
        id: "axum".to_string(),
        method: method.to_string(),
        params,
    };

    let res = st.http
        .post(&st.rpc_url)
        .basic_auth(&st.rpc_user, Some(&st.rpc_pass))
        .json(&req)
        .send()
        .await
        .context("rpc http send failed")?;

    let status: HttpStatus = res.status();
    let body = res
        .json::<RpcResponse<T>>()
        .await
        .with_context(|| format!("rpc parse failed (status {status})"))?;

    if let Some(err) = body.error {
        return Err(anyhow::anyhow!("rpc error {}: {}", err.code, err.message));
    }
    body
        .result
        .ok_or_else(|| anyhow::anyhow!("rpc response missing result"))
}