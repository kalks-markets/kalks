//! NOWPayments crypto deposits (README "NOWPayments"): per-broker settings with encrypted keys, payment creation,
//! the IPN, the poller fallback, crediting exactly once, and the staff review queue with four-eyes.
//!
//! Money rule: a payment is credited only after the service re-read it from NOWPayments (`GET /payment/{id}`) and it is
//! `finished` with `actually_paid` covering `pay_amount` (within `OVERPAY_TOLERANCE` above it) for the price, currency
//! and order it was created with. The client gets `price_amount` USD as USDT 1:1. Everything else that has money in
//! it (partially paid, overpaid, paid after expiry, a repeated deposit to the same address, a mismatch) goes to
//! `review`. Crediting locks the row and posts the ledger txn `deposit:nowpayments:<np_payment_id>` (unique per
//! tenant), so an IPN, a duplicate IPN, the poller and a staff approval racing each other book it once.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use sqlx::postgres::PgRow;

use super::{clean_text, notify, random_id};
use crate::audit::{self, Entry};
use crate::error::{ApiError, ApiResult};
use crate::ledger::{self, Leg, NewTxn, PostError};
use crate::money::{D, check_amount, floor_dp, s, s_opt};
use crate::nowpayments::{self as np, Np, NpError};
use crate::secrets;
use crate::state::{AppState, Ctx, ROLES_APPROVE, ROLES_SETTINGS, ROLES_WRITE, StaffCtx};

const CCY: &str = "USDT";
/// Counter-account of the credits: `sys:USDT:deposit:nowpayments` (like `sys:USDT:deposit:<chain>`).
pub const SYS_ACCOUNT: &str = "deposit:nowpayments";
/// Paying up to 1% more than asked is still credited automatically (price_amount); more goes to review.
fn overpay_tolerance() -> D {
    D::new(1, 2)
}
/// The poller re-checks an open payment at most this often (the IPN is the fast path).
const POLL_EVERY_SECS: i64 = 60;
/// An open payment nobody paid is closed after this long; one with money on the way is followed for 7 days.
const POLL_UNPAID_HOURS: i64 = 48;
const POLL_MAX_DAYS: i64 = 7;
/// Per client: payments waiting for money at once, and new payments per hour.
const MAX_OPEN: i64 = 5;
const MAX_PER_HOUR: i64 = 20;

pub fn ledger_key(np_payment_id: &str) -> String {
    format!("deposit:nowpayments:{np_payment_id}")
}

/* ------------------------------------------------------------------ */
/* Settings                                                            */
/* ------------------------------------------------------------------ */

#[derive(Clone, Debug)]
pub struct Settings {
    pub enabled: bool,
    pub sandbox: bool,
    api_key_enc: Option<String>,
    pub api_key_last4: Option<String>,
    pub api_key_updated_at: Option<DateTime<Utc>>,
    pub api_key_updated_by: Option<String>,
    ipn_secret_enc: Option<String>,
    pub ipn_secret_last4: Option<String>,
    pub ipn_secret_updated_at: Option<DateTime<Utc>>,
    pub ipn_secret_updated_by: Option<String>,
    pub coins: Vec<String>,
    pub min_deposit_usd: D,
    pub max_deposit_usd: D,
    pub fee_paid_by_user: bool,
    pub ipn_url: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
    pub updated_by: Option<String>,
}

impl Settings {
    fn defaults() -> Self {
        Settings {
            enabled: false,
            sandbox: false,
            api_key_enc: None,
            api_key_last4: None,
            api_key_updated_at: None,
            api_key_updated_by: None,
            ipn_secret_enc: None,
            ipn_secret_last4: None,
            ipn_secret_updated_at: None,
            ipn_secret_updated_by: None,
            coins: np::DEFAULT_COINS.iter().map(|c| c.to_string()).collect(),
            min_deposit_usd: D::TEN,
            max_deposit_usd: D::from(50_000),
            fee_paid_by_user: true,
            ipn_url: None,
            updated_at: None,
            updated_by: None,
        }
    }

    fn from_row(r: &PgRow) -> Self {
        Settings {
            enabled: r.get("enabled"),
            sandbox: r.get("sandbox"),
            api_key_enc: r.get("api_key_enc"),
            api_key_last4: r.get("api_key_last4"),
            api_key_updated_at: r.get("api_key_updated_at"),
            api_key_updated_by: r.get("api_key_updated_by"),
            ipn_secret_enc: r.get("ipn_secret_enc"),
            ipn_secret_last4: r.get("ipn_secret_last4"),
            ipn_secret_updated_at: r.get("ipn_secret_updated_at"),
            ipn_secret_updated_by: r.get("ipn_secret_updated_by"),
            coins: r.get("coins"),
            min_deposit_usd: r.get("min_deposit_usd"),
            max_deposit_usd: r.get("max_deposit_usd"),
            fee_paid_by_user: r.get("fee_paid_by_user"),
            ipn_url: r.get("ipn_url"),
            updated_at: Some(r.get("updated_at")),
            updated_by: r.get("updated_by"),
        }
    }

    pub fn has_api_key(&self) -> bool {
        self.api_key_enc.is_some()
    }
    pub fn has_ipn_secret(&self) -> bool {
        self.ipn_secret_enc.is_some()
    }
    /// Clients see the option only when it is switched on and both keys are set.
    pub fn ready(&self) -> bool {
        self.enabled && self.has_api_key() && self.has_ipn_secret()
    }

    pub fn ipn_url(&self, cfg: &crate::config::Config) -> String {
        self.ipn_url.clone().filter(|u| !u.is_empty()).unwrap_or_else(|| cfg.nowpayments_ipn_url.clone())
    }

    /// The settings without the secrets (Back Office view and audit before / after).
    pub fn public_json(&self, cfg: &crate::config::Config) -> Value {
        let secret = |set: bool, last4: &Option<String>, at: &Option<DateTime<Utc>>, by: &Option<String>| json!({"set": set, "last4": if set { last4.clone() } else { None }, "updated_at": at, "updated_by": by});
        json!({
            "enabled": self.enabled,
            "sandbox": self.sandbox,
            "api_key": secret(self.has_api_key(), &self.api_key_last4, &self.api_key_updated_at, &self.api_key_updated_by),
            "ipn_secret": secret(self.has_ipn_secret(), &self.ipn_secret_last4, &self.ipn_secret_updated_at, &self.ipn_secret_updated_by),
            "coins": self.coins,
            "min_deposit_usd": s(self.min_deposit_usd),
            "max_deposit_usd": s(self.max_deposit_usd),
            "fee_paid_by_user": self.fee_paid_by_user,
            "ipn_url": self.ipn_url(cfg),
            "ipn_url_custom": self.ipn_url.is_some(),
            "ready": self.ready(),
            "updated_at": self.updated_at,
            "updated_by": self.updated_by,
        })
    }
}

fn ctx_name(tenant_id: i64, name: &str) -> String {
    format!("{tenant_id}:nowpayments.{name}")
}

fn secret_unreadable() -> ApiError {
    ApiError::Coded {
        status: axum::http::StatusCode::SERVICE_UNAVAILABLE,
        code: "not_configured",
        message: "The NOWPayments keys can't be read on this server (WALLET_SECRETS_KEY missing or changed). Paste them again in the Back Office.".into(),
    }
}

fn open_secret(st: &AppState, tenant_id: i64, name: &str, enc: &Option<String>) -> ApiResult<String> {
    let enc = enc.as_ref().ok_or_else(|| ApiError::Coded { status: axum::http::StatusCode::SERVICE_UNAVAILABLE, code: "not_configured", message: "NOWPayments is not configured".into() })?;
    let key = secrets::key(&st.cfg).ok_or_else(secret_unreadable)?;
    secrets::open(&key, &ctx_name(tenant_id, name), enc).map_err(|e| {
        tracing::error!(tenant_id, setting = name, error = %e, "NOWPayments secret can't be decrypted");
        secret_unreadable()
    })
}

