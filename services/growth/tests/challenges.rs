//! Challenges (phase 4) against a throw-away database `kalks_growth_ch_<pid>_<n>` on the local PostgreSQL
//! (GROWTH_TEST_DATABASE_URL, default :5433); skipped when PostgreSQL is unreachable. The engine and the wallet are
//! one mock HTTP server; the service's own router is served on a loopback port for the HTTP checks.
//!
//! What it guards: templates turn into contests (prize split, schedule, asset-class filter) and clone; missions
//! count only qualifying trades and reward once per period; XP books once per ref, levels and streaks; the
//! auto-pay threshold (below paid at once, above waits for staff); leaderboards never expose names, emails,
//! logins or user ids, and leave opted-out clients off the public board.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{Duration, Utc};
use growth::audit::Actor;
use growth::config::Config;
use growth::deals::{self, AccountFacts, DealIn, Programme};
use growth::money::{D, dec};
use growth::state::AppState;
use growth::{challenge_calc, contests, db, missions, payouts, xp};
use serde_json::{Value, json};
use sqlx::ConnectOptions;
use sqlx::postgres::PgConnectOptions;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

static N: AtomicUsize = AtomicUsize::new(0);
static DEAL: AtomicI64 = AtomicI64::new(7_000_001);
const TOKEN: &str = "test-internal-token";

// ---------------------------------------------------------------- mock engine + wallet

#[derive(Default)]
struct Mock {
    accounts: HashMap<i64, Value>,
    ledgers: HashMap<i64, Vec<Value>>,
    postings: Vec<(i64, String, D, String)>,
    wallet: Vec<(String, i64, D, String)>,
}
type M = Arc<Mutex<Mock>>;

async fn m_account(State(m): State<M>, Path(login): Path<i64>) -> (axum::http::StatusCode, Json<Value>) {
    match m.lock().unwrap().accounts.get(&login) {
        Some(a) => (axum::http::StatusCode::OK, Json(json!({"account": a}))),
        None => (axum::http::StatusCode::NOT_FOUND, Json(json!({"error": {"code": "not_found"}}))),
    }
}

async fn m_accounts(State(m): State<M>, Query(q): Query<HashMap<String, String>>) -> Json<Value> {
    let user: i64 = q.get("user_id").and_then(|u| u.parse().ok()).unwrap_or(0);
    let kind = q.get("type").cloned();
    let g = m.lock().unwrap();
    let items: Vec<Value> = g.accounts.values().filter(|a| a["userId"] == user && kind.as_ref().is_none_or(|k| a["type"] == k.as_str())).cloned().collect();
    Json(json!({"items": items, "total": items.len()}))
}

async fn m_balance(State(m): State<M>, Path(login): Path<i64>, Json(b): Json<Value>) -> (axum::http::StatusCode, Json<Value>) {
    let mut g = m.lock().unwrap();
    let key = b["idempotencyKey"].as_str().unwrap_or("").to_string();
    if g.postings.iter().any(|p| p.3 == key) {
        return (axum::http::StatusCode::CONFLICT, Json(json!({"error": {"code": "duplicate_idempotency_key"}})));
    }
    let amount = D::from_str(b["amount"].as_str().unwrap()).unwrap();
    g.postings.push((login, b["type"].as_str().unwrap().to_string(), amount, key));
    (axum::http::StatusCode::OK, Json(json!({"data": {"txn": 1}})))
}

async fn m_ledger(State(m): State<M>, Path(login): Path<i64>) -> Json<Value> {
    Json(json!({"items": m.lock().unwrap().ledgers.get(&login).cloned().unwrap_or_default()}))
}

async fn m_wallet(State(m): State<M>, Json(b): Json<Value>) -> Json<Value> {
    let mut g = m.lock().unwrap();
    let key = b["idempotency_key"].as_str().unwrap().to_string();
    if !g.wallet.iter().any(|w| w.0 == key) {
        g.wallet.push((key, b["user_id"].as_i64().unwrap(), D::from_str(b["amount"].as_str().unwrap()).unwrap(), b["kind"].as_str().unwrap().to_string()));
    }
    Json(json!({"status": "completed", "txn_id": g.wallet.len()}))
}

async fn mock() -> (String, M) {
    let m: M = Arc::new(Mutex::new(Mock::default()));
    let app = Router::new()
        .route("/v1/admin/accounts", get(m_accounts))
        .route("/v1/admin/accounts/{login}", get(m_account))
        .route("/v1/admin/accounts/{login}/balance", post(m_balance))
        .route("/v1/accounts/{login}/ledger", get(m_ledger))
        .route("/v1/wallets/transfers", post(m_wallet))
        .with_state(m.clone());
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    (url, m)
}

struct Env {
    st: AppState,
    m: M,
    admin: PgConnectOptions,
    name: String,
    base: String,
    http: reqwest::Client,
}

