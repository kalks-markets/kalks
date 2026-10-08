//! Test harness: a throw-away database `kalks_circle_test_<pid>_<n>` on the local PostgreSQL
//! (CIRCLE_TEST_DATABASE_URL, default :5433), the service on an ephemeral port, and one mock server standing in
//! for every upstream (gateway, trading engine, market-data, Academy, support /v1/notify, growth, Claude, S3, FCM).
//! Tests are skipped when PostgreSQL is unreachable.

#![allow(dead_code)]

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post};
use circle::config::Config;
use circle::state::AppState;
use circle::{api, db};
use serde_json::{Value, json};
use sqlx::ConnectOptions;
use sqlx::postgres::PgConnectOptions;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

static N: AtomicUsize = AtomicUsize::new(0);
pub const TOKEN: &str = "test-internal-token";

#[derive(Default)]
pub struct Mock {
    pub notifies: Mutex<Vec<(String, Value)>>,
    pub growth: Mutex<Vec<Value>>,
    pub fcm: Mutex<Vec<Value>>,
    pub fcm_fail_token: Mutex<Option<String>>,
    pub claude: Mutex<Vec<Value>>,
    /// Verdict the Claude mock gives when images are attached.
    pub image_verdict: Mutex<Option<Value>>,
    /// user -> engine accounts
    pub accounts: Mutex<HashMap<i64, Vec<Value>>>,
    /// login -> positions
    pub positions: Mutex<HashMap<i64, Vec<Value>>>,
    /// login -> deals
    pub deals: Mutex<HashMap<i64, Vec<Value>>>,
    /// user -> master record
    pub masters: Mutex<HashMap<i64, Value>>,
    pub s3: Mutex<HashMap<String, Vec<u8>>>,
    pub s3_bad_auth: AtomicUsize,
}

pub struct Env {
    pub base: String,
    pub st: AppState,
    pub mock: Arc<Mock>,
    pub mock_url: String,
    admin: PgConnectOptions,
    name: String,
    pub http: reqwest::Client,
    pub dir: String,
}

pub struct Opts {
    pub ai: bool,
}

pub async fn env() -> Option<Env> {
    env_with(Opts { ai: false }).await
}

