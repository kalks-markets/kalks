//! Posts (Q9, Q12, Q21–Q24, Q36): text with #hashtags / $cashtags / @mentions, up to 10 photos or chart
//! snapshots, one video, a verified trade card, a poll, a quote of another post; reposts; reactions (like, and
//! bull / bear on symbol posts); saves into collections; the automatic risk line. New and edited posts are
//! `pending` until the safety check passes (moderation worker), then `published`.

use crate::api::Me;
use crate::error::{ApiError, ApiResult, conflict, denied, invalid};
use crate::profiles::{self, Excl, Profile};
use crate::state::AppState;
use crate::text;
use crate::{media, tradecards};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::Row;
use sqlx::postgres::PgRow;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

pub const MAX_BODY: usize = 3000;
pub const MAX_COMMENT: usize = 1000;

/// SQL condition: post `p` (author profile `a`) is visible to viewer `$1` in lists.
pub const VISIBLE: &str = "(p.author = $1 OR (p.status = 'published' AND NOT p.shadow AND NOT a.shadow_hidden
    AND NOT (a.status = 'banned' AND (a.banned_until IS NULL OR a.banned_until > now()))
    AND NOT EXISTS (SELECT 1 FROM blocks b WHERE (b.blocker = $1 AND b.blocked = p.author) OR (b.blocker = p.author AND b.blocked = $1))
    AND ((p.visibility = 'public' AND NOT a.private) OR EXISTS (SELECT 1 FROM follows f WHERE f.follower = $1 AND f.followee = p.author AND f.status = 'active'))))";

/// SQL condition for the public website (no viewer): public posts of public profiles.
pub const PUBLIC: &str = "(p.status = 'published' AND NOT p.shadow AND NOT a.shadow_hidden AND NOT a.private AND p.visibility = 'public'
    AND NOT (a.status = 'banned' AND (a.banned_until IS NULL OR a.banned_until > now())))";

pub async fn row(st: &AppState, id: i64) -> ApiResult<PgRow> {
    sqlx::query("SELECT * FROM posts WHERE id = $1 AND status <> 'deleted'").bind(id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)
}

/// The post if `viewer` may see it.
pub async fn visible(st: &AppState, viewer: i64, id: i64) -> ApiResult<PgRow> {
    let r = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT p.* FROM posts p JOIN profiles a ON a.user_id = p.author WHERE p.id = $2 AND p.status <> 'deleted' AND {VISIBLE}")))
        .bind(viewer)
        .bind(id)
        .fetch_optional(&st.pool)
        .await?;
    r.ok_or(ApiError::NotFound)
}

fn poll_json(poll: &Value, my_vote: Option<i32>) -> Value {
    let counts: Vec<i64> = poll["counts"].as_array().map(|a| a.iter().map(|c| c.as_i64().unwrap_or(0)).collect()).unwrap_or_default();
    let ends = poll["endsAt"].as_str().and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|d| d.with_timezone(&Utc));
    json!({
        "options": poll["options"], "counts": counts, "total": counts.iter().sum::<i64>(), "endsAt": poll["endsAt"],
        "closed": ends.is_some_and(|e| e < Utc::now()), "myVote": my_vote,
    })
}

/// Renders posts for `viewer` (batched lookups). Quoted / reposted originals are embedded one level deep.
pub async fn render(st: &AppState, viewer: &Me, rows: &[PgRow]) -> ApiResult<Vec<Value>> {
    render_for(st, viewer.id(), Some(&viewer.p.tenant), rows, true).await
}

