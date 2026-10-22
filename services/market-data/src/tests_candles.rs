//! Candle correctness end to end: what live ticks build, what is stored, and what `/v1/candles` serves must be the
//! same bars. The database tests run against PostgreSQL :5433 (a throw-away database per test; they fail, never
//! skip, when it is not reachable), like tests_catalogue.rs.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::str::FromStr;

use crate::db;
use crate::demand::{Demand, Limits};
use crate::instruments::Catalogue;
use crate::state::Market;

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config");

fn repo() -> Catalogue {
    Catalogue::load(&format!("{ROOT}/instruments.json"), &format!("{ROOT}/holidays")).unwrap()
}

fn t(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

fn ms(s: &str) -> i64 {
    t(s).timestamp_millis()
}

fn base_url() -> String {
    std::env::var("MARKET_DATA_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into())
}

/// A fresh database on :5433; drop it with [`drop_db`].
async fn fresh_db(tag: &str) -> (PgPool, String) {
    let name = format!("kalks_md_candles_{tag}_{}", std::process::id());
    let server = sqlx::postgres::PgConnectOptions::from_str(&base_url()).unwrap();
    let url = sqlx::ConnectOptions::to_url_lossy(&server.database(&name)).to_string();
    (db::connect(&url).await.expect("PostgreSQL :5433 must be running for this test"), name)
}

async fn drop_db(pool: PgPool, name: &str) {
    pool.close().await;
    let server = sqlx::postgres::PgConnectOptions::from_str(&base_url()).unwrap();
    let mut admin = sqlx::ConnectOptions::connect(&server.database("postgres")).await.unwrap();
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)"))).execute(&mut admin).await;
}

fn market(pool: PgPool) -> std::sync::Arc<Market> {
    Market::new(repo(), pool, crate::spreads::Spreads::default(), false, Demand::new(Vec::<String>::new(), Limits::default()))
}

/// The last ticks of a minute and the first tick of the next one arrive between two 1-second flushes (an active
/// market: every minute). The stored bar of the minute that closed must end at its last tick, with its full range.
#[tokio::test]
async fn a_bar_closing_between_two_flushes_is_stored_with_its_last_tick() {
    let (pool, name) = fresh_db("flush").await;
    let mk = market(pool.clone());
    // BTCUSD trades around the clock; 2026-10-21 is a Wednesday
    mk.on_trade("BTCUSD", 100.0, 1.0, ms("2026-10-21T10:00:05Z"));
    mk.flush().await.unwrap();
    mk.on_trade("BTCUSD", 104.0, 1.0, ms("2026-10-21T10:00:59.200Z"));
    mk.on_trade("BTCUSD", 98.0, 1.0, ms("2026-10-21T10:00:59.600Z"));
    mk.on_trade("BTCUSD", 101.0, 1.0, ms("2026-10-21T10:00:59.900Z"));
    // the next minute starts before the next flush
    mk.on_trade("BTCUSD", 102.0, 1.0, ms("2026-10-21T10:01:00.100Z"));
    mk.flush().await.unwrap();

    let m1 = db::load_bars(&pool, "BTCUSD", 1, None, None, 10).await.unwrap();
    assert_eq!(m1.len(), 2);
    let closed = m1[0];
    assert_eq!(closed.t, t("2026-10-21T10:00:00Z"));
    assert_eq!((closed.o, closed.h, closed.l, closed.c), (100.0, 104.0, 98.0, 101.0), "the closed minute keeps its last ticks");
    let next = m1[1];
    assert_eq!((next.t, next.o, next.c), (t("2026-10-21T10:01:00Z"), 102.0, 102.0));
    // M5 (still forming) carries every tick
    let m5 = db::load_bars(&pool, "BTCUSD", 5, None, None, 10).await.unwrap();
    assert_eq!(m5.len(), 1);
    assert_eq!((m5[0].o, m5[0].h, m5[0].l, m5[0].c), (100.0, 104.0, 98.0, 102.0));
    drop_db(pool, &name).await;
}

/// `GET /v1/candles` (latest page) as the app reads it: `[t, o, h, l, c]` per bar.
async fn latest_page(mk: &std::sync::Arc<Market>, symbol: &str, tf: &str) -> Vec<(i64, f64, f64, f64, f64)> {
    use axum::extract::{Query, State};
    use axum::response::IntoResponse;
    let st = crate::api::AppState { market: mk.clone(), admin_token: String::new() };
    let q = crate::api::CandlesQ { symbol: symbol.into(), tf: tf.into(), limit: Some(50), to: None };
    let res = match crate::api::candles(State(st), Query(q)).await {
        Ok(r) => r,
        Err(e) => e.into_response(),
    };
    let body = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let f = |x: &serde_json::Value| x.as_f64().unwrap();
    v["bars"].as_array().unwrap().iter().map(|b| (b["t"].as_i64().unwrap(), f(&b["o"]), f(&b["h"]), f(&b["l"]), f(&b["c"]))).collect()
}

/// A quiet market: the live builder's last M1 bar closed minutes ago and the reconciler replaced the stored bar with
/// the provider's final values. The latest page must serve the stored (final) bar, not mix the stale live bar into it;
/// a bar still forming is merged as before.
#[tokio::test]
async fn the_latest_page_serves_a_closed_bar_as_stored() {
    use crate::db::{Bar, Source};
    use crate::timeframes::Tf;
    let (pool, name) = fresh_db("page").await;
    let mk = market(pool.clone());
    let now = Utc::now();
    let closed = Tf::M1.bucket(now) - chrono::Duration::minutes(10);
    let fin = Bar { t: closed, o: 1.10000, h: 1.10200, l: 1.09900, c: 1.10150, v: 9.0 };
    db::upsert_bars(&pool, &[("EURUSD".into(), 1, fin)], Source::Reconciled).await.unwrap();
    // the live builder saw a late spike and a different last tick for that minute, and nothing since
    mk.seed_forming("EURUSD", Tf::M1, Bar { t: closed, o: 1.10000, h: 1.10350, l: 1.09900, c: 1.10080, v: 7.0 });
    let page = latest_page(&mk, "EURUSD", "M1").await;
    assert_eq!(page.len(), 1);
    assert_eq!(page[0], (closed.timestamp(), 1.10000, 1.10200, 1.09900, 1.10150), "the final stored bar, untouched");

    // the bar still forming: the live builder's newest values are merged into the stored row
    let cur = Tf::M5.bucket(now);
    db::upsert_bars(&pool, &[("EURUSD".into(), 5, Bar { t: cur, o: 1.1, h: 1.101, l: 1.099, c: 1.1005, v: 1.0 })], Source::Live).await.unwrap();
    mk.seed_forming("EURUSD", Tf::M5, Bar { t: cur, o: 1.1, h: 1.102, l: 1.099, c: 1.1015, v: 2.0 });
    let page = latest_page(&mk, "EURUSD", "M5").await;
    assert_eq!(page.last().copied(), Some((cur.timestamp(), 1.1, 1.102, 1.099, 1.1015)));
    drop_db(pool, &name).await;
}
