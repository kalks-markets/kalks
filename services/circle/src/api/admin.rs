//! Back Office › Circle (Q38): moderation queue, reports, content actions (remove / restore / feature / helpful /
//! shadow), members (warn / ban / shadow-hide / badges), keyword & link rules, announcements, topics, featured
//! creators, settings, chat access through a report or a legal request (time-limited, every read audited), the
//! audit log and the Rewards outbox.
//!
//! Permissions (gateway RBAC): `circle.read`, `circle.moderate`, `circle.content`, `circle.admin`,
//! `circle.chat_access`. Staff of the platform broker (CIRCLE_PLATFORM_TENANT) and the Platform Owner act on the
//! whole community; other brokers' staff see and act on their own clients only.

use super::{Body, Page, Staff, paged};
use crate::audit;
use crate::error::{ApiError, ApiResult, conflict, invalid};
use crate::state::AppState;
use crate::{chat, media, posts, profiles, stories};
use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use sqlx::postgres::PgRow;

type R = ApiResult<Json<Value>>;

/// `AND (<col> = $n)` for staff limited to their broker (bind [`scope_tenant`] as `$n`).
fn scope_sql(_s: &Staff, col: &str, n: usize) -> String {
    format!(" AND (${n}::text IS NULL OR {col} = ${n})")
}

/// The broker a staff member is limited to (None = the whole community).
fn scope_tenant(s: &Staff) -> Option<String> {
    (!s.global).then(|| s.tenant.clone())
}

pub async fn overview(State(st): State<AppState>, s: Staff) -> R {
    s.require("circle.read")?;
    let t = if s.global { None } else { Some(s.tenant.clone()) };
    let r = sqlx::query(
        "SELECT
            (SELECT count(*) FROM mod_queue WHERE status = 'open' AND ($1::text IS NULL OR tenant = $1)) AS queue,
            (SELECT count(*) FROM reports WHERE status = 'open' AND ($1::text IS NULL OR tenant = $1)) AS reports,
            (SELECT count(*) FROM posts p JOIN profiles a ON a.user_id = p.author WHERE p.status = 'published' AND p.published_at > now() - interval '24 hours' AND ($1::text IS NULL OR a.tenant = $1)) AS posts_24h,
            (SELECT count(*) FROM profiles WHERE last_seen_at > now() - interval '24 hours' AND ($1::text IS NULL OR tenant = $1)) AS active_24h,
            (SELECT count(*) FROM profiles WHERE ($1::text IS NULL OR tenant = $1)) AS members,
            (SELECT count(*) FROM profiles WHERE status = 'banned' AND ($1::text IS NULL OR tenant = $1)) AS banned,
            (SELECT count(*) FROM profiles WHERE shadow_hidden AND ($1::text IS NULL OR tenant = $1)) AS shadow,
            (SELECT count(*) FROM posts p JOIN profiles a ON a.user_id = p.author WHERE p.status = 'rejected' AND p.created_at > now() - interval '24 hours' AND ($1::text IS NULL OR a.tenant = $1)) AS rejected_24h,
            (SELECT count(*) FROM outbox WHERE status = 'failed') AS outbox_failed,
            (SELECT count(*) FROM media WHERE status = 'processing') AS media_processing",
    )
    .bind(&t)
    .fetch_one(&st.pool)
    .await?;
    let n = |k: &str| r.get::<i64, _>(k);
    Ok(Json(json!({
        "queue": n("queue"), "reports": n("reports"), "posts24h": n("posts_24h"), "active24h": n("active_24h"), "members": n("members"), "banned": n("banned"),
        "shadowHidden": n("shadow"), "rejected24h": n("rejected_24h"), "outboxFailed": n("outbox_failed"), "mediaProcessing": n("media_processing"),
        "scope": if s.global { "community" } else { "broker" }, "ai": st.ai(), "storage": st.storage.kind(), "push": crate::push::configured(&st.cfg),
    })))
}

/// Staff view of a moderated item (media visible whatever its status; chat messages need chat access).
async fn target_json(st: &AppState, kind: &str, id: i64) -> ApiResult<Value> {
    Ok(match kind {
        "post" => {
            let rows = sqlx::query("SELECT * FROM posts WHERE id = $1").bind(id).fetch_all(&st.pool).await?;
            let author = rows.first().map(|r| r.get::<i64, _>("author")).unwrap_or(0);
            let mut v = posts::render_for(st, author, None, &rows, true).await?.into_iter().next().unwrap_or(Value::Null);
            if let Some(r) = rows.first() {
                v["status"] = json!(r.get::<String, _>("status"));
                v["moderation"] = r.get::<Option<sqlx::types::Json<Value>>, _>("moderation").map(|j| j.0).unwrap_or(Value::Null);
                v["reports"] = json!(r.get::<i32, _>("reports"));
                v["shadow"] = json!(r.get::<bool, _>("shadow"));
            }
            v
        }
        "comment" => {
            let rows = sqlx::query("SELECT * FROM comments WHERE id = $1").bind(id).fetch_all(&st.pool).await?;
            let author = rows.first().map(|r| r.get::<i64, _>("author")).unwrap_or(0);
            posts::comment_json(st, author, &rows).await?.into_iter().next().unwrap_or(Value::Null)
        }
        "story" => {
            let rows = sqlx::query("SELECT * FROM stories WHERE id = $1").bind(id).fetch_all(&st.pool).await?;
            let author = rows.first().map(|r| r.get::<i64, _>("author")).unwrap_or(0);
            stories::render(st, author, &rows).await?.into_iter().next().unwrap_or(Value::Null)
        }
        "media" => {
            let r = sqlx::query("SELECT * FROM media WHERE id = $1").bind(id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
            media::json_row(st, &r, true)
        }
        "profile" => {
            let p = profiles::by_id(st, id).await?.ok_or(ApiError::NotFound)?;
            json!({"profile": profiles::card(st, &p), "bio": p.bio})
        }
        // messages: shown only through chat access (the queue shows the excerpt the AI flagged)
        "message" => json!({"id": id, "note": "Open the conversation through chat access to read it."}),
        _ => Value::Null,
    })
}

async fn owner_tenant(st: &AppState, owner: i64) -> ApiResult<String> {
    Ok(sqlx::query_scalar("SELECT tenant FROM profiles WHERE user_id = $1").bind(owner).fetch_optional(&st.pool).await?.unwrap_or_else(|| "kalks".into()))
}

#[derive(Deserialize)]
pub struct QueueQ {
    status: Option<String>,
    kind: Option<String>,
    cursor: Option<String>,
    limit: Option<i64>,
}

pub async fn queue(State(st): State<AppState>, s: Staff, Query(q): Query<QueueQ>) -> R {
    s.require("circle.read")?;
    let limit = crate::util::clamp_limit(q.limit, 30, 100);
    let before = q.cursor.as_deref().and_then(|c| c.parse::<i64>().ok()).unwrap_or(i64::MAX);
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT * FROM mod_queue WHERE status = $1 AND ($2::text IS NULL OR target_kind = $2) AND id < $3 {} ORDER BY id DESC LIMIT $4",
        scope_sql(&s, "tenant", 5)
    )))
    .bind(q.status.as_deref().unwrap_or("open"))
    .bind(&q.kind)
    .bind(before)
    .bind(limit + 1)
    .bind(scope_tenant(&s))
    .fetch_all(&st.pool)
    .await?;
    let owners: Vec<i64> = rows.iter().map(|r| r.get("owner")).collect();
    let cards = profiles::cards(&st, &owners).await?;
    let items: Vec<Value> = rows.iter().map(|r| queue_json(r, cards.get(&r.get::<i64, _>("owner")).cloned())).collect();
    Ok(Json(paged(items, limit, |v| v["id"].as_i64().map(|i| i.to_string()))))
}

