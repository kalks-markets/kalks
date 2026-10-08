//! Me, profiles and the social graph: follow (requests for private profiles), bell, block, mute, restrict,
//! close friends, hidden words, notification preferences, devices (push) and the activity list.

use super::{Body, Me, Page, paged};
use crate::error::{ApiError, ApiResult, conflict, denied, invalid};
use crate::profiles::{self, Profile};
use crate::state::AppState;
use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;

type R = ApiResult<Json<Value>>;

pub async fn me(State(st): State<AppState>, me: Me) -> R {
    profiles::refresh_badges_later(&st, &me.p);
    let mut v = profiles::profile_json(&st, &me, &me.p).await?;
    let follow_requests: i64 = sqlx::query_scalar("SELECT count(*) FROM follows WHERE followee = $1 AND status = 'requested'").bind(me.id()).fetch_one(&st.pool).await?;
    let chat_unread: i64 = sqlx::query_scalar("SELECT COALESCE(sum(m.unread),0)::bigint FROM conv_members m JOIN conversations c ON c.id = m.conversation_id WHERE m.user_id = $1 AND m.state = 'active' AND NOT m.muted AND c.kind IN ('dm','group')").bind(me.id()).fetch_one(&st.pool).await?;
    let chat_requests: i64 = sqlx::query_scalar("SELECT count(*) FROM conv_members m JOIN conversations c ON c.id = m.conversation_id WHERE m.user_id = $1 AND m.state = 'request' AND c.last_message_id IS NOT NULL").bind(me.id()).fetch_one(&st.pool).await?;
    let s = profiles::settings(&st).await;
    v["counters"] = json!({"activityUnread": crate::notify::unread(&st, me.id()).await?, "followRequests": follow_requests, "chatUnread": chat_unread, "chatRequests": chat_requests});
    v["banned"] = if me.p.is_banned() { json!({"until": me.p.banned_until, "reason": me.p.ban_reason}) } else { Value::Null };
    v["limits"] = json!({
        "postChars": crate::posts::MAX_BODY, "commentChars": crate::posts::MAX_COMMENT, "photosPerPost": 10, "videoSecs": st.cfg.max_video_secs,
        "topicVideoSecs": st.cfg.max_topic_video_secs, "voiceSecs": st.cfg.max_voice_secs, "photoMb": st.cfg.max_photo_bytes / 1048576,
        "videoMb": st.cfg.max_video_bytes / 1048576, "fileMb": st.cfg.max_file_bytes / 1048576, "chunkBytes": st.cfg.chunk_bytes, "groupMembers": s.max_group_members,
        "fileTypes": crate::media::FILE_EXTS,
    });
    v["features"] = json!({"ai": st.ai(), "push": crate::push::configured(&st.cfg), "video": crate::video::available(&st.cfg).await, "storage": st.storage.kind()});
    v["riskLine"] = json!(crate::text::RISK_LINE);
    Ok(Json(v))
}