pub async fn settings(pool: &sqlx::PgPool, tenant_id: i64) -> sqlx::Result<Settings> {
    Ok(sqlx::query("SELECT * FROM nowpayments_settings WHERE tenant_id = $1").bind(tenant_id).fetch_optional(pool).await?.as_ref().map(Settings::from_row).unwrap_or_else(Settings::defaults))
}

/// The API client with the broker's key; `sandbox` is the payment's own environment (not today's switch).
fn client(st: &AppState, tenant_id: i64, cfg: &Settings, sandbox: bool) -> ApiResult<Np> {
    Ok(Np::new(&st.cfg, sandbox, open_secret(st, tenant_id, "api_key", &cfg.api_key_enc)?))
}

fn provider_error(e: &NpError) -> ApiError {
    let message = match e {
        NpError::Api(_, m) => format!("NOWPayments refused the request: {m}"),
        NpError::Network(_) => "NOWPayments is not reachable right now. Please try again shortly.".into(),
    };
    ApiError::Coded { status: axum::http::StatusCode::BAD_GATEWAY, code: "provider_error", message }
}

/* ------------------------------------------------------------------ */
/* Views                                                               */
/* ------------------------------------------------------------------ */

/// The client-facing stage: waiting (send the coins), confirming, credited, review, expired, failed, refunded, rejected.
pub fn client_status(r: &PgRow) -> &'static str {
    let np_status: Option<String> = r.get("np_status");
    match (r.get::<String, _>("status").as_str(), np_status.as_deref()) {
        ("creating", _) => "creating",
        ("open", None | Some("waiting")) => "waiting",
        ("open", Some("expired")) => "expired",
        ("open", Some("failed")) => "failed",
        ("open", Some("refunded")) => "refunded",
        ("open", _) => "confirming",
        ("credited", _) => "credited",
        ("review" | "approval", _) => "review",
        ("rejected", _) => "rejected",
        ("closed", Some("expired")) => "expired",
        ("closed", Some("refunded")) => "refunded",
        _ => "failed",
    }
}

/// What staff are offered to credit for a payment in review: the USD price scaled by what was really paid.
pub fn suggested_credit(r: &PgRow) -> Option<D> {
    let price: D = r.get("price_amount");
    let (paid, due): (Option<D>, Option<D>) = (r.get("actually_paid"), r.get("pay_amount"));
    match (paid, due) {
        (Some(p), Some(d)) if d > D::ZERO && p > D::ZERO => Some(floor_dp(price * p / d, 2)).filter(|x| *x > D::ZERO),
        _ => None,
    }
}

pub fn client_json(r: &PgRow) -> Value {
    let coin: String = r.get("pay_currency");
    json!({
        "id": r.get::<i64, _>("id"),
        "order_id": r.get::<String, _>("order_id"),
        "status": client_status(r),
        "np_status": r.get::<Option<String>, _>("np_status"),
        "price_amount": s(r.get::<D, _>("price_amount")),
        "price_currency": "USD",
        "pay_currency": coin,
        "coin": np::coin_info(&coin),
        "pay_amount": s_opt(r.get::<Option<D>, _>("pay_amount")),
        "pay_address": r.get::<Option<String>, _>("pay_address"),
        "payin_extra_id": r.get::<Option<String>, _>("payin_extra_id"),
        "network": r.get::<Option<String>, _>("network"),
        "actually_paid": s_opt(r.get::<Option<D>, _>("actually_paid")),
        "payin_hash": r.get::<Option<String>, _>("payin_hash"),
        "expires_at": r.get::<Option<DateTime<Utc>>, _>("expires_at"),
        "credit_amount": s_opt(r.get::<Option<D>, _>("credit_amount")),
        "credited_at": r.get::<Option<DateTime<Utc>>, _>("credited_at"),
        "failure_reason": r.get::<Option<String>, _>("failure_reason"),
        "sandbox": r.get::<bool, _>("sandbox"),
        "created_at": r.get::<DateTime<Utc>, _>("created_at"),
        "updated_at": r.get::<DateTime<Utc>, _>("updated_at"),
    })
}

pub fn admin_json(r: &PgRow) -> Value {
    let mut v = client_json(r);
    let o = v.as_object_mut().expect("object");
    o.insert("state".into(), json!(r.get::<String, _>("status")));
    for k in [
        "np_payment_id",
        "review_reason",
        "outcome_currency",
        "credited_by",
        "approval_reason",
        "approval_by_id",
        "approval_by_name",
        "decided_by",
        "decision_note",
        "check_error",
        "ip",
    ] {
        o.insert(k.into(), json!(r.get::<Option<String>, _>(k)));
    }
    for k in ["parent_id", "ledger_txn_id"] {
        o.insert(k.into(), json!(r.get::<Option<i64>, _>(k)));
    }
    for k in ["actually_paid_at_fiat", "outcome_amount", "approval_amount"] {
        o.insert(k.into(), s_opt(r.get::<Option<D>, _>(k)));
    }
    for k in ["approval_at", "decided_at", "last_ipn_at", "last_checked_at"] {
        o.insert(k.into(), json!(r.get::<Option<DateTime<Utc>>, _>(k)));
    }
    o.insert("user_id".into(), json!(r.get::<i64, _>("user_id")));
    o.insert("fee_paid_by_user".into(), json!(r.get::<bool, _>("fee_paid_by_user")));
    o.insert("ipn_count".into(), json!(r.get::<i32, _>("ipn_count")));
    o.insert("checks".into(), json!(r.get::<i32, _>("checks")));
    o.insert("suggested_credit".into(), s_opt(suggested_credit(r)));
    o.insert("last_ipn".into(), r.get::<Option<sqlx::types::Json<Value>>, _>("last_ipn").map(|j| j.0).unwrap_or(Value::Null));
    o.insert("np_payment".into(), r.get::<Option<sqlx::types::Json<Value>>, _>("np_payment").map(|j| j.0).unwrap_or(Value::Null));
    v
}

async fn load(pool: &sqlx::PgPool, id: i64) -> sqlx::Result<Option<PgRow>> {
    sqlx::query("SELECT * FROM nowpayments_payments WHERE id = $1").bind(id).fetch_optional(pool).await
}

/* ------------------------------------------------------------------ */
/* Client                                                              */
/* ------------------------------------------------------------------ */

/// `GET /v1/nowpayments/config`: off unless enabled with both keys; else the limits and the coins with their USD
/// minimum (the larger of the broker's minimum and NOWPayments' own).
pub async fn client_config(st: &AppState, tenant_id: i64) -> ApiResult<Value> {
    let cfg = settings(&st.pool, tenant_id).await?;
    if !cfg.ready() {
        return Ok(json!({"enabled": false, "coins": []}));
    }
    let npc = client(st, tenant_id, &cfg, cfg.sandbox)?;
    // only coins the merchant account accepts right now (when NOWPayments can't be asked, all of them)
    let enabled = np::merchant_coins_cached(&npc, tenant_id, cfg.sandbox).await;
    let offered: Vec<String> = cfg.coins.iter().filter(|c| enabled.as_ref().is_none_or(|e| e.contains(c))).cloned().collect();
    if offered.is_empty() {
        return Ok(json!({"enabled": false, "coins": []}));
    }
    let npc = std::sync::Arc::new(npc);
    let handles: Vec<_> = offered
        .iter()
        .map(|coin| {
            let (npc, coin, sandbox, fee) = (npc.clone(), coin.clone(), cfg.sandbox, cfg.fee_paid_by_user);
            tokio::spawn(async move { np::min_usd(&npc, tenant_id, sandbox, &coin, fee).await })
        })
        .collect();
    let mut coins = vec![];
    for (coin, h) in offered.iter().zip(handles) {
        let np_min = h.await.ok().flatten();
        let min = np_min.map(|m| m.max(cfg.min_deposit_usd)).unwrap_or(cfg.min_deposit_usd);
        let mut c = np::coin_info(coin);
        c["min_usd"] = json!(s(min));
        coins.push(c);
    }
    Ok(json!({
        "enabled": true,
        "sandbox": cfg.sandbox,
        "min_deposit_usd": s(cfg.min_deposit_usd),
        "max_deposit_usd": s(cfg.max_deposit_usd),
        "fee_paid_by_user": cfg.fee_paid_by_user,
        "coins": coins,
    }))
}

