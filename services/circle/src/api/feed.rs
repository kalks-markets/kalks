//! Feeds, Explore, search, topics, leaderboards, prices, announcements, the Academy chapter threads and the AI
//! sentiment / digest.

use super::{Me, Page};
use crate::error::{ApiError, ApiResult};
use crate::feeds;
use crate::state::AppState;
use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use std::time::Duration;

type R = ApiResult<Json<Value>>;

pub async fn following(State(st): State<AppState>, me: Me, Query(p): Query<Page>) -> R {
    Ok(Json(feeds::following(&st, &me, &p).await?))
}

pub async fn for_you(State(st): State<AppState>, me: Me, Query(p): Query<Page>) -> R {
    let mut v = feeds::for_you(&st, &me, &p).await?;
    if p.cursor.is_none() {
        v["announcements"] = json!(feeds::announcements(&st).await?);
    }
    Ok(Json(v))
}

pub async fn hashtag(State(st): State<AppState>, me: Me, Path(tag): Path<String>, Query(p): Query<Page>) -> R {
    Ok(Json(feeds::hashtag(&st, &me, &tag, &p).await?))
}

pub async fn cashtag(State(st): State<AppState>, me: Me, Path(symbol): Path<String>, Query(p): Query<Page>) -> R {
    Ok(Json(feeds::cashtag(&st, &me, &symbol, &p).await?))
}

#[derive(Deserialize)]
pub struct TabQ {
    tab: Option<String>,
    sort: Option<String>,
}

pub async fn profile_posts(State(st): State<AppState>, me: Me, Path(handle): Path<String>, Query(t): Query<TabQ>, Query(p): Query<Page>) -> R {
    Ok(Json(feeds::profile_posts(&st, &me, &handle, t.tab.as_deref().unwrap_or("posts"), &p).await?))
}

pub async fn explore(State(st): State<AppState>, me: Me) -> R {
    Ok(Json(feeds::explore(&st, &me).await?))
}

#[derive(Deserialize)]
pub struct SearchQ {
    q: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
}

pub async fn search(State(st): State<AppState>, me: Me, Query(q): Query<SearchQ>) -> R {
    let kind = q.kind.unwrap_or_else(|| "all".into());
    if !["all", "users", "hashtags", "cashtags", "posts"].contains(&kind.as_str()) {
        return Err(crate::error::invalid("type", "type is all, users, hashtags, cashtags or posts."));
    }
    Ok(Json(feeds::search(&st, &me, q.q.as_deref().unwrap_or(""), &kind).await?))
}

pub async fn topics(State(st): State<AppState>, _me: Me) -> R {
    let rows = sqlx::query("SELECT t.key, t.title, t.description, (SELECT count(*) FROM posts p WHERE p.topic = t.key AND p.status = 'published' AND p.kind = 'video') AS videos FROM topics t WHERE t.active ORDER BY t.position, t.key")
        .fetch_all(&st.pool)
        .await?;
    Ok(Json(json!({"items": rows.iter().map(|r| json!({"key": r.get::<String, _>("key"), "title": r.get::<String, _>("title"), "description": r.get::<String, _>("description"), "videos": r.get::<i64, _>("videos")})).collect::<Vec<_>>(), "maxVideoSecs": st.cfg.max_topic_video_secs})))
}

pub async fn topic_videos(State(st): State<AppState>, me: Me, Path(key): Path<String>, Query(t): Query<TabQ>, Query(p): Query<Page>) -> R {
    Ok(Json(feeds::topic_videos(&st, &me, &key, t.sort.as_deref().unwrap_or("new"), &p).await?))
}

pub async fn leaderboard(State(st): State<AppState>, me: Me, Path(board): Path<String>) -> R {
    Ok(Json(feeds::leaderboard(&st, &me, &board).await?))
}

#[derive(Deserialize)]
pub struct SymbolsQ {
    symbols: Option<String>,
}

pub async fn prices(State(st): State<AppState>, _me: Me, Query(q): Query<SymbolsQ>) -> R {
    let syms: Vec<String> = q.symbols.unwrap_or_default().split(',').map(|s| s.trim().trim_start_matches('$').to_uppercase()).filter(|s| crate::upstream::is_symbol(s)).take(50).collect();
    Ok(Json(json!({"quotes": crate::upstream::quotes(&st, &syms).await})))
}

pub async fn announcements(State(st): State<AppState>, _me: Me) -> R {
    Ok(Json(json!({"items": feeds::announcements(&st).await?})))
}

