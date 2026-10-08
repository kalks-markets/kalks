//! Chat (Q25–Q28): DMs (strangers land in Requests until accepted), private groups (≤ 100), public symbol rooms
//! (#gold, #eurusd, …) and masters' follower rooms. Text, photos, chart snapshots, trade cards, shared posts,
//! voice notes and files; typing and read receipts over the stream. Messages are stored; staff read a chat only
//! through a report or a legal request (admin chat access, audited). No paid rooms.
//!
//! Text passes the link / keyword rules when sent and is AI-checked right after delivery (hidden if it breaks the
//! rules); media is delivered once its check passed (`pending` until then).

use crate::api::Me;
use crate::error::{ApiError, ApiResult, conflict, denied, invalid};
use crate::profiles;
use crate::state::{AppState, Target};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::Row;
use sqlx::postgres::PgRow;
use std::collections::HashMap;
use std::time::Duration;

pub async fn conv(st: &AppState, id: i64) -> ApiResult<PgRow> {
    sqlx::query("SELECT * FROM conversations WHERE id = $1").bind(id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)
}

pub async fn member(st: &AppState, conv: i64, user: i64) -> ApiResult<Option<PgRow>> {
    Ok(sqlx::query("SELECT * FROM conv_members WHERE conversation_id = $1 AND user_id = $2").bind(conv).bind(user).fetch_optional(&st.pool).await?)
}

/// The conversation and the caller's membership in one of `states`.
pub async fn require(st: &AppState, id: i64, user: i64, states: &[&str]) -> ApiResult<(PgRow, PgRow)> {
    let c = conv(st, id).await?;
    let m = member(st, id, user).await?.filter(|m| states.contains(&m.get::<String, _>("state").as_str())).ok_or(ApiError::NotFound)?;
    Ok((c, m))
}

/// Conversation ids a socket of `user` follows (active members and open requests).
pub async fn member_ids(st: &AppState, user: i64) -> anyhow::Result<Vec<i64>> {
    Ok(sqlx::query_scalar("SELECT conversation_id FROM conv_members WHERE user_id = $1 AND state IN ('active','request')").bind(user).fetch_all(&st.pool).await?)
}

pub async fn messages_json(st: &AppState, viewer: i64, rows: &[PgRow]) -> ApiResult<Vec<Value>> {
    let senders: Vec<i64> = rows.iter().map(|r| r.get("sender")).collect();
    let cards = profiles::many(st, &senders).await?;
    let media_ids: Vec<i64> = rows.iter().filter_map(|r| r.get::<Option<i64>, _>("media")).collect();
    let media = crate::media::many(st, &media_ids, viewer).await?;
    let card_ids: Vec<i64> = rows.iter().filter_map(|r| r.get::<Option<i64>, _>("trade_card")).collect();
    let trade: HashMap<i64, Value> = if card_ids.is_empty() {
        HashMap::new()
    } else {
        sqlx::query("SELECT * FROM trade_cards WHERE id = ANY($1)").bind(&card_ids).fetch_all(&st.pool).await?.iter().map(|r| (r.get::<i64, _>("id"), crate::tradecards::json(st, r, viewer, None))).collect()
    };
    let reply_ids: Vec<i64> = rows.iter().filter_map(|r| r.get::<Option<i64>, _>("reply_to")).collect();
    let replies: HashMap<i64, Value> = if reply_ids.is_empty() {
        HashMap::new()
    } else {
        sqlx::query("SELECT m.id, m.body, m.kind, m.deleted_at, p.handle FROM messages m LEFT JOIN profiles p ON p.user_id = m.sender WHERE m.id = ANY($1)")
            .bind(&reply_ids)
            .fetch_all(&st.pool)
            .await?
            .iter()
            .map(|r| {
                let gone = r.get::<Option<DateTime<Utc>>, _>("deleted_at").is_some();
                (r.get::<i64, _>("id"), json!({"id": r.get::<i64, _>("id"), "kind": r.get::<String, _>("kind"), "preview": if gone { String::new() } else { crate::util::preview(&r.get::<String, _>("body"), 80) }, "handle": r.get::<Option<String>, _>("handle"), "deleted": gone}))
            })
            .collect()
    };
    Ok(rows
        .iter()
        .map(|r| {
            let sender: i64 = r.get("sender");
            let own = sender == viewer;
            let deleted = r.get::<Option<DateTime<Utc>>, _>("deleted_at").is_some();
            let hidden = r.get::<String, _>("status") == "hidden";
            let mut v = json!({
                "id": r.get::<i64, _>("id"),
                "conversationId": r.get::<i64, _>("conversation_id"),
                "sender": cards.get(&sender).map(|p| profiles::card(st, p)).unwrap_or_else(|| profiles::deleted_card(sender)),
                "kind": r.get::<String, _>("kind"),
                "body": if deleted || (hidden && !own) { String::new() } else { r.get::<String, _>("body") },
                "media": if deleted { Value::Null } else { r.get::<Option<i64>, _>("media").and_then(|m| media.get(&m).cloned()).filter(|m| own || m["status"] == "ready").unwrap_or(Value::Null) },
                "tradeCard": if deleted { Value::Null } else { r.get::<Option<i64>, _>("trade_card").and_then(|c| trade.get(&c).cloned()).unwrap_or(Value::Null) },
                "postId": r.get::<Option<i64>, _>("post_id"),
                "replyTo": r.get::<Option<i64>, _>("reply_to").and_then(|x| replies.get(&x).cloned()),
                "deleted": deleted,
                "createdAt": r.get::<DateTime<Utc>, _>("created_at"),
                "editedAt": r.get::<Option<DateTime<Utc>>, _>("edited_at"),
                "clientId": r.get::<Option<String>, _>("client_id"),
            });
            if own {
                v["status"] = json!(r.get::<String, _>("status"));
            }
            v
        })
        .collect())
}