pub async fn update_me(State(st): State<AppState>, me: Me, Body(b): Body) -> R {
    let id = me.id();
    if let Some(h) = b["handle"].as_str() {
        let h = crate::text::handle(h).map_err(|m| invalid("handle", m))?;
        if h != me.p.handle {
            let r = sqlx::query("UPDATE profiles SET handle = $2, updated_at = now() WHERE user_id = $1").bind(id).bind(&h).execute(&st.pool).await;
            match r {
                Err(sqlx::Error::Database(e)) if e.constraint() == Some("profiles_handle") => return Err(conflict("handle_taken", "This handle is taken.")),
                Err(e) => return Err(e.into()),
                Ok(_) => {}
            }
        }
    }
    if let Some(n) = b["displayName"].as_str() {
        let n = crate::util::clean(n, 41);
        if n.is_empty() || n.chars().count() > 40 {
            return Err(invalid("displayName", "Display names are 1 to 40 characters."));
        }
        crate::moderation::precheck(&st, &n).await?;
        sqlx::query("UPDATE profiles SET display_name = $2 WHERE user_id = $1").bind(id).bind(&n).execute(&st.pool).await?;
    }
    if let Some(bio) = b["bio"].as_str() {
        let bio = crate::util::clean(bio, 161);
        if bio.chars().count() > 160 {
            return Err(invalid("bio", "Bios are up to 160 characters."));
        }
        crate::moderation::precheck(&st, &bio).await?;
        if st.ai() && !bio.is_empty() {
            if let Ok(v) = crate::moderation::classify_text(&st, "profile bio", &bio).await
                && v.decision == crate::moderation::Decision::Block
            {
                return Err(ApiError::Rejected { code: "content_blocked", message: crate::moderation::main_reason(&v) });
            }
        }
        sqlx::query("UPDATE profiles SET bio = $2 WHERE user_id = $1").bind(id).bind(&bio).execute(&st.pool).await?;
    }
    for (field, col, purpose) in [("avatarMediaId", "avatar", "avatar"), ("coverMediaId", "cover", "cover")] {
        match &b[field] {
            Value::Null if b.get(field).is_some() => {
                sqlx::query(sqlx::AssertSqlSafe(format!("UPDATE profiles SET {col}_media = NULL, {col} = NULL WHERE user_id = $1"))).bind(id).execute(&st.pool).await?;
            }
            v if v.as_i64().is_some() => {
                let mid = v.as_i64().unwrap_or(0);
                let r = sqlx::query("SELECT status, purpose, variants FROM media WHERE id = $1 AND owner = $2").bind(mid).bind(id).fetch_optional(&st.pool).await?.ok_or_else(|| invalid(field, "Upload the photo first."))?;
                if r.get::<String, _>("purpose") != purpose {
                    return Err(invalid(field, "This photo was uploaded for something else."));
                }
                match r.get::<String, _>("status").as_str() {
                    "ready" => {}
                    "processing" | "uploading" => return Err(conflict("media_processing", "The photo is still being processed. Try again in a moment.")),
                    "review" => return Err(conflict("media_review", "The photo is waiting for a moderator.")),
                    _ => return Err(ApiError::Rejected { code: "media_rejected", message: "This photo can't be used.".into() }),
                }
                let variants: sqlx::types::Json<Value> = r.get("variants");
                sqlx::query(sqlx::AssertSqlSafe(format!("UPDATE profiles SET {col}_media = $2, {col} = $3 WHERE user_id = $1"))).bind(id).bind(mid).bind(variants).execute(&st.pool).await?;
                crate::media::mark_attached(&st, &[mid]).await?;
            }
            _ => {}
        }
    }
    if let Some(p) = b["private"].as_bool() {
        sqlx::query("UPDATE profiles SET private = $2 WHERE user_id = $1").bind(id).bind(p).execute(&st.pool).await?;
        if !p {
            // going public accepts every pending request
            let accepted: Vec<i64> = sqlx::query_scalar("UPDATE follows SET status = 'active' WHERE followee = $1 AND status = 'requested' RETURNING follower").bind(id).fetch_all(&st.pool).await?;
            for f in &accepted {
                sqlx::query("UPDATE profiles SET following_count = following_count + 1 WHERE user_id = $1").bind(f).execute(&st.pool).await?;
            }
            sqlx::query("UPDATE profiles SET followers_count = followers_count + $2 WHERE user_id = $1").bind(id).bind(accepted.len() as i64).execute(&st.pool).await?;
        }
    }
    if let Some(l) = b["lang"].as_str() {
        let l = crate::text::valid_lang(l).ok_or_else(|| invalid("lang", "Unknown language."))?;
        sqlx::query("UPDATE profiles SET lang = $2 WHERE user_id = $1").bind(id).bind(l).execute(&st.pool).await?;
    }
    if let Some(s) = b["showStats"].as_bool() {
        sqlx::query("UPDATE profiles SET show_stats = $2 WHERE user_id = $1").bind(id).bind(s).execute(&st.pool).await?;
        if s && me.p.stats_at.is_none_or(|t| Utc::now() - t > chrono::Duration::hours(1)) {
            let st2 = st.clone();
            tokio::spawn(async move {
                let _ = crate::stats::refresh(&st2, id).await;
            });
        }
    }
    if let Some(s) = b["showIbLink"].as_bool() {
        sqlx::query("UPDATE profiles SET show_ib_link = $2 WHERE user_id = $1").bind(id).bind(s).execute(&st.pool).await?;
    }
    if let Some(c) = b["commentsDefault"].as_str() {
        if !["everyone", "followers", "off"].contains(&c) {
            return Err(invalid("commentsDefault", "everyone, followers or off."));
        }
        sqlx::query("UPDATE profiles SET comments_default = $2 WHERE user_id = $1").bind(id).bind(c).execute(&st.pool).await?;
    }
    if let Some(d) = b["dmPolicy"].as_str() {
        if !["everyone", "following", "none"].contains(&d) {
            return Err(invalid("dmPolicy", "everyone, following or none."));
        }
        sqlx::query("UPDATE profiles SET dm_policy = $2 WHERE user_id = $1").bind(id).bind(d).execute(&st.pool).await?;
    }
    if b["onboarded"].as_bool() == Some(true) {
        sqlx::query("UPDATE profiles SET onboarded = true WHERE user_id = $1").bind(id).execute(&st.pool).await?;
    }
    if let Some(p) = b["pinnedPostId"].as_i64() {
        let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM posts WHERE id = $1 AND author = $2 AND status = 'published')").bind(p).bind(id).fetch_one(&st.pool).await?;
        if !ok {
            return Err(invalid("pinnedPostId", "Pin one of your published posts."));
        }
        sqlx::query("UPDATE profiles SET pinned_post = $2 WHERE user_id = $1").bind(id).bind(p).execute(&st.pool).await?;
    }
    sqlx::query("UPDATE profiles SET updated_at = now() WHERE user_id = $1").bind(id).execute(&st.pool).await?;
    let p = profiles::by_id(&st, id).await?.ok_or(ApiError::NotFound)?;
    let me2 = Me { client: me.client.clone(), p };
    Ok(Json(json!({"profile": profiles::profile_json(&st, &me2, &me2.p).await?})))
}

