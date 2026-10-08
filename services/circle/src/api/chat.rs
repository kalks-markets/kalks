//! Chat routes: conversations (inbox / requests / rooms), DMs, groups, rooms, master rooms, members, messages,
//! read receipts and typing.

use super::{Body, Me};
use crate::chat;
use crate::error::{ApiError, ApiResult, invalid};
use crate::profiles;
use crate::state::{AppState, Target};
use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;

type R = ApiResult<Json<Value>>;

#[derive(Deserialize)]
pub struct ListQ {
    #[serde(rename = "box")]
    box_: Option<String>,
    cursor: Option<String>,
    limit: Option<i64>,
}

pub async fn list(State(st): State<AppState>, me: Me, Query(q): Query<ListQ>) -> R {
    let page = super::Page { cursor: q.cursor, limit: q.limit };
    Ok(Json(chat::list(&st, &me, q.box_.as_deref().unwrap_or("inbox"), &page).await?))
}

async fn conv_out(st: &AppState, me: &Me, id: i64) -> R {
    let c = chat::conv(st, id).await?;
    Ok(Json(json!({"conversation": chat::conv_json(st, me.id(), &c).await?})))
}

pub async fn dm(State(st): State<AppState>, me: Me, Body(b): Body) -> R {
    let handle = b["handle"].as_str().ok_or_else(|| invalid("handle", "Who do you want to message?"))?;
    let p = profiles::by_handle(&st, handle).await?;
    let id = chat::open_dm(&st, &me, &p).await?;
    conv_out(&st, &me, id).await
}

pub async fn create_group(State(st): State<AppState>, me: Me, Body(b): Body) -> R {
    let handles: Vec<String> = b["members"].as_array().map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
    if handles.is_empty() {
        return Err(invalid("members", "Add at least one member."));
    }
    if !st.limiter.hit(&format!("group:{}", me.id()), 20, std::time::Duration::from_secs(86_400)) {
        return Err(ApiError::RateLimited("You've created many groups today.".into()));
    }
    let id = chat::create_group(&st, &me, b["title"].as_str().unwrap_or(""), &handles).await?;
    conv_out(&st, &me, id).await
}

pub async fn rooms(State(st): State<AppState>, me: Me) -> R {
    let rows = sqlx::query(
        "SELECT c.*, EXISTS (SELECT 1 FROM conv_members m WHERE m.conversation_id = c.id AND m.user_id = $1 AND m.state = 'active') AS joined
         FROM conversations c WHERE c.kind = 'room' AND c.status = 'active' ORDER BY c.member_count DESC, c.id",
    )
    .bind(me.id())
    .fetch_all(&st.pool)
    .await?;
    let syms: Vec<String> = rows.iter().filter_map(|r| r.get::<Option<String>, _>("symbol")).collect();
    let quotes = crate::upstream::quotes(&st, &syms).await;
    // masters' rooms of the traders I follow
    let masters = sqlx::query(
        "SELECT c.*, EXISTS (SELECT 1 FROM conv_members m WHERE m.conversation_id = c.id AND m.user_id = $1 AND m.state = 'active') AS joined
         FROM conversations c WHERE c.kind = 'master_room' AND c.status = 'active'
           AND (c.master_user = $1 OR c.master_user IN (SELECT followee FROM follows WHERE follower = $1 AND status = 'active'))
         ORDER BY c.member_count DESC LIMIT 100",
    )
    .bind(me.id())
    .fetch_all(&st.pool)
    .await?;
    let owners: Vec<i64> = masters.iter().filter_map(|r| r.get::<Option<i64>, _>("master_user")).collect();
    let cards = profiles::cards(&st, &owners).await?;
    let room = |r: &sqlx::postgres::PgRow| {
        json!({
            "id": r.get::<i64, _>("id"), "kind": r.get::<String, _>("kind"), "title": r.get::<String, _>("title"), "about": r.get::<String, _>("about"),
            "symbol": r.get::<Option<String>, _>("symbol"), "slug": r.get::<Option<String>, _>("slug"), "memberCount": r.get::<i32, _>("member_count"),
            "joined": r.get::<bool, _>("joined"), "lastMessageAt": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_message_at"),
        })
    };
    Ok(Json(json!({
        "symbolRooms": rows.iter().map(|r| {
            let mut v = room(r);
            v["quote"] = r.get::<Option<String>, _>("symbol").and_then(|s| quotes.get(&s).cloned()).unwrap_or(Value::Null);
            v
        }).collect::<Vec<_>>(),
        "masterRooms": masters.iter().map(|r| {
            let mut v = room(r);
            v["master"] = r.get::<Option<i64>, _>("master_user").and_then(|m| cards.get(&m).cloned()).unwrap_or(Value::Null);
            v
        }).collect::<Vec<_>>(),
        "paidRooms": false,
    })))
}

