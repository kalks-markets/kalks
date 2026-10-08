//! MAM routes (see README "MAM (multi-account manager)"):
//! - Client Area (BFF sends the signed-in user in `X-Kalks-User-Id`): find a programme, link an existing live
//!   account with an explicit consent to the terms, see and limit the link, revoke it;
//! - manager (the same user as the approved master): open the programme, dashboard, per-account multiplier /
//!   percent, allocation preview and audit;
//! - Kalks Trader (terminal session): the MAM role and allocation summary of the signed-in account;
//! - Back Office (`/v1/social/admin/mam/*`): programmes, links, allocation audit, emergency stop, stop a link.
//!   Reads: any staff role. Writes: `ROLES_SOCIAL_WRITE`, with a note, audited as `social.mam.*`.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::social::{dec, fee_rows, soc, user};
use super::social_admin::ROLES_SOCIAL_WRITE;
use super::{ApiError, ApiResult, AppState, Body, Ctx, StaffCtx};
use crate::money::{D, ZERO, num};
use crate::shard::Staff;
use crate::social::allocation::Method;
use crate::social::mam::{Link, Manager};
use crate::social::pamm::SocErr;
use crate::social::{Master, valid_period};

fn bad(code: &'static str, message: impl Into<String>) -> ApiError {
    soc(SocErr::new(code, message))
}

fn master_of_user(st: &AppState, tenant: i64, user: i64) -> Option<Master> {
    let reg = st.social.reg.read().unwrap();
    reg.masters.values().filter(|m| m.tenant_id == tenant && m.user_id == user).max_by_key(|m| (m.status != "rejected", m.id)).cloned()
}

fn own_manager(st: &AppState, tenant: i64, user: i64) -> Option<Manager> {
    let m = master_of_user(st, tenant, user)?;
    st.social.reg.read().unwrap().manager_of_master(m.id).filter(|x| x.open()).cloned()
}

fn client_staff(user: i64) -> Staff {
    Staff { id: format!("user:{user}"), name: "Client".into(), role: "client".into() }
}

/// Optional limit: absent = unchanged (None), null = cleared (Some(None)).
fn opt_limit(b: &Map<String, Value>, k: &'static str) -> ApiResult<Option<Option<D>>> {
    if !b.contains_key(k) {
        return Ok(None);
    }
    let v = dec(b.get(k), k)?;
    match (k, v) {
        ("maxLot", Some(x)) if x <= ZERO || x > D::from(100) => Err(ApiError::Validation { field: "maxLot", message: "Max lot must be above 0 and at most 100".into() }),
        ("equityStop", Some(x)) if x <= ZERO => Err(ApiError::Validation { field: "equityStop", message: "Equity stop must be above 0".into() }),
        _ => Ok(Some(v)),
    }
}

async fn track(st: &AppState, master: i64) -> Value {
    let m = st.social.reg.read().unwrap().masters.get(&master).cloned();
    let Some(m) = m else { return Value::Null };
    let pts = st.social.points(&[m.login]).await.remove(&m.login).unwrap_or_default();
    let s = st.social.stats_for(m.login, pts).await;
    json!({"return1m": s.return_1m, "return1y": s.return_1y, "returnAll": s.return_all, "maxDd": s.max_dd, "riskScore": s.risk, "since": m.approved_at})
}

/* ------------------------------------------------------------------ */
/* Client Area: programmes and links                                   */
/* ------------------------------------------------------------------ */

pub async fn managers(State(st): State<AppState>, ctx: Ctx) -> ApiResult<Json<Value>> {
    let tenant = ctx.tenant.tenant_id;
    let list = st.social.managers_where(|m| m.tenant_id == tenant && m.status == "active");
    let visible: Vec<Manager> = {
        let reg = st.social.reg.read().unwrap();
        list.into_iter().filter(|m| reg.masters.get(&m.master_id).is_some_and(|x| x.status == "approved" && !x.hidden && !x.frozen)).collect()
    };
    let mut items = Vec::new();
    for m in &visible {
        let mut v = st.social.manager_json(m, false);
        v["track"] = track(&st, m.master_id).await;
        items.push(v);
    }
    Ok(Json(json!({"items": items})))
}

