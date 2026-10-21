//! HTTP clients for the services the programme depends on: gateway (referral tree), trading engine (deals,
//! accounts, ledgers) and wallet (payout credits). All loopback, all with their own internal token.

use crate::money::{D, value_dec};
use crate::state::AppState;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};

/// Query string from simple values (digits, RFC 3339 times, lower-case words): percent-encodes anything else.
fn qs(pairs: &[(&str, String)]) -> String {
    let enc = |v: &str| -> String {
        v.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b':' => (b as char).to_string(),
                _ => format!("%{b:02X}"),
            })
            .collect()
    };
    pairs.iter().map(|(k, v)| format!("{k}={}", enc(v))).collect::<Vec<_>>().join("&")
}

// ---------------------------------------------------------------- gateway

#[derive(Clone, Debug, Deserialize)]
pub struct GwUser {
    pub id: i64,
    pub tenant: String,
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    #[serde(default)]
    pub country: String,
    pub referral_code: String,
    pub referred_by: Option<i64>,
    #[serde(default)]
    pub referral_campaign: Option<String>,
    #[serde(default)]
    pub kyc_status: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub email_verified: bool,
    pub created_at: DateTime<Utc>,
    pub changed_at: DateTime<Utc>,
    #[serde(default)]
    pub identity: Vec<String>,
    #[serde(default)]
    pub ips: Vec<String>,
    #[serde(default)]
    pub devices: Vec<String>,
}

pub async fn gateway_users(st: &AppState, since: Option<&str>, after_id: i64, limit: i64) -> anyhow::Result<Vec<GwUser>> {
    let mut q: Vec<(&str, String)> = vec![("after_id", after_id.to_string()), ("limit", limit.to_string())];
    if let Some(s) = since {
        q.push(("since", s.to_string()));
    }
    let res = st
        .http
        .get(format!("{}/v1/internal/referrals/users?{}", st.cfg.gateway_url, qs(&q)))
        .header("x-kalks-internal", &st.cfg.gateway_token)
        .send()
        .await?;
    if !res.status().is_success() {
        anyhow::bail!("gateway referral feed returned {}", res.status());
    }
    #[derive(Deserialize)]
    struct R {
        items: Vec<GwUser>,
    }
    Ok(res.json::<R>().await?.items)
}

// ---------------------------------------------------------------- trading engine

fn engine(st: &AppState, tenant: &str, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    rb.header("x-kalks-internal", &st.cfg.trading_token)
        .header("x-kalks-tenant", tenant)
        .header("x-kalks-staff-id", "ib-service")
        .header("x-kalks-staff-name", "IB%20service")
        .header("x-kalks-staff-role", "viewer")
}

/// A closing deal from `GET /v1/dealing/deals` (DeskDeal).
#[derive(Clone, Debug)]
pub struct EngineDeal {
    pub id: i64,
    pub login: i64,
    pub user_id: i64,
    pub symbol: String,
    pub side: String,
    pub volume: D,
    pub open_time: DateTime<Utc>,
    pub close_time: DateTime<Utc>,
    pub kind: String,
    pub reversed: bool,
    /// Kalks FX Options deal (`option` / `instrument` in the feed, or an option series symbol): volume = contracts.
    pub option: bool,
    /// Ledger label of the trade's origin (`deal_origin`): `copy`, `mam`, `pamm` or `engine`.
    pub origin: String,
}

/// Ledger label for the engine's `source` of a deal (the order source that opened the position): copy-trading
/// and MAM fills keep their own label; every other source, and a feed without one, is `engine`.
pub fn deal_origin(source: Option<&str>) -> &'static str {
    match source.map(str::trim) {
        Some(s) if s.eq_ignore_ascii_case("copy") => "copy",
        Some(s) if s.eq_ignore_ascii_case("mam") => "mam",
        Some(s) if s.eq_ignore_ascii_case("pamm") => "pamm",
        _ => "engine",
    }
}

/// Whether an engine deal JSON is a Kalks FX Options deal: the `option` object or `instrument: "option"`, with
/// the series code as a fallback for feeds that do not carry either.
pub fn is_option_deal(v: &Value) -> bool {
    v["option"].is_object() || v["instrument"].as_str() == Some("option") || v["symbol"].as_str().is_some_and(crate::model::is_option_series)
}

fn int(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

fn time(v: &Value) -> Option<DateTime<Utc>> {
    v.as_str().and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|t| t.with_timezone(&Utc))
}

pub fn parse_deal(v: &Value) -> Option<EngineDeal> {
    Some(EngineDeal {
        id: int(&v["id"])?,
        login: int(&v["login"])?,
        user_id: int(&v["clientId"])?,
        symbol: v["symbol"].as_str()?.to_string(),
        side: v["side"].as_str().unwrap_or("").to_string(),
        volume: value_dec(&v["volume"])?,
        open_time: time(&v["openTime"])?,
        close_time: time(&v["closeTime"])?,
        kind: v["kind"].as_str().unwrap_or("close").to_string(),
        reversed: v["reversed"].as_bool().unwrap_or(false),
        option: is_option_deal(v),
        origin: deal_origin(v["source"].as_str()).to_string(),
    })
}

/// Closing deals in `[from, to)`, newest first, at most `limit` (engine cap 2000).
pub async fn closing_deals(st: &AppState, tenant: &str, from: Option<DateTime<Utc>>, to: Option<DateTime<Utc>>, limit: i64) -> anyhow::Result<Vec<EngineDeal>> {
    let mut q: Vec<(&str, String)> = vec![("limit", limit.to_string())];
    if let Some(f) = from {
        q.push(("from", f.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)));
    }
    if let Some(t) = to {
        q.push(("to", t.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)));
    }
    let res = engine(st, tenant, st.http.get(format!("{}/v1/dealing/deals?{}", st.cfg.trading_url, qs(&q)))).send().await?;
    if !res.status().is_success() {
        anyhow::bail!("engine deals returned {}", res.status());
    }
    let v: Value = res.json().await?;
    Ok(v.as_array().map(|a| a.iter().filter_map(parse_deal).collect()).unwrap_or_default())
}

