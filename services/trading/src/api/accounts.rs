//! Client Area API (called by the CRM BFF with the signed-in gateway user id in `X-Kalks-User-Id` or
//! `?user_id=`): open live/demo accounts, list/summary, demo refill, passwords, leverage, history,
//! ledger, and SSO into the terminal.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use std::sync::atomic::Ordering;

use super::terminal::{PageQ, deals_page, parse_time};
use super::{ApiError, ApiResult, AppState, Body, Ctx};
use crate::auth;
use crate::engine::funds;
use crate::model::{Account, AccountKind, Controls, DemoCfg, Status};
use crate::money::{D, de_opt_dec, num};
use crate::shard::Op;
use crate::views;

#[derive(Deserialize)]
pub struct UserQ {
    pub user_id: Option<i64>,
}

pub fn user_of(h: &HeaderMap, q: Option<i64>) -> ApiResult<i64> {
    super::user_id(h.get("x-kalks-user-id").and_then(|v| v.to_str().ok()).map(str::to_string), q)
}

/// Accounts of `user` of `kind` that count against the account limit of a `product` group (CFD / Options account
/// split: a client may hold up to the group's `maxAccountsPerUser` CFD accounts AND as many Options accounts, per
/// live / demo): not retired, not `except`, and not in a system-managed group (copy, PAMM, MAM, prop and the market
/// maker's: the platform opens those, never the client).
pub fn product_accounts_used(st: &AppState, tenant: &crate::rules::TenantConfig, user: i64, kind: AccountKind, product: crate::rules::Product, except: Option<i64>) -> u32 {
    let idx = st.hub.shared.index.read().unwrap();
    idx.accounts
        .iter()
        .filter(|(l, m)| Some(**l) != except && m.tenant_id == tenant.tenant_id && m.user_id == user && m.kind == kind && !m.status.is_retired())
        .filter(|(_, m)| !crate::engine::options::system_group(&m.group) && m.group != crate::book::LP_GROUP)
        .filter(|(_, m)| tenant.groups.get(&m.group).map(|g| g.product).unwrap_or_default() == product)
        .count() as u32
}

/// The `account_limit` refusal of a group.
pub fn limit_message(g: &crate::rules::Group, kind: AccountKind) -> String {
    format!("You can have at most {} {} {} account(s)", g.max_accounts_per_user, kind.as_str(), funds::product_name(g.product))
}

