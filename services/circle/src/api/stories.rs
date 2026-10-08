//! Stories: create, tray, a member's live stories, views and viewers, sentiment votes, questions, replies (DM),
//! archive and highlights.

use super::{Body, Me};
use crate::error::{ApiError, ApiResult, conflict, invalid};
use crate::state::AppState;
use crate::{profiles, stories};
use axum::Json;
use axum::extract::{Path, State};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::Row;

type R = ApiResult<Json<Value>>;

async fn story_row(st: &AppState, id: i64) -> ApiResult<sqlx::postgres::PgRow> {
    sqlx::query("SELECT * FROM stories WHERE id = $1 AND status NOT IN ('deleted','removed')").bind(id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)
}

/// The story if `me` may see it now (live) or through one of the author's highlights.
async fn seeable(st: &AppState, me: &Me, id: i64) -> ApiResult<sqlx::postgres::PgRow> {
    let r = story_row(st, id).await?;
    let in_highlight: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM highlight_items WHERE story_id = $1)").bind(id).fetch_one(&st.pool).await?;
    if stories::can_see(st, me.id(), &r, in_highlight).await? { Ok(r) } else { Err(ApiError::NotFound) }
}

pub async fn create(State(st): State<AppState>, me: Me, Body(b): Body) -> R {
    let id = stories::create(&st, &me, &b).await?;
    let r = story_row(&st, id).await?;
    Ok(Json(json!({"story": stories::render(&st, me.id(), &[r]).await?.remove(0)})))
}

pub async fn tray(State(st): State<AppState>, me: Me) -> R {
    Ok(Json(stories::tray(&st, &me).await?))
}

pub async fn of_user(State(st): State<AppState>, me: Me, Path(handle): Path<String>) -> R {
    let p = profiles::by_handle(&st, &handle).await?;
    if !profiles::can_view(&st, me.id(), &p).await? {
        return Err(crate::error::denied("private_profile", "This profile is private."));
    }
    let rows = stories::live_of(&st, me.id(), p.user_id).await?;
    Ok(Json(json!({"author": profiles::card(&st, &p), "items": stories::render(&st, me.id(), &rows).await?})))
}

pub async fn get(State(st): State<AppState>, me: Me, Path(id): Path<i64>) -> R {
    let r = seeable(&st, &me, id).await?;
    Ok(Json(json!({"story": stories::render(&st, me.id(), &[r]).await?.remove(0)})))
}

pub async fn remove(State(st): State<AppState>, me: Me, Path(id): Path<i64>) -> R {
    let r = story_row(&st, id).await?;
    if r.get::<i64, _>("author") != me.id() {
        return Err(ApiError::Forbidden("Only the author can delete a story.".into()));
    }
    sqlx::query("UPDATE stories SET status = 'deleted' WHERE id = $1").bind(id).execute(&st.pool).await?;
    sqlx::query("DELETE FROM highlight_items WHERE story_id = $1").bind(id).execute(&st.pool).await?;
    if let Some(m) = r.get::<Option<i64>, _>("media") {
        crate::media::purge(&st, m).await?;
    }
    Ok(Json(json!({"status": "deleted"})))
}

pub async fn view(State(st): State<AppState>, me: Me, Path(id): Path<i64>) -> R {
    let r = seeable(&st, &me, id).await?;
    if r.get::<i64, _>("author") != me.id() && sqlx::query("INSERT INTO story_views (story_id, viewer) VALUES ($1,$2) ON CONFLICT DO NOTHING").bind(id).bind(me.id()).execute(&st.pool).await?.rows_affected() > 0 {
        sqlx::query("UPDATE stories SET views = views + 1 WHERE id = $1").bind(id).execute(&st.pool).await?;
    }
    Ok(Json(json!({"status": "ok"})))
}