async fn env() -> Option<Env> {
    let base = std::env::var("GROWTH_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into());
    let admin = PgConnectOptions::from_str(&base).ok()?.database("postgres");
    if admin.connect().await.is_err() {
        eprintln!("skipping challenges DB tests: no PostgreSQL at {base}");
        return None;
    }
    let name = format!("kalks_growth_ch_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst));
    let url = admin.clone().database(&name).to_url_lossy().to_string();
    let pool = db::connect(&url).await.expect("create + migrate");
    db::seed(&pool, "kalks").await.unwrap();
    let (mock_url, m) = mock().await;
    let mut cfg = Config::for_tests(&url);
    cfg.trading_url = mock_url.clone();
    cfg.gateway_url = mock_url.clone();
    cfg.wallet_url = mock_url;
    cfg.internal_token = TOKEN.into();
    let st = AppState::new(pool, cfg);
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = format!("http://{}", l.local_addr().unwrap());
    let router = growth::api::router(st.clone());
    tokio::spawn(async move { axum::serve(l, router).await.unwrap() });
    Some(Env { st, m, admin, name, base: addr, http: reqwest::Client::new() })
}

impl Env {
    async fn drop(self) {
        self.st.pool.close().await;
        if let Ok(mut c) = self.admin.connect().await {
            let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{}\" WITH (FORCE)", self.name))).execute(&mut c).await;
        }
    }

    fn add_account(&self, login: i64, user: i64, kind: &str) {
        self.m.lock().unwrap().accounts.insert(
            login,
            json!({"login": login, "userId": user, "type": kind, "group": "standard", "cent": false, "currency": "USD", "status": "active", "balance": 1000.0, "credit": 0.0, "bonus": 0.0, "equity": 1000.0}),
        );
    }

    async fn profile(&self, user: i64, first: &str, last: &str, email: &str, kyc: &str) {
        sqlx::query("INSERT INTO profiles (tenant, user_id, first_name, last_name, email, country, kyc_status) VALUES ('kalks',$1,$2,$3,$4,'IN',$5) ON CONFLICT (tenant, user_id) DO UPDATE SET kyc_status = $5")
            .bind(user)
            .bind(first)
            .bind(last)
            .bind(email)
            .bind(kyc)
            .execute(&self.st.pool)
            .await
            .unwrap();
    }

    /// A closed CFD deal: `hold_secs` held, closed now.
    #[allow(clippy::too_many_arguments)]
    async fn deal(&self, login: i64, user: i64, kind: &str, symbol: &str, lots: &str, profit: &str, hold_secs: i64) {
        let p = Programme::load(&self.st, "kalks").await.unwrap();
        let now = Utc::now();
        let d = DealIn {
            deal_id: DEAL.fetch_add(1, Ordering::SeqCst),
            tenant: "kalks".into(),
            login,
            user_id: user,
            symbol: symbol.into(),
            side: "buy".into(),
            volume: dec(lots),
            profit: dec(profit),
            open_time: now - Duration::seconds(hold_secs),
            close_time: now,
            kind: "close".into(),
            reversed: false,
            account: AccountFacts { kind: kind.into(), group: "standard".into(), cent: false },
            option: false,
            premium: None,
            fill_id: None,
        };
        deals::ingest(&self.st, &p, &d).await.unwrap();
    }

    async fn settings(&self, patch: Value) {
        db::settings(&self.st.pool, "kalks").await.unwrap();
        sqlx::query("UPDATE settings SET data = data || $1 WHERE tenant = 'kalks'").bind(sqlx::types::Json(patch)).execute(&self.st.pool).await.unwrap();
    }

    async fn one<T: for<'r> sqlx::Decode<'r, sqlx::Postgres> + sqlx::Type<sqlx::Postgres> + Send + Unpin>(&self, sql: &str) -> T {
        sqlx::query_scalar::<_, T>(sqlx::AssertSqlSafe(sql.to_string())).fetch_one(&self.st.pool).await.unwrap()
    }

    async fn staff(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> (u16, Value) {
        let mut r = self
            .http
            .request(method, format!("{}{path}", self.base))
            .header("x-kalks-internal", TOKEN)
            .header("x-kalks-staff-id", "1")
            .header("x-kalks-staff-role", "admin")
            .header("x-kalks-staff-perms", "marketing.read,marketing.write,marketing.approve");
        if let Some(b) = body {
            r = r.json(&b);
        }
        let res = r.send().await.unwrap();
        (res.status().as_u16(), res.json().await.unwrap_or(Value::Null))
    }

    async fn client(&self, user: i64, method: reqwest::Method, path: &str, body: Option<Value>) -> (u16, Value) {
        let mut r = self.http.request(method, format!("{}{path}", self.base)).header("x-kalks-internal", TOKEN).header("x-kalks-user-id", user.to_string());
        if let Some(b) = body {
            r = r.json(&b);
        }
        let res = r.send().await.unwrap();
        (res.status().as_u16(), res.json().await.unwrap_or(Value::Null))
    }

    async fn entry(&self, contest: i64, user: i64, login: i64, name: &str, joined_ago_min: i32) -> i64 {
        sqlx::query_scalar("INSERT INTO contest_entries (contest_id, tenant, user_id, login, display_name, country, start_equity, joined_at) VALUES ($1,'kalks',$2,$3,$4,'IN',1000, now() - make_interval(mins => $5)) RETURNING id")
            .bind(contest)
            .bind(user)
            .bind(login)
            .bind(name)
            .bind(joined_ago_min)
            .fetch_one(&self.st.pool)
            .await
            .unwrap()
    }
}