pub async fn conv_json(st: &AppState, viewer: i64, c: &PgRow) -> ApiResult<Value> {
    let id: i64 = c.get("id");
    let kind: String = c.get("kind");
    let me = member(st, id, viewer).await?;
    let small = kind == "dm" || kind == "group";
    let members: Vec<PgRow> = if small {
        sqlx::query("SELECT user_id, role, state, last_read_id FROM conv_members WHERE conversation_id = $1 AND state IN ('active','request') ORDER BY joined_at LIMIT 100").bind(id).fetch_all(&st.pool).await?
    } else {
        vec![]
    };
    let ids: Vec<i64> = members.iter().map(|m| m.get("user_id")).collect();
    let cards = profiles::cards(st, &ids).await?;
    let last = match c.get::<Option<i64>, _>("last_message_id") {
        Some(m) => {
            let rows = sqlx::query("SELECT * FROM messages WHERE id = $1 AND (status = 'sent' OR sender = $2)").bind(m).bind(viewer).fetch_all(&st.pool).await?;
            messages_json(st, viewer, &rows).await?.into_iter().next()
        }
        None => None,
    };
    let other = if kind == "dm" { ids.iter().find(|u| **u != viewer).and_then(|u| cards.get(u).cloned()) } else { None };
    let online = if kind == "dm" { ids.iter().find(|u| **u != viewer).map(|u| st.hub.is_online(*u)) } else { None };
    let avatar = match c.get::<Option<i64>, _>("avatar_media") {
        Some(m) => crate::media::many(st, &[m], viewer).await?.remove(&m),
        None => None,
    };
    Ok(json!({
        "id": id,
        "kind": kind,
        "title": if kind == "dm" { other.as_ref().and_then(|o| o["displayName"].as_str().map(str::to_string)).unwrap_or_default() } else { c.get::<String, _>("title") },
        "about": c.get::<String, _>("about"),
        "avatar": avatar,
        "symbol": c.get::<Option<String>, _>("symbol"),
        "slug": c.get::<Option<String>, _>("slug"),
        "status": c.get::<String, _>("status"),
        "memberCount": c.get::<i32, _>("member_count"),
        "other": other,
        "otherOnline": online,
        "members": if small { json!(members.iter().map(|m| json!({"user": cards.get(&m.get::<i64, _>("user_id")), "role": m.get::<String, _>("role"), "state": m.get::<String, _>("state"), "lastReadId": m.get::<i64, _>("last_read_id")})).collect::<Vec<_>>()) } else { Value::Null },
        "me": me.as_ref().map(|m| json!({"role": m.get::<String, _>("role"), "state": m.get::<String, _>("state"), "muted": m.get::<bool, _>("muted"), "unread": m.get::<i32, _>("unread"), "lastReadId": m.get::<i64, _>("last_read_id")})),
        "lastMessage": last,
        "lastMessageAt": c.get::<Option<DateTime<Utc>>, _>("last_message_at"),
        "createdAt": c.get::<DateTime<Utc>, _>("created_at"),
    }))
}

async fn add_member(st: &AppState, conv: i64, user: i64, role: &str, state: &str) -> ApiResult<()> {
    let inserted: Option<bool> = sqlx::query_scalar(
        "INSERT INTO conv_members (conversation_id, user_id, role, state) VALUES ($1,$2,$3,$4)
         ON CONFLICT (conversation_id, user_id) DO UPDATE SET state = EXCLUDED.state, role = CASE WHEN conv_members.role = 'owner' THEN 'owner' ELSE EXCLUDED.role END,
             joined_at = CASE WHEN conv_members.state IN ('active','request') THEN conv_members.joined_at ELSE now() END
         RETURNING (xmax = 0)",
    )
    .bind(conv)
    .bind(user)
    .bind(role)
    .bind(state)
    .fetch_optional(&st.pool)
    .await?;
    if inserted.is_some() {
        recount(st, conv).await?;
    }
    st.hub.send(Target::User(user), json!({"type": "conversation.joined", "conversationId": conv, "state": state}));
    Ok(())
}

