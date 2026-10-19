//! HTTP API. `/health` and `/v1/public/*` are open (the edge exposes them read-only on api.kalkstrade.com);
//! everything else needs `X-Kalks-Internal`.
//!
//! Tenant: `X-Kalks-Tenant` (broker slug, default "kalks"); the public read also takes `?tenant=`.
//!
//! Public (no token):
//!
//! | route                                            | response                                                         |
//! |--------------------------------------------------|------------------------------------------------------------------|
//! | `GET /v1/public/content?surface&page&locale&preview` | `{tenant, surface, locale, preview, pages: {page: {slot: {kind, locale, value}}}}`; `page` is one page, a comma list (≤ 10) or absent (every page). Published values only, unless `preview` is a valid token (then drafts win). A locale without a value falls back to English. `Cache-Control: public, max-age=60` + `ETag` (304 on `If-None-Match`); previews are `no-store`. |
//! | `GET /v1/public/media/{file}/{variant}`           | the file (`orig`, `sm` 480, `md` 1024, `lg` 1920 px wide WebP); a missing variant serves the next larger one, then `orig`. Immutable cache headers (ids are content hashes). |
//!
//! Back Office (admin BFF checks `content.read` / `content.write` / `content.publish`, sends `X-Kalks-Staff`
//! (email) and `X-Kalks-Staff-Perms` (comma list; when present the service enforces it too)):
//!
//! | route                                       | body                                                  | perm     |
//! |---------------------------------------------|-------------------------------------------------------|----------|
//! | `GET    /v1/admin/content/meta`             | –  → `{surfaces, locales, kinds}`                     | read     |
//! | `GET    /v1/admin/content/tree?surface`     | –  → `{pages: [{surface, page, slots: [...]}]}`       | read     |
//! | `GET    /v1/admin/content/slot?surface&page&slot` | – → `{slot, entries: {locale: …}, media: {id: asset}}` | read |
//! | `PUT    /v1/admin/content/entry`            | `{surface, page, slot, kind, label?, locale, value}` (draft) | write |
//! | `POST   /v1/admin/content/discard`          | `{surface, page, slot, locale}`                       | write    |
//! | `POST   /v1/admin/content/publish`          | `{surface, page, slot?, locale?}` (every draft in scope) | publish |
//! | `POST   /v1/admin/content/unpublish`        | `{surface, page, slot, locale}`                       | publish  |
//! | `GET    /v1/admin/content/versions?surface&page&slot&locale` | – → `{versions}` (newest first, last 50) | read |
//! | `POST   /v1/admin/content/rollback`         | `{surface, page, slot, locale, version}`              | publish  |
//! | `DELETE /v1/admin/content/slot?surface&page&slot` | – (the slot, every locale and its versions)     | publish  |
//! | `POST   /v1/admin/content/import`           | `{surface, publish?, entries: [{page, slot, kind, label?, locale, value}]}` (≤ 500) | write (+ publish) |
//! | `POST   /v1/admin/content/preview-token`    | `{surface}` → `{token, expires_at}` (30 min)          | read     |
//! | `GET    /v1/admin/content/audit`            | – → `{audit}` (last 100)                              | read     |
//! | `GET    /v1/admin/media?q&limit&offset`     | – → `{assets, total}`                                 | read     |
//! | `POST   /v1/admin/media?name`               | raw image, `X-File-Name` → `{asset, existing}`        | write    |
//! | `GET    /v1/admin/media/{id}`               | – → `{asset, usage}`                                  | read     |
//! | `PUT    /v1/admin/media/{id}`               | `{name?, alt?: {locale: text}}`                       | write    |
//! | `POST   /v1/admin/media/{id}/file?theme=light|dark` | raw image: replaces the light file or sets the dark one | write (publish when live) |
//! | `DELETE /v1/admin/media/{id}/file?theme=dark` | – (removes the dark file)                           | write (publish when live) |
//! | `DELETE /v1/admin/media/{id}`               | – (refused with 409 while used)                       | publish  |
//!
//! Publish, unpublish, rollback, slot delete, import-with-publish and media replace / delete are written to
//! `content_audit`.

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::Arc;
use subtle::ConstantTimeEq;

use crate::config::Config;
use crate::storage::Storage;
use crate::{content, media};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub cfg: Arc<Config>,
    pub storage: Arc<dyn Storage>,
}

/* ------------------------------------------------------------------ */
/* Errors and request context                                          */
/* ------------------------------------------------------------------ */

