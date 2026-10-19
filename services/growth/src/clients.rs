//! HTTP clients for the services growth depends on: gateway (profiles feed), trading engine (deals, accounts,
//! ledgers, history, bonus / credit / adjustment postings, demo contest accounts), wallet (cash rewards) and the
//! optional notifications service. All loopback, each with its own internal token.

use crate::money::{D, value_dec};
use crate::state::AppState;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};

/// Query string from simple values: percent-encodes anything but unreserved characters and ':'.
pub fn qs(pairs: &[(&str, String)]) -> String {
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

fn int(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

fn time(v: &Value) -> Option<DateTime<Utc>> {
    v.as_str().and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|t| t.with_timezone(&Utc))
}

fn ts(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

// ---------------------------------------------------------------- gateway

#[derive(Clone, Debug, Deserialize)]
pub struct GwUser {
    pub id: i64,
    pub tenant: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub first_name: String,
    #[serde(default)]
    pub last_name: String,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub referral_code: String,
    #[serde(default)]
    pub kyc_status: String,
    pub created_at: DateTime<Utc>,
    pub changed_at: DateTime<Utc>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub email_verified_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub kyc_verified_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_login_at: Option<DateTime<Utc>>,
    /// MM-DD
    #[serde(default)]
    pub birthday: Option<String>,
    #[serde(default)]
    pub marketing_consent: Option<bool>,
    #[serde(default)]
    pub utm_source: Option<String>,
    #[serde(default)]
    pub utm_campaign: Option<String>,
    /// The client who referred this one (referral missions).
    #[serde(default)]
    pub referred_by: Option<i64>,
}

/// The gateway's referral users feed (every client, keyset by (changed_at, id)).
pub async fn gateway_users(st: &AppState, since: Option<&str>, after_id: i64, limit: i64) -> anyhow::Result<Vec<GwUser>> {
    let mut q: Vec<(&str, String)> = vec![("after_id", after_id.to_string()), ("limit", limit.to_string())];
    if let Some(s) = since {
        q.push(("since", s.to_string()));
    }
    let res = st.http.get(format!("{}/v1/internal/referrals/users?{}", st.cfg.gateway_url, qs(&q))).header("x-kalks-internal", &st.cfg.gateway_token).send().await?;
    if !res.status().is_success() {
        anyhow::bail!("gateway users feed returned {}", res.status());
    }
    #[derive(Deserialize)]
    struct R {
        items: Vec<GwUser>,
    }
    Ok(res.json::<R>().await?.items)
}

/// Whether the client may trade Kalks FX Options (gateway suitability, O41: the options intro accepted). `Ok(false)`
/// for an unknown client; `Err` when the gateway can't answer (the caller must not guess).
pub async fn options_eligible(st: &AppState, tenant: &str, user_id: i64) -> anyhow::Result<bool> {
    let res = st
        .http
        .get(format!("{}/v1/internal/suitability/{user_id}?product=options", st.cfg.gateway_url))
        .header("x-kalks-internal", &st.cfg.gateway_token)
        .header("x-kalks-tenant", tenant)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?;
    if res.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(false);
    }
    if !res.status().is_success() {
        anyhow::bail!("gateway suitability returned {}", res.status());
    }
    let v: Value = res.json().await?;
    Ok(v["eligible"].as_bool().unwrap_or(false))
}

// ---------------------------------------------------------------- trading engine

/// Staff identity for engine admin calls. `finance` may post balance, credit and bonus.
fn engine_staff(st: &AppState, tenant: &str, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    rb.header("x-kalks-internal", &st.cfg.trading_token)
        .header("x-kalks-tenant", tenant)
        .header("x-kalks-staff-id", "growth-service")
        .header("x-kalks-staff-name", "Growth%20service")
        .header("x-kalks-staff-role", "finance")
}

fn engine_user(st: &AppState, tenant: &str, user_id: i64, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    rb.header("x-kalks-internal", &st.cfg.trading_token).header("x-kalks-tenant", tenant).header("x-kalks-user-id", user_id.to_string())
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
    pub profit: D,
    pub open_time: DateTime<Utc>,
    pub close_time: DateTime<Utc>,
    pub kind: String,
    pub reversed: bool,
    /// Kalks FX Options deal (volume = contracts): never earns points, cashback or bonus release; counts only in
    /// options contests (O36).
    pub option: bool,
    /// Options: opening premium of the closed contracts (account currency, unsigned) = |price P&L − exit cash|.
    pub premium: Option<D>,
    /// Options: the order-book fill this deal came from (absent on house-priced deals and older engines).
    pub fill_id: Option<String>,
}

pub fn parse_deal(v: &Value) -> Option<EngineDeal> {
    let option = crate::calc::is_option_deal(v);
    // DeskDeal: `priceProfit` = realised price P&L (cash + premium basis), `option.cash` = cash booked by the exit
    let premium = if option { value_dec(&v["priceProfit"]).zip(value_dec(&v["option"]["cash"])).map(|(p, c)| crate::calc::option_premium(p, c)) } else { None };
    let fill_id = v["option"]["fill"]["id"].as_str().or(v["option"]["fillId"].as_str()).map(str::trim).filter(|s| !s.is_empty()).map(|s| s.chars().take(64).collect());
    Some(EngineDeal {
        id: int(&v["id"])?,
        login: int(&v["login"])?,
        user_id: int(&v["clientId"])?,
        symbol: v["symbol"].as_str()?.to_string(),
        side: v["side"].as_str().unwrap_or("").to_string(),
        volume: value_dec(&v["volume"])?,
        profit: value_dec(&v["profit"]).unwrap_or(D::ZERO),
        open_time: time(&v["openTime"])?,
        close_time: time(&v["closeTime"])?,
        kind: v["kind"].as_str().unwrap_or("close").to_string(),
        reversed: v["reversed"].as_bool().unwrap_or(false),
        option,
        premium,
        fill_id,
    })
}

/// Closing deals in `[from, to)`, newest first, at most `limit` (engine cap 2000).
pub async fn closing_deals(st: &AppState, tenant: &str, login: Option<i64>, from: Option<DateTime<Utc>>, to: Option<DateTime<Utc>>, limit: i64) -> anyhow::Result<Vec<EngineDeal>> {
    let mut q: Vec<(&str, String)> = vec![("limit", limit.to_string())];
    if let Some(l) = login {
        q.push(("login", l.to_string()));
    }
    if let Some(f) = from {
        q.push(("from", ts(f)));
    }
    if let Some(t) = to {
        q.push(("to", ts(t)));
    }
    let res = engine_staff(st, tenant, st.http.get(format!("{}/v1/dealing/deals?{}", st.cfg.trading_url, qs(&q)))).send().await?;
    if !res.status().is_success() {
        anyhow::bail!("engine deals returned {}", res.status());
    }
    let v: Value = res.json().await?;
    Ok(v.as_array().map(|a| a.iter().filter_map(parse_deal).collect()).unwrap_or_default())
}

#[derive(Clone, Debug)]
pub struct Account {
    pub login: i64,
    pub user_id: i64,
    pub kind: String,
    pub group: String,
    pub cent: bool,
    pub currency: String,
    pub status: String,
    pub balance: D,
    pub credit: D,
    pub bonus: D,
    pub equity: D,
    /// Kalks FX Options at their mark (long +, short −), included in `equity` by the engine.
    pub option_value: D,
}

impl Account {
    /// Account currency → USD (cent accounts report USC).
    pub fn usd(&self, v: D) -> D {
        if self.cent { v / crate::money::HUNDRED } else { v }
    }
    /// Unrealised CFD P&L in USD: equity − balance − credit − bonus − option value. Options are excluded from
    /// contests: the engine's equity carries the options at their full mark (the premium already left the
    /// balance), so without this a bought option would look like a gain of its whole value.
    pub fn floating_usd(&self) -> D {
        self.usd(self.equity - self.balance - self.credit - self.bonus - self.option_value)
    }
    /// Equity without the options (USD): the contest starting equity and minimum-equity check.
    pub fn equity_ex_options_usd(&self) -> D {
        self.usd(self.equity - self.option_value)
    }
}

pub fn parse_account(a: &Value) -> Option<Account> {
    let d = |k: &str| value_dec(&a[k]).unwrap_or(D::ZERO);
    Some(Account {
        login: int(&a["login"])?,
        user_id: int(&a["userId"])?,
        kind: a["type"].as_str()?.to_string(),
        group: a["group"].as_str().unwrap_or("").to_string(),
        cent: a["cent"].as_bool().unwrap_or(false),
        currency: a["currency"].as_str().unwrap_or("USD").to_string(),
        status: a["status"].as_str().unwrap_or("active").to_string(),
        balance: d("balance"),
        credit: d("credit"),
        bonus: d("bonus"),
        equity: d("equity"),
        option_value: d("optionValue"),
    })
}

pub async fn account(st: &AppState, tenant: &str, login: i64) -> anyhow::Result<Option<Account>> {
    let res = engine_staff(st, tenant, st.http.get(format!("{}/v1/admin/accounts/{login}", st.cfg.trading_url))).send().await?;
    if res.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !res.status().is_success() {
        anyhow::bail!("engine account {login} returned {}", res.status());
    }
    let v: Value = res.json().await?;
    Ok(parse_account(&v["account"]))
}

/// Open positions of an account from the dealing desk (`GET /v1/dealing/positions?login=`): (symbol, side, open
/// time). Used for self-trade detection in options contests (the other leg may still be open).
pub async fn open_positions(st: &AppState, tenant: &str, login: i64) -> anyhow::Result<Vec<(String, String, DateTime<Utc>)>> {
    let res = engine_staff(st, tenant, st.http.get(format!("{}/v1/dealing/positions?login={login}", st.cfg.trading_url))).send().await?;
    if !res.status().is_success() {
        anyhow::bail!("engine positions of {login} returned {}", res.status());
    }
    let v: Value = res.json().await?;
    Ok(v.as_array()
        .into_iter()
        .flatten()
        .filter(|p| int(&p["login"]) == Some(login))
        .filter_map(|p| Some((p["symbol"].as_str()?.to_string(), p["side"].as_str()?.to_string(), time(&p["openTime"])?)))
        .collect())
}

/// A client's accounts (`kind` = live | demo | None for both).
pub async fn accounts_of(st: &AppState, tenant: &str, user_id: i64, kind: Option<&str>) -> anyhow::Result<Vec<Account>> {
    let mut q: Vec<(&str, String)> = vec![("user_id", user_id.to_string()), ("limit", "100".into())];
    if let Some(k) = kind {
        q.push(("type", k.to_string()));
    }
    let res = engine_staff(st, tenant, st.http.get(format!("{}/v1/admin/accounts?{}", st.cfg.trading_url, qs(&q)))).send().await?;
    if !res.status().is_success() {
        anyhow::bail!("engine accounts of {user_id} returned {}", res.status());
    }
    let v: Value = res.json().await?;
    Ok(v["items"].as_array().map(|a| a.iter().filter_map(parse_account).filter(|a| a.user_id == user_id).collect()).unwrap_or_default())
}

#[derive(Clone, Debug)]
pub struct LedgerItem {
    pub txn: i64,
    pub kind: String,
    pub sub_ledger: String,
    /// USD (USC converted).
    pub amount: D,
    pub reason_code: Option<String>,
    pub at: DateTime<Utc>,
}

/// Ledger lines of an account since `from` (newest first, up to 1000).
pub async fn ledger(st: &AppState, tenant: &str, login: i64, user_id: i64, from: DateTime<Utc>) -> anyhow::Result<Vec<LedgerItem>> {
    let url = format!("{}/v1/accounts/{login}/ledger?{}", st.cfg.trading_url, qs(&[("from", ts(from)), ("limit", "1000".into())]));
    let res = engine_user(st, tenant, user_id, st.http.get(url)).send().await?;
    if !res.status().is_success() {
        anyhow::bail!("engine ledger {login} returned {}", res.status());
    }
    let v: Value = res.json().await?;
    Ok(v["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|it| {
            let mut amount = value_dec(&it["amount"])?;
            if it["currency"].as_str() == Some("USC") {
                amount /= crate::money::HUNDRED;
            }
            Some(LedgerItem {
                txn: int(&it["txn"])?,
                kind: it["kind"].as_str()?.to_string(),
                sub_ledger: it["subLedger"].as_str().unwrap_or("").to_string(),
                amount,
                reason_code: it["reasonCode"].as_str().map(str::to_string),
                at: time(&it["at"])?,
            })
        })
        .collect())
}