pub async fn recount(st: &AppState, conv: i64) -> anyhow::Result<()> {
    sqlx::query("UPDATE conversations SET member_count = (SELECT count(*) FROM conv_members WHERE conversation_id = $1 AND state = 'active') WHERE id = $1").bind(conv).execute(&st.pool).await?;
    Ok(())
}

/// Opens (or returns) the DM with `target`.
pub async fn open_dm(st: &AppState, me: &Me, target: &profiles::Profile) -> ApiResult<i64> {
    if target.user_id == me.id() {
        return Err(invalid("handle", "You can't message yourself."));
    }
    if target.is_banned() || profiles::blocked_between(st, me.id(), target.user_id).await? {
        return Err(denied("blocked", "You can't message this member."));
    }
    let key = format!("{}:{}", me.id().min(target.user_id), me.id().max(target.user_id));
    if let Some(id) = sqlx::query_scalar::<_, i64>("SELECT id FROM conversations WHERE dm_key = $1").bind(&key).fetch_optional(&st.pool).await? {
        // re-open for me if I had left / declined
        sqlx::query("UPDATE conv_members SET state = 'active' WHERE conversation_id = $1 AND user_id = $2 AND state IN ('left','declined')").bind(id).bind(me.id()).execute(&st.pool).await?;
        return Ok(id);
    }
    let they_follow_me = profiles::is_follower(st, target.user_id, me.id()).await?;
    match target.dm_policy.as_str() {
        "none" if !they_follow_me => return Err(denied("dms_off", "This member doesn't accept new messages.")),
        "following" if !they_follow_me => return Err(denied("dms_limited", "This member only accepts messages from people they follow.")),
        _ => {}
    }
    if !they_follow_me {
        let s = profiles::settings(st).await;
        if !st.limiter.hit(&format!("dmreq:{}", me.id()), s.dm_requests_per_day.max(1) as usize, Duration::from_secs(86_400)) {
            return Err(ApiError::RateLimited("You've sent many message requests today.".into()));
        }
    }
    let id: Option<i64> = sqlx::query_scalar("INSERT INTO conversations (kind, dm_key, owner) VALUES ('dm', $1, $2) ON CONFLICT (dm_key) DO NOTHING RETURNING id").bind(&key).bind(me.id()).fetch_optional(&st.pool).await?;
    let id = match id {
        Some(i) => i,
        None => return Ok(sqlx::query_scalar("SELECT id FROM conversations WHERE dm_key = $1").bind(&key).fetch_one(&st.pool).await?),
    };
    add_member(st, id, me.id(), "member", "active").await?;
    add_member(st, id, target.user_id, "member", if they_follow_me { "active" } else { "request" }).await?;
    Ok(id)
}

pub async fn create_group(st: &AppState, me: &Me, title: &str, handles: &[String]) -> ApiResult<i64> {
    let title = crate::util::clean(title, 60);
    if title.is_empty() {
        return Err(invalid("title", "Give the group a name."));
    }
    let s = profiles::settings(st).await;
    if handles.len() as i64 + 1 > s.max_group_members {
        return Err(invalid("members", format!("Groups have up to {} members.", s.max_group_members)));
    }
    let id: i64 = sqlx::query_scalar("INSERT INTO conversations (kind, title, owner) VALUES ('group', $1, $2) RETURNING id").bind(&title).bind(me.id()).fetch_one(&st.pool).await?;
    add_member(st, id, me.id(), "owner", "active").await?;
    add_members(st, me, id, handles).await?;
    Ok(id)
}

pub async fn add_members(st: &AppState, me: &Me, id: i64, handles: &[String]) -> ApiResult<Vec<i64>> {
    let c = conv(st, id).await?;
    if c.get::<String, _>("kind") != "group" {
        return Err(invalid("id", "Members are added to groups only."));
    }
    let s = profiles::settings(st).await;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM conv_members WHERE conversation_id = $1 AND state IN ('active','request')").bind(id).fetch_one(&st.pool).await?;
    if count + handles.len() as i64 > s.max_group_members {
        return Err(invalid("members", format!("Groups have up to {} members.", s.max_group_members)));
    }
    let mut added = Vec::new();
    for h in handles.iter().take(100) {
        let Ok(p) = profiles::by_handle(st, h).await else { continue };
        if p.user_id == me.id() || p.is_banned() || profiles::blocked_between(st, me.id(), p.user_id).await? || p.dm_policy == "none" {
            continue;
        }
        // members who don't follow the inviter get the group as a request
        let follows = profiles::is_follower(st, p.user_id, me.id()).await?;
        if p.dm_policy == "following" && !follows {
            continue;
        }
        add_member(st, id, p.user_id, "member", if follows { "active" } else { "request" }).await?;
        crate::notify::event(st, crate::notify::Ev::new(p.user_id, "group_invite", me.id()).conversation(id).data(json!({"title": c.get::<String, _>("title")}))).await?;
        added.push(p.user_id);
    }
    Ok(added)
}