pub async fn master_room(State(st): State<AppState>, me: Me) -> R {
    let id = chat::master_room(&st, &me).await?;
    conv_out(&st, &me, id).await
}

pub async fn get(State(st): State<AppState>, me: Me, Path(id): Path<i64>) -> R {
    let c = chat::conv(&st, id).await?;
    let kind: String = c.get("kind");
    // public rooms can be previewed before joining
    if !(kind == "room" && c.get::<String, _>("status") == "active") {
        chat::require(&st, id, me.id(), &["active", "request"]).await?;
    }
    Ok(Json(json!({"conversation": chat::conv_json(&st, me.id(), &c).await?})))
}

pub async fn edit(State(st): State<AppState>, me: Me, Path(id): Path<i64>, Body(b): Body) -> R {
    let (c, m) = chat::require(&st, id, me.id(), &["active"]).await?;
    if !matches!(c.get::<String, _>("kind").as_str(), "group" | "master_room") || !matches!(m.get::<String, _>("role").as_str(), "owner" | "admin") {
        return Err(ApiError::Forbidden("Only group admins can change the group.".into()));
    }
    if let Some(t) = b["title"].as_str() {
        let t = crate::util::clean(t, 60);
        if t.is_empty() {
            return Err(invalid("title", "Give the group a name."));
        }
        crate::moderation::precheck(&st, &t).await?;
        sqlx::query("UPDATE conversations SET title = $2 WHERE id = $1").bind(id).bind(t).execute(&st.pool).await?;
    }
    if let Some(a) = b["about"].as_str() {
        let a = crate::util::clean(a, 300);
        crate::moderation::precheck(&st, &a).await?;
        sqlx::query("UPDATE conversations SET about = $2 WHERE id = $1").bind(id).bind(a).execute(&st.pool).await?;
    }
    if let Some(mid) = b["avatarMediaId"].as_i64() {
        let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM media WHERE id = $1 AND owner = $2 AND status = 'ready' AND kind = 'photo')").bind(mid).bind(me.id()).fetch_one(&st.pool).await?;
        if !ok {
            return Err(invalid("avatarMediaId", "Upload a photo first."));
        }
        sqlx::query("UPDATE conversations SET avatar_media = $2 WHERE id = $1").bind(id).bind(mid).execute(&st.pool).await?;
        crate::media::mark_attached(&st, &[mid]).await?;
    }
    if let Some(locked) = b["locked"].as_bool() {
        sqlx::query("UPDATE conversations SET status = $2 WHERE id = $1").bind(id).bind(if locked { "locked" } else { "active" }).execute(&st.pool).await?;
    }
    let out = conv_out(&st, &me, id).await?;
    st.hub.send(Target::Conv(id), json!({"type": "conversation", "conversation": out.0["conversation"]}));
    Ok(out)
}