/// Closed deals (exit side) of an account in `[from, to)` from the client history API.
pub async fn history(st: &AppState, tenant: &str, login: i64, user_id: i64, from: Option<DateTime<Utc>>, to: Option<DateTime<Utc>>) -> anyhow::Result<Option<Vec<Value>>> {
    let mut q: Vec<(&str, String)> = vec![("limit", "1000".into())];
    if let Some(f) = from {
        q.push(("from", ts(f)));
    }
    if let Some(t) = to {
        q.push(("to", ts(t)));
    }
    let res = engine_user(st, tenant, user_id, st.http.get(format!("{}/v1/accounts/{login}/history?{}", st.cfg.trading_url, qs(&q)))).send().await?;
    if res.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !res.status().is_success() {
        anyhow::bail!("engine history {login} returned {}", res.status());
    }
    let v: Value = res.json().await?;
    Ok(Some(v["deals"].as_array().cloned().unwrap_or_default()))
}

pub enum EngineOutcome {
    Booked,
    /// Network error, timeout, 5xx: retry with the same key.
    Retry(String),
    /// 4xx other than a duplicate key: needs a person.
    Fail(String),
}

/// `POST /v1/admin/accounts/{login}/balance` with an idempotency key. `amount` is USD; cent accounts get × 100.
pub async fn post_balance(st: &AppState, tenant: &str, login: i64, cent: bool, kind: &str, amount_usd: D, key: &str, reason_code: &str, note: &str) -> EngineOutcome {
    let amount = if cent { amount_usd * crate::money::HUNDRED } else { amount_usd };
    let body = json!({"type": kind, "amount": amount.normalize().to_string(), "idempotencyKey": key, "reasonCode": reason_code, "note": note});
    let res = engine_staff(st, tenant, st.http.post(format!("{}/v1/admin/accounts/{login}/balance", st.cfg.trading_url))).json(&body).send().await;
    match res {
        Err(e) => EngineOutcome::Retry(format!("engine unreachable: {e}")),
        Ok(r) if r.status().is_success() => EngineOutcome::Booked,
        Ok(r) => {
            let status = r.status();
            let v: Value = r.json().await.unwrap_or(Value::Null);
            let code = v["error"]["code"].as_str().unwrap_or("").to_string();
            let msg = format!("engine {}: {} {}", status.as_u16(), code, v["error"]["message"].as_str().unwrap_or("")).trim().to_string();
            if code == "duplicate_idempotency_key" {
                EngineOutcome::Booked
            } else if status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                EngineOutcome::Retry(msg)
            } else {
                EngineOutcome::Fail(msg)
            }
        }
    }
}

