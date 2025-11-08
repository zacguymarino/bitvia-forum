use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use crate::{
    models::{PrevoutResolved, ResolveQ, TxDecoded, TxView},
    rpc::{rpc_batch, rpc_call},
    state::AppState,
    utils::{internalize, tx_is_coinbase, vout_address, vout_value_btc},
};

pub async fn tx_by_id(
    State(st): State<Arc<AppState>>,
    Path(txid): Path<String>,
    Query(q): Query<ResolveQ>,
) -> Result<Json<TxView>, (axum::http::StatusCode, String)> {
    // main tx
    let tx: TxDecoded = rpc_call(&st, "getrawtransaction", serde_json::json!([txid, true]))
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.to_lowercase().contains("no such mempool or blockchain transaction") {
                (axum::http::StatusCode::NOT_FOUND, format!("tx not found: {msg}"))
            } else {
                internalize(msg)
            }
        })?;

    // outputs total
    let outputs_total_btc: f64 = tx.vout.iter().map(vout_value_btc).sum();

    // resolve inputs (capped)
    let total_inputs = tx.vin.len();
    let cap_max = 100usize;
    let mut resolve_n = q.resolve.unwrap_or(20);
    if resolve_n > cap_max { resolve_n = cap_max; }
    if resolve_n > total_inputs { resolve_n = total_inputs; }

    let mut prev_params = Vec::with_capacity(resolve_n);
    let mut in_map: Vec<(String, u32)> = Vec::with_capacity(resolve_n);
    for vin in tx.vin.iter().take(resolve_n) {
        if let (Some(prev_txid), Some(vout_idx)) = (
            vin.get("txid").and_then(|x| x.as_str()),
            vin.get("vout").and_then(|x| x.as_u64()).map(|x| x as u32),
        ) {
            prev_params.push(serde_json::json!([prev_txid, true]));
            in_map.push((prev_txid.to_string(), vout_idx));
        }
    }

    let mut inputs_resolved = Vec::<PrevoutResolved>::new();
    let mut inputs_total_btc = None::<f64>;

    if !prev_params.is_empty() {
        let prev_txs: Vec<TxDecoded> = rpc_batch(&st, "getrawtransaction", prev_params)
            .await
            .map_err(internalize)?;

        use std::collections::HashMap;
        let mut by_id: HashMap<&str, &TxDecoded> = HashMap::with_capacity(prev_txs.len());
        for t in &prev_txs {
            by_id.insert(t.txid.as_str(), t);
        }

        let mut sum_inputs = 0.0_f64;
        for (prev_txid, vout_idx) in in_map {
            if let Some(prev) = by_id.get(prev_txid.as_str()) {
                if let Some(vout_val) = prev.vout.get(vout_idx as usize) {
                    let val = vout_value_btc(vout_val);
                    let addr = vout_address(vout_val);
                    inputs_resolved.push(PrevoutResolved {
                        txid: prev_txid.clone(),
                        vout: vout_idx,
                        value_btc: val,
                        address: addr,
                    });
                    sum_inputs += val;
                }
            }
        }
        inputs_total_btc = Some(sum_inputs);
    }

    let fee_btc = inputs_total_btc.map(|ins| (ins - outputs_total_btc).max(0.0));
    let feerate_sat_vb = match (fee_btc, tx.vsize) {
        (Some(fee_btc), Some(vsize)) if vsize > 0 => {
            let fee_sats = fee_btc * 100_000_000.0;
            Some(fee_sats / (vsize as f64))
        }
        _ => None,
    };

    let is_cb = tx_is_coinbase(&tx);

    let view = TxView {
        txid: tx.txid,
        size: tx.size,
        vsize: tx.vsize,
        weight: tx.weight,
        confirmations: tx.confirmations,
        blockhash: tx.blockhash,
        is_coinbase: is_cb,

        inputs_resolved,
        inputs_total_btc,
        outputs_total_btc,
        fee_btc,
        feerate_sat_vb,

        total_inputs,
        resolved_inputs: resolve_n,
        more_inputs: total_inputs > resolve_n,

        vout: tx.vout,
    };

    Ok(Json(view))
}
