use serde::{Deserialize, Serialize};

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

/// `getblock` verbosity=2 (subset)
#[derive(Deserialize, Serialize)]
pub struct GetBlockV2 {
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
    pub tx: Vec<serde_json::Value>,
}

/// What we send to the UI for a block
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
}

#[derive(Serialize)]
pub struct BlockHashResp {
    pub height: u64,
    pub hash: String,
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
