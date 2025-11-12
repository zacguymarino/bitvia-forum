// src/handlers/address.rs
use std::sync::Arc;

use axum::{extract::{Path, Query, State}, Json};
use bitcoin::{address::NetworkUnchecked, Address, Network};
use electrum_client::{Client as ElectrumClient, ElectrumApi};

use crate::{
    models::{AddrBalance, AddrUtxo, AddrQ, HistQ, AddrHistoryItem, AddrHistoryResp},
    state::AppState,
    utils::internalize,
};

fn sats_to_btc_i64(s: i64) -> f64 {
    (s as f64) / 100_000_000.0
}

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

        // Uses Electrum's "blockchain.script.get_history"
        // Type is typically Vec<electrum_client::types::History>
        let hist = cli.script_get_history(spk.as_script())?;

        let total = hist.len();
        let end = (offset + limit).min(total);
        let slice = if offset < end { &hist[offset..end] } else { &[] };

        let items = slice
            .iter()
            .map(|h| AddrHistoryItem {
                txid: h.tx_hash.to_string(),
                height: h.height, // <= 0 means unconfirmed
            })
            .collect::<Vec<_>>();

        Ok(AddrHistoryResp {
            address: addr.to_string(),
            total,
            offset,
            limit,
            items,
        })
    })
    .await
    .map_err(|e| internalize(format!("electrum task failed: {e}")))?
    .map_err(internalize)?;

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
