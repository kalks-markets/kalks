//! NOWPayments routes: client (CRM BFF), IPN (forwarded raw by the Client Area) and Back Office. Rules in
//! `ops::nowpayments`.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;

use super::{Body, Q, ok, paging};
use crate::error::{ApiError, ApiResult};
use crate::money::{D, de_dec, de_opt_dec};
use crate::ops::{self, nowpayments as npo};
use crate::state::{AppState, Ctx, ROLES_READ, StaffCtx};

/* ---------------- client ---------------- */

pub async fn config(State(st): State<AppState>, ctx: Ctx) -> ApiResult<Json<Value>> {
    Ok(ok(npo::client_config(&st, ctx.tenant.id).await?))
}

#[derive(Deserialize)]
pub struct CreateBody {
    #[serde(default)]
    user_id: i64,
    #[serde(deserialize_with = "de_dec")]
    amount: D,
    #[serde(default)]
    pay_currency: String,
}

pub async fn create(State(st): State<AppState>, ctx: Ctx, Body(b): Body<CreateBody>) -> ApiResult<Json<Value>> {
    if b.user_id <= 0 {
        return Err(ApiError::validation("user_id", "user_id must be a positive integer"));
    }
    ops::record_ip(&st.pool, ctx.tenant.id, b.user_id, ctx.ip.as_deref()).await;
    Ok(ok(json!({"payment": npo::create(&st, &ctx, b.user_id, b.amount, &b.pay_currency).await?})))
}

#[derive(Deserialize, Default)]
pub struct ListQ {
    user_id: Option<i64>,
    status: Option<String>,
    q: Option<String>,
    page: Option<i64>,
    limit: Option<i64>,
}

pub async fn list(State(st): State<AppState>, ctx: Ctx, Q(q): Q<ListQ>) -> ApiResult<Json<Value>> {
    let user_id = q.user_id.filter(|u| *u > 0).ok_or_else(|| ApiError::validation("user_id", "user_id must be a positive integer"))?;
    let (page, limit, offset) = paging(q.page, q.limit, 25, 100);
    let rows = sqlx::query(
        "SELECT *, count(*) OVER () AS total FROM nowpayments_payments
          WHERE tenant_id = $1 AND user_id = $2 AND status NOT IN ('creating', 'error') ORDER BY id DESC LIMIT $3 OFFSET $4",
    )
    .bind(ctx.tenant.id)
    .bind(user_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&st.pool)
    .await?;
    let total = rows.first().map(|r| r.get::<i64, _>("total")).unwrap_or(0);
    Ok(ok(json!({"items": rows.iter().map(npo::client_json).collect::<Vec<_>>(), "page": page, "limit": limit, "total": total})))
}

#[derive(Deserialize, Default)]
pub struct UserQ {
    user_id: Option<i64>,
}

pub async fn get(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>, Q(q): Q<UserQ>) -> ApiResult<Json<Value>> {
    Ok(ok(json!({"payment": npo::get(&st, ctx.tenant.id, q.user_id, id).await?})))
}

/* ---------------- IPN ---------------- */

/// `POST /v1/nowpayments/ipn`: the raw body exactly as NOWPayments sent it, with `x-nowpayments-sig`.
pub async fn ipn(State(st): State<AppState>, headers: HeaderMap, body: Bytes) -> ApiResult<Json<Value>> {
    let sig = headers.get("x-nowpayments-sig").and_then(|v| v.to_str().ok());
    Ok(ok(npo::ipn(&st, &body, sig).await?))
}

/* ---------------- Back Office ---------------- */

pub async fn admin_settings(State(st): State<AppState>, s_ctx: StaffCtx) -> ApiResult<Json<Value>> {
    s_ctx.require_perm("finance.read", ROLES_READ)?;
    let cfg = npo::settings(&st.pool, s_ctx.ctx.tenant.id).await?;
    let history: Vec<Value> = sqlx::query("SELECT * FROM audit_log WHERE tenant_id = $1 AND action = 'wallet.settings.nowpayments' ORDER BY id DESC LIMIT 30")
        .bind(s_ctx.ctx.tenant.id)
        .fetch_all(&st.pool)
        .await?
        .iter()
        .map(super::admin::audit_json)
        .collect();
    Ok(ok(json!({
        "settings": cfg.public_json(&st.cfg),
        "secrets_storage": crate::secrets::key(&st.cfg).is_some(),
        "default_coins": crate::nowpayments::DEFAULT_COINS,
        "history": history,
    })))
}

pub async fn admin_update_settings(State(st): State<AppState>, s_ctx: StaffCtx, Body(b): Body<npo::SettingsPatch>) -> ApiResult<Json<Value>> {
    npo::update_settings(&st, &s_ctx, b).await?;
    admin_settings(State(st), s_ctx).await
}

#[derive(Deserialize, Default)]
pub struct TestBody {
    #[serde(default)]
    api_key: Option<String>,
    #[serde(default)]
    sandbox: Option<bool>,
}

pub async fn admin_test(State(st): State<AppState>, s_ctx: StaffCtx, Body(b): Body<TestBody>) -> ApiResult<Json<Value>> {
    Ok(ok(npo::test_connection(&st, &s_ctx, b.api_key, b.sandbox).await?))
}