pub async fn manager_detail(State(st): State<AppState>, ctx: Ctx, h: HeaderMap, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let u = user(&h)?;
    let m = st.social.manager(id).filter(|m| m.tenant_id == ctx.tenant.tenant_id && m.open()).ok_or_else(|| ApiError::NotFound("MAM programme not found".into()))?;
    let (text, hash) = st.social.manager_terms(&m);
    let mut v = st.social.manager_json(&m, false);
    v["track"] = track(&st, m.master_id).await;
    Ok(Json(json!({
        "manager": v,
        "terms": {"text": text, "hash": hash},
        "accounts": st.social.link_candidates(ctx.tenant.tenant_id, u, Some(&m)).await,
        "own": m.user_id == u,
    })))
}

pub async fn links(State(st): State<AppState>, ctx: Ctx, h: HeaderMap) -> ApiResult<Json<Value>> {
    let u = user(&h)?;
    let tenant = ctx.tenant.tenant_id;
    let list = st.social.links_where(|l| l.tenant_id == tenant && l.user_id == u);
    let mut items = Vec::new();
    for l in &list {
        items.push(st.social.link_json(l, "client").await);
    }
    Ok(Json(json!({"items": items, "accounts": st.social.link_candidates(tenant, u, None).await})))
}

fn own_link(st: &AppState, ctx: &Ctx, u: i64, id: i64) -> ApiResult<Link> {
    st.social.link(id).filter(|l| l.tenant_id == ctx.tenant.tenant_id && l.user_id == u).ok_or_else(|| ApiError::NotFound("Link not found".into()))
}

pub async fn create_link(State(st): State<AppState>, ctx: Ctx, h: HeaderMap, Body(b): Body<Map<String, Value>>) -> ApiResult<Json<Value>> {
    crate::modules::require(&st, &ctx.tenant.slug, "mam").await?;
    let u = user(&h)?;
    super::controls::social_gate(&st, u)?;
    let manager = b.get("managerId").and_then(Value::as_i64).ok_or(ApiError::Validation { field: "managerId", message: "Choose a MAM programme".into() })?;
    let login = b.get("login").and_then(Value::as_i64).ok_or(ApiError::Validation { field: "login", message: "Choose the account to link".into() })?;
    if b.get("accept").and_then(Value::as_bool) != Some(true) {
        return Err(ApiError::Validation { field: "accept", message: "Read and accept the terms to continue".into() });
    }
    let hash = b.get("termsHash").and_then(Value::as_str).filter(|h| h.len() == 64).ok_or(ApiError::Validation { field: "termsHash", message: "Read and accept the terms to continue".into() })?;
    let max_lot = opt_limit(&b, "maxLot")?.flatten();
    let equity_stop = opt_limit(&b, "equityStop")?.flatten();
    let l = st.social.create_link(ctx.tenant.tenant_id, u, manager, login, max_lot, equity_stop, hash, Some(ctx.ip.clone()), Some(ctx.user_agent.clone())).await.map_err(soc)?;
    st.social.audit(l.tenant_id, &client_staff(u), "social.mam.link", Some(login), &format!("mam-link:{}", l.id), None, Some(json!({"linkId": l.id, "managerId": manager, "login": login, "termsHash": hash, "ip": ctx.ip})), "Client consented to the MAM terms").await;
    Ok(Json(json!({"link": st.social.link_json(&l, "client").await})))
}