fn system() -> Actor {
    Actor { id: "staff:1".into(), name: Some("Test".into()) }
}

// ---------------------------------------------------------------- templates

#[tokio::test]
async fn templates_become_contests_and_clone() {
    let Some(e) = env().await else { return };
    let (s, list) = e.staff(reqwest::Method::GET, "/v1/growth/admin/contest-templates", None).await;
    assert_eq!(s, 200);
    let items = list["items"].as_array().unwrap();
    assert_eq!(items.len(), 3, "seeded per broker");
    assert!(items.iter().all(|t| t["kind"] == "live" && t["kycRequired"] == true), "launch templates are free live contests with KYC");
    assert!(list["presets"].as_array().unwrap().iter().any(|p| p["key"] == "top10"));
    let weekly = items.iter().find(|t| t["key"] == "weekly-return").unwrap();
    let crypto = items.iter().find(|t| t["key"] == "weekend-crypto").unwrap();

    // step 2 of the wizard: a pool and a split; the window comes from the schedule
    let (s, r) = e.staff(reqwest::Method::POST, &format!("/v1/growth/admin/contest-templates/{}/contest", weekly["id"]), Some(json!({"prizePool": 2000, "split": "top3"}))).await;
    assert_eq!(s, 200, "{r}");
    let c = &r["contest"];
    assert_eq!(c["kind"], "live");
    assert_eq!(c["templateId"], weekly["id"]);
    assert_eq!(c["prizePool"], 2000.0);
    assert_eq!(c["prizes"].as_array().unwrap().iter().map(|p| p["amount"].as_f64().unwrap()).collect::<Vec<_>>(), vec![1000.0, 600.0, 400.0]);
    let starts = chrono::DateTime::parse_from_rfc3339(c["startsAt"].as_str().unwrap()).unwrap().with_timezone(&Utc);
    assert_eq!(challenge_calc::server_day(starts).format("%u").to_string(), "1", "starts on a Monday (server time)");
    // an explicit split that doesn't add up is refused
    let (s, r) = e.staff(reqwest::Method::POST, &format!("/v1/growth/admin/contest-templates/{}/contest", weekly["id"]), Some(json!({"split": [{"rankFrom": 1, "rankTo": 1, "pct": 70}]}))).await;
    assert_eq!((s, r["error"]["field"].as_str()), (422, Some("split")));

    // clone: a draft of the same length after the original
    let id = c["id"].as_i64().unwrap();
    let (s, r) = e.staff(reqwest::Method::POST, &format!("/v1/growth/admin/contests/{id}/duplicate"), Some(json!({}))).await;
    assert_eq!(s, 200, "{r}");
    assert_eq!((r["contest"]["status"].as_str(), r["contest"]["clonedFrom"].as_i64()), (Some("draft"), Some(id)));
    assert_eq!(r["contest"]["startsAt"], c["endsAt"]);
    assert_eq!(r["contest"]["prizes"], c["prizes"]);
    assert_ne!(r["contest"]["slug"], c["slug"]);

    // the crypto sprint counts crypto deals only
    let (_, r) = e.staff(reqwest::Method::POST, &format!("/v1/growth/admin/contest-templates/{}/contest", crypto["id"]), Some(json!({"name": "Sprint now", "startsAt": (Utc::now() - Duration::hours(1)).to_rfc3339(), "endsAt": (Utc::now() + Duration::hours(1)).to_rfc3339()}))).await;
    assert_eq!(r["contest"]["assetClasses"], json!(["crypto"]));
    let cid = r["contest"]["id"].as_i64().unwrap();
    e.entry(cid, 11, 51000011, "A", 30).await;
    e.deal(51000011, 11, "live", "BTCUSD", "1", "50", 600).await;
    e.deal(51000011, 11, "live", "EURUSD", "1", "50", 600).await;
    let n: i64 = e.one(&format!("SELECT count(*) FROM contest_trades WHERE contest_id = {cid}")).await;
    let class: Option<String> = e.one("SELECT asset_class FROM deals WHERE symbol = 'BTCUSD'").await;
    if class.as_deref() == Some("crypto") {
        assert_eq!(n, 1, "EURUSD is not crypto");
    }

    // editing a template, and saving a contest as a template
    let (s, r) = e.staff(reqwest::Method::PATCH, &format!("/v1/growth/admin/contest-templates/{}", weekly["id"]), Some(json!({"prizePool": 1500, "split": "top5"}))).await;
    assert_eq!((s, r["template"]["split"].as_str()), (200, Some("top5")));
    let (s, r) = e.staff(reqwest::Method::POST, &format!("/v1/growth/admin/contests/{id}/save-template"), Some(json!({"name": "Our weekly"}))).await;
    assert_eq!(s, 200, "{r}");
    let bands = r["template"]["split"].as_array().unwrap();
    assert_eq!(bands.iter().map(|b| b["pct"].as_f64().unwrap()).sum::<f64>(), 100.0);
    e.drop().await;
}

