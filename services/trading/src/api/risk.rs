//! Back Office risk desk routes (docs/RISK-DESK.md): exposure, risk limits and settings, alerts, client scores,
//! routing suggestions, news windows. Same staff headers, reason codes and `{data, audit}` replies as the dealing API.

use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;

use super::dealing::{self, Reason, check_reason, reply, row};
use super::{ApiError, ApiResult, AppState, Body, ROLES_CONFIG, ROLES_DEALING, StaffCtx};
use crate::engine::dealing::{BookMove, transfer_book};
use crate::model::{AccountKind, Book};
use crate::persist;
use crate::risk::{self, RiskLimits, RoutingMode};
use crate::rules::{RoutingAction, RoutingCondition, RoutingRule};
use crate::shard::{ExecError, Op};

/* ---------------- exposure ---------------- */

#[derive(Deserialize, Default)]
pub struct KindQ {
    kind: Option<String>,
}

/// `GET /v1/dealing/exposure?kind=live|demo`: the last pass of the exposure loop (live, ≤ 7 s old) or computed now.
pub async fn exposure(State(st): State<AppState>, s: StaffCtx, Query(q): Query<KindQ>) -> ApiResult<Json<Value>> {
    let kind = match q.kind.as_deref() {
        None | Some("") | Some("live") => AccountKind::Live,
        Some("demo") => AccountKind::Demo,
        _ => return Err(ApiError::Validation { field: "kind", message: "kind must be live or demo".into() }),
    };
    let t = s.ctx.tenant.tenant_id;
    if kind == AccountKind::Live
        && let Some(v) = st.hub.shared.risk.views.read().unwrap().get(&t).cloned()
        && v["generatedAt"].as_str().and_then(|g| chrono::DateTime::parse_from_rfc3339(g).ok()).is_some_and(|g| Utc::now() - g.with_timezone(&Utc) < chrono::Duration::seconds(7))
    {
        return Ok(Json((*v).clone()));
    }
    Ok(Json(risk::exposure::compute(&st, t, kind).await?))
}

/* ---------------- limits and settings ---------------- */

pub async fn limits(s: StaffCtx) -> ApiResult<Json<Value>> {
    Ok(Json(s.ctx.tenant.risk.json()))
}

#[derive(Deserialize)]
pub struct LimitsBody {
    limits: RiskLimits,
    #[serde(flatten)]
    reason: Reason,
}

pub async fn save_limits(State(st): State<AppState>, s: StaffCtx, Body(b): Body<LimitsBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_DEALING)?;
    check_reason(&b.reason)?;
    let mut limits = b.limits;
    for l in &mut limits.exposure {
        l.symbol = l.symbol.trim().to_uppercase();
    }
    let t = s.ctx.tenant.clone();
    limits.validate(&t.groups, &st.hub.shared.specs.load()).map_err(|(field, message)| ApiError::Validation { field, message })?;
    let mut flags = vec![];
    if !limits.alert_only() {
        flags.push("enforce".to_string());
    }
    let a = row(&s, &b.reason, "risk.limits", vec![], None, None, Some(json!(t.risk.limits)), Some(json!(limits)), flags);
    let mut tx = st.pool.begin().await?;
    risk::limits::store(&mut tx, t.tenant_id, &limits, &s.staff.name).await?;
    let id = persist::insert_audit(&mut *tx, &a).await?;
    tx.commit().await?;
    let staff = s.staff.name.clone();
    let l2 = limits.clone();
    st.hub.shared.registry.update(t.tenant_id, |cfg| {
        cfg.risk.limits = l2;
        cfg.risk.updated_at = Some(Utc::now());
        cfg.risk.updated_by = Some(staff);
    });
    let entry = persist::audit_json(id, &a);
    push_audit(&st, t.tenant_id, &entry);
    Ok(reply(json!({"limits": limits}), vec![entry]))
}

