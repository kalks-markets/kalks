//! Module switches (D112): the Platform Owner turns the `rewards` module off per broker in the gateway; the client's
//! rewards routes (points, redemptions, vouchers, cashback, promotions, bonuses, promo codes, contests) then answer
//! 403 `module_disabled`; banners and share cards stay, accruals and payouts keep running, the Back Office keeps its
//! routes.
//!
//! The broker's effective modules come from the gateway's `/v1/internal/tenants/{slug}` (`modules`), cached 30 s per
//! broker. A missing key, an unknown broker or an unreachable gateway count as on (as in the Client Area); a failed
//! lookup keeps the last map it had.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::{Map, Value, json};

use crate::state::AppState;

/// The module this service's client routes belong to.
pub const KEY: &str = "rewards";
const MESSAGE: &str = "Rewards aren't available on your account.";
const TTL: Duration = Duration::from_secs(30);

/// (gateway, broker) -> (fetched at, modules)
type Cache = HashMap<(String, String), (Instant, Map<String, Value>)>;
static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(Default::default);

/// Whether module `key` is on for broker `tenant`.
pub async fn on(st: &AppState, tenant: &str, key: &str) -> bool {
    let k = (st.cfg.gateway_url.clone(), tenant.to_string());
    let hit = CACHE.lock().unwrap().get(&k).cloned();
    let map = match hit {
        Some((at, m)) if at.elapsed() < TTL => m,
        stale => {
            let fetched = async {
                let r = st.http.get(format!("{}/v1/internal/tenants/{tenant}", st.cfg.gateway_url)).header("x-kalks-internal", &st.cfg.gateway_token).timeout(Duration::from_secs(2)).send().await.ok()?;
                if !r.status().is_success() {
                    return None;
                }
                r.json::<Value>().await.ok()?.get("modules").and_then(Value::as_object).cloned()
            };
            let m = fetched.await.or(stale.map(|(_, m)| m)).unwrap_or_default();
            let mut c = CACHE.lock().unwrap();
            if c.len() > 1000 {
                c.clear();
            }
            c.insert(k, (Instant::now(), m.clone()));
            m
        }
    };
    map.get(key) != Some(&Value::Bool(false))
}

/// Middleware for the client routes: 403 `module_disabled` while [`KEY`] is off for the request's broker.
pub async fn gate(State(st): State<AppState>, req: Request, next: Next) -> Response {
    let tenant = req.headers().get("x-kalks-tenant").and_then(|v| v.to_str().ok()).map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).unwrap_or_else(|| "kalks".into());
    if !on(&st, &tenant, KEY).await {
        return (StatusCode::FORBIDDEN, Json(json!({"error": {"code": "module_disabled", "message": MESSAGE}}))).into_response();
    }
    next.run(req).await
}
