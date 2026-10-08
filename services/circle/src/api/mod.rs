//! HTTP API on 127.0.0.1:8105 (docs/CIRCLE-API.md).
//!
//! - Client routes `/v1/circle/*` need `X-Kalks-Internal` plus the signed-in gateway user resolved by the Client
//!   Area BFF from the session (`X-Kalks-User-Id`, `X-Kalks-Tenant`, name, country, KYC, locale headers). They
//!   are switched off per broker by module `circle` (403 `module_disabled`).
//! - Back Office routes `/v1/circle/admin/*` take the staff headers the admin BFF verified with the gateway
//!   (`X-Kalks-Staff-Id/-Name/-Role/-Perms`), permissions `circle.read|moderate|content|admin|chat_access`.
//! - `GET /v1/stream?ticket=` (WebSocket) and `PUT|HEAD /v1/upload/{id}?token=` (resumable direct uploads) are
//!   reached through Caddy (`/circle/stream`, `/circle/upload/*`) and authenticate with one-time tokens.
//! - `GET /v1/public/*`: read-only public profile / post pages for the website (no token, public content only).

pub mod admin;
pub mod chat;
pub mod feed;
pub mod media;
pub mod posts;
pub mod profiles;
pub mod public;
pub mod stories;
pub mod stream;

use crate::error::{ApiError, ApiResult, denied};
use crate::profiles::Profile;
use crate::state::AppState;
use crate::util::percent_decode;
use axum::extract::{DefaultBodyLimit, FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post, put};
use axum::{Json, Router};
use serde_json::json;
use subtle::ConstantTimeEq;