/// Opens a demo account for a contest. Returns (account, credentials JSON).
pub async fn open_demo(st: &AppState, tenant: &str, user_id: i64, group: &str, balance: D, name: &str) -> Result<(Account, Value), String> {
    let body = json!({"userId": user_id, "type": "demo", "group": group, "initialBalance": balance.normalize().to_string(), "name": name});
    let res = engine_user(st, tenant, user_id, st.http.post(format!("{}/v1/accounts", st.cfg.trading_url))).json(&body).send().await.map_err(|e| format!("The trading engine is unavailable ({e})."))?;
    let status = res.status();
    let v: Value = res.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        return Err(v["error"]["message"].as_str().unwrap_or("The contest account could not be opened.").to_string());
    }
    let acc = parse_account(&v["account"]).ok_or("Unexpected engine response.")?;
    Ok((acc, v["credentials"].clone()))
}

// ---------------------------------------------------------------- wallet

pub enum WalletOutcome {
    Credited { txn: String },
    Retry(String),
    Fail(String),
}

pub async fn wallet_credit(st: &AppState, tenant: &str, key: &str, user_id: i64, amount: D, kind: &str, reference: &str, note: &str) -> WalletOutcome {
    let body = json!({
        "idempotency_key": key, "user_id": user_id, "currency": "USDT", "amount": amount.normalize().to_string(),
        "direction": "credit", "kind": kind, "ref": reference, "note": note,
    });
    let res = st
        .http
        .post(format!("{}/v1/wallets/transfers", st.cfg.wallet_url))
        .header("x-kalks-internal", &st.cfg.wallet_token)
        .header("x-kalks-tenant", tenant)
        .header("x-kalks-service", "growth")
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
                WalletOutcome::Retry(msg)
            } else {
                WalletOutcome::Fail(msg)
            }
        }
    }
}

