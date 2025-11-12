use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use bitcoin::{Address, Network, Txid};
use electrum_client::{Client as ElectrumClient, ElectrumApi};

use crate::{
    models::{PrevoutResolved, ResolveQ, TxDecoded, TxView},
    rpc::rpc_call,
    state::AppState,
    utils::{internalize, tx_is_coinbase, vout_address, vout_value_btc},
};

fn sats_to_btc(s: u64) -> f64 {
    (s as f64) / 100_000_000.0
}

use std::str::FromStr;

pub async fn tx_by_id(
    State(st): State<Arc<AppState>>,
    Path(txid): Path<String>,
    Query(q): Query<ResolveQ>,
) -> Result<Json<TxView>, (axum::http::StatusCode, String)> {
    // 1) Main tx via Core (keeps confirmations/blockhash/vsize accurate)
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

    // 2) Outputs total
    let outputs_total_btc: f64 = tx.vout.iter().map(vout_value_btc).sum();

    // 3) Prevouts list (capped)
    let total_inputs = tx.vin.len();
    let cap_max = 100usize;
    let mut resolve_n = q.resolve.unwrap_or(20);
    if resolve_n > cap_max { resolve_n = cap_max; }
    if resolve_n > total_inputs { resolve_n = total_inputs; }

    let mut prev_pairs: Vec<(String, u32)> = Vec::with_capacity(resolve_n);
    for vin in tx.vin.iter().take(resolve_n) {
        if let (Some(prev_txid), Some(vout_idx)) = (
            vin.get("txid").and_then(|x| x.as_str()),
            vin.get("vout").and_then(|x| x.as_u64()).map(|x| x as u32),
        ) {
            prev_pairs.push((prev_txid.to_string(), vout_idx));
        }
    }

    // 4) Resolve prevouts via Electrs in spawn_blocking
    let electrs_addr = st.electrs_addr.clone();
    let (inputs_resolved, inputs_total_btc) =
        tokio::task::spawn_blocking(move || -> anyhow::Result<(Vec<PrevoutResolved>, Option<f64>)> {
            let cli = ElectrumClient::new(&format!("tcp://{}", electrs_addr))?;

            let mut out = Vec::<PrevoutResolved>::with_capacity(prev_pairs.len());
            let mut sum_inputs_sats: u128 = 0;

            for (prev_txid_str, vout_idx) in prev_pairs {
                let prev_txid = Txid::from_str(&prev_txid_str)
                    .map_err(|e| anyhow::anyhow!("bad prev txid {}: {}", prev_txid_str, e))?;

                // Fetch previous tx (bitcoin::Transaction)
                let prev = cli.transaction_get(&prev_txid)?;
                let vout = prev
                    .output
                    .get(vout_idx as usize)
                    .ok_or_else(|| anyhow::anyhow!("prevout index {} out of range", vout_idx))?;

                // Amount is `Amount`; convert to primitive sats
                let val_sats: u64 = vout.value.to_sat();
                sum_inputs_sats += val_sats as u128;

                // Try to render address from script
                let addr = Address::from_script(&vout.script_pubkey, Network::Bitcoin)
                    .map(|a| a.to_string())
                    .unwrap_or_else(|_| "(no address)".to_string()); // <- accept error arg

                out.push(PrevoutResolved {
                    txid: prev_txid_str,
                    vout: vout_idx,
                    value_btc: (val_sats as f64) / 100_000_000.0, // sats → BTC
                    address: addr,
                });
            }

            let inputs_total_btc = if sum_inputs_sats > 0 {
                Some((sum_inputs_sats as f64) / 100_000_000.0)
            } else {
                None
            };

            Ok((out, inputs_total_btc))
        })
        .await
        .map_err(|e| internalize(format!("electrum task failed: {e}")))?
        .map_err(internalize)?;

    // 5) Fee & feerate
    let fee_btc = inputs_total_btc.map(|ins| (ins - outputs_total_btc).max(0.0));
    let feerate_sat_vb = match (fee_btc, tx.vsize) {
        (Some(fee_btc), Some(vsize)) if vsize > 0 => {
            let fee_sats = fee_btc * 100_000_000.0;
            Some(fee_sats / (vsize as f64))
        }
        _ => None,
    };

    // 6) Coinbase?
    let is_cb = tx_is_coinbase(&tx);

    // 7) Response (unchanged shape)
    let view = TxView {
        txid: tx.txid.clone(),
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

