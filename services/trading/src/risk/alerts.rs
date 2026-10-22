//! Risk alerts (`risk_alerts`): one open alert per key that escalates (amber → red), can be acknowledged, and is
//! resolved when its condition clears. New and escalated alerts go to the dealing stream (`{type:"risk_alert"}`) and,
//! red and amber ones, to Telegram (src/risk/telegram.rs).
//!
//! Keys: `exposure:{SYMBOL}`, `daily_loss`, `news:{event id}`, `reject:{code}:{SYMBOL}` (orders a limit refused or
//! would have refused), `toxic:{login}`, `routing:{suggestion id}`.

use serde_json::{Value, json};
use sqlx::Row;
use std::collections::HashSet;
use std::sync::Arc;

use crate::api::AppState;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Info,
    Amber,
    Red,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Info => "info",
            Level::Amber => "amber",
            Level::Red => "red",
        }
    }
    pub fn parse(s: &str) -> Level {
        match s {
            "red" => Level::Red,
            "amber" => Level::Amber,
            _ => Level::Info,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AlertSpec {
    pub key: String,
    pub kind: &'static str,
    pub level: Level,
    pub title: String,
    pub message: String,
    pub symbol: Option<String>,
    pub login: Option<i64>,
    pub data: Value,
}

pub fn row_json(r: &sqlx::postgres::PgRow) -> Value {
    let acked: Option<chrono::DateTime<chrono::Utc>> = r.get("acked_at");
    let resolved: Option<chrono::DateTime<chrono::Utc>> = r.get("resolved_at");
    json!({
        "id": r.get::<i64, _>("id").to_string(),
        "key": r.get::<String, _>("key"),
        "kind": r.get::<String, _>("kind"),
        "level": r.get::<String, _>("level"),
        "title": r.get::<String, _>("title"),
        "message": r.get::<String, _>("message"),
        "symbol": r.get::<Option<String>, _>("symbol"),
        "login": r.get::<Option<i64>, _>("login").map(|l| l.to_string()),
        "data": r.get::<sqlx::types::Json<Value>, _>("data").0,
        "count": r.get::<i32, _>("count"),
        "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updatedAt": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
        "ackedAt": acked,
        "ackedBy": r.get::<Option<String>, _>("acked_by"),
        "resolvedAt": resolved,
        "status": if resolved.is_some() { "resolved" } else if acked.is_some() { "acked" } else { "open" },
    })
}

fn push(st: &AppState, tenant_id: i64, alert: &Value) {
    if let Some(s) = st.hub.shared.streams.dealing(tenant_id) {
        let _ = s.send(Arc::from(json!({"type": "risk_alert", "alert": alert}).to_string()));
    }
}

fn telegram(st: &AppState, tenant_id: i64, a: &AlertSpec) {
    if a.level == Level::Info {
        return;
    }
    let slug = st.hub.shared.registry.get(tenant_id).map(|t| t.slug.clone()).unwrap_or_else(|| tenant_id.to_string());
    super::telegram::send(&format!("{tenant_id}:{}:{}", a.key, a.level.as_str()), &slug, a.level.as_str(), &a.title, &a.message);
}

/// Raises (or refreshes) the alert of `a.key`. New alerts and escalations are pushed and sent to Telegram; a refresh
/// at the same level only updates the stored numbers (at most once a minute). Returns the alert when it was pushed.
pub async fn raise(st: &AppState, tenant_id: i64, a: AlertSpec) -> Option<Value> {
    match raise_inner(st, tenant_id, &a).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(key = %a.key, error = %e, "risk alert not stored");
            None
        }
    }
}

async fn raise_inner(st: &AppState, tenant_id: i64, a: &AlertSpec) -> anyhow::Result<Option<Value>> {
    let pool = &st.pool;
    let open = sqlx::query("SELECT id, level, updated_at, message FROM risk_alerts WHERE tenant_id = $1 AND key = $2 AND resolved_at IS NULL").bind(tenant_id).bind(&a.key).fetch_optional(pool).await?;
    let Some(open) = open else {
        let r = sqlx::query(
            "INSERT INTO risk_alerts (tenant_id, key, kind, level, title, message, symbol, login, data) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
             ON CONFLICT (tenant_id, key) WHERE resolved_at IS NULL DO NOTHING RETURNING *",
        )
        .bind(tenant_id)
        .bind(&a.key)
        .bind(a.kind)
        .bind(a.level.as_str())
        .bind(&a.title)
        .bind(&a.message)
        .bind(&a.symbol)
        .bind(a.login)
        .bind(sqlx::types::Json(&a.data))
        .fetch_optional(pool)
        .await?;
        let Some(r) = r else { return Ok(None) };
        let v = row_json(&r);
        push(st, tenant_id, &v);
        telegram(st, tenant_id, a);
        return Ok(Some(v));
    };
    let id: i64 = open.get("id");
    let level = Level::parse(&open.get::<String, _>("level"));
    let updated: chrono::DateTime<chrono::Utc> = open.get("updated_at");
    if a.level > level {
        // escalation: re-opens an acknowledged alert
        let r = sqlx::query("UPDATE risk_alerts SET level = $2, title = $3, message = $4, data = $5, count = count + 1, updated_at = now(), acked_at = NULL, acked_by = NULL WHERE id = $1 RETURNING *")
            .bind(id)
            .bind(a.level.as_str())
            .bind(&a.title)
            .bind(&a.message)
            .bind(sqlx::types::Json(&a.data))
            .fetch_one(pool)
            .await?;
        let v = row_json(&r);
        push(st, tenant_id, &v);
        telegram(st, tenant_id, a);
        return Ok(Some(v));
    }
    let stale = chrono::Utc::now() - updated > chrono::Duration::seconds(60);
    if a.level < level || stale {
        let bump = if a.kind == "limit_reject" { 1 } else { 0 };
        let r = sqlx::query("UPDATE risk_alerts SET level = $2, title = $3, message = $4, data = $5, count = count + $6, updated_at = now() WHERE id = $1 RETURNING *")
            .bind(id)
            .bind(a.level.as_str())
            .bind(&a.title)
            .bind(&a.message)
            .bind(sqlx::types::Json(&a.data))
            .bind(bump)
            .fetch_one(pool)
            .await?;
        let v = row_json(&r);
        if a.level != level || a.kind == "limit_reject" {
            push(st, tenant_id, &v);
            return Ok(Some(v));
        }
    }
    Ok(None)
}