pub fn router(st: AppState) -> Router {
    let chunk_limit = DefaultBodyLimit::max(st.cfg.chunk_bytes as usize + 64 * 1024);
    let client = Router::new()
        // me, profiles, graph
        .route("/v1/circle/me", get(profiles::me).patch(profiles::update_me))
        .route("/v1/circle/handles/check", get(profiles::check_handle))
        .route("/v1/circle/me/requests", get(profiles::requests))
        .route("/v1/circle/me/requests/{user}/{action}", post(profiles::answer_request))
        .route("/v1/circle/me/followers/{user}/remove", post(profiles::remove_follower))
        .route("/v1/circle/me/lists/{list}", get(profiles::my_list))
        .route("/v1/circle/me/hidden-words", get(profiles::hidden_words).put(profiles::put_hidden_words))
        .route("/v1/circle/me/notification-prefs", get(profiles::prefs).put(profiles::put_prefs))
        .route("/v1/circle/me/devices", post(profiles::register_device).delete(profiles::unregister_device))
        .route("/v1/circle/me/activity", get(profiles::activity))
        .route("/v1/circle/me/activity/read", post(profiles::activity_read))
        .route("/v1/circle/me/stats", get(profiles::my_stats))
        .route("/v1/circle/me/stats/refresh", post(profiles::refresh_stats))
        .route("/v1/circle/me/saved", get(posts::saved))
        .route("/v1/circle/me/collections", get(posts::collections).post(posts::create_collection))
        .route("/v1/circle/me/collections/{id}", patch(posts::rename_collection).delete(posts::delete_collection))
        .route("/v1/circle/me/stories/archive", get(stories::archive))
        .route("/v1/circle/profiles/{handle}", get(profiles::profile))
        .route("/v1/circle/profiles/{handle}/posts", get(feed::profile_posts))
        .route("/v1/circle/profiles/{handle}/followers", get(profiles::followers))
        .route("/v1/circle/profiles/{handle}/following", get(profiles::following))
        .route("/v1/circle/profiles/{handle}/highlights", get(stories::highlights_of))
        .route("/v1/circle/profiles/{handle}/{action}", post(profiles::relate).delete(profiles::unrelate))
        // posts, comments, reactions
        .route("/v1/circle/posts", post(posts::create))
        .route("/v1/circle/posts/views", post(posts::views))
        .route("/v1/circle/posts/{id}", get(posts::get).patch(posts::edit).delete(posts::remove))
        .route("/v1/circle/posts/{id}/react", post(posts::react).delete(posts::unreact))
        .route("/v1/circle/posts/{id}/reactions", get(posts::reactions))
        .route("/v1/circle/posts/{id}/repost", post(posts::repost).delete(posts::unrepost))
        .route("/v1/circle/posts/{id}/save", post(posts::save).delete(posts::unsave))
        .route("/v1/circle/posts/{id}/vote", post(posts::vote))
        .route("/v1/circle/posts/{id}/pin", post(posts::pin).delete(posts::unpin))
        .route("/v1/circle/posts/{id}/copy", get(posts::copy))
        .route("/v1/circle/posts/{id}/translate", post(posts::translate))
        .route("/v1/circle/posts/{id}/comments", get(posts::comments).post(posts::comment))
        .route("/v1/circle/posts/{id}/comments/{cid}/pin", post(posts::pin_comment).delete(posts::unpin_comment))
        .route("/v1/circle/comments/{id}", patch(posts::edit_comment).delete(posts::delete_comment))
        .route("/v1/circle/comments/{id}/like", post(posts::like_comment).delete(posts::unlike_comment))
        .route("/v1/circle/comments/{id}/translate", post(posts::translate_comment))
        // trade cards
        .route("/v1/circle/trade-cards/sources", get(posts::card_sources))
        .route("/v1/circle/trade-cards", post(posts::create_card))
        .route("/v1/circle/trade-cards/{id}", get(posts::get_card))
        .route("/v1/circle/trade-cards/{id}/live", get(posts::live_card))
        // feeds & discovery
        .route("/v1/circle/feed/following", get(feed::following))
        .route("/v1/circle/feed/for-you", get(feed::for_you))
        .route("/v1/circle/tags/{tag}", get(feed::hashtag))
        .route("/v1/circle/cashtags/{symbol}", get(feed::cashtag))
        .route("/v1/circle/explore", get(feed::explore))
        .route("/v1/circle/search", get(feed::search))
        .route("/v1/circle/topics", get(feed::topics))
        .route("/v1/circle/topics/{key}/videos", get(feed::topic_videos))
        .route("/v1/circle/leaderboards/{board}", get(feed::leaderboard))
        .route("/v1/circle/prices", get(feed::prices))
        .route("/v1/circle/announcements", get(feed::announcements))
        .route("/v1/circle/academy/{chapter}/thread", get(feed::chapter_thread))
        .route("/v1/circle/ai/sentiment/{symbol}", get(feed::sentiment))
        .route("/v1/circle/ai/digest", get(feed::digest))
        .route("/v1/circle/ai/caption", post(posts::caption))
        .route("/v1/circle/rules", get(feed::rules))
        // stories
        .route("/v1/circle/stories", post(stories::create))
        .route("/v1/circle/stories/tray", get(stories::tray))
        .route("/v1/circle/stories/users/{handle}", get(stories::of_user))
        .route("/v1/circle/stories/{id}", get(stories::get).delete(stories::remove))
        .route("/v1/circle/stories/{id}/view", post(stories::view))
        .route("/v1/circle/stories/{id}/viewers", get(stories::viewers))
        .route("/v1/circle/stories/{id}/vote", post(stories::vote))
        .route("/v1/circle/stories/{id}/answer", post(stories::answer))
        .route("/v1/circle/stories/{id}/answers", get(stories::answers))
        .route("/v1/circle/stories/{id}/reply", post(stories::reply))
        .route("/v1/circle/highlights", post(stories::create_highlight))
        .route("/v1/circle/highlights/{id}", get(stories::highlight).patch(stories::edit_highlight).delete(stories::delete_highlight))
        // chat
        .route("/v1/circle/chat/conversations", get(chat::list))
        .route("/v1/circle/chat/dm", post(chat::dm))
        .route("/v1/circle/chat/groups", post(chat::create_group))
        .route("/v1/circle/chat/rooms", get(chat::rooms))
        .route("/v1/circle/chat/master-room", post(chat::master_room))
        .route("/v1/circle/chat/conversations/{id}", get(chat::get).patch(chat::edit))
        .route("/v1/circle/chat/conversations/{id}/members", get(chat::members).post(chat::add_members))
        .route("/v1/circle/chat/conversations/{id}/members/{user}", delete(chat::remove_member))
        .route("/v1/circle/chat/conversations/{id}/admins/{user}", post(chat::make_admin))
        .route("/v1/circle/chat/conversations/{id}/{action}", post(chat::action))
        .route("/v1/circle/chat/conversations/{id}/messages", get(chat::messages).post(chat::send))
        .route("/v1/circle/chat/messages/{id}", patch(chat::edit_message).delete(chat::delete_message))
        // media uploads (small files in one chunk; video resumable)
        .route("/v1/circle/uploads", post(media::start))
        .route("/v1/circle/uploads/{id}", get(media::status).put(media::chunk).layer(chunk_limit.clone()))
        .route("/v1/circle/media/{id}", get(media::get))
        // safety
        .route("/v1/circle/reports", post(posts::report))
        .layer(middleware::from_fn_with_state(st.clone(), crate::modules::gate));
    let staff = Router::new()
        .route("/v1/circle/admin/overview", get(admin::overview))
        .route("/v1/circle/admin/queue", get(admin::queue))
        .route("/v1/circle/admin/queue/{id}", get(admin::queue_item))
        .route("/v1/circle/admin/queue/{id}/{decision}", post(admin::decide))
        .route("/v1/circle/admin/reports", get(admin::reports))
        .route("/v1/circle/admin/reports/{id}/resolve", post(admin::resolve_report))
        .route("/v1/circle/admin/content/{kind}/{id}", get(admin::content))
        .route("/v1/circle/admin/content/{kind}/{id}/{action}", post(admin::content_action))
        .route("/v1/circle/admin/users", get(admin::users))
        .route("/v1/circle/admin/users/{user}", get(admin::user))
        .route("/v1/circle/admin/users/{user}/{action}", post(admin::user_action))
        .route("/v1/circle/admin/rules", get(admin::rules).post(admin::create_rule))
        .route("/v1/circle/admin/rules/{id}", patch(admin::edit_rule).delete(admin::delete_rule))
        .route("/v1/circle/admin/announcements", get(admin::announcements).post(admin::create_announcement))
        .route("/v1/circle/admin/announcements/{id}", patch(admin::edit_announcement).delete(admin::delete_announcement))
        .route("/v1/circle/admin/topics", get(admin::topics).post(admin::upsert_topic))
        .route("/v1/circle/admin/features", get(admin::features).post(admin::feature_profile))
        .route("/v1/circle/admin/features/{user}", delete(admin::unfeature_profile))
        .route("/v1/circle/admin/creators", get(admin::creators))
        .route("/v1/circle/admin/settings", get(admin::settings).put(admin::put_settings))
        .route("/v1/circle/admin/chat-access", get(admin::chat_grants).post(admin::grant_chat_access))
        .route("/v1/circle/admin/chat-access/{id}/revoke", post(admin::revoke_chat_access))
        .route("/v1/circle/admin/conversations/{id}/messages", get(admin::conversation_messages))
        .route("/v1/circle/admin/audit", get(admin::audit))
        .route("/v1/circle/admin/outbox", get(admin::outbox))
        .route("/v1/circle/admin/outbox/{id}/retry", post(admin::retry_outbox));
    let internal = Router::new()
        .route("/v1/stream/ticket", post(stream::ticket))
        .route("/v1/internal/creators/fee-discounts", get(admin::fee_discounts))
        .merge(client)
        .merge(staff)
        .layer(DefaultBodyLimit::max(256 * 1024))
        .layer(middleware::from_fn_with_state(st.clone(), internal_only));
    Router::new()
        .route("/health", get(health))
        .route("/v1/stream", get(stream::stream))
        .route("/v1/upload/{id}", put(media::direct_chunk).head(media::direct_head).layer(chunk_limit))
        .route("/v1/public/profiles/{handle}", get(public::profile))
        .route("/v1/public/profiles/{handle}/posts", get(public::posts))
        .route("/v1/public/posts/{id}", get(public::post))
        .merge(internal)
        .fallback(|| async { (axum::http::StatusCode::NOT_FOUND, Json(json!({"error": {"code": "not_found", "message": "Not found."}}))) })
        .with_state(st)
}