fn push_audit(st: &AppState, tenant: i64, entry: &Value) {
    if let Some(tx) = st.hub.shared.streams.dealing(tenant) {
        let _ = tx.send(std::sync::Arc::from(json!({"type": "audit", "entry": entry}).to_string()));
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsBody {
    routing_mode: Option<RoutingMode>,
    score_interval_mins: Option<u32>,
    #[serde(flatten)]
    reason: Reason,
}

/// Routing mode (recommend / execute) and the score interval: tenant policy (dealing.policy).
pub async fn save_settings(State(st): State<AppState>, s: StaffCtx, Body(b): Body<SettingsBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_CONFIG)?;
    check_reason(&b.reason)?;
    let t = s.ctx.tenant.clone();
    let mut set = t.risk.settings.clone();
    if let Some(m) = b.routing_mode {
        set.routing_mode = m;
    }
    if let Some(m) = b.score_interval_mins {
        if !(15..=10_080).contains(&m) {
            return Err(ApiError::Validation { field: "scoreIntervalMins", message: "The score interval is 15 minutes to 7 days".into() });
        }
        set.score_interval_mins = m;
    }
    let flags = if set.routing_mode == RoutingMode::Execute { vec!["routing executes".to_string()] } else { vec![] };
    let a = row(&s, &b.reason, "risk.settings", vec![], None, None, Some(json!(t.risk.settings)), Some(json!(set)), flags);
    let mut tx = st.pool.begin().await?;
    risk::limits::store_settings(&mut tx, t.tenant_id, &set, &s.staff.name).await?;
    let id = persist::insert_audit(&mut *tx, &a).await?;
    tx.commit().await?;
    let s2 = set.clone();
    st.hub.shared.registry.update(t.tenant_id, |cfg| cfg.risk.settings = s2);
    let entry = persist::audit_json(id, &a);
    push_audit(&st, t.tenant_id, &entry);
    Ok(reply(json!({"settings": set}), vec![entry]))
}

/* ---------------- alerts ---------------- */

#[derive(Deserialize, Default)]
pub struct ListQ {
    status: Option<String>,
    limit: Option<i64>,
}

pub async fn alerts(State(st): State<AppState>, s: StaffCtx, Query(q): Query<ListQ>) -> ApiResult<Json<Value>> {
    Ok(Json(json!(risk::alerts::list(&st, s.ctx.tenant.tenant_id, q.status.as_deref().unwrap_or("open"), q.limit.unwrap_or(200)).await?)))
}

#[derive(Deserialize, Default)]
pub struct AckBody {
    #[serde(default)]
    note: String,
}

pub async fn ack_alert(State(st): State<AppState>, s: StaffCtx, Path(id): Path<String>, Body(b): Body<AckBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_DEALING)?;
    let id: i64 = id.trim().parse().map_err(|_| ApiError::BadRequest(format!("Invalid alert id {id}")))?;
    let Some((before, after)) = risk::alerts::ack(&st, s.ctx.tenant.tenant_id, id, &s.staff.name).await? else { return Err(ApiError::NotFound(format!("Alert {id} not found"))) };
    let r = Reason { reason_code: "RISK · Alert acknowledged".into(), note: b.note.chars().take(500).collect() };
    let a = row(&s, &r, "risk.alert_ack", vec![], after["login"].as_str().and_then(|l| l.parse().ok()), after["symbol"].as_str().map(str::to_string), Some(json!({"alert": id, "level": before["level"], "status": before["status"]})), Some(json!({"alert": id, "title": after["title"], "status": after["status"]})), vec![]);
    let entry = dealing::audit_now(&st, a).await?;
    Ok(reply(after, vec![entry]))
}

/* ---------------- client scores ---------------- */

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ScoresQ {
    category: Option<String>,
    q: Option<String>,
    #[serde(alias = "user_id")]
    user_id: Option<i64>,
    limit: Option<i64>,
}

