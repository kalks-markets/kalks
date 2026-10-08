//! Module switches (D112): the Platform Owner turns a module off per broker in the gateway. Off `api` = the public
//! API and webhooks answer 403 `module_disabled`; off `algo` = running strategies ignore their signals (paused,
//! never stopped or deleted) until the module is back on.
//!
//! The broker's effective modules come from the gateway's `/v1/internal/tenants/{slug}` (`modules`), cached 30 s per
//! broker. A missing key, an unknown broker or an unreachable gateway count as on (as in the Client Area); a failed
//! lookup keeps the last map it had.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use serde_json::{Map, Value};

use crate::error::ApiError;
use crate::state::AppState;

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

/// 403 `module_disabled` when module `key` is off for broker `tenant`.
pub async fn require(st: &AppState, tenant: &str, key: &str) -> Result<(), ApiError> {
    if on(st, tenant, key).await {
        return Ok(());
    }
    let message = if key == "api" { "The API and webhooks aren't available for this account." } else { "This feature isn't available on your account." };
    Err(ApiError::coded(StatusCode::FORBIDDEN, "module_disabled", message))
}