/// `POST /v1/nowpayments/payments`: creates the payment with NOWPayments and returns what the client must send.
pub async fn create(st: &AppState, ctx: &Ctx, user_id: i64, amount: D, coin_raw: &str) -> ApiResult<Value> {
    let tenant_id = ctx.tenant.id;
    if user_id <= 0 {
        return Err(ApiError::validation("user_id", "user_id is required"));
    }
    crate::users::gate(st, &ctx.tenant.slug, user_id, "deposits").await?;
    let cfg = settings(&st.pool, tenant_id).await?;
    if !cfg.ready() {
        return Err(ApiError::unprocessable("nowpayments_disabled", "Crypto deposits are not available at the moment"));
    }
    let coin = np::clean_coin(coin_raw).filter(|c| cfg.coins.contains(c)).ok_or_else(|| ApiError::validation("pay_currency", "Choose one of the listed coins"))?;
    let amount = check_amount(amount, 2).map_err(|m| ApiError::validation("amount", m))?;
    if amount < cfg.min_deposit_usd {
        return Err(ApiError::unprocessable("below_minimum", format!("The minimum deposit is {} USD", s(cfg.min_deposit_usd))));
    }
    if amount > cfg.max_deposit_usd {
        return Err(ApiError::unprocessable("above_maximum", format!("The maximum deposit is {} USD", s(cfg.max_deposit_usd))));
    }
    let r = sqlx::query(
        "SELECT count(*) FILTER (WHERE created_at > now() - interval '1 hour') AS hour,
                count(*) FILTER (WHERE status IN ('creating', 'open') AND COALESCE(np_status, 'waiting') = 'waiting' AND (expires_at IS NULL OR expires_at > now())) AS open
           FROM nowpayments_payments WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_one(&st.pool)
    .await?;
    if r.get::<i64, _>("hour") >= MAX_PER_HOUR {
        return Err(ApiError::Coded { status: axum::http::StatusCode::TOO_MANY_REQUESTS, code: "rate_limited", message: "Too many deposit requests. Please try again later.".into() });
    }
    if r.get::<i64, _>("open") >= MAX_OPEN {
        return Err(ApiError::Coded {
            status: axum::http::StatusCode::TOO_MANY_REQUESTS,
            code: "too_many_open",
            message: format!("You already have {MAX_OPEN} crypto deposits waiting for payment. Pay one of them or wait until they expire."),
        });
    }
    let npc = client(st, tenant_id, &cfg, cfg.sandbox)?;
    if let Some(min) = np::min_usd(&npc, tenant_id, cfg.sandbox, &coin, cfg.fee_paid_by_user).await
        && amount < min
    {
        return Err(ApiError::unprocessable("below_minimum", format!("The minimum for this coin is {} USD", s(min))));
    }
    let order_id = random_id("np_");
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO nowpayments_payments (tenant_id, user_id, order_id, sandbox, price_amount, pay_currency, fee_paid_by_user, status, ip)
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'creating', $8) RETURNING id",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(&order_id)
    .bind(cfg.sandbox)
    .bind(amount)
    .bind(&coin)
    .bind(cfg.fee_paid_by_user)
    .bind(ctx.ip.as_deref())
    .fetch_one(&st.pool)
    .await?;
    let ipn_url = cfg.ipn_url(&st.cfg);
    let mut body = json!({
        "price_amount": s(amount).parse::<serde_json::Number>().map(Value::Number).unwrap_or_else(|_| json!(s(amount))),
        "price_currency": "usd",
        "pay_currency": coin,
        "order_id": order_id,
        "order_description": format!("Kalks wallet deposit {order_id}"),
        "is_fee_paid_by_user": cfg.fee_paid_by_user,
        // NOWPayments: a fee-paid-by-user payment is always fixed rate
        "is_fixed_rate": cfg.fee_paid_by_user,
    });
    if ipn_url.starts_with("https://") || (st.cfg.dev_mode && ipn_url.starts_with("http://")) {
        body["ipn_callback_url"] = json!(ipn_url);
    }
    let created = match npc.create_payment(&body).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(tenant_id, user_id, payment = id, error = %e, "NOWPayments create payment failed");
            sqlx::query("UPDATE nowpayments_payments SET status = 'error', failure_reason = $2, updated_at = now() WHERE id = $1").bind(id).bind(e.to_string()).execute(&st.pool).await?;
            return Err(provider_error(&e));
        }
    };
    let pid = np::text(created.get("payment_id")).filter(|p| p.bytes().all(|b| b.is_ascii_digit()));
    let address = np::text(created.get("pay_address"));
    let (Some(pid), Some(address)) = (pid, address) else {
        sqlx::query("UPDATE nowpayments_payments SET status = 'error', failure_reason = 'NOWPayments returned no payment id or address', np_payment = $2, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(sqlx::types::Json(&created))
            .execute(&st.pool)
            .await?;
        return Err(ApiError::Coded { status: axum::http::StatusCode::BAD_GATEWAY, code: "provider_error", message: "NOWPayments did not return a payment address. Please try again.".into() });
    };
    let expires = ["expiration_estimate_date", "valid_until"]
        .iter()
        .find_map(|k| created.get(*k).and_then(Value::as_str).and_then(|t| DateTime::parse_from_rfc3339(t).ok()).map(|d| d.with_timezone(&Utc)));
    let r = sqlx::query(
        "UPDATE nowpayments_payments SET np_payment_id = $2, np_status = $3, pay_amount = $4, pay_address = $5, payin_extra_id = $6, network = $7,
                expires_at = $8, np_payment = $9, status = 'open', updated_at = now()
          WHERE id = $1 AND status = 'creating' RETURNING *",
    )
    .bind(id)
    .bind(&pid)
    .bind(created.get("payment_status").and_then(Value::as_str).unwrap_or("waiting").to_ascii_lowercase())
    .bind(np::dec(created.get("pay_amount")))
    .bind(&address)
    .bind(np::text(created.get("payin_extra_id")))
    .bind(np::text(created.get("network")))
    .bind(expires)
    .bind(sqlx::types::Json(&created))
    .fetch_one(&st.pool)
    .await?;
    tracing::info!(tenant_id, user_id, payment = id, np_payment_id = %pid, coin = %coin, amount = %amount, sandbox = cfg.sandbox, "NOWPayments payment created");
    Ok(client_json(&r))
}

/// One payment of the client (`user_id` = None: any client, internal callers). A stale open payment is re-checked in
/// the background, so the page moves on even when an IPN is late.
pub async fn get(st: &AppState, tenant_id: i64, user_id: Option<i64>, id: i64) -> ApiResult<Value> {
    let r = sqlx::query("SELECT * FROM nowpayments_payments WHERE tenant_id = $1 AND id = $2 AND ($3::bigint IS NULL OR user_id = $3)")
        .bind(tenant_id)
        .bind(id)
        .bind(user_id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| ApiError::not_found("Payment"))?;
    let stale = r.get::<String, _>("status") == "open"
        && r.get::<Option<String>, _>("np_payment_id").is_some()
        && r.get::<Option<DateTime<Utc>>, _>("last_checked_at").is_none_or(|t| Utc::now() - t > chrono::Duration::seconds(20));
    if stale {
        let st2 = st.clone();
        tokio::spawn(async move {
            if let Err(e) = sync(&st2, id).await {
                tracing::debug!(payment = id, error = %e, "NOWPayments refresh failed");
            }
        });
    }
    Ok(client_json(&r))
}

