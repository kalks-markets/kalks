//! Webhook signals (D77, D85): per-user secret URLs that accept TradingView-style JSON alerts and fan them
//! out to several of the user's own accounts with per-account sizing.
//!
//! Alert body (JSON, max 16 KB):
//! `{"passphrase":"…", "action":"buy|sell|close|close_buy|close_sell", "symbol":"EURUSD" (or "OANDA:EUR/USD"),
//!   "volume":0.1, "sl":1.08, "tp":1.1, "sl_pips":20, "tp_pips":40, "sl_points":…, "tp_points":…,
//!   "comment":"…", "id":"unique alert id", "timestamp":"{{timenow}}"}`
//!
//! Replay protection: an `id` (or `nonce`) is accepted once per webhook; with a `timestamp` the alert must be
//! within 5 minutes of the server clock and the same body is accepted once; with neither, an identical body is
//! accepted once per minute. Rate limits: per URL (tenant setting, default 30/min) and per source IP (120/min).

use std::net::SocketAddr;
use std::time::Duration;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use reqwest::Method;
use serde_json::{Value, json};
use sqlx::Row;

use super::{Body, Res, f, i, s};
use crate::error::ApiError;
use crate::security::{ct_eq, random_token, sha256_hex};
use crate::state::{AppState, User, audit, halted, settings};

const MAX_ROUTES: usize = 10;

pub fn client_ip(headers: &HeaderMap, peer: Option<SocketAddr>) -> String {
    let loopback = peer.is_none_or(|p| p.ip().is_loopback());
    if loopback && let Some(x) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first) = x.split(',').next().map(str::trim).filter(|s| !s.is_empty()) {
            return first.to_string();
        }
    }
    peer.map(|p| p.ip().to_string()).unwrap_or_else(|| "unknown".into())
}

fn url(st: &AppState, token: &str) -> String {
    format!("{}/hooks/{token}", st.cfg.public_url)
}

async fn own(st: &AppState, u: &User, id: i64) -> Result<sqlx::postgres::PgRow, ApiError> {
    sqlx::query("SELECT * FROM webhooks WHERE id = $1 AND tenant_id = $2 AND user_id = $3").bind(id).bind(&u.tenant).bind(u.id).fetch_optional(&st.pool).await?.ok_or_else(|| ApiError::not_found("Webhook"))
}

fn hook_view(r: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": r.get::<i64, _>("id"), "name": r.get::<String, _>("name"), "status": r.get::<String, _>("status"),
        "tokenHint": r.get::<String, _>("token_hint"), "passphrase": r.get::<Option<String>, _>("passphrase_hash").is_some(),
        "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"), "lastUsedAt": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_used_at"),
    })
}

async fn routes_of(st: &AppState, id: i64) -> Result<Vec<Value>, ApiError> {
    let rows = sqlx::query("SELECT id, login, account_type, sizing, symbol_map, enabled FROM webhook_routes WHERE webhook_id = $1 ORDER BY id").bind(id).fetch_all(&st.pool).await?;
    Ok(rows
        .iter()
        .map(|r| json!({"id": r.get::<i64, _>("id"), "login": r.get::<i64, _>("login"), "accountType": r.get::<String, _>("account_type"), "sizing": r.get::<Value, _>("sizing"), "symbolMap": r.get::<Value, _>("symbol_map"), "enabled": r.get::<bool, _>("enabled")}))
        .collect())
}