// ---------------------------------------------------------------- missions

#[tokio::test]
async fn missions_count_qualifying_trades_and_reward_once_per_period() {
    let Some(e) = env().await else { return };
    e.profile(21, "Asha", "Rao", "asha@example.com", "verified").await;
    e.add_account(51000021, 21, "live");
    // only this mission: 1 lot today, 30 XP + $5 cash (auto-paid below $20)
    sqlx::query("UPDATE missions SET active = false").execute(&e.st.pool).await.unwrap();
    e.settings(json!({"autoPayBelow": 20})).await;
    let (s, r) = e.staff(reqwest::Method::POST, "/v1/growth/admin/missions", Some(json!({"name": "Daily lot", "kind": "lots", "schedule": "daily", "target": 1, "xp": 30, "rewardAmount": 5, "rewardPayout": "wallet"}))).await;
    assert_eq!(s, 200, "{r}");
    let mid = r["mission"]["id"].as_i64().unwrap();
    // a scalp (held 10 s), a demo deal and a micro lot don't count
    e.deal(51000021, 21, "live", "EURUSD", "0.6", "5", 10).await;
    e.deal(51000021, 21, "demo", "EURUSD", "5", "5", 600).await;
    e.deal(51000021, 21, "live", "EURUSD", "0.001", "1", 600).await;
    assert_eq!(missions::evaluate_user(&e.st, "kalks", 21).await.unwrap(), 0);
    e.deal(51000021, 21, "live", "EURUSD", "0.6", "5", 600).await;
    assert_eq!(missions::evaluate_user(&e.st, "kalks", 21).await.unwrap(), 0);
    let p: D = e.one(&format!("SELECT progress FROM mission_progress WHERE mission_id = {mid}")).await;
    assert_eq!(p, dec("0.6"));
    e.deal(51000021, 21, "live", "XAUUSD", "0.5", "5", 600).await;
    assert_eq!(missions::evaluate_user(&e.st, "kalks", 21).await.unwrap(), 1, "1.1 lots: complete");
    // again, and concurrently: still one completion, one XP booking, one wallet credit
    let (a, b) = tokio::join!(missions::evaluate_user(&e.st, "kalks", 21), missions::evaluate_user(&e.st, "kalks", 21));
    assert_eq!(a.unwrap() + b.unwrap(), 0);
    e.deal(51000021, 21, "live", "EURUSD", "3", "5", 600).await;
    assert_eq!(missions::evaluate_user(&e.st, "kalks", 21).await.unwrap(), 0);
    assert_eq!(xp::total(&e.st.pool, "kalks", 21).await.unwrap(), 30);
    payouts::wallet_tick(&e.st).await.unwrap();
    let rs: String = e.one(&format!("SELECT reward_status FROM mission_progress WHERE mission_id = {mid}")).await;
    assert_eq!(rs, "paid");
    assert_eq!(e.m.lock().unwrap().wallet.iter().filter(|w| w.0.starts_with("growth:mission:")).count(), 1);
    // another period counts again (yesterday's row is its own)
    let m = missions::mission_of(&sqlx::query(sqlx::AssertSqlSafe(format!("SELECT * FROM missions WHERE id = {mid}"))).fetch_one(&e.st.pool).await.unwrap());
    let s = db::settings(&e.st.pool, "kalks").await.unwrap();
    assert!(missions::record(&e.st, &m, 21, "2020-01-01", dec("2"), &s).await.unwrap());
    assert!(!missions::record(&e.st, &m, 21, "2020-01-01", dec("2"), &s).await.unwrap(), "once per period");
    assert_eq!(xp::total(&e.st.pool, "kalks", 21).await.unwrap(), 60);

    // the client sees it done in the hub
    let (s, hub) = e.client(21, reqwest::Method::GET, "/v1/growth/me/hub", None).await;
    assert_eq!(s, 200, "{hub}");
    let mine = hub["missions"].as_array().unwrap().iter().find(|x| x["id"] == mid).unwrap().clone();
    assert_eq!((mine["done"].as_bool(), mine["rewardStatus"].as_str()), (Some(true), Some("paid")));
    assert_eq!(hub["level"]["xp"], 60);
    assert!(hub["recent"].as_array().unwrap().iter().any(|x| x["kind"] == "mission"));
    e.drop().await;
}