#[derive(Deserialize)]
pub struct HandleQ {
    handle: Option<String>,
}

pub async fn check_handle(State(st): State<AppState>, me: Me, Query(q): Query<HandleQ>) -> R {
    let raw = q.handle.unwrap_or_default();
    match crate::text::handle(&raw) {
        Err(m) => Ok(Json(json!({"handle": raw, "valid": false, "available": false, "message": m}))),
        Ok(h) => {
            let taken: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM profiles WHERE handle = $1 AND user_id <> $2)").bind(&h).bind(me.id()).fetch_one(&st.pool).await?;
            Ok(Json(json!({"handle": h, "valid": true, "available": !taken, "message": if taken { "This handle is taken." } else { "" }})))
        }
    }
}

pub async fn profile(State(st): State<AppState>, me: Me, Path(handle): Path<String>) -> R {
    let p = profiles::by_handle(&st, &handle).await?;
    Ok(Json(json!({"profile": profiles::profile_json(&st, &me, &p).await?})))
}

async fn list_people(st: &AppState, me: &Me, p: &Profile, followers: bool, page: &Page) -> R {
    if !profiles::can_view(st, me.id(), p).await? {
        return Err(denied("private_profile", "This profile is private."));
    }
    let limit = page.limit(30, 100);
    let before = page.cursor.as_deref().and_then(|c| c.parse::<i64>().ok()).unwrap_or(i64::MAX);
    let sql = if followers {
        "SELECT f.follower AS uid, (extract(epoch FROM f.created_at) * 1000000)::bigint AS k FROM follows f JOIN profiles a ON a.user_id = f.follower
         WHERE f.followee = $1 AND f.status = 'active' AND a.status = 'active' AND NOT a.shadow_hidden AND (extract(epoch FROM f.created_at) * 1000000)::bigint < $2
           AND NOT EXISTS (SELECT 1 FROM blocks b WHERE (b.blocker = $3 AND b.blocked = a.user_id) OR (b.blocker = a.user_id AND b.blocked = $3))
         ORDER BY k DESC LIMIT $4"
    } else {
        "SELECT f.followee AS uid, (extract(epoch FROM f.created_at) * 1000000)::bigint AS k FROM follows f JOIN profiles a ON a.user_id = f.followee
         WHERE f.follower = $1 AND f.status = 'active' AND a.status = 'active' AND NOT a.shadow_hidden AND (extract(epoch FROM f.created_at) * 1000000)::bigint < $2
           AND NOT EXISTS (SELECT 1 FROM blocks b WHERE (b.blocker = $3 AND b.blocked = a.user_id) OR (b.blocker = a.user_id AND b.blocked = $3))
         ORDER BY k DESC LIMIT $4"
    };
    let rows = sqlx::query(sql).bind(p.user_id).bind(before).bind(me.id()).bind(limit + 1).fetch_all(&st.pool).await?;
    let ids: Vec<i64> = rows.iter().map(|r| r.get("uid")).collect();
    let cards = profiles::cards(st, &ids).await?;
    let following: std::collections::HashSet<i64> = sqlx::query_scalar("SELECT followee FROM follows WHERE follower = $1 AND followee = ANY($2) AND status = 'active'").bind(me.id()).bind(&ids).fetch_all(&st.pool).await?.into_iter().collect();
    let items: Vec<Value> = rows
        .iter()
        .filter_map(|r| {
            let uid: i64 = r.get("uid");
            let mut c = cards.get(&uid)?.clone();
            c["following"] = json!(following.contains(&uid));
            c["cursor"] = json!(r.get::<i64, _>("k").to_string());
            Some(c)
        })
        .collect();
    Ok(Json(paged(items, limit, |v| v["cursor"].as_str().map(str::to_string))))
}

