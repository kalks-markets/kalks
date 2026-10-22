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