pub async fn members(State(st): State<AppState>, me: Me, Path(id): Path<i64>, Query(page): Query<super::Page>) -> R {
    chat::require(&st, id, me.id(), &["active", "request"]).await?;
    let limit = page.limit(50, 200);
    let rows = sqlx::query("SELECT user_id, role, state, joined_at FROM conv_members WHERE conversation_id = $1 AND state = 'active' AND user_id > $2 ORDER BY user_id LIMIT $3")
        .bind(id)
        .bind(page.cursor.as_deref().and_then(|c| c.parse::<i64>().ok()).unwrap_or(0))
        .bind(limit + 1)
        .fetch_all(&st.pool)
        .await?;
    let ids: Vec<i64> = rows.iter().map(|r| r.get("user_id")).collect();
    let cards = profiles::cards(&st, &ids).await?;
    let online = st.hub.online_of(&ids);
    let items: Vec<Value> = rows.iter().filter_map(|r| {
        let u: i64 = r.get("user_id");
        Some(json!({"user": cards.get(&u)?, "role": r.get::<String, _>("role"), "online": online.contains(&u), "id": u}))
    }).collect();
    Ok(Json(super::paged(items, limit, |v| v["id"].as_i64().map(|i| i.to_string()))))
}

pub async fn add_members(State(st): State<AppState>, me: Me, Path(id): Path<i64>, Body(b): Body) -> R {
    let (_c, m) = chat::require(&st, id, me.id(), &["active"]).await?;
    if !matches!(m.get::<String, _>("role").as_str(), "owner" | "admin") {
        return Err(ApiError::Forbidden("Only group admins can add members.".into()));
    }
    let handles: Vec<String> = b["members"].as_array().map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
    let added = chat::add_members(&st, &me, id, &handles).await?;
    Ok(Json(json!({"added": added})))
}

pub async fn remove_member(State(st): State<AppState>, me: Me, Path((id, user)): Path<(i64, i64)>) -> R {
    let (_c, m) = chat::require(&st, id, me.id(), &["active"]).await?;
    if user != me.id() && !matches!(m.get::<String, _>("role").as_str(), "owner" | "admin") {
        return Err(ApiError::Forbidden("Only admins can remove members.".into()));
    }
    let target = chat::member(&st, id, user).await?.ok_or(ApiError::NotFound)?;
    if target.get::<String, _>("role") == "owner" && user != me.id() {
        return Err(ApiError::Forbidden("The owner can't be removed.".into()));
    }
    chat::remove_member(&st, id, user, if user == me.id() { "left" } else { "removed" }).await?;
    Ok(Json(json!({"status": "ok"})))
}

pub async fn make_admin(State(st): State<AppState>, me: Me, Path((id, user)): Path<(i64, i64)>) -> R {
    let (_c, m) = chat::require(&st, id, me.id(), &["active"]).await?;
    if m.get::<String, _>("role") != "owner" {
        return Err(ApiError::Forbidden("Only the owner can make admins.".into()));
    }
    let n = sqlx::query("UPDATE conv_members SET role = 'admin' WHERE conversation_id = $1 AND user_id = $2 AND state = 'active' AND role = 'member'").bind(id).bind(user).execute(&st.pool).await?.rows_affected();
    if n == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(Json(json!({"status": "ok"})))
}