pub async fn env_with(o: Opts) -> Option<Env> {
    let url = std::env::var("CIRCLE_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into());
    let admin = PgConnectOptions::from_str(&url).ok()?.database("postgres");
    if admin.connect().await.is_err() {
        eprintln!("skipping circle tests: no PostgreSQL at {url}");
        return None;
    }
    let name = format!("kalks_circle_test_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst));
    let db_url = admin.clone().database(&name).to_url_lossy().to_string();
    let pool = db::connect(&db_url).await.expect("create + migrate");
    let dir = std::env::temp_dir().join(&name).to_string_lossy().to_string();
    let mock = Arc::new(Mock::default());
    let mock_url = spawn_mock(mock.clone()).await;
    let mut cfg = Config::for_tests(&db_url, &dir);
    cfg.internal_token = TOKEN.into();
    for u in [&mut cfg.gateway_url, &mut cfg.trading_url, &mut cfg.market_data_url, &mut cfg.academy_url, &mut cfg.growth_url, &mut cfg.notify_url, &mut cfg.anthropic_url, &mut cfg.fcm_api_url] {
        *u = mock_url.clone();
    }
    if o.ai {
        cfg.anthropic_key = "test-key".into();
    }
    // keep the tests deterministic: no ffmpeg even when the machine has one
    cfg.ffmpeg = "/nonexistent/ffmpeg".into();
    cfg.ffprobe = "/nonexistent/ffprobe".into();
    let st = AppState::new(pool, cfg);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = api::router(st.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    Some(Env { base, st, mock, mock_url, admin, name, http: reqwest::Client::new(), dir })
}

impl Env {
    pub async fn drop(self) {
        self.st.pool.close().await;
        let _ = std::fs::remove_dir_all(&self.dir);
        if let Ok(mut c) = self.admin.connect().await {
            let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{}\" WITH (FORCE)", self.name))).execute(&mut c).await;
        }
    }

    /// A client request: user `uid` of broker `kalks`, KYC verified, India.
    pub fn user(&self, method: reqwest::Method, path: &str, uid: i64) -> reqwest::RequestBuilder {
        self.user_of(method, path, uid, "kalks")
    }

    /// A client request with some headers replaced (e.g. `[("x-kalks-country", "ir")]`).
    pub fn user_with(&self, method: reqwest::Method, path: &str, uid: i64, tenant: &str, overrides: &[(&str, &str)]) -> reqwest::RequestBuilder {
        let mut rb = self.http.request(method, format!("{}{path}", self.base)).header("x-kalks-internal", TOKEN).header("x-kalks-user-id", uid.to_string()).header("x-kalks-tenant", tenant);
        for (k, v) in [("x-kalks-user-name", "Eve%20Stone"), ("x-kalks-country", "in"), ("x-kalks-kyc", "verified"), ("x-kalks-locale", "en")] {
            if !overrides.iter().any(|(o, _)| *o == k) {
                rb = rb.header(k, v);
            }
        }
        for (k, v) in overrides {
            rb = rb.header(*k, *v);
        }
        rb
    }

    pub fn user_of(&self, method: reqwest::Method, path: &str, uid: i64, tenant: &str) -> reqwest::RequestBuilder {
        let name = match uid % 4 {
            0 => "Ana%20Silva",
            1 => "Ben%20Okafor",
            2 => "Chen%20Wei",
            _ => "Dara%20Khan",
        };
        self.http
            .request(method, format!("{}{path}", self.base))
            .header("x-kalks-internal", TOKEN)
            .header("x-kalks-user-id", uid.to_string())
            .header("x-kalks-tenant", tenant)
            .header("x-kalks-user-name", name)
            .header("x-kalks-country", "in")
            .header("x-kalks-kyc", "verified")
            .header("x-kalks-locale", "en")
            .header("x-kalks-referral-code", format!("REF{uid}"))
    }

    pub fn staff(&self, method: reqwest::Method, path: &str, perms: &str) -> reqwest::RequestBuilder {
        self.staff_of(method, path, perms, "kalks")
    }

    pub fn staff_of(&self, method: reqwest::Method, path: &str, perms: &str, tenant: &str) -> reqwest::RequestBuilder {
        self.http
            .request(method, format!("{}{path}", self.base))
            .header("x-kalks-internal", TOKEN)
            .header("x-kalks-tenant", tenant)
            .header("x-kalks-staff-id", "s1")
            .header("x-kalks-staff-name", "Mei%20Lin")
            .header("x-kalks-staff-role", "admin")
            .header("x-kalks-staff-perms", perms)
    }

    pub async fn json(rb: reqwest::RequestBuilder) -> (u16, Value) {
        let r = rb.send().await.unwrap();
        let s = r.status().as_u16();
        (s, r.json().await.unwrap_or(Value::Null))
    }

    pub async fn get(&self, path: &str, uid: i64) -> (u16, Value) {
        Env::json(self.user(reqwest::Method::GET, path, uid)).await
    }

    pub async fn post(&self, path: &str, uid: i64, body: Value) -> (u16, Value) {
        Env::json(self.user(reqwest::Method::POST, path, uid).json(&body)).await
    }

    pub async fn patch(&self, path: &str, uid: i64, body: Value) -> (u16, Value) {
        Env::json(self.user(reqwest::Method::PATCH, path, uid).json(&body)).await
    }

    pub async fn delete(&self, path: &str, uid: i64) -> (u16, Value) {
        Env::json(self.user(reqwest::Method::DELETE, path, uid)).await
    }

    /// Creates the profile of `uid` and sets its handle.
    pub async fn member(&self, uid: i64, handle: &str) -> Value {
        let (s, v) = self.get("/v1/circle/me", uid).await;
        assert_eq!(s, 200, "{v}");
        let (s, v) = self.patch("/v1/circle/me", uid, json!({"handle": handle, "onboarded": true})).await;
        assert_eq!(s, 200, "{v}");
        v["profile"].clone()
    }

    /// Runs the background work until nothing is left (media, safety checks, chat, notifications).
    pub async fn settle(&self) {
        for _ in 0..5 {
            let a = circle::media::process_pending(&self.st).await.unwrap();
            let b = circle::workers::moderate_once(&self.st).await.unwrap();
            if a == 0 && b == 0 {
                break;
            }
        }
    }

    pub async fn deliver(&self) -> usize {
        sqlx::query("UPDATE notify_outbox SET next_at = now() WHERE status = 'pending'").execute(&self.st.pool).await.unwrap();
        circle::notify::deliver(&self.st).await.unwrap()
    }

    /// Creates a published post of `uid` and returns its id.
    pub async fn post_text(&self, uid: i64, body: &str) -> i64 {
        let (s, v) = self.post("/v1/circle/posts", uid, json!({"body": body})).await;
        assert_eq!(s, 200, "{v}");
        let id = v["post"]["id"].as_i64().unwrap();
        self.settle().await;
        id
    }
}

/// A small PNG (w × h).
pub fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::ImageBuffer::from_fn(w, h, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 90u8]));
    let mut out = Vec::new();
    image::DynamicImage::ImageRgb8(img).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
    out
}

// ---------------------------------------------------------------- the mock upstream

type M = State<Arc<Mock>>;

fn uid(h: &HeaderMap) -> i64 {
    h.get("x-kalks-user-id").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok()).unwrap_or(0)
}

