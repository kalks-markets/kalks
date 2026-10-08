//! Posts, comments, reactions, reposts, saves and collections, polls, trade cards, "Copy this trade",
//! translation, the post helper and reports.

use super::{Body, Me, Page, paged};
use crate::error::{ApiError, ApiResult, denied, invalid};
use crate::posts;
use crate::state::AppState;
use crate::tradecards;
use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use std::time::Duration;

type R = ApiResult<Json<Value>>;

async fn one(st: &AppState, me: &Me, id: i64) -> ApiResult<Value> {
    let r = posts::visible(st, me.id(), id).await?;
    Ok(posts::render(st, me, &[r]).await?.remove(0))
}

pub async fn create(State(st): State<AppState>, me: Me, Body(b): Body) -> R {
    let id = posts::create(&st, &me, &b).await?;
    Ok(Json(json!({"post": one(&st, &me, id).await?})))
}

pub async fn get(State(st): State<AppState>, me: Me, Path(id): Path<i64>) -> R {
    let post = one(&st, &me, id).await?;
    let comments = posts::list_comments(&st, &me, id, None, i64::MAX, 3).await.unwrap_or(json!({"items": []}));
    Ok(Json(json!({"post": post, "comments": comments})))
}

pub async fn edit(State(st): State<AppState>, me: Me, Path(id): Path<i64>, Body(b): Body) -> R {
    posts::edit(&st, &me, id, &b).await?;
    Ok(Json(json!({"post": one(&st, &me, id).await?})))
}

pub async fn remove(State(st): State<AppState>, me: Me, Path(id): Path<i64>) -> R {
    posts::delete(&st, &me, id).await?;
    Ok(Json(json!({"status": "deleted"})))
}

#[derive(Deserialize)]
pub struct KindQ {
    kind: Option<String>,
}

pub async fn react(State(st): State<AppState>, me: Me, Path(id): Path<i64>, Body(b): Body) -> R {
    Ok(Json(json!({"counts": posts::react(&st, &me, id, b["kind"].as_str().unwrap_or("like")).await?})))
}

pub async fn unreact(State(st): State<AppState>, me: Me, Path(id): Path<i64>, Query(q): Query<KindQ>) -> R {
    Ok(Json(json!({"counts": posts::unreact(&st, &me, id, q.kind.as_deref().unwrap_or("like")).await?})))
}

pub async fn reactions(State(st): State<AppState>, me: Me, Path(id): Path<i64>, Query(q): Query<KindQ>, Query(page): Query<Page>) -> R {
    posts::visible(&st, me.id(), id).await?;
    let kind = q.kind.unwrap_or_else(|| "like".into());
    let limit = page.limit(50, 200);
    let before = page.cursor.as_deref().and_then(|c| c.parse::<i64>().ok()).unwrap_or(i64::MAX);
    let rows = sqlx::query(
        "SELECT r.user_id, (extract(epoch FROM r.created_at) * 1000000)::bigint AS k FROM reactions r JOIN profiles a ON a.user_id = r.user_id
         WHERE r.post_id = $1 AND r.kind = $2 AND a.status = 'active' AND (extract(epoch FROM r.created_at) * 1000000)::bigint < $3
           AND NOT EXISTS (SELECT 1 FROM blocks b WHERE (b.blocker = $4 AND b.blocked = r.user_id) OR (b.blocker = r.user_id AND b.blocked = $4))
         ORDER BY k DESC LIMIT $5",
    )
    .bind(id)
    .bind(&kind)
    .bind(before)
    .bind(me.id())
    .bind(limit + 1)
    .fetch_all(&st.pool)
    .await?;
    let ids: Vec<i64> = rows.iter().map(|r| r.get("user_id")).collect();
    let cards = crate::profiles::cards(&st, &ids).await?;
    let items: Vec<Value> = rows.iter().filter_map(|r| {
        let mut c = cards.get(&r.get::<i64, _>("user_id"))?.clone();
        c["cursor"] = json!(r.get::<i64, _>("k").to_string());
        Some(c)
    }).collect();
    Ok(Json(paged(items, limit, |v| v["cursor"].as_str().map(str::to_string))))
}

pub async fn repost(State(st): State<AppState>, me: Me, Path(id): Path<i64>) -> R {
    let rid = posts::repost(&st, &me, id).await?;
    Ok(Json(json!({"repostId": rid, "counts": posts::counts(&st, id).await?})))
}