pub async fn followers(State(st): State<AppState>, me: Me, Path(handle): Path<String>, Query(page): Query<Page>) -> R {
    let p = profiles::by_handle(&st, &handle).await?;
    list_people(&st, &me, &p, true, &page).await
}

pub async fn following(State(st): State<AppState>, me: Me, Path(handle): Path<String>, Query(page): Query<Page>) -> R {
    let p = profiles::by_handle(&st, &handle).await?;
    list_people(&st, &me, &p, false, &page).await
}

async fn counts_after_follow(st: &AppState, follower: i64, followee: i64, delta: i64) -> anyhow::Result<()> {
    sqlx::query("UPDATE profiles SET following_count = GREATEST(following_count + $2, 0) WHERE user_id = $1").bind(follower).bind(delta).execute(&st.pool).await?;
    let n: i64 = sqlx::query_scalar("UPDATE profiles SET followers_count = GREATEST(followers_count + $2, 0) WHERE user_id = $1 RETURNING followers_count").bind(followee).bind(delta).fetch_one(&st.pool).await?;
    if delta > 0 {
        crate::gamify::xp(st, followee, crate::gamify::XP_FOLLOWER).await?;
        crate::gamify::followers_milestones(st, followee, n).await?;
    }
    Ok(())
}

/// Follows `p` (a request when the profile is private). Returns `active` or `requested`.
pub async fn follow(st: &AppState, me: &Me, p: &Profile) -> ApiResult<&'static str> {
    if p.user_id == me.id() {
        return Err(invalid("handle", "You can't follow yourself."));
    }
    if p.is_banned() || profiles::blocked_between(st, me.id(), p.user_id).await? {
        return Err(ApiError::NotFound);
    }
    if !st.limiter.hit(&format!("follow:{}", me.id()), 200, std::time::Duration::from_secs(3600)) {
        return Err(ApiError::RateLimited("You're following very fast. Please wait a little.".into()));
    }
    let status = if p.private { "requested" } else { "active" };
    let inserted = sqlx::query("INSERT INTO follows (follower, followee, status) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING").bind(me.id()).bind(p.user_id).bind(status).execute(&st.pool).await?.rows_affected() > 0;
    if !inserted {
        let cur: String = sqlx::query_scalar("SELECT status FROM follows WHERE follower = $1 AND followee = $2").bind(me.id()).bind(p.user_id).fetch_one(&st.pool).await?;
        return Ok(if cur == "active" { "active" } else { "requested" });
    }
    if status == "active" {
        counts_after_follow(st, me.id(), p.user_id, 1).await?;
        crate::notify::event(st, crate::notify::Ev::new(p.user_id, "follow", me.id())).await?;
    } else {
        crate::notify::event(st, crate::notify::Ev::new(p.user_id, "follow_request", me.id())).await?;
    }
    Ok(status)
}

