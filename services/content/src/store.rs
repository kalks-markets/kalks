//! Database access: connect (creating the database on first start) + migrate, and the audit log.

use serde_json::Value;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{ConnectOptions, PgPool};
use std::str::FromStr;

pub async fn connect(url: &str) -> anyhow::Result<PgPool> {
    let opts = PgConnectOptions::from_str(url)?;
    let db = opts.get_database().unwrap_or("kalks_content").to_string();
    let admin = opts.clone().database("postgres");
    let mut conn = admin.connect().await?;
    let exists: Option<i32> = sqlx::query_scalar("SELECT 1 FROM pg_database WHERE datname = $1").bind(&db).fetch_optional(&mut conn).await?;
    if exists.is_none() {
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE \"{}\"", db.replace('"', "")))).execute(&mut conn).await?;
        tracing::info!(%db, "created database");
    }
    drop(conn);
    let pool = PgPoolOptions::new().max_connections(16).connect_with(opts).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

/// Records a staff change. A failed write is logged, never fatal (the change itself already happened).
pub async fn audit<'e, E: sqlx::PgExecutor<'e>>(db: E, tenant: &str, staff: &str, action: &str, target: &str, detail: Value) {
    let r = sqlx::query("INSERT INTO content_audit (tenant, staff, action, target, detail) VALUES ($1,$2,$3,$4,$5)")
        .bind(tenant)
        .bind(staff)
        .bind(action)
        .bind(target)
        .bind(detail)
        .execute(db)
        .await;
    if let Err(e) = r {
        tracing::error!(error = %e, "content audit write failed");
    }
}
