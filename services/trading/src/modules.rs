//! Module switches (D112): the Platform Owner turns a module off per broker in the gateway. The engine refuses the
//! client actions that start or grow participation in an off module (new copy subscriptions / funds, PAMM funds and
//! investments, MAM links and managers, new option orders) with 403 `module_disabled`, in the API handlers before
//! anything reaches a shard, so the event log and its replay never depend on it. Exits (stop, redeem, revoke,
//! reduce-only) stay open, and running copy / PAMM / MAM accounts keep working.
//!
//! The broker's effective modules come from the gateway's `/v1/internal/tenants/{slug}` (`modules`), cached 30 s per
//! broker. A missing key, an unknown broker or an unreachable gateway count as on (as in the Client Area); a failed
//! lookup keeps the last map it had.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Map, Value};

use crate::api::{ApiError, ApiResult, AppState};

const TTL: Duration = Duration::from_secs(30);

static CACHE: LazyLock<Mutex<HashMap<String, (Instant, Map<String, Value>)>>> = LazyLock::new(Default::default);

fn cached(slug: &str, fresh_only: bool) -> Option<Map<String, Value>> {
    let c = CACHE.lock().unwrap();
    c.get(slug).filter(|(at, _)| !fresh_only || at.elapsed() < TTL).map(|(_, m)| m.clone())
}

/// Stores a broker's module map (also used by tests).
pub fn prime(slug: &str, modules: Map<String, Value>) {
    let mut c = CACHE.lock().unwrap();
    if c.len() > 1000 {
        c.clear();
    }
    c.insert(slug.to_string(), (Instant::now(), modules));
}

/// Whether module `key` is on for broker `slug`.
pub async fn on(st: &AppState, slug: &str, key: &str) -> bool {
    let map = match cached(slug, true) {
        Some(m) => m,
        None => {
            let got = tokio::time::timeout(Duration::from_secs(2), st.gateway.call("GET", &format!("/v1/internal/tenants/{slug}"), None)).await;
            let m = match got {
                Ok(Ok((200, v))) => v.get("modules").and_then(Value::as_object).cloned().unwrap_or_default(),
                _ => cached(slug, false).unwrap_or_default(),
            };
            prime(slug, m.clone());
            m
        }
    };
    map.get(key) != Some(&Value::Bool(false))
}

/// 403 `module_disabled` when module `key` is off for broker `slug`.
pub async fn require(st: &AppState, slug: &str, key: &str) -> ApiResult<()> {
    if on(st, slug, key).await {
        return Ok(());
    }
    Err(ApiError::Status { status: 403, code: "module_disabled", message: "This feature isn't available on your account.".into() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_primed_map_answers_and_missing_keys_are_on() {
        prime("qa-modules", serde_json::json!({"options": false, "prop": true}).as_object().unwrap().clone());
        let m = cached("qa-modules", true).unwrap();
        assert_eq!(m.get("options"), Some(&Value::Bool(false)));
        assert!(m.get("news").is_none());
        assert!(cached("qa-unknown", true).is_none());
    }
}
