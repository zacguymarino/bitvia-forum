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
    pub id: serde_json::Value,
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
    Ok(body
        .result
        .ok_or_else(|| anyhow::anyhow!("rpc response missing result"))?)
}

pub async fn rpc_batch<T: DeserializeOwned>(
    st: &AppState,
    method: &str,
    params_list: Vec<serde_json::Value>,
) -> anyhow::Result<Vec<T>> {
    let batch: Vec<RpcRequestOwned> = params_list
        .into_iter()
        .enumerate()
        .map(|(i, params)| RpcRequestOwned {
            jsonrpc: "1.0",
            id: format!("b{i}"),
            method: method.to_string(),
            params,
        })
        .collect();

    let res = st.http
        .post(&st.rpc_url)
        .basic_auth(&st.rpc_user, Some(&st.rpc_pass))
        .json(&batch)
        .send()
        .await
        .context("rpc batch http send failed")?;

    let status = res.status();
    let body = res
        .json::<Vec<RpcResponse<T>>>()
        .await
        .with_context(|| format!("rpc batch parse failed (status {status})"))?;

    let mut out = Vec::with_capacity(body.len());
    for item in body {
        if let Some(err) = item.error {
            return Err(anyhow::anyhow!("rpc batch item error {}: {}", err.code, err.message));
        }
        out.push(item.result.ok_or_else(|| anyhow::anyhow!("rpc batch item missing result"))?);
    }
    Ok(out)
}
