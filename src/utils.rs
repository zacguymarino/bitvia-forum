use axum::http::StatusCode;

use crate::models::TxDecoded;

pub fn internalize<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::BAD_GATEWAY, format!("RPC failed: {e}"))
}

pub fn vout_value_btc(v: &serde_json::Value) -> f64 {
    v.get("value").and_then(|x| x.as_f64()).unwrap_or(0.0)
}

pub fn vout_address(v: &serde_json::Value) -> String {
    let spk = v.get("scriptPubKey").cloned().unwrap_or(serde_json::json!({}));
    if let Some(addr) = spk.get("address").and_then(|a| a.as_str()) {
        return addr.to_string();
    }
    if let Some(arr) = spk.get("addresses").and_then(|a| a.as_array()) {
        if let Some(first) = arr.first().and_then(|x| x.as_str()) {
            return first.to_string();
        }
    }
    "(no address)".to_string()
}

pub fn tx_is_coinbase(tx: &TxDecoded) -> bool {
    if tx.vin.is_empty() { return false; }
    tx.vin[0].get("coinbase").is_some()
}
