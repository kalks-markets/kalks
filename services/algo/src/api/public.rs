//! Public REST API (D77, D78) for API keys: account / positions / orders / history reads and order
//! placement, modification and closing on the key's trading account. Orders carry `source: "api"`.
//!
//! Authentication (either):
//! - `Authorization: Bearer <key_id>:<secret>` (or `X-API-Key` + `X-API-Secret`);
//! - HMAC: `X-Kalks-Key: <key_id>`, `X-Kalks-Timestamp: <unix ms>`, `X-Kalks-Signature: hex(HMAC-SHA256(secret,
//!   timestamp + METHOD + path_with_query + body))`; the timestamp must be within 30 s and a signature is
//!   accepted once.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use axum::Json;
use axum::body::Bytes;
use axum::extract::{OriginalUri, RawPathParams};
use axum::extract::State;
use axum::http::{HeaderMap, Method as HttpMethod, StatusCode};
use axum::response::{IntoResponse, Response};
use reqwest::Method;
use serde_json::{Value, json};
use sqlx::Row;

use super::f;
use super::keys::secret_for;
use super::webhooks::client_ip;
use crate::error::ApiError;
use crate::security::{ct_eq, hmac_hex, ip_allowed};
use crate::state::{AppState, halted, settings};

pub struct Key {
    pub id: i64,
    pub tenant: String,
    pub user_id: i64,
    pub login: i64,
    pub scopes: Vec<String>,
}