pub async fn link(State(st): State<AppState>, ctx: Ctx, h: HeaderMap, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let u = user(&h)?;
    let l = own_link(&st, &ctx, u, id)?;
    let detail = st
        .hub
        .read(
            l.login,
            Box::new(|x| match x {
                Some((a, env)) => json!({
                    "positions": a.positions.values().filter(|p| p.source == crate::model::Source::Mam).map(|p| crate::views::position_json(env, a, p)).collect::<Vec<_>>(),
                    "orders": a.orders.values().filter(|o| o.source == crate::model::Source::Mam).map(crate::views::order_json).collect::<Vec<_>>(),
                }),
                None => json!({"positions": [], "orders": []}),
            }),
        )
        .await;
    let terms: Option<String> = sqlx::query_scalar("SELECT consent_terms FROM mam_links WHERE id = $1").bind(id).fetch_optional(&st.pool).await?;
    Ok(Json(json!({
        "link": st.social.link_json(&l, "client").await,
        "positions": detail["positions"], "orders": detail["orders"],
        "deals": st.social.mam_deals(&l, 200).await,
        "log": st.social.mam_log(id, 100).await,
        "fees": fee_rows(&st, "f.link_id = $1", id).await?,
        "terms": terms,
    })))
}

pub async fn update_link(State(st): State<AppState>, ctx: Ctx, h: HeaderMap, Path(id): Path<i64>, Body(b): Body<Map<String, Value>>) -> ApiResult<Json<Value>> {
    let u = user(&h)?;
    let mut l = own_link(&st, &ctx, u, id)?;
    if !l.active() {
        return Err(bad("link_status", "This link has ended"));
    }
    if let Some(v) = opt_limit(&b, "maxLot")? {
        l.max_lot = v;
    }
    if let Some(v) = opt_limit(&b, "equityStop")? {
        if let Some(es) = v {
            let eq = st.social.account_brief(l.login).await.map(|b| b.equity).unwrap_or(ZERO);
            if es >= eq {
                return Err(ApiError::Validation { field: "equityStop", message: format!("The equity stop must be below the account's equity ({} USD)", crate::money::r2(eq).normalize()) });
            }
        }
        l.equity_stop = v;
    }
    st.social.save_link(&l).await?;
    Ok(Json(json!({"link": st.social.link_json(&l, "client").await})))
}

pub async fn revoke_link(State(st): State<AppState>, ctx: Ctx, h: HeaderMap, Path(id): Path<i64>, body: Option<Json<Map<String, Value>>>) -> ApiResult<Json<Value>> {
    let u = user(&h)?;
    let l = own_link(&st, &ctx, u, id)?;
    if !l.active() {
        return Err(bad("link_status", "This link has already ended"));
    }
    let close = body.as_ref().and_then(|b| b.get("closePositions")).and_then(Value::as_bool).unwrap_or(false);
    let out = st.social.end_link(id, "revoked", "client", &format!("user:{u}"), close).await?;
    st.social.audit(l.tenant_id, &client_staff(u), "social.mam.revoke", Some(l.login), &format!("mam-link:{id}"), Some(json!({"linkId": id, "status": "active"})), Some(json!({"linkId": id, "status": "revoked", "closePositions": close, "closed": out["closed"].as_array().map(|a| a.len())})), "Client revoked the MAM authority").await;
    let l = st.social.link(id).unwrap_or(l);
    let mut v = out;
    v["link"] = st.social.link_json(&l, "client").await;
    Ok(Json(v))
}

/* ------------------------------------------------------------------ */
/* Manager                                                             */
/* ------------------------------------------------------------------ */