#[derive(Clone, Debug)]
pub struct AccountInfo {
    pub login: i64,
    pub user_id: i64,
    pub kind: String,
    pub group: String,
    pub cent: bool,
}

fn parse_account(a: &Value) -> Option<AccountInfo> {
    Some(AccountInfo {
        login: int(&a["login"])?,
        user_id: int(&a["userId"])?,
        kind: a["type"].as_str()?.to_string(),
        group: a["group"].as_str().unwrap_or("").to_string(),
        cent: a["cent"].as_bool().unwrap_or(false),
    })
}

pub async fn account(st: &AppState, tenant: &str, login: i64) -> anyhow::Result<Option<AccountInfo>> {
    let res = engine(st, tenant, st.http.get(format!("{}/v1/admin/accounts/{login}", st.cfg.trading_url))).send().await?;
    if res.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !res.status().is_success() {
        anyhow::bail!("engine account {login} returned {}", res.status());
    }
    let v: Value = res.json().await?;
    Ok(parse_account(&v["account"]))
}

pub async fn live_accounts_of(st: &AppState, tenant: &str, user_id: i64) -> anyhow::Result<Vec<AccountInfo>> {
    let res = engine(st, tenant, st.http.get(format!("{}/v1/admin/accounts?user_id={user_id}&type=live&limit=100", st.cfg.trading_url)))
        .send()
        .await?;
    if !res.status().is_success() {
        anyhow::bail!("engine accounts of {user_id} returned {}", res.status());
    }
    let v: Value = res.json().await?;
    Ok(v["items"].as_array().map(|a| a.iter().filter_map(parse_account).filter(|a| a.user_id == user_id).collect()).unwrap_or_default())
}

/// First funding of a trading account from its ledger: the oldest `deposit` / `transfer_in` on the balance
/// sub-ledger, in USD (cent accounts report USC).
pub async fn first_deposit(st: &AppState, tenant: &str, login: i64, user_id: i64) -> anyhow::Result<Option<(DateTime<Utc>, D)>> {
    let res = st
        .http
        .get(format!("{}/v1/accounts/{login}/ledger?limit=1000", st.cfg.trading_url))
        .header("x-kalks-internal", &st.cfg.trading_token)
        .header("x-kalks-tenant", tenant)
        .header("x-kalks-user-id", user_id.to_string())
        .send()
        .await?;
    if !res.status().is_success() {
        anyhow::bail!("engine ledger {login} returned {}", res.status());
    }
    let v: Value = res.json().await?;
    let mut best: Option<(i64, DateTime<Utc>, D)> = None;
    for it in v["items"].as_array().into_iter().flatten() {
        let kind = it["kind"].as_str().unwrap_or("");
        if !matches!(kind, "deposit" | "transfer_in") || it["subLedger"].as_str() != Some("balance") {
            continue;
        }
        let (Some(txn), Some(at), Some(mut amt)) = (int(&it["txn"]), time(&it["at"]), value_dec(&it["amount"])) else { continue };
        if amt <= D::ZERO {
            continue;
        }
        if it["currency"].as_str() == Some("USC") {
            amt /= crate::money::HUNDRED;
        }
        if best.as_ref().is_none_or(|b| txn < b.0) {
            best = Some((txn, at, amt));
        }
    }
    Ok(best.map(|(_, at, amt)| (at, amt)))
}

// ---------------------------------------------------------------- wallet

pub enum WalletOutcome {
    Credited { txn: String },
    /// Network error, timeout, 5xx: retry with the same idempotency key.
    Retry(String),
    /// 4xx: needs a person (e.g. idempotency conflict, validation).
    Fail(String),
}

pub async fn wallet_credit(st: &AppState, tenant: &str, key: &str, user_id: i64, amount: D, reference: &str, note: &str) -> WalletOutcome {
    let body = json!({
        "idempotency_key": key,
        "user_id": user_id,
        "currency": "USDT",
        "amount": amount.normalize().to_string(),
        "direction": "credit",
        "kind": "ib_payout",
        "ref": reference,
        "note": note,
    });
    let res = st
        .http
        .post(format!("{}/v1/wallets/transfers", st.cfg.wallet_url))
        .header("x-kalks-internal", &st.cfg.wallet_token)
        .header("x-kalks-tenant", tenant)
        .json(&body)
        .send()
        .await;
    match res {
        Err(e) => WalletOutcome::Retry(format!("wallet unreachable: {e}")),
        Ok(r) if r.status().is_success() => {
            let v: Value = r.json().await.unwrap_or(Value::Null);
            let txn = v["txn_id"].as_i64().map(|t| t.to_string()).or_else(|| v["txn_id"].as_str().map(str::to_string)).unwrap_or_default();
            WalletOutcome::Credited { txn }
        }
        Ok(r) => {
            let status = r.status();
            let v: Value = r.json().await.unwrap_or(Value::Null);
            let msg = format!("wallet {}: {} {}", status.as_u16(), v["error"]["code"].as_str().unwrap_or(""), v["error"]["message"].as_str().unwrap_or("")).trim().to_string();
            if status.is_server_error() || status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                // 404 here means the wallet route is not deployed yet: keep the payout pending
                WalletOutcome::Retry(msg)
            } else {
                WalletOutcome::Fail(msg)
            }
        }
    }
}