pub async fn unrepost(State(st): State<AppState>, me: Me, Path(id): Path<i64>) -> R {
    posts::unrepost(&st, &me, id).await?;
    Ok(Json(json!({"counts": posts::counts(&st, id).await?})))
}

pub async fn save(State(st): State<AppState>, me: Me, Path(id): Path<i64>, Body(b): Body) -> R {
    posts::save(&st, &me, id, b["collectionId"].as_i64()).await?;
    Ok(Json(json!({"saved": true, "counts": posts::counts(&st, id).await?})))
}

pub async fn unsave(State(st): State<AppState>, me: Me, Path(id): Path<i64>) -> R {
    posts::unsave(&st, &me, id).await?;
    Ok(Json(json!({"saved": false, "counts": posts::counts(&st, id).await?})))
}

pub async fn vote(State(st): State<AppState>, me: Me, Path(id): Path<i64>, Body(b): Body) -> R {
    let opt = b["option"].as_i64().ok_or_else(|| invalid("option", "option is the 0-based index."))?;
    Ok(Json(json!({"poll": posts::vote(&st, &me, id, opt).await?})))
}

pub async fn pin(State(st): State<AppState>, me: Me, Path(id): Path<i64>) -> R {
    let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM posts WHERE id = $1 AND author = $2 AND status = 'published')").bind(id).bind(me.id()).fetch_one(&st.pool).await?;
    if !ok {
        return Err(ApiError::NotFound);
    }
    sqlx::query("UPDATE profiles SET pinned_post = $2 WHERE user_id = $1").bind(me.id()).bind(id).execute(&st.pool).await?;
    Ok(Json(json!({"pinnedPost": id})))
}

pub async fn unpin(State(st): State<AppState>, me: Me, Path(id): Path<i64>) -> R {
    sqlx::query("UPDATE profiles SET pinned_post = NULL WHERE user_id = $1 AND pinned_post = $2").bind(me.id()).bind(id).execute(&st.pool).await?;
    Ok(Json(json!({"pinnedPost": null})))
}

pub async fn views(State(st): State<AppState>, me: Me, Body(b): Body) -> R {
    let ids: Vec<i64> = b["ids"].as_array().map(|a| a.iter().filter_map(Value::as_i64).take(100).collect()).unwrap_or_default();
    if !ids.is_empty() {
        let fresh: Vec<i64> = sqlx::query_scalar("INSERT INTO seen (user_id, post_id) SELECT $1, unnest($2::bigint[]) ON CONFLICT DO NOTHING RETURNING post_id").bind(me.id()).bind(&ids).fetch_all(&st.pool).await?;
        if !fresh.is_empty() {
            sqlx::query("UPDATE posts SET views = views + 1 WHERE id = ANY($1) AND author <> $2").bind(&fresh).bind(me.id()).execute(&st.pool).await?;
        }
    }
    Ok(Json(json!({"status": "ok"})))
}

/// "Copy this trade": prefilled ticket data, same broker only.
pub async fn copy(State(st): State<AppState>, me: Me, Path(id): Path<i64>) -> R {
    let p = posts::visible(&st, me.id(), id).await?;
    let card = p.get::<Option<i64>, _>("trade_card").ok_or_else(|| invalid("id", "This post has no trade."))?;
    let r = tradecards::get(&st, card).await?;
    if r.get::<String, _>("tenant") != me.p.tenant {
        return Err(denied("other_broker", "Copying is available between clients of the same broker."));
    }
    if r.get::<i64, _>("owner") == me.id() {
        return Err(invalid("id", "This is your own trade."));
    }
    Ok(Json(json!({"copy": tradecards::copy_payload(&r)})))
}

async fn translated(st: &AppState, me: &Me, text: &str, lang: Option<&str>) -> ApiResult<Value> {
    let lang = lang.and_then(crate::text::valid_lang).unwrap_or(&me.p.lang).to_string();
    if text.trim().is_empty() {
        return Ok(json!({"lang": lang, "text": ""}));
    }
    if !st.limiter.hit(&format!("translate:{}", me.id()), 120, Duration::from_secs(3600)) {
        return Err(ApiError::RateLimited("Too many translations. Please try again later.".into()));
    }
    match crate::ai::translate(st, text, &lang).await {
        Ok(t) => Ok(json!({"lang": lang, "text": t})),
        Err(crate::ai::AiError::NotConfigured) => Err(ApiError::Unavailable("Translation isn't available.".into())),
        Err(e) => Err(ApiError::Unavailable(format!("Translation failed ({e}).")))
    }
}