/* ------------------------------------------------------------------ */
/* IPN                                                                 */
/* ------------------------------------------------------------------ */

/// `POST /v1/nowpayments/ipn` (forwarded by the Client Area with the raw body): the signature is checked with the
/// IPN secret of the broker the payment belongs to, the body is stored, and the payment is re-read from NOWPayments
/// and applied. Unknown payments are 404, an unconfigured broker 503, a bad signature 401.
pub async fn ipn(st: &AppState, raw: &[u8], signature: Option<&str>) -> ApiResult<Value> {
    let v: Value = serde_json::from_slice(raw).map_err(|_| ApiError::BadRequest("Invalid JSON".into()))?;
    let pid = np::text(v.get("payment_id")).filter(|p| p.len() <= 32 && p.bytes().all(|b| b.is_ascii_digit())).ok_or_else(|| ApiError::BadRequest("payment_id is missing".into()))?;
    enum Target {
        Known(i64),
        /// a create call whose answer we never got: the IPN names our order id
        Adopt(i64),
        /// a repeated deposit (NOWPayments `parent_payment_id`) to the address of one of our payments
        Child(PgRow),
    }
    let (tenant_id, target) = if let Some(r) = sqlx::query("SELECT id, tenant_id FROM nowpayments_payments WHERE np_payment_id = $1").bind(&pid).fetch_optional(&st.pool).await? {
        (r.get::<i64, _>("tenant_id"), Target::Known(r.get("id")))
    } else if let Some(r) = match np::text(v.get("order_id")) {
        Some(o) => sqlx::query("SELECT id, tenant_id FROM nowpayments_payments WHERE order_id = $1 AND np_payment_id IS NULL").bind(o).fetch_optional(&st.pool).await?,
        None => None,
    } {
        (r.get::<i64, _>("tenant_id"), Target::Adopt(r.get("id")))
    } else if let Some(r) = match np::text(v.get("parent_payment_id")) {
        Some(p) => sqlx::query("SELECT * FROM nowpayments_payments WHERE np_payment_id = $1").bind(p).fetch_optional(&st.pool).await?,
        None => None,
    } {
        (r.get::<i64, _>("tenant_id"), Target::Child(r))
    } else {
        tracing::info!(np_payment_id = %pid, "NOWPayments IPN for an unknown payment");
        return Err(ApiError::not_found("Payment"));
    };
    let cfg = settings(&st.pool, tenant_id).await?;
    if !cfg.has_ipn_secret() {
        tracing::warn!(tenant_id, np_payment_id = %pid, "NOWPayments IPN refused: no IPN secret configured");
        return Err(ApiError::Coded { status: axum::http::StatusCode::SERVICE_UNAVAILABLE, code: "not_configured", message: "NOWPayments is not configured".into() });
    }
    let secret = open_secret(st, tenant_id, "ipn_secret", &cfg.ipn_secret_enc)?;
    if !np::verify(&secret, raw, signature.unwrap_or("")) {
        tracing::warn!(tenant_id, np_payment_id = %pid, signed = signature.is_some(), "NOWPayments IPN with an invalid signature");
        return Err(ApiError::Coded { status: axum::http::StatusCode::UNAUTHORIZED, code: "invalid_signature", message: "Invalid signature".into() });
    }
    let id = match target {
        Target::Known(id) => id,
        Target::Adopt(id) => {
            sqlx::query("UPDATE nowpayments_payments SET np_payment_id = $2, status = CASE WHEN status IN ('creating', 'error') THEN 'open' ELSE status END, failure_reason = NULL, updated_at = now() WHERE id = $1 AND np_payment_id IS NULL")
                .bind(id)
                .bind(&pid)
                .execute(&st.pool)
                .await?;
            id
        }
        Target::Child(parent) => {
            let price = np::dec(v.get("price_amount")).filter(|p| *p > D::ZERO).unwrap_or_else(|| parent.get("price_amount"));
            let coin = np::text(v.get("pay_currency")).and_then(|c| np::clean_coin(&c)).unwrap_or_else(|| parent.get("pay_currency"));
            sqlx::query(
                "INSERT INTO nowpayments_payments (tenant_id, user_id, order_id, np_payment_id, parent_id, sandbox, price_amount, pay_currency, pay_address, payin_extra_id,
                                                   fee_paid_by_user, status)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'open') ON CONFLICT (np_payment_id) DO NOTHING",
            )
            .bind(tenant_id)
            .bind(parent.get::<i64, _>("user_id"))
            .bind(random_id("np_"))
            .bind(&pid)
            .bind(parent.get::<i64, _>("id"))
            .bind(parent.get::<bool, _>("sandbox"))
            .bind(price)
            .bind(coin)
            .bind(parent.get::<Option<String>, _>("pay_address"))
            .bind(parent.get::<Option<String>, _>("payin_extra_id"))
            .bind(parent.get::<bool, _>("fee_paid_by_user"))
            .execute(&st.pool)
            .await?;
            tracing::warn!(tenant_id, np_payment_id = %pid, parent = parent.get::<i64, _>("id"), "NOWPayments repeated deposit recorded for review");
            sqlx::query_scalar("SELECT id FROM nowpayments_payments WHERE np_payment_id = $1").bind(&pid).fetch_one(&st.pool).await?
        }
    };
    sqlx::query("UPDATE nowpayments_payments SET last_ipn = $2, last_ipn_at = now(), ipn_count = ipn_count + 1, updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(sqlx::types::Json(&v))
        .execute(&st.pool)
        .await?;
    let reported = v.get("payment_status").and_then(|x| x.as_str()).unwrap_or("").to_string();
    tracing::info!(tenant_id, payment = id, np_payment_id = %pid, np_status = %reported, "NOWPayments IPN accepted");
    // the IPN body is only a wake-up: the money decision is made on NOWPayments' own answer (the poller retries)
    if let Err(e) = sync(st, id).await {
        tracing::warn!(payment = id, error = %e, "NOWPayments re-check after the IPN failed; the poller retries");
    }
    Ok(json!({"status": "ok"}))
}

/* ------------------------------------------------------------------ */
/* Re-check and decide                                                 */
/* ------------------------------------------------------------------ */