/// Resolves the open alerts of `kinds` whose key is not in `active` (their condition cleared).
pub async fn resolve_missing(st: &AppState, tenant_id: i64, kinds: &[&str], active: &HashSet<String>) {
    let keys: Vec<String> = active.iter().cloned().collect();
    let kinds: Vec<String> = kinds.iter().map(|k| k.to_string()).collect();
    match sqlx::query("UPDATE risk_alerts SET resolved_at = now(), updated_at = now() WHERE tenant_id = $1 AND kind = ANY($2) AND resolved_at IS NULL AND NOT (key = ANY($3)) RETURNING *").bind(tenant_id).bind(&kinds).bind(&keys).fetch_all(&st.pool).await {
        Ok(rows) => {
            for r in rows {
                push(st, tenant_id, &row_json(&r));
            }
        }
        Err(e) => tracing::warn!(error = %e, "risk alerts not resolved"),
    }
}

/// Resolves the open alert of `key` now (its condition was cleared by staff, e.g. a leverage cap disabled).
pub async fn resolve_key(st: &AppState, tenant_id: i64, key: &str) {
    match sqlx::query("UPDATE risk_alerts SET resolved_at = now(), updated_at = now() WHERE tenant_id = $1 AND key = $2 AND resolved_at IS NULL RETURNING *").bind(tenant_id).bind(key).fetch_all(&st.pool).await {
        Ok(rows) => {
            for r in rows {
                push(st, tenant_id, &row_json(&r));
            }
        }
        Err(e) => tracing::warn!(error = %e, %key, "risk alert not resolved"),
    }
}

/// Resolves alerts of `kinds` with no new occurrence for `quiet_secs` (order refusals, routing notices).
pub async fn resolve_quiet(st: &AppState, tenant_id: i64, kinds: &[&str], quiet_secs: i64) {
    let kinds: Vec<String> = kinds.iter().map(|k| k.to_string()).collect();
    if let Ok(rows) = sqlx::query("UPDATE risk_alerts SET resolved_at = now() WHERE tenant_id = $1 AND kind = ANY($2) AND resolved_at IS NULL AND updated_at < now() - make_interval(secs => $3) RETURNING *")
        .bind(tenant_id)
        .bind(&kinds)
        .bind(quiet_secs as f64)
        .fetch_all(&st.pool)
        .await
    {
        for r in rows {
            push(st, tenant_id, &row_json(&r));
        }
    }
}

pub async fn list(st: &AppState, tenant_id: i64, status: &str, limit: i64) -> anyhow::Result<Vec<Value>> {
    let cond = match status {
        "open" | "unacked" => "resolved_at IS NULL AND acked_at IS NULL",
        "active" => "resolved_at IS NULL",
        "acked" => "resolved_at IS NULL AND acked_at IS NOT NULL",
        "resolved" => "resolved_at IS NOT NULL",
        _ => "true",
    };
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT * FROM risk_alerts WHERE tenant_id = $1 AND {cond}
         ORDER BY (resolved_at IS NULL) DESC, CASE level WHEN 'red' THEN 0 WHEN 'amber' THEN 1 ELSE 2 END, updated_at DESC, id DESC LIMIT $2"
    )))
    .bind(tenant_id)
    .bind(limit.clamp(1, 1000))
    .fetch_all(&st.pool)
    .await?;
    Ok(rows.iter().map(row_json).collect())
}

/// Acknowledges alert `id` (open or not yet acknowledged). None = no such alert of the tenant.
pub async fn ack(st: &AppState, tenant_id: i64, id: i64, staff: &str) -> anyhow::Result<Option<(Value, Value)>> {
    let before = sqlx::query("SELECT * FROM risk_alerts WHERE id = $1 AND tenant_id = $2").bind(id).bind(tenant_id).fetch_optional(&st.pool).await?;
    let Some(before) = before else { return Ok(None) };
    let r = sqlx::query("UPDATE risk_alerts SET acked_at = COALESCE(acked_at, now()), acked_by = COALESCE(acked_by, $3) WHERE id = $1 AND tenant_id = $2 RETURNING *").bind(id).bind(tenant_id).bind(staff).fetch_one(&st.pool).await?;
    let v = row_json(&r);
    push(st, tenant_id, &v);
    Ok(Some((row_json(&before), v)))
}