pub async fn translate(State(st): State<AppState>, me: Me, Path(id): Path<i64>, Body(b): Body) -> R {
    let p = posts::visible(&st, me.id(), id).await?;
    Ok(Json(json!({"translation": translated(&st, &me, &p.get::<String, _>("body"), b["lang"].as_str()).await?})))
}

pub async fn translate_comment(State(st): State<AppState>, me: Me, Path(id): Path<i64>, Body(b): Body) -> R {
    let c = sqlx::query("SELECT post_id, body, status FROM comments WHERE id = $1").bind(id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    posts::visible(&st, me.id(), c.get("post_id")).await?;
    if c.get::<String, _>("status") != "published" {
        return Err(ApiError::NotFound);
    }
    Ok(Json(json!({"translation": translated(&st, &me, &c.get::<String, _>("body"), b["lang"].as_str()).await?})))
}

/// Post helper: a caption and a beginner explanation from a trade card or a chart snapshot (+ notes).
pub async fn caption(State(st): State<AppState>, me: Me, Body(b): Body) -> R {
    if !st.limiter.hit(&format!("caption:{}", me.id()), 20, Duration::from_secs(3600)) {
        return Err(ApiError::RateLimited("The post helper can be used 20 times per hour.".into()));
    }
    let mut context = json!({});
    if let Some(c) = b["tradeCardId"].as_i64() {
        let r = tradecards::get(&st, c).await?;
        if r.get::<i64, _>("owner") != me.id() {
            return Err(ApiError::NotFound);
        }
        context["trade"] = tradecards::json(&st, &r, me.id(), None);
    }
    if let Some(m) = b["mediaId"].as_i64() {
        let chart: Option<sqlx::types::Json<Value>> = sqlx::query_scalar("SELECT chart FROM media WHERE id = $1 AND owner = $2").bind(m).bind(me.id()).fetch_optional(&st.pool).await?.flatten();
        if let Some(c) = chart {
            context["chart"] = json!({"symbol": c.0["symbol"], "timeframe": c.0["timeframe"], "indicators": c.0["indicators"], "drawings": c.0["drawings"].as_array().map(|a| a.len()).unwrap_or(0)});
        }
    }
    if context.as_object().is_some_and(|o| o.is_empty()) && b["notes"].as_str().unwrap_or("").trim().is_empty() {
        return Err(invalid("tradeCardId", "Give a trade card, a chart snapshot or some notes."));
    }
    let lang = b["lang"].as_str().and_then(crate::text::valid_lang).unwrap_or(&me.p.lang).to_string();
    match crate::ai::caption(&st, &context, b["notes"].as_str().unwrap_or(""), &lang).await {
        Ok(v) => Ok(Json(json!({"suggestion": v, "riskLine": crate::text::RISK_LINE}))),
        Err(crate::ai::AiError::NotConfigured) => Err(ApiError::Unavailable("The post helper isn't available.".into())),
        Err(e) => Err(ApiError::Unavailable(format!("The post helper failed ({e})."))),
    }
}

// ---------------------------------------------------------------- comments

#[derive(Deserialize)]
pub struct CommentsQ {
    cursor: Option<String>,
    limit: Option<i64>,
    parent: Option<i64>,
}

pub async fn comments(State(st): State<AppState>, me: Me, Path(id): Path<i64>, Query(q): Query<CommentsQ>) -> R {
    let limit = crate::util::clamp_limit(q.limit, 20, 100);
    let before = q.cursor.as_deref().and_then(|c| c.parse::<i64>().ok()).unwrap_or(i64::MAX);
    Ok(Json(posts::list_comments(&st, &me, id, q.parent, before, limit).await?))
}

pub async fn comment(State(st): State<AppState>, me: Me, Path(id): Path<i64>, Body(b): Body) -> R {
    let cid = posts::add_comment(&st, &me, id, &b).await?;
    let rows = sqlx::query("SELECT * FROM comments WHERE id = $1").bind(cid).fetch_all(&st.pool).await?;
    Ok(Json(json!({"comment": posts::comment_json(&st, me.id(), &rows).await?.remove(0)})))
}

pub async fn edit_comment(State(st): State<AppState>, me: Me, Path(id): Path<i64>, Body(b): Body) -> R {
    posts::edit_comment(&st, &me, id, b["body"].as_str().unwrap_or("")).await?;
    let rows = sqlx::query("SELECT * FROM comments WHERE id = $1").bind(id).fetch_all(&st.pool).await?;
    Ok(Json(json!({"comment": posts::comment_json(&st, me.id(), &rows).await?.remove(0)})))
}

pub async fn delete_comment(State(st): State<AppState>, me: Me, Path(id): Path<i64>) -> R {
    posts::delete_comment(&st, &me, id).await?;
    Ok(Json(json!({"status": "deleted"})))
}

pub async fn like_comment(State(st): State<AppState>, me: Me, Path(id): Path<i64>) -> R {
    Ok(Json(posts::like_comment(&st, &me, id, true).await?))
}

pub async fn unlike_comment(State(st): State<AppState>, me: Me, Path(id): Path<i64>) -> R {
    Ok(Json(posts::like_comment(&st, &me, id, false).await?))
}

pub async fn pin_comment(State(st): State<AppState>, me: Me, Path((id, cid)): Path<(i64, i64)>) -> R {
    posts::pin_comment(&st, &me, id, Some(cid)).await?;
    Ok(Json(json!({"pinnedComment": cid})))
}

pub async fn unpin_comment(State(st): State<AppState>, me: Me, Path((id, _cid)): Path<(i64, i64)>) -> R {
    posts::pin_comment(&st, &me, id, None).await?;
    Ok(Json(json!({"pinnedComment": null})))
}

// ---------------------------------------------------------------- saved & collections

#[derive(Deserialize)]
pub struct SavedQ {
    collection: Option<i64>,
    cursor: Option<String>,
    limit: Option<i64>,
}

pub async fn saved(State(st): State<AppState>, me: Me, Query(q): Query<SavedQ>) -> R {
    let limit = crate::util::clamp_limit(q.limit, 20, 50);
    let before = q.cursor.as_deref().and_then(|c| c.parse::<i64>().ok()).unwrap_or(i64::MAX);
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT p.*, (extract(epoch FROM s.created_at) * 1000000)::bigint AS saved_k FROM saves s JOIN posts p ON p.id = s.post_id JOIN profiles a ON a.user_id = p.author
         WHERE s.user_id = $1 AND ($2::bigint IS NULL OR s.collection_id = $2) AND (extract(epoch FROM s.created_at) * 1000000)::bigint < $3 AND {}
         ORDER BY saved_k DESC LIMIT $4",
        posts::VISIBLE
    )))
    .bind(me.id())
    .bind(q.collection)
    .bind(before)
    .bind(limit + 1)
    .fetch_all(&st.pool)
    .await?;
    let keys: Vec<i64> = rows.iter().map(|r| r.get("saved_k")).collect();
    let mut items = posts::render(&st, &me, &rows).await?;
    for (v, k) in items.iter_mut().zip(keys) {
        v["savedCursor"] = json!(k.to_string());
    }
    Ok(Json(paged(items, limit, |v| v["savedCursor"].as_str().map(str::to_string))))
}