pub async fn render_for(st: &AppState, viewer: i64, viewer_tenant: Option<&str>, rows: &[PgRow], embed: bool) -> ApiResult<Vec<Value>> {
    if rows.is_empty() {
        return Ok(vec![]);
    }
    let ids: Vec<i64> = rows.iter().map(|r| r.get("id")).collect();
    // originals of reposts / quotes
    let refs: Vec<i64> = rows.iter().filter_map(|r| r.get::<Option<i64>, _>("repost_of").or(r.get::<Option<i64>, _>("quote_of"))).collect();
    let embedded: HashMap<i64, Value> = if embed && !refs.is_empty() {
        let orig = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT p.* FROM posts p JOIN profiles a ON a.user_id = p.author WHERE p.id = ANY($2) AND p.status <> 'deleted' AND {VISIBLE}")))
            .bind(viewer)
            .bind(&refs)
            .fetch_all(&st.pool)
            .await?;
        Box::pin(render_for(st, viewer, viewer_tenant, &orig, false)).await?.into_iter().map(|v| (v["id"].as_i64().unwrap_or(0), v)).collect()
    } else {
        HashMap::new()
    };
    let authors: Vec<i64> = rows.iter().map(|r| r.get("author")).collect();
    let cards = profiles::many(st, &authors).await?;
    let media_ids: Vec<i64> = rows.iter().flat_map(|r| r.get::<Vec<i64>, _>("media")).collect();
    let media = media::many(st, &media_ids, viewer).await?;
    let card_ids: Vec<i64> = rows.iter().filter_map(|r| r.get::<Option<i64>, _>("trade_card")).collect();
    let trade_rows = if card_ids.is_empty() { vec![] } else { sqlx::query("SELECT * FROM trade_cards WHERE id = ANY($1)").bind(&card_ids).fetch_all(&st.pool).await? };
    let open_syms: Vec<String> = trade_rows.iter().filter(|r| r.get::<String, _>("state") == "open").map(|r| r.get("symbol")).collect();
    let quotes = crate::upstream::quotes(st, &open_syms).await;
    let trade: HashMap<i64, Value> = trade_rows
        .iter()
        .map(|r| {
            let sym: String = r.get("symbol");
            let live = quotes.get(&sym).and_then(crate::upstream::price_of);
            (r.get::<i64, _>("id"), tradecards::json(st, r, viewer, live))
        })
        .collect();
    let mine = sqlx::query("SELECT post_id, kind FROM reactions WHERE user_id = $1 AND post_id = ANY($2)").bind(viewer).bind(&ids).fetch_all(&st.pool).await?;
    let saved: HashSet<i64> = sqlx::query_scalar("SELECT post_id FROM saves WHERE user_id = $1 AND post_id = ANY($2)").bind(viewer).bind(&ids).fetch_all(&st.pool).await?.into_iter().collect();
    let reposted: HashSet<i64> = sqlx::query_scalar("SELECT repost_of FROM posts WHERE author = $1 AND kind = 'repost' AND status = 'published' AND repost_of = ANY($2)").bind(viewer).bind(&ids).fetch_all(&st.pool).await?.into_iter().collect();
    let votes: HashMap<i64, i32> = sqlx::query("SELECT post_id, option FROM poll_votes WHERE user_id = $1 AND post_id = ANY($2)").bind(viewer).bind(&ids).fetch_all(&st.pool).await?.iter().map(|r| (r.get("post_id"), r.get("option"))).collect();
    let mention_ids: Vec<i64> = rows.iter().flat_map(|r| r.get::<Vec<i64>, _>("mentions")).collect();
    let mention_cards = profiles::many(st, &mention_ids).await?;
    let mut out = Vec::new();
    for r in rows {
        let id: i64 = r.get("id");
        let author: i64 = r.get("author");
        let own = author == viewer;
        let p = cards.get(&author);
        let kinds: Vec<String> = mine.iter().filter(|m| m.get::<i64, _>("post_id") == id).map(|m| m.get("kind")).collect();
        let vote = kinds.iter().find(|k| *k == "bull" || *k == "bear").cloned();
        let status: String = r.get("status");
        let poll = r.get::<Option<sqlx::types::Json<Value>>, _>("poll").map(|j| poll_json(&j.0, votes.get(&id).copied()));
        let media_list: Vec<Value> = r.get::<Vec<i64>, _>("media").iter().filter_map(|m| media.get(m).cloned()).filter(|m| own || m["status"] == "ready").collect();
        let same_broker = viewer_tenant.is_some_and(|t| p.is_some_and(|p| p.tenant == t));
        let mut v = json!({
            "id": id,
            "kind": r.get::<String, _>("kind"),
            "author": p.map(|p| profiles::card(st, p)).unwrap_or_else(|| profiles::deleted_card(author)),
            "body": r.get::<String, _>("body"),
            "lang": r.get::<String, _>("lang"),
            "media": media_list,
            "tradeCard": r.get::<Option<i64>, _>("trade_card").and_then(|c| trade.get(&c).cloned()),
            "poll": poll,
            "repostOf": r.get::<Option<i64>, _>("repost_of").map(|o| embedded.get(&o).cloned().unwrap_or(json!({"id": o, "unavailable": true}))),
            "quoteOf": r.get::<Option<i64>, _>("quote_of").map(|o| embedded.get(&o).cloned().unwrap_or(json!({"id": o, "unavailable": true}))),
            "topic": r.get::<Option<String>, _>("topic"),
            "academyChapter": r.get::<Option<String>, _>("academy_chapter"),
            "visibility": r.get::<String, _>("visibility"),
            "comments": {"mode": r.get::<String, _>("comments_mode"), "pinned": r.get::<Option<i64>, _>("pinned_comment")},
            "counts": {
                "likes": r.get::<i64, _>("likes"), "bulls": r.get::<i64, _>("bulls"), "bears": r.get::<i64, _>("bears"), "comments": r.get::<i64, _>("comments"),
                "reposts": r.get::<i64, _>("reposts"), "quotes": r.get::<i64, _>("quotes"), "saves": r.get::<i64, _>("saves"), "views": r.get::<i64, _>("views"),
            },
            "viewer": {"liked": kinds.iter().any(|k| k == "like"), "vote": vote, "saved": saved.contains(&id), "reposted": reposted.contains(&id), "isAuthor": own},
            "hashtags": r.get::<Vec<String>, _>("hashtags"),
            "cashtags": r.get::<Vec<String>, _>("cashtags"),
            "mentions": r.get::<Vec<i64>, _>("mentions").iter().filter_map(|m| mention_cards.get(m).map(|c| json!({"id": m, "handle": c.handle}))).collect::<Vec<_>>(),
            "riskLine": if r.get::<bool, _>("risk_line") { json!(text::RISK_LINE) } else { Value::Null },
            "helpful": r.get::<bool, _>("helpful"),
            "featured": r.get::<Option<DateTime<Utc>>, _>("featured_until").is_some_and(|t| t > Utc::now()),
            "copyable": r.get::<Option<i64>, _>("trade_card").is_some() && same_broker && !own,
            "createdAt": r.get::<DateTime<Utc>, _>("created_at"),
            "publishedAt": r.get::<Option<DateTime<Utc>>, _>("published_at"),
            "editedAt": r.get::<Option<DateTime<Utc>>, _>("edited_at"),
            "url": format!("/circle/post/{id}"),
        });
        if own {
            v["status"] = json!(status);
            v["reason"] = json!(r.get::<Option<String>, _>("reason"));
        }
        out.push(v);
    }
    Ok(out)
}

/// Filters rows for feeds: muted authors and the viewer's hidden words.
pub fn feed_filter(rows: Vec<PgRow>, ex: &Excl, viewer: i64) -> Vec<PgRow> {
    rows.into_iter()
        .filter(|r| {
            let a: i64 = r.get("author");
            a == viewer || (!ex.muted_posts.contains(&a) && !ex.blocked.contains(&a) && !text::has_hidden_word(&r.get::<String, _>("body"), &ex.hidden_words))
        })
        .collect()
}

// ---------------------------------------------------------------- create / edit / delete

async fn resolve_mentions(st: &AppState, author: i64, handles: &[String]) -> ApiResult<Vec<i64>> {
    if handles.is_empty() {
        return Ok(vec![]);
    }
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT user_id FROM profiles p WHERE handle = ANY($1) AND user_id <> $2 AND status = 'active'
           AND NOT EXISTS (SELECT 1 FROM blocks b WHERE (b.blocker = p.user_id AND b.blocked = $2) OR (b.blocker = $2 AND b.blocked = p.user_id))",
    )
    .bind(handles)
    .bind(author)
    .fetch_all(&st.pool)
    .await?;
    Ok(ids)
}