fn owns(m: &Mock, user: i64, login: i64) -> bool {
    m.accounts.lock().unwrap().get(&user).is_some_and(|a| a.iter().any(|x| x["login"].as_i64() == Some(login)))
}

async fn spawn_mock(m: Arc<Mock>) -> String {
    let app = Router::new()
        .route("/v1/internal/tenants/{slug}", get(tenant))
        .route("/v1/accounts", get(accounts))
        .route("/v1/accounts/{login}", get(account))
        .route("/v1/accounts/{login}/history", get(history))
        .route("/v1/social/master/me", get(master))
        .route("/v1/quotes", get(quotes))
        .route("/v1/instruments", get(instruments))
        .route("/v1/me/certificates", get(certificates))
        .route("/v1/notify", post(notify))
        .route("/v1/growth/admin/points/adjust", post(growth))
        .route("/v1/messages", post(claude))
        .route("/token", post(fcm_token))
        .route("/v1/projects/{project}/{send}", post(fcm_send))
        .route("/{bucket}/{*key}", any(s3))
        .with_state(m);
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    url
}

async fn tenant(Path(slug): Path<String>) -> Response {
    // broker "offbroker" has Circle switched off; "blockedbroker" blocks Nigeria
    let modules = json!({"circle": slug != "offbroker", "rewards": true});
    let blocked = if slug == "blockedbroker" { json!(["ng"]) } else { json!([]) };
    axum::Json(json!({"id": 1, "slug": slug, "name": slug, "status": "active", "modules": modules, "blockedCountries": blocked})).into_response()
}

async fn accounts(State(m): M, h: HeaderMap) -> Response {
    let list = m.accounts.lock().unwrap().get(&uid(&h)).cloned().unwrap_or_default();
    axum::Json(json!({"accounts": list})).into_response()
}

async fn account(State(m): M, h: HeaderMap, Path(login): Path<i64>) -> Response {
    let u = uid(&h);
    if !owns(&m, u, login) {
        return (StatusCode::NOT_FOUND, axum::Json(json!({"error": "not found"}))).into_response();
    }
    let acc = m.accounts.lock().unwrap().get(&u).and_then(|a| a.iter().find(|x| x["login"].as_i64() == Some(login)).cloned()).unwrap();
    let positions = m.positions.lock().unwrap().get(&login).cloned().unwrap_or_default();
    axum::Json(json!({"account": acc, "positions": positions, "orders": []})).into_response()
}

async fn history(State(m): M, h: HeaderMap, Path(login): Path<i64>) -> Response {
    if !owns(&m, uid(&h), login) {
        return (StatusCode::NOT_FOUND, axum::Json(json!({"error": "not found"}))).into_response();
    }
    axum::Json(json!({"deals": m.deals.lock().unwrap().get(&login).cloned().unwrap_or_default()})).into_response()
}

async fn master(State(m): M, h: HeaderMap) -> Response {
    axum::Json(json!({"master": m.masters.lock().unwrap().get(&uid(&h)).cloned(), "settings": {}, "candidates": []})).into_response()
}

async fn quotes(Query(q): Query<HashMap<String, String>>) -> Response {
    let mut out = serde_json::Map::new();
    for s in q.get("symbols").map(|s| s.split(',').map(str::to_string).collect::<Vec<_>>()).unwrap_or_default() {
        let px = match s.as_str() {
            "XAUUSD" => 2410.0,
            "EURUSD" => 1.1050,
            _ => 100.0,
        };
        out.insert(s, json!({"bid": px, "ask": px, "last": px, "t": 1}));
    }
    axum::Json(Value::Object(out)).into_response()
}

async fn instruments() -> Response {
    axum::Json(json!([{"symbol": "XAUUSD", "name": "Gold"}, {"symbol": "EURUSD", "name": "Euro / US Dollar"}, {"symbol": "BTCUSD", "name": "Bitcoin"}])).into_response()
}

async fn certificates() -> Response {
    axum::Json(json!({"certificates": [{"code": "C1", "phase": "basics", "phase_title": "Basics", "level": "beginner", "issued_at": "2026-10-01T00:00:00Z", "verify_url": "https://x/c/C1"}]})).into_response()
}

async fn notify(State(m): M, h: HeaderMap, axum::Json(b): axum::Json<Value>) -> Response {
    let tenant = h.get("x-kalks-tenant").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    m.notifies.lock().unwrap().push((tenant, b));
    axum::Json(json!({"status": "ok"})).into_response()
}

async fn growth(State(m): M, h: HeaderMap, axum::Json(b): axum::Json<Value>) -> Response {
    let mut b = b;
    b["staffRole"] = json!(h.get("x-kalks-staff-role").and_then(|v| v.to_str().ok()));
    m.growth.lock().unwrap().push(b);
    axum::Json(json!({"balance": 50})).into_response()
}

