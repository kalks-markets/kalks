//! Stories (Q13–Q15): photo / video / chart snapshot / trade result / text, 24 hours, stickers (bull / bear
//! sentiment poll, question box), audience everyone or close friends, named highlights on the profile (kept after
//! the 24 hours). Checked like posts before they show.

use crate::api::Me;
use crate::error::{ApiError, ApiResult, invalid};
use crate::profiles;
use crate::state::AppState;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::Row;
use sqlx::postgres::PgRow;
use std::collections::HashMap;

pub const KINDS: &[&str] = &["photo", "video", "chart", "trade", "text"];

fn stickers(v: &Value) -> ApiResult<Value> {
    let arr = match v.as_array() {
        None => return Ok(json!([])),
        Some(a) if a.len() > 3 => return Err(invalid("stickers", "Up to 3 stickers per story.")),
        Some(a) => a,
    };
    let mut out = Vec::new();
    for s in arr {
        let x = s["x"].as_f64().unwrap_or(0.5).clamp(0.0, 1.0);
        let y = s["y"].as_f64().unwrap_or(0.5).clamp(0.0, 1.0);
        match s["type"].as_str() {
            Some("sentiment") => {
                let symbol = s["symbol"].as_str().map(str::to_uppercase).filter(|x| crate::upstream::is_symbol(x));
                let q = crate::util::clean(s["question"].as_str().unwrap_or("Bull or bear?"), 80);
                out.push(json!({"type": "sentiment", "symbol": symbol, "question": q, "x": x, "y": y}));
            }
            Some("question") => {
                let prompt = crate::util::clean(s["prompt"].as_str().unwrap_or("Ask me anything"), 80);
                out.push(json!({"type": "question", "prompt": prompt, "x": x, "y": y}));
            }
            Some("cashtag") => {
                let symbol = s["symbol"].as_str().map(str::to_uppercase).filter(|x| crate::upstream::is_symbol(x)).ok_or_else(|| invalid("stickers", "A cashtag sticker needs a symbol."))?;
                out.push(json!({"type": "cashtag", "symbol": symbol, "x": x, "y": y}));
            }
            Some("mention") => {
                let h = s["handle"].as_str().and_then(|h| crate::text::handle(h).ok()).ok_or_else(|| invalid("stickers", "A mention sticker needs a handle."))?;
                out.push(json!({"type": "mention", "handle": h, "x": x, "y": y}));
            }
            _ => return Err(invalid("stickers", "Stickers are sentiment, question, cashtag or mention.")),
        }
    }
    Ok(json!(out))
}

pub async fn create(st: &AppState, me: &Me, b: &Value) -> ApiResult<i64> {
    let kind = b["kind"].as_str().unwrap_or("");
    if !KINDS.contains(&kind) {
        return Err(invalid("kind", "kind is photo, video, chart, trade or text."));
    }
    if !st.limiter.hit(&format!("story:{}", me.id()), 50, std::time::Duration::from_secs(86_400)) {
        return Err(ApiError::RateLimited("You've added a lot of stories today.".into()));
    }
    let body = crate::util::clean(b["body"].as_str().unwrap_or(""), 501);
    if body.chars().count() > 500 {
        return Err(invalid("body", "Story text can be up to 500 characters."));
    }
    let media_id = b["mediaId"].as_i64();
    let media = crate::media::attachable(st, me.id(), &media_id.into_iter().collect::<Vec<_>>(), &["story"]).await?;
    match kind {
        "photo" | "video" | "chart" if media.first().map(|m| m.1.as_str()) != Some(kind) => return Err(invalid("mediaId", format!("A {kind} story needs a {kind} upload."))),
        "text" if body.is_empty() => return Err(invalid("body", "Write something.")),
        _ => {}
    }
    let card = b["tradeCardId"].as_i64();
    if kind == "trade" && card.is_none() {
        return Err(invalid("tradeCardId", "A trade story needs a trade card."));
    }
    if let Some(c) = card {
        let owner: Option<i64> = sqlx::query_scalar("SELECT owner FROM trade_cards WHERE id = $1").bind(c).fetch_optional(&st.pool).await?;
        if owner != Some(me.id()) {
            return Err(invalid("tradeCardId", "Share a trade card of your own."));
        }
    }
    let review = crate::moderation::precheck(st, &body).await?;
    let audience = if b["audience"].as_str() == Some("close_friends") { "close_friends" } else { "everyone" };
    let bg = b["background"].as_str().map(|c| crate::util::clean(c, 32)).filter(|c| !c.is_empty());
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO stories (author, kind, media, body, background, trade_card, stickers, audience, status, expires_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9, now() + interval '24 hours') RETURNING id",
    )
    .bind(me.id())
    .bind(kind)
    .bind(media.first().map(|m| m.0))
    .bind(&body)
    .bind(bg)
    .bind(card)
    .bind(sqlx::types::Json(stickers(&b["stickers"])?))
    .bind(audience)
    .bind(if review.is_some() { "review" } else { "pending" })
    .fetch_one(&st.pool)
    .await?;
    crate::media::mark_attached(st, &media.iter().map(|m| m.0).collect::<Vec<_>>()).await?;
    if let Some(v) = review {
        crate::moderation::queue(st, "story", id, me.id(), &me.p.tenant, "rules", &v, &body).await?;
    }
    st.wake.moderation.notify_one();
    Ok(id)
}