fn queue_json(r: &PgRow, owner: Option<Value>) -> Value {
    json!({
        "id": r.get::<i64, _>("id"), "targetKind": r.get::<String, _>("target_kind"), "targetId": r.get::<i64, _>("target_id"), "owner": owner,
        "source": r.get::<String, _>("source"), "categories": r.get::<Vec<String>, _>("categories"), "verdict": r.get::<sqlx::types::Json<Value>, _>("verdict").0,
        "excerpt": r.get::<String, _>("excerpt"), "status": r.get::<String, _>("status"), "decidedBy": r.get::<Option<String>, _>("decided_by"),
        "note": r.get::<Option<String>, _>("note"), "createdAt": r.get::<DateTime<Utc>, _>("created_at"), "decidedAt": r.get::<Option<DateTime<Utc>>, _>("decided_at"),
    })
}

pub async fn queue_item(State(st): State<AppState>, s: Staff, Path(id): Path<i64>) -> R {
    s.require("circle.read")?;
    let r = sqlx::query("SELECT * FROM mod_queue WHERE id = $1").bind(id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    s.scope(&r.get::<String, _>("tenant"))?;
    let owner = profiles::cards(&st, &[r.get("owner")]).await?.remove(&r.get::<i64, _>("owner"));
    let mut v = queue_json(&r, owner);
    v["target"] = target_json(&st, &r.get::<String, _>("target_kind"), r.get("target_id")).await?;
    let reports = sqlx::query("SELECT id, reason, note, status, created_at FROM reports WHERE target_kind = $1 AND target_id = $2 ORDER BY id DESC LIMIT 50").bind(r.get::<String, _>("target_kind")).bind(r.get::<i64, _>("target_id")).fetch_all(&st.pool).await?;
    v["reports"] = json!(reports.iter().map(|x| json!({"id": x.get::<i64, _>("id"), "reason": x.get::<String, _>("reason"), "note": x.get::<String, _>("note"), "status": x.get::<String, _>("status"), "createdAt": x.get::<DateTime<Utc>, _>("created_at")})).collect::<Vec<_>>());
    Ok(Json(v))
}

/// Restores a held / removed item to visible.
async fn approve_target(st: &AppState, kind: &str, id: i64) -> ApiResult<()> {
    match kind {
        "post" => {
            // a removed post that was already published counts again on its author's profile
            let was: Option<(String, bool, i64)> = sqlx::query_as("SELECT status, notified, author FROM posts WHERE id = $1").bind(id).fetch_optional(&st.pool).await?;
            sqlx::query("UPDATE posts SET status = 'pending' WHERE id = $1 AND status IN ('review','removed','rejected')").bind(id).execute(&st.pool).await?;
            posts::publish(st, id).await?;
            if let Some((status, true, author)) = was
                && status == "removed"
            {
                sqlx::query("UPDATE profiles SET posts_count = posts_count + 1 WHERE user_id = $1").bind(author).execute(&st.pool).await?;
            }
        }
        "comment" => {
            sqlx::query("UPDATE comments SET status = 'pending' WHERE id = $1 AND status IN ('review','removed','rejected')").bind(id).execute(&st.pool).await?;
            posts::publish_comment(st, id).await?;
        }
        "story" => {
            sqlx::query("UPDATE stories SET status = 'pending' WHERE id = $1 AND status IN ('review','removed','rejected')").bind(id).execute(&st.pool).await?;
            stories::publish(st, id).await?;
        }
        "media" => {
            sqlx::query("UPDATE media SET status = 'ready', updated_at = now() WHERE id = $1 AND status = 'review'").bind(id).execute(&st.pool).await?;
            st.wake.moderation.notify_one();
        }
        "message" => {
            if let Some(r) = sqlx::query("UPDATE messages SET status = 'sent' WHERE id = $1 AND status IN ('hidden','pending') RETURNING *").bind(id).fetch_optional(&st.pool).await? {
                chat::deliver(st, &r).await?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Takes an item down; returns its owner.
async fn remove_target(st: &AppState, kind: &str, id: i64) -> ApiResult<Option<i64>> {
    Ok(match kind {
        "post" => {
            let r = sqlx::query("SELECT * FROM posts WHERE id = $1").bind(id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
            posts::remove_post(st, &r, "removed").await?;
            Some(r.get("author"))
        }
        "comment" => {
            let r = sqlx::query("SELECT * FROM comments WHERE id = $1").bind(id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
            posts::remove_comment(st, &r, "removed").await?;
            Some(r.get("author"))
        }
        "story" => sqlx::query_scalar("UPDATE stories SET status = 'removed' WHERE id = $1 RETURNING author").bind(id).fetch_optional(&st.pool).await?,
        "message" => {
            let r = sqlx::query("UPDATE messages SET status = 'hidden' WHERE id = $1 RETURNING sender, conversation_id").bind(id).fetch_optional(&st.pool).await?;
            if let Some(r) = &r {
                let c: i64 = r.get("conversation_id");
                st.hub.send(crate::state::Target::Conv(c), json!({"type": "message.hidden", "conversationId": c, "messageId": id}));
            }
            r.map(|r| r.get("sender"))
        }
        "media" => {
            let owner: Option<i64> = sqlx::query_scalar("UPDATE media SET status = 'rejected', reason = 'Removed by a moderator.' WHERE id = $1 RETURNING owner").bind(id).fetch_optional(&st.pool).await?;
            media::purge(st, id).await?;
            sqlx::query("UPDATE media SET status = 'rejected' WHERE id = $1").bind(id).execute(&st.pool).await?;
            st.wake.moderation.notify_one();
            owner
        }
        "profile" => Some(id),
        _ => return Err(ApiError::NotFound),
    })
}

async fn tell_owner(st: &AppState, owner: i64, kind: &str, id: i64, title: &str, reason: &str) -> anyhow::Result<()> {
    crate::notify::event(
        st,
        crate::notify::Ev { to: owner, kind: "moderation", actor: None, post: (kind == "post").then_some(id), comment: (kind == "comment").then_some(id), story: (kind == "story").then_some(id), conversation: None, data: json!({"title": title, "reason": reason}) },
    )
    .await
}

pub async fn decide(State(st): State<AppState>, s: Staff, Path((id, decision)): Path<(i64, String)>, Body(b): Body) -> R {
    s.require("circle.moderate")?;
    let r = sqlx::query("SELECT * FROM mod_queue WHERE id = $1").bind(id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    s.scope(&r.get::<String, _>("tenant"))?;
    let kind: String = r.get("target_kind");
    let tid: i64 = r.get("target_id");
    let note = crate::util::clean(b["note"].as_str().unwrap_or(""), 500);
    let status = match decision.as_str() {
        "approve" => {
            approve_target(&st, &kind, tid).await?;
            "approved"
        }
        "remove" => {
            if let Some(owner) = remove_target(&st, &kind, tid).await? {
                let reason = b["reason"].as_str().map(|r| crate::util::clean(r, 300)).filter(|r| !r.is_empty()).unwrap_or_else(|| {
                    r.get::<Vec<String>, _>("categories").first().map(|c| crate::moderation::reason_text(c).to_string()).unwrap_or_else(|| "It breaks the community rules.".into())
                });
                tell_owner(&st, owner, &kind, tid, &format!("Your {kind} was removed"), &reason).await?;
                if b["warn"].as_bool() == Some(true) {
                    sanction(&st, &s, owner, "warn", &reason, None).await?;
                }
            }
            "removed"
        }
        "dismiss" => "dismissed",
        _ => return Err(ApiError::NotFound),
    };
    sqlx::query("UPDATE mod_queue SET status = $2, decided_by = $3, decided_at = now(), note = $4 WHERE id = $1").bind(id).bind(status).bind(format!("staff:{}", s.id)).bind(&note).execute(&st.pool).await?;
    sqlx::query("UPDATE reports SET status = CASE WHEN $3 = 'removed' THEN 'actioned' ELSE 'dismissed' END, resolution = $3, resolved_by = $4, resolved_at = now() WHERE target_kind = $1 AND target_id = $2 AND status = 'open'")
        .bind(&kind)
        .bind(tid)
        .bind(status)
        .bind(format!("staff:{}", s.id))
        .execute(&st.pool)
        .await?;
    audit::record(&st.pool, &s.actor(), &format!("queue.{decision}"), Some(format!("{kind}:{tid}")), None, Some(json!({"queueId": id, "status": status})), Some(&note)).await?;
    Ok(Json(json!({"status": status})))
}

#[derive(Deserialize)]
pub struct ReportsQ {
    status: Option<String>,
    cursor: Option<String>,
    limit: Option<i64>,
}

pub async fn reports(State(st): State<AppState>, s: Staff, Query(q): Query<ReportsQ>) -> R {
    s.require("circle.read")?;
    let limit = crate::util::clamp_limit(q.limit, 30, 100);
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT * FROM reports WHERE status = $1 AND id < $2 {} ORDER BY id DESC LIMIT $3",
        scope_sql(&s, "tenant", 4)
    )))
    .bind(q.status.as_deref().unwrap_or("open"))
    .bind(q.cursor.as_deref().and_then(|c| c.parse::<i64>().ok()).unwrap_or(i64::MAX))
    .bind(limit + 1)
    .bind(scope_tenant(&s))
    .fetch_all(&st.pool)
    .await?;
    let ids: Vec<i64> = rows.iter().flat_map(|r| [r.get::<i64, _>("reporter"), r.get::<Option<i64>, _>("target_owner").unwrap_or(0)]).collect();
    let cards = profiles::cards(&st, &ids).await?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<i64, _>("id"), "reporter": cards.get(&r.get::<i64, _>("reporter")), "targetKind": r.get::<String, _>("target_kind"), "targetId": r.get::<i64, _>("target_id"),
                "targetOwner": r.get::<Option<i64>, _>("target_owner").and_then(|o| cards.get(&o).cloned()), "reason": r.get::<String, _>("reason"), "note": r.get::<String, _>("note"),
                "status": r.get::<String, _>("status"), "resolution": r.get::<Option<String>, _>("resolution"), "createdAt": r.get::<DateTime<Utc>, _>("created_at"),
            })
        })
        .collect();
    Ok(Json(paged(items, limit, |v| v["id"].as_i64().map(|i| i.to_string()))))
}

/// Resolves a report: dismiss | remove | warn | ban | shadow (the last three act on the reported member).
pub async fn resolve_report(State(st): State<AppState>, s: Staff, Path(id): Path<i64>, Body(b): Body) -> R {
    s.require("circle.moderate")?;
    let r = sqlx::query("SELECT * FROM reports WHERE id = $1").bind(id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    s.scope(&r.get::<String, _>("tenant"))?;
    let action = b["action"].as_str().unwrap_or("dismiss");
    let kind: String = r.get("target_kind");
    let tid: i64 = r.get("target_id");
    let owner: Option<i64> = r.get("target_owner");
    let note = crate::util::clean(b["note"].as_str().unwrap_or(""), 500);
    let reason = b["reason"].as_str().map(|x| crate::util::clean(x, 300)).filter(|x| !x.is_empty()).unwrap_or_else(|| "It breaks the community rules.".into());
    match action {
        "dismiss" => {}
        "remove" => {
            if kind != "profile" && kind != "conversation" {
                if let Some(o) = remove_target(&st, &kind, tid).await? {
                    tell_owner(&st, o, &kind, tid, &format!("Your {kind} was removed"), &reason).await?;
                }
            }
        }
        "warn" | "ban" | "shadow" => {
            let o = owner.ok_or_else(|| invalid("action", "This report has no member to act on."))?;
            sanction(&st, &s, o, action, &reason, b["days"].as_i64()).await?;
        }
        _ => return Err(invalid("action", "action is dismiss, remove, warn, ban or shadow.")),
    }
    let status = if action == "dismiss" { "dismissed" } else { "actioned" };
    sqlx::query("UPDATE reports SET status = $3, resolution = $4, resolved_by = $5, resolved_at = now() WHERE target_kind = $1 AND target_id = $2 AND status = 'open'")
        .bind(&kind)
        .bind(tid)
        .bind(status)
        .bind(action)
        .bind(format!("staff:{}", s.id))
        .execute(&st.pool)
        .await?;
    sqlx::query("UPDATE mod_queue SET status = $3, decided_by = $4, decided_at = now(), note = $5 WHERE target_kind = $1 AND target_id = $2 AND status = 'open'")
        .bind(&kind)
        .bind(tid)
        .bind(if action == "dismiss" { "dismissed" } else { "removed" })
        .bind(format!("staff:{}", s.id))
        .bind(&note)
        .execute(&st.pool)
        .await?;
    audit::record(&st.pool, &s.actor(), &format!("report.{action}"), Some(format!("{kind}:{tid}")), None, Some(json!({"reportId": id})), Some(&note)).await?;
    Ok(Json(json!({"status": status})))
}

/// Warn / ban / unban / shadow / unshadow a member (audited, the member is told except for shadow-hiding).
async fn sanction(st: &AppState, s: &Staff, user: i64, kind: &str, reason: &str, days: Option<i64>) -> ApiResult<()> {
    let before = profiles::by_id(st, user).await?.ok_or(ApiError::NotFound)?;
    s.scope(&before.tenant)?;
    let until = days.filter(|d| *d > 0).map(|d| Utc::now() + chrono::Duration::days(d.min(3650)));
    match kind {
        "warn" => {
            sqlx::query("UPDATE profiles SET warnings = warnings + 1 WHERE user_id = $1").bind(user).execute(&st.pool).await?;
            crate::notify::event(st, crate::notify::Ev { to: user, kind: "moderation", actor: None, post: None, comment: None, story: None, conversation: None, data: json!({"title": "You received a warning", "reason": reason}) }).await?;
        }
        "ban" => {
            sqlx::query("UPDATE profiles SET status = 'banned', banned_until = $2, ban_reason = $3 WHERE user_id = $1").bind(user).bind(until).bind(reason).execute(&st.pool).await?;
            crate::notify::event(st, crate::notify::Ev { to: user, kind: "moderation", actor: None, post: None, comment: None, story: None, conversation: None, data: json!({"title": "Your Kalks Circle access is suspended", "reason": reason, "until": until}) }).await?;
        }
        "unban" => {
            sqlx::query("UPDATE profiles SET status = 'active', banned_until = NULL, ban_reason = NULL WHERE user_id = $1").bind(user).execute(&st.pool).await?;
        }
        "shadow" => {
            sqlx::query("UPDATE profiles SET shadow_hidden = true WHERE user_id = $1").bind(user).execute(&st.pool).await?;
        }
        "unshadow" => {
            sqlx::query("UPDATE profiles SET shadow_hidden = false WHERE user_id = $1").bind(user).execute(&st.pool).await?;
        }
        _ => return Err(ApiError::NotFound),
    }
    sqlx::query("INSERT INTO sanctions (user_id, kind, reason, until, staff) VALUES ($1,$2,$3,$4,$5)").bind(user).bind(kind).bind(reason).bind(until).bind(format!("staff:{}", s.id)).execute(&st.pool).await?;
    audit::record(&st.pool, &s.actor(), &format!("user.{kind}"), Some(format!("user:{user}")), Some(json!({"status": before.status, "shadow": before.shadow_hidden, "warnings": before.warnings})), Some(json!({"until": until})), Some(reason)).await?;
    Ok(())
}

pub async fn content(State(st): State<AppState>, s: Staff, Path((kind, id)): Path<(String, i64)>) -> R {
    s.require("circle.read")?;
    if kind == "message" {
        return Err(ApiError::Forbidden("Chat messages are opened through chat access (a report or a legal request).".into()));
    }
    let owner: Option<i64> = match kind.as_str() {
        "post" => sqlx::query_scalar("SELECT author FROM posts WHERE id = $1").bind(id).fetch_optional(&st.pool).await?,
        "comment" => sqlx::query_scalar("SELECT author FROM comments WHERE id = $1").bind(id).fetch_optional(&st.pool).await?,
        "story" => sqlx::query_scalar("SELECT author FROM stories WHERE id = $1").bind(id).fetch_optional(&st.pool).await?,
        "media" => sqlx::query_scalar("SELECT owner FROM media WHERE id = $1").bind(id).fetch_optional(&st.pool).await?,
        _ => return Err(ApiError::NotFound),
    };
    let owner = owner.ok_or(ApiError::NotFound)?;
    s.scope(&owner_tenant(&st, owner).await?)?;
    Ok(Json(json!({"kind": kind, "item": target_json(&st, &kind, id).await?})))
}

/// `POST /v1/circle/admin/content/{kind}/{id}/{action}`: remove | restore (posts, comments, stories, messages,
/// media); feature | unfeature | helpful | shadow | unshadow (posts).
pub async fn content_action(State(st): State<AppState>, s: Staff, Path((kind, id, action)): Path<(String, i64, String)>, Body(b): Body) -> R {
    let content_actions = ["feature", "unfeature", "helpful"];
    s.require(if content_actions.contains(&action.as_str()) { "circle.content" } else { "circle.moderate" })?;
    let owner: Option<i64> = match kind.as_str() {
        "post" => sqlx::query_scalar("SELECT author FROM posts WHERE id = $1").bind(id).fetch_optional(&st.pool).await?,
        "comment" => sqlx::query_scalar("SELECT author FROM comments WHERE id = $1").bind(id).fetch_optional(&st.pool).await?,
        "story" => sqlx::query_scalar("SELECT author FROM stories WHERE id = $1").bind(id).fetch_optional(&st.pool).await?,
        "message" => sqlx::query_scalar("SELECT sender FROM messages WHERE id = $1").bind(id).fetch_optional(&st.pool).await?,
        "media" => sqlx::query_scalar("SELECT owner FROM media WHERE id = $1").bind(id).fetch_optional(&st.pool).await?,
        _ => return Err(ApiError::NotFound),
    };
    let owner = owner.ok_or(ApiError::NotFound)?;
    s.scope(&owner_tenant(&st, owner).await?)?;
    let reason = b["reason"].as_str().map(|x| crate::util::clean(x, 300)).filter(|x| !x.is_empty()).unwrap_or_else(|| "It breaks the community rules.".into());
    match (kind.as_str(), action.as_str()) {
        (_, "remove") => {
            remove_target(&st, &kind, id).await?;
            if kind != "message" {
                tell_owner(&st, owner, &kind, id, &format!("Your {kind} was removed"), &reason).await?;
            }
        }
        (_, "restore") => approve_target(&st, &kind, id).await?,
        ("post", "feature") => {
            let hours = b["hours"].as_i64().unwrap_or(72).clamp(1, 24 * 30);
            sqlx::query("UPDATE posts SET featured_until = now() + make_interval(hours => $2) WHERE id = $1 AND status = 'published'").bind(id).bind(hours as i32).execute(&st.pool).await?;
        }
        ("post", "unfeature") => {
            sqlx::query("UPDATE posts SET featured_until = NULL WHERE id = $1").bind(id).execute(&st.pool).await?;
        }
        ("post", "helpful") => {
            if !crate::rewards::mark_helpful(&st, id, &format!("staff:{}", s.id)).await? {
                return Err(conflict("already_helpful", "This post is already marked helpful."));
            }
        }
        ("post", "shadow") => {
            sqlx::query("UPDATE posts SET shadow = true WHERE id = $1").bind(id).execute(&st.pool).await?;
        }
        ("post", "unshadow") => {
            sqlx::query("UPDATE posts SET shadow = false WHERE id = $1").bind(id).execute(&st.pool).await?;
        }
        _ => return Err(ApiError::NotFound),
    }
    audit::record(&st.pool, &s.actor(), &format!("{kind}.{action}"), Some(format!("{kind}:{id}")), None, Some(b.clone()), None).await?;
    Ok(Json(json!({"status": "ok"})))
}

#[derive(Deserialize)]
pub struct UsersQ {
    q: Option<String>,
    status: Option<String>,
    cursor: Option<String>,
    limit: Option<i64>,
}

fn admin_user_json(st: &AppState, p: &profiles::Profile) -> Value {
    let mut v = profiles::card(st, p);
    v["status"] = json!(p.status);
    v["bannedUntil"] = json!(p.banned_until);
    v["banReason"] = json!(p.ban_reason);
    v["shadowHidden"] = json!(p.shadow_hidden);
    v["warnings"] = json!(p.warnings);
    v["staffBadge"] = json!(p.staff_badge);
    v["creator"] = json!(p.creator);
    v["feeDiscount"] = json!(p.fee_discount);
    v["kycStatus"] = json!(p.kyc_status);
    v["tenant"] = json!(p.tenant);
    v["counts"] = json!({"followers": p.followers_count, "following": p.following_count, "posts": p.posts_count});
    v["joinedAt"] = json!(p.created_at);
    v
}

pub async fn users(State(st): State<AppState>, s: Staff, Query(q): Query<UsersQ>) -> R {
    s.require("circle.read")?;
    let limit = crate::util::clamp_limit(q.limit, 30, 100);
    let needle = q.q.map(|x| crate::util::clean(&x, 60).trim_start_matches('@').to_lowercase()).filter(|x| !x.is_empty());
    let id_match = needle.as_deref().and_then(|n| n.parse::<i64>().ok());
    let rows = sqlx::query_as::<_, profiles::Profile>(sqlx::AssertSqlSafe(format!(
        "SELECT {} FROM profiles a WHERE ($1::text IS NULL OR a.handle LIKE $1 || '%' OR lower(a.display_name) LIKE '%' || $1 || '%' OR a.user_id = $2)
           AND ($3::text IS NULL OR a.status = $3 OR ($3 = 'shadow' AND a.shadow_hidden)) AND a.user_id < $4 {} ORDER BY a.user_id DESC LIMIT $5",
        profiles::cols("a"),
        scope_sql(&s, "a.tenant", 6)
    )))
    .bind(&needle)
    .bind(id_match)
    .bind(&q.status)
    .bind(q.cursor.as_deref().and_then(|c| c.parse::<i64>().ok()).unwrap_or(i64::MAX))
    .bind(limit + 1)
    .bind(scope_tenant(&s))
    .fetch_all(&st.pool)
    .await?;
    let items: Vec<Value> = rows.iter().map(|p| admin_user_json(&st, p)).collect();
    Ok(Json(paged(items, limit, |v| v["id"].as_i64().map(|i| i.to_string()))))
}

pub async fn user(State(st): State<AppState>, s: Staff, Path(user): Path<i64>) -> R {
    s.require("circle.read")?;
    let p = profiles::by_id(&st, user).await?.ok_or(ApiError::NotFound)?;
    s.scope(&p.tenant)?;
    let sanctions = sqlx::query("SELECT kind, reason, until, staff, created_at FROM sanctions WHERE user_id = $1 ORDER BY id DESC LIMIT 50").bind(user).fetch_all(&st.pool).await?;
    let reports: i64 = sqlx::query_scalar("SELECT count(*) FROM reports WHERE target_owner = $1").bind(user).fetch_one(&st.pool).await?;
    let recent = sqlx::query("SELECT * FROM posts WHERE author = $1 AND status <> 'deleted' ORDER BY id DESC LIMIT 10").bind(user).fetch_all(&st.pool).await?;
    let mut v = admin_user_json(&st, &p);
    v["bio"] = json!(p.bio);
    v["stats"] = profiles::stats_public(&p.stats.0, p.stats_at);
    v["sanctions"] = json!(sanctions.iter().map(|r| json!({"kind": r.get::<String, _>("kind"), "reason": r.get::<String, _>("reason"), "until": r.get::<Option<DateTime<Utc>>, _>("until"), "staff": r.get::<String, _>("staff"), "at": r.get::<DateTime<Utc>, _>("created_at")})).collect::<Vec<_>>());
    v["reportsAgainst"] = json!(reports);
    v["recentPosts"] = json!(posts::render_for(&st, user, None, &recent, false).await?);
    Ok(Json(json!({"user": v})))
}

/// `POST /v1/circle/admin/users/{user}/{action}`: warn | ban | unban | shadow | unshadow (moderate);
/// badge {badge: team | mentor | null} | creator {on} | fee-discount {eligible} (content / admin).
pub async fn user_action(State(st): State<AppState>, s: Staff, Path((user, action)): Path<(i64, String)>, Body(b): Body) -> R {
    let reason = crate::util::clean(b["reason"].as_str().unwrap_or(""), 300);
    match action.as_str() {
        "warn" | "ban" | "unban" | "shadow" | "unshadow" => {
            s.require("circle.moderate")?;
            if (action == "warn" || action == "ban") && reason.is_empty() {
                return Err(invalid("reason", "Give a reason (the member sees it)."));
            }
            sanction(&st, &s, user, &action, &reason, b["days"].as_i64()).await?;
        }
        "badge" => {
            s.require("circle.content")?;
            let p = profiles::by_id(&st, user).await?.ok_or(ApiError::NotFound)?;
            s.scope(&p.tenant)?;
            let badge = match b["badge"].as_str() {
                Some("team") => Some("team"),
                Some("mentor") => Some("mentor"),
                None => None,
                _ => return Err(invalid("badge", "badge is team, mentor or null.")),
            };
            sqlx::query("UPDATE profiles SET staff_badge = $2 WHERE user_id = $1").bind(user).bind(badge).execute(&st.pool).await?;
            audit::record(&st.pool, &s.actor(), "user.badge", Some(format!("user:{user}")), Some(json!({"badge": p.staff_badge})), Some(json!({"badge": badge})), None).await?;
        }
        "creator" => {
            s.require("circle.content")?;
            let on = b["on"].as_bool().unwrap_or(true);
            sqlx::query("UPDATE profiles SET creator = $2 WHERE user_id = $1").bind(user).bind(on).execute(&st.pool).await?;
            audit::record(&st.pool, &s.actor(), "user.creator", Some(format!("user:{user}")), None, Some(json!({"creator": on})), None).await?;
        }
        "fee-discount" => {
            s.require("circle.admin")?;
            let on = b["eligible"].as_bool().unwrap_or(true);
            sqlx::query("UPDATE profiles SET fee_discount = $2 WHERE user_id = $1").bind(user).bind(on).execute(&st.pool).await?;
            audit::record(&st.pool, &s.actor(), "user.fee_discount", Some(format!("user:{user}")), None, Some(json!({"eligible": on})), None).await?;
        }
        _ => return Err(ApiError::NotFound),
    }
    let p = profiles::by_id(&st, user).await?.ok_or(ApiError::NotFound)?;
    Ok(Json(json!({"user": admin_user_json(&st, &p)})))
}

// ---------------------------------------------------------------- rules

pub async fn rules(State(st): State<AppState>, s: Staff) -> R {
    s.require("circle.read")?;
    let rows = sqlx::query("SELECT * FROM rules ORDER BY kind, lower(pattern)").fetch_all(&st.pool).await?;
    Ok(Json(json!({"items": rows.iter().map(rule_json).collect::<Vec<_>>()})))
}

fn rule_json(r: &PgRow) -> Value {
    json!({"id": r.get::<i64, _>("id"), "kind": r.get::<String, _>("kind"), "pattern": r.get::<String, _>("pattern"), "action": r.get::<String, _>("action"), "note": r.get::<String, _>("note"), "active": r.get::<bool, _>("active"), "createdBy": r.get::<String, _>("created_by"), "createdAt": r.get::<DateTime<Utc>, _>("created_at")})
}

fn rule_fields(b: &Value) -> ApiResult<(String, String, String)> {
    let kind = match b["kind"].as_str() {
        Some("keyword") => "keyword",
        Some("link_allow") => "link_allow",
        _ => return Err(invalid("kind", "kind is keyword or link_allow.")),
    };
    let pattern = crate::util::clean(b["pattern"].as_str().unwrap_or(""), 120).to_lowercase();
    if pattern.len() < 2 {
        return Err(invalid("pattern", "Enter the word, phrase or domain."));
    }
    let pattern = if kind == "link_allow" {
        let p = pattern.trim_start_matches("https://").trim_start_matches("http://").trim_start_matches("www.").trim_end_matches('/').to_string();
        if p.contains('/') || !p.contains('.') || !p.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
            return Err(invalid("pattern", "Enter a domain such as tradingview.com."));
        }
        p
    } else {
        pattern
    };
    let action = if kind == "link_allow" {
        "allow"
    } else {
        match b["action"].as_str().unwrap_or("block") {
            "block" => "block",
            "review" => "review",
            _ => return Err(invalid("action", "action is block or review.")),
        }
    };
    Ok((kind.into(), pattern, action.into()))
}

pub async fn create_rule(State(st): State<AppState>, s: Staff, Body(b): Body) -> R {
    s.require("circle.admin")?;
    let (kind, pattern, action) = rule_fields(&b)?;
    let r = sqlx::query("INSERT INTO rules (kind, pattern, action, note, created_by) VALUES ($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING RETURNING *")
        .bind(&kind)
        .bind(&pattern)
        .bind(&action)
        .bind(crate::util::clean(b["note"].as_str().unwrap_or(""), 200))
        .bind(format!("staff:{}", s.id))
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| conflict("exists", "This rule exists already."))?;
    crate::moderation::invalidate_rules(&st);
    audit::record(&st.pool, &s.actor(), "rule.create", Some(format!("rule:{}", r.get::<i64, _>("id"))), None, Some(rule_json(&r)), None).await?;
    Ok(Json(json!({"rule": rule_json(&r)})))
}

