//! Module switches (D112): the Platform Owner turns the `academy` module off per broker in the gateway; this service's
//! client routes (catalogue, chapters, quizzes, exams, my certificates, glossary) then answer 403 `module_disabled`;
//! learner progress is kept, public certificate checks and the Back Office CMS stay.
//!
//! The switch is read from the gateway database (`tenant_features`, `GATEWAY_DATABASE_URL`, read-only),
//! cached 30 s per broker. No connection, an unknown broker, a missing key or a failed read count as on (as
//! in the Client Area); a failed read keeps the last answer it had.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use sqlx::PgPool;

use crate::api::AppState;

const TTL: Duration = Duration::from_secs(30);

static CACHE: LazyLock<Mutex<HashMap<(String, String), (Instant, bool)>>> = LazyLock::new(Default::default);

/// Stores an answer (also used by tests).
pub fn prime(tenant: &str, key: &str, on: bool) {
    let mut c = CACHE.lock().unwrap();
    if c.len() > 1000 {
        c.clear();
    }
    c.insert((tenant.to_string(), key.to_string()), (Instant::now(), on));
}

/// Whether module `key` is on for broker `tenant`.
pub async fn on(gateway: Option<&PgPool>, tenant: &str, key: &str) -> bool {
    let k = (tenant.to_string(), key.to_string());
    let hit = CACHE.lock().unwrap().get(&k).copied();
    if let Some((_, v)) = hit.filter(|(at, _)| at.elapsed() < TTL) {
        return v;
    }
    let Some(g) = gateway else { return true };
    let read = sqlx::query_scalar::<_, bool>(
        "SELECT COALESCE(tf.enabled, f.default_enabled) FROM feature_flags f JOIN tenants t ON t.slug = $1
         LEFT JOIN tenant_features tf ON tf.key = f.key AND tf.tenant_id = t.id WHERE f.key = $2",
    )
    .bind(tenant)
    .bind(key)
    .fetch_optional(g);
    let v = match tokio::time::timeout(Duration::from_secs(2), read).await {
        Ok(Ok(v)) => v.unwrap_or(true),
        _ => hit.map(|(_, v)| v).unwrap_or(true),
    };
    prime(tenant, key, v);
    v
}

/// Middleware for the client routes: 403 `module_disabled` while `academy` is off for the request's broker.
pub async fn gate(State(st): State<AppState>, req: Request, next: Next) -> Response {
    let tenant = req.headers().get("x-kalks-tenant").and_then(|v| v.to_str().ok()).map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).unwrap_or_else(|| "kalks".into());
    if !on(st.gateway.as_ref(), &tenant, "academy").await {
        let body = json!({"error": {"code": "module_disabled", "message": "The Academy isn't available on your account."}});
        return (StatusCode::FORBIDDEN, Json(body)).into_response();
    }
    next.run(req).await
}
