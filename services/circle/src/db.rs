//! Connection + migrations, and the community settings (Back Office › Circle › settings).

use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{ConnectOptions, PgPool};
use std::str::FromStr;

/// Connects, creating the database on first run, and applies migrations.
pub async fn connect(url: &str) -> anyhow::Result<PgPool> {
    let opts = PgConnectOptions::from_str(url)?;
    let db = opts.get_database().unwrap_or("kalks_circle").to_string();
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

/// Community-wide settings (one shared community across brokers).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Added to CIRCLE_RESTRICTED_COUNTRIES (ISO alpha-2).
    pub restricted_countries: Vec<String>,
    /// A post becomes "helpful" (Rewards points) at this many likes + bull/bear votes from distinct verified users.
    pub helpful_threshold: i64,
    pub helpful_points: i64,
    /// Helpful-post rewards per author per day.
    pub helpful_daily_cap: i64,
    /// Followers for the automatic creator badge.
    pub creator_min_followers: i64,
    /// Top creators (by 30-day engagement) flagged eligible for a fee discount.
    pub fee_discount_top: i64,
    /// Closed trades needed to appear on the traders leaderboard.
    pub leaderboard_min_trades: i64,
    /// Reach multiplier for KYC-verified authors in For you / Explore.
    pub verified_boost: f64,
    /// Posts per author per hour.
    pub posts_per_hour: i64,
    /// Chat messages per user per minute.
    pub messages_per_minute: i64,
    pub max_group_members: i64,
    /// Requests from strangers per user per day.
    pub dm_requests_per_day: i64,
    /// Community rules text shown in the composer and the onboarding sheet.
    pub rules_text: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            restricted_countries: vec![],
            helpful_threshold: 25,
            helpful_points: 50,
            helpful_daily_cap: 3,
            creator_min_followers: 500,
            fee_discount_top: 20,
            leaderboard_min_trades: 10,
            verified_boost: 1.25,
            posts_per_hour: 20,
            messages_per_minute: 30,
            max_group_members: 100,
            dm_requests_per_day: 50,
            rules_text: DEFAULT_RULES.into(),
        }
    }
}

pub const DEFAULT_RULES: &str = "Kalks Circle community rules (draft)
1. Be respectful. No harassment, hate, threats or personal attacks.
2. No profit promises. Never say a trade or strategy is guaranteed. Profit claims are shown only through verified Kalks trade cards; screenshots of P&L from other platforms are removed.
3. No scams. No requests for money, passwords, account access or off-platform \"account management\". Report anyone who asks.
4. No spam or advertising. External links are limited to Kalks pages, YouTube and TradingView.
5. No nudity, violence or illegal content.
6. Share ideas, not instructions. Nothing on Kalks Circle is investment advice. Trading involves risk.
7. Respect privacy. Don't share anyone's personal data.
Breaking the rules can lead to removal of content, warnings, limited reach or a ban.";

pub async fn settings(pool: &PgPool) -> anyhow::Result<Settings> {
    let row: Option<sqlx::types::Json<serde_json::Value>> = sqlx::query_scalar("SELECT data FROM settings WHERE key = 'general'").fetch_optional(pool).await?;
    Ok(match row {
        Some(v) => serde_json::from_value(v.0).unwrap_or_default(),
        None => Settings::default(),
    })
}

pub async fn save_settings(pool: &PgPool, s: &Settings, by: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO settings (key, data, updated_by) VALUES ('general', $1, $2)
         ON CONFLICT (key) DO UPDATE SET data = EXCLUDED.data, updated_by = EXCLUDED.updated_by, updated_at = now()",
    )
    .bind(sqlx::types::Json(s))
    .bind(by)
    .execute(pool)
    .await?;
    Ok(())
}