pub async fn edit_rule(State(st): State<AppState>, s: Staff, Path(id): Path<i64>, Body(b): Body) -> R {
    s.require("circle.admin")?;
    let before = sqlx::query("SELECT * FROM rules WHERE id = $1").bind(id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    if let Some(a) = b["active"].as_bool() {
        sqlx::query("UPDATE rules SET active = $2 WHERE id = $1").bind(id).bind(a).execute(&st.pool).await?;
    }
    if let Some(a) = b["action"].as_str() {
        if before.get::<String, _>("kind") == "keyword" && ["block", "review"].contains(&a) {
            sqlx::query("UPDATE rules SET action = $2 WHERE id = $1").bind(id).bind(a).execute(&st.pool).await?;
        }
    }
    if let Some(n) = b["note"].as_str() {
        sqlx::query("UPDATE rules SET note = $2 WHERE id = $1").bind(id).bind(crate::util::clean(n, 200)).execute(&st.pool).await?;
    }
    let after = sqlx::query("SELECT * FROM rules WHERE id = $1").bind(id).fetch_one(&st.pool).await?;
    crate::moderation::invalidate_rules(&st);
    audit::record(&st.pool, &s.actor(), "rule.update", Some(format!("rule:{id}")), Some(rule_json(&before)), Some(rule_json(&after)), None).await?;
    Ok(Json(json!({"rule": rule_json(&after)})))
}

pub async fn delete_rule(State(st): State<AppState>, s: Staff, Path(id): Path<i64>) -> R {
    s.require("circle.admin")?;
    let before = sqlx::query("DELETE FROM rules WHERE id = $1 RETURNING *").bind(id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    crate::moderation::invalidate_rules(&st);
    audit::record(&st.pool, &s.actor(), "rule.delete", Some(format!("rule:{id}")), Some(rule_json(&before)), None, None).await?;
    Ok(Json(json!({"status": "deleted"})))
}

// ---------------------------------------------------------------- announcements, topics, features

fn announcement_json(r: &PgRow) -> Value {
    json!({"id": r.get::<i64, _>("id"), "title": r.get::<String, _>("title"), "body": r.get::<String, _>("body"), "link": r.get::<Option<String>, _>("link"), "pinned": r.get::<bool, _>("pinned"), "startsAt": r.get::<DateTime<Utc>, _>("starts_at"), "endsAt": r.get::<Option<DateTime<Utc>>, _>("ends_at"), "createdBy": r.get::<String, _>("created_by")})
}

fn parse_time(v: &Value) -> ApiResult<Option<DateTime<Utc>>> {
    match v.as_str() {
        None => Ok(None),
        Some(s) => DateTime::parse_from_rfc3339(s).map(|d| Some(d.with_timezone(&Utc))).map_err(|_| invalid("endsAt", "Use an ISO 8601 time.")),
    }
}

fn link_ok(v: &Value) -> ApiResult<Option<String>> {
    match v.as_str().map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(l) if (l.starts_with('/') && !l.starts_with("//")) || l.starts_with("https://") => Ok(Some(l.chars().take(500).collect())),
        Some(_) => Err(invalid("link", "Use an app path such as /circle/tags/gold or an https:// URL.")),
    }
}

pub async fn announcements(State(st): State<AppState>, s: Staff) -> R {
    s.require("circle.read")?;
    let rows = sqlx::query("SELECT * FROM announcements ORDER BY id DESC LIMIT 100").fetch_all(&st.pool).await?;
    Ok(Json(json!({"items": rows.iter().map(announcement_json).collect::<Vec<_>>()})))
}

pub async fn create_announcement(State(st): State<AppState>, s: Staff, Body(b): Body) -> R {
    s.require("circle.content")?;
    let title = crate::util::clean(b["title"].as_str().unwrap_or(""), 120);
    if title.is_empty() {
        return Err(invalid("title", "Give the announcement a title."));
    }
    let r = sqlx::query("INSERT INTO announcements (title, body, link, pinned, starts_at, ends_at, created_by) VALUES ($1,$2,$3,$4,COALESCE($5, now()),$6,$7) RETURNING *")
        .bind(&title)
        .bind(crate::util::clean(b["body"].as_str().unwrap_or(""), 1000))
        .bind(link_ok(&b["link"])?)
        .bind(b["pinned"].as_bool().unwrap_or(true))
        .bind(parse_time(&b["startsAt"])?)
        .bind(parse_time(&b["endsAt"])?)
        .bind(format!("staff:{}", s.id))
        .fetch_one(&st.pool)
        .await?;
    audit::record(&st.pool, &s.actor(), "announcement.create", Some(format!("announcement:{}", r.get::<i64, _>("id"))), None, Some(announcement_json(&r)), None).await?;
    Ok(Json(json!({"announcement": announcement_json(&r)})))
}

pub async fn edit_announcement(State(st): State<AppState>, s: Staff, Path(id): Path<i64>, Body(b): Body) -> R {
    s.require("circle.content")?;
    let before = sqlx::query("SELECT * FROM announcements WHERE id = $1").bind(id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    let title = b["title"].as_str().map(|t| crate::util::clean(t, 120)).filter(|t| !t.is_empty()).unwrap_or_else(|| before.get("title"));
    let body = b["body"].as_str().map(|t| crate::util::clean(t, 1000)).unwrap_or_else(|| before.get("body"));
    let link = if b.get("link").is_some() { link_ok(&b["link"])? } else { before.get("link") };
    let pinned = b["pinned"].as_bool().unwrap_or_else(|| before.get("pinned"));
    let ends = if b.get("endsAt").is_some() { parse_time(&b["endsAt"])? } else { before.get("ends_at") };
    let r = sqlx::query("UPDATE announcements SET title = $2, body = $3, link = $4, pinned = $5, ends_at = $6 WHERE id = $1 RETURNING *").bind(id).bind(title).bind(body).bind(link).bind(pinned).bind(ends).fetch_one(&st.pool).await?;
    audit::record(&st.pool, &s.actor(), "announcement.update", Some(format!("announcement:{id}")), Some(announcement_json(&before)), Some(announcement_json(&r)), None).await?;
    Ok(Json(json!({"announcement": announcement_json(&r)})))
}

pub async fn delete_announcement(State(st): State<AppState>, s: Staff, Path(id): Path<i64>) -> R {
    s.require("circle.content")?;
    sqlx::query("UPDATE announcements SET ends_at = now() WHERE id = $1").bind(id).execute(&st.pool).await?;
    audit::record(&st.pool, &s.actor(), "announcement.end", Some(format!("announcement:{id}")), None, None, None).await?;
    Ok(Json(json!({"status": "ended"})))
}

pub async fn topics(State(st): State<AppState>, s: Staff) -> R {
    s.require("circle.read")?;
    let rows = sqlx::query("SELECT * FROM topics ORDER BY position, key").fetch_all(&st.pool).await?;
    Ok(Json(json!({"items": rows.iter().map(|r| json!({"key": r.get::<String, _>("key"), "title": r.get::<String, _>("title"), "description": r.get::<String, _>("description"), "position": r.get::<i32, _>("position"), "active": r.get::<bool, _>("active")})).collect::<Vec<_>>()})))
}

pub async fn upsert_topic(State(st): State<AppState>, s: Staff, Body(b): Body) -> R {
    s.require("circle.content")?;
    let key = b["key"].as_str().unwrap_or("").trim().to_lowercase();
    if key.is_empty() || key.len() > 32 || !key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err(invalid("key", "key: lower-case letters, digits and dashes."));
    }
    let title = crate::util::clean(b["title"].as_str().unwrap_or(""), 60);
    if title.is_empty() {
        return Err(invalid("title", "Give the topic a title."));
    }
    sqlx::query(
        "INSERT INTO topics (key, title, description, position, active) VALUES ($1,$2,$3,$4,$5)
         ON CONFLICT (key) DO UPDATE SET title = EXCLUDED.title, description = EXCLUDED.description, position = EXCLUDED.position, active = EXCLUDED.active",
    )
    .bind(&key)
    .bind(&title)
    .bind(crate::util::clean(b["description"].as_str().unwrap_or(""), 200))
    .bind(b["position"].as_i64().unwrap_or(100) as i32)
    .bind(b["active"].as_bool().unwrap_or(true))
    .execute(&st.pool)
    .await?;
    audit::record(&st.pool, &s.actor(), "topic.upsert", Some(format!("topic:{key}")), None, Some(b.clone()), None).await?;
    Ok(Json(json!({"status": "ok", "key": key})))
}

pub async fn features(State(st): State<AppState>, s: Staff) -> R {
    s.require("circle.read")?;
    let rows = sqlx::query("SELECT * FROM features WHERE kind = 'profile' ORDER BY created_at DESC").fetch_all(&st.pool).await?;
    let ids: Vec<i64> = rows.iter().map(|r| r.get("target")).collect();
    let cards = profiles::cards(&st, &ids).await?;
    Ok(Json(json!({"items": rows.iter().map(|r| json!({"profile": cards.get(&r.get::<i64, _>("target")), "until": r.get::<Option<DateTime<Utc>>, _>("until"), "createdBy": r.get::<String, _>("created_by")})).collect::<Vec<_>>()})))
}

pub async fn feature_profile(State(st): State<AppState>, s: Staff, Body(b): Body) -> R {
    s.require("circle.content")?;
    let user = b["userId"].as_i64().ok_or_else(|| invalid("userId", "Choose the member."))?;
    profiles::by_id(&st, user).await?.ok_or(ApiError::NotFound)?;
    let until = b["days"].as_i64().map(|d| Utc::now() + chrono::Duration::days(d.clamp(1, 365)));
    sqlx::query("INSERT INTO features (kind, target, until, created_by) VALUES ('profile',$1,$2,$3) ON CONFLICT (kind, target) DO UPDATE SET until = EXCLUDED.until, created_by = EXCLUDED.created_by")
        .bind(user)
        .bind(until)
        .bind(format!("staff:{}", s.id))
        .execute(&st.pool)
        .await?;
    audit::record(&st.pool, &s.actor(), "profile.feature", Some(format!("user:{user}")), None, Some(json!({"until": until})), None).await?;
    Ok(Json(json!({"status": "ok"})))
}

pub async fn unfeature_profile(State(st): State<AppState>, s: Staff, Path(user): Path<i64>) -> R {
    s.require("circle.content")?;
    sqlx::query("DELETE FROM features WHERE kind = 'profile' AND target = $1").bind(user).execute(&st.pool).await?;
    audit::record(&st.pool, &s.actor(), "profile.unfeature", Some(format!("user:{user}")), None, None, None).await?;
    Ok(Json(json!({"status": "ok"})))
}

/// Top creators of the last 30 days and the fee-discount flags.
pub async fn creators(State(st): State<AppState>, s: Staff) -> R {
    s.require("circle.read")?;
    let ids: Vec<i64> = sqlx::query_scalar(crate::rewards::TOP_CREATORS).bind(100i64).fetch_all(&st.pool).await?;
    let ps = profiles::many(&st, &ids).await?;
    let items: Vec<Value> = ids.iter().enumerate().filter_map(|(i, id)| ps.get(id).filter(|p| s.in_scope(&p.tenant)).map(|p| {
        let mut v = admin_user_json(&st, p);
        v["rank"] = json!(i + 1);
        v
    })).collect();
    let set = profiles::settings(&st).await;
    Ok(Json(json!({"items": items, "feeDiscountTop": set.fee_discount_top})))
}

pub async fn fee_discounts(State(st): State<AppState>) -> R {
    let rows = sqlx::query("SELECT user_id, tenant FROM profiles WHERE fee_discount AND status = 'active' ORDER BY user_id").fetch_all(&st.pool).await?;
    Ok(Json(json!({"items": rows.iter().map(|r| json!({"userId": r.get::<i64, _>("user_id"), "tenant": r.get::<String, _>("tenant")})).collect::<Vec<_>>()})))
}

pub async fn settings(State(st): State<AppState>, s: Staff) -> R {
    s.require("circle.read")?;
    let set = crate::db::settings(&st.pool).await?;
    Ok(Json(json!({"settings": set, "env": {"restrictedCountries": st.cfg.restricted_countries, "maxVideoSecs": st.cfg.max_video_secs, "maxTopicVideoSecs": st.cfg.max_topic_video_secs, "moderationFallback": st.cfg.moderation_fallback, "aiModel": st.cfg.ai_model, "moderationModel": st.cfg.moderation_model}})))
}

pub async fn put_settings(State(st): State<AppState>, s: Staff, Body(b): Body) -> R {
    s.require("circle.admin")?;
    let before = crate::db::settings(&st.pool).await?;
    let mut merged = serde_json::to_value(&before)?;
    if let (Some(m), Some(p)) = (merged.as_object_mut(), b.as_object()) {
        for (k, v) in p {
            if m.contains_key(k) {
                m.insert(k.clone(), v.clone());
            }
        }
    }
    let mut set: crate::db::Settings = serde_json::from_value(merged).map_err(|e| invalid("settings", format!("Invalid settings: {e}")))?;
    set.restricted_countries = crate::config::parse_countries(&set.restricted_countries.join(","));
    set.helpful_threshold = set.helpful_threshold.clamp(1, 100_000);
    set.helpful_points = set.helpful_points.clamp(0, 100_000);
    set.helpful_daily_cap = set.helpful_daily_cap.clamp(0, 100);
    set.posts_per_hour = set.posts_per_hour.clamp(1, 1000);
    set.messages_per_minute = set.messages_per_minute.clamp(1, 600);
    set.max_group_members = set.max_group_members.clamp(2, 100);
    set.dm_requests_per_day = set.dm_requests_per_day.clamp(1, 1000);
    set.verified_boost = set.verified_boost.clamp(1.0, 3.0);
    set.rules_text = crate::util::clean(&set.rules_text, 8000);
    crate::db::save_settings(&st.pool, &set, &format!("staff:{}", s.id)).await?;
    st.cache.put("settings", serde_json::to_value(&set)?);
    audit::record(&st.pool, &s.actor(), "settings.update", Some("settings:general".into()), Some(serde_json::to_value(&before)?), Some(serde_json::to_value(&set)?), None).await?;
    Ok(Json(json!({"settings": set})))
}

// ---------------------------------------------------------------- chat access (reports / legal requests)

pub async fn chat_grants(State(st): State<AppState>, s: Staff) -> R {
    s.require("circle.chat_access")?;
    let rows = sqlx::query("SELECT * FROM chat_access WHERE ($1 OR staff_tenant = $2) ORDER BY id DESC LIMIT 200").bind(s.global).bind(&s.tenant).fetch_all(&st.pool).await?;
    Ok(Json(json!({"items": rows.iter().map(grant_json).collect::<Vec<_>>()})))
}

fn grant_json(r: &PgRow) -> Value {
    json!({
        "id": r.get::<i64, _>("id"), "conversationId": r.get::<i64, _>("conversation_id"), "staffId": r.get::<String, _>("staff_id"), "staffName": r.get::<String, _>("staff_name"),
        "basis": r.get::<String, _>("basis"), "reportId": r.get::<Option<i64>, _>("report_id"), "legalReference": r.get::<Option<String>, _>("legal_ref"), "reason": r.get::<String, _>("reason"),
        "expiresAt": r.get::<DateTime<Utc>, _>("expires_at"), "revokedAt": r.get::<Option<DateTime<Utc>>, _>("revoked_at"), "createdAt": r.get::<DateTime<Utc>, _>("created_at"),
    })
}

/// Opens a conversation for one staff member: basis `report` (an open or recent report on the conversation or one
/// of its messages) or `legal` (a legal reference; `circle.admin` too). 24 hours by default (max 72).
pub async fn grant_chat_access(State(st): State<AppState>, s: Staff, Body(b): Body) -> R {
    s.require("circle.chat_access")?;
    let conv = b["conversationId"].as_i64().ok_or_else(|| invalid("conversationId", "Which conversation?"))?;
    let c = chat::conv(&st, conv).await?;
    let reason = crate::util::clean(b["reason"].as_str().unwrap_or(""), 500);
    if reason.len() < 5 {
        return Err(invalid("reason", "Explain why the conversation must be read."));
    }
    // scope: a member of the conversation must be a client of the staff member's broker
    if !s.global {
        let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM conv_members m JOIN profiles p ON p.user_id = m.user_id WHERE m.conversation_id = $1 AND p.tenant = $2)").bind(conv).bind(&s.tenant).fetch_one(&st.pool).await?;
        if !ok {
            return Err(ApiError::NotFound);
        }
    }
    let (basis, report_id, legal_ref) = match b["basis"].as_str() {
        Some("report") => {
            let rid = b["reportId"].as_i64().ok_or_else(|| invalid("reportId", "Give the report."))?;
            let ok: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM reports r WHERE r.id = $1 AND r.created_at > now() - interval '90 days' AND (
                    (r.target_kind = 'conversation' AND r.target_id = $2) OR (r.target_kind = 'message' AND r.target_id IN (SELECT id FROM messages WHERE conversation_id = $2))))",
            )
            .bind(rid)
            .bind(conv)
            .fetch_one(&st.pool)
            .await?;
            if !ok {
                return Err(invalid("reportId", "This report isn't about this conversation."));
            }
            ("report", Some(rid), None)
        }
        Some("legal") => {
            s.require("circle.admin")?;
            let r = crate::util::clean(b["legalReference"].as_str().unwrap_or(""), 200);
            if r.len() < 3 {
                return Err(invalid("legalReference", "Give the legal request's reference."));
            }
            ("legal", None, Some(r))
        }
        _ => return Err(invalid("basis", "basis is report or legal.")),
    };
    let hours = b["hours"].as_i64().unwrap_or(24).clamp(1, 72);
    let r = sqlx::query(
        "INSERT INTO chat_access (conversation_id, staff_id, staff_name, staff_tenant, basis, report_id, legal_ref, reason, expires_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8, now() + make_interval(hours => $9)) RETURNING *",
    )
    .bind(conv)
    .bind(&s.id)
    .bind(&s.name)
    .bind(&s.tenant)
    .bind(basis)
    .bind(report_id)
    .bind(&legal_ref)
    .bind(&reason)
    .bind(hours as i32)
    .fetch_one(&st.pool)
    .await?;
    audit::record(&st.pool, &s.actor(), "chat.access_granted", Some(format!("conversation:{conv}")), None, Some(grant_json(&r)), Some(&reason)).await?;
    let _ = c;
    Ok(Json(json!({"grant": grant_json(&r)})))
}