pub async fn chapter_thread(State(st): State<AppState>, me: Me, Path(chapter): Path<String>, Query(p): Query<Page>) -> R {
    let chapter = chapter.trim().to_lowercase();
    if chapter.is_empty() || chapter.len() > 120 || !chapter.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err(ApiError::NotFound);
    }
    Ok(Json(feeds::chapter_thread(&st, &me, &chapter, &p).await?))
}

pub async fn rules(State(st): State<AppState>, _me: Me) -> R {
    let s = crate::profiles::settings(&st).await;
    let r = crate::moderation::rules(&st).await?;
    Ok(Json(json!({"rules": s.rules_text, "allowedLinks": r.allow, "riskLine": crate::text::RISK_LINE, "reportReasons": super::posts::REPORT_REASONS})))
}

/// Per-symbol community sentiment: the crowd's bull / bear votes and, with AI, a short neutral summary.
pub async fn sentiment(State(st): State<AppState>, me: Me, Path(symbol): Path<String>) -> R {
    let symbol = symbol.trim().trim_start_matches('$').to_uppercase();
    if !crate::upstream::is_symbol(&symbol) {
        return Err(ApiError::NotFound);
    }
    let (bulls, bears) = feeds::crowd(&st, &symbol).await?;
    let texts: Vec<String> = sqlx::query_scalar(
        "SELECT p.body FROM posts p JOIN profiles a ON a.user_id = p.author WHERE p.status = 'published' AND $1 = ANY(p.cashtags) AND p.published_at > now() - interval '24 hours'
           AND p.visibility = 'public' AND NOT a.private AND NOT a.shadow_hidden AND NOT p.shadow ORDER BY p.published_at DESC LIMIT 60",
    )
    .bind(&symbol)
    .fetch_all(&st.pool)
    .await?;
    let crowd = json!({"bulls": bulls, "bears": bears, "posts24h": texts.len(), "bullPct": if bulls + bears > 0 { json!((bulls as f64 * 1000.0 / (bulls + bears) as f64).round() / 10.0) } else { Value::Null }});
    let summary = if texts.len() >= 3 && st.ai() && st.limiter.hit(&format!("sentiment:{}", me.id()), 60, Duration::from_secs(3600)) {
        crate::ai::sentiment(&st, &symbol, &texts, bulls, bears).await.ok()
    } else {
        None
    };
    Ok(Json(json!({"symbol": symbol, "crowd": crowd, "summary": summary, "riskLine": crate::text::RISK_LINE})))
}

/// Daily digest of the member's feed (AI, cached per member per day).
pub async fn digest(State(st): State<AppState>, me: Me) -> R {
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT p.id, p.body, a.display_name FROM posts p JOIN profiles a ON a.user_id = p.author
         WHERE p.status = 'published' AND p.published_at > now() - interval '24 hours' AND p.kind <> 'repost' AND length(p.body) > 20
           AND p.author IN (SELECT followee FROM follows WHERE follower = $1 AND status = 'active') AND {}
         ORDER BY (p.likes + p.bulls + p.bears + 2 * p.comments + 3 * p.reposts) DESC LIMIT 40",
        crate::posts::VISIBLE
    )))
    .bind(me.id())
    .fetch_all(&st.pool)
    .await?;
    let posts: Vec<(i64, String, String)> = rows.iter().map(|r| (r.get("id"), r.get("display_name"), r.get("body"))).collect();
    let trending: Vec<String> = feeds::trending(&st, "hashtag", 5).await?.iter().chain(feeds::trending(&st, "cashtag", 5).await?.iter()).filter_map(|t| t["tag"].as_str().map(str::to_string)).collect();
    if posts.is_empty() {
        return Ok(Json(json!({"digest": null, "trending": trending, "reason": "quiet"})));
    }
    if !st.ai() {
        return Ok(Json(json!({"digest": null, "trending": trending, "reason": "ai_unavailable", "postIds": posts.iter().take(6).map(|p| p.0).collect::<Vec<_>>()})));
    }
    if !st.limiter.hit(&format!("digest:{}", me.id()), 10, Duration::from_secs(3600)) {
        return Err(ApiError::RateLimited("Please try again later.".into()));
    }
    match crate::ai::digest(&st, me.id(), &me.p.lang, &posts, &trending).await {
        Ok(d) => Ok(Json(json!({"digest": d, "trending": trending}))),
        Err(e) => Err(ApiError::Unavailable(format!("The digest isn't available right now ({e})."))),
    }
}