pub async fn scores(State(st): State<AppState>, s: StaffCtx, Query(q): Query<ScoresQ>) -> ApiResult<Json<Value>> {
    let t = s.ctx.tenant.tenant_id;
    let rows = sqlx::query(
        "SELECT * FROM (SELECT DISTINCT ON (login) login, user_id, category, score, data, computed_at FROM client_scores WHERE tenant_id = $1 ORDER BY login, computed_at DESC, id DESC) x
          WHERE ($2::text IS NULL OR category = $2) AND ($3::bigint IS NULL OR user_id = $3) AND ($4::text IS NULL OR login::text LIKE $4 || '%' OR user_id::text = $4)
          ORDER BY score DESC, login LIMIT $5",
    )
    .bind(t)
    .bind(q.category.as_deref().filter(|c| !c.is_empty() && *c != "all"))
    .bind(q.user_id)
    .bind(q.q.as_deref().map(str::trim).filter(|x| !x.is_empty()))
    .bind(q.limit.unwrap_or(500).clamp(1, 2000))
    .fetch_all(&st.pool)
    .await?;
    let at: Option<chrono::DateTime<Utc>> = sqlx::query_scalar("SELECT max(computed_at) FROM client_scores WHERE tenant_id = $1").bind(t).fetch_one(&st.pool).await?;
    let items: Vec<Value> = rows.iter().map(|r| r.get::<sqlx::types::Json<Value>, _>("data").0).collect();
    Ok(Json(json!({"items": items, "computedAt": at, "running": st.hub.shared.risk.score_running.load(std::sync::atomic::Ordering::SeqCst)})))
}

pub async fn score(State(st): State<AppState>, s: StaffCtx, Path(login): Path<String>) -> ApiResult<Json<Value>> {
    let login: i64 = login.trim().parse().map_err(|_| ApiError::BadRequest(format!("Invalid login {login}")))?;
    let rows = sqlx::query("SELECT score, category, data, computed_at FROM client_scores WHERE tenant_id = $1 AND login = $2 ORDER BY computed_at DESC, id DESC LIMIT 60").bind(s.ctx.tenant.tenant_id).bind(login).fetch_all(&st.pool).await?;
    let current = rows.first().map(|r| r.get::<sqlx::types::Json<Value>, _>("data").0);
    let history: Vec<Value> = rows.iter().rev().map(|r| json!({"computedAt": r.get::<chrono::DateTime<Utc>, _>("computed_at"), "score": crate::money::num(r.get("score")), "category": r.get::<String, _>("category")})).collect();
    Ok(Json(json!({"current": current, "history": history})))
}

#[derive(Deserialize, Default)]
pub struct RunBody {
    #[serde(default)]
    login: Option<Value>,
}

pub async fn run_scores(State(st): State<AppState>, s: StaffCtx, Body(b): Body<RunBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_DEALING)?;
    let login = match &b.login {
        None | Some(Value::Null) => None,
        Some(Value::String(x)) if x.trim().is_empty() => None,
        Some(Value::String(x)) => Some(x.trim().parse::<i64>().map_err(|_| ApiError::Validation { field: "login", message: "Unknown account".into() })?),
        Some(Value::Number(n)) => n.as_i64(),
        _ => return Err(ApiError::Validation { field: "login", message: "Unknown account".into() }),
    };
    if let Some(l) = login
        && st.hub.meta(l).is_none_or(|m| m.tenant_id != s.ctx.tenant.tenant_id)
    {
        return Err(ApiError::NotFound(format!("Unknown account {l}")));
    }
    st.hub.shared.risk.request_scores(s.ctx.tenant.tenant_id, login);
    Ok(reply(json!({"queued": true, "login": login.map(|l| l.to_string())}), vec![]))
}

/* ---------------- news ---------------- */

pub async fn news(State(st): State<AppState>, s: StaffCtx) -> ApiResult<Json<Value>> {
    let specs = st.hub.shared.specs.load();
    Ok(Json(risk::news::windows_json(&st, s.ctx.tenant.risk.limits.news.as_ref(), &specs, st.hub.shared.clock.now())))
}

/* ---------------- routing suggestions ---------------- */

