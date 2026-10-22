//! Spread markups per account group (set from the Back Office), applied only to outgoing client quotes.

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::instruments::Instrument;
use crate::state::Quote;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Markup {
    pub group_code: String,
    /// symbol or "*" for the group default
    pub symbol: String,
    pub markup_points: i32,
    pub min_spread_points: i32,
}

impl Markup {
    /// Shape checks before a markup is stored. Which spread groups a broker may edit is decided by the Back Office
    /// BFF (the spread groups its account groups use in the trading engine); here any well-formed spread group code
    /// is accepted, so a white-label broker's own groups (`<broker>-standard`, custom ones) can be priced too.
    pub fn check(&self) -> Result<(), &'static str> {
        let g = &self.group_code;
        if g.is_empty() || g.len() > 64 || !g.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit()) || !g.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_') {
            return Err("unknown spread group: use the account group's spread group code (lower-case letters, digits, - and _)");
        }
        if g == "raw" {
            return Err("the raw spread group is the unmarked feed and has no markup");
        }
        if !(0..=100_000).contains(&self.markup_points) || !(0..=100_000).contains(&self.min_spread_points) {
            return Err("markup and minimum spread must be whole points from 0 to 100,000");
        }
        Ok(())
    }
}

#[derive(Clone, Default)]
pub struct Spreads(Arc<RwLock<HashMap<(String, String), Markup>>>);

impl Spreads {
    pub async fn load(pool: &PgPool) -> anyhow::Result<Self> {
        let s = Spreads::default();
        s.reload(pool).await?;
        Ok(s)
    }

    pub async fn reload(&self, pool: &PgPool) -> anyhow::Result<()> {
        let rows = sqlx::query("SELECT group_code, symbol, markup_points, min_spread_points FROM spread_markups").fetch_all(pool).await?;
        let map = rows
            .into_iter()
            .map(|r| {
                let m = Markup { group_code: r.get("group_code"), symbol: r.get("symbol"), markup_points: r.get("markup_points"), min_spread_points: r.get("min_spread_points") };
                ((m.group_code.clone(), m.symbol.clone()), m)
            })
            .collect();
        *self.0.write().unwrap() = map;
        Ok(())
    }

    pub fn all(&self) -> Vec<Markup> {
        let mut v: Vec<Markup> = self.0.read().unwrap().values().cloned().collect();
        v.sort_by(|a, b| (&a.group_code, &a.symbol).cmp(&(&b.group_code, &b.symbol)));
        v
    }

    /// Saves a markup; a spread group not seen before (a white-label broker's `<broker>-standard`, a custom group) is
    /// added to the `account_groups` catalogue in the same transaction so the foreign key holds.
    pub async fn upsert(&self, pool: &PgPool, m: &Markup) -> anyhow::Result<()> {
        let mut tx = pool.begin().await?;
        sqlx::query("INSERT INTO account_groups (code, name) VALUES ($1, $1) ON CONFLICT (code) DO NOTHING").bind(&m.group_code).execute(&mut *tx).await?;
        sqlx::query(
            "INSERT INTO spread_markups (group_code, symbol, markup_points, min_spread_points, updated_at) VALUES ($1,$2,$3,$4, now())
             ON CONFLICT (group_code, symbol) DO UPDATE SET markup_points = EXCLUDED.markup_points, min_spread_points = EXCLUDED.min_spread_points, updated_at = now()",
        )
        .bind(&m.group_code)
        .bind(&m.symbol)
        .bind(m.markup_points)
        .bind(m.min_spread_points)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.reload(pool).await
    }

    /// Client quote for a group: widen the raw spread symmetrically around mid by the markup,
    /// never below the group's minimum spread. `group = "raw"` (or unknown) returns the raw book.
    pub fn apply(&self, group: &str, inst: &Instrument, q: Quote) -> Quote {
        let map = self.0.read().unwrap();
        let m = map.get(&(group.to_string(), inst.symbol.clone())).or_else(|| map.get(&(group.to_string(), "*".to_string())));
        let Some(m) = m else { return q };
        let pt = inst.point();
        let raw = (q.ask - q.bid).max(0.0);
        let spread = (raw + m.markup_points as f64 * pt).max(m.min_spread_points as f64 * pt);
        let mid = (q.ask + q.bid) / 2.0;
        Quote { bid: inst.round(mid - spread / 2.0), ask: inst.round(mid + spread / 2.0), ..q }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::ConnectOptions;
    use sqlx::postgres::PgConnectOptions;
    use std::str::FromStr;

    /// Throw-away database `kalks_md_spreads_test_<pid>` on the local Postgres (127.0.0.1:5433); None (test skipped
    /// with a message) when Postgres is not reachable. Override the server with MD_TEST_DATABASE_URL.
    async fn test_db() -> Option<String> {
        let server = std::env::var("MD_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into());
        let opts = PgConnectOptions::from_str(&server).ok()?;
        let db = format!("kalks_md_spreads_test_{}", std::process::id());
        let mut conn = match opts.database("postgres").connect().await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("skipping: Postgres not reachable ({e})");
                return None;
            }
        };
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{db}\" WITH (FORCE)"))).execute(&mut conn).await;
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

    fn m(group: &str, symbol: &str, markup: i32) -> Markup {
        Markup { group_code: group.into(), symbol: symbol.into(), markup_points: markup, min_spread_points: 0 }
    }

    #[test]
    fn markup_shape_is_checked() {
        assert!(m("standard", "*", 10).check().is_ok());
        assert!(m("northwind-standard", "EURUSD", 0).check().is_ok());
        assert!(m("acme_vip2", "*", 100_000).check().is_ok());
        for bad in ["", "Standard", "north wind", "-pro", "x'; drop", &"a".repeat(65)] {
            assert!(m(bad, "*", 10).check().is_err(), "{bad:?}");
        }
        assert!(m("raw", "*", 10).check().unwrap_err().contains("raw"));
        assert!(m("pro", "*", -1).check().is_err());
        assert!(m("pro", "*", 100_001).check().is_err());
    }

    #[tokio::test]
    async fn markups_save_for_any_brokers_spread_group() {
        let Some(url) = test_db().await else { return };
        let pool = crate::db::connect(&url).await.unwrap();
        let s = Spreads::load(&pool).await.unwrap();

        // a seeded group still saves
        s.upsert(&pool, &m("standard", "EURUSD", 12)).await.unwrap();
        // a white-label broker's own spread groups ("<broker>-standard", or a custom one) save too
        s.upsert(&pool, &m("northwind-standard", "*", 15)).await.expect("a broker's own spread group saves");
        s.upsert(&pool, &m("northwind-vip", "XAUUSD", 7)).await.expect("a custom spread group saves");
        s.upsert(&pool, &m("northwind-standard", "*", 20)).await.unwrap();
        let all = s.all();
        assert!(all.iter().any(|x| x.group_code == "northwind-standard" && x.symbol == "*" && x.markup_points == 20), "{all:?}");
        assert!(all.iter().any(|x| x.group_code == "northwind-vip" && x.symbol == "XAUUSD" && x.markup_points == 7), "{all:?}");
        // survives a reload from the database
        let again = Spreads::load(&pool).await.unwrap();
        assert_eq!(again.all().len(), all.len());

        pool.close().await;
        drop_db(&url).await;
    }
}