pub async fn manager_me(State(st): State<AppState>, ctx: Ctx, h: HeaderMap) -> ApiResult<Json<Value>> {
    let u = user(&h)?;
    let tenant = ctx.tenant.tenant_id;
    let master = master_of_user(&st, tenant, u);
    let settings = st.social.settings(tenant);
    let base = json!({
        "master": master.as_ref().map(|m| json!({"id": m.id, "nickname": m.nickname, "status": m.status, "frozen": m.frozen})),
        "settings": {"feeMinPct": num(settings.fee_min_pct), "feeMaxPct": num(settings.fee_max_pct), "mgmtMaxPct": 10, "platformCutPct": num(settings.platform_cut_pct)},
    });
    let Some(m) = own_manager(&st, tenant, u) else {
        let mut v = base;
        v["manager"] = Value::Null;
        return Ok(Json(v));
    };
    let list = st.social.links_where(|l| l.manager_id == m.id);
    let mut links = Vec::new();
    for l in &list {
        links.push(st.social.link_json(l, "manager").await);
    }
    let mut fees = fee_rows(&st, "f.source = 'mam' AND f.link_id IN (SELECT id FROM mam_links WHERE manager_id = $1)", m.id).await?;
    for f in fees.iter_mut() {
        if let Some(o) = f.as_object_mut() {
            o.remove("payerUserId");
            o.remove("reviewedBy");
            if let Some(l) = o.get("login").and_then(Value::as_i64) {
                o.insert("login".into(), json!(crate::social::mam::mask(l)));
            }
        }
    }
    let (text, hash) = st.social.manager_terms(&m);
    let mut v = base;
    v["manager"] = st.social.manager_json(&m, true);
    v["totals"] = st.social.manager_totals(&m).await;
    v["links"] = json!(links);
    v["allocations"] = json!(st.social.allocations("manager_id = $1", m.id, 50, true).await);
    v["fees"] = json!(fees);
    v["terms"] = json!({"text": text, "hash": hash});
    Ok(Json(v))
}

fn method_of(b: &Map<String, Value>) -> ApiResult<Option<Method>> {
    match b.get("method") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_str().and_then(Method::parse).map(Some).ok_or(ApiError::Validation { field: "method", message: "method must be equity, balance, multiplier or percent".into() }),
    }
}

fn text_field(b: &Map<String, Value>, k: &'static str, min: usize, max: usize, label: &str) -> ApiResult<Option<String>> {
    match b.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => {
            let s = s.trim();
            if s.chars().count() < min || s.chars().count() > max {
                return Err(ApiError::Validation { field: k, message: format!("{label}: {min}–{max} characters") });
            }
            Ok(Some(s.to_string()))
        }
        _ => Err(ApiError::Validation { field: k, message: format!("{label} must be text") }),
    }
}

pub async fn create_manager(State(st): State<AppState>, ctx: Ctx, h: HeaderMap, Body(b): Body<Map<String, Value>>) -> ApiResult<Json<Value>> {
    crate::modules::require(&st, &ctx.tenant.slug, "mam").await?;
    let u = user(&h)?;
    super::controls::social_gate(&st, u)?;
    let tenant = ctx.tenant.tenant_id;
    let master = master_of_user(&st, tenant, u).filter(|m| m.status == "approved").ok_or_else(|| bad("not_master", "Only approved masters can run a MAM programme. Apply as a master first (Social → Become a master)."))?;
    let name = text_field(&b, "name", 3, 60, "Programme name")?.ok_or(ApiError::Validation { field: "name", message: "Enter a programme name".into() })?;
    let description = text_field(&b, "description", 0, 1000, "Description")?.unwrap_or_default();
    let method = method_of(&b)?.ok_or(ApiError::Validation { field: "method", message: "Choose an allocation method".into() })?;
    let perf = dec(b.get("perfFeePct"), "perfFeePct")?.ok_or(ApiError::Validation { field: "perfFeePct", message: "Enter the performance fee".into() })?;
    let mgmt = dec(b.get("mgmtFeePct"), "mgmtFeePct")?.unwrap_or(ZERO);
    let period = b.get("feePeriod").and_then(Value::as_str).unwrap_or(&master.fee_period).to_string();
    let min_equity = dec(b.get("minEquity"), "minEquity")?.unwrap_or(ZERO);
    let seed = dec(b.get("seed"), "seed")?.filter(|s| *s > ZERO);
    let (m, creds) = st.social.create_manager(&master, &name, &description, method, perf, mgmt, &period, min_equity, seed).await.map_err(soc)?;
    st.social.audit(tenant, &client_staff(u), "social.mam.create", Some(m.login), &format!("mam:{}", m.id), None, Some(st.social.manager_json(&m, true)), "MAM programme opened").await;
    Ok(Json(json!({"manager": st.social.manager_json(&m, true), "credentials": creds})))
}