pub async fn collections(State(st): State<AppState>, me: Me) -> R {
    let rows = sqlx::query("SELECT c.id, c.name, c.created_at, (SELECT count(*) FROM saves s WHERE s.collection_id = c.id) AS n FROM collections c WHERE c.user_id = $1 ORDER BY c.created_at").bind(me.id()).fetch_all(&st.pool).await?;
    let all: i64 = sqlx::query_scalar("SELECT count(*) FROM saves WHERE user_id = $1").bind(me.id()).fetch_one(&st.pool).await?;
    Ok(Json(json!({"all": all, "items": rows.iter().map(|r| json!({"id": r.get::<i64, _>("id"), "name": r.get::<String, _>("name"), "posts": r.get::<i64, _>("n"), "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at")})).collect::<Vec<_>>()})))
}

pub async fn create_collection(State(st): State<AppState>, me: Me, Body(b): Body) -> R {
    let name = crate::util::clean(b["name"].as_str().unwrap_or(""), 40);
    if name.is_empty() {
        return Err(invalid("name", "Name the collection."));
    }
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM collections WHERE user_id = $1").bind(me.id()).fetch_one(&st.pool).await?;
    if n >= 100 {
        return Err(invalid("name", "Up to 100 collections."));
    }
    let id: Option<i64> = sqlx::query_scalar("INSERT INTO collections (user_id, name) VALUES ($1,$2) ON CONFLICT DO NOTHING RETURNING id").bind(me.id()).bind(&name).fetch_optional(&st.pool).await?;
    let id = id.ok_or_else(|| crate::error::conflict("exists", "You already have a collection with this name."))?;
    Ok(Json(json!({"collection": {"id": id, "name": name, "posts": 0}})))
}

