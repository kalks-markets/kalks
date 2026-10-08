//! Creators (Q32): Rewards points for helpful posts, the creator badge, fee-discount eligibility of the top
//! creators. Points are delivered to the growth service through the `outbox` (retried; a 4xx needs a person and
//! is listed in the Back Office). Fee-discount eligibility is a flag the growth / trading side reads from
//! `GET /v1/internal/creators/fee-discounts`.
//!
//! A post becomes helpful automatically when likes + bull / bear votes from distinct KYC-verified members reach the
//! threshold (settings), or when staff mark it; at most `helpful_daily_cap` rewarded posts per author per day.

use crate::state::AppState;
use serde_json::{Value, json};
use sqlx::Row;

pub async fn check_helpful(st: &AppState, post: i64) -> anyhow::Result<bool> {
    let s = crate::profiles::settings(st).await;
    let r = sqlx::query("SELECT author, helpful, status FROM posts WHERE id = $1").bind(post).fetch_optional(&st.pool).await?;
    let Some(r) = r else { return Ok(false) };
    if r.get::<bool, _>("helpful") || r.get::<String, _>("status") != "published" {
        return Ok(false);
    }
    let author: i64 = r.get("author");
    let verified: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT x.user_id) FROM reactions x JOIN profiles p ON p.user_id = x.user_id WHERE x.post_id = $1 AND p.kyc_status = 'verified' AND x.user_id <> $2",
    )
    .bind(post)
    .bind(author)
    .fetch_one(&st.pool)
    .await?;
    if verified < s.helpful_threshold.max(1) {
        return Ok(false);
    }
    mark_helpful(st, post, "auto").await
}

/// Marks a post helpful (once) and rewards its author within the daily cap.
pub async fn mark_helpful(st: &AppState, post: i64, by: &str) -> anyhow::Result<bool> {
    let s = crate::profiles::settings(st).await;
    let Some(r) = sqlx::query("UPDATE posts SET helpful = true WHERE id = $1 AND NOT helpful RETURNING author").bind(post).fetch_optional(&st.pool).await? else { return Ok(false) };
    let author: i64 = r.get("author");
    let tenant: String = sqlx::query_scalar("SELECT tenant FROM profiles WHERE user_id = $1").bind(author).fetch_one(&st.pool).await?;
    let today: i64 = sqlx::query_scalar("SELECT count(*) FROM outbox WHERE kind = 'rewards.points' AND user_id = $1 AND created_at > date_trunc('day', now())").bind(author).fetch_one(&st.pool).await?;
    let points = if today < s.helpful_daily_cap { s.helpful_points.max(0) } else { 0 };
    if points > 0 {
        enqueue(st, "rewards.points", &tenant, author, &format!("helpful:{post}"), json!({"points": points, "note": format!("Kalks Circle: helpful post #{post}"), "postId": post, "by": by})).await?;
    }
    crate::gamify::xp(st, author, crate::gamify::XP_HELPFUL).await?;
    crate::gamify::award(st, author, "helpful_1").await?;
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM posts WHERE author = $1 AND helpful").bind(author).fetch_one(&st.pool).await?;
    if n >= 10 {
        crate::gamify::award(st, author, "helpful_10").await?;
    }
    crate::notify::event(st, crate::notify::Ev { to: author, kind: "helpful", actor: None, post: Some(post), comment: None, story: None, conversation: None, data: json!({"points": points}) }).await?;
    Ok(true)
}

pub async fn enqueue(st: &AppState, kind: &str, tenant: &str, user: i64, dedupe: &str, payload: Value) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO outbox (kind, tenant, user_id, dedupe_key, payload) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (dedupe_key) DO NOTHING")
        .bind(kind)
        .bind(tenant)
        .bind(user)
        .bind(dedupe)
        .bind(sqlx::types::Json(payload))
        .execute(&st.pool)
        .await?;
    st.wake.notify.notify_one();
    Ok(())
}

