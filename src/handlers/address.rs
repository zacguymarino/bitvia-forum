// src/handlers/address.rs
use std::{collections::HashMap, sync::Arc};

use axum::{extract::{Path, Query, State}, Json};
use bitcoin::{address::NetworkUnchecked, Address, Network, Txid};
use electrum_client::{Client as ElectrumClient, ElectrumApi};

use crate::{
    models::{AddrBalance, AddrUtxo, AddrQ, HistQ, AddrHistoryItem, AddrHistoryResp},
    state::AppState,
    utils::internalize,
};

fn sats_to_btc_i64(s: i64) -> f64 {
    (s as f64) / 100_000_000.0
}

fn sats_to_btc_u64(s: u64) -> f64 {
    (s as f64) / 100_000_000.0
}

// pub async fn addr_history(
//     State(st): State<Arc<AppState>>,
//     Path(addr_str): Path<String>,
//     Query(q): Query<HistQ>,
// ) -> Result<Json<AddrHistoryResp>, (axum::http::StatusCode, String)> {
//     let electrs = st.electrs_addr.clone();
//     let limit = q.limit.unwrap_or(25).clamp(1, 200);
//     let offset = q.offset.unwrap_or(0);

//     let res = tokio::task::spawn_blocking(move || -> anyhow::Result<AddrHistoryResp> {
//         // Parse -> require mainnet, then get scriptPubKey
//         let unchecked: Address<NetworkUnchecked> = addr_str.parse()?;
//         let addr = unchecked
//             .require_network(Network::Bitcoin)
//             .map_err(|_| anyhow::anyhow!("address is not a mainnet address"))?;
//         let spk = addr.script_pubkey();

//         let cli = ElectrumClient::new(&format!("tcp://{}", electrs))?;

//         // Uses Electrum's "blockchain.script.get_history"
//         // Type is typically Vec<electrum_client::types::History>
//         let hist = cli.script_get_history(spk.as_script())?;

//         let total = hist.len();
//         let end = (offset + limit).min(total);
//         let slice = if offset < end { &hist[offset..end] } else { &[] };

//         let items = slice
//             .iter()
//             .map(|h| AddrHistoryItem {
//                 txid: h.tx_hash.to_string(),
//                 height: h.height, // <= 0 means unconfirmed
//             })
//             .collect::<Vec<_>>();

//         Ok(AddrHistoryResp {
//             address: addr.to_string(),
//             total,
//             offset,
//             limit,
//             items,
//         })
//     })
//     .await
//     .map_err(|e| internalize(format!("electrum task failed: {e}")))?
//     .map_err(internalize)?;

//     Ok(Json(res))
// }