/// The account must belong to the tenant and to the calling user.
pub fn owned(st: &AppState, ctx: &Ctx, login: i64, user: i64) -> ApiResult<crate::shard::AccountMeta> {
    let m = st.hub.meta(login).ok_or_else(|| ApiError::NotFound("Account not found".into()))?;
    if m.tenant_id != ctx.tenant.tenant_id || m.user_id != user {
        return Err(ApiError::NotFound("Account not found".into()));
    }
    Ok(m)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenReq {
    #[serde(alias = "user_id")]
    user_id: Option<i64>,
    #[serde(rename = "type")]
    kind: String,
    group: String,
    leverage: Option<u32>,
    name: Option<String>,
    password: Option<String>,
    investor_password: Option<String>,
    /// Demo only: starting balance (USD), 100 – 1 000 000; default = the group's.
    #[serde(default, deserialize_with = "de_opt_dec")]
    initial_balance: Option<D>,
}

/// Opens a live or demo account in an allowed group (D8, D23). Passwords not supplied are generated and
/// returned once in `credentials`.
pub async fn open(State(st): State<AppState>, ctx: Ctx, headers: HeaderMap, Body(r): Body<OpenReq>) -> ApiResult<Json<Value>> {
    let user = user_of(&headers, r.user_id)?;
    let kind = match r.kind.as_str() {
        "live" => AccountKind::Live,
        "demo" => AccountKind::Demo,
        _ => return Err(ApiError::Validation { field: "type", message: "type must be live or demo".into() }),
    };
    let g = ctx.tenant.groups.get(&r.group).filter(|g| g.enabled).ok_or(ApiError::Validation { field: "group", message: "Unknown or disabled group".into() })?.clone();
    if !g.allows(kind.as_str()) {
        return Err(ApiError::Validation { field: "group", message: format!("{} does not offer {} accounts", g.name, kind.as_str()) });
    }
    let leverage = r.leverage.unwrap_or(g.default_leverage);
    if !g.leverages.contains(&leverage) {
        return Err(ApiError::Validation { field: "leverage", message: format!("Leverage must be one of {:?}", g.leverages) });
    }
    let mut generated = serde_json::Map::new();
    let password = match r.password {
        Some(p) => p,
        None => {
            let p = auth::generate_password();
            generated.insert("password".into(), json!(p));
            p
        }
    };
    let investor = match r.investor_password {
        Some(p) => p,
        None => {
            let p = auth::generate_password();
            generated.insert("investorPassword".into(), json!(p));
            p
        }
    };
    auth::check_password(&password).map_err(|m| ApiError::Validation { field: "password", message: m.into() })?;
    auth::check_password(&investor).map_err(|m| ApiError::Validation { field: "investorPassword", message: m.into() })?;
    if password == investor {
        return Err(ApiError::Validation { field: "investorPassword", message: "The investor password must differ from the trading password".into() });
    }
    let demo = if kind == AccountKind::Demo {
        let initial = r.initial_balance.unwrap_or(g.demo_initial_balance);
        if initial < D::from(100) || initial > D::from(1_000_000) {
            return Err(ApiError::Validation { field: "initialBalance", message: "Demo balance must be between 100 and 1 000 000".into() });
        }
        Some(DemoCfg { initial_balance: crate::money::r2(initial), refills_per_day: g.demo_refills_per_day, expiry_days: g.demo_expiry_days })
    } else {
        None
    };
    let (th, ih) = tokio::task::spawn_blocking(move || -> anyhow::Result<(String, String)> { Ok((auth::hash_password(&password)?, auth::hash_password(&investor)?)) }).await??;

    let _guard = st.open_lock.lock().await;
    if product_accounts_used(&st, &ctx.tenant, user, kind, g.product, None) >= g.max_accounts_per_user {
        return Err(ApiError::Conflict { code: "account_limit", message: limit_message(&g, kind) });
    }
    let login = match kind {
        AccountKind::Live => st.logins.live.fetch_add(1, Ordering::SeqCst) + 1,
        AccountKind::Demo => st.logins.demo.fetch_add(1, Ordering::SeqCst) + 1,
    };
    let account = Account {
        tenant_id: ctx.tenant.tenant_id,
        login,
        user_id: user,
        kind,
        group: g.code.clone(),
        mode: g.mode,
        cent: g.cent,
        leverage,
        status: Status::Active,
        name: r.name.unwrap_or_default().chars().take(80).collect(),
        route_override: None,
        controls: Controls::default(),
        demo,
        created_at: Utc::now(),
        lifecycle: None,
    };
    let v = st.hub.open(account, (th, ih), &format!("user:{user}")).await?;
    tracing::info!(login, user, kind = kind.as_str(), group = %g.code, "account opened");
    let mut creds = json!({"login": login});
    if let (Value::Object(o), false) = (&mut creds, generated.is_empty()) {
        o.extend(generated);
    }
    Ok(Json(json!({"account": v, "credentials": creds})))
}

pub async fn list(State(st): State<AppState>, ctx: Ctx, headers: HeaderMap, Query(q): Query<UserQ>) -> ApiResult<Json<Value>> {
    let user = user_of(&headers, q.user_id)?;
    let mut logins: Vec<i64> = {
        let idx = st.hub.shared.index.read().unwrap();
        idx.accounts.iter().filter(|(_, m)| m.tenant_id == ctx.tenant.tenant_id && m.user_id == user).map(|(l, _)| *l).collect()
    };
    logins.sort();
    let mut out = Vec::new();
    for l in logins {
        let v = super::terminal::account_view(&st, l).await;
        if !v.is_null() {
            out.push(v);
        }
    }
    // archive / close dates, dormancy, the open closure request and the starred default account
    super::lifecycle::decorate(&st, ctx.tenant.tenant_id, &mut out).await?;
    Ok(Json(json!({"accounts": out})))
}

pub async fn detail(State(st): State<AppState>, ctx: Ctx, headers: HeaderMap, Path(login): Path<i64>, Query(q): Query<UserQ>) -> ApiResult<Json<Value>> {
    let user = user_of(&headers, q.user_id)?;
    owned(&st, &ctx, login, user)?;
    let v = st
        .hub
        .read(
            login,
            Box::new(|x| match x {
                Some((a, env)) => json!({
                    "account": views::account_json(env, a),
                    "positions": a.positions.values().map(|p| views::position_json(env, a, p)).collect::<Vec<_>>(),
                    "orders": a.orders.values().map(views::order_json).collect::<Vec<_>>(),
                }),
                None => Value::Null,
            }),
        )
        .await;
    let mut v = v;
    if v["account"].is_object() {
        let mut one = [v["account"].clone()];
        super::lifecycle::decorate(&st, ctx.tenant.tenant_id, &mut one).await?;
        v["account"] = one[0].clone();
    }
    Ok(Json(v))
}

pub async fn demo_refill(State(st): State<AppState>, ctx: Ctx, headers: HeaderMap, Path(login): Path<i64>, Query(q): Query<UserQ>) -> ApiResult<Json<Value>> {
    let user = user_of(&headers, q.user_id)?;
    owned(&st, &ctx, login, user)?;
    let op: Op = Box::new(|tx, env| funds::demo_refill(tx, env).map(|amt| json!({"status": "ok", "amount": num(amt), "balance": num(tx.st.balance)})));
    let done = st.hub.exec(login, &format!("user:{user}"), None, "", "", None, op).await?;
    Ok(Json(done.value))
}

#[derive(Deserialize)]
pub struct PasswordReq {
    /// trading | investor
    kind: String,
    password: String,
}

/// Changes the trading or investor password (the CRM does the email-OTP step before calling, D20).
/// Existing terminal sessions of that kind are revoked.
pub async fn passwords(State(st): State<AppState>, ctx: Ctx, headers: HeaderMap, Path(login): Path<i64>, Query(q): Query<UserQ>, Body(r): Body<PasswordReq>) -> ApiResult<Json<Value>> {
    let user = user_of(&headers, q.user_id)?;
    owned(&st, &ctx, login, user)?;
    auth::check_password(&r.password).map_err(|m| ApiError::Validation { field: "password", message: m.into() })?;
    let investor = match r.kind.as_str() {
        "trading" => false,
        "investor" => true,
        _ => return Err(ApiError::Validation { field: "kind", message: "kind must be trading or investor".into() }),
    };
    let row = sqlx::query("SELECT trading_hash, investor_hash FROM account_credentials WHERE login = $1").bind(login).fetch_one(&st.pool).await?;
    let other: String = row.get(if investor { "trading_hash" } else { "investor_hash" });
    let pw = r.password.clone();
    let (same, hash) = tokio::task::spawn_blocking(move || -> anyhow::Result<(bool, String)> { Ok((auth::verify_password(&pw, &other), auth::hash_password(&pw)?)) }).await??;
    if same {
        return Err(ApiError::Validation { field: "password", message: "Trading and investor passwords must differ".into() });
    }
    let col = if investor { "investor_hash" } else { "trading_hash" };
    sqlx::query(sqlx::AssertSqlSafe(format!("UPDATE account_credentials SET {col} = $2, failed_logins = 0, locked_until = NULL, updated_at = now() WHERE login = $1"))).bind(login).bind(hash).execute(&st.pool).await?;
    let n = sqlx::query("UPDATE terminal_sessions SET revoked_at = now() WHERE login = $1 AND read_only = $2 AND revoked_at IS NULL").bind(login).bind(investor).execute(&st.pool).await?.rows_affected();
    tracing::info!(login, kind = %r.kind, sessions_revoked = n, "account password changed");
    Ok(Json(json!({"status": "ok", "sessionsRevoked": n})))
}

#[derive(Deserialize)]
pub struct LeverageReq {
    leverage: u32,
}

pub async fn leverage(State(st): State<AppState>, ctx: Ctx, headers: HeaderMap, Path(login): Path<i64>, Query(q): Query<UserQ>, Body(r): Body<LeverageReq>) -> ApiResult<Json<Value>> {
    let user = user_of(&headers, q.user_id)?;
    owned(&st, &ctx, login, user)?;
    let op: Op = Box::new(move |tx, env| funds::change_leverage(tx, env, r.leverage, false).map(|(from, to)| json!({"status": "ok", "from": from, "leverage": to})));
    Ok(Json(st.hub.exec(login, &format!("user:{user}"), None, "", "", None, op).await?.value))
}

#[derive(Deserialize)]
pub struct HistQ {
    user_id: Option<i64>,
    from: Option<String>,
    to: Option<String>,
    page: Option<i64>,
    limit: Option<i64>,
}

pub async fn history_json(st: &AppState, login: i64, q: &PageQ) -> ApiResult<Value> {
    let limit = q.limit.unwrap_or(100).clamp(1, 1000);
    let page = q.page.unwrap_or(1).max(1);
    let from = parse_time(&q.from)?;
    let to = parse_time(&q.to)?;
    let (deals, total) = deals_page(st, login, from, to, (page - 1) * limit, limit).await?;
    let orders = sqlx::query(
        "SELECT data, status, done_at, fill_price, position_ticket, reason FROM orders WHERE login = $1 AND status <> 'pending'
         AND ($2::timestamptz IS NULL OR done_at >= $2) AND ($3::timestamptz IS NULL OR done_at < $3) ORDER BY done_at DESC LIMIT $4",
    )
    .bind(login)
    .bind(from)
    .bind(to)
    .bind(limit)
    .fetch_all(&st.pool)
    .await?
    .iter()
    .map(|r| {
        let o: sqlx::types::Json<crate::model::Order> = r.get("data");
        let mut v = views::order_json(&o.0);
        v["status"] = json!(r.get::<String, _>("status"));
        v["doneAt"] = json!(r.get::<Option<chrono::DateTime<Utc>>, _>("done_at"));
        v["fillPrice"] = crate::money::num_opt(r.get::<Option<D>, _>("fill_price"));
        v["positionTicket"] = json!(r.get::<Option<i64>, _>("position_ticket"));
        v["reason"] = json!(r.get::<Option<String>, _>("reason"));
        v
    })
    .collect::<Vec<_>>();
    let totals = sqlx::query(
        "SELECT COALESCE(sum(profit),0) AS profit, COALESCE(sum(swap),0) AS swap, COALESCE(sum(CASE WHEN entry = 'in' THEN commission ELSE 0 END),0) AS commission
         FROM deals WHERE login = $1 AND NOT reversed AND ($2::timestamptz IS NULL OR time >= $2) AND ($3::timestamptz IS NULL OR time < $3)",
    )
    .bind(login)
    .bind(from)
    .bind(to)
    .fetch_one(&st.pool)
    .await?;
    Ok(json!({
        "deals": deals,
        "orders": orders,
        "page": page,
        "limit": limit,
        "total": total,
        "totals": {"profit": num(totals.get("profit")), "swap": num(totals.get("swap")), "commission": num(totals.get("commission"))},
    }))
}

pub async fn history(State(st): State<AppState>, ctx: Ctx, headers: HeaderMap, Path(login): Path<i64>, Query(q): Query<HistQ>) -> ApiResult<Json<Value>> {
    let user = user_of(&headers, q.user_id)?;
    owned(&st, &ctx, login, user)?;
    Ok(Json(history_json(&st, login, &PageQ { from: q.from, to: q.to, page: q.page, limit: q.limit }).await?))
}

pub async fn ledger_json(st: &AppState, login: i64, q: &PageQ) -> ApiResult<Value> {
    let limit = q.limit.unwrap_or(100).clamp(1, 1000);
    let page = q.page.unwrap_or(1).max(1);
    let from = parse_time(&q.from)?;
    let to = parse_time(&q.to)?;
    let prefix = format!("acct:{login}:%");
    let rows = sqlx::query(
        "SELECT t.id, t.kind, t.reference, t.reason_code, t.note, t.created_at, p.account_code, p.currency, p.amount, count(*) OVER () AS total
         FROM ledger_postings p JOIN ledger_txns t ON t.id = p.txn_id
         WHERE t.login = $1 AND p.account_code LIKE $2 AND ($3::timestamptz IS NULL OR t.created_at >= $3) AND ($4::timestamptz IS NULL OR t.created_at < $4)
         ORDER BY t.id DESC, p.id DESC OFFSET $5 LIMIT $6",
    )
    .bind(login)
    .bind(&prefix)
    .bind(from)
    .bind(to)
    .bind((page - 1) * limit)
    .bind(limit)
    .fetch_all(&st.pool)
    .await?;
    let total = rows.first().map(|r| r.get::<i64, _>("total")).unwrap_or(0);
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            let code: String = r.get("account_code");
            json!({
                "txn": r.get::<i64, _>("id"),
                "kind": r.get::<String, _>("kind"),
                "subLedger": code.rsplit(':').next().unwrap_or(""),
                "amount": num(r.get("amount")),
                "currency": r.get::<String, _>("currency"),
                "reference": r.get::<Option<String>, _>("reference"),
                "reasonCode": r.get::<Option<String>, _>("reason_code"),
                "note": r.get::<Option<String>, _>("note"),
                "at": r.get::<chrono::DateTime<Utc>, _>("created_at"),
            })
        })
        .collect();
    Ok(json!({"items": items, "page": page, "limit": limit, "total": total}))
}

