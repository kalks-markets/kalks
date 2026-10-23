//! Business operations shared by the HTTP handlers, the background workers and the tests.

pub mod adjustments;
pub mod deposits;
pub mod nowpayments;
pub mod trading;
pub mod transfers;
pub mod withdrawals;

use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};

/// Records a client notification (D41) inside the caller's transaction. `notifier` pushes it to the support
/// service after the commit (bell, realtime, email per the client's preference).
pub async fn notify(tx: &mut Transaction<'_, Postgres>, tenant_id: i64, user_id: i64, kind: &str, title: &str, body: &str, data: Value) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO notifications (tenant_id, user_id, kind, title, body, data) VALUES ($1, $2, $3, $4, $5, $6)")
        .bind(tenant_id)
        .bind(user_id)
        .bind(kind)
        .bind(title)
        .bind(body)
        .bind(sqlx::types::Json(data))
        .execute(&mut **tx)
        .await?;
    crate::notifier::WAKE.notify_one();
    Ok(())
}

/// Remembers the client's IP for the withdrawal risk checklist (best effort).
pub async fn record_ip(pool: &PgPool, tenant_id: i64, user_id: i64, ip: Option<&str>) {
    let Some(ip) = ip.filter(|i| !i.is_empty() && *i != "unknown") else { return };
    let _ = sqlx::query(
        "INSERT INTO client_ips (tenant_id, user_id, ip) VALUES ($1, $2, $3)
         ON CONFLICT (tenant_id, user_id, ip) DO UPDATE SET last_seen = now(), hits = client_ips.hits + 1",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(ip)
    .execute(pool)
    .await;
}

/// 12 random bytes as lower-case hex (ids of deposit intents).
pub fn random_id(prefix: &str) -> String {
    let mut b = [0u8; 12];
    getrandom::fill(&mut b).expect("OS randomness unavailable");
    format!("{prefix}{}", crate::chain::hex_encode(&b))
}

pub fn clean_text(v: Option<&str>, max: usize) -> Option<String> {
    v.map(|s| s.trim().chars().filter(|c| !c.is_control()).take(max).collect::<String>()).filter(|s| !s.is_empty())
}