pub async fn viewers(State(st): State<AppState>, me: Me, Path(id): Path<i64>) -> R {
    let r = story_row(&st, id).await?;
    if r.get::<i64, _>("author") != me.id() {
        return Err(ApiError::NotFound);
    }
    let rows = sqlx::query("SELECT v.viewer, v.at, sv.choice FROM story_views v LEFT JOIN story_votes sv ON sv.story_id = v.story_id AND sv.user_id = v.viewer WHERE v.story_id = $1 ORDER BY v.at DESC LIMIT 1000").bind(id).fetch_all(&st.pool).await?;
    let ids: Vec<i64> = rows.iter().map(|r| r.get("viewer")).collect();
    let cards = profiles::cards(&st, &ids).await?;
    Ok(Json(json!({"count": r.get::<i64, _>("views"), "items": rows.iter().filter_map(|r| Some(json!({"user": cards.get(&r.get::<i64, _>("viewer"))?, "at": r.get::<DateTime<Utc>, _>("at"), "vote": r.get::<Option<String>, _>("choice")}))).collect::<Vec<_>>()})))
}

pub async fn vote(State(st): State<AppState>, me: Me, Path(id): Path<i64>, Body(b): Body) -> R {
    let r = seeable(&st, &me, id).await?;
    let has: bool = r.get::<sqlx::types::Json<Value>, _>("stickers").0.as_array().is_some_and(|a| a.iter().any(|s| s["type"] == "sentiment"));
    if !has {
        return Err(invalid("id", "This story has no bull / bear sticker."));
    }
    let choice = match b["choice"].as_str() {
        Some("bull") => "bull",
        Some("bear") => "bear",
        _ => return Err(invalid("choice", "choice is bull or bear.")),
    };
    if sqlx::query("INSERT INTO story_votes (story_id, user_id, choice) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING").bind(id).bind(me.id()).bind(choice).execute(&st.pool).await?.rows_affected() == 0 {
        return Err(conflict("already_voted", "You already voted."));
    }
    let t = sqlx::query("SELECT count(*) FILTER (WHERE choice = 'bull') AS b, count(*) FILTER (WHERE choice = 'bear') AS s FROM story_votes WHERE story_id = $1").bind(id).fetch_one(&st.pool).await?;
    Ok(Json(json!({"sentiment": {"bulls": t.get::<i64, _>("b"), "bears": t.get::<i64, _>("s"), "myVote": choice}})))
}

pub async fn answer(State(st): State<AppState>, me: Me, Path(id): Path<i64>, Body(b): Body) -> R {
    let r = seeable(&st, &me, id).await?;
    let has: bool = r.get::<sqlx::types::Json<Value>, _>("stickers").0.as_array().is_some_and(|a| a.iter().any(|s| s["type"] == "question"));
    if !has {
        return Err(invalid("id", "This story has no question sticker."));
    }
    let body = crate::util::clean(b["body"].as_str().unwrap_or(""), 300);
    if body.is_empty() {
        return Err(invalid("body", "Write an answer."));
    }
    crate::moderation::precheck(&st, &body).await?;
    if !st.limiter.hit(&format!("answer:{}", me.id()), 30, std::time::Duration::from_secs(3600)) {
        return Err(ApiError::RateLimited("Please wait a little.".into()));
    }
    sqlx::query("INSERT INTO story_answers (story_id, user_id, body) VALUES ($1,$2,$3)").bind(id).bind(me.id()).bind(&body).execute(&st.pool).await?;
    crate::notify::event(&st, crate::notify::Ev::new(r.get("author"), "story_answer", me.id()).story(id).data(json!({"preview": body}))).await?;
    Ok(Json(json!({"status": "sent"})))
}

pub async fn answers(State(st): State<AppState>, me: Me, Path(id): Path<i64>) -> R {
    let r = story_row(&st, id).await?;
    if r.get::<i64, _>("author") != me.id() {
        return Err(ApiError::NotFound);
    }
    let rows = sqlx::query("SELECT * FROM story_answers WHERE story_id = $1 ORDER BY at DESC LIMIT 500").bind(id).fetch_all(&st.pool).await?;
    let ids: Vec<i64> = rows.iter().map(|r| r.get("user_id")).collect();
    let cards = profiles::cards(&st, &ids).await?;
    Ok(Json(json!({"items": rows.iter().map(|r| json!({"id": r.get::<i64, _>("id"), "user": cards.get(&r.get::<i64, _>("user_id")), "body": r.get::<String, _>("body"), "at": r.get::<DateTime<Utc>, _>("at")})).collect::<Vec<_>>()})))
}