/// Claude mock: answers per system prompt; moderation keys on "scamword" / "reviewword" and `image_verdict`.
async fn claude(State(m): M, h: HeaderMap, axum::Json(b): axum::Json<Value>) -> Response {
    if h.get("x-api-key").and_then(|v| v.to_str().ok()) != Some("test-key") {
        return (StatusCode::UNAUTHORIZED, axum::Json(json!({"error": {"message": "bad key"}}))).into_response();
    }
    m.claude.lock().unwrap().push(b.clone());
    let system = b["system"][0]["text"].as_str().unwrap_or("");
    let blocks = b["messages"][0]["content"].as_array().cloned().unwrap_or_default();
    let text: String = blocks.iter().filter_map(|x| x["text"].as_str()).collect::<Vec<_>>().join("\n");
    let images = blocks.iter().filter(|x| x["type"] == "image").count();
    let answer = if system.contains("content safety classifier") {
        if images > 0 && let Some(v) = m.image_verdict.lock().unwrap().clone() {
            v
        } else if text.contains("scamword") {
            json!({"decision": "block", "categories": ["scam"], "reason": "Asks for money."})
        } else if text.contains("reviewword") {
            json!({"decision": "review", "categories": ["spam"], "reason": "Looks promotional."})
        } else {
            json!({"decision": "allow", "categories": [], "reason": ""})
        }
    } else if system.contains("You translate") {
        json!({"text": format!("[translated] {}", text.lines().skip_while(|l| !l.starts_with("<text>")).nth(1).unwrap_or(""))})
    } else if system.contains("community is saying") {
        json!({"mood": "bullish", "summary": "Most members expect gold to hold its gains.", "points": ["Central-bank buying", "Weaker dollar"]})
    } else if system.contains("short, honest caption") {
        json!({"caption": "Closed my $XAUUSD long at TP.", "explanation": "A long position profits when the price rises.", "hashtags": ["#gold"]})
    } else {
        json!({"headline": "Gold talk dominated.", "items": []})
    };
    axum::Json(json!({"id": "msg_1", "type": "message", "model": b["model"], "stop_reason": "end_turn", "content": [{"type": "text", "text": answer.to_string()}]})).into_response()
}

async fn fcm_token(body: Bytes) -> Response {
    let s = String::from_utf8_lossy(&body);
    if !s.contains("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer") || !s.contains("assertion=") {
        return StatusCode::BAD_REQUEST.into_response();
    }
    axum::Json(json!({"access_token": "ya29.test", "expires_in": 3600})).into_response()
}

async fn fcm_send(State(m): M, h: HeaderMap, Path((project, send)): Path<(String, String)>, axum::Json(b): axum::Json<Value>) -> Response {
    if send != "messages:send" || h.get("authorization").and_then(|v| v.to_str().ok()) != Some("Bearer ya29.test") {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let token = b["message"]["token"].as_str().unwrap_or("").to_string();
    if m.fcm_fail_token.lock().unwrap().as_deref() == Some(token.as_str()) {
        return (StatusCode::NOT_FOUND, axum::Json(json!({"error": {"status": "NOT_FOUND", "details": [{"errorCode": "UNREGISTERED"}]}}))).into_response();
    }
    let mut b = b;
    b["project"] = json!(project);
    m.fcm.lock().unwrap().push(b);
    axum::Json(json!({"name": "projects/p/messages/1"})).into_response()
}

async fn s3(State(m): M, method: axum::http::Method, h: HeaderMap, Path((bucket, key)): Path<(String, String)>, body: Bytes) -> Response {
    let auth = h.get("authorization").and_then(|v| v.to_str().ok()).unwrap_or("");
    let ok = auth.starts_with("AWS4-HMAC-SHA256 Credential=AKTEST/") && auth.contains("/auto/s3/aws4_request") && auth.contains("SignedHeaders=host;x-amz-content-sha256;x-amz-date") && h.contains_key("x-amz-date");
    if !ok || bucket != "circle-media" {
        m.s3_bad_auth.fetch_add(1, Ordering::SeqCst);
        return StatusCode::FORBIDDEN.into_response();
    }
    let k = format!("{bucket}/{key}");
    match method.as_str() {
        "PUT" => {
            m.s3.lock().unwrap().insert(k, body.to_vec());
            StatusCode::OK.into_response()
        }
        "GET" => match m.s3.lock().unwrap().get(&k) {
            Some(b) => b.clone().into_response(),
            None => StatusCode::NOT_FOUND.into_response(),
        },
        "DELETE" => {
            m.s3.lock().unwrap().remove(&k);
            StatusCode::NO_CONTENT.into_response()
        }
        _ => StatusCode::METHOD_NOT_ALLOWED.into_response(),
    }
}