pub async fn unfollow(st: &AppState, follower: i64, followee: i64) -> anyhow::Result<()> {
    if let Some(s) = sqlx::query_scalar::<_, String>("DELETE FROM follows WHERE follower = $1 AND followee = $2 RETURNING status").bind(follower).bind(followee).fetch_optional(&st.pool).await?
        && s == "active"
    {
        counts_after_follow(st, follower, followee, -1).await?;
    }
    Ok(())
}

/// `POST /v1/circle/profiles/{handle}/{action}`: follow | bell | block | mute | restrict | close-friend.
pub async fn relate(State(st): State<AppState>, me: Me, Path((handle, action)): Path<(String, String)>, Body(b): Body) -> R {
    let p = profiles::by_handle(&st, &handle).await?;
    if p.user_id == me.id() {
        return Err(invalid("handle", "That's you."));
    }
    match action.as_str() {
        "follow" => {
            let status = follow(&st, &me, &p).await?;
            return Ok(Json(json!({"status": status, "relationship": profiles::rel(&st, me.id(), p.user_id).await?.json()})));
        }
        "bell" => {
            let on = b["on"].as_bool().unwrap_or(true);
            let n = sqlx::query("UPDATE follows SET bell = $3 WHERE follower = $1 AND followee = $2 AND status = 'active'").bind(me.id()).bind(p.user_id).bind(on).execute(&st.pool).await?.rows_affected();
            if n == 0 {
                return Err(conflict("not_following", "Follow first to turn on notifications."));
            }
        }
        "block" => {
            sqlx::query("INSERT INTO blocks (blocker, blocked) VALUES ($1,$2) ON CONFLICT DO NOTHING").bind(me.id()).bind(p.user_id).execute(&st.pool).await?;
            unfollow(&st, me.id(), p.user_id).await?;
            unfollow(&st, p.user_id, me.id()).await?;
            sqlx::query("DELETE FROM close_friends WHERE (owner = $1 AND friend = $2) OR (owner = $2 AND friend = $1)").bind(me.id()).bind(p.user_id).execute(&st.pool).await?;
            crate::audit::record(&st.pool, &crate::audit::Actor::user(me.id()), "user.block", Some(format!("user:{}", p.user_id)), None, None, None).await?;
        }
        "mute" => {
            let posts = b["posts"].as_bool().unwrap_or(true);
            let stories = b["stories"].as_bool().unwrap_or(true);
            sqlx::query("INSERT INTO mutes (muter, muted, posts, stories) VALUES ($1,$2,$3,$4) ON CONFLICT (muter, muted) DO UPDATE SET posts = EXCLUDED.posts, stories = EXCLUDED.stories")
                .bind(me.id())
                .bind(p.user_id)
                .bind(posts)
                .bind(stories)
                .execute(&st.pool)
                .await?;
        }
        "restrict" => {
            sqlx::query("INSERT INTO restricts (restrictor, restricted) VALUES ($1,$2) ON CONFLICT DO NOTHING").bind(me.id()).bind(p.user_id).execute(&st.pool).await?;
        }
        "close-friend" => {
            if !profiles::is_follower(&st, p.user_id, me.id()).await? {
                return Err(conflict("not_follower", "Close friends are chosen among your followers."));
            }
            sqlx::query("INSERT INTO close_friends (owner, friend) VALUES ($1,$2) ON CONFLICT DO NOTHING").bind(me.id()).bind(p.user_id).execute(&st.pool).await?;
        }
        _ => return Err(ApiError::NotFound),
    }
    Ok(Json(json!({"relationship": profiles::rel(&st, me.id(), p.user_id).await?.json()})))
}