async fn health(State(st): State<AppState>) -> impl IntoResponse {
    let db = sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&st.pool).await.is_ok();
    Json(json!({
        "status": if db { "ok" } else { "degraded" },
        "db": db,
        "service": "circle",
        "ai": st.ai(),
        "storage": st.storage.kind(),
        "ffmpeg": crate::video::available(&st.cfg).await,
        "push": crate::push::configured(&st.cfg),
    }))
}

async fn internal_only(State(st): State<AppState>, req: Request, next: Next) -> Response {
    let expected = st.cfg.internal_token.as_bytes();
    if !expected.is_empty() {
        let got = req.headers().get("x-kalks-internal").map(|v| v.as_bytes()).unwrap_or_default();
        if got.len() != expected.len() || !bool::from(got.ct_eq(expected)) {
            return ApiError::Forbidden("Missing or wrong internal token.".into()).into_response();
        }
    }
    next.run(req).await
}

pub fn header(parts: &Parts, name: &str) -> Option<String> {
    parts.headers.get(name).and_then(|v| v.to_str().ok()).map(str::trim).filter(|v| !v.is_empty()).map(str::to_string)
}

pub fn tenant_of(parts: &Parts) -> ApiResult<String> {
    let t = header(parts, "x-kalks-tenant").unwrap_or_else(|| "kalks".into()).to_lowercase();
    if t.len() > 40 || !t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(ApiError::BadRequest("Invalid tenant.".into()));
    }
    Ok(t)
}

