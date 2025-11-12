// state.rs
use reqwest::Client;

#[derive(Clone)]
pub struct AppState {
    pub http: Client,
    pub rpc_url: String,
    pub rpc_user: String,
    pub rpc_pass: String,
    pub electrs_addr: String,
}

impl AppState {
    pub fn new(rpc_url: String, rpc_user: String, rpc_pass: String, electrs_addr: String) -> Self {
        Self {
            http: Client::new(),
            rpc_url,
            rpc_user,
            rpc_pass,
            electrs_addr,
        }
    }
}