/// Re-reads a payment from NOWPayments (`GET /payment/{id}`), records it and applies the money rules. Safe to call
/// any number of times and concurrently.
pub async fn sync(st: &AppState, id: i64) -> anyhow::Result<()> {
    let Some(r) = load(&st.pool, id).await? else { return Ok(()) };
    let Some(pid) = r.get::<Option<String>, _>("np_payment_id") else { return Ok(()) };
    if matches!(r.get::<String, _>("status").as_str(), "rejected" | "error") {
        return Ok(());
    }
    let tenant_id: i64 = r.get("tenant_id");
    let cfg = settings(&st.pool, tenant_id).await?;
    let npc = match client(st, tenant_id, &cfg, r.get("sandbox")) {
        Ok(c) => c,
        Err(e) => {
            sqlx::query("UPDATE nowpayments_payments SET checks = checks + 1, last_checked_at = now(), check_error = 'NOWPayments API key not available' WHERE id = $1").bind(id).execute(&st.pool).await?;
            anyhow::bail!("no API key: {}", e.code());
        }
    };
    let p = match npc.payment(&pid).await {
        Ok(p) => p,
        Err(e) => {
            sqlx::query("UPDATE nowpayments_payments SET checks = checks + 1, last_checked_at = now(), check_error = $2 WHERE id = $1").bind(id).bind(e.to_string()).execute(&st.pool).await?;
            return Err(e.into());
        }
    };
    let np_status = p.get("payment_status").and_then(Value::as_str).unwrap_or("").to_ascii_lowercase();
    let r = sqlx::query(
        "UPDATE nowpayments_payments SET np_status = NULLIF($2, ''), actually_paid = $3, actually_paid_at_fiat = $4, outcome_amount = $5, outcome_currency = $6,
                payin_hash = COALESCE($7, payin_hash), pay_amount = COALESCE(pay_amount, $8), pay_address = COALESCE(pay_address, $9),
                payin_extra_id = COALESCE(payin_extra_id, $10), np_payment = $11, checks = checks + 1, last_checked_at = now(), check_error = NULL, updated_at = now()
          WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .bind(&np_status)
    .bind(np::dec(p.get("actually_paid")))
    .bind(np::dec(p.get("actually_paid_at_fiat")))
    .bind(np::dec(p.get("outcome_amount")))
    .bind(np::text(p.get("outcome_currency")))
    .bind(np::text(p.get("payin_hash")))
    .bind(np::dec(p.get("pay_amount")))
    .bind(np::text(p.get("pay_address")))
    .bind(np::text(p.get("payin_extra_id")))
    .bind(sqlx::types::Json(&p))
    .fetch_one(&st.pool)
    .await?;
    decide(st, &r, &p, &np_status).await
}

async fn decide(st: &AppState, r: &PgRow, p: &Value, np_status: &str) -> anyhow::Result<()> {
    let id: i64 = r.get("id");
    let status: String = r.get("status");
    if status == "credited" {
        if np_status == "refunded" {
            tracing::error!(payment = id, "NOWPayments reports a credited payment as refunded: check it in the Back Office");
        }
        return Ok(());
    }
    if status != "open" && status != "closed" {
        // review / approval / rejected: staff decide; the NOWPayments fields above stay current
        return Ok(());
    }
    let coin: String = r.get("pay_currency");
    let price: D = r.get("price_amount");
    let is_child = r.get::<Option<i64>, _>("parent_id").is_some();
    let paid: Option<D> = r.get("actually_paid");
    let due: Option<D> = r.get("pay_amount");
    // the payment NOWPayments answered with must be the one we created
    let mut mismatch: Vec<String> = vec![];
    if np::text(p.get("payment_id")).as_deref() != r.get::<Option<String>, _>("np_payment_id").as_deref() {
        mismatch.push("NOWPayments answered with another payment id".into());
    }
    if !is_child {
        if np::text(p.get("order_id")).as_deref() != Some(r.get::<String, _>("order_id").as_str()) {
            mismatch.push("The order id differs".into());
        }
        if np::dec(p.get("price_amount")) != Some(price) {
            mismatch.push(format!("The price differs ({} USD expected)", s(price)));
        }
    }
    if p.get("price_currency").and_then(Value::as_str).map(str::to_ascii_lowercase).as_deref() != Some("usd") {
        mismatch.push("The price currency is not USD".into());
    }
    if np::text(p.get("pay_currency")).map(|c| c.to_ascii_lowercase()).as_deref() != Some(coin.as_str()) {
        mismatch.push(format!("The coin differs ({} expected)", coin.to_uppercase()));
    }
    let amounts = || format!("{} of {} {}", paid.map(s).unwrap_or_else(|| "?".into()), due.map(s).unwrap_or_else(|| "?".into()), coin.to_uppercase());
    match np_status {
        "finished" => {
            let reason = if is_child {
                Some(format!("Repeated deposit to the address of payment #{} ({})", r.get::<Option<i64>, _>("parent_id").unwrap_or_default(), amounts()))
            } else if !mismatch.is_empty() {
                Some(mismatch.join("; "))
            } else if status == "closed" {
                Some(format!("Paid after the payment had {} ({})", r.get::<Option<String>, _>("failure_reason").unwrap_or_else(|| "closed".into()), amounts()))
            } else {
                match (paid, due) {
                    (Some(paid), Some(due)) if paid >= due && paid <= due * (D::ONE + overpay_tolerance()) => None,
                    (Some(paid), Some(due)) if paid > due => Some(format!("Overpaid: {}", amounts())),
                    (Some(_), Some(_)) => Some(format!("Finished with less than asked: {}", amounts())),
                    _ => Some("NOWPayments did not report the amount paid".into()),
                }
            };
            match reason {
                None => {
                    credit(st, id, &["open"], price, "system", None, None).await?;
                }
                Some(reason) => to_review(st, id, &status, &reason).await?,
            }
        }
        "partially_paid" => to_review(st, id, &status, &format!("Partially paid: {}", amounts())).await?,
        "failed" | "expired" | "refunded" if status == "open" => {
            let n = sqlx::query("UPDATE nowpayments_payments SET status = 'closed', failure_reason = $2, updated_at = now() WHERE id = $1 AND status = 'open'")
                .bind(id)
                .bind(np_status)
                .execute(&st.pool)
                .await?
                .rows_affected();
            if n == 1 {
                tracing::info!(payment = id, np_status, "NOWPayments payment closed");
            }
        }
        _ => {}
    }
    Ok(())
}

async fn to_review(st: &AppState, id: i64, from: &str, reason: &str) -> anyhow::Result<()> {
    let n = sqlx::query("UPDATE nowpayments_payments SET status = 'review', review_reason = $3, updated_at = now() WHERE id = $1 AND status = $2")
        .bind(id)
        .bind(from)
        .bind(reason)
        .execute(&st.pool)
        .await?
        .rows_affected();
    if n == 1 {
        tracing::warn!(payment = id, %reason, "NOWPayments payment held for review");
    }
    Ok(())
}

/// Credits `amount` USD (as USDT) in one database transaction: row locked in one of the `from` states, ledger txn
/// `deposit:nowpayments:<id>` (unique), row → credited, IB deposit event, client notification, audit. Returns false
/// when the row is no longer in a `from` state (someone else credited or decided it).
async fn credit(st: &AppState, id: i64, from: &[&str], amount: D, actor: &str, staff: Option<&StaffCtx>, reason: Option<&str>) -> anyhow::Result<bool> {
    let mut tx = st.pool.begin().await?;
    let from: Vec<String> = from.iter().map(|x| x.to_string()).collect();
    let Some(r) = sqlx::query("SELECT * FROM nowpayments_payments WHERE id = $1 AND status = ANY($2) FOR UPDATE").bind(id).bind(&from).fetch_optional(&mut *tx).await? else { return Ok(false) };
    let Some(pid) = r.get::<Option<String>, _>("np_payment_id") else { return Ok(false) };
    let tenant_id: i64 = r.get("tenant_id");
    let user_id: i64 = r.get("user_id");
    let coin: String = r.get("pay_currency");
    let info = np::coin_info(&coin);
    let label = format!("{} {}", info["ticker"].as_str().unwrap_or(""), info["network"].as_str().unwrap_or("")).trim().to_string();
    let amount = floor_dp(amount, 2);
    if amount <= D::ZERO {
        anyhow::bail!("nothing to credit");
    }
    let key = ledger_key(&pid);
    let txn = match ledger::post(
        &mut tx,
        NewTxn {
            tenant_id,
            key: key.clone(),
            kind: "deposit".into(),
            user_id: Some(user_id),
            currency: CCY.into(),
            reference: Some(format!("nowpayments:{pid}")),
            note: Some(format!("Crypto deposit · {label} (NOWPayments)")),
            actor: actor.into(),
            request: None,
            legs: vec![Leg::available(user_id, CCY, amount), Leg::sys(CCY, SYS_ACCOUNT, -amount)],
        },
    )
    .await
    {
        Ok(t) => t,
        Err(PostError::Duplicate) => {
            // booked by an earlier run that failed before updating the row: link it, book nothing
            drop(tx);
            let t: Option<(i64, D)> = sqlx::query_as(
                "SELECT t.id, sum(p.amount) FROM ledger_txns t JOIN ledger_postings p ON p.txn_id = t.id AND p.user_id = t.user_id WHERE t.tenant_id = $1 AND t.idempotency_key = $2 GROUP BY t.id",
            )
            .bind(tenant_id)
            .bind(&key)
            .fetch_optional(&st.pool)
            .await?;
            let n = sqlx::query("UPDATE nowpayments_payments SET status = 'credited', ledger_txn_id = $3, credit_amount = $4, credited_at = COALESCE(credited_at, now()), updated_at = now() WHERE id = $1 AND status = ANY($2)")
                .bind(id)
                .bind(&from)
                .bind(t.map(|x| x.0))
                .bind(t.map(|x| x.1))
                .execute(&st.pool)
                .await?
                .rows_affected();
            if let Some((t, _)) = t {
                crate::ib::record(&st.pool, t).await?;
            }
            return Ok(n == 1);
        }
        Err(PostError::Insufficient) => anyhow::bail!("unexpected insufficient funds on a deposit credit"),
        Err(PostError::Db(e)) => return Err(e),
    };
    sqlx::query("UPDATE nowpayments_payments SET status = 'credited', credit_amount = $2, ledger_txn_id = $3, credited_at = now(), credited_by = $4, updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(amount)
        .bind(txn)
        .bind(actor)
        .execute(&mut *tx)
        .await?;
    // real money in: reported to the IB service after the commit (first deposit / CPA)
    crate::ib::record(&mut *tx, txn).await?;
    notify(
        &mut tx,
        tenant_id,
        user_id,
        "deposit.credited",
        "Deposit credited",
        &format!("{} USDT was credited to your wallet.", s(amount)),
        json!({"nowpayments_id": id, "pay_currency": coin}),
    )
    .await?;
    let trail = json!({"amount": s(amount), "np_payment_id": pid, "ledger_txn_id": txn, "actually_paid": s_opt(r.get::<Option<D>, _>("actually_paid")), "pay_amount": s_opt(r.get::<Option<D>, _>("pay_amount")), "pay_currency": coin});
    match staff {
        Some(s_ctx) => {
            audit::staff(&mut tx, s_ctx, Entry { action: "wallet.nowpayments.credited", target_kind: "nowpayments_payment", target_id: id.to_string(), reason, before: Some(admin_json(&r)), after: Some(trail) }).await?
        }
        None => audit::system(&mut tx, tenant_id, "wallet.nowpayments.credited", "nowpayments_payment", id.to_string(), Some(trail)).await?,
    }
    tx.commit().await?;
    tracing::info!(payment = id, tenant_id, user_id, amount = %amount, np_payment_id = %pid, actor, "NOWPayments deposit credited");
    Ok(true)
}

/* ------------------------------------------------------------------ */
/* Poller                                                              */
/* ------------------------------------------------------------------ */

/// One pass of the poller (watcher, every `WALLET_POLL_SECS`): re-checks open payments at most once a minute, for
/// 48 hours (7 days once money is on the way); closes unpaid ones after 48 hours and failed create calls after 10
/// minutes. Returns how many payments were checked.
pub async fn poll(st: &AppState) -> anyhow::Result<usize> {
    sqlx::query("UPDATE nowpayments_payments SET status = 'error', failure_reason = 'No answer from NOWPayments when the payment was created', updated_at = now() WHERE status = 'creating' AND created_at < now() - interval '10 minutes'")
        .execute(&st.pool)
        .await?;
    sqlx::query(
        "UPDATE nowpayments_payments SET status = 'closed', failure_reason = COALESCE(np_status, 'expired'), updated_at = now()
          WHERE status = 'open' AND created_at < now() - make_interval(hours => $1) AND COALESCE(actually_paid, 0) = 0
            AND COALESCE(np_status, 'waiting') IN ('waiting', 'expired', 'failed')",
    )
    .bind(POLL_UNPAID_HOURS as i32)
    .execute(&st.pool)
    .await?;
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM nowpayments_payments
          WHERE status = 'open' AND np_payment_id IS NOT NULL
            AND (last_checked_at IS NULL OR last_checked_at < now() - make_interval(secs => $1))
            AND (created_at > now() - make_interval(hours => $2) OR (created_at > now() - make_interval(days => $3) AND COALESCE(actually_paid, 0) > 0))
          ORDER BY last_checked_at NULLS FIRST, id LIMIT 50",
    )
    .bind(POLL_EVERY_SECS as f64)
    .bind(POLL_UNPAID_HOURS as i32)
    .bind(POLL_MAX_DAYS as i32)
    .fetch_all(&st.pool)
    .await?;
    for id in &ids {
        if let Err(e) = sync(st, *id).await {
            tracing::warn!(payment = id, error = %e, "NOWPayments payment check failed");
        }
    }
    Ok(ids.len())
}

/* ------------------------------------------------------------------ */
/* Back Office                                                         */
/* ------------------------------------------------------------------ */

#[derive(Deserialize, Default)]
pub struct SettingsPatch {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub sandbox: Option<bool>,
    /// write-only; empty / missing = unchanged
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub ipn_secret: Option<String>,
    #[serde(default)]
    pub clear_api_key: bool,
    #[serde(default)]
    pub clear_ipn_secret: bool,
    #[serde(default)]
    pub coins: Option<Vec<String>>,
    #[serde(default, deserialize_with = "crate::money::de_opt_dec")]
    pub min_deposit_usd: Option<D>,
    #[serde(default, deserialize_with = "crate::money::de_opt_dec")]
    pub max_deposit_usd: Option<D>,
    #[serde(default)]
    pub fee_paid_by_user: Option<bool>,
    /// "" = back to the default (WALLET_NOWPAYMENTS_IPN_URL)
    #[serde(default)]
    pub ipn_url: Option<String>,
    #[serde(default)]
    pub reason: String,
}

fn clean_secret(field: &'static str, v: &str) -> ApiResult<String> {
    let v = v.trim();
    if v.len() < 8 || v.len() > 256 || v.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(ApiError::validation(field, "Paste the value exactly as NOWPayments shows it (8–256 characters, no spaces)"));
    }
    Ok(v.to_string())
}

