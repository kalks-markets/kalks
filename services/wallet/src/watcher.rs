//! Background workers (one instance only, `WALLET_WORKERS=true`):
//! - every `poll_secs`: verify / confirm / credit open deposits, verify paid withdrawals, settle pending
//!   trading transfers, expire intents, re-check open NOWPayments payments (the IPN fallback);
//! - every `scan_secs`: scan the receiving addresses for incoming transfers nobody claimed.

use std::time::Duration;

use crate::chain::ChainId;
use crate::ops::{adjustments, deposits, nowpayments, trading, withdrawals};
use crate::state::AppState;

/// One pass of the fast loop. Returns (deposits checked, payouts checked, transfers recovered).
pub async fn tick(st: &AppState) -> anyhow::Result<(usize, usize, usize)> {
    let deps: Vec<i64> = sqlx::query_scalar("SELECT id FROM deposits WHERE status IN ('pending', 'confirming') ORDER BY last_checked_at NULLS FIRST, id LIMIT 200").fetch_all(&st.pool).await?;
    for id in &deps {
        if let Err(e) = deposits::process(st, *id).await {
            tracing::warn!(deposit = id, error = %e, "deposit check failed");
        }
    }
    let pays: Vec<i64> = sqlx::query_scalar("SELECT id FROM withdrawals WHERE status = 'paid' ORDER BY updated_at LIMIT 100").fetch_all(&st.pool).await?;
    for id in &pays {
        if let Err(e) = withdrawals::verify_payout(st, *id).await {
            tracing::warn!(withdrawal = id, error = %e, "payout check failed");
        }
    }
    let rec = trading::recover(st, 10).await?;
    if let Err(e) = adjustments::recover(st, 30).await {
        tracing::warn!(error = %e, "adjustment recovery pass failed");
    }
    deposits::expire_intents(st).await?;
    if let Err(e) = nowpayments::poll(st).await {
        tracing::warn!(error = %e, "NOWPayments poll pass failed");
    }
    Ok((deps.len(), pays.len(), rec))
}

pub async fn scan_all(st: &AppState) {
    for (tenant_id, _) in st.tenants.all() {
        for chain in ChainId::ALL {
            match deposits::scan(st, tenant_id, chain).await {
                Ok(n) if n > 0 => tracing::info!(tenant_id, %chain, new = n, "scanner found incoming transfers"),
                Ok(_) => {}
                Err(e) => tracing::warn!(tenant_id, %chain, error = %e, "scan failed"),
            }
        }
    }
}

pub fn spawn(st: AppState) {
    let fast = st.clone();
    tokio::spawn(async move {
        let mut t = tokio::time::interval(Duration::from_secs(fast.cfg.poll_secs));
        t.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            t.tick().await;
            if let Err(e) = tick(&fast).await {
                tracing::error!(error = %e, "watcher pass failed");
            }
        }
    });
    tokio::spawn(async move {
        let mut t = tokio::time::interval(Duration::from_secs(st.cfg.scan_secs));
        t.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            t.tick().await;
            scan_all(&st).await;
        }
    });
}
