//! Background loops: media processing (one item at a time: transcoding is heavy), the safety check of new and
//! edited posts / comments / stories and chat messages, notification + Rewards delivery, and the periodic jobs
//! (trending tags, creators, verified stats, clean-up).

use crate::moderation::{self, Decision, Verdict};
use crate::state::AppState;
use serde_json::json;
use sqlx::Row;
use sqlx::postgres::PgRow;
use std::time::{Duration, Instant};

pub fn spawn(st: &AppState) {
    let s = st.clone();
    tokio::spawn(async move {
        loop {
            if let Err(e) = crate::media::process_pending(&s).await {
                tracing::error!(error = %e, "media worker");
            }
            let _ = tokio::time::timeout(Duration::from_secs(5), s.wake.media.notified()).await;
        }
    });
    let s = st.clone();
    tokio::spawn(async move {
        loop {
            if let Err(e) = moderate_once(&s).await {
                tracing::error!(error = %e, "moderation worker");
            }
            let _ = tokio::time::timeout(Duration::from_secs(3), s.wake.moderation.notified()).await;
        }
    });
    let s = st.clone();
    tokio::spawn(async move {
        loop {
            if let Err(e) = crate::notify::deliver(&s).await {
                tracing::error!(error = %e, "notify worker");
            }
            if let Err(e) = crate::rewards::deliver(&s).await {
                tracing::error!(error = %e, "outbox worker");
            }
            let _ = tokio::time::timeout(Duration::from_secs(5), s.wake.notify.notified()).await;
        }
    });
    let s = st.clone();
    tokio::spawn(async move {
        let mut last_trending = Instant::now() - Duration::from_secs(3600);
        let mut last_hourly = Instant::now() - Duration::from_secs(7200);
        let mut last_daily = Instant::now() - Duration::from_secs(90_000);
        loop {
            if last_trending.elapsed() > Duration::from_secs(300) {
                if let Err(e) = crate::feeds::compute_trending(&s).await {
                    tracing::warn!(error = %e, "trending");
                }
                last_trending = Instant::now();
            }
            if let Err(e) = crate::stats::refresh_due(&s).await {
                tracing::warn!(error = %e, "stats refresh");
            }
            if last_hourly.elapsed() > Duration::from_secs(3600) {
                if let Err(e) = crate::rewards::creators(&s).await {
                    tracing::warn!(error = %e, "creators");
                }
                if let Err(e) = crate::media::cleanup(&s).await {
                    tracing::warn!(error = %e, "media clean-up");
                }
                last_hourly = Instant::now();
            }
            if last_daily.elapsed() > Duration::from_secs(86_400) {
                let _ = prune(&s).await;
                last_daily = Instant::now();
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
}

async fn prune(st: &AppState) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM seen WHERE at < now() - interval '14 days'").execute(&st.pool).await?;
    sqlx::query("DELETE FROM notify_outbox WHERE status IN ('sent','skipped') AND created_at < now() - interval '30 days'").execute(&st.pool).await?;
    sqlx::query("DELETE FROM ai_cache WHERE expires_at < now()").execute(&st.pool).await?;
    sqlx::query("DELETE FROM activity WHERE created_at < now() - interval '180 days'").execute(&st.pool).await?;
    Ok(())
}

/// One pass of the safety check over everything waiting. Returns how many items were decided.
pub async fn moderate_once(st: &AppState) -> anyhow::Result<usize> {
    let mut n = 0;
    n += moderate_table(st, Kind::Post).await?;
    n += moderate_table(st, Kind::Comment).await?;
    n += moderate_table(st, Kind::Story).await?;
    n += crate::chat::deliver_pending(st).await?;
    n += crate::chat::check_unchecked(st).await?;
    Ok(n)
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Post,
    Comment,
    Story,
}

impl Kind {
    fn table(self) -> &'static str {
        match self {
            Kind::Post => "posts",
            Kind::Comment => "comments",
            Kind::Story => "stories",
        }
    }
    fn name(self) -> &'static str {
        match self {
            Kind::Post => "post",
            Kind::Comment => "comment",
            Kind::Story => "story",
        }
    }
}

fn media_of(k: Kind, r: &PgRow) -> Vec<i64> {
    match k {
        Kind::Post => r.get::<Vec<i64>, _>("media"),
        _ => r.get::<Option<i64>, _>("media").into_iter().collect(),
    }
}

fn text_of(k: Kind, r: &PgRow) -> String {
    let mut t = r.get::<String, _>("body");
    if k == Kind::Post
        && let Some(p) = r.get::<Option<sqlx::types::Json<serde_json::Value>>, _>("poll")
    {
        let opts: Vec<String> = p.0["options"].as_array().into_iter().flatten().filter_map(|o| o.as_str().map(str::to_string)).collect();
        t.push_str(&format!("\n[poll options: {}]", opts.join(" | ")));
    }
    if k == Kind::Story {
        let stickers = r.get::<sqlx::types::Json<serde_json::Value>, _>("stickers").0;
        for s in stickers.as_array().into_iter().flatten() {
            for key in ["question", "prompt"] {
                if let Some(x) = s[key].as_str() {
                    t.push_str(&format!("\n[sticker: {x}]"));
                }
            }
        }
    }
    t
}

async fn moderate_table(st: &AppState, k: Kind) -> anyhow::Result<usize> {
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT * FROM {} WHERE status = 'pending' ORDER BY created_at LIMIT 20", k.table()))).fetch_all(&st.pool).await?;
    let mut decided = 0;
    for r in &rows {
        let id: i64 = r.get("id");
        let owner: i64 = r.get("author");
        // media first: the item waits for its uploads (at most an hour)
        let media = media_of(k, r);
        let statuses: Vec<(String, Option<String>)> = if media.is_empty() { vec![] } else { sqlx::query_as("SELECT status, reason FROM media WHERE id = ANY($1)").bind(&media).fetch_all(&st.pool).await? };
        let created: chrono::DateTime<chrono::Utc> = r.get("created_at");
        let waiting = statuses.iter().any(|(s, _)| s == "processing" || s == "uploading");
        let mut verdict = Verdict::allow("rules");
        if waiting {
            if chrono::Utc::now() - created < chrono::Duration::hours(1) {
                continue;
            }
            verdict = Verdict { decision: Decision::Review, categories: vec![], reason: "Media still processing after an hour.".into(), source: "fallback", model: None };
        }
        if let Some((_, reason)) = statuses.iter().find(|(s, _)| s == "rejected" || s == "failed" || s == "deleted") {
            verdict = verdict.merge(Verdict { decision: Decision::Block, categories: vec![], reason: reason.clone().unwrap_or_else(|| "A file didn't pass the content check.".into()), source: "ai", model: None });
        } else if statuses.iter().any(|(s, _)| s == "review") {
            verdict = verdict.merge(Verdict { decision: Decision::Review, categories: vec![], reason: "A file is waiting for a moderator.".into(), source: "ai", model: None });
        }
        let text = text_of(k, r);
        if verdict.decision != Decision::Block && !text.trim().is_empty() && st.ai() {
            match moderation::classify_text(st, k.name(), &text).await {
                Ok(v) => verdict = verdict.merge(v),
                Err(e) => {
                    let attempts: i32 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!("UPDATE {} SET attempts = attempts + 1 WHERE id = $1 RETURNING attempts", k.table()))).bind(id).fetch_one(&st.pool).await?;
                    if attempts < 5 {
                        tracing::warn!(error = %e, kind = k.name(), id, "AI check failed; will retry");
                        continue;
                    }
                    verdict = verdict.merge(Verdict { decision: Decision::Review, categories: vec![], reason: format!("AI check unavailable: {e}"), source: "fallback", model: None });
                }
            }
        }
        apply(st, k, r, id, owner, &text, verdict).await?;
        decided += 1;
    }
    Ok(decided)
}

