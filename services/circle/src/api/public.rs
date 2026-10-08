//! Public read-only pages for the website (Q3, Q51): a public profile and its public posts, one public post.
//! No token; only public content of public profiles; no relationships, no broker, no counts of private data.
//! Served at `https://api.kalkstrade.com/circle/public/*` (Caddy) and cached briefly by the website.

use super::Page;
use crate::error::{ApiError, ApiResult};
use crate::posts::{self, PUBLIC};
use crate::profiles;
use crate::state::AppState;
use axum::Json;
use axum::extract::{Path, Query, State};
use serde_json::{Value, json};

type R = ApiResult<Json<Value>>;

async fn public_profile(st: &AppState, handle: &str) -> ApiResult<profiles::Profile> {
    let p = profiles::by_handle(st, handle).await?;
    if p.is_banned() || p.shadow_hidden {
        return Err(ApiError::NotFound);
    }
    Ok(p)
}

pub async fn profile(State(st): State<AppState>, Path(handle): Path<String>) -> R {
    let p = public_profile(&st, &handle).await?;
    let mut v = profiles::card(&st, &p);
    v["bio"] = json!(if p.private { String::new() } else { p.bio.clone() });
    v["cover"] = if p.private { Value::Null } else { profiles::image_urls(&st, &p.cover) };
    v["counts"] = json!({"followers": p.followers_count, "following": p.following_count, "posts": p.posts_count});
    v["stats"] = if p.show_stats && !p.private { profiles::stats_public(&p.stats.0, p.stats_at) } else { Value::Null };
    v["joinedAt"] = json!(p.created_at);
    v["url"] = json!(format!("/circle/@{}", p.handle));
    v["join"] = json!({"title": "Join Kalks Circle", "url": "/register"});
    Ok(Json(json!({"profile": v})))
}

pub async fn posts(State(st): State<AppState>, Path(handle): Path<String>, Query(page): Query<Page>) -> R {
    let p = public_profile(&st, &handle).await?;
    if p.private {
        return Ok(Json(json!({"items": [], "nextCursor": null, "private": true})));
    }
    let limit = page.limit(20, 50);
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT p.* FROM posts p JOIN profiles a ON a.user_id = p.author WHERE p.author = $1 AND p.kind <> 'repost' AND {PUBLIC} AND p.id < $2 ORDER BY p.id DESC LIMIT $3"
    )))
    .bind(p.user_id)
    .bind(page.before_id())
    .bind(limit + 1)
    .fetch_all(&st.pool)
    .await?;
    let items = posts::render_for(&st, 0, None, &rows, true).await?.into_iter().map(public_post).collect();
    Ok(Json(super::paged(items, limit, |v| v["id"].as_i64().map(|i| i.to_string()))))
}

pub async fn post(State(st): State<AppState>, Path(id): Path<i64>) -> R {
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT p.* FROM posts p JOIN profiles a ON a.user_id = p.author WHERE p.id = $1 AND {PUBLIC}"))).bind(id).fetch_all(&st.pool).await?;
    let v = posts::render_for(&st, 0, None, &rows, true).await?.into_iter().next().ok_or(ApiError::NotFound)?;
    Ok(Json(json!({"post": public_post(v), "join": {"title": "Join Kalks Circle", "url": "/register"}})))
}

/// Drops viewer-specific fields from a rendered post.
fn public_post(mut v: Value) -> Value {
    if let Some(o) = v.as_object_mut() {
        o.remove("viewer");
        o.remove("copyable");
    }
    v
}
