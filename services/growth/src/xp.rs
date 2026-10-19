//! XP, levels and daily streaks (phase 4). XP is an append-only ledger (`xp_ledger`, unique per source + ref, so
//! every source books once); the level is derived from the total against the broker's `levels` table (seeded
//! Rookie → Legend). Streaks: `trading` (a day with a qualifying live CFD trade; weekends never break it) and
//! `login` (a day the client opened the Client Area or the app), with `streakGraceDays` forgiven once a week. Every
//! 7 days of a trading streak earn `xpStreakWeek` XP.

use crate::challenge_calc::{self as cc, Level, Streak};
use crate::clients;
use crate::db;
use crate::state::AppState;
use chrono::{DateTime, NaiveDate, Utc};
use serde_json::{Value, json};
use sqlx::Row;

/// The broker's levels by level (seeded on first use).
pub async fn levels(pool: &sqlx::PgPool, tenant: &str) -> anyhow::Result<Vec<Level>> {
    let rows = sqlx::query("SELECT level, name, min_xp, badge FROM levels WHERE tenant = $1 ORDER BY level").bind(tenant).fetch_all(pool).await?;
    if !rows.is_empty() {
        return Ok(rows.iter().map(|r| Level { level: r.get("level"), name: r.get("name"), min_xp: r.get("min_xp"), badge: r.get("badge") }).collect());
    }
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))").bind(format!("growth-levels:{tenant}")).execute(&mut *tx).await?;
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM levels WHERE tenant = $1").bind(tenant).fetch_one(&mut *tx).await?;
    if n == 0 {
        replace_levels(&mut tx, tenant, &cc::default_levels()).await?;
    }
    tx.commit().await?;
    Box::pin(levels(pool, tenant)).await
}

pub async fn replace_levels(tx: &mut sqlx::PgConnection, tenant: &str, levels: &[Level]) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM levels WHERE tenant = $1").bind(tenant).execute(&mut *tx).await?;
    for l in levels {
        sqlx::query("INSERT INTO levels (tenant, level, name, min_xp, badge) VALUES ($1,$2,$3,$4,$5)").bind(tenant).bind(l.level).bind(l.name.trim()).bind(l.min_xp).bind(&l.badge).execute(&mut *tx).await?;
    }
    Ok(())
}

pub async fn total<'e, E: sqlx::PgExecutor<'e>>(ex: E, tenant: &str, user_id: i64) -> anyhow::Result<i64> {
    let v: Option<i64> = sqlx::query_scalar("SELECT sum(xp)::bigint FROM xp_ledger WHERE tenant = $1 AND user_id = $2").bind(tenant).bind(user_id).fetch_one(ex).await?;
    Ok(v.unwrap_or(0).max(0))
}

/// XP totals of many clients at once (leaderboards).
pub async fn totals(pool: &sqlx::PgPool, tenant: &str, users: &[i64]) -> anyhow::Result<std::collections::HashMap<i64, i64>> {
    if users.is_empty() {
        return Ok(Default::default());
    }
    let rows: Vec<(i64, i64)> = sqlx::query_as("SELECT user_id, sum(xp)::bigint FROM xp_ledger WHERE tenant = $1 AND user_id = ANY($2) GROUP BY user_id").bind(tenant).bind(users).fetch_all(pool).await?;
    Ok(rows.into_iter().collect())
}

/// `{level, name, badge}` of an XP total: what leaderboards show next to a nickname.
pub fn badge_json(levels: &[Level], xp: i64) -> Value {
    let (l, _) = cc::level_for(levels, xp);
    json!({"level": l.level, "name": l.name, "badge": l.badge})
}

/// The client's level card: XP, current and next level, progress, the level table.
pub fn level_json(levels: &[Level], xp: i64) -> Value {
    let (cur, next) = cc::level_for(levels, xp);
    json!({
        "xp": xp,
        "level": cur.level, "name": cur.name, "badge": cur.badge, "minXp": cur.min_xp,
        "next": next.as_ref().map(|n| json!({"level": n.level, "name": n.name, "badge": n.badge, "minXp": n.min_xp, "xpToGo": n.min_xp - xp})),
        "progressPct": (cc::level_progress(&cur, next.as_ref(), xp) * 10.0).round() / 10.0,
        "levels": levels,
    })
}

/// Books XP once per (source, ref) inside the caller's transaction. Returns whether it was new.
pub async fn award_in(tx: &mut sqlx::PgConnection, tenant: &str, user_id: i64, xp: i64, source: &str, reference: &str, description: &str) -> anyhow::Result<bool> {
    if xp == 0 {
        return Ok(false);
    }
    let n = sqlx::query("INSERT INTO xp_ledger (tenant, user_id, xp, source, ref, description) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (tenant, source, ref) DO NOTHING")
        .bind(tenant)
        .bind(user_id)
        .bind(xp as i32)
        .bind(source)
        .bind(reference)
        .bind(description.chars().take(200).collect::<String>())
        .execute(&mut *tx)
        .await?
        .rows_affected();
    Ok(n > 0)
}

/// Books XP and tells the client about a level-up (best effort, deduplicated by level).
pub async fn award(st: &AppState, tenant: &str, user_id: i64, xp: i64, source: &str, reference: &str, description: &str) -> anyhow::Result<bool> {
    let before = total(&st.pool, tenant, user_id).await?;
    let mut tx = st.pool.begin().await?;
    let fresh = award_in(&mut tx, tenant, user_id, xp, source, reference, description).await?;
    tx.commit().await?;
    if fresh {
        level_up_check(st, tenant, user_id, before, before + xp).await;
    }
    Ok(fresh)
}