fn country(v: Option<String>) -> Option<String> {
    v.map(|c| c.trim().to_ascii_lowercase()).filter(|c| c.len() == 2 && c.chars().all(|x| x.is_ascii_alphabetic()) && c != "xx")
}

/// The signed-in client as the BFF forwarded it (no database access).
#[derive(Clone, Debug)]
pub struct Client {
    pub id: i64,
    pub tenant: String,
    pub name: String,
    /// Registered country (gateway profile).
    pub country: Option<String>,
    /// Country of the request IP at the edge, when the BFF knows it.
    pub ip_country: Option<String>,
    pub kyc: Option<String>,
    pub locale: Option<String>,
    pub referral: Option<String>,
}

impl<S: Send + Sync> FromRequestParts<S> for Client {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let id = header(parts, "x-kalks-user-id").and_then(|v| v.parse::<i64>().ok()).filter(|v| *v > 0).ok_or(ApiError::Unauthorized)?;
        let name = header(parts, "x-kalks-user-name").or_else(|| header(parts, "x-kalks-name")).map(|n| percent_decode(&n)).unwrap_or_default();
        let kyc = header(parts, "x-kalks-kyc").map(|k| k.to_lowercase()).filter(|k| ["unverified", "pending", "verified", "rejected"].contains(&k.as_str()));
        let referral = header(parts, "x-kalks-referral-code").filter(|r| r.len() <= 32 && r.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        Ok(Client {
            id,
            tenant: tenant_of(parts)?,
            name: crate::util::clean(&name, 60),
            country: country(header(parts, "x-kalks-country")),
            ip_country: country(header(parts, "x-kalks-ip-country").or_else(|| header(parts, "cf-ipcountry"))),
            kyc,
            locale: header(parts, "x-kalks-locale").and_then(|l| crate::text::valid_lang(&l)).map(str::to_string),
            referral,
        })
    }
}

/// The signed-in client with their Circle profile (created on first use). Refused for restricted countries
/// (403 `restricted_country`) and banned members (403 `banned`, except `GET /v1/circle/me`).
pub struct Me {
    pub client: Client,
    pub p: Profile,
}

impl Me {
    pub fn id(&self) -> i64 {
        self.p.user_id
    }
}

impl FromRequestParts<AppState> for Me {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, st: &AppState) -> Result<Self, Self::Rejection> {
        let client = Client::from_request_parts(parts, st).await?;
        crate::profiles::check_country(st, &client).await?;
        let p = crate::profiles::ensure(st, &client).await?;
        if p.is_banned() && !(parts.method == axum::http::Method::GET && parts.uri.path() == "/v1/circle/me") {
            return Err(denied("banned", "Your Kalks Circle access is suspended."));
        }
        Ok(Me { client, p })
    }
}

/// Back Office staff, as verified by the admin BFF, plus the permissions it resolved (`X-Kalks-Staff-Perms`).
#[derive(Clone, Debug)]
pub struct Staff {
    pub id: String,
    pub name: String,
    pub role: String,
    pub tenant: String,
    perms: Option<Vec<String>>,
    /// Staff of the platform broker (or the Platform Owner) moderate the whole community; other brokers' staff
    /// see and act on their own clients only.
    pub global: bool,
}

pub const CIRCLE_READ: &[&str] = &["platform_owner", "super_admin", "admin", "compliance", "support", "marketing", "viewer"];
pub const CIRCLE_MODERATE: &[&str] = &["platform_owner", "super_admin", "admin", "compliance", "support"];
pub const CIRCLE_CONTENT: &[&str] = &["platform_owner", "super_admin", "admin", "marketing"];
pub const CIRCLE_ADMIN: &[&str] = &["platform_owner", "super_admin", "admin"];
pub const CIRCLE_CHAT_ACCESS: &[&str] = &["platform_owner", "super_admin", "admin", "compliance"];

