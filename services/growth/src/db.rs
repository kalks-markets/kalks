//! Connection + migrations, per-tenant configuration (seeded on first use) and small shared queries.

use crate::model::{EarnRule, Settings, Tier, default_rules, default_tiers};
use crate::money::{D, dec};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{ConnectOptions, PgPool, Row};
use std::str::FromStr;

/// Connects, creating the database on first run, and applies migrations.
pub async fn connect(url: &str) -> anyhow::Result<PgPool> {
    let opts = PgConnectOptions::from_str(url)?;
    let db = opts.get_database().unwrap_or("kalks_growth").to_string();
    let mut admin = opts.clone().database("postgres").connect().await?;
    let exists: Option<i32> = sqlx::query_scalar("SELECT 1 FROM pg_database WHERE datname = $1").bind(&db).fetch_optional(&mut admin).await?;
    if exists.is_none() {
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE \"{}\"", db.replace('"', "")))).execute(&mut admin).await?;
        tracing::info!(%db, "created database");
    }
    drop(admin);
    let pool = PgPoolOptions::new().max_connections(20).connect_with(opts).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

/// Seeds a tenant's defaults once: settings, tiers, earning rules per asset class and a starter catalogue.
pub async fn seed(pool: &PgPool, tenant: &str) -> anyhow::Result<()> {
    settings(pool, tenant).await?;
    tiers(pool, tenant).await?;
    let mut tx = pool.begin().await?;
    // advisory lock so two instances never seed twice
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))").bind(format!("growth-seed:{tenant}")).execute(&mut *tx).await?;
    let rules: i64 = sqlx::query_scalar("SELECT count(*) FROM earn_rules WHERE tenant = $1").bind(tenant).fetch_one(&mut *tx).await?;
    if rules == 0 {
        for (name, class, per) in default_rules() {
            sqlx::query("INSERT INTO earn_rules (tenant, name, asset_class, points_per_lot) VALUES ($1,$2,$3,$4)")
                .bind(tenant)
                .bind(name)
                .bind(class)
                .bind(dec(per))
                .execute(&mut *tx)
                .await?;
        }
    }
    let items: i64 = sqlx::query_scalar("SELECT count(*) FROM catalogue WHERE tenant = $1").bind(tenant).fetch_one(&mut *tx).await?;
    if items == 0 {
        let seed: [(&str, &str, &str, i64, &str, serde_json::Value, i32); 4] = [
            ("$10 cash to your wallet", "Paid to your USDT wallet at once. Withdrawable.", "cashback", 1_000, "10", serde_json::json!({}), 10),
            ("$50 cash to your wallet", "Paid to your USDT wallet at once. Withdrawable.", "cashback", 4_800, "50", serde_json::json!({}), 20),
            ("$100 trading bonus", "Bonus credit on a live account. Releases to balance at $5 per lot traded within 60 days.", "bonus_credit", 5_000, "100", serde_json::json!({"releasePerLot": 5, "expiryDays": 60}), 30),
            ("20% off a prop challenge", "A one-time voucher for any prop challenge fee. Valid for 90 days.", "fee_discount", 2_500, "20", serde_json::json!({"appliesTo": "prop", "validDays": 90}), 40),
        ];
        for (name, desc, kind, cost, value, params, sort) in seed {
            sqlx::query("INSERT INTO catalogue (tenant, name, description, kind, cost_points, value, params, sort) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
                .bind(tenant)
                .bind(name)
                .bind(desc)
                .bind(kind)
                .bind(cost)
                .bind(dec(value))
                .bind(sqlx::types::Json(params))
                .bind(sort)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    // phase 4: contest templates, starter missions and levels
    crate::templates::seed(pool, tenant).await?;
    crate::missions::seed(pool, tenant).await?;
    crate::xp::levels(pool, tenant).await?;
    Ok(())
}

pub async fn settings(pool: &PgPool, tenant: &str) -> anyhow::Result<Settings> {
    let row: Option<sqlx::types::Json<serde_json::Value>> = sqlx::query_scalar("SELECT data FROM settings WHERE tenant = $1").bind(tenant).fetch_optional(pool).await?;
    match row {
        Some(v) => {
            let mut base = serde_json::to_value(Settings::default())?;
            if let (Some(b), Some(o)) = (base.as_object_mut(), v.0.as_object()) {
                for (k, val) in o {
                    b.insert(k.clone(), val.clone());
                }
            }
            Ok(serde_json::from_value(base)?)
        }
        None => {
            let s = Settings::default();
            sqlx::query("INSERT INTO settings (tenant, data, updated_by) VALUES ($1, $2, 'system') ON CONFLICT (tenant) DO NOTHING")
                .bind(tenant)
                .bind(sqlx::types::Json(&s))
                .execute(pool)
                .await?;
            Ok(s)
        }
    }
}

fn tier_row(r: &sqlx::postgres::PgRow) -> Tier {
    let perks: sqlx::types::Json<Vec<String>> = r.get("perks");
    Tier { key: r.get("key"), name: r.get("name"), rank: r.get("rank"), min_points: r.get("min_points"), multiplier: r.get("multiplier"), perks: perks.0 }
}

/// Tiers by rank (seeded on first use).
pub async fn tiers(pool: &PgPool, tenant: &str) -> anyhow::Result<Vec<Tier>> {
    let rows = sqlx::query("SELECT * FROM tiers WHERE tenant = $1 ORDER BY rank").bind(tenant).fetch_all(pool).await?;
    if !rows.is_empty() {
        return Ok(rows.iter().map(tier_row).collect());
    }
    let mut tx = pool.begin().await?;
    replace_tiers(&mut tx, tenant, &default_tiers()).await?;
    tx.commit().await?;
    Ok(default_tiers())
}

pub async fn replace_tiers(tx: &mut sqlx::PgConnection, tenant: &str, tiers: &[Tier]) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM tiers WHERE tenant = $1").bind(tenant).execute(&mut *tx).await?;
    for t in tiers {
        sqlx::query("INSERT INTO tiers (tenant, key, name, rank, min_points, multiplier, perks) VALUES ($1,$2,$3,$4,$5,$6,$7)")
            .bind(tenant)
            .bind(&t.key)
            .bind(&t.name)
            .bind(t.rank)
            .bind(t.min_points)
            .bind(t.multiplier)
            .bind(sqlx::types::Json(&t.perks))
            .execute(&mut *tx)
            .await?;
    }
    Ok(())
}

pub fn rule_row(r: &sqlx::postgres::PgRow) -> EarnRule {
    EarnRule {
        id: r.get("id"),
        name: r.get("name"),
        asset_class: r.get("asset_class"),
        symbols: r.get("symbols"),
        account_groups: r.get("account_groups"),
        account_type: r.get("account_type"),
        points_per_lot: r.get::<D, _>("points_per_lot"),
        priority: r.get("priority"),
        active: r.get("active"),
    }
}

pub async fn rules(pool: &PgPool, tenant: &str) -> anyhow::Result<Vec<EarnRule>> {
    let rows = sqlx::query("SELECT * FROM earn_rules WHERE tenant = $1 ORDER BY priority, id").bind(tenant).fetch_all(pool).await?;
    Ok(rows.iter().map(rule_row).collect())
}

pub async fn cursor(pool: &PgPool, name: &str) -> anyhow::Result<Option<String>> {
    Ok(sqlx::query_scalar("SELECT value FROM cursors WHERE name = $1").bind(name).fetch_optional(pool).await?)
}

pub async fn set_cursor(pool: &PgPool, name: &str, value: &str) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO cursors (name, value) VALUES ($1, $2) ON CONFLICT (name) DO UPDATE SET value = EXCLUDED.value, updated_at = now()")
        .bind(name)
        .bind(value)
        .execute(pool)
        .await?;
    Ok(())
}

/// Tenants the workers serve: every tenant with configuration, plus the default.
pub async fn tenants(pool: &PgPool) -> anyhow::Result<Vec<String>> {
    let mut t: Vec<String> = sqlx::query_scalar("SELECT tenant FROM settings").fetch_all(pool).await?;
    if !t.iter().any(|x| x == "kalks") {
        t.push("kalks".into());
    }
    Ok(t)
}

/// A readable short code: 10 chars from an unambiguous alphabet (no 0/O/1/I).
pub fn random_code(len: usize) -> String {
    const A: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut b = vec![0u8; len];
    getrandom::fill(&mut b).expect("OS randomness unavailable");
    b.iter().map(|x| A[(*x as usize) % A.len()] as char).collect()
}