/// `DELETE /v1/circle/profiles/{handle}/{action}`: unfollow (or cancel the request), bell off, unblock, unmute, …
pub async fn unrelate(State(st): State<AppState>, me: Me, Path((handle, action)): Path<(String, String)>) -> R {
    let p = profiles::by_handle(&st, &handle).await?;
    match action.as_str() {
        "follow" => unfollow(&st, me.id(), p.user_id).await?,
        "bell" => {
            sqlx::query("UPDATE follows SET bell = false WHERE follower = $1 AND followee = $2").bind(me.id()).bind(p.user_id).execute(&st.pool).await?;
        }
        "block" => {
            sqlx::query("DELETE FROM blocks WHERE blocker = $1 AND blocked = $2").bind(me.id()).bind(p.user_id).execute(&st.pool).await?;
        }
        "mute" => {
            sqlx::query("DELETE FROM mutes WHERE muter = $1 AND muted = $2").bind(me.id()).bind(p.user_id).execute(&st.pool).await?;
        }
        "restrict" => {
            sqlx::query("DELETE FROM restricts WHERE restrictor = $1 AND restricted = $2").bind(me.id()).bind(p.user_id).execute(&st.pool).await?;
        }
        "close-friend" => {
            sqlx::query("DELETE FROM close_friends WHERE owner = $1 AND friend = $2").bind(me.id()).bind(p.user_id).execute(&st.pool).await?;
        }
        _ => return Err(ApiError::NotFound),
    }
    Ok(Json(json!({"relationship": profiles::rel(&st, me.id(), p.user_id).await?.json()})))
}

pub async fn requests(State(st): State<AppState>, me: Me) -> R {
    let rows = sqlx::query("SELECT follower, created_at FROM follows WHERE followee = $1 AND status = 'requested' ORDER BY created_at DESC LIMIT 200").bind(me.id()).fetch_all(&st.pool).await?;
    let ids: Vec<i64> = rows.iter().map(|r| r.get("follower")).collect();
    let cards = profiles::cards(&st, &ids).await?;
    Ok(Json(json!({"items": rows.iter().filter_map(|r| Some(json!({"user": cards.get(&r.get::<i64, _>("follower"))?, "requestedAt": r.get::<DateTime<Utc>, _>("created_at")}))).collect::<Vec<_>>()})))
}

pub async fn answer_request(State(st): State<AppState>, me: Me, Path((user, action)): Path<(i64, String)>) -> R {
    match action.as_str() {
        "accept" => {
            let n = sqlx::query("UPDATE follows SET status = 'active' WHERE follower = $1 AND followee = $2 AND status = 'requested'").bind(user).bind(me.id()).execute(&st.pool).await?.rows_affected();
            if n == 0 {
                return Err(ApiError::NotFound);
            }
            counts_after_follow(&st, user, me.id(), 1).await?;
            crate::notify::event(&st, crate::notify::Ev::new(user, "follow_accept", me.id())).await?;
        }
        "decline" => {
            sqlx::query("DELETE FROM follows WHERE follower = $1 AND followee = $2 AND status = 'requested'").bind(user).bind(me.id()).execute(&st.pool).await?;
        }
        _ => return Err(ApiError::NotFound),
    }
    Ok(Json(json!({"status": "ok"})))
}