pub async fn rename_collection(State(st): State<AppState>, me: Me, Path(id): Path<i64>, Body(b): Body) -> R {
    let name = crate::util::clean(b["name"].as_str().unwrap_or(""), 40);
    if name.is_empty() {
        return Err(invalid("name", "Name the collection."));
    }
    let n = sqlx::query("UPDATE collections SET name = $3 WHERE id = $1 AND user_id = $2").bind(id).bind(me.id()).bind(&name).execute(&st.pool).await.map_err(|_| crate::error::conflict("exists", "You already have a collection with this name."))?.rows_affected();
    if n == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(Json(json!({"collection": {"id": id, "name": name}})))
}

pub async fn delete_collection(State(st): State<AppState>, me: Me, Path(id): Path<i64>) -> R {
    // saved posts stay saved (in "All")
    sqlx::query("DELETE FROM collections WHERE id = $1 AND user_id = $2").bind(id).bind(me.id()).execute(&st.pool).await?;
    Ok(Json(json!({"status": "deleted"})))
}

// ---------------------------------------------------------------- trade cards

pub async fn card_sources(State(st): State<AppState>, me: Me) -> R {
    Ok(Json(tradecards::sources(&st, &me.p.tenant, me.id()).await?))
}

pub async fn create_card(State(st): State<AppState>, me: Me, Body(b): Body) -> R {
    if !st.limiter.hit(&format!("card:{}", me.id()), 60, Duration::from_secs(3600)) {
        return Err(ApiError::RateLimited("Too many trade cards. Please try again later.".into()));
    }
    let id = tradecards::create(&st, &me.p.tenant, me.id(), &b).await?;
    let r = tradecards::get(&st, id).await?;
    Ok(Json(json!({"tradeCard": tradecards::json(&st, &r, me.id(), None)})))
}

/// Who may see a card: its owner, and anyone who can see a post / story / message that carries it.
async fn card_visible(st: &AppState, me: &Me, id: i64) -> ApiResult<sqlx::postgres::PgRow> {
    let r = tradecards::get(st, id).await?;
    if r.get::<i64, _>("owner") == me.id() {
        return Ok(r);
    }
    let post: Option<i64> = sqlx::query_scalar("SELECT id FROM posts WHERE trade_card = $1 AND status = 'published' ORDER BY id LIMIT 1").bind(id).fetch_optional(&st.pool).await?;
    if let Some(p) = post
        && posts::visible(st, me.id(), p).await.is_ok()
    {
        return Ok(r);
    }
    let in_chat: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM messages m JOIN conv_members c ON c.conversation_id = m.conversation_id AND c.user_id = $2 AND c.state IN ('active','request') WHERE m.trade_card = $1 AND m.deleted_at IS NULL)")
        .bind(id)
        .bind(me.id())
        .fetch_one(&st.pool)
        .await?;
    let story: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM stories WHERE trade_card = $1 AND status = 'published' AND expires_at > now())").bind(id).fetch_one(&st.pool).await?;
    if in_chat || story {
        return Ok(r);
    }
    Err(ApiError::NotFound)
}

pub async fn get_card(State(st): State<AppState>, me: Me, Path(id): Path<i64>) -> R {
    let r = card_visible(&st, &me, id).await?;
    let sym: String = r.get("symbol");
    let q = crate::upstream::quotes(&st, std::slice::from_ref(&sym)).await;
    let mut v = tradecards::json(&st, &r, me.id(), q.get(&sym).and_then(crate::upstream::price_of));
    v["copyable"] = json!(r.get::<String, _>("tenant") == me.p.tenant && r.get::<i64, _>("owner") != me.id());
    Ok(Json(json!({"tradeCard": v})))
}

/// Fresh engine figures of an open card (and its close once the position is closed).
pub async fn live_card(State(st): State<AppState>, me: Me, Path(id): Path<i64>) -> R {
    card_visible(&st, &me, id).await?;
    let r = tradecards::refresh(&st, id).await?;
    let sym: String = r.get("symbol");
    let q = crate::upstream::quotes(&st, std::slice::from_ref(&sym)).await;
    Ok(Json(json!({"tradeCard": tradecards::json(&st, &r, me.id(), q.get(&sym).and_then(crate::upstream::price_of))})))
}