impl Staff {
    /// `perm` = `circle.read`, `circle.moderate`, `circle.content`, `circle.admin` or `circle.chat_access`.
    pub fn require(&self, perm: &str) -> ApiResult<()> {
        let ok = match &self.perms {
            Some(p) => p.iter().any(|x| x == perm),
            None => match perm {
                "circle.read" => CIRCLE_READ.contains(&self.role.as_str()),
                "circle.moderate" => CIRCLE_MODERATE.contains(&self.role.as_str()),
                "circle.content" => CIRCLE_CONTENT.contains(&self.role.as_str()),
                "circle.admin" => CIRCLE_ADMIN.contains(&self.role.as_str()),
                "circle.chat_access" => CIRCLE_CHAT_ACCESS.contains(&self.role.as_str()),
                _ => false,
            },
        };
        if ok { Ok(()) } else { Err(ApiError::Forbidden("Your role doesn't allow this.".into())) }
    }

    pub fn can(&self, perm: &str) -> bool {
        self.require(perm).is_ok()
    }

    /// Whether this staff member may act on content / members of broker `tenant`.
    pub fn in_scope(&self, tenant: &str) -> bool {
        self.global || self.tenant == tenant
    }

    pub fn scope(&self, tenant: &str) -> ApiResult<()> {
        if self.in_scope(tenant) { Ok(()) } else { Err(ApiError::NotFound) }
    }

    pub fn actor(&self) -> crate::audit::Actor {
        crate::audit::Actor { id: format!("staff:{}", self.id), name: Some(self.name.clone()), tenant: Some(self.tenant.clone()) }
    }
}

impl FromRequestParts<AppState> for Staff {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, st: &AppState) -> Result<Self, Self::Rejection> {
        let id = header(parts, "x-kalks-staff-id").ok_or(ApiError::Unauthorized)?;
        let role = header(parts, "x-kalks-staff-role").ok_or(ApiError::Unauthorized)?;
        if id.len() > 64 || role.len() > 32 {
            return Err(ApiError::BadRequest("Invalid staff headers.".into()));
        }
        let name = header(parts, "x-kalks-staff-name").map(|n| percent_decode(&n)).unwrap_or_else(|| format!("Staff {id}"));
        let perms = header(parts, "x-kalks-staff-perms").map(|p| p.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect());
        let tenant = tenant_of(parts)?;
        let global = role == "platform_owner" || tenant == st.cfg.platform_tenant;
        Ok(Staff { id, name: name.chars().take(80).collect(), role, tenant, perms, global })
    }
}

/// JSON body as a `Value` with a JSON error on malformed input.
pub struct Body(pub serde_json::Value);

impl<S: Send + Sync> axum::extract::FromRequest<S> for Body {
    type Rejection = ApiError;
    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = axum::body::Bytes::from_request(req, state).await.map_err(|_| ApiError::TooLarge("Request body too large or unreadable.".into()))?;
        if bytes.is_empty() {
            return Ok(Body(json!({})));
        }
        let v: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| ApiError::BadRequest("Invalid JSON.".into()))?;
        if !v.is_object() {
            return Err(ApiError::BadRequest("Expected a JSON object.".into()));
        }
        Ok(Body(v))
    }
}

/// Keyset pagination: `?cursor=<opaque>&limit=`.
#[derive(serde::Deserialize, Default, Clone)]
pub struct Page {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

impl Page {
    pub fn limit(&self, default: i64, max: i64) -> i64 {
        crate::util::clamp_limit(self.limit, default, max)
    }
    /// Cursor as an id (most lists page by descending id).
    pub fn before_id(&self) -> i64 {
        self.cursor.as_deref().and_then(|c| c.parse::<i64>().ok()).filter(|v| *v > 0).unwrap_or(i64::MAX)
    }
}

/// `{"items": [...], "nextCursor": ...}` for a page fetched with `limit + 1` rows.
pub fn paged(mut items: Vec<serde_json::Value>, limit: i64, cursor_of: impl Fn(&serde_json::Value) -> Option<String>) -> serde_json::Value {
    let more = items.len() as i64 > limit;
    items.truncate(limit as usize);
    let next = if more { items.last().and_then(&cursor_of) } else { None };
    json!({"items": items, "nextCursor": next})
}