pub async fn remove_member(st: &AppState, id: i64, user: i64, state: &str) -> ApiResult<()> {
    sqlx::query("UPDATE conv_members SET state = $3 WHERE conversation_id = $1 AND user_id = $2").bind(id).bind(user).bind(state).execute(&st.pool).await?;
    recount(st, id).await?;
    st.hub.send(Target::User(user), json!({"type": "conversation.left", "conversationId": id}));
    Ok(())
}

/// Joins a public symbol room or a master's follower room.
pub async fn join_room(st: &AppState, me: &Me, id: i64) -> ApiResult<()> {
    let c = conv(st, id).await?;
    match c.get::<String, _>("kind").as_str() {
        "room" => {}
        "master_room" => {
            let master: i64 = c.get::<Option<i64>, _>("master_user").unwrap_or(0);
            if master != me.id() && !profiles::is_follower(st, me.id(), master).await? {
                return Err(denied("follow_required", "Follow this trader to join their room."));
            }
            if profiles::blocked_between(st, me.id(), master).await? {
                return Err(ApiError::NotFound);
            }
        }
        _ => return Err(ApiError::NotFound),
    }
    if c.get::<String, _>("status") == "archived" {
        return Err(ApiError::NotFound);
    }
    add_member(st, id, me.id(), "member", "active").await
}

/// The master's follower room (created on first use). Requires a live copy-trading master / PAMM manager.
pub async fn master_room(st: &AppState, me: &Me) -> ApiResult<i64> {
    if me.p.master_id.is_none() {
        return Err(denied("not_master", "Follower rooms are for copy-trading masters and PAMM managers."));
    }
    if let Some(id) = sqlx::query_scalar::<_, i64>("SELECT id FROM conversations WHERE kind = 'master_room' AND master_user = $1").bind(me.id()).fetch_optional(&st.pool).await? {
        return Ok(id);
    }
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO conversations (kind, title, about, master_user, master_tenant, owner) VALUES ('master_room', $1, $2, $3, $4, $3)
         ON CONFLICT (master_user) WHERE kind = 'master_room' DO UPDATE SET title = conversations.title RETURNING id",
    )
    .bind(format!("{}'s room", me.p.display_name))
    .bind("Followers' room. Not investment advice.")
    .bind(me.id())
    .bind(&me.p.tenant)
    .fetch_one(&st.pool)
    .await?;
    add_member(st, id, me.id(), "owner", "active").await?;
    Ok(id)
}

fn kind_for(media_kind: Option<&str>, card: bool, post: bool, story: bool) -> &'static str {
    if card {
        "trade_card"
    } else if story {
        "story_reply"
    } else if post {
        "post"
    } else {
        match media_kind {
            Some("photo") => "photo",
            Some("video") => "video",
            Some("voice") => "voice",
            Some("file") => "file",
            Some("chart") => "chart",
            _ => "text",
        }
    }
}