pub async fn suggestions(State(st): State<AppState>, s: StaffCtx, Query(q): Query<ListQ>) -> ApiResult<Json<Value>> {
    let status = q.status.as_deref().unwrap_or("pending");
    if !["pending", "approved", "rejected", "expired", "all"].contains(&status) {
        return Err(ApiError::Validation { field: "status", message: "status must be pending, approved, rejected, expired or all".into() });
    }
    Ok(Json(json!(risk::routing::list(&st, s.ctx.tenant.tenant_id, status, q.limit.unwrap_or(200)).await?)))
}

async fn pending(st: &AppState, s: &StaffCtx, id: &str) -> ApiResult<sqlx::postgres::PgRow> {
    let id: i64 = id.trim().parse().map_err(|_| ApiError::BadRequest(format!("Invalid suggestion id {id}")))?;
    let r = sqlx::query("SELECT * FROM routing_suggestions WHERE id = $1 AND tenant_id = $2").bind(id).bind(s.ctx.tenant.tenant_id).fetch_optional(&st.pool).await?.ok_or_else(|| ApiError::NotFound(format!("Suggestion {id} not found")))?;
    if r.get::<String, _>("status") != "pending" {
        return Err(ApiError::Conflict { code: "already_decided", message: format!("Suggestion {id} is already {}", r.get::<String, _>("status")) });
    }
    Ok(r)
}

/// One book transfer through the shard (audited `book.transfer` / `book.split`, flagged "routing suggestion").
async fn transfer(st: &AppState, s: &StaffCtx, r: &Reason, login: i64, ticket: i64, to: Book) -> Result<Vec<Value>, String> {
    let d = dealing::dealer(s, r, false);
    let op: Op = Box::new(move |tx, env| {
        let sym = tx.st.positions.get(&ticket).map(|p| p.symbol.clone());
        let (child, before, after, split) = transfer_book(tx, env, ticket, to, BookMove::Full, &d)?;
        let tickets = match child {
            Some(c) => vec![ticket, c],
            None => vec![ticket],
        };
        tx.audit.push(dealing::draft(if split { "book.split" } else { "book.transfer" }, tickets, sym, Some(before), Some(after), vec!["routing suggestion"]));
        Ok(Value::Null)
    });
    match st.hub.exec(login, &format!("staff:{}", s.staff.id), Some(s.staff.clone()), &r.reason_code, r.note.trim(), None, op).await {
        Ok(d) => Ok(d.audit),
        Err(ExecError::Reject(e)) => Err(e.message),
        Err(e) => Err(format!("{e:?}")),
    }
}