async fn apply(st: &AppState, k: Kind, r: &PgRow, id: i64, owner: i64, text: &str, v: Verdict) -> anyhow::Result<()> {
    let tenant: String = sqlx::query_scalar("SELECT tenant FROM profiles WHERE user_id = $1").bind(owner).fetch_optional(&st.pool).await?.unwrap_or_else(|| "kalks".into());
    sqlx::query(sqlx::AssertSqlSafe(format!("UPDATE {} SET moderation = $2 WHERE id = $1", k.table()))).bind(id).bind(sqlx::types::Json(v.json())).execute(&st.pool).await?;
    match v.decision {
        Decision::Allow => match k {
            Kind::Post => crate::posts::publish(st, id).await?,
            Kind::Comment => crate::posts::publish_comment(st, id).await?,
            Kind::Story => crate::stories::publish(st, id).await?,
        },
        Decision::Review => {
            sqlx::query(sqlx::AssertSqlSafe(format!("UPDATE {} SET status = 'review' WHERE id = $1 AND status = 'pending'", k.table()))).bind(id).execute(&st.pool).await?;
            moderation::queue(st, k.name(), id, owner, &tenant, v.source, &v, text).await?;
            st.hub.send(crate::state::Target::User(owner), json!({"type": format!("{}.status", k.name()), "id": id, "status": "review"}));
        }
        Decision::Block => {
            let reason = moderation::main_reason(&v);
            let reason = if v.categories.is_empty() && !v.reason.is_empty() && v.source != "ai" { v.reason.clone() } else { reason };
            sqlx::query(sqlx::AssertSqlSafe(format!("UPDATE {} SET status = 'rejected', reason = $2 WHERE id = $1", k.table()))).bind(id).bind(&reason).execute(&st.pool).await?;
            // an edited comment that was counted stops counting
            if k == Kind::Comment && r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("edited_at").is_some() {
                sqlx::query("UPDATE posts SET comments = GREATEST(comments - 1, 0) WHERE id = $1").bind(r.get::<i64, _>("post_id")).execute(&st.pool).await?;
            }
            // decided by the AI: listed in the queue history for appeals, not open
            sqlx::query(
                "INSERT INTO mod_queue (target_kind, target_id, owner, tenant, source, categories, verdict, excerpt, status, decided_by, decided_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'removed','ai',now())",
            )
            .bind(k.name())
            .bind(id)
            .bind(owner)
            .bind(&tenant)
            .bind(v.source)
            .bind(&v.categories)
            .bind(sqlx::types::Json(v.json()))
            .bind(crate::util::preview(text, 500))
            .execute(&st.pool)
            .await?;
            let post = match k {
                Kind::Post => Some(id),
                Kind::Comment => Some(r.get::<i64, _>("post_id")),
                Kind::Story => None,
            };
            crate::notify::event(
                st,
                crate::notify::Ev { to: owner, kind: "moderation", actor: None, post, comment: (k == Kind::Comment).then_some(id), story: (k == Kind::Story).then_some(id), conversation: None, data: json!({"title": format!("Your {} wasn't published", k.name()), "reason": reason}) },
            )
            .await?;
            st.hub.send(crate::state::Target::User(owner), json!({"type": format!("{}.status", k.name()), "id": id, "status": "rejected", "reason": reason}));
        }
    }
    Ok(())
}