fn usd_limit(field: &'static str, v: D) -> ApiResult<D> {
    if v <= D::ZERO || v > D::from(10_000_000) || v.normalize().scale() > 2 {
        return Err(ApiError::validation(field, "Enter a USD amount above 0 with up to 2 decimals"));
    }
    Ok(v.normalize())
}

/// `PUT /v1/admin/nowpayments/settings` (finance.settings). Secrets are encrypted and never echoed; every change is
/// audited (`wallet.settings.nowpayments`) with before / after values that only say whether a key is set and its
/// last 4 characters.
pub async fn update_settings(st: &AppState, s_ctx: &StaffCtx, b: SettingsPatch) -> ApiResult<Value> {
    s_ctx.require_perm("finance.settings", ROLES_SETTINGS)?;
    let reason = b.reason.trim().to_string();
    if reason.chars().count() < 3 {
        return Err(ApiError::validation("reason", "Enter a reason for the change (kept in the audit log)"));
    }
    let t = s_ctx.ctx.tenant.id;
    let cur = settings(&st.pool, t).await?;
    let mut next = cur.clone();
    let mut changed: Vec<&str> = vec![];
    let tag = s_ctx.staff.tag();
    let now = Utc::now();
    let new_api = b.api_key.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(|v| clean_secret("api_key", v)).transpose()?;
    let new_ipn = b.ipn_secret.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(|v| clean_secret("ipn_secret", v)).transpose()?;
    if new_api.is_some() || new_ipn.is_some() {
        let key = secrets::key(&st.cfg).ok_or_else(|| ApiError::Coded {
            status: axum::http::StatusCode::SERVICE_UNAVAILABLE,
            code: "not_configured",
            message: "WALLET_SECRETS_KEY is not set on the server, so keys can't be stored safely. Ask the operator to set it (deploy.sh generates it).".into(),
        })?;
        if let Some(v) = &new_api {
            next.api_key_enc = Some(secrets::seal(&key, &ctx_name(t, "api_key"), v)?);
            next.api_key_last4 = Some(secrets::last4(v));
            next.api_key_updated_at = Some(now);
            next.api_key_updated_by = Some(s_ctx.staff.name.clone());
            changed.push("api_key");
        }
        if let Some(v) = &new_ipn {
            next.ipn_secret_enc = Some(secrets::seal(&key, &ctx_name(t, "ipn_secret"), v)?);
            next.ipn_secret_last4 = Some(secrets::last4(v));
            next.ipn_secret_updated_at = Some(now);
            next.ipn_secret_updated_by = Some(s_ctx.staff.name.clone());
            changed.push("ipn_secret");
        }
    }
    if b.clear_api_key && new_api.is_none() && cur.has_api_key() {
        (next.api_key_enc, next.api_key_last4, next.api_key_updated_at, next.api_key_updated_by) = (None, None, Some(now), Some(s_ctx.staff.name.clone()));
        changed.push("api_key");
    }
    if b.clear_ipn_secret && new_ipn.is_none() && cur.has_ipn_secret() {
        (next.ipn_secret_enc, next.ipn_secret_last4, next.ipn_secret_updated_at, next.ipn_secret_updated_by) = (None, None, Some(now), Some(s_ctx.staff.name.clone()));
        changed.push("ipn_secret");
    }
    if let Some(v) = b.enabled {
        next.enabled = v;
    }
    if let Some(v) = b.sandbox {
        next.sandbox = v;
    }
    if let Some(v) = b.fee_paid_by_user {
        next.fee_paid_by_user = v;
    }
    if let Some(coins) = &b.coins {
        let mut out: Vec<String> = vec![];
        for c in coins {
            let c = np::clean_coin(c).ok_or_else(|| ApiError::validation("coins", format!("Unknown coin code {c:?}")))?;
            if !out.contains(&c) {
                out.push(c);
            }
        }
        if out.is_empty() || out.len() > 60 {
            return Err(ApiError::validation("coins", "Choose between 1 and 60 coins"));
        }
        next.coins = out;
    }
    if let Some(v) = b.min_deposit_usd {
        next.min_deposit_usd = usd_limit("min_deposit_usd", v)?;
    }
    if let Some(v) = b.max_deposit_usd {
        next.max_deposit_usd = usd_limit("max_deposit_usd", v)?;
    }
    if next.max_deposit_usd < next.min_deposit_usd {
        return Err(ApiError::validation("max_deposit_usd", "The maximum must be at least the minimum"));
    }
    if let Some(u) = &b.ipn_url {
        let u = u.trim();
        next.ipn_url = if u.is_empty() {
            None
        } else {
            let local = st.cfg.dev_mode && u.starts_with("http://");
            if !(u.starts_with("https://") || local) || u.len() > 300 || u.chars().any(|c| c.is_whitespace() || c.is_control()) {
                return Err(ApiError::validation("ipn_url", "Enter the https:// address of the IPN route"));
            }
            Some(u.to_string())
        };
    }
    if next.enabled && !(next.has_api_key() && next.has_ipn_secret()) {
        return Err(ApiError::validation("enabled", "Paste the API key and the IPN secret before switching NOWPayments on"));
    }
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "INSERT INTO nowpayments_settings (tenant_id, enabled, sandbox, api_key_enc, api_key_last4, api_key_updated_at, api_key_updated_by,
                                           ipn_secret_enc, ipn_secret_last4, ipn_secret_updated_at, ipn_secret_updated_by,
                                           coins, min_deposit_usd, max_deposit_usd, fee_paid_by_user, ipn_url, updated_at, updated_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, now(), $17)
         ON CONFLICT (tenant_id) DO UPDATE SET enabled = $2, sandbox = $3, api_key_enc = $4, api_key_last4 = $5, api_key_updated_at = $6, api_key_updated_by = $7,
             ipn_secret_enc = $8, ipn_secret_last4 = $9, ipn_secret_updated_at = $10, ipn_secret_updated_by = $11, coins = $12, min_deposit_usd = $13,
             max_deposit_usd = $14, fee_paid_by_user = $15, ipn_url = $16, updated_at = now(), updated_by = $17",
    )
    .bind(t)
    .bind(next.enabled)
    .bind(next.sandbox)
    .bind(&next.api_key_enc)
    .bind(&next.api_key_last4)
    .bind(next.api_key_updated_at)
    .bind(&next.api_key_updated_by)
    .bind(&next.ipn_secret_enc)
    .bind(&next.ipn_secret_last4)
    .bind(next.ipn_secret_updated_at)
    .bind(&next.ipn_secret_updated_by)
    .bind(&next.coins)
    .bind(next.min_deposit_usd)
    .bind(next.max_deposit_usd)
    .bind(next.fee_paid_by_user)
    .bind(&next.ipn_url)
    .bind(&tag)
    .execute(&mut *tx)
    .await?;
    let mut after = next.public_json(&st.cfg);
    after["secrets_changed"] = json!(changed);
    audit::staff(&mut tx, s_ctx, Entry { action: "wallet.settings.nowpayments", target_kind: "settings", target_id: "nowpayments".into(), reason: Some(&reason), before: Some(cur.public_json(&st.cfg)), after: Some(after) }).await?;
    tx.commit().await?;
    np::forget_mins(t);
    tracing::info!(tenant_id = t, staff = %tag, enabled = next.enabled, sandbox = next.sandbox, secrets_changed = ?changed, "NOWPayments settings changed");
    Ok(next.public_json(&st.cfg))
}