pub async fn list(State(st): State<AppState>, u: User) -> Res {
    let rows = sqlx::query(
        "SELECT w.*, (SELECT count(*) FROM webhook_routes r WHERE r.webhook_id = w.id) AS routes,
                (SELECT count(*) FROM webhook_events e WHERE e.webhook_id = w.id AND e.received_at > now() - interval '1 day') AS events_24h
         FROM webhooks w WHERE w.tenant_id = $1 AND w.user_id = $2 ORDER BY w.id DESC",
    )
    .bind(&u.tenant)
    .bind(u.id)
    .fetch_all(&st.pool)
    .await?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            let mut v = hook_view(r);
            v["routes"] = json!(r.get::<i64, _>("routes"));
            v["events24h"] = json!(r.get::<i64, _>("events_24h"));
            v
        })
        .collect();
    let events = sqlx::query("SELECT e.id, e.webhook_id, e.received_at, e.ip, e.payload, e.status, e.error, e.results FROM webhook_events e WHERE e.tenant_id = $1 AND e.user_id = $2 ORDER BY e.id DESC LIMIT 50")
        .bind(&u.tenant)
        .bind(u.id)
        .fetch_all(&st.pool)
        .await?;
    Ok(Json(json!({"items": items, "events": events.iter().map(event_view).collect::<Vec<_>>(), "baseUrl": format!("{}/hooks/", st.cfg.public_url)})))
}

fn event_view(r: &sqlx::postgres::PgRow) -> Value {
    json!({"id": r.get::<i64, _>("id"), "webhookId": r.get::<i64, _>("webhook_id"), "receivedAt": r.get::<chrono::DateTime<chrono::Utc>, _>("received_at"), "ip": r.get::<Option<String>, _>("ip"),
           "payload": r.get::<Option<Value>, _>("payload"), "status": r.get::<String, _>("status"), "error": r.get::<Option<String>, _>("error"), "results": r.get::<Value, _>("results")})
}

async fn check_routes(st: &AppState, u: &User, v: &Value) -> Result<Vec<(i64, String, Value, Value, bool)>, ApiError> {
    let list = v.get("routes").and_then(Value::as_array).cloned().unwrap_or_default();
    if list.len() > MAX_ROUTES {
        return Err(ApiError::validation("routes", format!("At most {MAX_ROUTES} accounts per webhook.")));
    }
    let accounts = st.engine.accounts(&u.tenant, u.id).await.map_err(|_| ApiError::unavailable("Trading service is unavailable."))?;
    let mut out: Vec<(i64, String, Value, Value, bool)> = vec![];
    for r in list {
        let login = i(&r, "login").ok_or_else(|| ApiError::validation("routes", "Each route needs an account."))?;
        let a = accounts.iter().find(|a| a.get("login").and_then(Value::as_i64) == Some(login)).ok_or_else(|| ApiError::validation("routes", format!("Account {login} is not one of yours.")))?;
        if out.iter().any(|(l, ..)| *l == login) {
            return Err(ApiError::validation("routes", format!("Account {login} is listed twice.")));
        }
        let sz = r.get("sizing").cloned().unwrap_or(json!({}));
        let mode = s(&sz, "mode").unwrap_or("fixed");
        if !["fixed", "alert", "multiplier", "risk"].contains(&mode) {
            return Err(ApiError::validation("routes", "Sizing mode must be fixed, alert, multiplier or risk."));
        }
        let value = f(&sz, "value").unwrap_or(if mode == "alert" { 0.0 } else { 0.01 });
        if mode != "alert" && !(value > 0.0) {
            return Err(ApiError::validation("routes", "Sizing value must be above 0."));
        }
        if mode == "risk" && value > 5.0 {
            return Err(ApiError::validation("routes", "Risk above 5% per trade is not allowed."));
        }
        let mut sizing = json!({"mode": mode, "value": value});
        if let Some(m) = f(&sz, "maxLots").filter(|m| *m > 0.0) {
            sizing["maxLots"] = json!(m);
        }
        let map = r.get("symbolMap").filter(|m| m.is_object()).cloned().unwrap_or(json!({}));
        out.push((login, a.get("type").and_then(Value::as_str).unwrap_or("demo").to_string(), sizing, map, r.get("enabled").and_then(Value::as_bool).unwrap_or(true)));
    }
    Ok(out)
}

