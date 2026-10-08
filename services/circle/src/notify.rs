//! Notifications (Q41): every event lands in the member's Circle activity list (realtime over the stream) and,
//! per kind and preference, in the bell (support service `POST /v1/notify`, in-app only) and as a push (FCM).
//!
//! Delivery goes through `notify_outbox` (retried with back-off by the notify worker). Likes and votes are batched:
//! one notification per post per 10 minutes ("Ana and 12 others liked your post"); DMs are batched per
//! conversation for a few seconds and only pushed (the chat itself shows the unread count).

use crate::state::{AppState, Target};
use crate::upstream;
use serde_json::{Value, json};
use sqlx::Row;

/// Preference groups: (key, label, bell default, push default).
pub const PREFS: &[(&str, &str)] = &[
    ("follows", "New followers and follow requests"),
    ("likes", "Likes and votes on your posts"),
    ("comments", "Comments and replies"),
    ("mentions", "Mentions"),
    ("messages", "Messages and message requests"),
    ("posts", "Posts, stories and trades from traders you follow"),
    ("achievements", "Achievements and levels"),
    ("moderation", "Moderation decisions"),
];

fn pref_group(kind: &str) -> &'static str {
    match kind {
        "follow" | "follow_request" | "follow_accept" => "follows",
        "like" | "vote" | "comment_like" => "likes",
        "comment" | "reply" => "comments",
        "mention" => "mentions",
        "dm" | "dm_request" | "group_invite" => "messages",
        "post" | "story" | "trade" => "posts",
        "achievement" | "helpful" => "achievements",
        _ => "moderation",
    }
}

pub struct Ev {
    pub to: i64,
    pub kind: &'static str,
    pub actor: Option<i64>,
    pub post: Option<i64>,
    pub comment: Option<i64>,
    pub story: Option<i64>,
    pub conversation: Option<i64>,
    pub data: Value,
}

impl Ev {
    pub fn new(to: i64, kind: &'static str, actor: i64) -> Self {
        Ev { to, kind, actor: Some(actor), post: None, comment: None, story: None, conversation: None, data: json!({}) }
    }
    pub fn post(mut self, id: i64) -> Self {
        self.post = Some(id);
        self
    }
    pub fn comment(mut self, id: i64) -> Self {
        self.comment = Some(id);
        self
    }
    pub fn story(mut self, id: i64) -> Self {
        self.story = Some(id);
        self
    }
    pub fn conversation(mut self, id: i64) -> Self {
        self.conversation = Some(id);
        self
    }
    pub fn data(mut self, d: Value) -> Self {
        self.data = d;
        self
    }
}

pub async fn unread(st: &AppState, user: i64) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar("SELECT count(*) FROM activity WHERE user_id = $1 AND NOT read").bind(user).fetch_one(&st.pool).await?)
}

pub fn activity_json(r: &sqlx::postgres::PgRow, actor: Option<Value>) -> Value {
    json!({
        "id": r.get::<i64, _>("id"),
        "kind": r.get::<String, _>("kind"),
        "actor": actor,
        "postId": r.get::<Option<i64>, _>("post_id"),
        "commentId": r.get::<Option<i64>, _>("comment_id"),
        "storyId": r.get::<Option<i64>, _>("story_id"),
        "conversationId": r.get::<Option<i64>, _>("conversation_id"),
        "data": r.get::<sqlx::types::Json<Value>, _>("data").0,
        "read": r.get::<bool, _>("read"),
        "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
    })
}