/// Replies to a story in the author's DMs (a request if they don't follow the sender).
pub async fn reply(State(st): State<AppState>, me: Me, Path(id): Path<i64>, Body(b): Body) -> R {
    let r = seeable(&st, &me, id).await?;
    let author = profiles::by_id(&st, r.get("author")).await?.ok_or(ApiError::NotFound)?;
    if author.user_id == me.id() {
        return Err(invalid("id", "This is your own story."));
    }
    let conv = crate::chat::open_dm(&st, &me, &author).await?;
    let msg = crate::chat::send(&st, &me, conv, &json!({"body": b["body"], "storyId": id, "clientId": b["clientId"]})).await?;
    Ok(Json(json!({"conversationId": conv, "message": msg})))
}

pub async fn archive(State(st): State<AppState>, me: Me, axum::extract::Query(page): axum::extract::Query<super::Page>) -> R {
    let limit = page.limit(30, 100);
    let rows = sqlx::query("SELECT * FROM stories WHERE author = $1 AND status NOT IN ('deleted') AND id < $2 ORDER BY id DESC LIMIT $3").bind(me.id()).bind(page.before_id()).bind(limit + 1).fetch_all(&st.pool).await?;
    let items = stories::render(&st, me.id(), &rows).await?;
    Ok(Json(super::paged(items, limit, |v| v["id"].as_i64().map(|i| i.to_string()))))
}

// ---------------------------------------------------------------- highlights

async fn highlight_json(st: &AppState, viewer: i64, h: &sqlx::postgres::PgRow, with_items: bool) -> ApiResult<Value> {
    let id: i64 = h.get("id");
    let cover_story: Option<i64> = h.get("cover_story");
    let cover = match cover_story {
        Some(c) => {
            let rows = sqlx::query("SELECT * FROM stories WHERE id = $1 AND status = 'published'").bind(c).fetch_all(&st.pool).await?;
            stories::render(st, viewer, &rows).await?.into_iter().next().and_then(|s| s["media"]["urls"]["thumb"].as_str().or(s["media"]["urls"]["poster"].as_str()).map(str::to_string))
        }
        None => None,
    };
    let mut v = json!({"id": id, "title": h.get::<String, _>("title"), "cover": cover, "position": h.get::<i32, _>("position")});
    if with_items {
        let rows = sqlx::query("SELECT s.* FROM highlight_items i JOIN stories s ON s.id = i.story_id WHERE i.highlight_id = $1 AND s.status = 'published' ORDER BY i.position, s.created_at").bind(id).fetch_all(&st.pool).await?;
        let mut visible = Vec::new();
        for r in rows {
            if stories::can_see(st, viewer, &r, true).await? {
                visible.push(r);
            }
        }
        v["items"] = json!(stories::render(st, viewer, &visible).await?);
    } else {
        v["count"] = json!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM highlight_items WHERE highlight_id = $1").bind(id).fetch_one(&st.pool).await?);
    }
    Ok(v)
}

pub async fn highlights_of(State(st): State<AppState>, me: Me, Path(handle): Path<String>) -> R {
    let p = profiles::by_handle(&st, &handle).await?;
    if !profiles::can_view(&st, me.id(), &p).await? {
        return Ok(Json(json!({"items": []})));
    }
    let rows = sqlx::query("SELECT * FROM highlights WHERE owner = $1 ORDER BY position, id").bind(p.user_id).fetch_all(&st.pool).await?;
    let mut items = Vec::new();
    for h in &rows {
        items.push(highlight_json(&st, me.id(), h, false).await?);
    }
    Ok(Json(json!({"items": items})))
}

async fn own_story_ids(st: &AppState, me: &Me, v: &Value) -> ApiResult<Vec<i64>> {
    let ids: Vec<i64> = v.as_array().map(|a| a.iter().filter_map(Value::as_i64).take(100).collect()).unwrap_or_default();
    let ok: i64 = sqlx::query_scalar("SELECT count(*) FROM stories WHERE id = ANY($1) AND author = $2 AND status = 'published'").bind(&ids).bind(me.id()).fetch_one(&st.pool).await?;
    if ok as usize != ids.len() {
        return Err(invalid("storyIds", "Add your own published stories."));
    }
    Ok(ids)
}