// ---------------------------------------------------------------- reports

pub const REPORT_REASONS: &[&str] = &["spam", "scam", "abuse", "hate", "nudity", "violence", "pnl_claim", "impersonation", "personal_data", "self_harm", "other"];

pub async fn report(State(st): State<AppState>, me: Me, Body(b): Body) -> R {
    let kind = b["targetKind"].as_str().unwrap_or("");
    let id = b["targetId"].as_i64().ok_or_else(|| invalid("targetId", "What are you reporting?"))?;
    let reason = b["reason"].as_str().unwrap_or("");
    if !REPORT_REASONS.contains(&reason) {
        return Err(invalid("reason", format!("reason is one of {}.", REPORT_REASONS.join(", "))));
    }
    if !st.limiter.hit(&format!("report:{}", me.id()), 30, Duration::from_secs(3600)) {
        return Err(ApiError::RateLimited("Too many reports. Please try again later.".into()));
    }
    let owner: i64 = match kind {
        "post" => posts::visible(&st, me.id(), id).await?.get("author"),
        "comment" => sqlx::query_scalar("SELECT author FROM comments WHERE id = $1").bind(id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?,
        "story" => sqlx::query_scalar("SELECT author FROM stories WHERE id = $1").bind(id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?,
        "profile" => crate::profiles::by_id(&st, id).await?.ok_or(ApiError::NotFound)?.user_id,
        "message" => {
            let r = sqlx::query("SELECT m.sender FROM messages m JOIN conv_members c ON c.conversation_id = m.conversation_id AND c.user_id = $2 WHERE m.id = $1").bind(id).bind(me.id()).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
            r.get("sender")
        }
        "conversation" => {
            crate::chat::require(&st, id, me.id(), &["active", "request", "left", "declined"]).await?;
            sqlx::query_scalar("SELECT COALESCE(owner, 0) FROM conversations WHERE id = $1").bind(id).fetch_one(&st.pool).await?
        }
        _ => return Err(invalid("targetKind", "targetKind is post, comment, story, message, profile or conversation.")),
    };
    if owner == me.id() {
        return Err(invalid("targetId", "You can't report your own content."));
    }
    let tenant: String = sqlx::query_scalar("SELECT tenant FROM profiles WHERE user_id = $1").bind(owner).fetch_optional(&st.pool).await?.unwrap_or_else(|| me.p.tenant.clone());
    let note = crate::util::clean(b["note"].as_str().unwrap_or(""), 500);
    let rid: Option<i64> = sqlx::query_scalar(
        "INSERT INTO reports (reporter, target_kind, target_id, target_owner, tenant, reason, note) VALUES ($1,$2,$3,$4,$5,$6,$7)
         ON CONFLICT (reporter, target_kind, target_id) WHERE status = 'open' DO NOTHING RETURNING id",
    )
    .bind(me.id())
    .bind(kind)
    .bind(id)
    .bind(owner)
    .bind(&tenant)
    .bind(reason)
    .bind(&note)
    .fetch_optional(&st.pool)
    .await?;
    if rid.is_some() && kind == "post" {
        let n: i32 = sqlx::query_scalar("UPDATE posts SET reports = reports + 1 WHERE id = $1 RETURNING reports").bind(id).fetch_one(&st.pool).await?;
        // three reports take a post down until a moderator looks at it
        if n >= 3 {
            let r = sqlx::query("UPDATE posts SET status = 'review' WHERE id = $1 AND status = 'published' RETURNING body").bind(id).fetch_optional(&st.pool).await?;
            if let Some(r) = r {
                let v = crate::moderation::Verdict { decision: crate::moderation::Decision::Review, categories: vec![], reason: format!("{n} reports"), source: "rules", model: None };
                crate::moderation::queue(&st, "post", id, owner, &tenant, "report", &v, &r.get::<String, _>("body")).await?;
            }
        }
    }
    st.hub.send(crate::state::Target::Staff, json!({"type": "report", "kind": kind, "id": id}));
    Ok(Json(json!({"status": "received", "reportId": rid, "message": "Thanks. Our team will look at this. You can also block or mute this member."})))
}