/// Sends a message. Replying to a request accepts it.
pub async fn send(st: &AppState, me: &Me, id: i64, b: &Value) -> ApiResult<Value> {
    let (c, m) = require(st, id, me.id(), &["active", "request"]).await?;
    let kind: String = c.get("kind");
    if c.get::<String, _>("status") != "active" && m.get::<String, _>("role") == "member" {
        return Err(denied("locked", "This conversation is locked."));
    }
    let s = profiles::settings(st).await;
    if !st.limiter.hit(&format!("msg:{}", me.id()), s.messages_per_minute.max(1) as usize, Duration::from_secs(60)) {
        return Err(ApiError::RateLimited("You're sending messages very fast.".into()));
    }
    if kind == "dm" {
        let other: Option<i64> = sqlx::query_scalar("SELECT user_id FROM conv_members WHERE conversation_id = $1 AND user_id <> $2").bind(id).bind(me.id()).fetch_optional(&st.pool).await?;
        if let Some(o) = other
            && profiles::blocked_between(st, me.id(), o).await?
        {
            return Err(denied("blocked", "You can't message this member."));
        }
    }
    let body = crate::util::clean(b["body"].as_str().unwrap_or(""), 4001);
    if body.chars().count() > 4000 {
        return Err(invalid("body", "Messages can be up to 4000 characters."));
    }
    let media_id = b["mediaId"].as_i64();
    let media = crate::media::attachable(st, me.id(), &media_id.into_iter().collect::<Vec<_>>(), &["chat"]).await?;
    let card = b["tradeCardId"].as_i64();
    if let Some(cid) = card {
        let owner: Option<i64> = sqlx::query_scalar("SELECT owner FROM trade_cards WHERE id = $1").bind(cid).fetch_optional(&st.pool).await?;
        if owner != Some(me.id()) {
            return Err(invalid("tradeCardId", "Share a trade card of your own."));
        }
    }
    let post = b["postId"].as_i64();
    if let Some(p) = post {
        crate::posts::visible(st, me.id(), p).await.map_err(|_| invalid("postId", "This post can't be shared."))?;
    }
    let story = b["storyId"].as_i64();
    if body.is_empty() && media.is_empty() && card.is_none() && post.is_none() {
        return Err(invalid("body", "Write a message."));
    }
    crate::moderation::precheck(st, &body).await?;
    let reply_to = match b["replyTo"].as_i64() {
        Some(r) => {
            let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM messages WHERE id = $1 AND conversation_id = $2)").bind(r).bind(id).fetch_one(&st.pool).await?;
            ok.then_some(r)
        }
        None => None,
    };
    let client_id = b["clientId"].as_str().map(|c| crate::util::clean(c, 64)).filter(|c| !c.is_empty());
    if let Some(cid) = &client_id
        && let Some(existing) = sqlx::query("SELECT * FROM messages WHERE conversation_id = $1 AND sender = $2 AND client_id = $3").bind(id).bind(me.id()).bind(cid).fetch_optional(&st.pool).await?
    {
        // retried send: the same message
        return Ok(messages_json(st, me.id(), &[existing]).await?.remove(0));
    }
    if m.get::<String, _>("state") == "request" {
        sqlx::query("UPDATE conv_members SET state = 'active' WHERE conversation_id = $1 AND user_id = $2").bind(id).bind(me.id()).execute(&st.pool).await?;
        recount(st, id).await?;
    }
    let media_ready = media.iter().all(|x| x.2 == "ready");
    let mkind = kind_for(media.first().map(|x| x.1.as_str()), card.is_some(), post.is_some(), story.is_some());
    let row = sqlx::query(
        "INSERT INTO messages (conversation_id, sender, kind, body, media, trade_card, post_id, reply_to, client_id, status, checked)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) RETURNING *",
    )
    .bind(id)
    .bind(me.id())
    .bind(mkind)
    .bind(&body)
    .bind(media.first().map(|x| x.0))
    .bind(card)
    .bind(post.or(story))
    .bind(reply_to)
    .bind(&client_id)
    .bind(if media_ready { "sent" } else { "pending" })
    .bind(body.is_empty() || !st.ai())
    .fetch_one(&st.pool)
    .await?;
    crate::media::mark_attached(st, &media.iter().map(|x| x.0).collect::<Vec<_>>()).await?;
    if media_ready {
        deliver(st, &row).await?;
    }
    if !body.is_empty() && st.ai() {
        st.wake.moderation.notify_one();
    }
    Ok(messages_json(st, me.id(), &[row]).await?.remove(0))
}