#[tokio::test]
async fn mission_rewards_wait_for_kyc_and_staff_above_the_threshold() {
    let Some(e) = env().await else { return };
    sqlx::query("UPDATE missions SET active = false").execute(&e.st.pool).await.unwrap();
    e.settings(json!({"autoPayBelow": 20})).await;
    e.profile(31, "Ben", "Cole", "ben@example.com", "pending").await;
    e.profile(32, "Cai", "Dee", "cai@example.com", "verified").await;
    e.add_account(51000032, 32, "live");
    let (_, r) = e.staff(reqwest::Method::POST, "/v1/growth/admin/missions", Some(json!({"name": "Join", "kind": "join_contest", "schedule": "weekly", "target": 1, "xp": 10, "rewardAmount": 10}))).await;
    let small = r["mission"]["id"].as_i64().unwrap();
    let (_, r) = e.staff(reqwest::Method::POST, "/v1/growth/admin/missions", Some(json!({"name": "Big join", "kind": "join_contest", "schedule": "weekly", "target": 1, "xp": 10, "rewardAmount": 50, "rewardPayout": "credit"}))).await;
    let big = r["mission"]["id"].as_i64().unwrap();
    // KYC and first-deposit missions are one-off only
    let (s, r) = e.staff(reqwest::Method::POST, "/v1/growth/admin/missions", Some(json!({"name": "KYC", "kind": "kyc", "schedule": "daily", "target": 1, "xp": 10}))).await;
    assert_eq!((s, r["error"]["field"].as_str()), (422, Some("schedule")));
    let cid: i64 = sqlx::query_scalar("INSERT INTO contests (tenant, slug, name, kind, starts_at, ends_at, scoring) VALUES ('kalks','j','J','live', now(), now() + interval '1 day','profit') RETURNING id").fetch_one(&e.st.pool).await.unwrap();
    e.entry(cid, 31, 51000031, "B", 0).await;
    e.entry(cid, 32, 51000032, "C", 0).await;
    missions::evaluate_user(&e.st, "kalks", 31).await.unwrap();
    missions::evaluate_user(&e.st, "kalks", 32).await.unwrap();
    let st = |u: i64, m: i64| format!("SELECT reward_status FROM mission_progress WHERE user_id = {u} AND mission_id = {m}");
    assert_eq!(e.one::<String>(&st(31, small)).await, "kyc", "unverified: XP now, cash after KYC");
    assert_eq!(xp::total(&e.st.pool, "kalks", 31).await.unwrap(), 20);
    assert_eq!(e.one::<String>(&st(32, small)).await, "pending", "below the threshold: paid at once");
    assert_eq!(e.one::<String>(&st(32, big)).await, "approval", "at or above: staff");
    assert!(e.m.lock().unwrap().postings.is_empty(), "nothing above the threshold was paid");

    // staff see it in the payouts queue and approve it (engine credit on the live account, idempotent)
    let (s, q) = e.staff(reqwest::Method::GET, "/v1/growth/admin/payouts?status=approval", None).await;
    assert_eq!(s, 200, "{q}");
    let row = q["items"].as_array().unwrap().iter().find(|x| x["source"] == "mission" && x["userId"] == 32).unwrap().clone();
    assert_eq!(row["amount"], 50.0);
    let pid = row["id"].as_i64().unwrap();
    let mut read_only = e.http.post(format!("{}/v1/growth/admin/missions/rewards/{pid}/approve", e.base)).header("x-kalks-internal", TOKEN).header("x-kalks-staff-id", "2").header("x-kalks-staff-role", "marketing");
    read_only = read_only.header("x-kalks-staff-perms", "marketing.read,marketing.write").json(&json!({}));
    assert_eq!(read_only.send().await.unwrap().status().as_u16(), 403, "approve needs marketing.approve");
    let (s, r) = e.staff(reqwest::Method::POST, &format!("/v1/growth/admin/missions/rewards/{pid}/approve"), Some(json!({}))).await;
    assert_eq!((s, r["reward"]["rewardStatus"].as_str()), (200, Some("paid")), "{r}");
    let (s, _) = e.staff(reqwest::Method::POST, &format!("/v1/growth/admin/missions/rewards/{pid}/approve"), Some(json!({}))).await;
    assert_eq!(s, 409, "decided once");
    assert_eq!(e.m.lock().unwrap().postings.iter().filter(|p| p.1 == "credit" && p.2 == dec("50")).count(), 1);

    // the client gets verified: the next step pays the waiting reward
    e.profile(31, "Ben", "Cole", "ben@example.com", "verified").await;
    missions::tick(&e.st).await.unwrap();
    payouts::wallet_tick(&e.st).await.unwrap();
    assert_eq!(e.one::<String>(&st(31, small)).await, "paid");
    let audit: i64 = e.one("SELECT count(*) FROM audit_log WHERE action LIKE 'mission.%'").await;
    assert!(audit >= 5, "completions and payouts are audited");
    e.drop().await;
}

// ---------------------------------------------------------------- XP, levels, streaks