pub async fn revoke_chat_access(State(st): State<AppState>, s: Staff, Path(id): Path<i64>) -> R {
    s.require("circle.chat_access")?;
    let r = sqlx::query("UPDATE chat_access SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL AND ($2 OR staff_id = $3) RETURNING *").bind(id).bind(s.can("circle.admin")).bind(&s.id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    audit::record(&st.pool, &s.actor(), "chat.access_revoked", Some(format!("conversation:{}", r.get::<i64, _>("conversation_id"))), None, Some(grant_json(&r)), None).await?;
    Ok(Json(json!({"grant": grant_json(&r)})))
}

#[derive(Deserialize)]
pub struct MsgQ {
    before: Option<i64>,
    limit: Option<i64>,
}

/// Reads a conversation under an active grant of this staff member; every read is audited.
pub async fn conversation_messages(State(st): State<AppState>, s: Staff, Path(id): Path<i64>, Query(q): Query<MsgQ>) -> R {
    s.require("circle.chat_access")?;
    let g = sqlx::query("SELECT * FROM chat_access WHERE conversation_id = $1 AND staff_id = $2 AND staff_tenant = $3 AND revoked_at IS NULL AND expires_at > now() ORDER BY id DESC LIMIT 1")
        .bind(id)
        .bind(&s.id)
        .bind(&s.tenant)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| ApiError::Forbidden("Open this conversation through chat access first (a report or a legal request).".into()))?;
    let limit = crate::util::clamp_limit(q.limit, 50, 200);
    let rows = sqlx::query("SELECT * FROM messages WHERE conversation_id = $1 AND id < $2 ORDER BY id DESC LIMIT $3").bind(id).bind(q.before.unwrap_or(i64::MAX)).bind(limit).fetch_all(&st.pool).await?;
    let senders: Vec<i64> = rows.iter().map(|r| r.get("sender")).collect();
    let cards = profiles::cards(&st, &senders).await?;
    let media_ids: Vec<i64> = rows.iter().filter_map(|r| r.get::<Option<i64>, _>("media")).collect();
    let mediam = media::many(&st, &media_ids, -1).await?;
    // compliance sees everything that was stored, including hidden and deleted messages
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<i64, _>("id"), "sender": cards.get(&r.get::<i64, _>("sender")), "kind": r.get::<String, _>("kind"), "body": r.get::<String, _>("body"),
                "media": r.get::<Option<i64>, _>("media").and_then(|m| mediam.get(&m).cloned()), "status": r.get::<String, _>("status"),
                "moderation": r.get::<Option<sqlx::types::Json<Value>>, _>("moderation").map(|j| j.0), "createdAt": r.get::<DateTime<Utc>, _>("created_at"),
                "editedAt": r.get::<Option<DateTime<Utc>>, _>("edited_at"), "deletedAt": r.get::<Option<DateTime<Utc>>, _>("deleted_at"),
            })
        })
        .collect();
    let c = chat::conv(&st, id).await?;
    let members: Vec<i64> = sqlx::query_scalar("SELECT user_id FROM conv_members WHERE conversation_id = $1").bind(id).fetch_all(&st.pool).await?;
    let mcards = profiles::cards(&st, &members).await?;
    audit::record(&st.pool, &s.actor(), "chat.read", Some(format!("conversation:{id}")), None, Some(json!({"grant": g.get::<i64, _>("id"), "messages": items.len(), "before": q.before})), None).await?;
    Ok(Json(json!({
        "conversation": {"id": id, "kind": c.get::<String, _>("kind"), "title": c.get::<String, _>("title"), "members": mcards.values().collect::<Vec<_>>()},
        "grant": grant_json(&g),
        "items": items,
    })))
}