pub async fn ledger(State(st): State<AppState>, ctx: Ctx, headers: HeaderMap, Path(login): Path<i64>, Query(q): Query<HistQ>) -> ApiResult<Json<Value>> {
    let user = user_of(&headers, q.user_id)?;
    owned(&st, &ctx, login, user)?;
    Ok(Json(ledger_json(&st, login, &PageQ { from: q.from, to: q.to, page: q.page, limit: q.limit }).await?))
}

/// Short-lived (60 s) one-time token for the terminal: the CRM "Trade" button opens
/// `trade.<domain>/sso?token=…`, whose BFF calls `POST /v1/terminal/sso`.
pub async fn sso(State(st): State<AppState>, ctx: Ctx, headers: HeaderMap, Path(login): Path<i64>, Query(q): Query<UserQ>) -> ApiResult<Json<Value>> {
    let user = user_of(&headers, q.user_id)?;
    let m = owned(&st, &ctx, login, user)?;
    if m.status == Status::Expired {
        return Err(ApiError::Forbidden("This demo account has expired.".into()));
    }
    if m.status.is_retired() {
        return Err(ApiError::Forbidden(format!("This account is {}.", m.status.as_str())));
    }
    super::controls::login_gate(&st, login)?;
    let token = auth::random_token(32);
    let expires = Utc::now() + Duration::seconds(60);
    sqlx::query("INSERT INTO sso_tokens (token_hash, tenant_id, login, user_id, expires_at) VALUES ($1,$2,$3,$4,$5)")
        .bind(st.keys.hash("sso", &token))
        .bind(ctx.tenant.tenant_id)
        .bind(login)
        .bind(user)
        .bind(expires)
        .execute(&st.pool)
        .await?;
    Ok(Json(json!({"token": token, "expiresAt": expires, "login": login})))
}