async fn save_routes(st: &AppState, tenant: &str, id: i64, routes: &[(i64, String, Value, Value, bool)]) -> Result<(), ApiError> {
    let mut tx = st.pool.begin().await?;
    sqlx::query("DELETE FROM webhook_routes WHERE webhook_id = $1").bind(id).execute(&mut *tx).await?;
    for (login, kind, sizing, map, enabled) in routes {
        sqlx::query("INSERT INTO webhook_routes (tenant_id, webhook_id, login, account_type, sizing, symbol_map, enabled) VALUES ($1,$2,$3,$4,$5,$6,$7)")
            .bind(tenant)
            .bind(id)
            .bind(login)
            .bind(kind)
            .bind(sizing)
            .bind(map)
            .bind(enabled)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn create(State(st): State<AppState>, u: User, Body(v): Body<Value>) -> Res {
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM webhooks WHERE tenant_id = $1 AND user_id = $2").bind(&u.tenant).bind(u.id).fetch_one(&st.pool).await?;
    if n >= 20 {
        return Err(ApiError::conflict("limit", "You can have up to 20 webhooks."));
    }
    let name = s(&v, "name").unwrap_or("TradingView alerts").chars().take(60).collect::<String>();
    let routes = check_routes(&st, &u, &v).await?;
    let token = format!("wh_{}", random_token(24));
    let pass = s(&v, "passphrase").map(|p| sha256_hex(p.as_bytes()));
    let id: i64 = sqlx::query_scalar("INSERT INTO webhooks (tenant_id, user_id, name, token_hash, token_hint, passphrase_hash) VALUES ($1,$2,$3,$4,$5,$6) RETURNING id")
        .bind(&u.tenant)
        .bind(u.id)
        .bind(&name)
        .bind(sha256_hex(token.as_bytes()))
        .bind(&token[token.len() - 4..])
        .bind(pass)
        .fetch_one(&st.pool)
        .await?;
    save_routes(&st, &u.tenant, id, &routes).await?;
    audit(&st.pool, &u.tenant, &format!("user:{}", u.id), "webhook.create", &format!("webhook:{id}"), json!({"routes": routes.len()})).await;
    Ok(Json(json!({"id": id, "name": name, "url": url(&st, &token), "token": token, "note": "The URL is shown once. Keep it secret: anyone with it can send alerts."})))
}

pub async fn detail(State(st): State<AppState>, u: User, Path(id): Path<i64>) -> Res {
    let r = own(&st, &u, id).await?;
    let mut v = hook_view(&r);
    v["routes"] = json!(routes_of(&st, id).await?);
    let ev = sqlx::query("SELECT * FROM webhook_events WHERE webhook_id = $1 ORDER BY id DESC LIMIT 100").bind(id).fetch_all(&st.pool).await?;
    v["events"] = json!(ev.iter().map(event_view).collect::<Vec<_>>());
    Ok(Json(v))
}

pub async fn update(State(st): State<AppState>, u: User, Path(id): Path<i64>, Body(v): Body<Value>) -> Res {
    own(&st, &u, id).await?;
    if let Some(n) = s(&v, "name") {
        sqlx::query("UPDATE webhooks SET name = $2 WHERE id = $1").bind(id).bind(n.chars().take(60).collect::<String>()).execute(&st.pool).await?;
    }
    if let Some(stt) = s(&v, "status") {
        if !["active", "disabled"].contains(&stt) {
            return Err(ApiError::validation("status", "status must be active or disabled"));
        }
        sqlx::query("UPDATE webhooks SET status = $2 WHERE id = $1").bind(id).bind(stt).execute(&st.pool).await?;
    }
    match v.get("passphrase") {
        Some(Value::String(p)) if !p.trim().is_empty() => {
            sqlx::query("UPDATE webhooks SET passphrase_hash = $2 WHERE id = $1").bind(id).bind(sha256_hex(p.trim().as_bytes())).execute(&st.pool).await?;
        }
        Some(Value::Null) | Some(Value::String(_)) => {
            sqlx::query("UPDATE webhooks SET passphrase_hash = NULL WHERE id = $1").bind(id).execute(&st.pool).await?;
        }
        _ => {}
    }
    Ok(Json(json!({"status": "ok"})))
}

pub async fn remove(State(st): State<AppState>, u: User, Path(id): Path<i64>) -> Res {
    own(&st, &u, id).await?;
    sqlx::query("DELETE FROM webhooks WHERE id = $1").bind(id).execute(&st.pool).await?;
    audit(&st.pool, &u.tenant, &format!("user:{}", u.id), "webhook.delete", &format!("webhook:{id}"), json!({})).await;
    Ok(Json(json!({"status": "deleted"})))
}

pub async fn rotate(State(st): State<AppState>, u: User, Path(id): Path<i64>) -> Res {
    own(&st, &u, id).await?;
    let token = format!("wh_{}", random_token(24));
    sqlx::query("UPDATE webhooks SET token_hash = $2, token_hint = $3 WHERE id = $1").bind(id).bind(sha256_hex(token.as_bytes())).bind(&token[token.len() - 4..]).execute(&st.pool).await?;
    audit(&st.pool, &u.tenant, &format!("user:{}", u.id), "webhook.rotate", &format!("webhook:{id}"), json!({})).await;
    Ok(Json(json!({"id": id, "url": url(&st, &token), "token": token})))
}

pub async fn set_routes(State(st): State<AppState>, u: User, Path(id): Path<i64>, Body(v): Body<Value>) -> Res {
    own(&st, &u, id).await?;
    let routes = check_routes(&st, &u, &v).await?;
    save_routes(&st, &u.tenant, id, &routes).await?;
    Ok(Json(json!({"routes": routes_of(&st, id).await?})))
}

/// Sends a test alert through the full pipeline (orders are real on the routed accounts).
pub async fn test(State(st): State<AppState>, u: User, Path(id): Path<i64>, Body(v): Body<Value>) -> Res {
    let r = own(&st, &u, id).await?;
    let payload = v.get("payload").cloned().ok_or_else(|| ApiError::validation("payload", "Send the alert JSON as payload."))?;
    let mut payload = payload;
    if payload.get("id").is_none() {
        payload["id"] = json!(format!("test-{}", random_token(6)));
    }
    let raw = serde_json::to_vec(&payload)?;
    let hook = Hook { id, tenant: u.tenant.clone(), user_id: u.id, passphrase: None, status: r.get("status") };
    let (status, body) = process(&st, &hook, &raw, "test (Client Area)", true).await;
    if status.is_success() { Ok(Json(body)) } else { Err(ApiError::coded(status, "webhook", body.pointer("/error/message").and_then(Value::as_str).unwrap_or("failed").to_string()).with_details(body)) }
}

/* ------------------------------------------------------------------ */
/* Receiving alerts                                                    */
/* ------------------------------------------------------------------ */

struct Hook {
    id: i64,
    tenant: String,
    user_id: i64,
    passphrase: Option<String>,
    status: String,
}

pub async fn receive(State(st): State<AppState>, peer: super::Peer, headers: HeaderMap, Path(token): Path<String>, body: Bytes) -> (StatusCode, Json<Value>) {
    let ip = client_ip(&headers, peer.0);
    if let Err(s) = st.limiter.hit(&format!("hookip:{ip}"), 120, Duration::from_secs(60)) {
        return (StatusCode::TOO_MANY_REQUESTS, Json(json!({"error": {"code": "rate_limited", "message": "Too many alerts from this address.", "retryAfter": s}})));
    }
    let not_found = (StatusCode::NOT_FOUND, Json(json!({"error": {"code": "not_found", "message": "Unknown webhook."}})));
    if token.len() > 100 || !token.starts_with("wh_") {
        return not_found;
    }
    let row = sqlx::query("SELECT id, tenant_id, user_id, passphrase_hash, status FROM webhooks WHERE token_hash = $1").bind(sha256_hex(token.as_bytes())).fetch_optional(&st.pool).await;
    let Ok(Some(r)) = row else { return not_found };
    let hook = Hook { id: r.get("id"), tenant: r.get("tenant_id"), user_id: r.get("user_id"), passphrase: r.get("passphrase_hash"), status: r.get("status") };
    // the broker switched the public API and webhooks off (module switches, gateway)
    if let Err(e) = crate::modules::require(&st, &hook.tenant, "api").await {
        return (e.status(), Json(json!({"error": {"code": e.code(), "message": e.message()}})));
    }
    let (status, v) = process(&st, &hook, &body, &ip, false).await;
    (status, Json(v))
}

fn err(status: StatusCode, code: &str, msg: &str) -> (StatusCode, Value) {
    (status, json!({"error": {"code": code, "message": msg}}))
}

pub fn normalize_symbol(raw: &str) -> String {
    let s = raw.rsplit(':').next().unwrap_or(raw).to_uppercase();
    let s = s.trim_end_matches(".P").trim_end_matches("PERP").to_string();
    s.chars().filter(|c| c.is_ascii_alphanumeric()).collect()
}

fn parse_ts(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_f64().map(|x| if x > 1e11 { (x / 1000.0) as i64 } else { x as i64 }),
        Value::String(t) => chrono::DateTime::parse_from_rfc3339(t.trim()).ok().map(|d| d.timestamp()).or_else(|| t.trim().parse::<f64>().ok().map(|x| if x > 1e11 { (x / 1000.0) as i64 } else { x as i64 })),
        _ => None,
    }
}

async fn process(st: &AppState, hook: &Hook, raw: &[u8], ip: &str, is_test: bool) -> (StatusCode, Value) {
    if raw.len() > 16 * 1024 {
        return err(StatusCode::PAYLOAD_TOO_LARGE, "too_large", "Alert body is larger than 16 KB.");
    }
    let per_min = settings(&st.pool, &hook.tenant).await.get("webhookRatePerMin").and_then(Value::as_u64).unwrap_or(30) as usize;
    if let Err(s) = st.limiter.hit(&format!("hook:{}", hook.id), per_min.max(1), Duration::from_secs(60)) {
        return (StatusCode::TOO_MANY_REQUESTS, json!({"error": {"code": "rate_limited", "message": "Too many alerts for this webhook.", "retryAfter": s}}));
    }
    let Ok(mut body) = serde_json::from_slice::<Value>(raw) else { return err(StatusCode::BAD_REQUEST, "bad_request", "The alert body must be JSON.") };
    if !body.is_object() {
        return err(StatusCode::BAD_REQUEST, "bad_request", "The alert body must be a JSON object.");
    }
    if hook.status != "active" {
        return err(StatusCode::FORBIDDEN, "disabled", "This webhook is disabled.");
    }
    if let Some(h) = &hook.passphrase {
        let given = body.get("passphrase").and_then(Value::as_str).map(|p| sha256_hex(p.as_bytes())).unwrap_or_default();
        if !ct_eq(&given, h) {
            return err(StatusCode::UNAUTHORIZED, "unauthorized", "Wrong or missing passphrase.");
        }
    }
    if let Some(o) = body.as_object_mut() {
        o.remove("passphrase");
    }
    let now = chrono::Utc::now().timestamp();
    let id = body.get("id").or_else(|| body.get("nonce")).and_then(|v| v.as_str().map(str::to_string).or_else(|| v.as_i64().map(|x| x.to_string())));
    let ts = body.get("timestamp").or_else(|| body.get("time")).and_then(parse_ts);
    if let Some(t) = ts
        && (now - t).abs() > 300
    {
        return err(StatusCode::UNPROCESSABLE_ENTITY, "stale", "The alert timestamp is more than 5 minutes away from the server clock.");
    }
    let dedup = match (&id, ts) {
        (Some(id), _) => format!("id:{}", id.chars().take(100).collect::<String>()),
        (None, Some(_)) => format!("body:{}", sha256_hex(raw)),
        (None, None) => format!("body:{}:{}", sha256_hex(raw), now / 60),
    };
    let ev: Result<Option<i64>, _> = sqlx::query_scalar("INSERT INTO webhook_events (tenant_id, webhook_id, user_id, ip, dedup_key, payload, status) VALUES ($1,$2,$3,$4,$5,$6,'received') ON CONFLICT (webhook_id, dedup_key) DO NOTHING RETURNING id")
        .bind(&hook.tenant)
        .bind(hook.id)
        .bind(hook.user_id)
        .bind(ip)
        .bind(&dedup)
        .bind(&body)
        .fetch_optional(&st.pool)
        .await;
    let ev = match ev {
        Ok(Some(e)) => e,
        Ok(None) => return err(StatusCode::CONFLICT, "duplicate", "This alert was already received (replay protection)."),
        Err(e) => {
            tracing::error!(error = %e, "webhook event insert");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "Could not record the alert.");
        }
    };
    let _ = sqlx::query("UPDATE webhooks SET last_used_at = now() WHERE id = $1").bind(hook.id).execute(&st.pool).await;
    let finish = |status: &'static str, error: Option<String>, results: Value| async move {
        let _ = sqlx::query("UPDATE webhook_events SET status = $2, error = $3, results = $4 WHERE id = $1").bind(ev).bind(status).bind(error).bind(results).execute(&st.pool).await;
    };
    let (halt, why) = halted(&st.pool, &hook.tenant, hook.user_id).await;
    if halt {
        finish("blocked", Some(why.to_string()), json!([])).await;
        return err(StatusCode::CONFLICT, "halted", &format!("Trading is halted: {why}."));
    }
    // action + symbol
    let action_raw = s(&body, "action").or_else(|| s(&body, "side")).or_else(|| s(&body, "signal")).unwrap_or("").to_lowercase();
    let flat = s(&body, "market_position").is_some_and(|m| m.eq_ignore_ascii_case("flat"));
    let action = match action_raw.as_str() {
        _ if flat => "close",
        "buy" | "long" => "buy",
        "sell" | "short" => "sell",
        "close" | "exit" | "flat" | "close_all" => "close",
        "close_buy" | "close_long" | "exit_long" => "close_buy",
        "close_sell" | "close_short" | "exit_short" => "close_sell",
        _ => {
            finish("rejected", Some("unknown action".into()), json!([])).await;
            return err(StatusCode::UNPROCESSABLE_ENTITY, "validation", "action must be buy, sell, close, close_buy or close_sell.");
        }
    };
    let Some(sym_raw) = s(&body, "symbol").or_else(|| s(&body, "ticker")).map(str::to_string) else {
        finish("rejected", Some("symbol missing".into()), json!([])).await;
        return err(StatusCode::UNPROCESSABLE_ENTITY, "validation", "symbol is required.");
    };
    let routes = sqlx::query("SELECT id, login, sizing, symbol_map FROM webhook_routes WHERE webhook_id = $1 AND enabled ORDER BY id").bind(hook.id).fetch_all(&st.pool).await.unwrap_or_default();
    if routes.is_empty() {
        finish("rejected", Some("no accounts routed".into()), json!([])).await;
        return err(StatusCode::UNPROCESSABLE_ENTITY, "no_routes", "This webhook has no enabled accounts. Add one in the Client Area.");
    }
    let alert_vol = f(&body, "volume").or_else(|| f(&body, "qty")).or_else(|| f(&body, "lots")).or_else(|| f(&body, "contracts"));
    let mut results = vec![];
    for r in routes {
        let rid: i64 = r.get("id");
        let login: i64 = r.get("login");
        let map: Value = r.get("symbol_map");
        let symbol = map.get(&sym_raw).and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| normalize_symbol(&sym_raw));
        let res = route_one(st, hook, ev, rid, login, &symbol, action, &r.get::<Value, _>("sizing"), alert_vol, &body).await;
        results.push(match res {
            Ok(mut v) => {
                v["login"] = json!(login);
                v["symbol"] = json!(symbol);
                v
            }
            Err(e) => json!({"login": login, "symbol": symbol, "status": "rejected", "error": e}),
        });
    }
    let ok = results.iter().filter(|r| r.get("status").and_then(Value::as_str) != Some("rejected")).count();
    let status = if ok == results.len() { "accepted" } else if ok > 0 { "partial" } else { "failed" };
    finish(status, None, json!(results)).await;
    let code = if ok > 0 { StatusCode::OK } else { StatusCode::UNPROCESSABLE_ENTITY };
    let _ = is_test;
    (code, json!({"status": status, "eventId": ev, "action": action, "results": results}))
}

