//! Integration test against PostgreSQL: ingests fixture feeds and a calendar export through the real store
//! code, then drives the HTTP API end to end — feed filters, cross-source de-duplication, per-tenant pin /
//! hide / retag, the world map, calendar ranges and server-time display across the November DST change,
//! event history, reminders, staff actuals / impact and the Back Office guards.
//!
//! Needs the local Postgres (127.0.0.1:5433). Uses a throw-away database `kalks_news_test_<pid>`; skipped
//! with a message when Postgres is not reachable. Override with NEWS_TEST_DATABASE_URL (a server URL).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::{Duration, Utc};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::ConnectOptions;
use sqlx::postgres::PgConnectOptions;
use std::str::FromStr;
use tower::ServiceExt;

use news::api::router;
use news::calendar::CalEvent;
use news::config::Config;
use news::{AppState, feed, store};

async fn test_db() -> Option<String> {
    let server = std::env::var("NEWS_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into());
    let opts = PgConnectOptions::from_str(&server).ok()?;
    let db = format!("kalks_news_test_{}", std::process::id());
    let mut conn = match opts.clone().database("postgres").connect().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("skipping: Postgres not reachable ({e})");
            return None;
        }
    };
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{db}\""))).execute(&mut conn).await;
    let base = server.rsplit_once('/').map(|x| x.0).unwrap_or(&server).to_string();
    Some(format!("{base}/{db}"))
}

async fn drop_db(url: &str) {
    let opts = PgConnectOptions::from_str(url).unwrap();
    let db = opts.get_database().unwrap().to_string();
    if let Ok(mut c) = opts.database("postgres").connect().await {
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{db}\" WITH (FORCE)"))).execute(&mut c).await;
    }
}

struct T {
    app: axum::Router,
}

impl T {
    async fn call(&self, method: &str, path: &str, headers: &[(&str, &str)], body: Option<Value>) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method).uri(path).header("x-kalks-internal", "test-token");
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let req = match body {
            Some(b) => req.header("content-type", "application/json").body(Body::from(b.to_string())).unwrap(),
            None => req.body(Body::empty()).unwrap(),
        };
        let res = self.app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }
    async fn get(&self, path: &str) -> Value {
        let (s, v) = self.call("GET", path, &[], None).await;
        assert_eq!(s, StatusCode::OK, "{path}: {v}");
        v
    }
}

fn rss(items: &[(&str, &str, &str, chrono::DateTime<Utc>)]) -> String {
    let body: String = items
        .iter()
        .map(|(t, d, l, at)| format!("<item><title>{t}</title><link>{l}</link><description>{d}</description><pubDate>{}</pubDate></item>", at.to_rfc2822()))
        .collect();
    format!("<rss version=\"2.0\"><channel><title>x</title>{body}</channel></rss>")
}

fn titles(v: &Value, key: &str) -> Vec<String> {
    v[key].as_array().unwrap().iter().map(|i| i["title"].as_str().unwrap().to_string()).collect()
}