/// Groups a client may open (for the open-account wizard).
pub async fn groups(ctx: Ctx) -> ApiResult<Json<Value>> {
    let mut v: Vec<&crate::rules::Group> = ctx.tenant.groups.values().filter(|g| g.enabled).collect();
    v.sort_by(|a, b| a.code.cmp(&b.code));
    Ok(Json(json!({"groups": v})))
}

#[derive(Deserialize)]
pub struct SymbolsQ {
    /// comma-separated symbols (default: every instrument)
    symbols: Option<String>,
    /// `core` or `catalogue`
    tier: Option<String>,
    #[serde(rename = "assetClass")]
    asset_class: Option<String>,
}

/// Contract specifications (terminal symbol info panel, risk calculator). `liveTrading` = tradable on live
/// accounts (demo accounts trade every instrument); `core` = one of the original instruments.
pub async fn symbols(State(st): State<AppState>, ctx: Ctx, Query(q): Query<SymbolsQ>) -> ApiResult<Json<Value>> {
    let now = Utc::now();
    let _ = ctx;
    let wanted: Option<std::collections::HashSet<String>> = q.symbols.as_deref().map(|x| x.split(',').map(|s| s.trim().to_uppercase()).filter(|s| !s.is_empty()).collect());
    let specs = st.hub.shared.specs.load();
    let v: Vec<Value> = specs
        .all()
        .filter(|s| wanted.as_ref().is_none_or(|w| w.contains(&s.symbol)))
        .filter(|s| match q.tier.as_deref() {
            Some("core") => s.core,
            Some("catalogue") => !s.core,
            _ => true,
        })
        .filter(|s| q.asset_class.as_deref().is_none_or(|c| c == s.asset_class))
        .map(|s| {
            json!({
                "symbol": s.symbol, "assetClass": s.asset_class, "digits": s.digits, "point": num(s.point), "pipSize": num(s.pip_size),
                "contractSize": num(s.contract_size), "profitCurrency": s.quote_ccy, "lotMin": num(s.lot_min), "lotMax": num(s.lot_max),
                "lotStep": num(s.lot_step), "marginPct": num(s.margin_pct), "maxLeverage": s.max_leverage, "swapLong": num(s.swap_long),
                "swapShort": num(s.swap_short), "swapUnit": if s.swap_mode == crate::specs::SwapMode::Points { "points" } else { "percent_per_year" }, "tripleSwapDay": s.triple_swap_day.map(|d| d.to_string()),
                "session": s.session.key(),
                "open": s.is_open(now), "stopsLevelPoints": s.stops_level_points,
                "core": s.core, "liveTrading": s.live, "liveOff": s.live_off, "name": s.name, "baseCurrency": s.base_ccy,
                "holidayCalendar": s.holidays.as_ref().map(|h| h.calendar.clone()),
            })
        })
        .collect();
    Ok(Json(json!({"symbols": v})))
}