pub async fn remove_follower(State(st): State<AppState>, me: Me, Path(user): Path<i64>) -> R {
    unfollow(&st, user, me.id()).await?;
    sqlx::query("DELETE FROM close_friends WHERE owner = $1 AND friend = $2").bind(me.id()).bind(user).execute(&st.pool).await?;
    Ok(Json(json!({"status": "ok"})))
}

/// `GET /v1/circle/me/lists/{list}`: blocked | muted | restricted | close-friends.
pub async fn my_list(State(st): State<AppState>, me: Me, Path(list): Path<String>) -> R {
    let sql = match list.as_str() {
        "blocked" => "SELECT blocked AS uid, created_at FROM blocks WHERE blocker = $1 ORDER BY created_at DESC LIMIT 1000",
        "muted" => "SELECT muted AS uid, created_at FROM mutes WHERE muter = $1 ORDER BY created_at DESC LIMIT 1000",
        "restricted" => "SELECT restricted AS uid, created_at FROM restricts WHERE restrictor = $1 ORDER BY created_at DESC LIMIT 1000",
        "close-friends" => "SELECT friend AS uid, created_at FROM close_friends WHERE owner = $1 ORDER BY created_at DESC LIMIT 1000",
        _ => return Err(ApiError::NotFound),
    };
    let rows = sqlx::query(sql).bind(me.id()).fetch_all(&st.pool).await?;
    let ids: Vec<i64> = rows.iter().map(|r| r.get("uid")).collect();
    let cards = profiles::cards(&st, &ids).await?;
    Ok(Json(json!({"items": ids.iter().filter_map(|i| cards.get(i).cloned()).collect::<Vec<_>>()})))
}

pub async fn hidden_words(State(st): State<AppState>, me: Me) -> R {
    let words: Vec<String> = sqlx::query_scalar("SELECT word FROM hidden_words WHERE user_id = $1 ORDER BY word").bind(me.id()).fetch_all(&st.pool).await?;
    Ok(Json(json!({"words": words})))
}

pub async fn put_hidden_words(State(st): State<AppState>, me: Me, Body(b): Body) -> R {
    let mut words: Vec<String> = b["words"].as_array().ok_or_else(|| invalid("words", "words is a list."))?.iter().filter_map(Value::as_str).map(|w| crate::util::clean(w, 60).to_lowercase()).filter(|w| !w.is_empty()).collect();
    words.sort();
    words.dedup();
    if words.len() > 200 {
        return Err(invalid("words", "Up to 200 hidden words."));
    }
    let mut tx = st.pool.begin().await?;
    sqlx::query("DELETE FROM hidden_words WHERE user_id = $1").bind(me.id()).execute(&mut *tx).await?;
    for w in &words {
        sqlx::query("INSERT INTO hidden_words (user_id, word) VALUES ($1,$2)").bind(me.id()).bind(w).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Json(json!({"words": words})))
}

pub async fn prefs(State(st): State<AppState>, me: Me) -> R {
    Ok(Json(json!({"prefs": crate::notify::prefs(&st, me.id()).await?})))
}

pub async fn put_prefs(State(st): State<AppState>, me: Me, Body(b): Body) -> R {
    Ok(Json(json!({"prefs": crate::notify::save_prefs(&st, me.id(), &b["prefs"]).await?})))
}