#[derive(Deserialize)]
pub struct AuditQ {
    action: Option<String>,
    cursor: Option<String>,
    limit: Option<i64>,
}

pub async fn audit(State(st): State<AppState>, s: Staff, Query(q): Query<AuditQ>) -> R {
    s.require("circle.admin")?;
    let limit = crate::util::clamp_limit(q.limit, 50, 200);
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT * FROM audit_log WHERE ($1::text IS NULL OR action LIKE $1 || '%') AND id < $2 {} ORDER BY id DESC LIMIT $3",
        scope_sql(&s, "actor_tenant", 4)
    )))
    .bind(&q.action)
    .bind(q.cursor.as_deref().and_then(|c| c.parse::<i64>().ok()).unwrap_or(i64::MAX))
    .bind(limit + 1)
    .bind(scope_tenant(&s))
    .fetch_all(&st.pool)
    .await?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<i64, _>("id"), "actor": r.get::<String, _>("actor"), "actorName": r.get::<Option<String>, _>("actor_name"), "action": r.get::<String, _>("action"),
                "target": r.get::<Option<String>, _>("target"), "before": r.get::<Option<sqlx::types::Json<Value>>, _>("before").map(|j| j.0),
                "after": r.get::<Option<sqlx::types::Json<Value>>, _>("after").map(|j| j.0), "note": r.get::<Option<String>, _>("note"), "at": r.get::<DateTime<Utc>, _>("created_at"),
            })
        })
        .collect();
    Ok(Json(paged(items, limit, |v| v["id"].as_i64().map(|i| i.to_string()))))
}