#[tokio::test]
async fn xp_levels_and_streaks() {
    let Some(e) = env().await else { return };
    assert!(xp::award(&e.st, "kalks", 41, 90, "adjust", "a1", "test").await.unwrap());
    assert!(!xp::award(&e.st, "kalks", 41, 90, "adjust", "a1", "test").await.unwrap(), "one booking per ref");
    let levels = xp::levels(&e.st.pool, "kalks").await.unwrap();
    assert_eq!(challenge_calc::level_for(&levels, xp::total(&e.st.pool, "kalks", 41).await.unwrap()).0.level, 1);
    xp::award(&e.st, "kalks", 41, 10, "adjust", "a2", "test").await.unwrap();
    assert_eq!(challenge_calc::level_for(&levels, 100).0.name, "Trader");
    // the ledger is append-only
    assert!(sqlx::query("UPDATE xp_ledger SET xp = 1").execute(&e.st.pool).await.is_err());
    assert!(sqlx::query("DELETE FROM xp_ledger").execute(&e.st.pool).await.is_err());

    // levels are editable but validated
    let (s, _) = e.staff(reqwest::Method::PUT, "/v1/growth/admin/levels", Some(json!({"levels": [{"level": 1, "name": "A", "minXp": 0, "badge": "bronze"}, {"level": 2, "name": "B", "minXp": 0, "badge": "gold"}]}))).await;
    assert_eq!(s, 422);
    let (s, r) = e.staff(reqwest::Method::GET, "/v1/growth/admin/levels", None).await;
    assert_eq!((s, r["levels"].as_array().unwrap().len()), (200, 8));

    // trading streak: weekdays in a row, a missed weekday forgiven once, the 7th day earns XP once
    let start = chrono::NaiveDate::from_ymd_opt(2026, 10, 5).unwrap(); // Monday
    let mut d = start;
    let mut last = 0;
    for _ in 0..9 {
        let s = xp::touch(&e.st, "kalks", 42, "trading", d).await.unwrap();
        last = s.current;
        d += Duration::days(1);
    }
    // Mon 5 .. Tue 13: 9 days in a row
    assert_eq!(last, 9);
    let streak_xp: i64 = e.one("SELECT count(*) FROM xp_ledger WHERE user_id = 42 AND source = 'streak'").await;
    assert_eq!(streak_xp, 1, "7th day");
    // late data never rewinds, the same day never double-counts
    assert_eq!(xp::touch(&e.st, "kalks", 42, "trading", start).await.unwrap().current, 9);
    // login streak over the API: once per day
    let (s, r) = e.client(42, reqwest::Method::GET, "/v1/growth/me/xp", None).await;
    assert_eq!(s, 200, "{r}");
    assert_eq!(r["streaks"]["login"]["current"], 1);
    assert_eq!(r["streaks"]["trading"]["best"], 9);
    let (_, r) = e.client(42, reqwest::Method::GET, "/v1/growth/me/xp", None).await;
    assert_eq!(r["streaks"]["login"]["current"], 1);
    e.drop().await;
}

// ---------------------------------------------------------------- auto-pay

async fn ended_contest(e: &Env, slug: &str, prizes: Value) -> i64 {
    let cid: i64 = sqlx::query_scalar("INSERT INTO contests (tenant, slug, name, kind, starts_at, ends_at, scoring, prizes) VALUES ('kalks',$1,$1,'live', now() - interval '2 hours', now() + interval '1 hour','profit',$2) RETURNING id")
        .bind(slug)
        .bind(sqlx::types::Json(prizes))
        .fetch_one(&e.st.pool)
        .await
        .unwrap();
    for (i, u) in [51, 52, 53].iter().enumerate() {
        let login = 51000000 + u;
        e.add_account(login, *u, "live");
        e.entry(cid, *u, login, &format!("T{u}"), 30 - i as i32).await;
        e.deal(login, *u, "live", "EURUSD", "1", &format!("{}", 300 - i * 100), 600).await;
    }
    contests::refresh(&e.st, cid).await.unwrap();
    sqlx::query("UPDATE contests SET ends_at = now() - interval '1 second' WHERE id = $1").bind(cid).execute(&e.st.pool).await.unwrap();
    cid
}