// ---------------------------------------------------------------- notifications (optional)

/// Best effort: the notifications service may not exist yet. Never fails the caller.
pub fn notify(st: &AppState, tenant: &str, user_id: i64, kind: &str, title: String, body: String, link: &str) {
    notify_dedupe(st, tenant, user_id, kind, title, body, link, None);
}

/// [`notify`] with a dedupe key (a repeated key for the same client is a no-op in the support service), in-app only.
/// The support stream pushes it to the client's open Client Area tabs, which reload their rewards views on it.
#[allow(clippy::too_many_arguments)]
pub fn notify_dedupe(st: &AppState, tenant: &str, user_id: i64, kind: &str, title: String, body: String, link: &str, dedupe: Option<String>) {
    let st = st.clone();
    let mut payload = json!({"userId": user_id, "type": kind, "title": title, "body": body, "link": link});
    if let Some(d) = dedupe {
        payload["dedupeKey"] = json!(d);
        payload["email"] = json!(false);
    }
    let tenant = tenant.to_string();
    tokio::spawn(async move {
        let r = st
            .http
            .post(format!("{}/v1/notify", st.cfg.notify_url))
            .header("x-kalks-internal", &st.cfg.notify_token)
            .header("x-kalks-tenant", tenant)
            .header("x-kalks-service", "growth")
            .timeout(std::time::Duration::from_secs(3))
            .json(&payload)
            .send()
            .await;
        if let Err(e) = r {
            tracing::debug!(error = %e, "notify skipped");
        }
    });
}
