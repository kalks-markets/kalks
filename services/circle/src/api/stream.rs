//! Realtime WebSocket stream for Circle (chat, activity, upload / post status) and the Back Office queue.
//!
//! 1. The BFF calls `POST /v1/stream/ticket` with the client (`X-Kalks-User-Id` …) or staff identity headers and
//!    gets a one-time ticket (30 s).
//! 2. The browser / app opens `wss://<app host>/circle/stream?ticket=…` (Caddy → `GET /v1/stream`).
//!
//! Server → client frames (JSON, `type`): `hello`, `message`, `message.updated`, `message.deleted`,
//! `message.hidden`, `typing`, `read`, `request.accepted`, `conversation`, `conversation.joined`,
//! `conversation.left`, `activity`, `activity.read`, `media`, `post.status`, `comment.status`, `story.status`,
//! `ping` (25 s), `resync`; staff sockets: `queue`, `report`.
//! Client → server: `{"type":"ping"}`, `{"type":"typing","conversationId"}`,
//! `{"type":"read","conversationId","messageId"}` (same as the HTTP routes).

use super::{Client, Staff};
use crate::error::{ApiError, ApiResult, denied};
use crate::state::{AppState, Target, Who};
use axum::Json;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{FromRequestParts, Query, State};
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::time::Duration;

pub enum Caller {
    User(Box<super::Me>),
    Staff(Staff),
}

impl FromRequestParts<AppState> for Caller {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, st: &AppState) -> Result<Self, Self::Rejection> {
        if parts.headers.contains_key("x-kalks-staff-id") {
            Ok(Caller::Staff(Staff::from_request_parts(parts, st).await?))
        } else {
            let c = Client::from_request_parts(parts, st).await?;
            if !crate::modules::on(st, &c.tenant, crate::modules::KEY).await {
                return Err(denied("module_disabled", "Kalks Circle isn't available on your account."));
            }
            Ok(Caller::User(Box::new(super::Me::from_request_parts(parts, st).await?)))
        }
    }
}

pub async fn ticket(State(st): State<AppState>, caller: Caller) -> ApiResult<Json<Value>> {
    let who = match caller {
        Caller::User(me) => Who::User { id: me.id(), tenant: me.p.tenant.clone() },
        Caller::Staff(s) => {
            s.require("circle.read")?;
            Who::Staff { id: s.id.clone(), tenant: s.tenant.clone(), global: s.global }
        }
    };
    Ok(Json(json!({"ticket": st.hub.issue(who), "expiresIn": crate::state::TICKET_TTL.as_secs()})))
}

#[derive(Deserialize)]
pub struct TicketQ {
    ticket: Option<String>,
}

pub async fn stream(State(st): State<AppState>, Query(q): Query<TicketQ>, ws: WebSocketUpgrade) -> Response {
    let Some(who) = q.ticket.as_deref().and_then(|t| st.hub.redeem(t)) else {
        return (axum::http::StatusCode::UNAUTHORIZED, Json(json!({"error": {"code": "unauthorized", "message": "Invalid or expired stream ticket."}}))).into_response();
    };
    ws.max_message_size(16 * 1024).on_upgrade(move |socket| run(st, who, socket))
}

fn wants(who: &Who, convs: &HashSet<i64>, ev: &crate::state::Event) -> bool {
    match (who, &ev.to) {
        (Who::User { id, .. }, Target::User(u)) => id == u,
        (Who::User { id, .. }, Target::Users(list)) => list.contains(id),
        (Who::User { id, .. }, Target::Conv(c)) => convs.contains(c) && ev.except != Some(*id),
        (Who::Staff { .. }, Target::Staff) => true,
        _ => false,
    }
}

async fn run(st: AppState, who: Who, mut socket: WebSocket) {
    let mut rx = st.hub.subscribe();
    let mut convs: HashSet<i64> = HashSet::new();
    let hello = match &who {
        Who::User { id, .. } => {
            st.hub.connected(*id, 1);
            convs = crate::chat::member_ids(&st, *id).await.unwrap_or_default().into_iter().collect();
            json!({"type": "hello", "who": "user", "activityUnread": crate::notify::unread(&st, *id).await.unwrap_or(0), "conversations": convs.len()})
        }
        Who::Staff { .. } => {
            let open: i64 = sqlx::query_scalar("SELECT count(*) FROM mod_queue WHERE status = 'open'").fetch_one(&st.pool).await.unwrap_or(0);
            json!({"type": "hello", "who": "staff", "queue": open})
        }
    };
    let me = match &who {
        Who::User { id, .. } => crate::profiles::by_id(&st, *id).await.ok().flatten(),
        _ => None,
    };
    if socket.send(Message::Text(hello.to_string().into())).await.is_ok() {
        let mut ping = tokio::time::interval(Duration::from_secs(25));
        ping.tick().await;
        loop {
            tokio::select! {
                ev = rx.recv() => match ev {
                    Ok(ev) => {
                        if !wants(&who, &convs, &ev) {
                            continue;
                        }
                        if let (Who::User { id, .. }, Target::User(u)) = (&who, &ev.to)
                            && id == u
                            && let Some(c) = ev.payload["conversationId"].as_i64()
                        {
                            match ev.payload["type"].as_str() {
                                Some("conversation.joined") => { convs.insert(c); }
                                Some("conversation.left") => { convs.remove(&c); }
                                _ => {}
                            }
                        }
                        if socket.send(Message::Text(ev.payload.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        if socket.send(Message::Text(json!({"type": "resync", "missed": n}).to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                },
                msg = socket.recv() => match msg {
                    Some(Ok(Message::Text(t))) => {
                        if let (Some(p), Ok(v)) = (&me, serde_json::from_str::<Value>(&t)) {
                            let conv = v["conversationId"].as_i64().filter(|c| convs.contains(c));
                            match (v["type"].as_str(), conv) {
                                (Some("typing"), Some(c)) => {
                                    if st.limiter.hit(&format!("typing:{}:{c}", p.user_id), 1, Duration::from_secs(2)) {
                                        st.hub.send_except(Target::Conv(c), json!({"type": "typing", "conversationId": c, "user": {"id": p.user_id, "handle": p.handle, "displayName": p.display_name}}), p.user_id);
                                    }
                                }
                                (Some("read"), Some(c)) => {
                                    let mid = v["messageId"].as_i64().unwrap_or(0);
                                    let _ = sqlx::query("UPDATE conv_members SET last_read_id = GREATEST(last_read_id, $3), unread = 0 WHERE conversation_id = $1 AND user_id = $2").bind(c).bind(p.user_id).bind(mid).execute(&st.pool).await;
                                    st.hub.send(Target::Conv(c), json!({"type": "read", "conversationId": c, "userId": p.user_id, "messageId": mid}));
                                }
                                _ => {}
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    _ => {}
                },
                _ = ping.tick() => {
                    if socket.send(Message::Text(json!({"type": "ping"}).to_string().into())).await.is_err() {
                        break;
                    }
                }
            }
        }
    }
    if let Who::User { id, .. } = &who {
        st.hub.connected(*id, -1);
    }
}