#[tokio::test]
async fn news_and_calendar_end_to_end() {
    let Some(url) = test_db().await else { return };
    let pool = store::connect(&url).await.unwrap();
    store::seed_sources(&pool).await.unwrap();
    // FXStreet ships disabled; enable it for the fixture so a commercial-style source is covered
    sqlx::query("UPDATE sources SET enabled = true WHERE id = 'fxstreet'").execute(&pool).await.unwrap();
    let now = Utc::now();

    // --- ingestion: tagging, stale items, duplicates within and across sources ---
    let fed = feed::parse(&rss(&[
        ("Federal Reserve issues FOMC statement", "The Committee decided to maintain the target range for the federal funds rate.", "https://www.federalreserve.gov/newsevents/pressreleases/monetary1.htm", now - Duration::hours(3)),
        ("Federal Reserve Board announces approval of application by Example Bancorp", "", "https://www.federalreserve.gov/newsevents/pressreleases/orders1.htm", now - Duration::hours(4)),
        ("Old release", "", "https://www.federalreserve.gov/old.htm", now - Duration::days(40)),
    ]));
    let r = store::ingest(&pool, "fed", &fed, &[], &[]).await.unwrap();
    assert_eq!((r.inserted, r.stale), (2, 1));
    let again = store::ingest(&pool, "fed", &fed, &[], &[]).await.unwrap();
    assert_eq!((again.inserted, again.duplicates), (0, 2), "re-polling the same feed stores nothing new");

    let fx = feed::parse(&rss(&[
        ("Gold climbs to a record high as the dollar slides", "XAU/USD extends gains.", "https://www.fxstreet.com/news/gold-1?utm_source=rss", now - Duration::hours(1)),
        ("Bitcoin tumbles below $60,000 as crypto selloff deepens", "", "https://www.fxstreet.com/news/btc-1", now - Duration::hours(2)),
        ("FEDERAL RESERVE ISSUES FOMC STATEMENT", "wire copy of the same headline", "https://www.fxstreet.com/news/fomc-copy", now - Duration::hours(2)),
        ("Sterling slumps as UK retail sales miss forecasts", "", "https://www.fxstreet.com/news/gbp-1", now - Duration::hours(5)),
    ]));
    let r = store::ingest(&pool, "fxstreet", &fx, &[], &[]).await.unwrap();
    assert_eq!((r.inserted, r.duplicates), (3, 1), "same headline from another source within 48 h is a duplicate");
    let same_link = feed::parse(&rss(&[("Gold at record (updated headline)", "", "http://fxstreet.com/news/gold-1/", now)]));
    assert_eq!(store::ingest(&pool, "fxstreet", &same_link, &[], &[]).await.unwrap().inserted, 0, "same canonical link");

    let st = AppState::new(pool.clone(), Config::for_tests(&url));
    let t = T { app: router(st) };

    // auth
    let res = t.app.clone().oneshot(Request::builder().uri("/v1/news").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(t.app.clone().oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap()).await.unwrap().status(), StatusCode::OK);

    // --- feed: newest first, administrative noise below the default importance floor ---
    let v = t.get("/v1/news").await;
    let list = titles(&v, "items");
    assert_eq!(list[0], "Gold climbs to a record high as the dollar slides");
    assert!(list.contains(&"Federal Reserve issues FOMC statement".to_string()));
    assert!(!list.iter().any(|x| x.contains("approval of application")), "{list:?}");
    assert_eq!(t.get("/v1/news?minImportance=0").await["items"].as_array().unwrap().len(), 5);
    let gold = &v["items"][0];
    assert_eq!(gold["symbols"][0], "XAUUSD");
    assert_eq!(gold["sentiment"], "bullish");
    assert_eq!(gold["source"]["name"], "FXStreet");
    // filters
    assert_eq!(titles(&t.get("/v1/news?symbol=btcusd").await, "items"), vec!["Bitcoin tumbles below $60,000 as crypto selloff deepens"]);
    assert_eq!(titles(&t.get("/v1/news?currency=GBP").await, "items"), vec!["Sterling slumps as UK retail sales miss forecasts"]);
    assert_eq!(titles(&t.get("/v1/news?country=us&category=macro").await, "items"), vec!["Federal Reserve issues FOMC statement"]);
    assert_eq!(titles(&t.get("/v1/news?sentiment=bearish").await, "items").len(), 2);
    assert_eq!(titles(&t.get("/v1/news?q=record").await, "items").len(), 1);
    // paging
    let p1 = t.get("/v1/news?limit=2").await;
    assert_eq!(p1["items"].as_array().unwrap().len(), 2);
    let next = p1["next"].as_str().unwrap().to_string();
    let p2 = t.get(&format!("/v1/news?limit=2&before={}", urlencode(&next))).await;
    assert_eq!(p2["items"].as_array().unwrap().len(), 2);
    assert_ne!(p1["items"][1]["id"], p2["items"][0]["id"]);

    // --- Back Office: pin / hide / retag are per tenant ---
    let staff = [("x-kalks-staff", "editor@kalkstrade.com")];
    let other = [("x-kalks-staff", "ops@acme.com"), ("x-kalks-tenant", "acme")];
    let fomc_id = v["items"].as_array().unwrap().iter().find(|i| i["title"] == "Federal Reserve issues FOMC statement").unwrap()["id"].as_i64().unwrap();
    let btc_id = v["items"].as_array().unwrap().iter().find(|i| i["symbols"][0] == "BTCUSD").unwrap()["id"].as_i64().unwrap();
    let (s, _) = t.call("PUT", &format!("/v1/admin/news/{fomc_id}"), &[], Some(json!({"pinned": true}))).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "staff header required");
    let (s, e) = t.call("PUT", &format!("/v1/admin/news/{fomc_id}"), &staff, Some(json!({"pinned": true}))).await;
    assert_eq!(s, StatusCode::OK, "{e}");
    assert_eq!(e["item"]["pinned"], true);
    let (s, _) = t.call("PUT", &format!("/v1/admin/news/{btc_id}"), &staff, Some(json!({"hidden": true}))).await;
    assert_eq!(s, StatusCode::OK);
    let v = t.get("/v1/news").await;
    assert_eq!(titles(&v, "pinned"), vec!["Federal Reserve issues FOMC statement"]);
    assert!(!titles(&v, "items").iter().any(|x| x.contains("FOMC") || x.contains("Bitcoin")), "pinned leads, hidden gone");
    let (s, _) = t.call("GET", &format!("/v1/news/{btc_id}"), &[], None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // another tenant sees the automatic feed
    let (_, acme) = t.call("GET", "/v1/news", &[("x-kalks-tenant", "acme")], None).await;
    assert!(acme["pinned"].as_array().unwrap().is_empty());
    assert!(titles(&acme, "items").iter().any(|x| x.contains("Bitcoin")));
    // retag: symbols + countries drive currencies; unknown values rejected
    let (s, e) = t.call("PUT", &format!("/v1/admin/news/{fomc_id}"), &staff, Some(json!({"symbols": ["XAUUSD", "NOPE"]}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{e}");
    let (s, e) = t.call("PUT", &format!("/v1/admin/news/{fomc_id}"), &staff, Some(json!({"symbols": ["XAUUSD", "USDJPY"], "countries": ["us", "jp"], "importance": 95}))).await;
    assert_eq!(s, StatusCode::OK, "{e}");
    assert_eq!(e["item"]["symbols"], json!(["XAUUSD", "USDJPY"]));
    assert_eq!(e["item"]["currencies"], json!(["JPY", "USD"]));
    assert_eq!(e["item"]["pinned"], true, "retag keeps the pin");
    assert_eq!(e["item"]["auto"]["symbols"][0], "EURUSD");
    assert_eq!(titles(&t.get("/v1/news?currency=JPY").await, "pinned"), vec!["Federal Reserve issues FOMC statement"]);
    let (s, _) = t.call("PUT", &format!("/v1/admin/news/{fomc_id}"), &staff, Some(json!({"hidden": true}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "a pinned story can't also be hidden");
    let (_, adm) = t.call("GET", "/v1/admin/news?status=hidden", &staff, None).await;
    assert_eq!(adm["total"], 1);
    let (s, e) = t.call("DELETE", &format!("/v1/admin/news/{btc_id}"), &staff, None).await;
    assert_eq!(s, StatusCode::OK, "{e}");
    assert_eq!(e["item"]["hidden"], false);

    // disabling a source removes its stories from every public list (platform tenant only)
    let (s, _) = t.call("PUT", "/v1/admin/sources/fxstreet", &other, Some(json!({"enabled": false}))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, e) = t.call("PUT", "/v1/admin/sources/fxstreet", &staff, Some(json!({"enabled": false}))).await;
    assert_eq!(s, StatusCode::OK, "{e}");
    assert!(titles(&t.get("/v1/news").await, "items").iter().all(|x| !x.contains("Gold")));
    t.call("PUT", "/v1/admin/sources/fxstreet", &staff, Some(json!({"enabled": true}))).await;

    // the brief and the feeds / calendar are shared by every broker: only the platform tenant regenerates or fetches
    for path in ["/v1/admin/brief", "/v1/admin/sources/fed/refresh", "/v1/admin/calendar/refresh"] {
        let (s, e) = t.call("POST", path, &other, Some(json!({}))).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{path}: {e}");
        assert_eq!(e["error"]["code"], "forbidden", "{path}");
    }
    let (s, e) = t.call("POST", "/v1/admin/brief", &staff, Some(json!({}))).await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE, "the platform passes the tenant check (no AI key in tests): {e}");
    let (_, au) = t.call("GET", "/v1/admin/audit", &other, None).await;
    assert!(au["entries"].as_array().unwrap().iter().all(|x| x["action"] != "source.refresh" && x["action"] != "calendar.refresh"), "{au}");

    // --- world map ---
    let m = t.get("/v1/news/map").await;
    let us = m["countries"].as_array().unwrap().iter().find(|c| c["country"] == "us").unwrap().clone();
    assert!(us["count"].as_i64().unwrap() >= 2, "{m}");
    assert_eq!(us["name"], "United States");
    let gb = m["countries"].as_array().unwrap().iter().find(|c| c["country"] == "gb").unwrap().clone();
    assert_eq!(gb["sentiment"], -1.0);
    assert!(m["mentions"].as_array().unwrap().iter().any(|x| x["symbol"] == "XAUUSD"));

    // --- calendar: the week of the November DST change ---
    let ev = |title: &str, ccy: &str, at: &str, impact: i16, f: &str, p: &str, a: &str| CalEvent {
        ext_key: format!("{ccy}-{title}-{at}"),
        title: title.into(),
        currency: ccy.into(),
        country: news::tagging::country_of_currency(ccy).into(),
        starts_at: chrono::DateTime::parse_from_rfc3339(at).unwrap().with_timezone(&Utc),
        all_day: false,
        impact,
        forecast: f.into(),
        previous: p.into(),
        actual: a.into(),
        symbols: news::calendar::event_symbols(ccy, title),
        url: String::new(),
    };
    let batch = vec![
        ev("Non-Farm Employment Change", "USD", "2026-10-02T08:30:00-04:00", 3, "150K", "142K", "254K"),
        ev("Non-Farm Employment Change", "USD", "2026-11-06T08:30:00-05:00", 3, "160K", "254K", ""),
        ev("Unemployment Rate", "USD", "2026-11-06T08:30:00-05:00", 3, "4.1%", "4.1%", ""),
        ev("Cash Rate", "AUD", "2026-11-03T03:30:00Z", 3, "4.60%", "4.60%", ""),
        ev("German Prelim CPI m/m", "EUR", "2026-11-02T13:00:00Z", 1, "0.2%", "0.1%", ""),
    ];
    let up = store::upsert_calendar(&pool, "forexfactory", &batch).await.unwrap();
    assert_eq!((up.inserted, up.updated, up.actuals.len()), (5, 0, 1));
    let up = store::upsert_calendar(&pool, "forexfactory", &batch).await.unwrap();
    assert_eq!((up.inserted, up.updated, up.actuals.len()), (0, 5, 0), "idempotent");

    let c = t.get("/v1/calendar?from=2026-11-02&to=2026-11-09").await;
    let evs = c["events"].as_array().unwrap();
    assert_eq!(evs.len(), 4);
    let nfp = evs.iter().find(|e| e["title"] == "Non-Farm Employment Change").unwrap();
    assert_eq!(nfp["serverTime"], "15:30", "08:30 EST = 13:30 UTC = 15:30 GMT+2");
    assert_eq!(nfp["serverDate"], "2026-11-06");
    assert_eq!(nfp["symbols"], json!(["EURUSD", "USDJPY", "XAUUSD", "US30"]));
    let oct = t.get("/v1/calendar?from=2026-09-28&to=2026-10-05").await;
    let nfp_oct = &oct["events"][0];
    assert_eq!(nfp_oct["serverTime"], "15:30", "08:30 EDT = 12:30 UTC = 15:30 GMT+3");
    assert_eq!(nfp_oct["surprise"], 1);
    assert_eq!(t.get("/v1/calendar?from=2026-11-02&to=2026-11-09&impact=1").await["events"].as_array().unwrap().len(), 1);
    assert_eq!(t.get("/v1/calendar?from=2026-11-02&to=2026-11-09&currency=aud,eur").await["events"].as_array().unwrap().len(), 2);
    let (s, _) = t.call("GET", "/v1/calendar?from=2026-01-01&to=2026-06-01", &[], None).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);

    // history: the October release shows up behind the November one
    let nfp_id = nfp["id"].as_i64().unwrap();
    let d = t.get(&format!("/v1/calendar/{nfp_id}")).await;
    assert_eq!(d["history"].as_array().unwrap().len(), 1);
    assert_eq!(d["history"][0]["actual"], "254K");

    // staff actual + impact override (platform tenant only); feed actuals later win again
    let (s, _) = t.call("PUT", &format!("/v1/admin/calendar/{nfp_id}"), &other, Some(json!({"actual": "180K"}))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = t.call("PUT", &format!("/v1/admin/calendar/{nfp_id}"), &staff, Some(json!({"actual": "<script>"}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    let (s, e) = t.call("PUT", &format!("/v1/admin/calendar/{nfp_id}"), &staff, Some(json!({"actual": "180K", "impact": 2}))).await;
    assert_eq!(s, StatusCode::OK, "{e}");
    assert_eq!(e["event"]["actual"], "180K");
    assert_eq!(e["event"]["actualSource"], "staff");
    assert_eq!(e["event"]["impact"], 2);
    assert_eq!(e["event"]["feedImpact"], 3);
    assert_eq!(e["event"]["surprise"], 1);
    let mut b2 = batch.clone();
    b2[1].actual = "175K".into();
    let up = store::upsert_calendar(&pool, "forexfactory", &b2).await.unwrap();
    assert_eq!(up.actuals, vec![nfp_id]);
    assert_eq!(t.get(&format!("/v1/calendar/{nfp_id}")).await["event"]["actualSource"], "feed");

    // --- reminders ---
    let future = store::upsert_calendar(&pool, "forexfactory", &[ev("CPI m/m", "USD", &(now + Duration::hours(2)).to_rfc3339(), 3, "0.3%", "0.2%", "")]).await.unwrap();
    assert_eq!(future.inserted, 1);
    let next = t.get("/v1/calendar/next").await;
    assert_eq!(next["event"]["title"], "CPI m/m");
    let cpi_id = next["event"]["id"].as_i64().unwrap();
    let user = [("x-kalks-user-id", "42")];
    let (s, _) = t.call("POST", "/v1/me/calendar/reminders", &[], Some(json!({"eventId": cpi_id}))).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, e) = t.call("POST", "/v1/me/calendar/reminders", &user, Some(json!({"eventId": cpi_id, "minutes": 7}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{e}");
    let (s, e) = t.call("POST", "/v1/me/calendar/reminders", &user, Some(json!({"eventId": cpi_id, "minutes": 30}))).await;
    assert_eq!(s, StatusCode::OK, "{e}");
    store::upsert_calendar(&pool, "forexfactory", &[ev("Retail Sales m/m", "USD", &(now - Duration::hours(1)).to_rfc3339(), 3, "", "", "")]).await.unwrap();
    let past: i64 = sqlx::query_scalar("SELECT id FROM calendar_events WHERE title = 'Retail Sales m/m'").fetch_one(&pool).await.unwrap();
    let (s, _) = t.call("POST", "/v1/me/calendar/reminders", &user, Some(json!({"eventId": past}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "past events can't be reminded");
    let (s, e) = t.call("PUT", "/v1/me/calendar/alerts", &user, Some(json!({"highImpact": true, "currencies": ["usd", "x"]}))).await;
    assert_eq!(s, StatusCode::OK, "{e}");
    assert_eq!(e["alerts"]["currencies"], json!(["USD"]));
    let (_, me) = t.call("GET", "/v1/me/calendar", &user, None).await;
    assert_eq!(me["reminders"], json!([cpi_id]));
    assert_eq!(me["alerts"]["minutes"], 15);
    let (_, me2) = t.call("GET", "/v1/me/calendar", &[("x-kalks-user-id", "42"), ("x-kalks-tenant", "acme")], None).await;
    assert_eq!(me2["reminders"], json!([]), "reminders are per tenant");
    t.call("DELETE", &format!("/v1/me/calendar/reminders/{cpi_id}"), &user, None).await;
    assert_eq!(t.call("GET", "/v1/me/calendar", &user, None).await.1["reminders"], json!([]));

    // --- brief, stats, audit ---
    let b = t.get("/v1/brief").await;
    assert_eq!(b["brief"], Value::Null);
    assert_eq!(b["configured"], false);
    let (s, _) = t.call("POST", "/v1/admin/brief", &staff, Some(json!({}))).await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    let (_, stats) = t.call("GET", "/v1/admin/stats", &staff, None).await;
    assert_eq!(stats["news"]["pinned"], 1);
    let (_, audit) = t.call("GET", "/v1/admin/audit", &staff, None).await;
    let actions: Vec<&str> = audit["entries"].as_array().unwrap().iter().map(|e| e["action"].as_str().unwrap()).collect();
    for a in ["news.pin", "news.hide", "news.retag", "news.reset", "source.disable", "calendar.edit"] {
        assert!(actions.contains(&a), "{a} missing from {actions:?}");
    }
    let (_, acme_audit) = t.call("GET", "/v1/admin/audit", &other, None).await;
    assert!(acme_audit["entries"].as_array().unwrap().is_empty());

    pool.close().await;
    drop_db(&url).await;
}

fn urlencode(s: &str) -> String {
    s.replace('+', "%2B").replace(':', "%3A")
}

/// Reminders and high-impact alerts go to the notifications service once, and are retried while it is down.
#[tokio::test]
async fn calendar_reminders_are_delivered_once() {
    use std::sync::{Arc, Mutex};
    let Some(url) = test_db().await else { return };
    let url = format!("{url}_rem");
    let pool = store::connect(&url).await.unwrap();
    let now = Utc::now();
    let mk = |key: &str, title: &str, mins: i64, impact: i16| CalEvent {
        ext_key: key.into(),
        title: title.into(),
        currency: "USD".into(),
        country: "us".into(),
        starts_at: now + Duration::minutes(mins),
        all_day: false,
        impact,
        forecast: "0.3%".into(),
        previous: "0.2%".into(),
        actual: String::new(),
        symbols: vec!["EURUSD".into()],
        url: String::new(),
    };
    store::upsert_calendar(&pool, "forexfactory", &[mk("a", "CPI m/m", 10, 3), mk("b", "Building Permits", 40, 1)]).await.unwrap();
    let cpi: i64 = sqlx::query_scalar("SELECT id FROM calendar_events WHERE ext_key = 'a'").fetch_one(&pool).await.unwrap();
    let permits: i64 = sqlx::query_scalar("SELECT id FROM calendar_events WHERE ext_key = 'b'").fetch_one(&pool).await.unwrap();
    for (u, ev, m) in [(1_i64, cpi, 15), (2, cpi, 15), (3, permits, 15)] {
        sqlx::query("INSERT INTO calendar_reminders (tenant, user_id, event_id, minutes) VALUES ('kalks', $1, $2, $3)").bind(u).bind(ev).bind(m).execute(&pool).await.unwrap();
    }
    sqlx::query("INSERT INTO calendar_alerts (tenant, user_id, high_impact, minutes) VALUES ('kalks', 9, true, 15)").execute(&pool).await.unwrap();

    let mut cfg = Config::for_tests(&url);
    cfg.support_url = "http://127.0.0.1:9".into(); // nothing listens: delivery fails
    let st = AppState::new(pool.clone(), cfg.clone());
    assert_eq!(news::workers::send_reminders(&st).await.unwrap(), 0);
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM calendar_reminders WHERE sent_at IS NULL").fetch_one(&pool).await.unwrap();
    assert_eq!(pending, 3, "kept for retry while the notifications service is down");

    let seen: Arc<Mutex<Vec<(String, Value)>>> = Arc::default();
    let s2 = seen.clone();
    let mock = axum::Router::new().route(
        "/v1/notify",
        axum::routing::post(move |h: axum::http::HeaderMap, axum::Json(b): axum::Json<Value>| {
            let s2 = s2.clone();
            async move {
                s2.lock().unwrap().push((h.get("x-kalks-service").and_then(|v| v.to_str().ok()).unwrap_or("").to_string(), b));
                axum::Json(json!({"results": []}))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, mock).await.unwrap() });
    cfg.support_url = format!("http://{addr}");
    let st = AppState::new(pool.clone(), cfg);
    assert_eq!(news::workers::send_reminders(&st).await.unwrap(), 3, "two reminders + one high-impact subscriber");
    {
        let calls = seen.lock().unwrap();
        assert_eq!(calls.len(), 2);
        let rem = calls.iter().find(|(_, b)| b["dedupeKey"] == format!("cal:{cpi}:15")).unwrap();
        assert_eq!(rem.0, "news");
        assert_eq!(rem.1["type"], "calendar.reminder");
        assert_eq!(rem.1["userIds"], json!([1, 2]));
        assert_eq!(rem.1["email"], false);
        assert_eq!(rem.1["title"], "USD CPI m/m in 10 min");
        assert!(rem.1["body"].as_str().unwrap().starts_with("High impact · "));
        let sub = calls.iter().find(|(_, b)| b["dedupeKey"] == format!("calsub:{cpi}")).unwrap();
        assert_eq!(sub.1["userIds"], json!([9]));
    }
    assert_eq!(news::workers::send_reminders(&st).await.unwrap(), 0, "never twice");
    // the low-impact event isn't due yet (40 min away, 15 min reminder) and isn't a high-impact alert
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM calendar_reminders WHERE sent_at IS NULL").fetch_one(&pool).await.unwrap();
    assert_eq!(pending, 1);
    pool.close().await;
    drop_db(&url).await;
}