pub async fn update_manager(State(st): State<AppState>, ctx: Ctx, h: HeaderMap, Body(b): Body<Map<String, Value>>) -> ApiResult<Json<Value>> {
    let u = user(&h)?;
    let mut m = own_manager(&st, ctx.tenant.tenant_id, u).ok_or_else(|| bad("not_manager", "You don't run a MAM programme"))?;
    let s = st.social.settings(m.tenant_id);
    if let Some(n) = text_field(&b, "name", 3, 60, "Programme name")? {
        if st.social.reg.read().unwrap().managers.values().any(|x| x.id != m.id && x.tenant_id == m.tenant_id && x.open() && x.name.eq_ignore_ascii_case(&n)) {
            return Err(ApiError::Conflict { code: "exists", message: "This programme name is taken".into() });
        }
        m.name = n;
    }
    if let Some(d) = text_field(&b, "description", 0, 1000, "Description")? {
        m.description = d;
    }
    if let Some(v) = dec(b.get("minEquity"), "minEquity")? {
        m.min_equity = v.max(ZERO);
    }
    if let Some(p) = dec(b.get("perfFeePct"), "perfFeePct")? {
        if p < s.fee_min_pct || p > s.fee_max_pct {
            return Err(bad("fee_out_of_range", format!("The performance fee must be between {}% and {}%", s.fee_min_pct.normalize(), s.fee_max_pct.normalize())));
        }
        m.perf_fee_pct = p;
    }
    if let Some(p) = dec(b.get("mgmtFeePct"), "mgmtFeePct")? {
        if p < ZERO || p > D::from(10) {
            return Err(bad("fee_out_of_range", "The management fee must be between 0% and 10% a year"));
        }
        m.mgmt_fee_pct = p;
    }
    if let Some(p) = b.get("feePeriod").and_then(Value::as_str) {
        if !valid_period(p) {
            return Err(ApiError::Validation { field: "feePeriod", message: "feePeriod must be daily, weekly or monthly".into() });
        }
        m.fee_period = p.into();
    }
    if let Some(method) = method_of(&b)?
        && method != m.method
    {
        if st.social.reg.read().unwrap().links_of(m.id).next().is_some() {
            return Err(bad("links_active", "The allocation method can only change while no accounts are linked"));
        }
        m.method = method;
    }
    st.social.save_manager(&m).await?;
    Ok(Json(json!({"manager": st.social.manager_json(&m, true)})))
}

/// The manager sets the multiplier / percent of one linked account.
pub async fn set_link_value(State(st): State<AppState>, ctx: Ctx, h: HeaderMap, Path(id): Path<i64>, Body(b): Body<Map<String, Value>>) -> ApiResult<Json<Value>> {
    let u = user(&h)?;
    let m = own_manager(&st, ctx.tenant.tenant_id, u).ok_or_else(|| bad("not_manager", "You don't run a MAM programme"))?;
    let mut l = st.social.link(id).filter(|l| l.manager_id == m.id && l.active()).ok_or_else(|| ApiError::NotFound("Linked account not found".into()))?;
    if m.method.proportional() {
        return Err(bad("method", "Your programme allocates by equity or balance share; there is no per-account value to set"));
    }
    let v = dec(b.get("value"), "value")?.ok_or(ApiError::Validation { field: "value", message: "Enter the value".into() })?;
    if !m.method.valid_value(v) {
        let msg = if m.method == Method::Percent { "Percent must be between 0.01 and 1000" } else { "Multiplier must be between 0.01 and 100" };
        return Err(ApiError::Validation { field: "value", message: msg.into() });
    }
    let before = l.alloc_value;
    l.alloc_value = v;
    st.social.save_link(&l).await?;
    st.social.audit(l.tenant_id, &client_staff(u), "social.mam.value", Some(l.login), &format!("mam-link:{id}"), Some(json!({"linkId": id, "value": num(before)})), Some(json!({"linkId": id, "value": num(v), "method": m.method.as_str()})), "Manager changed the allocation value").await;
    Ok(Json(json!({"link": st.social.link_json(&l, "manager").await})))
}