pub async fn addr_history(
    State(st): State<Arc<AppState>>,
    Path(addr_str): Path<String>,
    Query(q): Query<HistQ>,
) -> Result<Json<AddrHistoryResp>, (axum::http::StatusCode, String)> {
    let electrs = st.electrs_addr.clone();
    let limit = q.limit.unwrap_or(25).clamp(1, 200);
    let offset = q.offset.unwrap_or(0);

    let res = tokio::task::spawn_blocking(move || -> anyhow::Result<AddrHistoryResp> {
        // Parse -> require mainnet, then get scriptPubKey
        let unchecked: Address<NetworkUnchecked> = addr_str.parse()?;
        let addr = unchecked
            .require_network(Network::Bitcoin)
            .map_err(|_| anyhow::anyhow!("address is not a mainnet address"))?;
        let spk = addr.script_pubkey();

        let cli = ElectrumClient::new(&format!("tcp://{}", electrs))?;

        // Full history from Electrs
        let hist = cli.script_get_history(spk.as_script())?;

        let total = hist.len();
        let end = (offset + limit).min(total);
        let slice = if offset < end { &hist[offset..end] } else { &[] };

        // Small caches to avoid repeat work
        let mut header_cache: HashMap<i32, u32> = HashMap::new();           // height -> time
        let mut prevtx_cache: HashMap<Txid, bitcoin::Transaction> = HashMap::new(); // prev txid -> tx

        let mut items = Vec::with_capacity(slice.len());

        for h in slice {
            // Full tx for this history entry
            let tx = cli.transaction_get(&h.tx_hash)?;

            // --- Outputs to this address ---
            let mut value_out_sats: u64 = 0;
            for out in &tx.output {
                if out.script_pubkey == spk {
                    value_out_sats = value_out_sats.saturating_add(out.value.to_sat());
                }
            }

            // --- Inputs from this address ---
            let mut value_in_sats: u64 = 0;
            for inp in &tx.input {
                // Coinbase has no previous_output to inspect
                if inp.previous_output.is_null() {
                    continue;
                }

                let prev_txid = inp.previous_output.txid;
                let vout_idx = inp.previous_output.vout as usize;

                // Fetch and cache previous tx
                let prev_tx = if let Some(cached) = prevtx_cache.get(&prev_txid) {
                    cached.clone()
                } else {
                    let t = cli.transaction_get(&prev_txid)?;
                    prevtx_cache.insert(prev_txid, t.clone());
                    t
                };

                if let Some(prev_out) = prev_tx.output.get(vout_idx) {
                    if prev_out.script_pubkey == spk {
                        value_in_sats = value_in_sats.saturating_add(prev_out.value.to_sat());
                    }
                }
            }

            // Net change for this address in this tx
            let delta_sats: i64 = (value_out_sats as i64) - (value_in_sats as i64);

            let direction = if delta_sats > 0 {
                "in"
            } else if delta_sats < 0 {
                "out"
            } else if value_in_sats > 0 && value_out_sats > 0 {
                // spent from and paid back to same address, net zero
                "self"
            } else {
                "unknown"
            }
            .to_string();

            // Block time if confirmed
            let timestamp: Option<u64> = if h.height > 0 {
                if let Some(t) = header_cache.get(&h.height) {
                    Some(*t as u64)
                } else {
                    let header = cli.block_header(h.height as usize)?;
                    let t = header.time;
                    header_cache.insert(h.height, t);
                    Some(t as u64)
                }
            } else {
                None
            };

            items.push(AddrHistoryItem {
                txid: h.tx_hash.to_string(),
                height: h.height,
                timestamp,
                direction,
                delta_btc: sats_to_btc_i64(delta_sats),
                value_in_btc: sats_to_btc_u64(value_in_sats),
                value_out_btc: sats_to_btc_u64(value_out_sats),
            });
        }

        Ok(AddrHistoryResp {
            address: addr.to_string(),
            total,
            offset,
            limit,
            items,
        })
    })
    .await
    .map_err(|e| internalize(format!("electrum task failed: {e}")))? // join error
    .map_err(internalize)?; // anyhow -> (StatusCode, String)

    Ok(Json(res))
}


pub async fn addr_balance(
    State(st): State<Arc<AppState>>,
    Path(addr_str): Path<String>,
    Query(q): Query<AddrQ>,
) -> Result<Json<AddrBalance>, (axum::http::StatusCode, String)> {
    // Move data needed by the blocking task.
    let electrs = st.electrs_addr.clone();
    let details = q.details.unwrap_or(false);

    let task = tokio::task::spawn_blocking(move || -> anyhow::Result<AddrBalance> {
        // Create blocking Electrum client inside the blocking task.
        let client = ElectrumClient::new(&format!("tcp://{}", electrs))?;

        // Parse and require mainnet
        let unchecked: Address<NetworkUnchecked> = addr_str.parse()?;
        let addr = unchecked.require_network(Network::Bitcoin)
            .map_err(|_| anyhow::anyhow!("address is not a mainnet address"))?;

        let spk = addr.script_pubkey();

        // Balance (confirmed: u64, unconfirmed: i64)
        let bal = client.script_get_balance(spk.as_script())?;
        let total_i64 = ((bal.confirmed as i64) + bal.unconfirmed).max(0);

        // Optional UTXOs
        let mut utxos_vec: Vec<AddrUtxo> = Vec::new();
        if details {
            let utxos = client.script_list_unspent(spk.as_script())?;
            let spk_hex = hex::encode(spk.as_bytes());

            for u in utxos {
                let height_opt = if u.height == 0 { None } else { Some(u.height as u32) };
                utxos_vec.push(AddrUtxo {
                    txid: u.tx_hash.to_string(),
                    vout: u.tx_pos as u32,
                    amount_btc: sats_to_btc_i64(u.value as i64),
                    height: height_opt,
                    script_pub_key: spk_hex.clone(),
                });
            }
        }

        Ok(AddrBalance {
            address: addr.to_string(),
            total_btc: sats_to_btc_i64(total_i64),
            utxo_count: utxos_vec.len(),
            utxos: if details { Some(utxos_vec) } else { None },
        })
    });

    // Join the blocking task and map errors
    match task.await {
        Ok(Ok(res)) => Ok(Json(res)),
        Ok(Err(e)) => Err(internalize(e)),
        Err(join_err) => Err(internalize(format!("electrum task failed: {join_err}"))),
    }
}