/// Records an event for `ev.to`: activity item (+ realtime frame) and the bell / push delivery.
pub async fn event(st: &AppState, ev: Ev) -> anyhow::Result<()> {
    if ev.actor == Some(ev.to) {
        return Ok(());
    }
    if let Some(a) = ev.actor {
        let blocked: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM blocks WHERE (blocker = $1 AND blocked = $2) OR (blocker = $2 AND blocked = $1))").bind(ev.to).bind(a).fetch_one(&st.pool).await?;
        if blocked {
            return Ok(());
        }
    }
    let Some(recipient) = sqlx::query("SELECT tenant, status FROM profiles WHERE user_id = $1").bind(ev.to).fetch_optional(&st.pool).await? else { return Ok(()) };
    let tenant: String = recipient.get("tenant");
    let actor = match ev.actor {
        Some(a) => crate::profiles::by_id(st, a).await.ok().flatten(),
        None => None,
    };
    // shadow-hidden or suspended members reach nobody
    if actor.as_ref().is_some_and(|a| a.shadow_hidden || a.is_banned()) {
        return Ok(());
    }
    let name = actor.as_ref().map(|p| p.display_name.clone()).unwrap_or_else(|| "Kalks Circle".into());
    // DMs are not activity items (the chat list has its own unread counts)
    if !matches!(ev.kind, "dm") {
        let row = sqlx::query(
            "INSERT INTO activity (user_id, kind, actor, post_id, comment_id, story_id, conversation_id, data) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING *",
        )
        .bind(ev.to)
        .bind(ev.kind)
        .bind(ev.actor)
        .bind(ev.post)
        .bind(ev.comment)
        .bind(ev.story)
        .bind(ev.conversation)
        .bind(sqlx::types::Json(&ev.data))
        .fetch_one(&st.pool)
        .await?;
        let item = activity_json(&row, actor.as_ref().map(|p| crate::profiles::card(st, p)));
        st.hub.send(Target::User(ev.to), json!({"type": "activity", "item": item, "unread": unread(st, ev.to).await?}));
    }
    let preview = ev.data["preview"].as_str().map(|s| crate::util::preview(s, 120)).unwrap_or_default();
    let post_link = ev.post.map(|p| format!("/circle/post/{p}"));
    let (title, body, link, bell, push, batch, delay_secs): (String, String, Option<String>, bool, bool, Option<String>, i64) = match ev.kind {
        "follow" => (format!("{name} started following you"), String::new(), actor.as_ref().map(|a| format!("/circle/@{}", a.handle)), true, true, None, 0),
        "follow_request" => (format!("{name} wants to follow you"), "Review it in your follow requests.".into(), Some("/circle/requests".into()), true, true, None, 0),
        "follow_accept" => (format!("{name} accepted your follow request"), String::new(), actor.as_ref().map(|a| format!("/circle/@{}", a.handle)), true, true, None, 0),
        "like" | "vote" => (String::new(), String::new(), post_link.clone(), true, true, Some(format!("like:{}:{}", ev.to, ev.post.unwrap_or(0))), 600),
        "comment_like" => (String::new(), String::new(), post_link.clone(), true, false, Some(format!("clike:{}:{}", ev.to, ev.comment.unwrap_or(0))), 600),
        "comment" => (format!("{name} commented on your post"), preview.clone(), post_link.clone(), true, true, None, 0),
        "reply" => (format!("{name} replied to your comment"), preview.clone(), post_link.clone(), true, true, None, 0),
        "mention" => (format!("{name} mentioned you"), preview.clone(), post_link.clone().or_else(|| ev.story.map(|s| format!("/circle/story/{s}"))), true, true, None, 0),
        "dm" => (name.clone(), preview.clone(), ev.conversation.map(|c| format!("/circle/chat/{c}")), false, true, Some(format!("dm:{}:{}", ev.to, ev.conversation.unwrap_or(0))), 5),
        "dm_request" => (format!("{name} sent you a message request"), preview.clone(), Some("/circle/chat?box=requests".into()), true, true, None, 0),
        "group_invite" => (format!("{name} added you to a group"), ev.data["title"].as_str().unwrap_or("").to_string(), ev.conversation.map(|c| format!("/circle/chat/{c}")), true, true, None, 0),
        "post" => (format!("{name} posted"), preview.clone(), post_link.clone(), true, true, None, 0),
        "story" => (format!("{name} added a story"), String::new(), actor.as_ref().map(|a| format!("/circle/stories/@{}", a.handle)), false, true, None, 0),
        "trade" => (
            format!("{name} shared a trade"),
            format!("{} {}", ev.data["symbol"].as_str().unwrap_or(""), ev.data["side"].as_str().unwrap_or("")).trim().to_string(),
            post_link.clone(),
            true,
            true,
            None,
            0,
        ),
        "achievement" => (format!("New achievement: {}", ev.data["title"].as_str().unwrap_or("")), String::new(), Some("/circle/me".into()), true, true, None, 0),
        "helpful" => ("Your post was marked helpful".into(), ev.data["points"].as_i64().map(|p| format!("+{p} Rewards points")).unwrap_or_default(), post_link.clone(), true, true, None, 0),
        "moderation" => (
            ev.data["title"].as_str().unwrap_or("A moderation decision on your content").to_string(),
            ev.data["reason"].as_str().unwrap_or("").to_string(),
            post_link.clone().or_else(|| Some("/circle/rules".into())),
            true,
            false,
            None,
            0,
        ),
        // reposts, quotes and the rest: activity list only
        _ => return Ok(()),
    };
    enqueue(st, ev.to, &tenant, ev.kind, &title, &body, link.as_deref(), &ev.data, batch.as_deref(), ev.actor, bell, push, delay_secs).await
}