/// Fan-out of a sent message: last message, unread counters, the stream, push / bell for DMs, requests and groups.
pub async fn deliver(st: &AppState, r: &PgRow) -> anyhow::Result<()> {
    let id: i64 = r.get("id");
    let conv_id: i64 = r.get("conversation_id");
    let sender: i64 = r.get("sender");
    sqlx::query("UPDATE conversations SET last_message_id = $2, last_message_at = now() WHERE id = $1").bind(conv_id).bind(id).execute(&st.pool).await?;
    let recipients = sqlx::query("UPDATE conv_members SET unread = unread + 1 WHERE conversation_id = $1 AND user_id <> $2 AND state IN ('active','request') RETURNING user_id, state, muted")
        .bind(conv_id)
        .bind(sender)
        .fetch_all(&st.pool)
        .await?;
    sqlx::query("UPDATE conv_members SET last_read_id = GREATEST(last_read_id, $3), unread = 0 WHERE conversation_id = $1 AND user_id = $2").bind(conv_id).bind(sender).bind(id).execute(&st.pool).await?;
    let c = sqlx::query("SELECT kind, title FROM conversations WHERE id = $1").bind(conv_id).fetch_one(&st.pool).await?;
    let kind: String = c.get("kind");
    // the stream: each member renders it (sender card, media urls) — one generic payload, viewer-specific bits are the sender's own
    let payload = messages_json(st, 0, std::slice::from_ref(r)).await.unwrap_or_default().into_iter().next().unwrap_or(Value::Null);
    st.hub.send(Target::Conv(conv_id), json!({"type": "message", "conversationId": conv_id, "message": payload}));
    if kind == "room" {
        return Ok(()); // public rooms: no push
    }
    let preview = match r.get::<String, _>("kind").as_str() {
        "text" | "story_reply" => crate::util::preview(&r.get::<String, _>("body"), 120),
        "photo" => "Photo".into(),
        "video" => "Video".into(),
        "voice" => "Voice message".into(),
        "file" => "File".into(),
        "chart" => "Chart".into(),
        "trade_card" => "Shared a trade".into(),
        "post" => "Shared a post".into(),
        _ => String::new(),
    };
    for m in recipients.iter().take(200) {
        let to: i64 = m.get("user_id");
        if m.get::<bool, _>("muted") {
            continue;
        }
        let state: String = m.get("state");
        if state == "request" {
            // one request notification per conversation (the first message)
            let first: bool = sqlx::query_scalar("SELECT count(*) = 1 FROM messages WHERE conversation_id = $1 AND sender = $2").bind(conv_id).bind(sender).fetch_one(&st.pool).await?;
            if first {
                crate::notify::event(st, crate::notify::Ev::new(to, "dm_request", sender).conversation(conv_id).data(json!({"preview": preview}))).await?;
            }
            continue;
        }
        crate::notify::event(st, crate::notify::Ev::new(to, "dm", sender).conversation(conv_id).data(json!({"preview": preview, "kind": kind, "title": c.get::<String, _>("title")}))).await?;
    }
    Ok(())
}

/// Messages waiting for their media: delivered once it is ready, hidden when it was rejected.
pub async fn deliver_pending(st: &AppState) -> anyhow::Result<usize> {
    let rows = sqlx::query(
        "SELECT m.*, md.status AS media_status FROM messages m JOIN media md ON md.id = m.media WHERE m.status = 'pending' AND md.status NOT IN ('processing','uploading') ORDER BY m.id LIMIT 100",
    )
    .fetch_all(&st.pool)
    .await?;
    for r in &rows {
        let id: i64 = r.get("id");
        match r.get::<String, _>("media_status").as_str() {
            "ready" => {
                let row = sqlx::query("UPDATE messages SET status = 'sent' WHERE id = $1 AND status = 'pending' RETURNING *").bind(id).fetch_optional(&st.pool).await?;
                if let Some(row) = row {
                    deliver(st, &row).await?;
                }
            }
            // review: a moderator decides (the message stays pending); rejected / failed: never delivered
            "review" => {}
            _ => {
                sqlx::query("UPDATE messages SET status = 'hidden' WHERE id = $1").bind(id).execute(&st.pool).await?;
                st.hub.send(Target::User(r.get("sender")), json!({"type": "message.updated", "conversationId": r.get::<i64, _>("conversation_id"), "messageId": id, "status": "hidden"}));
            }
        }
    }
    Ok(rows.len())
}