#[derive(Deserialize)]
pub struct PreviewQ {
    symbol: Option<String>,
    volume: Option<String>,
}

fn preview_args(q: &PreviewQ) -> ApiResult<(String, D)> {
    let symbol = q.symbol.as_deref().unwrap_or("EURUSD").trim().to_uppercase();
    let volume = match q.volume.as_deref() {
        None | Some("") => D::ONE,
        Some(v) => v.parse::<D>().ok().filter(|v| *v > ZERO && *v <= D::from(1000)).ok_or(ApiError::Validation { field: "volume", message: "volume must be above 0 and at most 1000".into() })?,
    };
    Ok((symbol, volume))
}

pub async fn preview(State(st): State<AppState>, ctx: Ctx, h: HeaderMap, Query(q): Query<PreviewQ>) -> ApiResult<Json<Value>> {
    let u = user(&h)?;
    let m = own_manager(&st, ctx.tenant.tenant_id, u).ok_or_else(|| bad("not_manager", "You don't run a MAM programme"))?;
    let (symbol, volume) = preview_args(&q)?;
    Ok(Json(st.social.mam_preview(&m, &symbol, volume).await.map_err(soc)?))
}

#[derive(Deserialize)]
pub struct LimitQ {
    limit: Option<i64>,
    #[serde(rename = "managerId")]
    manager_id: Option<i64>,
    status: Option<String>,
}

pub async fn manager_allocations(State(st): State<AppState>, ctx: Ctx, h: HeaderMap, Query(q): Query<LimitQ>) -> ApiResult<Json<Value>> {
    let u = user(&h)?;
    let m = own_manager(&st, ctx.tenant.tenant_id, u).ok_or_else(|| bad("not_manager", "You don't run a MAM programme"))?;
    Ok(Json(json!({"items": st.social.allocations("manager_id = $1", m.id, q.limit.unwrap_or(100), true).await})))
}

/* ------------------------------------------------------------------ */
/* Kalks Trader                                                        */
/* ------------------------------------------------------------------ */

pub async fn terminal(State(st): State<AppState>, ctx: Ctx, Query(q): Query<PreviewQ>) -> ApiResult<Json<Value>> {
    let s = super::terminal::session(&st, &ctx).await?;
    let (symbol, volume) = preview_args(&q)?;
    let sym = q.symbol.as_ref().map(|_| symbol);
    Ok(Json(st.social.terminal_mam(s.login, sym.as_deref(), Some(volume)).await))
}

/* ------------------------------------------------------------------ */
/* Back Office                                                         */
/* ------------------------------------------------------------------ */

fn note(b: &Map<String, Value>) -> ApiResult<String> {
    let n = b.get("note").and_then(Value::as_str).map(str::trim).unwrap_or("");
    if n.is_empty() {
        return Err(ApiError::Validation { field: "note", message: "A note (reason) is required".into() });
    }
    Ok(n.chars().take(500).collect())
}

pub async fn admin_managers(State(st): State<AppState>, s: StaffCtx) -> ApiResult<Json<Value>> {
    let tenant = s.ctx.tenant.tenant_id;
    let list = st.social.managers_where(|m| m.tenant_id == tenant);
    let mut items = Vec::new();
    for m in &list {
        let mut v = st.social.manager_json(m, true);
        v["userId"] = json!(m.user_id);
        v["totals"] = st.social.manager_totals(m).await;
        items.push(v);
    }
    let fees: (i64, D) = sqlx::query_as("SELECT count(*), COALESCE(sum(amount), 0) FROM social_fees WHERE tenant_id = $1 AND source = 'mam' AND status = 'pending'").bind(tenant).fetch_one(&st.pool).await?;
    Ok(Json(json!({"items": items, "feesPending": {"count": fees.0, "amount": num(fees.1)}})))
}