#[allow(clippy::too_many_arguments)]
pub async fn enqueue(st: &AppState, user: i64, tenant: &str, kind: &str, title: &str, body: &str, link: Option<&str>, data: &Value, batch: Option<&str>, actor: Option<i64>, bell: bool, push: bool, delay_secs: i64) -> anyhow::Result<()> {
    let actors: Vec<i64> = actor.into_iter().collect();
    sqlx::query(
        "INSERT INTO notify_outbox (user_id, tenant, kind, title, body, link, data, batch_key, actors, bell, push, next_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11, now() + make_interval(secs => $12))
         ON CONFLICT (batch_key) WHERE status = 'pending' AND batch_key IS NOT NULL
         DO UPDATE SET actors = CASE WHEN $13::bigint IS NULL OR $13 = ANY(notify_outbox.actors) THEN notify_outbox.actors ELSE notify_outbox.actors || $13 END,
                       body = EXCLUDED.body, title = EXCLUDED.title",
    )
    .bind(user)
    .bind(tenant)
    .bind(format!("circle.{kind}"))
    .bind(title)
    .bind(body)
    .bind(link)
    .bind(sqlx::types::Json(data))
    .bind(batch)
    .bind(&actors)
    .bind(bell)
    .bind(push)
    .bind(delay_secs as f64)
    .bind(actor)
    .execute(&st.pool)
    .await?;
    st.wake.notify.notify_one();
    Ok(())
}

/// Effective preferences: group -> {bell, push} (everything on by default; moderation can't be turned off).
pub async fn prefs(st: &AppState, user: i64) -> anyhow::Result<serde_json::Map<String, Value>> {
    let saved: Value = sqlx::query_scalar::<_, sqlx::types::Json<Value>>("SELECT prefs FROM notif_prefs WHERE user_id = $1").bind(user).fetch_optional(&st.pool).await?.map(|j| j.0).unwrap_or(json!({}));
    let mut out = serde_json::Map::new();
    for (k, label) in PREFS {
        let locked = *k == "moderation";
        let s = &saved[*k];
        out.insert(
            (*k).into(),
            json!({"label": label, "bell": locked || s["bell"].as_bool().unwrap_or(true), "push": s["push"].as_bool().unwrap_or(!locked), "locked": locked}),
        );
    }
    Ok(out)
}

pub async fn save_prefs(st: &AppState, user: i64, patch: &Value) -> anyhow::Result<serde_json::Map<String, Value>> {
    let cur = prefs(st, user).await?;
    let mut saved = serde_json::Map::new();
    for (k, v) in &cur {
        let p = &patch[k.as_str()];
        saved.insert(k.clone(), json!({"bell": p["bell"].as_bool().unwrap_or(v["bell"].as_bool().unwrap_or(true)), "push": p["push"].as_bool().unwrap_or(v["push"].as_bool().unwrap_or(true))}));
    }
    sqlx::query("INSERT INTO notif_prefs (user_id, prefs) VALUES ($1, $2) ON CONFLICT (user_id) DO UPDATE SET prefs = EXCLUDED.prefs, updated_at = now()")
        .bind(user)
        .bind(sqlx::types::Json(Value::Object(saved)))
        .execute(&st.pool)
        .await?;
    prefs(st, user).await
}

