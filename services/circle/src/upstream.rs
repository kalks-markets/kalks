//! Calls to the other Kalks services. Circle never copies trading data: accounts, positions and deals come from the
//! trading engine, prices from market-data, certificates from the Academy, bell notifications go through the
//! support service and Rewards points through the growth service.

use crate::state::AppState;
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::Duration;

fn engine_user(st: &AppState, tenant: &str, user: i64, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    rb.header("x-kalks-internal", &st.cfg.trading_token).header("x-kalks-tenant", tenant).header("x-kalks-user-id", user.to_string()).timeout(Duration::from_secs(8))
}

fn ts(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// The client's trading accounts (engine `GET /v1/accounts`).
pub async fn accounts(st: &AppState, tenant: &str, user: i64) -> anyhow::Result<Vec<Value>> {
    let r = engine_user(st, tenant, user, st.http.get(format!("{}/v1/accounts", st.cfg.trading_url))).send().await?;
    if !r.status().is_success() {
        anyhow::bail!("engine accounts returned {}", r.status());
    }
    let v: Value = r.json().await?;
    Ok(v["accounts"].as_array().cloned().unwrap_or_default())
}

/// One account with its open positions (engine `GET /v1/accounts/{login}`); None = not this client's account.
pub async fn account_detail(st: &AppState, tenant: &str, user: i64, login: i64) -> anyhow::Result<Option<Value>> {
    let r = engine_user(st, tenant, user, st.http.get(format!("{}/v1/accounts/{login}", st.cfg.trading_url))).send().await?;
    if matches!(r.status().as_u16(), 403 | 404) {
        return Ok(None);
    }
    if !r.status().is_success() {
        anyhow::bail!("engine account {login} returned {}", r.status());
    }
    let v: Value = r.json().await?;
    Ok((!v["account"].is_null()).then_some(v))
}

/// Deals of an account (engine `GET /v1/accounts/{login}/history`), newest first.
pub async fn history(st: &AppState, tenant: &str, user: i64, login: i64, from: Option<DateTime<Utc>>, limit: i64) -> anyhow::Result<Option<Vec<Value>>> {
    let mut url = format!("{}/v1/accounts/{login}/history?limit={}", st.cfg.trading_url, limit.clamp(1, 1000));
    if let Some(f) = from {
        url.push_str(&format!("&from={}", ts(f)));
    }
    let r = engine_user(st, tenant, user, st.http.get(url)).send().await?;
    if matches!(r.status().as_u16(), 403 | 404) {
        return Ok(None);
    }
    if !r.status().is_success() {
        anyhow::bail!("engine history {login} returned {}", r.status());
    }
    let v: Value = r.json().await?;
    Ok(Some(v["deals"].as_array().cloned().unwrap_or_default()))
}

/// The client's copy-trading master / PAMM manager record (engine social module), when live.
pub async fn master_me(st: &AppState, tenant: &str, user: i64) -> anyhow::Result<Option<Value>> {
    let r = engine_user(st, tenant, user, st.http.get(format!("{}/v1/social/master/me", st.cfg.trading_url))).send().await?;
    if !r.status().is_success() {
        anyhow::bail!("engine master/me returned {}", r.status());
    }
    let v: Value = r.json().await?;
    Ok(v.get("master").filter(|m| m.is_object()).cloned())
}

/// Live quotes (market-data `GET /v1/quotes?symbols=`), cached 2 s per symbol set.
pub async fn quotes(st: &AppState, symbols: &[String]) -> HashMap<String, Value> {
    let mut syms: Vec<String> = symbols.iter().map(|s| s.to_uppercase()).filter(|s| is_symbol(s)).collect();
    syms.sort();
    syms.dedup();
    syms.truncate(50);
    if syms.is_empty() {
        return HashMap::new();
    }
    let ck = format!("quotes:{}", syms.join(","));
    let v = match st.cache.get(&ck, Duration::from_secs(2)) {
        Some(v) => v,
        None => {
            let got = async {
                let r = st.http.get(format!("{}/v1/quotes?symbols={}", st.cfg.market_data_url, syms.join(","))).timeout(Duration::from_secs(4)).send().await.ok()?;
                if !r.status().is_success() {
                    return None;
                }
                r.json::<Value>().await.ok()
            };
            let v = got.await.or_else(|| st.cache.stale(&ck)).unwrap_or_else(|| json!({}));
            st.cache.put(&ck, v.clone());
            v
        }
    };
    v.as_object().map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default()
}

/// Mid / last price of a quote.
pub fn price_of(q: &Value) -> Option<f64> {
    let bid = q["bid"].as_f64();
    let ask = q["ask"].as_f64();
    match (bid, ask) {
        (Some(b), Some(a)) if b > 0.0 && a > 0.0 => Some((a + b) / 2.0),
        _ => q["last"].as_f64().filter(|x| *x > 0.0),
    }
}

/// `$CASHTAG` symbols: 2..16 upper-case letters / digits (XAUUSD, EURUSD, NAS100, BTCUSD, AAPL).
pub fn is_symbol(s: &str) -> bool {
    (2..=16).contains(&s.len()) && s.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '.') && s.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// The Kalks instrument catalogue (market-data `GET /v1/instruments`), cached 1 hour: symbol -> name.
pub async fn instruments(st: &AppState) -> HashMap<String, String> {
    let ck = "instruments";
    let v = match st.cache.get(ck, Duration::from_secs(3600)) {
        Some(v) => v,
        None => {
            let got = async {
                let r = st.http.get(format!("{}/v1/instruments", st.cfg.market_data_url)).timeout(Duration::from_secs(5)).send().await.ok()?;
                if !r.status().is_success() {
                    return None;
                }
                r.json::<Value>().await.ok()
            };
            match got.await {
                Some(v) => {
                    st.cache.put(ck, v.clone());
                    v
                }
                None => st.cache.stale(ck).unwrap_or(Value::Null),
            }
        }
    };
    let list = v.as_array().cloned().or_else(|| v["instruments"].as_array().cloned()).unwrap_or_default();
    list.iter().filter_map(|i| Some((i["symbol"].as_str()?.to_string(), i["name"].as_str().unwrap_or("").to_string()))).collect()
}

/// The client's Academy certificates (academy `GET /v1/me/certificates`).
pub async fn certificates(st: &AppState, tenant: &str, user: i64) -> anyhow::Result<Vec<Value>> {
    let r = st
        .http
        .get(format!("{}/v1/me/certificates", st.cfg.academy_url))
        .header("x-kalks-internal", &st.cfg.academy_token)
        .header("x-kalks-tenant", tenant)
        .header("x-kalks-user-id", user.to_string())
        .timeout(Duration::from_secs(4))
        .send()
        .await?;
    if !r.status().is_success() {
        anyhow::bail!("academy certificates returned {}", r.status());
    }
    let v: Value = r.json().await?;
    Ok(v["certificates"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| json!({"code": c["code"], "phase": c["phase"], "title": c["phase_title"], "level": c["level"], "issuedAt": c["issued_at"], "verifyUrl": c["verify_url"]}))
        .collect())
}

/// One bell notification through the support service (`POST /v1/notify`, in-app only).
pub async fn notify(st: &AppState, tenant: &str, body: &Value) -> anyhow::Result<()> {
    let r = st
        .http
        .post(format!("{}/v1/notify", st.cfg.notify_url))
        .header("x-kalks-internal", &st.cfg.notify_token)
        .header("x-kalks-tenant", tenant)
        .header("x-kalks-service", "circle")
        .json(body)
        .timeout(Duration::from_secs(6))
        .send()
        .await?;
    if !r.status().is_success() {
        anyhow::bail!("support notify returned {}", r.status());
    }
    Ok(())
}

/// Rewards points through the growth service (`POST /v1/growth/admin/points/adjust` as the Circle service).
pub async fn growth_points(st: &AppState, tenant: &str, user: i64, points: i64, note: &str) -> Result<(), (bool, String)> {
    let r = st
        .http
        .post(format!("{}/v1/growth/admin/points/adjust", st.cfg.growth_url))
        .header("x-kalks-internal", &st.cfg.growth_token)
        .header("x-kalks-tenant", tenant)
        .header("x-kalks-staff-id", "circle-service")
        .header("x-kalks-staff-name", "Kalks%20Circle")
        .header("x-kalks-staff-role", "finance")
        .header("x-kalks-staff-perms", "marketing.read,marketing.approve")
        .json(&json!({"userId": user, "points": points, "note": note}))
        .timeout(Duration::from_secs(8))
        .send()
        .await
        .map_err(|e| (true, e.to_string()))?;
    let s = r.status();
    if s.is_success() {
        return Ok(());
    }
    let text = r.text().await.unwrap_or_default().chars().take(300).collect::<String>();
    // 5xx / 429 retry; other 4xx need a person
    Err((s.is_server_error() || s.as_u16() == 429, format!("growth returned {s}: {text}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbols_and_prices() {
        for ok in ["XAUUSD", "EURUSD", "NAS100", "BTCUSD", "AAPL", "BRK.B"] {
            assert!(is_symbol(ok), "{ok}");
        }
        for bad in ["X", "xauusd", "1ABC", "TOOLONGSYMBOLNAME123", "EUR/USD"] {
            assert!(!is_symbol(bad), "{bad}");
        }
        assert_eq!(price_of(&json!({"bid": 1.0, "ask": 2.0})), Some(1.5));
        assert_eq!(price_of(&json!({"last": 5.0})), Some(5.0));
        assert_eq!(price_of(&json!({})), None);
    }
}
