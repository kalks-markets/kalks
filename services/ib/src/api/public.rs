//! Link click tracking (called server-side by the CRM when a visitor lands with `?ref=CODE&c=CAMPAIGN`) and
//! events pushed by other services (wallet deposits, copy/PAMM lot allocations).

use super::Tenant;
use crate::clients::AccountInfo;
use crate::deals::{self, DealInput, Outcome};
use crate::error::{ApiError, ApiResult, invalid};
use crate::money::{D, ZERO, de_dec};
use crate::state::AppState;
use axum::Json;
use axum::extract::State;
use chrono::{DateTime, Utc};
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::Sha256;
use sqlx::Row;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClickReq {
    code: String,
    #[serde(default)]
    campaign: Option<String>,
    #[serde(default)]
    ip: String,
    #[serde(default)]
    user_agent: String,
    #[serde(default)]
    referer: Option<String>,
    #[serde(default)]
    landing: Option<String>,
}

pub fn clean_code(raw: &str) -> Option<String> {
    let v = raw.trim();
    ((3..=24).contains(&v.len()) && v.chars().all(|c| c.is_ascii_alphanumeric())).then(|| v.to_uppercase())
}

pub fn clean_slug(raw: &str) -> Option<String> {
    let v = raw.trim().to_lowercase();
    ((1..=40).contains(&v.len()) && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')).then_some(v)
}

fn visitor(st: &AppState, ip: &str, ua: &str) -> String {
    let key = if st.cfg.internal_token.is_empty() { b"kalks-ib-dev".to_vec() } else { st.cfg.internal_token.as_bytes().to_vec() };
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(&key).expect("hmac key");
    mac.update(ip.as_bytes());
    mac.update(b"|");
    mac.update(ua.as_bytes());
    mac.finalize().into_bytes().iter().take(16).map(|b| format!("{b:02x}")).collect()
}

/// `POST /v1/ib/clicks {code, campaign?, ip, userAgent, referer?, landing?}` → `{valid, code, campaign}`.
/// A click is unique per visitor (IP + user agent hash) per link per 24 h. Paused campaigns still attribute
/// the code but not the campaign.
pub async fn click(State(st): State<AppState>, Tenant(tenant): Tenant, Json(r): Json<ClickReq>) -> ApiResult<Json<Value>> {
    let Some(code) = clean_code(&r.code) else { return Ok(Json(json!({"valid": false}))) };
    let Some(ib) = sqlx::query_scalar::<_, i64>("SELECT user_id FROM members WHERE tenant = $1 AND referral_code = $2 AND status = 'active'")
        .bind(&tenant)
        .bind(&code)
        .fetch_optional(&st.pool)
        .await?
    else {
        return Ok(Json(json!({"valid": false})));
    };
    let campaign = match r.campaign.as_deref().and_then(clean_slug) {
        Some(slug) => sqlx::query("SELECT id, slug FROM campaigns WHERE tenant = $1 AND user_id = $2 AND slug = $3 AND active").bind(&tenant).bind(ib).bind(&slug).fetch_optional(&st.pool).await?,
        None => None,
    };
    let campaign_id: Option<i64> = campaign.as_ref().map(|c| c.get("id"));
    let v = visitor(&st, r.ip.trim(), r.user_agent.trim());
    let seen: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM clicks WHERE tenant = $1 AND user_id = $2 AND visitor = $3 AND campaign_id IS NOT DISTINCT FROM $4 AND at > now() - interval '24 hours' LIMIT 1",
    )
    .bind(&tenant)
    .bind(ib)
    .bind(&v)
    .bind(campaign_id)
    .fetch_optional(&st.pool)
    .await?;
    let trunc = |s: &Option<String>| s.as_deref().map(|x| x.chars().take(300).collect::<String>());
    sqlx::query("INSERT INTO clicks (tenant, user_id, campaign_id, visitor, unique_click, landing, referer) VALUES ($1,$2,$3,$4,$5,$6,$7)")
        .bind(&tenant)
        .bind(ib)
        .bind(campaign_id)
        .bind(&v)
        .bind(seen.is_none())
        .bind(trunc(&r.landing))
        .bind(trunc(&r.referer))
        .execute(&st.pool)
        .await?;
    Ok(Json(json!({"valid": true, "code": code, "campaign": campaign.map(|c| c.get::<String, _>("slug")), "unique": seen.is_none()})))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepositReq {
    user_id: i64,
    #[serde(deserialize_with = "de_dec")]
    amount: D,
    /// `USD` (default), `USDT` (1:1) or `USC` (cents).
    #[serde(default)]
    currency: Option<String>,
    #[serde(default)]
    at: Option<DateTime<Utc>>,
    /// Stable per deposit on the caller's side (the wallet sends `wallet:deposit:<tenant>:<txn>`).
    #[serde(default)]
    idempotency_key: Option<String>,
    #[serde(default)]
    source: Option<String>,
}

/// Amount in USD for a deposit currency (USDT is USD 1:1, USC is cents); `None` for anything else.
pub fn deposit_usd(amount: D, currency: Option<&str>) -> Option<D> {
    match currency.map(|c| c.trim().to_ascii_uppercase()).as_deref() {
        None | Some("") | Some("USD") | Some("USDT") => Some(amount),
        Some("USC") => Some(amount / crate::money::HUNDRED),
        _ => None,
    }
}

/// `POST /v1/ib/events/deposit {userId, amount, currency?, at?, idempotencyKey?, source?}`: a confirmed
/// real-money deposit (the wallet: on-chain deposit credited or a Back Office external deposit). The earliest
/// deposit is the client's first deposit for CPA (D57), whether it came from an event or from the engine
/// ledger, so wallet money later moved to a trading account (`transfer_in`) is never counted twice. With an
/// `idempotencyKey` a repeat returns `duplicate` and changes nothing; the same key with another user, amount or
/// time is `409 exists`. 404 while the client is not mirrored from the gateway yet (the caller retries).
pub async fn deposit_event(State(st): State<AppState>, Tenant(tenant): Tenant, Json(r): Json<DepositReq>) -> ApiResult<Json<Value>> {
    if r.amount <= ZERO {
        return Err(invalid("amount", "Amount must be positive."));
    }
    let currency = r.currency.as_deref().map(|c| c.trim().to_ascii_uppercase()).filter(|c| !c.is_empty()).unwrap_or_else(|| "USD".into());
    let Some(usd) = deposit_usd(r.amount, Some(&currency)) else {
        return Err(invalid("currency", "Currency must be USD, USDT or USC."));
    };
    let key = r.idempotency_key.as_deref().map(str::trim).filter(|k| !k.is_empty()).map(str::to_string);
    if key.as_ref().is_some_and(|k| k.len() > 128) {
        return Err(invalid("idempotencyKey", "At most 128 characters."));
    }
    let source: String = r.source.as_deref().unwrap_or("wallet").chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').take(32).collect();
    // a deposit can't be in the future; the caller sends its own time so retries carry the same one
    let at = r.at.unwrap_or_else(Utc::now).min(Utc::now());
    let known: Option<i64> = sqlx::query_scalar("SELECT user_id FROM members WHERE user_id = $1 AND tenant = $2").bind(r.user_id).bind(&tenant).fetch_optional(&st.pool).await?;
    if known.is_none() {
        return Err(ApiError::NotFound);
    }
    if let Some(k) = &key
        && let Some(prev) = sqlx::query("SELECT user_id, amount, at, first_deposit FROM deposit_events WHERE tenant = $1 AND idempotency_key = $2").bind(&tenant).bind(k).fetch_optional(&st.pool).await?
    {
        let same = prev.get::<i64, _>("user_id") == r.user_id && prev.get::<D, _>("amount") == usd && (r.at.is_none() || prev.get::<DateTime<Utc>, _>("at") == at);
        if !same {
            return Err(ApiError::Conflict { code: "exists", message: "This idempotency key was used for another deposit.".into() });
        }
        return Ok(Json(json!({"status": "duplicate", "firstDeposit": prev.get::<bool, _>("first_deposit")})));
    }
    // earliest wins; the same (user, at) again changes nothing, so two racing retries are harmless
    let first = deals::record_deposit(&st, &tenant, r.user_id, at, usd).await?;
    let mut status = "recorded";
    if let Some(k) = &key {
        let n = sqlx::query(
            "INSERT INTO deposit_events (tenant, idempotency_key, user_id, amount, currency, at, source, first_deposit)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
        )
        .bind(&tenant)
        .bind(k)
        .bind(r.user_id)
        .bind(usd)
        .bind(&currency)
        .bind(at)
        .bind(&source)
        .bind(first)
        .execute(&st.pool)
        .await?
        .rows_affected();
        if n == 0 {
            status = "duplicate";
        }
    }
    if first {
        tracing::info!(client = r.user_id, amount = %usd, %source, "first deposit recorded from a deposit event");
    }
    Ok(Json(json!({"status": status, "firstDeposit": first})))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LotsReq {
    /// `pamm` | `copy`
    source: String,
    deal_id: i64,
    user_id: i64,
    symbol: String,
    #[serde(default)]
    side: String,
    /// Standard lots allocated to this investor.
    #[serde(deserialize_with = "de_dec")]
    lots: D,
    open_time: DateTime<Utc>,
    close_time: DateTime<Utc>,
    #[serde(default)]
    login: Option<i64>,
    #[serde(default)]
    reversed: bool,
}

/// `POST /v1/ib/events/lots`: lots a referred investor generated through a PAMM / copy allocation (D64),
/// pushed by the copy/PAMM service when the master deal closes. Same rules and idempotency as engine deals,
/// keyed by (source, dealId, userId). Trades the investor's own account makes are read from the engine
/// directly and must not be pushed here.
pub async fn lots_event(State(st): State<AppState>, Tenant(tenant): Tenant, Json(r): Json<LotsReq>) -> ApiResult<Json<Value>> {
    if !matches!(r.source.as_str(), "pamm" | "copy") {
        return Err(invalid("source", "source must be pamm or copy."));
    }
    if r.lots <= ZERO || r.close_time < r.open_time {
        return Err(invalid("lots", "Lots must be positive and close after open."));
    }
    let d = DealInput {
        source: r.source.clone(),
        deal_id: r.deal_id,
        tenant,
        login: r.login,
        user_id: r.user_id,
        symbol: r.symbol.trim().to_uppercase(),
        side: r.side.clone(),
        volume: r.lots,
        open_time: r.open_time,
        close_time: r.close_time,
        kind: "close".into(),
        reversed: r.reversed,
        account: Some(AccountInfo { login: r.login.unwrap_or(0), user_id: r.user_id, kind: "live".into(), group: String::new(), cent: false }),
        // option trades are never mirrored; should an option series ever arrive here it is paid per contract
        // (options rate), never at a per-lot rate
        option: crate::model::is_option_series(r.symbol.trim()),
        origin: r.source.clone(),
    };
    Ok(Json(match deals::ingest(&st, &d).await? {
        Outcome::Duplicate => json!({"status": "duplicate"}),
        Outcome::Recorded { qualified, reason, lines } => json!({"status": "recorded", "qualified": qualified, "reason": reason, "lines": lines}),
    }))
}

#[cfg(test)]
mod tests {
    #[test]
    fn codes_and_slugs() {
        assert_eq!(super::clean_code(" arjun24 ").as_deref(), Some("ARJUN24"));
        assert_eq!(super::clean_code("x!"), None);
        assert_eq!(super::clean_slug("YT-Gold_2026").as_deref(), Some("yt-gold_2026"));
        assert_eq!(super::clean_slug("no spaces"), None);
    }
}