/// Delivers due outbox rows (bell + push). Returns how many were handled.
pub async fn deliver(st: &AppState) -> anyhow::Result<usize> {
    let mut tx = st.pool.begin().await?;
    let rows = sqlx::query("SELECT * FROM notify_outbox WHERE status = 'pending' AND next_at <= now() ORDER BY next_at LIMIT 100 FOR UPDATE SKIP LOCKED").fetch_all(&mut *tx).await?;
    let n = rows.len();
    for r in rows {
        let id: i64 = r.get("id");
        let user: i64 = r.get("user_id");
        let tenant: String = r.get("tenant");
        let kind: String = r.get("kind");
        let short = kind.trim_start_matches("circle.").to_string();
        let actors: Vec<i64> = r.get("actors");
        let mut title: String = r.get("title");
        let body: String = r.get("body");
        // batched kinds get their title now, with the final count
        if title.is_empty() || kind == "circle.dm" {
            let first = match actors.first() {
                Some(a) => crate::profiles::by_id(st, *a).await.ok().flatten().map(|p| p.display_name).unwrap_or_else(|| "Someone".into()),
                None => "Someone".into(),
            };
            let others = actors.len().saturating_sub(1);
            title = match short.as_str() {
                "like" | "vote" if others == 0 => format!("{first} reacted to your post"),
                "like" | "vote" => format!("{first} and {others} {} reacted to your post", if others == 1 { "other" } else { "others" }),
                "comment_like" if others == 0 => format!("{first} liked your comment"),
                "comment_like" => format!("{first} and {others} {} liked your comment", if others == 1 { "other" } else { "others" }),
                _ => title,
            };
        }
        let prefs = prefs(st, user).await?;
        let group = pref_group(&short);
        let bell = r.get::<bool, _>("bell") && prefs[group]["bell"].as_bool().unwrap_or(true);
        let push = r.get::<bool, _>("push") && prefs[group]["push"].as_bool().unwrap_or(true);
        let link: Option<String> = r.get("link");
        let data: Value = r.get::<sqlx::types::Json<Value>, _>("data").0;
        let mut error: Option<String> = None;
        if bell {
            let body_json = json!({
                "type": kind, "title": title, "body": body, "link": link, "userId": user, "email": false,
                "dedupeKey": format!("circle:{id}"), "data": {"circle": data, "actors": actors.len()},
            });
            if let Err(e) = upstream::notify(st, &tenant, &body_json).await {
                error = Some(e.to_string());
            }
        }
        // a DM to a member who is reading Circle right now needs no push
        let skip_push = short == "dm" && st.hub.is_online(user);
        if push && !skip_push && error.is_none() {
            let mut d = serde_json::Map::new();
            d.insert("kind".into(), json!(short));
            if let Some(l) = &link {
                d.insert("link".into(), json!(l));
            }
            crate::push::send(st, user, &title, &body, &Value::Object(d)).await;
        }
        match error {
            None => {
                sqlx::query("UPDATE notify_outbox SET status = 'sent', title = $2, sent_at = now(), error = NULL WHERE id = $1").bind(id).bind(&title).execute(&mut *tx).await?;
            }
            Some(e) => {
                let attempts: i32 = r.get::<i32, _>("attempts") + 1;
                let status = if attempts >= 6 { "failed" } else { "pending" };
                sqlx::query("UPDATE notify_outbox SET attempts = $2, status = $3, error = $4, next_at = now() + make_interval(secs => $5) WHERE id = $1")
                    .bind(id)
                    .bind(attempts)
                    .bind(status)
                    .bind(&e)
                    .bind(30.0 * 2f64.powi(attempts))
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }
    tx.commit().await?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups() {
        assert_eq!(pref_group("follow_request"), "follows");
        assert_eq!(pref_group("vote"), "likes");
        assert_eq!(pref_group("dm_request"), "messages");
        assert_eq!(pref_group("trade"), "posts");
        assert_eq!(pref_group("whatever"), "moderation");
    }
}