fn parse_poll(v: &Value) -> ApiResult<Option<Value>> {
    if v.is_null() {
        return Ok(None);
    }
    let opts: Vec<String> = v["options"].as_array().map(|a| a.iter().filter_map(Value::as_str).map(|s| crate::util::clean(s, 60)).filter(|s| !s.is_empty()).collect()).unwrap_or_default();
    if !(2..=4).contains(&opts.len()) {
        return Err(invalid("poll.options", "A poll has 2 to 4 options."));
    }
    let hours = v["durationHours"].as_i64().unwrap_or(24).clamp(1, 168);
    Ok(Some(json!({"options": opts, "counts": vec![0; opts.len()], "endsAt": (Utc::now() + chrono::Duration::hours(hours)).to_rfc3339()})))
}

fn chapter(v: &Value) -> ApiResult<Option<String>> {
    match v.as_str().map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) if s.len() <= 120 && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '/') => Ok(Some(s.to_string())),
        Some(_) => Err(invalid("academyChapter", "Unknown Academy chapter.")),
    }
}

/// `POST /v1/circle/posts`.
pub async fn create(st: &AppState, me: &Me, b: &Value) -> ApiResult<i64> {
    let s = profiles::settings(st).await;
    if !st.limiter.hit(&format!("post:{}", me.id()), s.posts_per_hour.max(1) as usize, Duration::from_secs(3600)) {
        return Err(ApiError::RateLimited("You're posting very fast. Please wait a little.".into()));
    }
    let body = crate::util::clean(b["body"].as_str().unwrap_or(""), MAX_BODY + 1);
    if body.chars().count() > MAX_BODY {
        return Err(invalid("body", format!("Posts can be up to {MAX_BODY} characters.")));
    }
    let media_ids: Vec<i64> = b["mediaIds"].as_array().map(|a| a.iter().filter_map(Value::as_i64).collect()).unwrap_or_default();
    if media_ids.len() > 10 {
        return Err(invalid("mediaIds", "Up to 10 photos per post."));
    }
    let topic = b["topic"].as_str().map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty());
    let purposes: &[&str] = if topic.is_some() { &["topic_video"] } else { &["post"] };
    let media = media::attachable(st, me.id(), &media_ids, purposes).await?;
    let videos = media.iter().filter(|(_, k, _)| k == "video").count();
    if videos > 1 || (videos == 1 && media.len() > 1) {
        return Err(invalid("mediaIds", "A post has one video or up to 10 photos."));
    }
    if let Some(t) = &topic {
        let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM topics WHERE key = $1 AND active)").bind(t).fetch_one(&st.pool).await?;
        if !ok {
            return Err(invalid("topic", "Unknown topic."));
        }
        if videos != 1 {
            return Err(invalid("mediaIds", "A topic video needs one video."));
        }
    }
    let trade_card = b["tradeCardId"].as_i64();
    if let Some(c) = trade_card {
        let owner: Option<i64> = sqlx::query_scalar("SELECT owner FROM trade_cards WHERE id = $1").bind(c).fetch_optional(&st.pool).await?;
        if owner != Some(me.id()) {
            return Err(invalid("tradeCardId", "Share a trade card of your own."));
        }
    }
    let poll = parse_poll(&b["poll"])?;
    let quote_of = b["quoteOf"].as_i64();
    if let Some(q) = quote_of {
        visible(st, me.id(), q).await.map_err(|_| invalid("quoteOf", "This post can't be quoted."))?;
    }
    if body.is_empty() && media.is_empty() && trade_card.is_none() && poll.is_none() && quote_of.is_none() {
        return Err(invalid("body", "Write something or add a photo, video, trade or poll."));
    }
    let review = crate::moderation::precheck(st, &body).await?;
    let parsed = text::parse(&body);
    let mentions = resolve_mentions(st, me.id(), &parsed.mentions).await?;
    let visibility = match b["visibility"].as_str().unwrap_or("public") {
        "followers" => "followers",
        _ => "public",
    };
    let comments_mode = match b["comments"].as_str().unwrap_or(&me.p.comments_default) {
        "followers" => "followers",
        "off" => "off",
        _ => "everyone",
    };
    let has_chart = media.iter().any(|(_, k, _)| k == "chart");
    let risk_line = trade_card.is_some() || !parsed.cashtags.is_empty() || has_chart;
    let lang = text::guess_lang(&body, b["lang"].as_str().and_then(text::valid_lang).unwrap_or(&me.p.lang));
    let kind = if quote_of.is_some() { "quote" } else if videos == 1 { "video" } else { "post" };
    let status = if review.is_some() { "review" } else { "pending" };
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO posts (author, kind, body, lang, media, trade_card, poll, quote_of, topic, academy_chapter, visibility, comments_mode, status, risk_line,
                            hashtags, cashtags, mentions, links)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18) RETURNING id",
    )
    .bind(me.id())
    .bind(kind)
    .bind(&body)
    .bind(&lang)
    .bind(media.iter().map(|(i, _, _)| *i).collect::<Vec<_>>())
    .bind(trade_card)
    .bind(poll.map(sqlx::types::Json))
    .bind(quote_of)
    .bind(&topic)
    .bind(chapter(&b["academyChapter"])?)
    .bind(visibility)
    .bind(comments_mode)
    .bind(status)
    .bind(risk_line)
    .bind(&parsed.hashtags)
    .bind(&parsed.cashtags)
    .bind(&mentions)
    .bind(&parsed.links)
    .fetch_one(&st.pool)
    .await?;
    media::mark_attached(st, &media.iter().map(|(i, _, _)| *i).collect::<Vec<_>>()).await?;
    if let Some(v) = review {
        sqlx::query("UPDATE posts SET moderation = $2 WHERE id = $1").bind(id).bind(sqlx::types::Json(v.json())).execute(&st.pool).await?;
        crate::moderation::queue(st, "post", id, me.id(), &me.p.tenant, "rules", &v, &body).await?;
    }
    st.wake.moderation.notify_one();
    Ok(id)
}