#[tokio::test]
async fn auto_pay_below_the_threshold_and_staff_above() {
    let Some(e) = env().await else { return };
    let prizes = json!([{"rankFrom": 1, "rankTo": 1, "amount": 300, "payout": "wallet"}, {"rankFrom": 2, "rankTo": 3, "amount": 50, "payout": "wallet"}]);
    // threshold 0 (default): finalize pays nothing, every prize waits for staff as before
    let c0 = ended_contest(&e, "legacy", prizes.clone()).await;
    contests::finalize(&e.st, c0, &system()).await.unwrap();
    payouts::wallet_tick(&e.st).await.unwrap();
    assert!(e.m.lock().unwrap().wallet.is_empty());
    let pending: i64 = e.one(&format!("SELECT count(*) FROM contest_entries WHERE contest_id = {c0} AND prize_status = 'pending'")).await;
    assert_eq!(pending, 3);

    e.settings(json!({"autoPayBelow": 100})).await;
    let c1 = {
        // fresh clients for the second contest
        let cid: i64 = sqlx::query_scalar("INSERT INTO contests (tenant, slug, name, kind, starts_at, ends_at, scoring, prizes) VALUES ('kalks','auto','Auto','live', now() - interval '2 hours', now() + interval '1 hour','profit',$1) RETURNING id")
            .bind(sqlx::types::Json(prizes))
            .fetch_one(&e.st.pool)
            .await
            .unwrap();
        for (i, u) in [61, 62, 63].iter().enumerate() {
            let login = 51000000 + u;
            e.add_account(login, *u, "live");
            e.entry(cid, *u, login, &format!("T{u}"), 30 - i as i32).await;
            e.deal(login, *u, "live", "EURUSD", "1", &format!("{}", 300 - i * 100), 600).await;
        }
        contests::refresh(&e.st, cid).await.unwrap();
        sqlx::query("UPDATE contests SET ends_at = now() - interval '1 second' WHERE id = $1").bind(cid).execute(&e.st.pool).await.unwrap();
        cid
    };
    contests::finalize(&e.st, c1, &system()).await.unwrap();
    payouts::wallet_tick(&e.st).await.unwrap();
    let rows: Vec<(i64, String)> = sqlx::query_as("SELECT user_id, prize_status FROM contest_entries WHERE contest_id = $1 ORDER BY user_id").bind(c1).fetch_all(&e.st.pool).await.unwrap();
    assert_eq!(rows, vec![(61, "approval".into()), (62, "paid".into()), (63, "paid".into())]);
    let status: String = e.one(&format!("SELECT status FROM contests WHERE id = {c1}")).await;
    assert_eq!(status, "finalized", "the $300 prize still waits");
    let autopay: i64 = e.one("SELECT count(*) FROM audit_log WHERE action = 'contest.autopay' AND actor = 'system'").await;
    assert_eq!(autopay, 1);
    // finalize twice is refused; staff pay the rest once
    assert!(contests::finalize(&e.st, c1, &system()).await.is_err());
    let (s, r) = e.staff(reqwest::Method::POST, &format!("/v1/growth/admin/contests/{c1}/pay"), Some(json!({}))).await;
    assert_eq!((s, r["paid"].as_i64()), (200, Some(1)), "{r}");
    e.staff(reqwest::Method::POST, &format!("/v1/growth/admin/contests/{c1}/pay"), Some(json!({}))).await;
    payouts::wallet_tick(&e.st).await.unwrap();
    let status: String = e.one(&format!("SELECT status FROM contests WHERE id = {c1}")).await;
    assert_eq!(status, "paid");
    let w = e.m.lock().unwrap().wallet.clone();
    assert_eq!(w.iter().filter(|x| x.0.starts_with("growth:prize:")).count(), 3, "each prize once");

    // auto-finalize: off by default; on, an ended contest without open flags is finalized by the service
    let c2 = ended_contest(&e, "af", json!([{"rankFrom": 1, "rankTo": 1, "amount": 10, "payout": "wallet"}])).await;
    assert_eq!(contests::auto_finalize_tick(&e.st).await.unwrap(), 0);
    e.settings(json!({"autoFinalize": true, "autoFinalizeDelayMinutes": 0})).await;
    contests::refresh(&e.st, c2).await.unwrap();
    assert_eq!(contests::auto_finalize_tick(&e.st).await.unwrap(), 1);
    let status: String = e.one(&format!("SELECT status FROM contests WHERE id = {c2}")).await;
    assert!(status == "paid" || status == "finalized", "{status}");
    e.drop().await;
}

// ---------------------------------------------------------------- leaderboard privacy