/// `POST /v1/admin/nowpayments/test`: `GET /status` and `GET /merchant/coins` with the key in the body (to try a key
/// before saving it) or the stored one.
pub async fn test_connection(st: &AppState, s_ctx: &StaffCtx, api_key: Option<String>, sandbox: Option<bool>) -> ApiResult<Value> {
    s_ctx.require_perm("finance.settings", ROLES_SETTINGS)?;
    let t = s_ctx.ctx.tenant.id;
    let cfg = settings(&st.pool, t).await?;
    let sandbox = sandbox.unwrap_or(cfg.sandbox);
    let key = match api_key.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
        Some(k) => clean_secret("api_key", k)?,
        None => open_secret(st, t, "api_key", &cfg.api_key_enc).map_err(|_| ApiError::validation("api_key", "Paste the API key first"))?,
    };
    let npc = Np::new(&st.cfg, sandbox, key);
    let status = npc.status().await.map_err(|e| provider_error(&e))?;
    let coins = npc.merchant_coins().await.map_err(|e| provider_error(&e))?;
    Ok(json!({
        "ok": status.eq_ignore_ascii_case("ok"),
        "status": status,
        "sandbox": sandbox,
        "merchant_coins": coins.iter().map(|c| np::coin_info(c)).collect::<Vec<_>>(),
        "missing": cfg.coins.iter().filter(|c| !coins.contains(c)).collect::<Vec<_>>(),
    }))
}