pub struct ApiError(pub StatusCode, pub &'static str, pub String, pub Option<Value>);

impl ApiError {
    pub fn new(s: StatusCode, code: &'static str, msg: impl Into<String>) -> Self {
        Self(s, code, msg.into(), None)
    }
    pub fn not_found(what: &str) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", format!("{what} not found."))
    }
    pub fn bad(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, "validation", msg)
    }
    pub fn conflict(code: &'static str, msg: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, code, msg)
    }
    pub fn forbidden() -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", "Your role doesn't allow this.")
    }
    pub fn with(mut self, extra: Value) -> Self {
        self.3 = Some(extra);
        self
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        tracing::error!(error = %e, "content internal error");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", "Something went wrong. Please try again.")
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        anyhow::Error::from(e).into()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut err = json!({"code": self.1, "message": self.2});
        if let Some(extra) = self.3 {
            err["detail"] = extra;
        }
        (self.0, Json(json!({ "error": err }))).into_response()
    }
}

pub type R<T = Json<Value>> = Result<T, ApiError>;

pub fn is_slug(s: &str) -> bool {
    (1..=64).contains(&s.len()) && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

pub fn tenant(h: &HeaderMap) -> String {
    let t = h.get("x-kalks-tenant").and_then(|v| v.to_str().ok()).unwrap_or("kalks").trim().to_ascii_lowercase();
    if is_slug(&t) { t } else { "kalks".into() }
}

/// The staff member (email) the BFF verified.
pub fn staff(h: &HeaderMap) -> R<String> {
    h.get("x-kalks-staff")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().chars().filter(|c| !c.is_control()).take(120).collect::<String>())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "unauthorized", "Missing staff member."))
}

/// Content permission check: the BFF already enforced it; when it forwards `X-Kalks-Staff-Perms` the service
/// checks again (a wrongly wired route can't widen access).
pub fn require(h: &HeaderMap, perm: &str) -> R<String> {
    let who = staff(h)?;
    if let Some(p) = h.get("x-kalks-staff-perms").and_then(|v| v.to_str().ok())
        && !p.split(',').any(|x| x.trim() == perm)
    {
        return Err(ApiError::forbidden());
    }
    Ok(who)
}

/// Percent-decoded header value (file names can be any script; the BFF sends encodeURIComponent()).
pub fn percent_decode(raw: &str) -> String {
    let b = raw.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && raw.is_char_boundary(i + 3)
            && let Ok(x) = u8::from_str_radix(&raw[i + 1..i + 3], 16)
        {
            out.push(x);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).chars().filter(|c| !c.is_control()).collect()
}

/// JSON object body (a missing / non-object body is a 400).
pub fn object(body: &Bytes) -> R<serde_json::Map<String, Value>> {
    match serde_json::from_slice::<Value>(body) {
        Ok(Value::Object(m)) => Ok(m),
        _ => Err(ApiError::new(StatusCode::BAD_REQUEST, "bad_request", "Expected a JSON object.")),
    }
}

async fn require_internal(State(st): State<AppState>, req: Request, next: Next) -> Response {
    if !st.cfg.internal_token.is_empty() {
        let got = req.headers().get("x-kalks-internal").map(|v| v.as_bytes()).unwrap_or(b"");
        if !bool::from(got.ct_eq(st.cfg.internal_token.as_bytes())) {
            return ApiError::new(StatusCode::UNAUTHORIZED, "unauthorized", "Internal token required.").into_response();
        }
    }
    next.run(req).await
}

pub fn router(st: AppState) -> Router {
    let upload_limit = DefaultBodyLimit::max(st.cfg.max_upload_bytes + 64 * 1024);
    let uploads = Router::new()
        .route("/v1/admin/media", get(media::list).post(media::upload))
        .route("/v1/admin/media/{id}/file", post(media::replace_file).delete(media::remove_file))
        .layer(upload_limit);
    let admin = Router::new()
        .route("/v1/admin/content/meta", get(content::meta))
        .route("/v1/admin/content/tree", get(content::tree))
        .route("/v1/admin/content/slot", get(content::slot_view).delete(content::slot_delete))
        .route("/v1/admin/content/entry", put(content::save_draft))
        .route("/v1/admin/content/discard", post(content::discard))
        .route("/v1/admin/content/publish", post(content::publish))
        .route("/v1/admin/content/unpublish", post(content::unpublish))
        .route("/v1/admin/content/versions", get(content::versions))
        .route("/v1/admin/content/rollback", post(content::rollback))
        .route("/v1/admin/content/import", post(content::import))
        .route("/v1/admin/content/preview-token", post(content::preview_token))
        .route("/v1/admin/content/audit", get(content::audit))
        .route("/v1/admin/media/{id}", get(media::detail).put(media::update).delete(media::delete))
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
        .merge(uploads)
        .layer(middleware::from_fn_with_state(st.clone(), require_internal));
    Router::new()
        .route("/health", get(health))
        .route("/v1/public/content", get(content::public_read))
        .route("/v1/public/media/{file}/{variant}", get(media::serve))
        .merge(admin)
        .with_state(st)
}

async fn health(State(st): State<AppState>) -> Json<Value> {
    let db = sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&st.pool).await.is_ok();
    Json(json!({"service": "content", "status": if db { "ok" } else { "degraded" }, "db": db, "storage": st.storage.name()}))
}