pub async fn create_highlight(State(st): State<AppState>, me: Me, Body(b): Body) -> R {
    let title = crate::util::clean(b["title"].as_str().unwrap_or(""), 30);
    if title.is_empty() {
        return Err(invalid("title", "Name the highlight."));
    }
    let ids = own_story_ids(&st, &me, &b["storyIds"]).await?;
    if ids.is_empty() {
        return Err(invalid("storyIds", "Add at least one story."));
    }
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM highlights WHERE owner = $1").bind(me.id()).fetch_one(&st.pool).await?;
    if n >= 50 {
        return Err(invalid("title", "Up to 50 highlights."));
    }
    let cover = b["coverStoryId"].as_i64().filter(|c| ids.contains(c)).or(ids.first().copied());
    let id: i64 = sqlx::query_scalar("INSERT INTO highlights (owner, title, cover_story, position) VALUES ($1,$2,$3,$4) RETURNING id").bind(me.id()).bind(&title).bind(cover).bind(n as i32).fetch_one(&st.pool).await?;
    for (i, s) in ids.iter().enumerate() {
        sqlx::query("INSERT INTO highlight_items (highlight_id, story_id, position) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING").bind(id).bind(s).bind(i as i32).execute(&st.pool).await?;
    }
    let h = sqlx::query("SELECT * FROM highlights WHERE id = $1").bind(id).fetch_one(&st.pool).await?;
    Ok(Json(json!({"highlight": highlight_json(&st, me.id(), &h, true).await?})))
}

pub async fn highlight(State(st): State<AppState>, me: Me, Path(id): Path<i64>) -> R {
    let h = sqlx::query("SELECT * FROM highlights WHERE id = $1").bind(id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    let owner = profiles::by_id(&st, h.get("owner")).await?.ok_or(ApiError::NotFound)?;
    if !profiles::can_view(&st, me.id(), &owner).await? {
        return Err(ApiError::NotFound);
    }
    Ok(Json(json!({"highlight": highlight_json(&st, me.id(), &h, true).await?, "owner": profiles::card(&st, &owner)})))
}

pub async fn edit_highlight(State(st): State<AppState>, me: Me, Path(id): Path<i64>, Body(b): Body) -> R {
    let h = sqlx::query("SELECT * FROM highlights WHERE id = $1 AND owner = $2").bind(id).bind(me.id()).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    if let Some(t) = b["title"].as_str() {
        let t = crate::util::clean(t, 30);
        if t.is_empty() {
            return Err(invalid("title", "Name the highlight."));
        }
        sqlx::query("UPDATE highlights SET title = $2 WHERE id = $1").bind(id).bind(t).execute(&st.pool).await?;
    }
    if b.get("storyIds").is_some() {
        let ids = own_story_ids(&st, &me, &b["storyIds"]).await?;
        sqlx::query("DELETE FROM highlight_items WHERE highlight_id = $1").bind(id).execute(&st.pool).await?;
        for (i, s) in ids.iter().enumerate() {
            sqlx::query("INSERT INTO highlight_items (highlight_id, story_id, position) VALUES ($1,$2,$3)").bind(id).bind(s).bind(i as i32).execute(&st.pool).await?;
        }
    }
    if let Some(c) = b["coverStoryId"].as_i64() {
        sqlx::query("UPDATE highlights SET cover_story = $2 WHERE id = $1 AND EXISTS (SELECT 1 FROM highlight_items WHERE highlight_id = $1 AND story_id = $2)").bind(id).bind(c).execute(&st.pool).await?;
    }
    if let Some(p) = b["position"].as_i64() {
        sqlx::query("UPDATE highlights SET position = $2 WHERE id = $1").bind(id).bind(p.clamp(0, 1000) as i32).execute(&st.pool).await?;
    }
    let _ = h;
    let h = sqlx::query("SELECT * FROM highlights WHERE id = $1").bind(id).fetch_one(&st.pool).await?;
    Ok(Json(json!({"highlight": highlight_json(&st, me.id(), &h, true).await?})))
}

pub async fn delete_highlight(State(st): State<AppState>, me: Me, Path(id): Path<i64>) -> R {
    sqlx::query("DELETE FROM highlights WHERE id = $1 AND owner = $2").bind(id).bind(me.id()).execute(&st.pool).await?;
    Ok(Json(json!({"status": "deleted"})))
}