/// A checked story shows: followers with the bell on get a push, mentioned members a mention.
pub async fn publish(st: &AppState, id: i64) -> anyhow::Result<()> {
    let Some(r) = sqlx::query("UPDATE stories SET status = 'published', reason = NULL WHERE id = $1 AND status IN ('pending','review') RETURNING *").bind(id).fetch_optional(&st.pool).await? else {
        return Ok(());
    };
    let author: i64 = r.get("author");
    st.hub.send(crate::state::Target::User(author), json!({"type": "story.status", "storyId": id, "status": "published"}));
    if sqlx::query("UPDATE stories SET notified = true WHERE id = $1 AND NOT notified").bind(id).execute(&st.pool).await?.rows_affected() == 0 {
        return Ok(());
    }
    crate::gamify::award(st, author, "first_story").await?;
    let close = r.get::<String, _>("audience") == "close_friends";
    let followers: Vec<i64> = sqlx::query_scalar(
        "SELECT f.follower FROM follows f WHERE f.followee = $1 AND f.status = 'active' AND f.bell
           AND ($2 = false OR EXISTS (SELECT 1 FROM close_friends c WHERE c.owner = $1 AND c.friend = f.follower)) LIMIT 5000",
    )
    .bind(author)
    .bind(close)
    .fetch_all(&st.pool)
    .await?;
    for f in followers {
        crate::notify::event(st, crate::notify::Ev::new(f, "story", author).story(id)).await?;
    }
    let stickers: Value = r.get::<sqlx::types::Json<Value>, _>("stickers").0;
    let handles: Vec<String> = stickers.as_array().into_iter().flatten().filter(|s| s["type"] == "mention").filter_map(|s| s["handle"].as_str().map(str::to_string)).chain(crate::text::parse(&r.get::<String, _>("body")).mentions).collect();
    if !handles.is_empty() {
        let ids: Vec<i64> = sqlx::query_scalar("SELECT user_id FROM profiles WHERE handle = ANY($1)").bind(&handles).fetch_all(&st.pool).await?;
        for m in ids {
            crate::notify::event(st, crate::notify::Ev::new(m, "mention", author).story(id).data(json!({"preview": "mentioned you in a story"}))).await?;
        }
    }
    Ok(())
}

/// Whether `viewer` may see story row `r` (author visibility, close friends, live or in a highlight).
pub async fn can_see(st: &AppState, viewer: i64, r: &PgRow, via_highlight: bool) -> ApiResult<bool> {
    let author: i64 = r.get("author");
    if author == viewer {
        return Ok(true);
    }
    if r.get::<String, _>("status") != "published" || r.get::<bool, _>("shadow") {
        return Ok(false);
    }
    if !via_highlight && r.get::<DateTime<Utc>, _>("expires_at") < Utc::now() {
        return Ok(false);
    }
    let Some(p) = profiles::by_id(st, author).await? else { return Ok(false) };
    if !profiles::can_view(st, viewer, &p).await? {
        return Ok(false);
    }
    if r.get::<String, _>("audience") == "close_friends" {
        return Ok(sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM close_friends WHERE owner = $1 AND friend = $2)").bind(author).bind(viewer).fetch_one(&st.pool).await?);
    }
    Ok(true)
}