pub async fn approve(State(st): State<AppState>, s: StaffCtx, Path(id): Path<String>, Body(r): Body<Reason>) -> ApiResult<Json<Value>> {
    s.require(ROLES_DEALING)?;
    check_reason(&r)?;
    let sug = pending(&st, &s, &id).await?;
    let sid: i64 = sug.get("id");
    let kind: String = sug.get("kind");
    let login: i64 = sug.get("login");
    let to = Book::parse(sug.get::<String, _>("to_book").trim()).unwrap_or(Book::A);
    let from = Book::parse(sug.get::<String, _>("from_book").trim()).unwrap_or(Book::B);
    if st.hub.meta(login).is_none_or(|m| m.tenant_id != s.ctx.tenant.tenant_id) {
        return Err(ApiError::NotFound(format!("Unknown account {login}")));
    }
    let (mut done, mut failed, mut audit) = (vec![], vec![], vec![]);
    let tickets: Vec<i64> = if kind == "position" {
        sug.get::<Option<i64>, _>("ticket").into_iter().collect()
    } else {
        let v = st.hub.read(login, Box::new(move |x| json!(x.map(|(a, _)| a.positions.values().filter(|p| p.option.is_none() && p.book == from).map(|p| p.ticket).collect::<Vec<_>>()).unwrap_or_default()))).await;
        v.as_array().map(|a| a.iter().filter_map(Value::as_i64).collect()).unwrap_or_default()
    };
    for t in tickets {
        match transfer(&st, &s, &r, login, t, to).await {
            Ok(a) => {
                done.push(json!(t.to_string()));
                audit.extend(a);
            }
            Err(e) => failed.push(json!({"ticket": t.to_string(), "error": e})),
        }
    }
    if kind == "account" {
        // new trades of the account follow: a quick route ahead of the rule set (dealer quick routes always apply)
        let rid = format!("RQ-L{login}");
        let mut rules = s.ctx.tenant.routing_rules.clone();
        let prev = rules.iter().find(|x| x.id == rid).map(|x| x.action.book);
        rules.retain(|x| x.id != rid);
        rules.insert(0, RoutingRule { id: rid, name: format!("Account {login} → {}-book", to.as_str()), conditions: vec![RoutingCondition { field: "Login".into(), op: "=".into(), value: login.to_string() }], join: "AND".into(), action: RoutingAction { book: to, pct: 100.0, lp: None }, enabled: true, hits24h: 0, lots24h: 0.0 });
        let a = row(&s, &r, "routing.rule", vec![], Some(login), None, Some(json!({"scope": format!("login {login}"), "route": prev.map(|b| format!("{}-book", b.as_str())).unwrap_or_else(|| "rules".into())})), Some(json!({"scope": format!("login {login}"), "route": format!("{}-book", to.as_str())})), vec!["new trades only".into(), "routing suggestion".into()]);
        let Json(v) = dealing::store_rules(&st, &s, rules, a).await?;
        if let Some(x) = v["audit"].as_array() {
            audit.extend(x.iter().cloned());
        }
    }
    let result = json!({"done": done, "failed": failed});
    let status = if kind == "position" && done.is_empty() { "pending" } else { "approved" };
    if status == "pending" {
        // nothing moved (closed meanwhile, market closed…): the suggestion stays for another try
        let e = failed.first().and_then(|f| f["error"].as_str()).unwrap_or("nothing to move").to_string();
        return Err(ApiError::Status { status: 422, code: "transfer_failed", message: e });
    }
    let row_after = sqlx::query("UPDATE routing_suggestions SET status = 'approved', decided_at = now(), decided_by = $2, note = $3, result = $4 WHERE id = $1 RETURNING *")
        .bind(sid)
        .bind(&s.staff.name)
        .bind(r.note.trim())
        .bind(sqlx::types::Json(&result))
        .fetch_one(&st.pool)
        .await?;
    let view = risk::routing::row_json(&row_after);
    let a = row(&s, &r, "routing.suggestion", done.iter().filter_map(|t| t.as_str().map(str::to_string)).collect(), Some(login), view["symbol"].as_str().map(str::to_string), Some(json!({"suggestion": sid.to_string(), "kind": kind, "from": from.as_str(), "to": to.as_str()})), Some(json!({"status": "approved", "done": done.len(), "failed": failed.len()})), vec!["approved".into()]);
    audit.push(dealing::audit_now(&st, a).await?);
    Ok(reply(json!({"suggestion": view, "done": done, "failed": failed}), audit))
}

pub async fn reject(State(st): State<AppState>, s: StaffCtx, Path(id): Path<String>, Body(r): Body<Reason>) -> ApiResult<Json<Value>> {
    s.require(ROLES_DEALING)?;
    check_reason(&r)?;
    let sug = pending(&st, &s, &id).await?;
    let sid: i64 = sug.get("id");
    let row_after = sqlx::query("UPDATE routing_suggestions SET status = 'rejected', decided_at = now(), decided_by = $2, note = $3 WHERE id = $1 RETURNING *").bind(sid).bind(&s.staff.name).bind(r.note.trim()).fetch_one(&st.pool).await?;
    let view = risk::routing::row_json(&row_after);
    let a = row(&s, &r, "routing.suggestion", view["ticket"].as_str().map(|t| vec![t.to_string()]).unwrap_or_default(), Some(sug.get("login")), view["symbol"].as_str().map(str::to_string), Some(json!({"suggestion": sid.to_string(), "kind": view["kind"], "from": view["from"], "to": view["to"]})), Some(json!({"status": "rejected"})), vec!["rejected".into()]);
    let entry = dealing::audit_now(&st, a).await?;
    Ok(reply(json!({"suggestion": view}), vec![entry]))
}