pub async fn outbox(State(st): State<AppState>, s: Staff, Query(page): Query<Page>) -> R {
    s.require("circle.admin")?;
    let limit = page.limit(50, 200);
    let rows = sqlx::query("SELECT * FROM outbox WHERE id < $1 ORDER BY id DESC LIMIT $2").bind(page.before_id()).bind(limit + 1).fetch_all(&st.pool).await?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<i64, _>("id"), "kind": r.get::<String, _>("kind"), "userId": r.get::<i64, _>("user_id"), "tenant": r.get::<String, _>("tenant"),
                "payload": r.get::<sqlx::types::Json<Value>, _>("payload").0, "status": r.get::<String, _>("status"), "attempts": r.get::<i32, _>("attempts"),
                "error": r.get::<Option<String>, _>("error"), "createdAt": r.get::<DateTime<Utc>, _>("created_at"), "deliveredAt": r.get::<Option<DateTime<Utc>>, _>("delivered_at"),
            })
        })
        .collect();
    Ok(Json(paged(items, limit, |v| v["id"].as_i64().map(|i| i.to_string()))))
}

pub async fn retry_outbox(State(st): State<AppState>, s: Staff, Path(id): Path<i64>) -> R {
    s.require("circle.admin")?;
    let n = sqlx::query("UPDATE outbox SET status = 'pending', next_at = now(), attempts = 0 WHERE id = $1 AND status = 'failed'").bind(id).execute(&st.pool).await?.rows_affected();
    if n == 0 {
        return Err(ApiError::NotFound);
    }
    st.wake.notify.notify_one();
    audit::record(&st.pool, &s.actor(), "outbox.retry", Some(format!("outbox:{id}")), None, None, None).await?;
    Ok(Json(json!({"status": "pending"})))
}