/// The AI check of chat text after delivery: a message that breaks the rules is hidden for everyone.
pub async fn check_unchecked(st: &AppState) -> anyhow::Result<usize> {
    if !st.ai() {
        sqlx::query("UPDATE messages SET checked = true WHERE NOT checked").execute(&st.pool).await?;
        return Ok(0);
    }
    let rows = sqlx::query("SELECT m.*, c.kind AS conv_kind FROM messages m JOIN conversations c ON c.id = m.conversation_id WHERE NOT m.checked AND m.deleted_at IS NULL ORDER BY m.id LIMIT 20").fetch_all(&st.pool).await?;
    for r in &rows {
        let id: i64 = r.get("id");
        let kind = format!("chat message ({})", r.get::<String, _>("conv_kind"));
        match crate::moderation::classify_text(st, &kind, &r.get::<String, _>("body")).await {
            Ok(v) => {
                sqlx::query("UPDATE messages SET checked = true, moderation = $2 WHERE id = $1").bind(id).bind(sqlx::types::Json(v.json())).execute(&st.pool).await?;
                if v.decision != crate::moderation::Decision::Allow {
                    let sender: i64 = r.get("sender");
                    let tenant: String = sqlx::query_scalar("SELECT tenant FROM profiles WHERE user_id = $1").bind(sender).fetch_optional(&st.pool).await?.unwrap_or_else(|| "kalks".into());
                    crate::moderation::queue(st, "message", id, sender, &tenant, "ai", &v, &r.get::<String, _>("body")).await?;
                    if v.decision == crate::moderation::Decision::Block {
                        sqlx::query("UPDATE messages SET status = 'hidden' WHERE id = $1").bind(id).execute(&st.pool).await?;
                        let conv_id: i64 = r.get("conversation_id");
                        st.hub.send(Target::Conv(conv_id), json!({"type": "message.hidden", "conversationId": conv_id, "messageId": id}));
                        crate::notify::event(st, crate::notify::Ev { to: sender, kind: "moderation", actor: None, post: None, comment: None, story: None, conversation: Some(conv_id), data: json!({"title": "A message you sent was hidden", "reason": crate::moderation::main_reason(&v)}) }).await?;
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "chat check failed; will retry");
                break;
            }
        }
    }
    Ok(rows.len())
}

pub async fn list(st: &AppState, me: &Me, box_: &str, page: &crate::api::Page) -> ApiResult<Value> {
    let limit = page.limit(30, 100);
    let before = page.cursor.as_deref().and_then(|c| c.parse::<i64>().ok()).unwrap_or(i64::MAX);
    let (states, need_msg) = match box_ {
        "requests" => ("'request'", true),
        "rooms" => ("'active'", false),
        _ => ("'active'", false),
    };
    let kinds = match box_ {
        "rooms" => "('room','master_room')",
        "requests" => "('dm','group')",
        _ => "('dm','group','room','master_room')",
    };
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT c.*, COALESCE(c.last_message_id, 0) AS sort_key FROM conversations c JOIN conv_members m ON m.conversation_id = c.id AND m.user_id = $1
         WHERE m.state IN ({states}) AND c.kind IN {kinds} AND c.status <> 'archived' {} AND COALESCE(c.last_message_id, 0) < $2
         ORDER BY COALESCE(c.last_message_id, 0) DESC, c.id DESC LIMIT $3",
        if need_msg { "AND c.last_message_id IS NOT NULL" } else { "AND (c.kind <> 'dm' OR c.last_message_id IS NOT NULL OR c.owner = $1)" }
    )))
    .bind(me.id())
    .bind(before)
    .bind(limit + 1)
    .fetch_all(&st.pool)
    .await?;
    let more = rows.len() as i64 > limit;
    let mut items = Vec::new();
    for r in rows.iter().take(limit as usize) {
        items.push(conv_json(st, me.id(), r).await?);
    }
    let next = if more { rows.get(limit as usize - 1).map(|r| r.get::<i64, _>("sort_key").to_string()) } else { None };
    let requests: i64 = sqlx::query_scalar("SELECT count(*) FROM conv_members m JOIN conversations c ON c.id = m.conversation_id WHERE m.user_id = $1 AND m.state = 'request' AND c.last_message_id IS NOT NULL").bind(me.id()).fetch_one(&st.pool).await?;
    let unread: i64 = sqlx::query_scalar("SELECT COALESCE(sum(unread),0)::bigint FROM conv_members m JOIN conversations c ON c.id = m.conversation_id WHERE m.user_id = $1 AND m.state = 'active' AND NOT m.muted AND c.kind IN ('dm','group')").bind(me.id()).fetch_one(&st.pool).await?;
    Ok(json!({"items": items, "nextCursor": next, "requests": requests, "unread": unread}))
}

pub async fn history(st: &AppState, me: &Me, id: i64, before: Option<i64>, after: Option<i64>, limit: i64) -> ApiResult<Value> {
    let (_c, m) = require(st, id, me.id(), &["active", "request"]).await?;
    // members see the history from when they joined (rooms / groups), DMs see everything
    let joined: DateTime<Utc> = m.get("joined_at");
    let rows = if let Some(a) = after {
        sqlx::query("SELECT * FROM messages WHERE conversation_id = $1 AND id > $2 AND (status = 'sent' OR sender = $3) AND (created_at >= $4 - interval '7 days' OR (SELECT kind FROM conversations WHERE id = $1) IN ('dm','room','master_room')) ORDER BY id ASC LIMIT $5")
            .bind(id)
            .bind(a)
            .bind(me.id())
            .bind(joined)
            .bind(limit + 1)
            .fetch_all(&st.pool)
            .await?
    } else {
        sqlx::query("SELECT * FROM messages WHERE conversation_id = $1 AND id < $2 AND (status = 'sent' OR sender = $3) AND (created_at >= $4 - interval '7 days' OR (SELECT kind FROM conversations WHERE id = $1) IN ('dm','room','master_room')) ORDER BY id DESC LIMIT $5")
            .bind(id)
            .bind(before.unwrap_or(i64::MAX))
            .bind(me.id())
            .bind(joined)
            .bind(limit + 1)
            .fetch_all(&st.pool)
            .await?
    };
    let ex = profiles::exclusions(st, me.id()).await?;
    let more = rows.len() as i64 > limit;
    let rows: Vec<PgRow> = rows.into_iter().take(limit as usize).filter(|r| !ex.blocked.contains(&r.get::<i64, _>("sender"))).collect();
    let items = messages_json(st, me.id(), &rows).await?;
    let next = if more { items.last().and_then(|v| v["id"].as_i64()) } else { None };
    Ok(json!({"items": items, "more": more, "nextBefore": if after.is_none() { next } else { None }, "nextAfter": if after.is_some() { next } else { None }}))
}