pub async fn admin_links(State(st): State<AppState>, s: StaffCtx, Query(q): Query<LimitQ>) -> ApiResult<Json<Value>> {
    let tenant = s.ctx.tenant.tenant_id;
    let list = st.social.links_where(|l| l.tenant_id == tenant && q.manager_id.is_none_or(|m| l.manager_id == m) && q.status.as_deref().is_none_or(|v| v == "all" || l.status == v));
    let mut items = Vec::new();
    for l in list.iter().take(q.limit.unwrap_or(500).clamp(1, 1000) as usize) {
        items.push(st.social.link_json(l, "admin").await);
    }
    Ok(Json(json!({"items": items})))
}

pub async fn admin_allocations(State(st): State<AppState>, s: StaffCtx, Query(q): Query<LimitQ>) -> ApiResult<Json<Value>> {
    let tenant = s.ctx.tenant.tenant_id;
    let items = match q.manager_id {
        Some(m) => {
            st.social.manager(m).filter(|x| x.tenant_id == tenant).ok_or_else(|| ApiError::NotFound("MAM programme not found".into()))?;
            st.social.allocations("manager_id = $1", m, q.limit.unwrap_or(200), false).await
        }
        None => st.social.allocations("tenant_id = $1", tenant, q.limit.unwrap_or(200), false).await,
    };
    Ok(Json(json!({"items": items})))
}

pub async fn admin_emergency(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>, Body(b): Body<Map<String, Value>>) -> ApiResult<Json<Value>> {
    s.require(ROLES_SOCIAL_WRITE)?;
    let n = note(&b)?;
    let m = st.social.manager(id).filter(|m| m.tenant_id == s.ctx.tenant.tenant_id).ok_or_else(|| ApiError::NotFound("MAM programme not found".into()))?;
    let freeze = b.get("freeze").and_then(Value::as_bool).unwrap_or(true);
    let close = b.get("closePositions").and_then(Value::as_bool).unwrap_or(false);
    let before = json!({"managerId": id, "status": m.status});
    let out = st.social.mam_freeze(id, freeze, close, &n, Some(s.staff.clone())).await.map_err(|e| ApiError::Status { status: 422, code: "mam_status", message: e.to_string() })?;
    let m = st.social.manager(id).unwrap_or(m);
    let a = st.social.audit(m.tenant_id, &s.staff, if freeze { "social.mam.emergency_stop" } else { "social.mam.unfreeze" }, Some(m.login), &format!("mam:{id}"), Some(before), Some(json!({"managerId": id, "status": m.status, "closed": out["closed"].as_array().map(|a| a.len())})), &n).await;
    Ok(Json(json!({"manager": st.social.manager_json(&m, true), "result": out, "audit": [a]})))
}

pub async fn admin_stop_link(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>, Body(b): Body<Map<String, Value>>) -> ApiResult<Json<Value>> {
    s.require(ROLES_SOCIAL_WRITE)?;
    let n = note(&b)?;
    let l = st.social.link(id).filter(|l| l.tenant_id == s.ctx.tenant.tenant_id).ok_or_else(|| ApiError::NotFound("Link not found".into()))?;
    if !l.active() {
        return Err(ApiError::Conflict { code: "link_status", message: format!("This link is already {}", l.status) });
    }
    let close = b.get("closePositions").and_then(Value::as_bool).unwrap_or(false);
    let out = st.social.end_link(id, "stopped", "admin", &format!("staff:{}", s.staff.id), close).await?;
    let a = st.social.audit(l.tenant_id, &s.staff, "social.mam.link_stop", Some(l.login), &format!("mam-link:{id}"), Some(json!({"linkId": id, "status": "active"})), Some(json!({"linkId": id, "status": "stopped", "closed": out["closed"].as_array().map(|a| a.len())})), &n).await;
    let l = st.social.link(id).unwrap_or(l);
    Ok(Json(json!({"link": st.social.link_json(&l, "admin").await, "result": out, "audit": [a]})))
}
