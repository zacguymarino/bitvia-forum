use serde::{Deserialize, Serialize};

/// Address balance stuff
#[derive(serde::Deserialize)]
pub struct AddrQ {
    pub details: Option<bool>, // ?details=1 to include utxo list
}

#[derive(serde::Serialize)]
pub struct AddrBalance {
    pub address: String,
    pub total_btc: f64,
    pub utxo_count: usize,
    pub utxos: Option<Vec<AddrUtxo>>,
}

/// Minimal header we read from `getblockheader`
#[derive(Deserialize, Serialize)]
pub struct BlockHeaderLite {
    pub time: u64,                 // UNIX seconds
    pub height: Option<u64>,
}

/// `getblockchaininfo` subset we need
#[derive(Deserialize, Serialize)]
pub struct ChainInfo {
    pub blocks: u64,
    pub difficulty: f64,
}

/// `getmempoolinfo`
#[derive(Deserialize, Serialize)]
pub struct MempoolInfo {
    pub size: u64,
    pub bytes: u64,
    pub usage: u64,
    #[serde(default)] pub fullrbf: bool,
    #[serde(default)] pub unbroadcastcount: u64,
    #[serde(default)] pub mempoolminfee: f64,
}

/// API response for `/api/network`
#[derive(Deserialize, Serialize)]
pub struct NetworkSummary {
    pub height: u64,
    pub difficulty: f64,
    pub hashrate_ghps: f64,
    pub avg_block_interval_sec: f64,

    pub blocks_into_epoch: u64,
    pub blocks_to_next_adjust: u64,
    pub est_diff_change_pct: f64,

    pub current_subsidy_btc: f64,
    pub est_new_btc_per_day: f64,
    pub est_circulating_btc: f64,

    pub tip_time: u64,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct GetBlockV1 {
    pub hash: String,
    pub height: u64,
    pub time: u64,
    pub mediantime: Option<u64>,
    pub size: u64,
    pub weight: Option<u64>,
    #[serde(rename = "nTx")]
    pub n_tx: u64,
    pub prevblockhash: Option<String>,
    pub nextblockhash: Option<String>,
    pub tx: Vec<String>, // <— txids only
}

/// Response for `/api/blockhash/{height}`
#[derive(Serialize)]
pub struct BlockHashResp {
    pub height: u64,
    pub hash: String,
}

/// Query params for block tx pagination
#[derive(Deserialize)]
pub struct BlockPageQ {
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

/// What we send to the UI for a block (with pagination fields)
#[derive(Serialize)]
pub struct BlockView {
    pub hash: String,
    pub height: u64,
    pub time: u64,
    pub mediantime: Option<u64>,
    pub size: u64,
    pub weight: Option<u64>,
    pub n_tx: u64,
    pub prev: Option<String>,
    pub next: Option<String>,

    pub txids: Vec<String>,
    pub more_tx: bool,

    // paging meta
    pub total_tx: usize,
    pub offset: usize,
    pub limit: usize,
}

/// `getrawtransaction` decoded (verbosity=true)
#[derive(Deserialize, Serialize, Clone)]
pub struct TxDecoded {
    pub txid: String,
    pub hash: Option<String>,
    pub size: Option<u64>,
    pub vsize: Option<u64>,
    pub weight: Option<u64>,
    pub version: Option<i64>,
    pub locktime: Option<u64>,
    pub vin: Vec<serde_json::Value>,
    pub vout: Vec<serde_json::Value>,
    pub hex: Option<String>,
    pub time: Option<u64>,
    pub blocktime: Option<u64>,
    pub confirmations: Option<u64>,
    pub blockhash: Option<String>,
}

#[derive(Serialize)]
pub struct PrevoutResolved {
    pub txid: String,
    pub vout: u32,
    pub value_btc: f64,
    pub address: String,
}

#[derive(Serialize)]
pub struct TxView {
    pub txid: String,
    pub size: Option<u64>,
    pub vsize: Option<u64>,
    pub weight: Option<u64>,
    pub confirmations: Option<u64>,
    pub blockhash: Option<String>,
    pub is_coinbase: bool,

    pub inputs_resolved: Vec<PrevoutResolved>,
    pub inputs_total_btc: Option<f64>,
    pub outputs_total_btc: f64,
    pub fee_btc: Option<f64>,
    pub feerate_sat_vb: Option<f64>,

    pub total_inputs: usize,
    pub resolved_inputs: usize,
    pub more_inputs: bool,

    pub vout: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
pub struct ResolveQ {
    pub resolve: Option<usize>,
}

#[derive(Serialize)]
pub struct AddrUtxo {
    pub txid: String,
    pub vout: u32,
    pub amount_btc: f64,
    pub height: Option<u32>,
    pub script_pub_key: String,
}

#[derive(serde::Deserialize)]
pub struct HistQ {
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

#[derive(serde::Serialize)]
pub struct AddrHistoryItem {
    pub txid: String,
    pub height: i32,
    pub timestamp: Option<u64>,
    pub direction: String,
    pub delta_btc: f64,
    pub value_in_btc: f64,
    pub value_out_btc: f64,
}

#[derive(serde::Serialize)]
pub struct AddrHistoryResp {
    pub address: String,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub items: Vec<AddrHistoryItem>,
}