#[tokio::test]
async fn leaderboards_show_nicknames_only_and_respect_opt_out() {
    let Some(e) = env().await else { return };
    let cid: i64 = sqlx::query_scalar("INSERT INTO contests (tenant, slug, name, kind, starts_at, ends_at, scoring) VALUES ('kalks','board-test','Board','live', now() - interval '2 hours', now() + interval '1 hour','profit') RETURNING id")
        .fetch_one(&e.st.pool)
        .await
        .unwrap();
    let people = [(71, "Priya", "Sharma", "priya.sharma@example.com"), (72, "Omar", "Haddad", "omar.h@example.com"), (73, "Lena", "Vogel", "lena@example.com")];
    for (i, (u, f, l, m)) in people.iter().enumerate() {
        e.profile(*u, f, l, m, "verified").await;
        let login = 52000000 + u;
        e.add_account(login, *u, "live");
        e.entry(cid, *u, login, &format!("{f} {}.", &l[..1]), 30 - i as i32).await;
        e.deal(login, *u, "live", "EURUSD", "1", &format!("{}", 100 + i * 50), 600).await;
    }
    contests::refresh(&e.st, cid).await.unwrap();
    xp::award(&e.st, "kalks", 72, 500, "adjust", "x", "test").await.unwrap();

    // 73 opts out of public boards and renames
    let (s, r) = e.client(73, reqwest::Method::POST, "/v1/growth/me/nickname", Some(json!({"nickname": "PipHunter", "public": false}))).await;
    assert_eq!((s, r["nickname"].as_str(), r["public"].as_bool()), (200, Some("PipHunter"), Some(false)), "{r}");
    let (s, r) = e.client(72, reqwest::Method::POST, "/v1/growth/me/nickname", Some(json!({"nickname": "piphunter"}))).await;
    assert_eq!((s, r["error"]["code"].as_str()), (409, Some("nickname_taken")));
    let (s, _) = e.client(72, reqwest::Method::POST, "/v1/growth/me/nickname", Some(json!({"nickname": "omar@example.com"}))).await;
    assert_eq!(s, 422);

    // public: no internal token needed, nicknames only, the opted-out client left out, cacheable
    let res = e.http.get(format!("{}/v1/public/leaderboard?contest=board-test", e.base)).send().await.unwrap();
    assert_eq!(res.status().as_u16(), 200);
    assert!(res.headers()["cache-control"].to_str().unwrap().contains("max-age=15"));
    assert_eq!(res.headers()["access-control-allow-origin"], "*");
    let body = res.text().await.unwrap();
    let v: Value = serde_json::from_str(&body).unwrap();
    let items = v["items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "{body}");
    assert!(items.iter().all(|i| i.as_object().unwrap().keys().all(|k| ["rank", "nickname", "score", "level"].contains(&k.as_str()))), "{body}");
    assert_eq!(items[0]["rank"], 2, "real ranks are kept (rank 1 opted out)");
    assert_eq!(items[0]["level"]["level"], 3, "500 XP = Pro");
    for (u, f, l, m) in people {
        let login = (52000000 + u).to_string();
        for needle in [f, l, m, login.as_str(), "PipHunter", "\"IN\""] {
            assert!(!body.contains(needle), "public board leaks {needle}: {body}");
        }
        assert!(!body.contains(&format!("\"userId\":{u}")));
    }
    // the XP board too
    let body = e.http.get(format!("{}/v1/public/leaderboard?period=all", e.base)).send().await.unwrap().text().await.unwrap();
    assert!(!body.contains("Omar") && !body.contains("omar.h") && body.contains("\"nickname\""), "{body}");
    // the broker can switch the public API off
    e.settings(json!({"publicLeaderboards": false})).await;
    let s = e.http.get(format!("{}/v1/public/leaderboard?period=week", e.base)).send().await.unwrap().status().as_u16();
    assert_eq!(s, 404);
    // every other route still needs the internal token
    let s = e.http.get(format!("{}/v1/growth/me/hub", e.base)).header("x-kalks-user-id", "71").send().await.unwrap().status().as_u16();
    assert_eq!(s, 403);

    // in the Client Area: nicknames for everyone else, the own row marked, no other client's login or country
    let (s, r) = e.client(71, reqwest::Method::GET, "/v1/growth/me/leaderboard?contest=board-test", None).await;
    assert_eq!(s, 200, "{r}");
    assert_eq!(r["items"].as_array().unwrap().len(), 3, "in-app boards list everyone by nickname");
    assert_eq!(r["me"]["me"], true);
    let (_, d) = e.client(71, reqwest::Method::GET, &format!("/v1/growth/me/contests/{cid}"), None).await;
    let board = d["leaderboard"].as_array().unwrap();
    let other = board.iter().find(|s| s["me"] == false).unwrap();
    assert!(other["login"].is_null() && other["userId"].is_null() && other["country"].is_null());
    assert!(!d.to_string().contains("Omar") && !d.to_string().contains("Lena"), "{d}");
    assert!(board.iter().any(|s| s["name"] == "PipHunter"));
    // staff still see real names
    let (_, a) = e.staff(reqwest::Method::GET, &format!("/v1/growth/admin/contests/{cid}"), None).await;
    assert!(a.to_string().contains("Omar H."));
    // Kalks Trader's rank widget for the entered account
    let (s, r) = e.client(72, reqwest::Method::GET, &format!("/v1/growth/me/rank?login={}", 52000072), None).await;
    assert_eq!((s, r["contests"][0]["rank"].as_i64()), (200, Some(2)), "{r}");
    e.drop().await;
}

#[tokio::test]
async fn seeded_one_off_missions_complete_from_profile_facts() {
    let Some(e) = env().await else { return };
    e.profile(81, "Dev", "Iyer", "dev@example.com", "verified").await;
    sqlx::query("UPDATE profiles SET first_deposit_at = now() WHERE user_id = 81").execute(&e.st.pool).await.unwrap();
    // a referred friend of 81 deposits: a one-off referral mission counts it
    e.profile(82, "Eva", "Jung", "eva@example.com", "verified").await;
    let (s, r) = e.staff(reqwest::Method::POST, "/v1/growth/admin/missions", Some(json!({"name": "Bring a friend", "kind": "referral", "schedule": "once", "target": 1, "xp": 200}))).await;
    assert_eq!(s, 200, "{r}");
    sqlx::query("UPDATE profiles SET referred_by = 81, first_deposit_at = now() + interval '1 second' WHERE user_id = 82").execute(&e.st.pool).await.unwrap();
    assert_eq!(missions::evaluate_user(&e.st, "kalks", 81).await.unwrap(), 3, "KYC, first deposit, referral");
    assert_eq!(xp::total(&e.st.pool, "kalks", 81).await.unwrap(), 100 + 150 + 200);
    assert_eq!(missions::evaluate_user(&e.st, "kalks", 81).await.unwrap(), 0, "one-off: never again");
    let (_, hub) = e.client(81, reqwest::Method::GET, "/v1/growth/me/missions", None).await;
    let done = hub["items"].as_array().unwrap().iter().filter(|m| m["done"] == true).count();
    assert_eq!(done, 3, "{hub}");
    e.drop().await;
}
