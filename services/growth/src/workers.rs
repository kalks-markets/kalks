//! Background loops. Each loop logs and carries on after an error; all work is idempotent.

use crate::state::AppState;
use crate::{bonus, contests, db, deals, journeys, loyalty, missions, payouts, profiles};
use std::future::Future;
use std::time::Duration;

fn every<F, Fut>(st: &AppState, name: &'static str, period: Duration, f: F)
where
    F: Fn(AppState) -> Fut + Send + 'static,
    Fut: Future<Output = anyhow::Result<()>> + Send,
{
    let st = st.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(period);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut failing = false;
        loop {
            tick.tick().await;
            match f(st.clone()).await {
                Ok(()) => {
                    if failing {
                        tracing::info!(worker = name, "worker recovered");
                    }
                    failing = false;
                }
                Err(e) => {
                    if !failing {
                        tracing::warn!(worker = name, error = %e, "worker step failed");
                    } else {
                        tracing::debug!(worker = name, error = %e, "worker step failed");
                    }
                    failing = true;
                }
            }
        }
    });
}

/// One run of a named job (Back Office "run now", tests).
pub async fn run_job(st: &AppState, job: &str) -> anyhow::Result<serde_json::Value> {
    use serde_json::json;
    Ok(match job {
        "profiles" => json!({"synced": profiles::sync_once(st).await?}),
        "deals" => {
            let mut n = 0;
            for t in db::tenants(&st.pool).await? {
                n += deals::poll_engine(st, &t).await?;
            }
            json!({"deals": n})
        }
        "bonus" => json!({"deposits": bonus::deposit_tick(st).await?, "lifecycle": bonus::lifecycle_tick(st).await?, "posted": bonus::post_tick(st).await?}),
        "contests" => json!({"refreshed": contests::tick(st).await?}),
        "payouts" => {
            let mut created = 0;
            for t in db::tenants(&st.pool).await? {
                created += payouts::cashback_batch(st, &t, false).await?.0;
            }
            json!({"cashbackBatches": created, "cashbackPaid": payouts::cashback_tick(st).await?, "walletCredits": payouts::wallet_tick(st).await?})
        }
        "reversals" => {
            let mut n = 0;
            for t in db::tenants(&st.pool).await? {
                n += deals::sweep_reversals(st, &t).await?;
            }
            json!({"reversed": n})
        }
        "expiry" => {
            let mut n = 0;
            for t in db::tenants(&st.pool).await? {
                n += loyalty::expiry_tick(st, &t).await?;
            }
            json!({"expired": n})
        }
        "journeys" => {
            let mut facts = 0;
            for t in db::tenants(&st.pool).await? {
                facts += journeys::sync_facts(st, &t).await.unwrap_or(0);
            }
            json!({"facts": facts, "enrolled": journeys::enrol_tick(st).await?, "processed": journeys::run_tick(st).await?})
        }
        "missions" => json!({"completed": missions::tick(st).await?}),
        _ => anyhow::bail!("unknown job"),
    })
}

pub fn spawn(st: &AppState) {
    let secs = Duration::from_secs;
    every(st, "profiles", secs(st.cfg.sync_secs), |st| async move {
        profiles::sync_once(&st).await?;
        Ok(())
    });
    every(st, "deals", secs(st.cfg.deals_secs), |st| async move {
        for t in db::tenants(&st.pool).await? {
            deals::poll_engine(&st, &t).await?;
        }
        Ok(())
    });
    every(st, "bonus-lifecycle", secs(15), |st| async move {
        bonus::deposit_tick(&st).await?;
        bonus::lifecycle_tick(&st).await?;
        Ok(())
    });
    every(st, "contests", secs(st.cfg.leaderboard_secs), |st| async move {
        contests::tick(&st).await?;
        Ok(())
    });
    // missions, trading streaks and rewards waiting for KYC (phase 4)
    every(st, "missions", secs(30), |st| async move {
        missions::tick(&st).await?;
        Ok(())
    });
    every(st, "cashback-batches", secs(300), |st| async move {
        for t in db::tenants(&st.pool).await? {
            payouts::cashback_batch(&st, &t, false).await?;
        }
        Ok(())
    });
    every(st, "reversals", secs(600), |st| async move {
        for t in db::tenants(&st.pool).await? {
            deals::sweep_reversals(&st, &t).await?;
        }
        Ok(())
    });
    every(st, "points-expiry", secs(3600), |st| async move {
        for t in db::tenants(&st.pool).await? {
            loyalty::expiry_tick(&st, &t).await?;
        }
        Ok(())
    });
    every(st, "journey-facts", secs(60), |st| async move {
        for t in db::tenants(&st.pool).await? {
            journeys::sync_facts(&st, &t).await?;
        }
        Ok(())
    });
    every(st, "journeys", secs(15), |st| async move {
        journeys::enrol_tick(&st).await?;
        while journeys::run_tick(&st).await? == 100 {}
        Ok(())
    });
    // engine legs and wallet credits: every 5 s, or at once when something is queued
    let st2 = st.clone();
    tokio::spawn(async move {
        loop {
            if let Err(e) = bonus::post_tick(&st2).await {
                tracing::warn!(error = %e, "bonus posting step failed");
            }
            if let Err(e) = payouts::cashback_tick(&st2).await {
                tracing::warn!(error = %e, "cashback payout step failed");
            }
            if let Err(e) = payouts::wallet_tick(&st2).await {
                tracing::warn!(error = %e, "wallet credit step failed");
            }
            tokio::select! {
                _ = st2.wake.notified() => {}
                _ = tokio::time::sleep(Duration::from_secs(5)) => {}
            }
        }
    });
}