/// Notifies a level-up between two totals (one notification per level reached, deduplicated).
pub async fn level_up_check(st: &AppState, tenant: &str, user_id: i64, before: i64, after: i64) {
    let Ok(ls) = levels(&st.pool, tenant).await else { return };
    let (a, _) = cc::level_for(&ls, before);
    let (b, _) = cc::level_for(&ls, after);
    if b.level > a.level {
        clients::notify_dedupe(
            st,
            tenant,
            user_id,
            "rewards.level_up",
            format!("Level {} reached: {}", b.level, b.name),
            format!("You now have {after} XP. Keep your streak going to reach the next level."),
            "/rewards",
            Some(format!("level:{user_id}:{}", b.level)),
        );
    }
}

pub fn tx_json(r: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": r.get::<i64, _>("id"),
        "xp": r.get::<i32, _>("xp"),
        "source": r.get::<String, _>("source"),
        "description": r.get::<String, _>("description"),
        "createdAt": r.get::<DateTime<Utc>, _>("created_at"),
    })
}

// ---------------------------------------------------------------- streaks

pub async fn streak(pool: &sqlx::PgPool, tenant: &str, user_id: i64, kind: &str) -> anyhow::Result<Streak> {
    let r = sqlx::query("SELECT current, best, last_day, grace_used FROM streaks WHERE tenant = $1 AND user_id = $2 AND kind = $3").bind(tenant).bind(user_id).bind(kind).fetch_optional(pool).await?;
    Ok(r.map(|r| Streak { current: r.get("current"), best: r.get("best"), last_day: r.get("last_day"), grace_used: r.get("grace_used") }).unwrap_or_default())
}

/// Records a day of activity on a streak (idempotent per day; late days change nothing). Trading streaks earn
/// `xpStreakWeek` XP at every multiple of 7 days. Returns the streak after the update.
pub async fn touch(st: &AppState, tenant: &str, user_id: i64, kind: &str, day: NaiveDate) -> anyhow::Result<Streak> {
    let settings = db::settings(&st.pool, tenant).await?;
    let mut tx = st.pool.begin().await?;
    sqlx::query("INSERT INTO streaks (tenant, user_id, kind) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING").bind(tenant).bind(user_id).bind(kind).execute(&mut *tx).await?;
    let r = sqlx::query("SELECT current, best, last_day, grace_used FROM streaks WHERE tenant = $1 AND user_id = $2 AND kind = $3 FOR UPDATE").bind(tenant).bind(user_id).bind(kind).fetch_one(&mut *tx).await?;
    let cur = Streak { current: r.get("current"), best: r.get("best"), last_day: r.get("last_day"), grace_used: r.get("grace_used") };
    let (next, moved) = cc::advance_streak(&cur, day, settings.streak_grace_days, kind == "trading");
    if !moved {
        tx.commit().await?;
        return Ok(cur);
    }
    sqlx::query("UPDATE streaks SET current = $4, best = $5, last_day = $6, grace_used = $7, updated_at = now() WHERE tenant = $1 AND user_id = $2 AND kind = $3")
        .bind(tenant)
        .bind(user_id)
        .bind(kind)
        .bind(next.current)
        .bind(next.best)
        .bind(next.last_day)
        .bind(next.grace_used)
        .execute(&mut *tx)
        .await?;
    let mut bonus = false;
    if kind == "trading" && next.current > 0 && next.current % 7 == 0 && settings.xp_streak_week > 0 {
        // one award per streak milestone: the ref carries the day the milestone was reached
        bonus = award_in(&mut tx, tenant, user_id, settings.xp_streak_week, "streak", &format!("trading:{}:{}", day, next.current), &format!("{}-day trading streak", next.current)).await?;
    }
    let before = if bonus { total(&mut *tx, tenant, user_id).await? - settings.xp_streak_week } else { 0 };
    tx.commit().await?;
    if bonus {
        level_up_check(st, tenant, user_id, before, before + settings.xp_streak_week).await;
    }
    Ok(next)
}

/// `{current, best, lastDay, alive}` as shown today.
pub fn streak_json(s: &Streak, today: NaiveDate, grace_days: i64, skip_weekends: bool) -> Value {
    json!({"current": cc::streak_alive(s, today, grace_days, skip_weekends), "best": s.best, "lastDay": s.last_day, "graceDays": grace_days})
}

/// Trading days from the deals growth has seen: every server-time day with a qualifying live CFD trade closed since
/// `since`, per client, in order (feeds the trading streaks).
pub async fn trading_days_since(pool: &sqlx::PgPool, tenant: &str, since: DateTime<Utc>, min_lots: rust_decimal::Decimal, min_hold: i64) -> anyhow::Result<Vec<(i64, NaiveDate)>> {
    Ok(sqlx::query_as(
        "SELECT DISTINCT user_id, (close_time + interval '3 hours')::date AS d FROM deals
          WHERE tenant = $1 AND processed_at >= $2 AND account_kind = 'live' AND instrument = 'cfd' AND NOT reversed AND deal_kind <> 'price-correction'
            AND lots >= $3 AND extract(epoch FROM close_time - open_time) >= $4
          ORDER BY user_id, d",
    )
    .bind(tenant)
    .bind(since)
    .bind(min_lots)
    .bind(min_hold as f64)
    .fetch_all(pool)
    .await?)
}