pub async fn admin_list(State(st): State<AppState>, s_ctx: StaffCtx, Q(q): Q<ListQ>) -> ApiResult<Json<Value>> {
    s_ctx.require_perm("finance.read", ROLES_READ)?;
    let (page, limit, offset) = paging(q.page, q.limit, 50, 200);
    let status = q.status.as_deref().map(str::trim).filter(|s| !s.is_empty() && *s != "all");
    if let Some(s) = status
        && !matches!(s, "queue" | "open" | "credited" | "review" | "approval" | "rejected" | "closed" | "error" | "creating")
    {
        return Err(ApiError::BadRequest("Unknown status".into()));
    }
    let term = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(|s| s.to_ascii_lowercase());
    let rows = sqlx::query(
        "SELECT *, count(*) OVER () AS total FROM nowpayments_payments
          WHERE tenant_id = $1
            AND ($2::text IS NULL OR status = $2 OR ($2 = 'queue' AND status IN ('review', 'approval')))
            AND ($3::bigint IS NULL OR user_id = $3)
            AND ($4::text IS NULL OR np_payment_id = $4 OR order_id = $4 OR lower(pay_address) = $4 OR lower(payin_hash) = $4 OR id::text = $4)
          ORDER BY CASE WHEN status IN ('review', 'approval') THEN 0 ELSE 1 END, id DESC LIMIT $5 OFFSET $6",
    )
    .bind(s_ctx.ctx.tenant.id)
    .bind(status)
    .bind(q.user_id)
    .bind(term)
    .bind(limit)
    .bind(offset)
    .fetch_all(&st.pool)
    .await?;
    let total = rows.first().map(|r| r.get::<i64, _>("total")).unwrap_or(0);
    let counts = sqlx::query(
        "SELECT count(*) FILTER (WHERE status = 'review') AS review, count(*) FILTER (WHERE status = 'approval') AS approval,
                count(*) FILTER (WHERE status = 'open') AS open,
                count(*) FILTER (WHERE status = 'credited' AND credited_at >= now() - interval '24 hours') AS credited_24h,
                COALESCE(sum(credit_amount) FILTER (WHERE status = 'credited' AND credited_at >= now() - interval '24 hours'), 0) AS credited_24h_usd
           FROM nowpayments_payments WHERE tenant_id = $1",
    )
    .bind(s_ctx.ctx.tenant.id)
    .fetch_one(&st.pool)
    .await?;
    Ok(ok(json!({
        "items": rows.iter().map(npo::admin_json).collect::<Vec<_>>(),
        "page": page, "limit": limit, "total": total,
        "counts": {
            "review": counts.get::<i64, _>("review"), "approval": counts.get::<i64, _>("approval"), "open": counts.get::<i64, _>("open"),
            "credited_24h": counts.get::<i64, _>("credited_24h"), "credited_24h_usd": crate::money::s(counts.get::<D, _>("credited_24h_usd")),
        },
    })))
}

pub async fn admin_get(State(st): State<AppState>, s_ctx: StaffCtx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    s_ctx.require_perm("finance.read", ROLES_READ)?;
    let t = s_ctx.ctx.tenant.id;
    let r = sqlx::query("SELECT * FROM nowpayments_payments WHERE id = $1 AND tenant_id = $2").bind(id).bind(t).fetch_optional(&st.pool).await?.ok_or_else(|| ApiError::not_found("Payment"))?;
    let history: Vec<Value> = sqlx::query("SELECT * FROM audit_log WHERE tenant_id = $1 AND target_kind = 'nowpayments_payment' AND target_id = $2 ORDER BY id")
        .bind(t)
        .bind(id.to_string())
        .fetch_all(&st.pool)
        .await?
        .iter()
        .map(super::admin::audit_json)
        .collect();
    let threshold: Option<Option<D>> = sqlx::query_scalar("SELECT adjust_approval_usd FROM tenant_settings WHERE tenant_id = $1").bind(t).fetch_optional(&st.pool).await?;
    let mut v = npo::admin_json(&r);
    v["history"] = json!(history);
    v["threshold_usd"] = crate::money::s_opt(threshold.flatten());
    v["can"] = json!({
        "write": s_ctx.has_perm("finance.write", crate::state::ROLES_WRITE),
        "approve": s_ctx.has_perm("finance.adjust_approve", crate::state::ROLES_APPROVE),
        "own_proposal": r.get::<Option<String>, _>("approval_by_id").as_deref() == Some(s_ctx.staff.id.as_str()),
    });
    Ok(ok(json!({"payment": v})))
}

#[derive(Deserialize, Default)]
pub struct DecideBody {
    #[serde(default, deserialize_with = "de_opt_dec")]
    amount: Option<D>,
    #[serde(default)]
    reason: String,
}

pub async fn admin_approve(State(st): State<AppState>, s_ctx: StaffCtx, Path(id): Path<i64>, Body(b): Body<DecideBody>) -> ApiResult<Json<Value>> {
    Ok(ok(json!({"payment": npo::approve(&st, &s_ctx, id, b.amount, &b.reason).await?})))
}

pub async fn admin_reject(State(st): State<AppState>, s_ctx: StaffCtx, Path(id): Path<i64>, Body(b): Body<DecideBody>) -> ApiResult<Json<Value>> {
    Ok(ok(json!({"payment": npo::reject(&st, &s_ctx, id, &b.reason).await?})))
}

pub async fn admin_refresh(State(st): State<AppState>, s_ctx: StaffCtx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    Ok(ok(json!({"payment": npo::refresh(&st, &s_ctx, id).await?})))
}
