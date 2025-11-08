/// Total mined supply up to (and including) `height` in BTC.
/// (Genesis subsidy excluded.)
pub fn mined_supply_btc(height: u64) -> f64 {
    let mut remaining = height;
    let mut subsidy_sats: u64 = 50_0000_0000; // 50 BTC
    let mut total_sats: u128 = 0;

    for _ in 0..64 {
        if remaining == 0 || subsidy_sats == 0 { break; }
        let blocks = remaining.min(210_000);
        total_sats += (blocks as u128) * (subsidy_sats as u128);
        remaining -= blocks;
        subsidy_sats >>= 1;
    }
    (total_sats as f64) / 100_000_000.0
}

/// Current block subsidy in BTC at `height`.
pub fn current_subsidy_btc(height: u64) -> f64 {
    let halvings = (height / 210_000) as u32;
    let sats: u64 = if halvings >= 64 { 0 } else { 50_0000_0000 >> halvings };
    (sats as f64) / 100_000_000.0
}