pub async fn register_device(State(st): State<AppState>, me: Me, Body(b): Body) -> R {
    let token = b["token"].as_str().map(str::trim).filter(|t| (20..=4096).contains(&t.len()) && t.chars().all(|c| c.is_ascii_graphic())).ok_or_else(|| invalid("token", "A push token is required."))?;
    let platform = match b["platform"].as_str().unwrap_or("android") {
        "ios" => "ios",
        "web" => "web",
        _ => "android",
    };
    sqlx::query(
        "INSERT INTO devices (token, user_id, platform, locale, app_version) VALUES ($1,$2,$3,$4,$5)
         ON CONFLICT (token) DO UPDATE SET user_id = EXCLUDED.user_id, platform = EXCLUDED.platform, locale = EXCLUDED.locale, app_version = EXCLUDED.app_version, disabled = false, last_seen_at = now()",
    )
    .bind(token)
    .bind(me.id())
    .bind(platform)
    .bind(b["locale"].as_str().and_then(crate::text::valid_lang))
    .bind(b["appVersion"].as_str().map(|v| crate::util::clean(v, 32)))
    .execute(&st.pool)
    .await?;
    Ok(Json(json!({"status": "ok", "push": crate::push::configured(&st.cfg)})))
}

pub async fn unregister_device(State(st): State<AppState>, me: Me, Body(b): Body) -> R {
    sqlx::query("DELETE FROM devices WHERE token = $1 AND user_id = $2").bind(b["token"].as_str().unwrap_or("")).bind(me.id()).execute(&st.pool).await?;
    Ok(Json(json!({"status": "ok"})))
}

pub async fn activity(State(st): State<AppState>, me: Me, Query(page): Query<Page>) -> R {
    let limit = page.limit(30, 100);
    let rows = sqlx::query("SELECT * FROM activity WHERE user_id = $1 AND id < $2 ORDER BY id DESC LIMIT $3").bind(me.id()).bind(page.before_id()).bind(limit + 1).fetch_all(&st.pool).await?;
    let actors: Vec<i64> = rows.iter().filter_map(|r| r.get::<Option<i64>, _>("actor")).collect();
    let cards = profiles::cards(&st, &actors).await?;
    let items: Vec<Value> = rows.iter().map(|r| crate::notify::activity_json(r, r.get::<Option<i64>, _>("actor").and_then(|a| cards.get(&a).cloned()))).collect();
    let mut v = paged(items, limit, |x| x["id"].as_i64().map(|i| i.to_string()));
    v["unread"] = json!(crate::notify::unread(&st, me.id()).await?);
    Ok(Json(v))
}

pub async fn activity_read(State(st): State<AppState>, me: Me, Body(b): Body) -> R {
    match b["ids"].as_array() {
        Some(ids) => {
            let ids: Vec<i64> = ids.iter().filter_map(Value::as_i64).collect();
            sqlx::query("UPDATE activity SET read = true WHERE user_id = $1 AND id = ANY($2)").bind(me.id()).bind(&ids).execute(&st.pool).await?;
        }
        None => {
            sqlx::query("UPDATE activity SET read = true WHERE user_id = $1 AND NOT read").bind(me.id()).execute(&st.pool).await?;
        }
    }
    let unread = crate::notify::unread(&st, me.id()).await?;
    st.hub.send(crate::state::Target::User(me.id()), json!({"type": "activity.read", "unread": unread}));
    Ok(Json(json!({"unread": unread})))
}

pub async fn my_stats(me: Me) -> R {
    Ok(Json(json!({"stats": profiles::stats_public(&me.p.stats.0, me.p.stats_at), "showStats": me.p.show_stats, "computedAt": me.p.stats_at})))
}

pub async fn refresh_stats(State(st): State<AppState>, me: Me) -> R {
    if !st.limiter.hit(&format!("stats:{}", me.id()), 6, std::time::Duration::from_secs(3600)) {
        return Err(ApiError::RateLimited("Stats can be refreshed a few times per hour.".into()));
    }
    let stats = crate::stats::refresh(&st, me.id()).await.map_err(|e| ApiError::Unavailable(format!("The trading service is unavailable ({e}).")))?;
    Ok(Json(json!({"stats": profiles::stats_public(&stats, Some(Utc::now())), "showStats": me.p.show_stats})))
}
