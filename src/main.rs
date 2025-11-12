use anyhow::{Context, Result};
use axum::{routing::get, Router};
use dotenvy::dotenv;
use std::{net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;
use tower_http::services::ServeDir;

mod state;
mod rpc;
mod models;
mod supply;
mod utils;
mod handlers;

use state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    let rpc_url = std::env::var("RPC_URL").context("missing RPC_URL")?;
    let rpc_user = std::env::var("RPC_USER").context("missing RPC_USER")?;
    let rpc_pass = std::env::var("RPC_PASS").context("missing RPC_PASS")?;
    let bind_addr: SocketAddr = std::env::var("BIND_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8000".to_string())
        .parse()
        .context("BIND_ADDR must be host:port")?;

    let electrs_addr = std::env::var("ELECTRS_ADDR").unwrap_or_else(|_| "127.0.0.1:50001".to_string());

    let state = Arc::new(AppState::new(
        rpc_url,
        rpc_user,
        rpc_pass,
        electrs_addr,
    ));

    let app = Router::new()
        // pages
        .route("/", get(handlers::pages::index))
        .route("/health", get(handlers::pages::health))
        // api
        .route("/api/mempoolinfo", get(handlers::mempool::mempoolinfo))
        .route("/api/network", get(handlers::network::network_summary))
        .route("/api/blockhash/{height}", get(handlers::blocks::blockhash_by_height))
        .route("/api/block/{hash}", get(handlers::blocks::block_by_hash))
        .route("/api/tx/{txid}", get(handlers::tx::tx_by_id))
        .route("/api/addr/{address}", get(handlers::address::addr_balance))
        .route("/api/addr/{address}/history", get(handlers::address::addr_history))
        // static
        .nest_service("/static", ServeDir::new("static"))
        // shared state
        .with_state(state);

    println!("listening on http://{bind_addr}");
    let listener = TcpListener::bind(bind_addr).await?;
    axum::serve(listener, app).await.context("server crashed")
}