#[allow(clippy::too_many_arguments)]
async fn route_one(st: &AppState, hook: &Hook, ev: i64, rid: i64, login: i64, symbol: &str, action: &str, sizing: &Value, alert_vol: Option<f64>, body: &Value) -> Result<Value, String> {
    let sp = st.specs.get(symbol).cloned().ok_or_else(|| format!("unknown symbol {symbol}"))?;
    let tag = format!("WH{}", hook.id);
    if action.starts_with("close") {
        let stt = st.engine.terminal(&hook.tenant, hook.user_id, login, Method::GET, "/v1/terminal/state?historyLimit=1", None).await.map_err(|e| e.to_string())?;
        if !stt.ok() {
            return Err(stt.message());
        }
        let want = match action {
            "close_buy" => Some("buy"),
            "close_sell" => Some("sell"),
            _ => None,
        };
        let mut closed = vec![];
        let mut errors = vec![];
        for p in stt.body.get("positions").and_then(Value::as_array).cloned().unwrap_or_default() {
            let mine = p.get("source").and_then(Value::as_str) == Some("webhook") && p.get("comment").and_then(Value::as_str).is_some_and(|c| c.starts_with(&tag));
            if !mine || p.get("symbol").and_then(Value::as_str) != Some(symbol) || want.is_some_and(|w| p.get("side").and_then(Value::as_str) != Some(w)) {
                continue;
            }
            let t = p.get("ticket").and_then(Value::as_i64).unwrap_or(0);
            match st.engine.terminal(&hook.tenant, hook.user_id, login, Method::POST, &format!("/v1/terminal/positions/{t}/close"), Some(&json!({}))).await {
                Ok(r) if r.ok() => closed.push(json!({"ticket": t, "profit": r.body.get("profit")})),
                Ok(r) => errors.push(format!("#{t}: {}", r.message())),
                Err(e) => errors.push(format!("#{t}: {e}")),
            }
        }
        if closed.is_empty() && !errors.is_empty() {
            return Err(errors.join("; "));
        }
        return Ok(json!({"status": if closed.is_empty() { "nothing_to_close" } else { "closed" }, "closed": closed, "errors": errors}));
    }
    // entry: price for SL/TP distances and risk sizing
    let acct = st.engine.account(&hook.tenant, hook.user_id, login).await.map_err(|e| e.to_string())?.ok_or("account not found")?;
    let a = acct.get("account").cloned().unwrap_or(json!({}));
    let groups = st.engine.groups(&hook.tenant).await.unwrap_or_default();
    let group = a.get("group").and_then(Value::as_str).unwrap_or("standard");
    let sg = groups.iter().find(|g| g.get("code").and_then(Value::as_str) == Some(group)).and_then(|g| g.get("spreadGroup").and_then(Value::as_str)).unwrap_or("standard").to_string();
    let q = st.md.quotes(&[symbol.to_string()], &sg).await.ok().and_then(|m| m.get(symbol).copied()).ok_or("no quote")?;
    let buy = action == "buy";
    let dir = if buy { 1.0 } else { -1.0 };
    let entry = if buy { q.ask } else { q.bid };
    let level = |abs: &str, pips: &str, pts: &str, sign: f64| -> Option<f64> {
        f(body, abs).or_else(|| f(body, pips).map(|p| entry + sign * dir * p * sp.pip_size)).or_else(|| f(body, pts).map(|p| entry + sign * dir * p * sp.point)).map(|x| sp.round_price(x))
    };
    let sl = level("sl", "sl_pips", "sl_points", -1.0);
    let tp = level("tp", "tp_pips", "tp_points", 1.0);
    let mode = sizing.get("mode").and_then(Value::as_str).unwrap_or("fixed");
    let value = sizing.get("value").and_then(Value::as_f64).unwrap_or(0.01);
    let mut vol = match mode {
        "alert" => alert_vol.ok_or("the alert has no volume (sizing = alert volume)")?,
        "multiplier" => alert_vol.ok_or("the alert has no volume to multiply")? * value,
        "risk" => {
            let s = sl.ok_or("risk sizing needs a stop loss in the alert")?;
            let bal = a.get("balance").and_then(Value::as_f64).unwrap_or(0.0);
            let conv = if sp.quote_ccy == "USD" { 1.0 } else if sp.symbol.starts_with("USD") { 1.0 / entry } else { 1.0 };
            bal * value / 100.0 / ((entry - s).abs() * sp.contract_size * conv)
        }
        _ => value,
    };
    if let Some(m) = sizing.get("maxLots").and_then(Value::as_f64) {
        vol = vol.min(m);
    }
    let vol = sp.floor_volume(vol).ok_or_else(|| format!("volume below the minimum {} lot", sp.lot_min))?;
    let note: String = s(body, "comment").unwrap_or("").chars().take(40).collect();
    let mut order = json!({"symbol": symbol, "side": if buy { "buy" } else { "sell" }, "type": "market", "volume": vol, "source": "webhook", "platform": "API",
                           "comment": format!("{tag} {note}").trim().to_string(), "clientOrderId": format!("wh-{ev}-{rid}")});
    if let Some(v) = sl {
        order["sl"] = json!(v);
    }
    if let Some(v) = tp {
        order["tp"] = json!(v);
    }
    let r = st.engine.terminal(&hook.tenant, hook.user_id, login, Method::POST, "/v1/terminal/orders", Some(&order)).await.map_err(|e| e.to_string())?;
    if !r.ok() {
        return Err(format!("{} ({})", r.message(), r.code()));
    }
    Ok(json!({"status": r.body.get("status").cloned().unwrap_or(json!("filled")), "ticket": r.body.get("positionTicket").or_else(|| r.body.get("ticket")), "price": r.body.get("price"), "volume": vol, "sl": sl, "tp": tp}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbols_and_timestamps() {
        assert_eq!(normalize_symbol("OANDA:EUR/USD"), "EURUSD");
        assert_eq!(normalize_symbol("BINANCE:BTCUSDT.P"), "BTCUSDT");
        assert_eq!(normalize_symbol("xauusd"), "XAUUSD");
        assert_eq!(parse_ts(&json!(1_790_000_000_000i64)), Some(1_790_000_000));
        assert_eq!(parse_ts(&json!("2026-09-28T10:00:00Z")), Some(1_790_589_600));
        let h = HeaderMap::new();
        assert_eq!(client_ip(&h, Some("10.1.2.3:5".parse().unwrap())), "10.1.2.3");
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "203.0.113.9, 10.0.0.1".parse().unwrap());
        assert_eq!(client_ip(&h, Some("127.0.0.1:5".parse().unwrap())), "203.0.113.9");
        // a remote peer cannot spoof its address with the header
        assert_eq!(client_ip(&h, Some("198.51.100.7:5".parse().unwrap())), "198.51.100.7");
    }
}