/// `PATCH /v1/circle/posts/{id}`: the author edits the text or the comment setting; edited text is checked again.
pub async fn edit(st: &AppState, me: &Me, id: i64, b: &Value) -> ApiResult<()> {
    let r = row(st, id).await?;
    if r.get::<i64, _>("author") != me.id() {
        return Err(ApiError::Forbidden("Only the author can edit a post.".into()));
    }
    if let Some(mode) = b["comments"].as_str() {
        let mode = match mode {
            "followers" => "followers",
            "off" => "off",
            "everyone" => "everyone",
            _ => return Err(invalid("comments", "comments is everyone, followers or off.")),
        };
        sqlx::query("UPDATE posts SET comments_mode = $2 WHERE id = $1").bind(id).bind(mode).execute(&st.pool).await?;
    }
    if let Some(body) = b["body"].as_str() {
        if r.get::<String, _>("kind") == "repost" {
            return Err(invalid("body", "A repost has no text."));
        }
        let body = crate::util::clean(body, MAX_BODY + 1);
        if body.chars().count() > MAX_BODY {
            return Err(invalid("body", format!("Posts can be up to {MAX_BODY} characters.")));
        }
        if body != r.get::<String, _>("body") {
            let review = crate::moderation::precheck(st, &body).await?;
            let parsed = text::parse(&body);
            let mentions = resolve_mentions(st, me.id(), &parsed.mentions).await?;
            let status = if review.is_some() { "review" } else { "pending" };
            sqlx::query("UPDATE posts SET body = $2, hashtags = $3, cashtags = $4, mentions = $5, links = $6, status = $7, edited_at = now(), attempts = 0,
                         risk_line = risk_line OR cardinality($4) > 0 WHERE id = $1")
                .bind(id)
                .bind(&body)
                .bind(&parsed.hashtags)
                .bind(&parsed.cashtags)
                .bind(&mentions)
                .bind(&parsed.links)
                .bind(status)
                .execute(&st.pool)
                .await?;
            if let Some(v) = review {
                crate::moderation::queue(st, "post", id, me.id(), &me.p.tenant, "rules", &v, &body).await?;
            }
            st.wake.moderation.notify_one();
        }
    }
    Ok(())
}

/// `DELETE /v1/circle/posts/{id}` (author).
pub async fn delete(st: &AppState, me: &Me, id: i64) -> ApiResult<()> {
    let r = row(st, id).await?;
    if r.get::<i64, _>("author") != me.id() {
        return Err(ApiError::Forbidden("Only the author can delete a post.".into()));
    }
    remove_post(st, &r, "deleted").await
}

/// Takes a post down (`deleted` by the author, `removed` by staff): counters, the original's counters, media.
pub async fn remove_post(st: &AppState, r: &PgRow, status: &str) -> ApiResult<()> {
    let id: i64 = r.get("id");
    let was_published = r.get::<String, _>("status") == "published";
    sqlx::query("UPDATE posts SET status = $2, deleted_at = CASE WHEN $2 = 'deleted' THEN now() ELSE deleted_at END WHERE id = $1").bind(id).bind(status).execute(&st.pool).await?;
    if was_published {
        sqlx::query("UPDATE profiles SET posts_count = GREATEST(posts_count - 1, 0), pinned_post = CASE WHEN pinned_post = $2 THEN NULL ELSE pinned_post END WHERE user_id = $1")
            .bind(r.get::<i64, _>("author"))
            .bind(id)
            .execute(&st.pool)
            .await?;
        if let Some(o) = r.get::<Option<i64>, _>("repost_of") {
            sqlx::query("UPDATE posts SET reposts = GREATEST(reposts - 1, 0) WHERE id = $1").bind(o).execute(&st.pool).await?;
        }
        if let Some(o) = r.get::<Option<i64>, _>("quote_of") {
            sqlx::query("UPDATE posts SET quotes = GREATEST(quotes - 1, 0) WHERE id = $1").bind(o).execute(&st.pool).await?;
        }
    }
    if status == "deleted" {
        for m in r.get::<Vec<i64>, _>("media") {
            media::purge(st, m).await?;
        }
    }
    Ok(())
}

/// Makes a checked post visible and runs its side effects once (counters, XP, notifications).
pub async fn publish(st: &AppState, id: i64) -> anyhow::Result<()> {
    let Some(r) = sqlx::query("UPDATE posts SET status = 'published', published_at = COALESCE(published_at, now()), reason = NULL WHERE id = $1 AND status IN ('pending','review') RETURNING *")
        .bind(id)
        .fetch_optional(&st.pool)
        .await?
    else {
        return Ok(());
    };
    let author: i64 = r.get("author");
    st.hub.send(crate::state::Target::User(author), json!({"type": "post.status", "postId": id, "status": "published"}));
    let first_time = sqlx::query("UPDATE posts SET notified = true WHERE id = $1 AND NOT notified").bind(id).execute(&st.pool).await?.rows_affected() > 0;
    if !first_time {
        return Ok(());
    }
    let kind: String = r.get("kind");
    sqlx::query("UPDATE profiles SET posts_count = posts_count + 1 WHERE user_id = $1").bind(author).execute(&st.pool).await?;
    let card: Option<i64> = r.get("trade_card");
    crate::gamify::xp(st, author, if card.is_some() { crate::gamify::XP_TRADE_CARD } else { crate::gamify::XP_POST }).await?;
    crate::gamify::award(st, author, "first_post").await?;
    if card.is_some() {
        crate::gamify::award(st, author, "first_trade_card").await?;
    }
    if kind == "video" {
        crate::gamify::award(st, author, "first_video").await?;
    }
    let preview = crate::util::preview(&r.get::<String, _>("body"), 140);
    for m in r.get::<Vec<i64>, _>("mentions") {
        crate::notify::event(st, crate::notify::Ev::new(m, "mention", author).post(id).data(json!({"preview": preview}))).await?;
    }
    if let Some(q) = r.get::<Option<i64>, _>("quote_of") {
        sqlx::query("UPDATE posts SET quotes = quotes + 1 WHERE id = $1").bind(q).execute(&st.pool).await?;
        if let Some(qa) = sqlx::query_scalar::<_, i64>("SELECT author FROM posts WHERE id = $1").bind(q).fetch_optional(&st.pool).await? {
            crate::notify::event(st, crate::notify::Ev::new(qa, "quote", author).post(id).data(json!({"preview": preview, "quoted": q}))).await?;
        }
    }
    // followers: a shared trade reaches every follower (Q41), other posts the followers with the bell on
    if r.get::<String, _>("visibility") == "public" || r.get::<String, _>("visibility") == "followers" {
        let (ev_kind, all): (&'static str, bool) = if card.is_some() { ("trade", true) } else { ("post", false) };
        let followers: Vec<i64> = sqlx::query_scalar(if all {
            "SELECT follower FROM follows WHERE followee = $1 AND status = 'active' ORDER BY created_at LIMIT 5000"
        } else {
            "SELECT follower FROM follows WHERE followee = $1 AND status = 'active' AND bell ORDER BY created_at LIMIT 5000"
        })
        .bind(author)
        .fetch_all(&st.pool)
        .await?;
        let data = if let Some(c) = card {
            let t = sqlx::query("SELECT symbol, side FROM trade_cards WHERE id = $1").bind(c).fetch_one(&st.pool).await?;
            json!({"preview": preview, "symbol": t.get::<String, _>("symbol"), "side": t.get::<String, _>("side")})
        } else {
            json!({"preview": preview})
        };
        for f in followers {
            crate::notify::event(st, crate::notify::Ev::new(f, ev_kind, author).post(id).data(data.clone())).await?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- reactions, reposts, saves, polls

/// Like, bull or bear (bull / bear exclusive). Returns the new counts.
pub async fn react(st: &AppState, me: &Me, id: i64, kind: &str) -> ApiResult<Value> {
    if !["like", "bull", "bear"].contains(&kind) {
        return Err(invalid("kind", "kind is like, bull or bear."));
    }
    let r = visible(st, me.id(), id).await?;
    if r.get::<String, _>("status") != "published" {
        return Err(conflict("not_published", "This post isn't published yet."));
    }
    if kind != "like" && r.get::<Vec<String>, _>("cashtags").is_empty() && r.get::<Option<i64>, _>("trade_card").is_none() {
        return Err(invalid("kind", "Bull / bear votes are for posts about a symbol."));
    }
    let author: i64 = r.get("author");
    let mut tx = st.pool.begin().await?;
    if kind != "like" {
        let other = if kind == "bull" { "bear" } else { "bull" };
        let removed = sqlx::query("DELETE FROM reactions WHERE post_id = $1 AND user_id = $2 AND kind = $3").bind(id).bind(me.id()).bind(other).execute(&mut *tx).await?.rows_affected();
        if removed > 0 {
            sqlx::query(sqlx::AssertSqlSafe(format!("UPDATE posts SET {other}s = GREATEST({other}s - 1, 0) WHERE id = $1"))).bind(id).execute(&mut *tx).await?;
        }
    }
    let added = sqlx::query("INSERT INTO reactions (post_id, user_id, kind) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING").bind(id).bind(me.id()).bind(kind).execute(&mut *tx).await?.rows_affected() > 0;
    if added {
        sqlx::query(sqlx::AssertSqlSafe(format!("UPDATE posts SET {kind}s = {kind}s + 1 WHERE id = $1"))).bind(id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    if added {
        crate::gamify::xp(st, author, crate::gamify::XP_LIKE).await?;
        crate::notify::event(st, crate::notify::Ev::new(author, if kind == "like" { "like" } else { "vote" }, me.id()).post(id).data(json!({"reaction": kind}))).await?;
        crate::rewards::check_helpful(st, id).await?;
    }
    counts(st, id).await
}

pub async fn unreact(st: &AppState, me: &Me, id: i64, kind: &str) -> ApiResult<Value> {
    if !["like", "bull", "bear"].contains(&kind) {
        return Err(invalid("kind", "kind is like, bull or bear."));
    }
    let removed = sqlx::query("DELETE FROM reactions WHERE post_id = $1 AND user_id = $2 AND kind = $3").bind(id).bind(me.id()).bind(kind).execute(&st.pool).await?.rows_affected();
    if removed > 0 {
        sqlx::query(sqlx::AssertSqlSafe(format!("UPDATE posts SET {kind}s = GREATEST({kind}s - 1, 0) WHERE id = $1"))).bind(id).execute(&st.pool).await?;
    }
    counts(st, id).await
}

pub async fn counts(st: &AppState, id: i64) -> ApiResult<Value> {
    let r = sqlx::query("SELECT likes, bulls, bears, comments, reposts, quotes, saves, views FROM posts WHERE id = $1").bind(id).fetch_one(&st.pool).await?;
    let bulls: i64 = r.get("bulls");
    let bears: i64 = r.get("bears");
    Ok(json!({
        "likes": r.get::<i64, _>("likes"), "bulls": bulls, "bears": bears, "comments": r.get::<i64, _>("comments"), "reposts": r.get::<i64, _>("reposts"),
        "quotes": r.get::<i64, _>("quotes"), "saves": r.get::<i64, _>("saves"), "views": r.get::<i64, _>("views"),
        "sentiment": if bulls + bears > 0 { json!({"bullPct": (bulls as f64 * 1000.0 / (bulls + bears) as f64).round() / 10.0}) } else { Value::Null },
    }))
}

pub async fn repost(st: &AppState, me: &Me, id: i64) -> ApiResult<i64> {
    let r = visible(st, me.id(), id).await?;
    if r.get::<String, _>("status") != "published" || r.get::<String, _>("kind") == "repost" {
        return Err(invalid("id", "This post can't be reposted."));
    }
    if r.get::<String, _>("visibility") != "public" {
        return Err(denied("not_public", "Only public posts can be reposted."));
    }
    let new: Option<i64> = sqlx::query_scalar(
        "INSERT INTO posts (author, kind, repost_of, status, published_at, notified, lang) VALUES ($1, 'repost', $2, 'published', now(), true, $3)
         ON CONFLICT (author, repost_of) WHERE kind = 'repost' AND status <> 'deleted' DO NOTHING RETURNING id",
    )
    .bind(me.id())
    .bind(id)
    .bind(&me.p.lang)
    .fetch_optional(&st.pool)
    .await?;
    match new {
        Some(n) => {
            sqlx::query("UPDATE posts SET reposts = reposts + 1 WHERE id = $1").bind(id).execute(&st.pool).await?;
            sqlx::query("UPDATE profiles SET posts_count = posts_count + 1 WHERE user_id = $1").bind(me.id()).execute(&st.pool).await?;
            crate::notify::event(st, crate::notify::Ev::new(r.get("author"), "repost", me.id()).post(id)).await?;
            Ok(n)
        }
        None => Err(conflict("already_reposted", "You already reposted this.")),
    }
}

pub async fn unrepost(st: &AppState, me: &Me, id: i64) -> ApiResult<()> {
    let r = sqlx::query("SELECT * FROM posts WHERE author = $1 AND repost_of = $2 AND kind = 'repost' AND status <> 'deleted'").bind(me.id()).bind(id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    remove_post(st, &r, "deleted").await
}

pub async fn save(st: &AppState, me: &Me, id: i64, collection: Option<i64>) -> ApiResult<()> {
    visible(st, me.id(), id).await?;
    if let Some(c) = collection {
        let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM collections WHERE id = $1 AND user_id = $2)").bind(c).bind(me.id()).fetch_one(&st.pool).await?;
        if !ok {
            return Err(invalid("collectionId", "Unknown collection."));
        }
    }
    let inserted: Option<bool> = sqlx::query_scalar(
        "INSERT INTO saves (user_id, post_id, collection_id) VALUES ($1,$2,$3) ON CONFLICT (user_id, post_id) DO UPDATE SET collection_id = EXCLUDED.collection_id RETURNING (xmax = 0)",
    )
    .bind(me.id())
    .bind(id)
    .bind(collection)
    .fetch_optional(&st.pool)
    .await?;
    if inserted == Some(true) {
        sqlx::query("UPDATE posts SET saves = saves + 1 WHERE id = $1").bind(id).execute(&st.pool).await?;
    }
    Ok(())
}

pub async fn unsave(st: &AppState, me: &Me, id: i64) -> ApiResult<()> {
    if sqlx::query("DELETE FROM saves WHERE user_id = $1 AND post_id = $2").bind(me.id()).bind(id).execute(&st.pool).await?.rows_affected() > 0 {
        sqlx::query("UPDATE posts SET saves = GREATEST(saves - 1, 0) WHERE id = $1").bind(id).execute(&st.pool).await?;
    }
    Ok(())
}

pub async fn vote(st: &AppState, me: &Me, id: i64, option: i64) -> ApiResult<Value> {
    let r = visible(st, me.id(), id).await?;
    let poll: Value = r.get::<Option<sqlx::types::Json<Value>>, _>("poll").map(|j| j.0).ok_or_else(|| invalid("id", "This post has no poll."))?;
    let n = poll["options"].as_array().map(|a| a.len()).unwrap_or(0) as i64;
    if option < 0 || option >= n {
        return Err(invalid("option", "Unknown option."));
    }
    if poll["endsAt"].as_str().and_then(|s| DateTime::parse_from_rfc3339(s).ok()).is_some_and(|e| e < Utc::now()) {
        return Err(conflict("poll_closed", "This poll has ended."));
    }
    let inserted = sqlx::query("INSERT INTO poll_votes (post_id, user_id, option) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING").bind(id).bind(me.id()).bind(option as i32).execute(&st.pool).await?.rows_affected() > 0;
    if !inserted {
        return Err(conflict("already_voted", "You already voted."));
    }
    let path = format!("{{counts,{option}}}");
    let updated: sqlx::types::Json<Value> = sqlx::query_scalar("UPDATE posts SET poll = jsonb_set(poll, $2::text[], to_jsonb(COALESCE((poll #>> $2::text[])::bigint, 0) + 1)) WHERE id = $1 RETURNING poll")
        .bind(id)
        .bind(path)
        .fetch_one(&st.pool)
        .await?;
    Ok(poll_json(&updated.0, Some(option as i32)))
}

// ---------------------------------------------------------------- comments

pub async fn comment_json(st: &AppState, viewer: i64, rows: &[PgRow]) -> ApiResult<Vec<Value>> {
    let authors: Vec<i64> = rows.iter().map(|r| r.get("author")).collect();
    let cards = profiles::many(st, &authors).await?;
    let ids: Vec<i64> = rows.iter().map(|r| r.get("id")).collect();
    let liked: HashSet<i64> = sqlx::query_scalar("SELECT comment_id FROM comment_likes WHERE user_id = $1 AND comment_id = ANY($2)").bind(viewer).bind(&ids).fetch_all(&st.pool).await?.into_iter().collect();
    let media_ids: Vec<i64> = rows.iter().filter_map(|r| r.get::<Option<i64>, _>("media")).collect();
    let media = media::many(st, &media_ids, viewer).await?;
    Ok(rows
        .iter()
        .map(|r| {
            let id: i64 = r.get("id");
            let author: i64 = r.get("author");
            let own = author == viewer;
            let deleted = r.get::<String, _>("status") == "deleted";
            let mut v = json!({
                "id": id,
                "postId": r.get::<i64, _>("post_id"),
                "parentId": r.get::<Option<i64>, _>("parent_id"),
                "author": cards.get(&author).map(|p| profiles::card(st, p)).unwrap_or_else(|| profiles::deleted_card(author)),
                "body": if deleted { String::new() } else { r.get::<String, _>("body") },
                "deleted": deleted,
                "lang": r.get::<String, _>("lang"),
                "media": r.get::<Option<i64>, _>("media").and_then(|m| media.get(&m).cloned()).filter(|m| own || m["status"] == "ready"),
                "likes": r.get::<i64, _>("likes"),
                "replies": r.get::<i64, _>("replies"),
                "liked": liked.contains(&id),
                "restricted": r.get::<bool, _>("restricted"),
                "createdAt": r.get::<DateTime<Utc>, _>("created_at"),
                "editedAt": r.get::<Option<DateTime<Utc>>, _>("edited_at"),
            });
            if own {
                v["status"] = json!(r.get::<String, _>("status"));
                v["reason"] = json!(r.get::<Option<String>, _>("reason"));
            }
            v
        })
        .collect())
}

/// SQL condition: comment `c` visible to `$1` on a post whose author is `$2`.
const COMMENT_VISIBLE: &str = "(c.author = $1 OR (c.status IN ('published','deleted') AND NOT c.shadow AND (NOT c.restricted OR $2 = $1)
    AND NOT EXISTS (SELECT 1 FROM profiles ap WHERE ap.user_id = c.author AND (ap.shadow_hidden OR (ap.status = 'banned' AND (ap.banned_until IS NULL OR ap.banned_until > now()))))
    AND NOT EXISTS (SELECT 1 FROM blocks b WHERE (b.blocker = $1 AND b.blocked = c.author) OR (b.blocker = c.author AND b.blocked = $1))))";

pub async fn list_comments(st: &AppState, me: &Me, post: i64, parent: Option<i64>, before: i64, limit: i64) -> ApiResult<Value> {
    let p = visible(st, me.id(), post).await?;
    let author: i64 = p.get("author");
    let ex = profiles::exclusions(st, me.id()).await?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT c.* FROM comments c WHERE c.post_id = $3 AND c.parent_id IS NOT DISTINCT FROM $4 AND c.id < $5 AND {COMMENT_VISIBLE}
           AND NOT (c.status = 'deleted' AND c.replies = 0)
         ORDER BY c.id DESC LIMIT $6"
    )))
    .bind(me.id())
    .bind(author)
    .bind(post)
    .bind(parent)
    .bind(before)
    .bind(limit + 1)
    .fetch_all(&st.pool)
    .await?;
    let rows: Vec<PgRow> = rows.into_iter().filter(|r| r.get::<i64, _>("author") == me.id() || !text::has_hidden_word(&r.get::<String, _>("body"), &ex.hidden_words)).collect();
    let items = comment_json(st, me.id(), &rows).await?;
    let mut page = crate::api::paged(items, limit, |v| v["id"].as_i64().map(|i| i.to_string()));
    // the pinned comment on the first page of top-level comments
    if parent.is_none() && before == i64::MAX {
        if let Some(pid) = p.get::<Option<i64>, _>("pinned_comment") {
            let pinned = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT c.* FROM comments c WHERE c.id = $3 AND {COMMENT_VISIBLE}"))).bind(me.id()).bind(author).bind(pid).fetch_all(&st.pool).await?;
            page["pinned"] = comment_json(st, me.id(), &pinned).await?.into_iter().next().unwrap_or(Value::Null);
        } else {
            page["pinned"] = Value::Null;
        }
    }
    page["mode"] = json!(p.get::<String, _>("comments_mode"));
    Ok(page)
}

pub async fn add_comment(st: &AppState, me: &Me, post: i64, b: &Value) -> ApiResult<i64> {
    let p = visible(st, me.id(), post).await?;
    if p.get::<String, _>("status") != "published" {
        return Err(conflict("not_published", "This post isn't published yet."));
    }
    let author: i64 = p.get("author");
    match p.get::<String, _>("comments_mode").as_str() {
        "off" if author != me.id() => return Err(denied("comments_off", "Comments are turned off for this post.")),
        "followers" if author != me.id() && !profiles::is_follower(st, me.id(), author).await? => {
            return Err(denied("comments_followers", "Only followers can comment on this post."));
        }
        _ => {}
    }
    if !st.limiter.hit(&format!("comment:{}", me.id()), 60, Duration::from_secs(3600)) {
        return Err(ApiError::RateLimited("You're commenting very fast. Please wait a little.".into()));
    }
    let body = crate::util::clean(b["body"].as_str().unwrap_or(""), MAX_COMMENT + 1);
    if body.chars().count() > MAX_COMMENT {
        return Err(invalid("body", format!("Comments can be up to {MAX_COMMENT} characters.")));
    }
    let media_id = b["mediaId"].as_i64();
    let media = media::attachable(st, me.id(), &media_id.into_iter().collect::<Vec<_>>(), &["comment"]).await?;
    if body.is_empty() && media.is_empty() {
        return Err(invalid("body", "Write a comment."));
    }
    let parent = match b["parentId"].as_i64() {
        Some(pid) => {
            let pr = sqlx::query("SELECT parent_id, post_id, status FROM comments WHERE id = $1").bind(pid).fetch_optional(&st.pool).await?.ok_or_else(|| invalid("parentId", "Unknown comment."))?;
            if pr.get::<i64, _>("post_id") != post || pr.get::<String, _>("status") != "published" {
                return Err(invalid("parentId", "Unknown comment."));
            }
            // one level of threads: replies to a reply go to its top-level comment
            Some(pr.get::<Option<i64>, _>("parent_id").unwrap_or(pid))
        }
        None => None,
    };
    let review = crate::moderation::precheck(st, &body).await?;
    let parsed = text::parse(&body);
    let mentions = resolve_mentions(st, me.id(), &parsed.mentions).await?;
    let restricted: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM restricts WHERE restrictor = $1 AND restricted = $2)").bind(author).bind(me.id()).fetch_one(&st.pool).await?;
    let lang = text::guess_lang(&body, &me.p.lang);
    let id: i64 = sqlx::query_scalar("INSERT INTO comments (post_id, author, parent_id, body, lang, media, mentions, status, restricted) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING id")
        .bind(post)
        .bind(me.id())
        .bind(parent)
        .bind(&body)
        .bind(&lang)
        .bind(media.first().map(|m| m.0))
        .bind(&mentions)
        .bind(if review.is_some() { "review" } else { "pending" })
        .bind(restricted)
        .fetch_one(&st.pool)
        .await?;
    media::mark_attached(st, &media.iter().map(|m| m.0).collect::<Vec<_>>()).await?;
    if let Some(v) = review {
        crate::moderation::queue(st, "comment", id, me.id(), &me.p.tenant, "rules", &v, &body).await?;
    }
    st.wake.moderation.notify_one();
    Ok(id)
}

/// A checked comment becomes visible: counters and notifications (post author, parent author, mentions).
pub async fn publish_comment(st: &AppState, id: i64) -> anyhow::Result<()> {
    let Some(c) = sqlx::query("UPDATE comments SET status = 'published', reason = NULL WHERE id = $1 AND status IN ('pending','review') RETURNING *").bind(id).fetch_optional(&st.pool).await? else {
        return Ok(());
    };
    let post: i64 = c.get("post_id");
    let author: i64 = c.get("author");
    let parent: Option<i64> = c.get("parent_id");
    let edited = c.get::<Option<DateTime<Utc>>, _>("edited_at").is_some();
    st.hub.send(crate::state::Target::User(author), json!({"type": "comment.status", "commentId": id, "postId": post, "status": "published"}));
    if edited {
        return Ok(());
    }
    sqlx::query("UPDATE posts SET comments = comments + 1 WHERE id = $1").bind(post).execute(&st.pool).await?;
    if let Some(p) = parent {
        sqlx::query("UPDATE comments SET replies = replies + 1 WHERE id = $1").bind(p).execute(&st.pool).await?;
    }
    let post_author: i64 = sqlx::query_scalar("SELECT author FROM posts WHERE id = $1").bind(post).fetch_one(&st.pool).await?;
    let preview = crate::util::preview(&c.get::<String, _>("body"), 140);
    // restricted commenters don't notify the author
    if !c.get::<bool, _>("restricted") {
        crate::gamify::xp(st, post_author, crate::gamify::XP_COMMENT).await?;
        crate::notify::event(st, crate::notify::Ev::new(post_author, "comment", author).post(post).comment(id).data(json!({"preview": preview}))).await?;
    }
    if let Some(p) = parent {
        let pa: i64 = sqlx::query_scalar("SELECT author FROM comments WHERE id = $1").bind(p).fetch_one(&st.pool).await?;
        if pa != post_author {
            crate::notify::event(st, crate::notify::Ev::new(pa, "reply", author).post(post).comment(id).data(json!({"preview": preview}))).await?;
        }
    }
    for m in c.get::<Vec<i64>, _>("mentions") {
        if m != post_author {
            crate::notify::event(st, crate::notify::Ev::new(m, "mention", author).post(post).comment(id).data(json!({"preview": preview}))).await?;
        }
    }
    Ok(())
}

pub async fn delete_comment(st: &AppState, me: &Me, id: i64) -> ApiResult<()> {
    let c = sqlx::query("SELECT c.*, p.author AS post_author FROM comments c JOIN posts p ON p.id = c.post_id WHERE c.id = $1 AND c.status <> 'deleted'").bind(id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    // the comment's author or the post's author (author controls)
    if c.get::<i64, _>("author") != me.id() && c.get::<i64, _>("post_author") != me.id() {
        return Err(ApiError::Forbidden("You can't delete this comment.".into()));
    }
    remove_comment(st, &c, "deleted").await
}

pub async fn remove_comment(st: &AppState, c: &PgRow, status: &str) -> ApiResult<()> {
    let id: i64 = c.get("id");
    let was = c.get::<String, _>("status") == "published";
    sqlx::query("UPDATE comments SET status = $2, deleted_at = now() WHERE id = $1").bind(id).bind(status).execute(&st.pool).await?;
    if was {
        let post: i64 = c.get("post_id");
        sqlx::query("UPDATE posts SET comments = GREATEST(comments - 1, 0), pinned_comment = CASE WHEN pinned_comment = $2 THEN NULL ELSE pinned_comment END WHERE id = $1").bind(post).bind(id).execute(&st.pool).await?;
        if let Some(p) = c.get::<Option<i64>, _>("parent_id") {
            sqlx::query("UPDATE comments SET replies = GREATEST(replies - 1, 0) WHERE id = $1").bind(p).execute(&st.pool).await?;
        }
    }
    if let Some(m) = c.get::<Option<i64>, _>("media") {
        media::purge(st, m).await?;
    }
    Ok(())
}

pub async fn edit_comment(st: &AppState, me: &Me, id: i64, body: &str) -> ApiResult<()> {
    let c = sqlx::query("SELECT * FROM comments WHERE id = $1 AND status <> 'deleted'").bind(id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    if c.get::<i64, _>("author") != me.id() {
        return Err(ApiError::Forbidden("Only the author can edit a comment.".into()));
    }
    let body = crate::util::clean(body, MAX_COMMENT + 1);
    if body.is_empty() || body.chars().count() > MAX_COMMENT {
        return Err(invalid("body", format!("Comments are 1 to {MAX_COMMENT} characters.")));
    }
    let review = crate::moderation::precheck(st, &body).await?;
    sqlx::query("UPDATE comments SET body = $2, edited_at = now(), status = $3, attempts = 0 WHERE id = $1")
        .bind(id)
        .bind(&body)
        .bind(if review.is_some() { "review" } else { "pending" })
        .execute(&st.pool)
        .await?;
    if let Some(v) = review {
        crate::moderation::queue(st, "comment", id, me.id(), &me.p.tenant, "rules", &v, &body).await?;
    }
    st.wake.moderation.notify_one();
    Ok(())
}

pub async fn like_comment(st: &AppState, me: &Me, id: i64, on: bool) -> ApiResult<Value> {
    let c = sqlx::query("SELECT post_id, author, status FROM comments WHERE id = $1").bind(id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    visible(st, me.id(), c.get("post_id")).await?;
    if c.get::<String, _>("status") != "published" {
        return Err(ApiError::NotFound);
    }
    if on {
        if sqlx::query("INSERT INTO comment_likes (comment_id, user_id) VALUES ($1,$2) ON CONFLICT DO NOTHING").bind(id).bind(me.id()).execute(&st.pool).await?.rows_affected() > 0 {
            sqlx::query("UPDATE comments SET likes = likes + 1 WHERE id = $1").bind(id).execute(&st.pool).await?;
            crate::notify::event(st, crate::notify::Ev::new(c.get("author"), "comment_like", me.id()).post(c.get("post_id")).comment(id)).await?;
        }
    } else if sqlx::query("DELETE FROM comment_likes WHERE comment_id = $1 AND user_id = $2").bind(id).bind(me.id()).execute(&st.pool).await?.rows_affected() > 0 {
        sqlx::query("UPDATE comments SET likes = GREATEST(likes - 1, 0) WHERE id = $1").bind(id).execute(&st.pool).await?;
    }
    let likes: i64 = sqlx::query_scalar("SELECT likes FROM comments WHERE id = $1").bind(id).fetch_one(&st.pool).await?;
    Ok(json!({"likes": likes, "liked": on}))
}

pub async fn pin_comment(st: &AppState, me: &Me, post: i64, comment: Option<i64>) -> ApiResult<()> {
    let p = row(st, post).await?;
    if p.get::<i64, _>("author") != me.id() {
        return Err(ApiError::Forbidden("Only the post's author can pin a comment.".into()));
    }
    if let Some(c) = comment {
        let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM comments WHERE id = $1 AND post_id = $2 AND status = 'published' AND parent_id IS NULL)").bind(c).bind(post).fetch_one(&st.pool).await?;
        if !ok {
            return Err(invalid("commentId", "Pin a published top-level comment of this post."));
        }
    }
    sqlx::query("UPDATE posts SET pinned_comment = $2 WHERE id = $1").bind(post).bind(comment).execute(&st.pool).await?;
    Ok(())
}

/// Author card of a profile or `deleted_card`.
pub fn author_card(st: &AppState, p: Option<&Profile>, id: i64) -> Value {
    p.map(|p| profiles::card(st, p)).unwrap_or_else(|| profiles::deleted_card(id))
}