pub async fn render(st: &AppState, viewer: i64, rows: &[PgRow]) -> ApiResult<Vec<Value>> {
    let ids: Vec<i64> = rows.iter().map(|r| r.get("id")).collect();
    let authors: Vec<i64> = rows.iter().map(|r| r.get("author")).collect();
    let cards = profiles::many(st, &authors).await?;
    let media_ids: Vec<i64> = rows.iter().filter_map(|r| r.get::<Option<i64>, _>("media")).collect();
    let media = crate::media::many(st, &media_ids, viewer).await?;
    let card_ids: Vec<i64> = rows.iter().filter_map(|r| r.get::<Option<i64>, _>("trade_card")).collect();
    let trade: HashMap<i64, Value> = if card_ids.is_empty() {
        HashMap::new()
    } else {
        sqlx::query("SELECT * FROM trade_cards WHERE id = ANY($1)").bind(&card_ids).fetch_all(&st.pool).await?.iter().map(|r| (r.get::<i64, _>("id"), crate::tradecards::json(st, r, viewer, None))).collect()
    };
    let seen: std::collections::HashSet<i64> = sqlx::query_scalar("SELECT story_id FROM story_views WHERE viewer = $1 AND story_id = ANY($2)").bind(viewer).bind(&ids).fetch_all(&st.pool).await?.into_iter().collect();
    let my_votes: HashMap<i64, String> = sqlx::query("SELECT story_id, choice FROM story_votes WHERE user_id = $1 AND story_id = ANY($2)").bind(viewer).bind(&ids).fetch_all(&st.pool).await?.iter().map(|r| (r.get("story_id"), r.get("choice"))).collect();
    let tallies: HashMap<i64, (i64, i64)> = sqlx::query("SELECT story_id, count(*) FILTER (WHERE choice = 'bull') AS b, count(*) FILTER (WHERE choice = 'bear') AS s FROM story_votes WHERE story_id = ANY($1) GROUP BY story_id")
        .bind(&ids)
        .fetch_all(&st.pool)
        .await?
        .iter()
        .map(|r| (r.get("story_id"), (r.get("b"), r.get("s"))))
        .collect();
    Ok(rows
        .iter()
        .map(|r| {
            let id: i64 = r.get("id");
            let author: i64 = r.get("author");
            let own = author == viewer;
            let (b, s) = tallies.get(&id).copied().unwrap_or((0, 0));
            let mut v = json!({
                "id": id,
                "author": cards.get(&author).map(|p| profiles::card(st, p)).unwrap_or_else(|| profiles::deleted_card(author)),
                "kind": r.get::<String, _>("kind"),
                "body": r.get::<String, _>("body"),
                "background": r.get::<Option<String>, _>("background"),
                "media": r.get::<Option<i64>, _>("media").and_then(|m| media.get(&m).cloned()).filter(|m| own || m["status"] == "ready"),
                "tradeCard": r.get::<Option<i64>, _>("trade_card").and_then(|c| trade.get(&c).cloned()),
                "stickers": r.get::<sqlx::types::Json<Value>, _>("stickers").0,
                "sentiment": {"bulls": b, "bears": s, "myVote": my_votes.get(&id)},
                "audience": r.get::<String, _>("audience"),
                "seen": seen.contains(&id),
                "createdAt": r.get::<DateTime<Utc>, _>("created_at"),
                "expiresAt": r.get::<DateTime<Utc>, _>("expires_at"),
                "riskLine": if r.get::<Option<i64>, _>("trade_card").is_some() || r.get::<String, _>("kind") == "chart" { json!(crate::text::RISK_LINE) } else { Value::Null },
            });
            if own {
                v["status"] = json!(r.get::<String, _>("status"));
                v["reason"] = json!(r.get::<Option<String>, _>("reason"));
                v["views"] = json!(r.get::<i64, _>("views"));
            }
            v
        })
        .collect())
}

/// Live stories of `author` that `viewer` may see, oldest first.
pub async fn live_of(st: &AppState, viewer: i64, author: i64) -> ApiResult<Vec<PgRow>> {
    let rows = sqlx::query(
        "SELECT * FROM stories WHERE author = $1 AND expires_at > now() AND (status = 'published' OR author = $2) AND status <> 'deleted' AND status <> 'removed'
           AND ($1 = $2 OR (NOT shadow AND (audience = 'everyone' OR EXISTS (SELECT 1 FROM close_friends c WHERE c.owner = $1 AND c.friend = $2))))
         ORDER BY created_at",
    )
    .bind(author)
    .bind(viewer)
    .fetch_all(&st.pool)
    .await?;
    Ok(rows)
}

/// The stories tray: me first, then followed members with live stories (unseen first, newest first).
pub async fn tray(st: &AppState, me: &Me) -> ApiResult<Value> {
    let ex = profiles::exclusions(st, me.id()).await?;
    let rows = sqlx::query(
        "SELECT s.author, max(s.created_at) AS latest, count(*) AS n,
                bool_and(EXISTS (SELECT 1 FROM story_views v WHERE v.story_id = s.id AND v.viewer = $1)) AS all_seen
         FROM stories s JOIN profiles a ON a.user_id = s.author
         WHERE s.expires_at > now() AND s.status = 'published' AND NOT s.shadow AND a.status = 'active' AND NOT a.shadow_hidden
           AND (s.author = $1 OR (s.author IN (SELECT followee FROM follows WHERE follower = $1 AND status = 'active')
                AND (s.audience = 'everyone' OR EXISTS (SELECT 1 FROM close_friends c WHERE c.owner = s.author AND c.friend = $1))))
         GROUP BY s.author",
    )
    .bind(me.id())
    .fetch_all(&st.pool)
    .await?;
    let mut items: Vec<(i64, DateTime<Utc>, i64, bool)> = rows
        .iter()
        .map(|r| (r.get::<i64, _>("author"), r.get::<DateTime<Utc>, _>("latest"), r.get::<i64, _>("n"), r.get::<Option<bool>, _>("all_seen").unwrap_or(false)))
        .filter(|(a, ..)| *a == me.id() || (!ex.muted_stories.contains(a) && !ex.blocked.contains(a)))
        .collect();
    items.sort_by(|a, b| (b.0 == me.id()).cmp(&(a.0 == me.id())).then(a.3.cmp(&b.3)).then(b.1.cmp(&a.1)));
    let cards = profiles::cards(st, &items.iter().map(|i| i.0).collect::<Vec<_>>()).await?;
    Ok(json!({"items": items.iter().filter_map(|(a, latest, n, seen)| Some(json!({"author": cards.get(a)?, "latestAt": latest, "count": n, "allSeen": seen, "isMe": *a == me.id()}))).collect::<Vec<_>>()}))
}