/// Delivers due outbox events. Returns how many were attempted.
pub async fn deliver(st: &AppState) -> anyhow::Result<usize> {
    let rows = sqlx::query("SELECT * FROM outbox WHERE status = 'pending' AND next_at <= now() ORDER BY id LIMIT 50").fetch_all(&st.pool).await?;
    let n = rows.len();
    for r in rows {
        let id: i64 = r.get("id");
        let kind: String = r.get("kind");
        let payload: Value = r.get::<sqlx::types::Json<Value>, _>("payload").0;
        let result = match kind.as_str() {
            "rewards.points" => crate::upstream::growth_points(st, &r.get::<String, _>("tenant"), r.get("user_id"), payload["points"].as_i64().unwrap_or(0), payload["note"].as_str().unwrap_or("Kalks Circle")).await,
            _ => Err((false, format!("no consumer for {kind}"))),
        };
        match result {
            Ok(()) => {
                sqlx::query("UPDATE outbox SET status = 'delivered', delivered_at = now(), error = NULL, attempts = attempts + 1 WHERE id = $1").bind(id).execute(&st.pool).await?;
            }
            Err((retry, e)) => {
                let attempts: i32 = r.get::<i32, _>("attempts") + 1;
                let status = if retry && attempts < 10 { "pending" } else { "failed" };
                sqlx::query("UPDATE outbox SET status = $2, attempts = $3, error = $4, next_at = now() + make_interval(secs => $5) WHERE id = $1")
                    .bind(id)
                    .bind(status)
                    .bind(attempts)
                    .bind(&e)
                    .bind(60.0 * 2f64.powi(attempts.min(10)))
                    .execute(&st.pool)
                    .await?;
            }
        }
    }
    Ok(n)
}

/// Hourly: creator badges (followers threshold or 30-day engagement) and fee-discount eligibility of the top N
/// creators by 30-day engagement.
pub async fn creators(st: &AppState) -> anyhow::Result<()> {
    let s = crate::profiles::settings(st).await;
    sqlx::query(
        "UPDATE profiles SET creator = (followers_count >= $1) WHERE creator <> (followers_count >= $1) AND staff_badge IS NULL AND status = 'active'",
    )
    .bind(s.creator_min_followers)
    .execute(&st.pool)
    .await?;
    let top: Vec<i64> = sqlx::query_scalar(TOP_CREATORS).bind(s.fee_discount_top.max(0)).fetch_all(&st.pool).await?;
    let before: Vec<i64> = sqlx::query_scalar("SELECT user_id FROM profiles WHERE fee_discount").fetch_all(&st.pool).await?;
    sqlx::query("UPDATE profiles SET fee_discount = (user_id = ANY($1)) WHERE fee_discount <> (user_id = ANY($1))").bind(&top).execute(&st.pool).await?;
    for u in top.iter().filter(|u| !before.contains(u)) {
        crate::gamify::award(st, *u, "top_creator").await?;
        crate::audit::record(&st.pool, &crate::audit::Actor::system(), "creator.fee_discount_on", Some(format!("user:{u}")), None, None, None).await?;
    }
    for u in before.iter().filter(|u| !top.contains(u)) {
        crate::audit::record(&st.pool, &crate::audit::Actor::system(), "creator.fee_discount_off", Some(format!("user:{u}")), None, None, None).await?;
    }
    Ok(())
}

/// Top creators by 30-day engagement on their published posts (likes + 2 × comments + 3 × reposts / quotes +
/// 2 × saves), active and not shadow-hidden; `$1` = how many.
pub const TOP_CREATORS: &str = "SELECT p.author FROM posts p JOIN profiles a ON a.user_id = p.author
     WHERE p.status = 'published' AND p.published_at > now() - interval '30 days' AND a.status = 'active' AND NOT a.shadow_hidden
     GROUP BY p.author HAVING sum(p.likes + p.bulls + p.bears + 2 * p.comments + 3 * (p.reposts + p.quotes) + 2 * p.saves) > 0
     ORDER BY sum(p.likes + p.bulls + p.bears + 2 * p.comments + 3 * (p.reposts + p.quotes) + 2 * p.saves) DESC LIMIT $1";