/// Approve & credit a payment in review. At or below the four-eyes threshold (`tenant_settings.adjust_approval_usd`,
/// the one of manual adjustments) it is credited at once (finance.write); above it the amount waits as `approval`
/// for a second staff member with finance.adjust_approve, who then credits exactly that amount.
pub async fn approve(st: &AppState, s_ctx: &StaffCtx, id: i64, amount: Option<D>, reason: &str) -> ApiResult<Value> {
    let t = s_ctx.ctx.tenant.id;
    let r = sqlx::query("SELECT * FROM nowpayments_payments WHERE id = $1 AND tenant_id = $2").bind(id).bind(t).fetch_optional(&st.pool).await?.ok_or_else(|| ApiError::not_found("Payment"))?;
    let status: String = r.get("status");
    let reason = clean_text(Some(reason), 500).unwrap_or_default();
    match status.as_str() {
        "review" => {
            s_ctx.require_perm("finance.write", ROLES_WRITE)?;
            if reason.chars().count() < 3 {
                return Err(ApiError::validation("reason", "Enter a reason (kept in the audit log)"));
            }
            let amount = amount.or_else(|| suggested_credit(&r)).ok_or_else(|| ApiError::validation("amount", "Enter the USD amount to credit"))?;
            let amount = check_amount(amount, 2).map_err(|m| ApiError::validation("amount", m))?;
            let threshold: Option<Option<D>> = sqlx::query_scalar("SELECT adjust_approval_usd FROM tenant_settings WHERE tenant_id = $1").bind(t).fetch_optional(&st.pool).await?;
            if let Some(limit) = threshold.flatten()
                && amount > limit
            {
                let n = sqlx::query(
                    "UPDATE nowpayments_payments SET status = 'approval', approval_amount = $3, approval_reason = $4, approval_by_id = $5, approval_by_name = $6, approval_at = now(), updated_at = now()
                      WHERE id = $1 AND tenant_id = $2 AND status = 'review'",
                )
                .bind(id)
                .bind(t)
                .bind(amount)
                .bind(&reason)
                .bind(&s_ctx.staff.id)
                .bind(&s_ctx.staff.name)
                .execute(&st.pool)
                .await?
                .rows_affected();
                if n == 0 {
                    return Err(ApiError::conflict("invalid_state", "This payment was decided by someone else just now"));
                }
                let mut tx = st.pool.begin().await?;
                audit::staff(&mut tx, s_ctx, Entry { action: "wallet.nowpayments.approval_requested", target_kind: "nowpayments_payment", target_id: id.to_string(), reason: Some(&reason), before: Some(json!({"status": "review"})), after: Some(json!({"status": "approval", "amount": s(amount), "threshold_usd": s(limit)})) }).await?;
                tx.commit().await?;
            } else if !credit(st, id, &["review"], amount, &s_ctx.staff.tag(), Some(s_ctx), Some(&reason)).await? {
                return Err(ApiError::conflict("invalid_state", "This payment was decided by someone else just now"));
            } else {
                sqlx::query("UPDATE nowpayments_payments SET decided_by = $2, decided_at = now(), decision_note = $3 WHERE id = $1").bind(id).bind(s_ctx.staff.tag()).bind(&reason).execute(&st.pool).await?;
            }
        }
        "approval" => {
            s_ctx.require_perm("finance.adjust_approve", ROLES_APPROVE)?;
            if r.get::<Option<String>, _>("approval_by_id").as_deref() == Some(s_ctx.staff.id.as_str()) {
                return Err(ApiError::Forbidden("Four-eyes rule: another staff member must approve your own request".into()));
            }
            let proposed: D = r.get::<Option<D>, _>("approval_amount").ok_or_else(|| ApiError::conflict("invalid_state", "No amount was proposed"))?;
            if amount.is_some_and(|a| a != proposed) {
                return Err(ApiError::conflict("invalid_state", format!("The proposed amount is {} USD; reject it to propose another", s(proposed))));
            }
            let note = if reason.is_empty() { r.get::<Option<String>, _>("approval_reason") } else { Some(reason.clone()) };
            if !credit(st, id, &["approval"], proposed, &s_ctx.staff.tag(), Some(s_ctx), note.as_deref()).await? {
                return Err(ApiError::conflict("invalid_state", "This payment was decided by someone else just now"));
            }
            sqlx::query("UPDATE nowpayments_payments SET decided_by = $2, decided_at = now(), decision_note = $3 WHERE id = $1").bind(id).bind(s_ctx.staff.tag()).bind(&note).execute(&st.pool).await?;
        }
        other => return Err(ApiError::conflict("invalid_state", format!("A payment in state {other} can't be approved"))),
    }
    Ok(admin_json(&load(&st.pool, id).await?.ok_or_else(|| ApiError::not_found("Payment"))?))
}

/// Rejects a payment in review (finance.write) or a proposal waiting for the second approval (finance.adjust_approve,
/// or the proposer withdrawing it: back to review). Nothing is booked; the client is told.
pub async fn reject(st: &AppState, s_ctx: &StaffCtx, id: i64, reason: &str) -> ApiResult<Value> {
    let t = s_ctx.ctx.tenant.id;
    let reason = clean_text(Some(reason), 500).unwrap_or_default();
    if reason.chars().count() < 3 {
        return Err(ApiError::validation("reason", "Enter a reason (kept in the audit log)"));
    }
    let r = sqlx::query("SELECT * FROM nowpayments_payments WHERE id = $1 AND tenant_id = $2").bind(id).bind(t).fetch_optional(&st.pool).await?.ok_or_else(|| ApiError::not_found("Payment"))?;
    let status: String = r.get("status");
    let own_proposal = status == "approval" && r.get::<Option<String>, _>("approval_by_id").as_deref() == Some(s_ctx.staff.id.as_str());
    match status.as_str() {
        "review" => s_ctx.require_perm("finance.write", ROLES_WRITE)?,
        "approval" if own_proposal => s_ctx.require_perm("finance.write", ROLES_WRITE)?,
        "approval" => s_ctx.require_perm("finance.adjust_approve", ROLES_APPROVE)?,
        other => return Err(ApiError::conflict("invalid_state", format!("A payment in state {other} can't be rejected"))),
    }
    let mut tx = st.pool.begin().await?;
    if own_proposal {
        // the proposer withdraws the proposal: the payment goes back to review
        sqlx::query("UPDATE nowpayments_payments SET status = 'review', approval_amount = NULL, approval_reason = NULL, approval_by_id = NULL, approval_by_name = NULL, approval_at = NULL, updated_at = now() WHERE id = $1 AND status = 'approval'")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        audit::staff(&mut tx, s_ctx, Entry { action: "wallet.nowpayments.approval_withdrawn", target_kind: "nowpayments_payment", target_id: id.to_string(), reason: Some(&reason), before: Some(json!({"status": "approval"})), after: Some(json!({"status": "review"})) }).await?;
        tx.commit().await?;
        return Ok(admin_json(&load(&st.pool, id).await?.ok_or_else(|| ApiError::not_found("Payment"))?));
    }
    let n = sqlx::query("UPDATE nowpayments_payments SET status = 'rejected', decided_by = $3, decided_at = now(), decision_note = $4, updated_at = now() WHERE id = $1 AND status = $2")
        .bind(id)
        .bind(&status)
        .bind(s_ctx.staff.tag())
        .bind(&reason)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::conflict("invalid_state", "This payment was decided by someone else just now"));
    }
    audit::staff(&mut tx, s_ctx, Entry { action: "wallet.nowpayments.rejected", target_kind: "nowpayments_payment", target_id: id.to_string(), reason: Some(&reason), before: Some(admin_json(&r)), after: Some(json!({"status": "rejected"})) }).await?;
    let coin: String = r.get("pay_currency");
    notify(
        &mut tx,
        t,
        r.get("user_id"),
        "deposit.rejected",
        "Deposit not credited",
        &format!("Your crypto deposit ({} USD in {}) was not credited: {reason}. Contact support if you need help.", s(r.get::<D, _>("price_amount")), coin.to_uppercase()),
        json!({"nowpayments_id": id}),
    )
    .await?;
    tx.commit().await?;
    Ok(admin_json(&load(&st.pool, id).await?.ok_or_else(|| ApiError::not_found("Payment"))?))
}

/// Staff "Re-check with NOWPayments" (finance.write).
pub async fn refresh(st: &AppState, s_ctx: &StaffCtx, id: i64) -> ApiResult<Value> {
    s_ctx.require_perm("finance.write", ROLES_WRITE)?;
    let t = s_ctx.ctx.tenant.id;
    sqlx::query_scalar::<_, i64>("SELECT id FROM nowpayments_payments WHERE id = $1 AND tenant_id = $2").bind(id).bind(t).fetch_optional(&st.pool).await?.ok_or_else(|| ApiError::not_found("Payment"))?;
    if let Err(e) = sync(st, id).await {
        return Err(ApiError::Coded { status: axum::http::StatusCode::BAD_GATEWAY, code: "provider_error", message: format!("The re-check failed: {e}") });
    }
    Ok(admin_json(&load(&st.pool, id).await?.ok_or_else(|| ApiError::not_found("Payment"))?))
}