pub async fn read(st: &AppState, me: &Me, id: i64, message: Option<i64>) -> ApiResult<()> {
    require(st, id, me.id(), &["active", "request"]).await?;
    let upto: i64 = match message {
        Some(m) => m,
        None => sqlx::query_scalar("SELECT COALESCE(max(id), 0) FROM messages WHERE conversation_id = $1").bind(id).fetch_one(&st.pool).await?,
    };
    sqlx::query("UPDATE conv_members SET last_read_id = GREATEST(last_read_id, $3), unread = (SELECT count(*) FROM messages WHERE conversation_id = $1 AND id > GREATEST(conv_members.last_read_id, $3) AND sender <> $2 AND status = 'sent') WHERE conversation_id = $1 AND user_id = $2")
        .bind(id)
        .bind(me.id())
        .bind(upto)
        .execute(&st.pool)
        .await?;
    let kind: String = sqlx::query_scalar("SELECT kind FROM conversations WHERE id = $1").bind(id).fetch_one(&st.pool).await?;
    // read receipts in DMs and groups (not in public rooms)
    if kind == "dm" || kind == "group" {
        st.hub.send(Target::Conv(id), json!({"type": "read", "conversationId": id, "userId": me.id(), "messageId": upto}));
    }
    Ok(())
}

pub async fn typing(st: &AppState, me: &Me, id: i64) -> ApiResult<()> {
    let (c, m) = require(st, id, me.id(), &["active"]).await?;
    let _ = m;
    if st.limiter.hit(&format!("typing:{}:{id}", me.id()), 1, Duration::from_secs(2)) && c.get::<String, _>("kind") != "room" {
        st.hub.send_except(Target::Conv(id), json!({"type": "typing", "conversationId": id, "user": {"id": me.id(), "handle": me.p.handle, "displayName": me.p.display_name}}), me.id());
    }
    Ok(())
}

pub async fn edit_message(st: &AppState, me: &Me, id: i64, body: &str) -> ApiResult<Value> {
    let r = sqlx::query("SELECT * FROM messages WHERE id = $1 AND sender = $2 AND deleted_at IS NULL").bind(id).bind(me.id()).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    if Utc::now() - r.get::<DateTime<Utc>, _>("created_at") > chrono::Duration::minutes(15) {
        return Err(conflict("edit_window", "Messages can be edited for 15 minutes."));
    }
    let body = crate::util::clean(body, 4001);
    if body.is_empty() || body.chars().count() > 4000 {
        return Err(invalid("body", "Messages are 1 to 4000 characters."));
    }
    crate::moderation::precheck(st, &body).await?;
    let row = sqlx::query("UPDATE messages SET body = $2, edited_at = now(), checked = $3 WHERE id = $1 RETURNING *").bind(id).bind(&body).bind(!st.ai()).fetch_one(&st.pool).await?;
    let conv_id: i64 = row.get("conversation_id");
    let v = messages_json(st, 0, std::slice::from_ref(&row)).await?.remove(0);
    st.hub.send(Target::Conv(conv_id), json!({"type": "message.updated", "conversationId": conv_id, "message": v}));
    st.wake.moderation.notify_one();
    Ok(messages_json(st, me.id(), &[row]).await?.remove(0))
}

pub async fn delete_message(st: &AppState, me: &Me, id: i64) -> ApiResult<()> {
    let r = sqlx::query("SELECT m.conversation_id, m.sender, cm.role FROM messages m LEFT JOIN conv_members cm ON cm.conversation_id = m.conversation_id AND cm.user_id = $2 WHERE m.id = $1 AND m.deleted_at IS NULL")
        .bind(id)
        .bind(me.id())
        .fetch_optional(&st.pool)
        .await?
        .ok_or(ApiError::NotFound)?;
    let admin = matches!(r.get::<Option<String>, _>("role").as_deref(), Some("owner" | "admin"));
    if r.get::<i64, _>("sender") != me.id() && !admin {
        return Err(ApiError::Forbidden("You can't delete this message.".into()));
    }
    // kept for compliance (reports / legal requests), shown as deleted
    sqlx::query("UPDATE messages SET deleted_at = now() WHERE id = $1").bind(id).execute(&st.pool).await?;
    let conv_id: i64 = r.get("conversation_id");
    st.hub.send(Target::Conv(conv_id), json!({"type": "message.deleted", "conversationId": conv_id, "messageId": id}));
    Ok(())
}
