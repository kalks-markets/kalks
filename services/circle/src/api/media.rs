//! Upload routes: declare, send chunks (through the BFF or directly through Caddy with the upload token),
//! resume (offset), status and the media item.

use super::{Body, Me};
use crate::error::{ApiError, ApiResult, invalid};
use crate::media;
use crate::state::AppState;
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;

type R = ApiResult<Json<Value>>;

pub async fn start(State(st): State<AppState>, me: Me, Body(b): Body) -> R {
    Ok(Json(media::start(&st, me.id(), &b).await?))
}

pub async fn status(State(st): State<AppState>, me: Me, Path(id): Path<i64>) -> R {
    Ok(Json(media::progress(&st, id, me.id()).await?))
}

fn offset(h: &HeaderMap) -> ApiResult<i64> {
    h.get("upload-offset").and_then(|v| v.to_str().ok()).and_then(|v| v.trim().parse::<i64>().ok()).filter(|v| *v >= 0).ok_or_else(|| invalid("Upload-Offset", "Send the byte offset of this chunk in Upload-Offset."))
}

/// `PUT /v1/circle/uploads/{id}` (BFF): one chunk at `Upload-Offset`.
pub async fn chunk(State(st): State<AppState>, me: Me, Path(id): Path<i64>, h: HeaderMap, body: Bytes) -> R {
    Ok(Json(media::write_chunk(&st, id, me.id(), offset(&h)?, &body).await?))
}

pub async fn get(State(st): State<AppState>, me: Me, Path(id): Path<i64>) -> R {
    let r = sqlx::query("SELECT * FROM media WHERE id = $1").bind(id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    let own = r.get::<i64, _>("owner") == me.id();
    if !own && r.get::<String, _>("status") != "ready" {
        return Err(ApiError::NotFound);
    }
    Ok(Json(json!({"media": media::json_row(&st, &r, own)})))
}

#[derive(Deserialize)]
pub struct TokenQ {
    token: Option<String>,
}

fn token_of(q: &TokenQ, h: &HeaderMap) -> Option<String> {
    q.token.clone().or_else(|| h.get("x-upload-token").and_then(|v| v.to_str().ok()).map(str::to_string)).filter(|t| t.len() == 48 && t.chars().all(|c| c.is_ascii_hexdigit()))
}

/// `PUT /v1/upload/{id}?token=` (Caddy `/circle/upload/{id}`): a chunk without the BFF.
pub async fn direct_chunk(State(st): State<AppState>, Path(id): Path<i64>, Query(q): Query<TokenQ>, h: HeaderMap, body: Bytes) -> R {
    let token = token_of(&q, &h).ok_or(ApiError::NotFound)?;
    let owner = media::check_token(&st, id, &token).await?;
    Ok(Json(media::write_chunk(&st, id, owner, offset(&h)?, &body).await?))
}

/// `HEAD /v1/upload/{id}?token=`: the offset to resume from (`Upload-Offset`, `Upload-Length`).
pub async fn direct_head(State(st): State<AppState>, Path(id): Path<i64>, Query(q): Query<TokenQ>, h: HeaderMap) -> Response {
    let Some(token) = token_of(&q, &h) else { return StatusCode::NOT_FOUND.into_response() };
    let Ok(owner) = media::check_token(&st, id, &token).await else { return StatusCode::NOT_FOUND.into_response() };
    match media::progress(&st, id, owner).await {
        Ok(v) => (
            StatusCode::OK,
            [
                ("upload-offset", v["offset"].as_i64().unwrap_or(0).to_string()),
                ("upload-length", v["size"].as_i64().unwrap_or(0).to_string()),
                ("x-upload-status", v["status"].as_str().unwrap_or("").to_string()),
                ("cache-control", "no-store".to_string()),
            ],
        )
            .into_response(),
        Err(e) => e.into_response(),
    }
}