/// `POST /v1/circle/chat/conversations/{id}/{action}`: accept | decline | join | leave | mute | unmute | read | typing.
pub async fn action(State(st): State<AppState>, me: Me, Path((id, action)): Path<(i64, String)>, Body(b): Body) -> R {
    match action.as_str() {
        "accept" => {
            let n = sqlx::query("UPDATE conv_members SET state = 'active' WHERE conversation_id = $1 AND user_id = $2 AND state = 'request'").bind(id).bind(me.id()).execute(&st.pool).await?.rows_affected();
            if n == 0 {
                return Err(ApiError::NotFound);
            }
            chat::recount(&st, id).await?;
            st.hub.send(Target::Conv(id), json!({"type": "request.accepted", "conversationId": id, "userId": me.id()}));
            return conv_out(&st, &me, id).await;
        }
        "decline" => {
            let n = sqlx::query("UPDATE conv_members SET state = 'declined', unread = 0 WHERE conversation_id = $1 AND user_id = $2 AND state = 'request'").bind(id).bind(me.id()).execute(&st.pool).await?.rows_affected();
            if n == 0 {
                return Err(ApiError::NotFound);
            }
            st.hub.send(Target::User(me.id()), json!({"type": "conversation.left", "conversationId": id}));
            if b["block"].as_bool() == Some(true) {
                let other: Option<i64> = sqlx::query_scalar("SELECT user_id FROM conv_members WHERE conversation_id = $1 AND user_id <> $2 LIMIT 1").bind(id).bind(me.id()).fetch_optional(&st.pool).await?;
                if let Some(o) = other {
                    sqlx::query("INSERT INTO blocks (blocker, blocked) VALUES ($1,$2) ON CONFLICT DO NOTHING").bind(me.id()).bind(o).execute(&st.pool).await?;
                }
            }
        }
        "join" => {
            chat::join_room(&st, &me, id).await?;
            return conv_out(&st, &me, id).await;
        }
        "leave" => {
            let (c, m) = chat::require(&st, id, me.id(), &["active", "request"]).await?;
            if c.get::<String, _>("kind") == "dm" {
                return Err(invalid("id", "Delete or mute a direct conversation instead."));
            }
            if m.get::<String, _>("role") == "owner" && c.get::<String, _>("kind") == "group" {
                // hand the group to the longest-standing admin / member
                let next: Option<i64> = sqlx::query_scalar("SELECT user_id FROM conv_members WHERE conversation_id = $1 AND user_id <> $2 AND state = 'active' ORDER BY (role = 'admin') DESC, joined_at LIMIT 1").bind(id).bind(me.id()).fetch_optional(&st.pool).await?;
                if let Some(n) = next {
                    sqlx::query("UPDATE conv_members SET role = 'owner' WHERE conversation_id = $1 AND user_id = $2").bind(id).bind(n).execute(&st.pool).await?;
                    sqlx::query("UPDATE conversations SET owner = $2 WHERE id = $1").bind(id).bind(n).execute(&st.pool).await?;
                }
            }
            chat::remove_member(&st, id, me.id(), "left").await?;
        }
        "mute" | "unmute" => {
            chat::require(&st, id, me.id(), &["active", "request"]).await?;
            sqlx::query("UPDATE conv_members SET muted = $3 WHERE conversation_id = $1 AND user_id = $2").bind(id).bind(me.id()).bind(action == "mute").execute(&st.pool).await?;
        }
        "read" => chat::read(&st, &me, id, b["messageId"].as_i64()).await?,
        "typing" => chat::typing(&st, &me, id).await?,
        _ => return Err(ApiError::NotFound),
    }
    Ok(Json(json!({"status": "ok"})))
}

#[derive(Deserialize)]
pub struct HistQ {
    before: Option<i64>,
    after: Option<i64>,
    limit: Option<i64>,
}

pub async fn messages(State(st): State<AppState>, me: Me, Path(id): Path<i64>, Query(q): Query<HistQ>) -> R {
    let c = chat::conv(&st, id).await?;
    // a public room can be read before joining (latest messages)
    if c.get::<String, _>("kind") == "room" && chat::member(&st, id, me.id()).await?.is_none_or(|m| m.get::<String, _>("state") != "active") {
        let rows = sqlx::query("SELECT * FROM messages WHERE conversation_id = $1 AND status = 'sent' AND id < $2 ORDER BY id DESC LIMIT $3").bind(id).bind(q.before.unwrap_or(i64::MAX)).bind(crate::util::clamp_limit(q.limit, 30, 100)).fetch_all(&st.pool).await?;
        return Ok(Json(json!({"items": chat::messages_json(&st, me.id(), &rows).await?, "preview": true})));
    }
    Ok(Json(chat::history(&st, &me, id, q.before, q.after, crate::util::clamp_limit(q.limit, 30, 100)).await?))
}

pub async fn send(State(st): State<AppState>, me: Me, Path(id): Path<i64>, Body(b): Body) -> R {
    Ok(Json(json!({"message": chat::send(&st, &me, id, &b).await?})))
}

pub async fn edit_message(State(st): State<AppState>, me: Me, Path(id): Path<i64>, Body(b): Body) -> R {
    Ok(Json(json!({"message": chat::edit_message(&st, &me, id, b["body"].as_str().unwrap_or("")).await?})))
}

pub async fn delete_message(State(st): State<AppState>, me: Me, Path(id): Path<i64>) -> R {
    chat::delete_message(&st, &me, id).await?;
    Ok(Json(json!({"status": "deleted"})))
}