fn h(headers: &HeaderMap, k: &str) -> Option<String> {
    headers.get(k).and_then(|v| v.to_str().ok()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

async fn authenticate(st: &AppState, headers: &HeaderMap, method: &HttpMethod, path_q: &str, body: &[u8], ip: &str) -> Result<Key, ApiError> {
    st.limiter.hit(&format!("apiip:{ip}"), 600, Duration::from_secs(60)).map_err(ApiError::RateLimited)?;
    let bad = || ApiError::Unauthorized("Invalid API key or signature.".into());
    let (key_id, secret, hmac) = if let Some(k) = h(headers, "x-kalks-key") {
        (k, None, Some((h(headers, "x-kalks-timestamp").ok_or_else(bad)?, h(headers, "x-kalks-signature").ok_or_else(bad)?)))
    } else if let Some(a) = h(headers, "authorization") {
        let t = a.strip_prefix("Bearer ").or_else(|| a.strip_prefix("bearer ")).ok_or_else(bad)?;
        let (k, s) = t.split_once(':').ok_or_else(bad)?;
        (k.to_string(), Some(s.to_string()), None)
    } else if let (Some(k), Some(s)) = (h(headers, "x-api-key"), h(headers, "x-api-secret")) {
        (k, Some(s), None)
    } else {
        return Err(ApiError::Unauthorized("API key required (Authorization: Bearer <key_id>:<secret>).".into()));
    };
    if !key_id.starts_with("kk_") || key_id.len() > 40 {
        return Err(bad());
    }
    let r = sqlx::query("SELECT * FROM api_keys WHERE key_id = $1").bind(&key_id).fetch_optional(&st.pool).await?.ok_or_else(bad)?;
    let expected = secret_for(&st.cfg.key_secret, &key_id, &r.get::<String, _>("salt"));
    match (secret, hmac) {
        (Some(s), _) => {
            if !ct_eq(&s, &expected) {
                return Err(bad());
            }
        }
        (None, Some((ts, sig))) => {
            let t: i64 = ts.parse().map_err(|_| bad())?;
            let now = chrono::Utc::now().timestamp_millis();
            if (now - t).abs() > 30_000 {
                return Err(ApiError::Unauthorized("Request timestamp is outside the 30 s window.".into()));
            }
            let mut msg = format!("{ts}{}{path_q}", method.as_str()).into_bytes();
            msg.extend_from_slice(body);
            if !ct_eq(&sig.to_lowercase(), &hmac_hex(expected.as_bytes(), &msg)) {
                return Err(bad());
            }
            // a signature is accepted once (replay protection inside the timestamp window)
            st.limiter.hit(&format!("apisig:{sig}"), 1, Duration::from_secs(60)).map_err(|_| ApiError::Unauthorized("This signed request was already used.".into()))?;
        }
        _ => return Err(bad()),
    }
    if r.get::<String, _>("status") != "active" {
        return Err(ApiError::Unauthorized("This API key was revoked.".into()));
    }
    if r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("expires_at").is_some_and(|e| e < chrono::Utc::now()) {
        return Err(ApiError::Unauthorized("This API key has expired.".into()));
    }
    let ips: Vec<String> = r.get("ip_whitelist");
    if !ip_allowed(ip, &ips) {
        return Err(ApiError::Forbidden(format!("Requests from {ip} are not allowed for this key (IP whitelist).")));
    }
    let tenant: String = r.get("tenant_id");
    let id: i64 = r.get("id");
    let rate = match r.get::<Option<i32>, _>("rate_per_min") {
        Some(x) => x as usize,
        None => settings(&st.pool, &tenant).await.get("apiRatePerMin").and_then(Value::as_u64).unwrap_or(60) as usize,
    };
    st.limiter.hit(&format!("apikey:{id}"), rate.max(1), Duration::from_secs(60)).map_err(ApiError::RateLimited)?;
    Ok(Key { id, tenant, user_id: r.get("user_id"), login: r.get("login"), scopes: r.get("scopes") })
}

type Handler = fn(AppState, Key, Vec<String>, Value, String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, ApiError>> + Send>>;

/// Authenticates, checks the scope, runs the handler and records the request.
async fn run(st: AppState, peer: Option<SocketAddr>, headers: HeaderMap, method: HttpMethod, uri: OriginalUri, body: Bytes, params: Vec<String>, write: bool, handler: Handler) -> Response {
    let started = Instant::now();
    let ip = client_ip(&headers, peer);
    let path_q = uri.0.path_and_query().map(|p| p.as_str().to_string()).unwrap_or_default();
    let key = match authenticate(&st, &headers, &method, &path_q, &body, &ip).await {
        Ok(k) => k,
        Err(e) => return e.into_response(),
    };
    // the broker switched the public API off (module switches, gateway)
    if let Err(e) = crate::modules::require(&st, &key.tenant, "api").await {
        return e.into_response();
    }
    let result: Result<Value, ApiError> = async {
        let needs = if write { "trade" } else { "read" };
        if !(key.scopes.iter().any(|s| s == needs) || (!write && key.scopes.iter().any(|s| s == "trade"))) {
            return Err(ApiError::Forbidden(format!("This key does not have the {needs} scope.")));
        }
        if write {
            let (halt, why) = halted(&st.pool, &key.tenant, key.user_id).await;
            if halt {
                return Err(ApiError::conflict("halted", format!("Trading is halted: {why}.")));
            }
        }
        let v: Value = if body.is_empty() { json!({}) } else { serde_json::from_slice(&body).map_err(|e| ApiError::BadRequest(format!("Invalid JSON: {e}")))? };
        handler(st.clone(), Key { ..key_clone(&key) }, params, v, uri.0.query().unwrap_or("").to_string()).await
    }
    .await;
    let status = match &result {
        Ok(_) => StatusCode::OK,
        Err(e) => e.status(),
    };
    let (pool, kid, m, p, ipc, ms) = (st.pool.clone(), key.id, method.to_string(), uri.0.path().to_string(), ip.clone(), started.elapsed().as_millis() as i32);
    tokio::spawn(async move {
        let _ = sqlx::query("INSERT INTO api_requests (tenant_id, key_id, method, path, status, ip, ms) SELECT tenant_id, id, $2, $3, $4, $5, $6 FROM api_keys WHERE id = $1").bind(kid).bind(&m).bind(&p).bind(status.as_u16() as i32).bind(&ipc).bind(ms).execute(&pool).await;
        let _ = sqlx::query("UPDATE api_keys SET last_used_at = now(), last_ip = $2 WHERE id = $1").bind(kid).bind(&ipc).execute(&pool).await;
    });
    match result {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(e) => e.into_response(),
    }
}

fn key_clone(k: &Key) -> Key {
    Key { id: k.id, tenant: k.tenant.clone(), user_id: k.user_id, login: k.login, scopes: k.scopes.clone() }
}

async fn term(st: &AppState, k: &Key, method: Method, path: &str, body: Option<&Value>) -> Result<Value, ApiError> {
    let r = st.engine.terminal(&k.tenant, k.user_id, k.login, method, path, body).await.map_err(|_| ApiError::unavailable("Trading service is unavailable."))?;
    if r.ok() { Ok(r.body) } else { Err(r.into_error()) }
}

/// Client-safe position / order views (dealing fields removed).
fn clean(v: &Value) -> Value {
    let mut v = v.clone();
    if let Some(o) = v.as_object_mut() {
        for k in ["book", "parentTicket", "childTickets", "priceCorrected", "route", "controls", "userId", "version"] {
            o.remove(k);
        }
    }
    v
}

macro_rules! endpoint {
    ($name:ident, $write:expr, $body:expr) => {
        pub async fn $name(State(st): State<AppState>, peer: super::Peer, headers: HeaderMap, method: HttpMethod, uri: OriginalUri, params: RawPathParams, body: Bytes) -> Response {
            let params: Vec<String> = params.iter().map(|(_, v)| v.to_string()).collect();
            run(st, peer.0, headers, method, uri, body, params, $write, |st, k, p, v, q| Box::pin(async move { $body(st, k, p, v, q).await })).await
        }
    };
}

async fn h_account(st: AppState, k: Key, _: Vec<String>, _: Value, _: String) -> Result<Value, ApiError> {
    let s = term(&st, &k, Method::GET, "/v1/terminal/state?historyLimit=1", None).await?;
    Ok(json!({"account": clean(s.get("account").unwrap_or(&Value::Null)), "serverTime": s.get("serverTime")}))
}
async fn h_positions(st: AppState, k: Key, _: Vec<String>, _: Value, _: String) -> Result<Value, ApiError> {
    let s = term(&st, &k, Method::GET, "/v1/terminal/state?historyLimit=1", None).await?;
    Ok(json!({"positions": s.get("positions").and_then(Value::as_array).map(|a| a.iter().map(clean).collect::<Vec<_>>()).unwrap_or_default()}))
}
async fn h_orders(st: AppState, k: Key, _: Vec<String>, _: Value, _: String) -> Result<Value, ApiError> {
    let s = term(&st, &k, Method::GET, "/v1/terminal/state?historyLimit=1", None).await?;
    Ok(json!({"orders": s.get("orders").and_then(Value::as_array).map(|a| a.iter().map(clean).collect::<Vec<_>>()).unwrap_or_default()}))
}
async fn h_history(st: AppState, k: Key, _: Vec<String>, _: Value, q: String) -> Result<Value, ApiError> {
    let allowed: Vec<String> = q.split('&').filter(|p| ["from=", "to=", "page=", "limit="].iter().any(|k| p.starts_with(k)) && p.len() < 60 && p.chars().all(|c| c.is_ascii_alphanumeric() || "=-:.TZ+%".contains(c))).map(str::to_string).collect();
    let path = format!("/v1/accounts/{}/history{}{}", k.login, if allowed.is_empty() { "" } else { "?" }, allowed.join("&"));
    let r = st.engine.user_call(Method::GET, &path, &k.tenant, k.user_id, None).await.map_err(|_| ApiError::unavailable("Trading service is unavailable."))?;
    if !r.ok() {
        return Err(r.into_error());
    }
    let mut v = r.body;
    if let Some(d) = v.get_mut("deals").and_then(Value::as_array_mut) {
        for x in d.iter_mut() {
            *x = clean(x);
        }
    }
    Ok(v)
}
async fn h_place(st: AppState, k: Key, _: Vec<String>, v: Value, _: String) -> Result<Value, ApiError> {
    let mut o = json!({"source": "api", "platform": "API"});
    for key in ["symbol", "side", "type", "volume", "price", "stopLimit", "sl", "tp", "trailingPoints", "expiry", "expiryAt", "requestedPrice", "deviationPoints", "ocoWith", "comment", "clientOrderId"] {
        if let Some(x) = v.get(key) {
            o[key] = x.clone();
        }
    }
    if o.get("symbol").is_none() || o.get("side").is_none() || o.get("volume").is_none() {
        return Err(ApiError::validation("symbol", "symbol, side and volume are required."));
    }
    if o.get("type").is_none() {
        o["type"] = json!("market");
    }
    term(&st, &k, Method::POST, "/v1/terminal/orders", Some(&o)).await.map(|v| clean(&v))
}
fn ticket(p: &[String]) -> Result<i64, ApiError> {
    p.first().and_then(|t| t.parse().ok()).ok_or_else(|| ApiError::BadRequest("Invalid ticket.".into()))
}
async fn h_close(st: AppState, k: Key, p: Vec<String>, v: Value, _: String) -> Result<Value, ApiError> {
    let t = ticket(&p)?;
    let mut b = json!({});
    if let Some(x) = f(&v, "volume") {
        b["volume"] = json!(x);
    }
    term(&st, &k, Method::POST, &format!("/v1/terminal/positions/{t}/close"), Some(&b)).await
}
async fn h_modify(st: AppState, k: Key, p: Vec<String>, v: Value, _: String) -> Result<Value, ApiError> {
    let t = ticket(&p)?;
    let mut b = json!({});
    for key in ["sl", "tp", "trailingPoints"] {
        if let Some(x) = v.get(key) {
            b[key] = x.clone();
        }
    }
    term(&st, &k, Method::PATCH, &format!("/v1/terminal/positions/{t}"), Some(&b)).await
}
async fn h_cancel(st: AppState, k: Key, p: Vec<String>, _: Value, _: String) -> Result<Value, ApiError> {
    let t = ticket(&p)?;
    term(&st, &k, Method::DELETE, &format!("/v1/terminal/orders/{t}"), None).await
}
async fn h_quotes(st: AppState, k: Key, _: Vec<String>, _: Value, q: String) -> Result<Value, ApiError> {
    let syms: Vec<String> = q
        .split('&')
        .find_map(|p| p.strip_prefix("symbols="))
        .unwrap_or("")
        .split(|c| c == ',' || c == '%')
        .map(|s| s.trim_start_matches("2C").to_uppercase())
        .filter(|s| st.specs.get(s).is_some())
        .take(50)
        .collect();
    if syms.is_empty() {
        return Err(ApiError::validation("symbols", "Pass symbols=EURUSD,XAUUSD"));
    }
    let acct = st.engine.account(&k.tenant, k.user_id, k.login).await.map_err(|_| ApiError::unavailable("Trading service is unavailable."))?.unwrap_or(json!({}));
    let group = acct.pointer("/account/group").and_then(Value::as_str).unwrap_or("standard").to_string();
    let groups = st.engine.groups(&k.tenant).await.unwrap_or_default();
    let sg = groups.iter().find(|g| g.get("code").and_then(Value::as_str) == Some(&group)).and_then(|g| g.get("spreadGroup").and_then(Value::as_str)).unwrap_or("standard").to_string();
    let qs = st.md.quotes(&syms, &sg).await.map_err(|_| ApiError::unavailable("Market data is unavailable."))?;
    Ok(json!({"quotes": qs.iter().map(|(s, q)| (s.clone(), json!({"bid": q.bid, "ask": q.ask, "time": q.t}))).collect::<serde_json::Map<_, _>>()}))
}

endpoint!(account, false, h_account);
endpoint!(positions, false, h_positions);
endpoint!(orders, false, h_orders);
endpoint!(history, false, h_history);
endpoint!(quotes, false, h_quotes);
endpoint!(place, true, h_place);
endpoint!(close_position, true, h_close);
endpoint!(modify_position, true, h_modify);
endpoint!(cancel_order, true, h_cancel);

/// OpenAPI 3.1 description of the public API.
pub async fn openapi(State(st): State<AppState>) -> Json<Value> {
    let err = json!({"$ref": "#/components/responses/Error"});
    let op = |summary: &str, scope: &str| json!({"summary": summary, "security": [{"bearer": []}, {"hmac": []}], "x-scope": scope, "responses": {"200": {"description": "OK"}, "401": err, "403": err, "422": err, "429": err}});
    Json(json!({
        "openapi": "3.1.0",
        "info": {"title": "Kalks Trading API", "version": "1.0", "description": "REST access to one trading account per API key. Orders carry source \"api\". Withdrawals are never possible through the API."},
        "servers": [{"url": format!("{}/public/v1", st.cfg.public_url)}],
        "components": {
            "securitySchemes": {
                "bearer": {"type": "http", "scheme": "bearer", "description": "Authorization: Bearer <key_id>:<secret>"},
                "hmac": {"type": "apiKey", "in": "header", "name": "X-Kalks-Signature", "description": "X-Kalks-Key, X-Kalks-Timestamp (unix ms, ±30 s) and X-Kalks-Signature = hex(HMAC-SHA256(secret, timestamp + METHOD + path_with_query + body)); each signature is accepted once"}
            },
            "responses": {"Error": {"description": "Error", "content": {"application/json": {"schema": {"type": "object", "properties": {"error": {"type": "object", "properties": {"code": {"type": "string"}, "message": {"type": "string"}}}}}}}}}
        },
        "paths": {
            "/account": {"get": op("Account balance, equity, margin", "read")},
            "/positions": {"get": op("Open positions", "read")},
            "/orders": {"get": op("Pending orders", "read"), "post": {"summary": "Place an order", "x-scope": "trade", "security": [{"bearer": []}, {"hmac": []}],
                "requestBody": {"content": {"application/json": {"schema": {"type": "object", "required": ["symbol", "side", "volume"], "properties": {
                    "symbol": {"type": "string"}, "side": {"enum": ["buy", "sell"]}, "type": {"enum": ["market", "limit", "stop", "stop_limit"]}, "volume": {"type": "number"},
                    "price": {"type": "number"}, "stopLimit": {"type": "number"}, "sl": {"type": "number"}, "tp": {"type": "number"}, "trailingPoints": {"type": "integer"},
                    "expiry": {"type": "string"}, "requestedPrice": {"type": "number"}, "deviationPoints": {"type": "integer"}, "comment": {"type": "string"}, "clientOrderId": {"type": "string", "description": "idempotency: a repeated id returns status duplicate"}}}}}},
                "responses": {"200": {"description": "filled / placed / duplicate"}, "422": err}}},
            "/orders/{ticket}": {"delete": op("Cancel a pending order", "trade")},
            "/positions/{ticket}": {"patch": op("Modify SL / TP / trailing (null clears)", "trade")},
            "/positions/{ticket}/close": {"post": op("Close a position (optional partial volume)", "trade")},
            "/history": {"get": op("Closed deals (from, to, page, limit)", "read")},
            "/quotes": {"get": op("Bid / ask with the account's spread (symbols=EURUSD,XAUUSD)", "read")},
        }
    }))
}
