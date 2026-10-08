//! Module switch `circle` (Track 2): the Platform Owner turns Kalks Circle off per broker in the gateway; that
//! broker's clients then get 403 `module_disabled` on every client route (Back Office routes stay).
//!
//! The broker's effective modules come from the gateway's `/v1/internal/tenants/{slug}` (`modules`), cached 30 s
//! per broker. A missing key, an unknown broker or an unreachable gateway count as on (as in the Client Area); a
//! failed lookup keeps the last map it had. The same lookup carries the broker's blocked countries when the gateway
//! provides them (`blockedCountries`).

use std::time::Duration;

use axum::Json;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::state::AppState;

pub const KEY: &str = "circle";
const MESSAGE: &str = "Kalks Circle isn't available on your account.";
const TTL: Duration = Duration::from_secs(30);

/// The gateway's view of a broker (`modules`, optional `blockedCountries`), cached.
pub async fn tenant_info(st: &AppState, tenant: &str) -> Value {
    let ck = format!("tenant:{tenant}");
    if let Some(v) = st.cache.get(&ck, TTL) {
        return v;
    }
    let fetched = async {
        let r = st.http.get(format!("{}/v1/internal/tenants/{tenant}", st.cfg.gateway_url)).header("x-kalks-internal", &st.cfg.gateway_token).timeout(Duration::from_secs(2)).send().await.ok()?;
        if !r.status().is_success() {
            return None;
        }
        r.json::<Value>().await.ok()
    };
    let v = fetched.await.or_else(|| st.cache.stale(&ck)).unwrap_or_else(|| json!({}));
    st.cache.put(&ck, v.clone());
    v
}

/// Whether module `key` is on for broker `tenant`.
pub async fn on(st: &AppState, tenant: &str, key: &str) -> bool {
    tenant_info(st, tenant).await.pointer(&format!("/modules/{key}")) != Some(&Value::Bool(false))
}

/// The broker's own blocked countries (lower-case ISO alpha-2), when the gateway lists them.
pub async fn blocked_countries(st: &AppState, tenant: &str) -> Vec<String> {
    tenant_info(st, tenant).await["blockedCountries"].as_array().into_iter().flatten().filter_map(Value::as_str).map(|c| c.trim().to_ascii_lowercase()).collect()
}

pub fn tenant_header(req: &Request) -> String {
    req.headers().get("x-kalks-tenant").and_then(|v| v.to_str().ok()).map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).unwrap_or_else(|| "kalks".into())
}

/// Middleware for the client routes: 403 `module_disabled` while [`KEY`] is off for the request's broker.
pub async fn gate(State(st): State<AppState>, req: Request, next: Next) -> Response {
    let tenant = tenant_header(&req);
    if !on(&st, &tenant, KEY).await {
        return (StatusCode::FORBIDDEN, Json(json!({"error": {"code": "module_disabled", "message": MESSAGE}}))).into_response();
    }
    next.run(req).await
}
