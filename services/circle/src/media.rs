//! Media uploads and processing.
//!
//! 1. `POST /v1/circle/uploads` declares the file (kind, purpose, type, size) and returns an upload id, a one-time
//!    upload token and the chunk size.
//! 2. The bytes are sent in order, resumable: `PUT /v1/upload/{id}?token=` straight through Caddy
//!    (`/circle/upload/{id}`) or `PUT /v1/circle/uploads/{id}` through the BFF, each with `Upload-Offset`; the
//!    current offset is read back with `HEAD` / `GET` after a dropped connection.
//! 3. When the last byte arrives the item is `processing`: photos → 3 WebP sizes, video → HLS 360p / 720p +
//!    poster, voice → AAC, files kept as sent; then the AI check (moderation.rs). Result: `ready`, `review`
//!    (staff queue, invisible to others), `rejected` or `failed` (with a reason).
//!
//! Media is attached to a post / story / comment / message / avatar by id; others only ever see `ready` media.

use crate::error::{ApiError, ApiResult, invalid};
use crate::imaging;
use crate::moderation::{self, Decision, Verdict};
use crate::state::AppState;
use crate::video;
use serde_json::{Value, json};
use sqlx::Row;
use std::path::PathBuf;
use std::time::Duration;

pub const KINDS: &[&str] = &["photo", "video", "voice", "file", "chart"];
pub const PURPOSES: &[&str] = &["post", "story", "topic_video", "chat", "avatar", "cover", "comment"];

pub fn allowed(kind: &str, purpose: &str) -> bool {
    match purpose {
        "post" | "story" => matches!(kind, "photo" | "video" | "chart"),
        "topic_video" => kind == "video",
        "chat" => KINDS.contains(&kind),
        "avatar" | "cover" => kind == "photo",
        "comment" => matches!(kind, "photo" | "chart"),
        _ => false,
    }
}

pub const FILE_EXTS: &[&str] = &["pdf", "txt", "csv", "docx", "xlsx", "pptx", "png", "jpg", "jpeg", "webp"];

fn mime_ok(kind: &str, mime: &str) -> bool {
    let m = mime.to_ascii_lowercase();
    match kind {
        "photo" | "chart" => ["image/jpeg", "image/jpg", "image/png", "image/webp", "image/gif"].contains(&m.as_str()),
        "video" => ["video/mp4", "video/quicktime", "video/webm", "video/x-matroska", "video/3gpp", "video/x-m4v"].contains(&m.as_str()),
        "voice" => m.starts_with("audio/") || m == "video/webm" || m == "video/mp4",
        "file" => true,
        _ => false,
    }
}

pub fn ext_of(name: &str) -> String {
    name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).filter(|e| e.len() <= 5 && e.chars().all(|c| c.is_ascii_alphanumeric())).unwrap_or_default()
}

fn max_bytes(st: &AppState, kind: &str) -> i64 {
    match kind {
        "video" => st.cfg.max_video_bytes,
        "voice" => st.cfg.max_voice_bytes,
        "file" => st.cfg.max_file_bytes,
        _ => st.cfg.max_photo_bytes,
    }
}

fn work_path(st: &AppState, key: &str) -> PathBuf {
    PathBuf::from(&st.cfg.work_dir).join("up").join(key)
}

/// Storage prefix of a media item.
pub fn prefix(key: &str, created: chrono::DateTime<chrono::Utc>) -> String {
    format!("m/{}/{key}", created.format("%Y"))
}

/// Declares an upload. Returns the media row id, the one-time token and the chunk size.
pub async fn start(st: &AppState, owner: i64, b: &Value) -> ApiResult<Value> {
    let kind = b["kind"].as_str().unwrap_or("");
    let purpose = b["purpose"].as_str().unwrap_or("");
    if !KINDS.contains(&kind) {
        return Err(invalid("kind", "kind is photo, video, voice, file or chart."));
    }
    if !PURPOSES.contains(&purpose) || !allowed(kind, purpose) {
        return Err(invalid("purpose", "This kind of media can't be used there."));
    }
    let mime = b["mime"].as_str().unwrap_or("").trim().to_ascii_lowercase();
    let name = crate::util::clean(b["name"].as_str().unwrap_or(""), 120);
    if !mime_ok(kind, &mime) {
        return Err(ApiError::Unsupported(match kind {
            "photo" | "chart" => "Photos can be JPEG, PNG, WebP or GIF.".into(),
            "video" => "Videos can be MP4, MOV or WebM.".into(),
            _ => "This file type isn't supported.".into(),
        }));
    }
    if kind == "file" && !FILE_EXTS.contains(&ext_of(&name).as_str()) {
        return Err(ApiError::Unsupported(format!("Files can be {}.", FILE_EXTS.join(", "))));
    }
    let size = b["size"].as_i64().unwrap_or(0);
    let max = max_bytes(st, kind);
    if size <= 0 {
        return Err(invalid("size", "size is the file size in bytes."));
    }
    if size > max {
        return Err(ApiError::TooLarge(format!("This file is too large (max {} MB).", max / 1024 / 1024)));
    }
    let chart = if kind == "chart" {
        let c = &b["chart"];
        let symbol = c["symbol"].as_str().map(str::to_uppercase).filter(|s| crate::upstream::is_symbol(s)).ok_or_else(|| invalid("chart.symbol", "A chart snapshot needs its symbol."))?;
        let tf = c["timeframe"].as_str().map(|t| crate::util::clean(t, 8)).filter(|t| !t.is_empty()).unwrap_or_else(|| "H1".into());
        let drawings = c.get("drawings").cloned().unwrap_or(json!([]));
        if drawings.to_string().len() > 64 * 1024 {
            return Err(invalid("chart.drawings", "Too many drawings to save with the snapshot."));
        }
        Some(json!({"symbol": symbol, "timeframe": tf, "drawings": drawings, "indicators": c.get("indicators").cloned().unwrap_or(json!([])), "price": c["price"].as_f64()}))
    } else {
        None
    };
    if !st.limiter.hit(&format!("upload:{owner}"), 60, Duration::from_secs(3600)) {
        return Err(ApiError::RateLimited("Too many uploads. Please try again later.".into()));
    }
    let key = crate::util::token(16);
    let token = crate::util::token(24);
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO media (key, owner, kind, purpose, mime, name, declared_size, upload_token, chart) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING id",
    )
    .bind(&key)
    .bind(owner)
    .bind(kind)
    .bind(purpose)
    .bind(&mime)
    .bind(&name)
    .bind(size)
    .bind(crate::util::sha256_hex(&token))
    .bind(chart.map(sqlx::types::Json))
    .fetch_one(&st.pool)
    .await?;
    Ok(json!({"upload": {"id": id, "token": token, "chunkSize": st.cfg.chunk_bytes, "size": size, "offset": 0, "directUrl": format!("/circle/upload/{id}"), "expiresIn": 86_400}}))
}

/// Authenticates a direct upload by its token.
pub async fn check_token(st: &AppState, id: i64, token: &str) -> ApiResult<i64> {
    let r = sqlx::query("SELECT owner, upload_token FROM media WHERE id = $1").bind(id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    if !crate::util::eq_ct(&r.get::<String, _>("upload_token"), &crate::util::sha256_hex(token)) {
        return Err(ApiError::NotFound);
    }
    Ok(r.get("owner"))
}

/// Upload progress: `{id, status, offset, size}` (+ the media once processed).
pub async fn progress(st: &AppState, id: i64, owner: i64) -> ApiResult<Value> {
    let r = sqlx::query("SELECT * FROM media WHERE id = $1 AND owner = $2").bind(id).bind(owner).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    Ok(json!({"id": id, "status": r.get::<String, _>("status"), "offset": r.get::<i64, _>("received"), "size": r.get::<i64, _>("declared_size"), "media": json_row(st, &r, true)}))
}

/// Appends one chunk at `offset`. Out-of-order chunks get 409 `offset_mismatch` with the expected offset.
pub async fn write_chunk(st: &AppState, id: i64, owner: i64, offset: i64, bytes: &[u8]) -> ApiResult<Value> {
    let r = sqlx::query("SELECT key, status, received, declared_size, created_at FROM media WHERE id = $1 AND owner = $2").bind(id).bind(owner).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    if r.get::<String, _>("status") != "uploading" {
        return Err(crate::error::conflict("upload_complete", "This upload is already complete."));
    }
    if chrono::Utc::now() - r.get::<chrono::DateTime<chrono::Utc>, _>("created_at") > chrono::Duration::hours(24) {
        return Err(crate::error::conflict("upload_expired", "This upload expired. Please start again."));
    }
    let received: i64 = r.get("received");
    let size: i64 = r.get("declared_size");
    if offset != received {
        return Err(ApiError::Conflict { code: "offset_mismatch", message: format!("Expected offset {received}.") });
    }
    if bytes.is_empty() {
        return Err(invalid("body", "Empty chunk."));
    }
    if bytes.len() as i64 > st.cfg.chunk_bytes || received + bytes.len() as i64 > size {
        return Err(ApiError::TooLarge("The chunk is larger than the declared file.".into()));
    }
    let path = work_path(st, &r.get::<String, _>("key"));
    tokio::fs::create_dir_all(path.parent().expect("work dir")).await?;
    {
        use tokio::io::AsyncWriteExt;
        let mut f = tokio::fs::OpenOptions::new().create(true).append(true).open(&path).await?;
        // the file must be exactly `received` long (a crashed earlier write is cut back)
        let len = f.metadata().await?.len() as i64;
        if len != received {
            f.set_len(received as u64).await?;
        }
        f.write_all(bytes).await?;
        f.flush().await?;
    }
    let now = received + bytes.len() as i64;
    let done = now == size;
    // optimistic: only the writer that saw `received` moves it on
    let updated = sqlx::query("UPDATE media SET received = $3, status = CASE WHEN $4 THEN 'processing' ELSE status END, updated_at = now() WHERE id = $1 AND received = $2")
        .bind(id)
        .bind(received)
        .bind(now)
        .bind(done)
        .execute(&st.pool)
        .await?
        .rows_affected();
    if updated == 0 {
        return Err(ApiError::Conflict { code: "offset_mismatch", message: "Another upload of this file is running.".into() });
    }
    if done {
        st.wake.media.notify_one();
    }
    Ok(json!({"id": id, "offset": now, "size": size, "complete": done, "status": if done { "processing" } else { "uploading" }}))
}

/// Public JSON of a media row. `own` adds status / reason for the uploader.
pub fn json_row(st: &AppState, r: &sqlx::postgres::PgRow, own: bool) -> Value {
    let status: String = r.get("status");
    let kind: String = r.get("kind");
    let variants: Value = r.get::<sqlx::types::Json<Value>, _>("variants").0;
    let urls: serde_json::Map<String, Value> = if status == "ready" || own {
        variants.as_object().map(|m| m.iter().filter_map(|(k, v)| Some((k.clone(), json!(st.storage.url(v.as_str()?))))).collect()).unwrap_or_default()
    } else {
        Default::default()
    };
    let mut v = json!({
        "id": r.get::<i64, _>("id"),
        "kind": kind,
        "status": status,
        "width": r.get::<Option<i32>, _>("width"),
        "height": r.get::<Option<i32>, _>("height"),
        "durationMs": r.get::<Option<i32>, _>("duration_ms"),
        "urls": urls,
        "chart": r.get::<Option<sqlx::types::Json<Value>>, _>("chart").map(|j| j.0),
    });
    if kind == "file" || kind == "voice" {
        v["name"] = json!(r.get::<String, _>("name"));
        v["size"] = json!(r.get::<i64, _>("declared_size"));
    }
    if own {
        v["reason"] = json!(r.get::<Option<String>, _>("reason"));
        v["purpose"] = json!(r.get::<String, _>("purpose"));
    }
    v
}

pub async fn many(st: &AppState, ids: &[i64], viewer: i64) -> ApiResult<std::collections::HashMap<i64, Value>> {
    if ids.is_empty() {
        return Ok(Default::default());
    }
    let rows = sqlx::query("SELECT * FROM media WHERE id = ANY($1)").bind(ids).fetch_all(&st.pool).await?;
    Ok(rows.iter().map(|r| (r.get::<i64, _>("id"), json_row(st, r, r.get::<i64, _>("owner") == viewer))).collect())
}

/// Media the owner may attach (`purposes`): not rejected / failed / deleted. Returns (id, kind, status).
pub async fn attachable(st: &AppState, owner: i64, ids: &[i64], purposes: &[&str]) -> ApiResult<Vec<(i64, String, String)>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let rows = sqlx::query("SELECT id, kind, status, purpose FROM media WHERE id = ANY($1) AND owner = $2").bind(ids).bind(owner).fetch_all(&st.pool).await?;
    let mut out = Vec::new();
    for id in ids {
        let r = rows.iter().find(|r| r.get::<i64, _>("id") == *id).ok_or_else(|| invalid("mediaIds", "Upload the media first."))?;
        let status: String = r.get("status");
        if !purposes.contains(&r.get::<String, _>("purpose").as_str()) {
            return Err(invalid("mediaIds", "This media was uploaded for something else."));
        }
        match status.as_str() {
            "rejected" => return Err(ApiError::Rejected { code: "media_rejected", message: "One of the files didn't pass the content check.".into() }),
            "failed" | "deleted" => return Err(invalid("mediaIds", "One of the files couldn't be processed. Please upload it again.")),
            "uploading" => return Err(invalid("mediaIds", "One of the files hasn't finished uploading.")),
            _ => {}
        }
        out.push((*id, r.get("kind"), status));
    }
    Ok(out)
}

pub async fn mark_attached(st: &AppState, ids: &[i64]) -> anyhow::Result<()> {
    if !ids.is_empty() {
        sqlx::query("UPDATE media SET attached = true WHERE id = ANY($1)").bind(ids).execute(&st.pool).await?;
    }
    Ok(())
}

/// Deletes the stored files of a media item (removal by staff, deleted content, failed uploads).
pub async fn purge(st: &AppState, id: i64) -> anyhow::Result<()> {
    let Some(r) = sqlx::query("SELECT key, files FROM media WHERE id = $1").bind(id).fetch_optional(&st.pool).await? else { return Ok(()) };
    for k in r.get::<Vec<String>, _>("files") {
        if let Err(e) = st.storage.delete(&k).await {
            tracing::warn!(error = %e, key = %k, "media delete failed");
        }
    }
    let _ = tokio::fs::remove_file(work_path(st, &r.get::<String, _>("key"))).await;
    sqlx::query("UPDATE media SET status = 'deleted', files = '{}', variants = '{}', updated_at = now() WHERE id = $1").bind(id).execute(&st.pool).await?;
    Ok(())
}

// ---------------------------------------------------------------- processing

/// Processes every completed upload (one at a time). Returns how many were handled.
pub async fn process_pending(st: &AppState) -> anyhow::Result<usize> {
    let mut n = 0;
    loop {
        let row = sqlx::query(
            "UPDATE media SET attempts = attempts + 1, updated_at = now() WHERE id = (
                SELECT id FROM media WHERE status = 'processing' AND (attempts = 0 OR updated_at < now() - interval '2 minutes')
                ORDER BY updated_at LIMIT 1 FOR UPDATE SKIP LOCKED) RETURNING *",
        )
        .fetch_optional(&st.pool)
        .await?;
        let Some(r) = row else { break };
        n += 1;
        let id: i64 = r.get("id");
        let attempts: i32 = r.get("attempts");
        match process_one(st, &r).await {
            Ok(Outcome::Done) => {}
            Ok(Outcome::Retry(e)) if attempts < 5 => {
                tracing::warn!(error = %e, media = id, "media check postponed");
            }
            Ok(Outcome::Retry(e)) => {
                // the AI check never answered: a person decides
                let v = Verdict { decision: Decision::Review, categories: vec![], reason: format!("AI check unavailable: {e}"), source: "fallback", model: None };
                finish_review(st, &r, &v).await?;
            }
            Err(e) => {
                tracing::warn!(error = %e, media = id, "media processing failed");
                fail(st, id, "This file couldn't be processed. Please try another one.").await?;
            }
        }
    }
    Ok(n)
}

enum Outcome {
    Done,
    /// AI temporarily unavailable: try again later (stored files are kept).
    Retry(String),
}

async fn fail(st: &AppState, id: i64, reason: &str) -> anyhow::Result<()> {
    let key: String = sqlx::query_scalar("UPDATE media SET status = 'failed', reason = $2, updated_at = now() WHERE id = $1 RETURNING key").bind(id).bind(reason).fetch_one(&st.pool).await?;
    let _ = tokio::fs::remove_file(work_path(st, &key)).await;
    notify_owner(st, id).await;
    Ok(())
}

async fn notify_owner(st: &AppState, id: i64) {
    if let Ok(Some(r)) = sqlx::query("SELECT * FROM media WHERE id = $1").bind(id).fetch_optional(&st.pool).await {
        let owner: i64 = r.get("owner");
        st.hub.send(crate::state::Target::User(owner), json!({"type": "media", "media": json_row(st, &r, true)}));
    }
    // posts / stories / messages waiting for this media move on
    st.wake.moderation.notify_one();
}

async fn finish_review(st: &AppState, r: &sqlx::postgres::PgRow, v: &Verdict) -> anyhow::Result<()> {
    let id: i64 = r.get("id");
    let owner: i64 = r.get("owner");
    sqlx::query("UPDATE media SET status = 'review', moderation = $2, updated_at = now(), processed_at = now() WHERE id = $1").bind(id).bind(sqlx::types::Json(v.json())).execute(&st.pool).await?;
    let tenant: String = sqlx::query_scalar("SELECT tenant FROM profiles WHERE user_id = $1").bind(owner).fetch_optional(&st.pool).await?.unwrap_or_else(|| "kalks".into());
    moderation::queue(st, "media", id, owner, &tenant, v.source, v, &format!("{} {}", r.get::<String, _>("kind"), r.get::<String, _>("name"))).await?;
    notify_owner(st, id).await;
    Ok(())
}

/// Applies the check result to a processed item.
async fn decide(st: &AppState, r: &sqlx::postgres::PgRow, v: Verdict) -> anyhow::Result<Outcome> {
    let id: i64 = r.get("id");
    match v.decision {
        Decision::Allow => {
            sqlx::query("UPDATE media SET status = 'ready', moderation = $2, updated_at = now(), processed_at = now() WHERE id = $1").bind(id).bind(sqlx::types::Json(v.json())).execute(&st.pool).await?;
            notify_owner(st, id).await;
        }
        Decision::Review => finish_review(st, r, &v).await?,
        Decision::Block => {
            sqlx::query("UPDATE media SET status = 'rejected', moderation = $2, reason = $3, updated_at = now(), processed_at = now() WHERE id = $1")
                .bind(id)
                .bind(sqlx::types::Json(v.json()))
                .bind(moderation::main_reason(&v))
                .execute(&st.pool)
                .await?;
            // rejected files are not kept
            let files: Vec<String> = sqlx::query_scalar("SELECT unnest(files) FROM media WHERE id = $1").bind(id).fetch_all(&st.pool).await?;
            for k in files {
                let _ = st.storage.delete(&k).await;
            }
            sqlx::query("UPDATE media SET files = '{}', variants = '{}' WHERE id = $1").bind(id).execute(&st.pool).await?;
            notify_owner(st, id).await;
        }
    }
    Ok(Outcome::Done)
}

/// The AI check of images, with the fallback policy when no key is configured.
async fn check_images(st: &AppState, kind: &str, images: &[(String, Vec<u8>)], text: &str) -> Result<Verdict, String> {
    if !st.ai() {
        return Ok(if st.cfg.moderation_fallback == "publish" {
            Verdict::allow("fallback")
        } else {
            Verdict { decision: Decision::Review, categories: vec![], reason: "No AI check configured: a moderator reviews uploads.".into(), source: "fallback", model: None }
        });
    }
    moderation::classify_images(st, kind, images, text).await.map_err(|e| e.to_string())
}

fn already_stored(r: &sqlx::postgres::PgRow) -> bool {
    !r.get::<Vec<String>, _>("files").is_empty()
}

async fn process_one(st: &AppState, r: &sqlx::postgres::PgRow) -> anyhow::Result<Outcome> {
    let id: i64 = r.get("id");
    let key: String = r.get("key");
    let kind: String = r.get("kind");
    let purpose: String = r.get("purpose");
    let pre = prefix(&key, r.get("created_at"));
    let src = work_path(st, &key);
    match kind.as_str() {
        "photo" | "chart" => {
            // retry of a stored photo whose AI check failed: check the stored preview again
            if already_stored(r) {
                let large = r.get::<sqlx::types::Json<Value>, _>("variants").0["large"].as_str().map(str::to_string).unwrap_or_default();
                let bytes = st.storage.get(&large).await?;
                let preview = imaging::preview(&bytes)?;
                return match check_images(st, &kind, &[("image/jpeg".into(), preview)], "").await {
                    Ok(v) => decide(st, r, v).await,
                    Err(e) => Ok(Outcome::Retry(e)),
                };
            }
            let bytes = tokio::fs::read(&src).await?;
            let purpose2 = purpose.clone();
            let p = match tokio::task::spawn_blocking(move || imaging::process(&bytes, &purpose2)).await? {
                Ok(p) => p,
                Err(e) => {
                    tracing::info!(error = %e, media = id, "image rejected by the decoder");
                    fail(st, id, "This image couldn't be read. Use a JPEG, PNG or WebP photo.").await?;
                    return Ok(Outcome::Done);
                }
            };
            let mut variants = serde_json::Map::new();
            let mut files = Vec::new();
            for (name, data) in p.variants {
                let k = format!("{pre}/{name}.webp");
                st.storage.put(&k, data).await?;
                variants.insert(name.into(), json!(k));
                files.push(k);
            }
            sqlx::query("UPDATE media SET variants = $2, files = $3, width = $4, height = $5, mime = 'image/webp' WHERE id = $1")
                .bind(id)
                .bind(sqlx::types::Json(Value::Object(variants)))
                .bind(&files)
                .bind(p.width as i32)
                .bind(p.height as i32)
                .execute(&st.pool)
                .await?;
            let _ = tokio::fs::remove_file(&src).await;
            let fresh = sqlx::query("SELECT * FROM media WHERE id = $1").bind(id).fetch_one(&st.pool).await?;
            match check_images(st, &kind, &[("image/jpeg".into(), p.preview)], "").await {
                Ok(v) => decide(st, &fresh, v).await,
                Err(e) => Ok(Outcome::Retry(e)),
            }
        }
        "video" => {
            if !video::available(&st.cfg).await {
                fail(st, id, "Video uploads aren't available yet. Please try again later.").await?;
                return Ok(Outcome::Done);
            }
            let max_secs = if purpose == "topic_video" { st.cfg.max_topic_video_secs } else { st.cfg.max_video_secs };
            let probe = match video::probe(&st.cfg, &src).await {
                Ok(p) if p.has_video && p.width > 0 => p,
                _ => {
                    fail(st, id, "This video couldn't be read. Use an MP4, MOV or WebM file.").await?;
                    return Ok(Outcome::Done);
                }
            };
            if probe.duration_ms > (max_secs * 1000) + 1500 {
                fail(st, id, &format!("Videos here can be up to {} seconds.", max_secs)).await?;
                return Ok(Outcome::Done);
            }
            let work = PathBuf::from(&st.cfg.work_dir).join("tx").join(&key);
            let _ = tokio::fs::remove_dir_all(&work).await;
            tokio::fs::create_dir_all(&work).await?;
            let input = src.to_string_lossy().to_string();
            let wd = work.to_string_lossy().to_string();
            // moderation frames first (cheap), then the transcode
            let n_frames = if probe.duration_ms > 120_000 { 8 } else { 4 };
            video::run(&st.cfg.ffmpeg, &video::frames_args(&input, &format!("{wd}/frame_%02d.jpg"), n_frames, probe.duration_ms), Duration::from_secs(300)).await?;
            let mut frames = Vec::new();
            for i in 1..=n_frames {
                if let Ok(b) = tokio::fs::read(work.join(format!("frame_{i:02}.jpg"))).await {
                    frames.push(("image/jpeg".to_string(), b));
                }
            }
            let verdict = match check_images(st, "video frames", &frames, "").await {
                Ok(v) => v,
                Err(e) => return Ok(Outcome::Retry(e)),
            };
            if verdict.decision == Decision::Block {
                let _ = tokio::fs::remove_dir_all(&work).await;
                let _ = tokio::fs::remove_file(&src).await;
                return decide(st, r, verdict).await;
            }
            video::run(&st.cfg.ffmpeg, &video::hls_args(&input, &format!("{wd}/hls"), &probe, max_secs), Duration::from_secs(1800)).await?;
            video::run(&st.cfg.ffmpeg, &video::poster_args(&input, &format!("{wd}/poster.jpg"), (probe.duration_ms / 3).min(1500)), Duration::from_secs(60)).await?;
            let poster = tokio::fs::read(work.join("poster.jpg")).await?;
            let poster_webp = tokio::task::spawn_blocking(move || imaging::decode(&poster).map(|img| imaging::webp(&img, 78.0))).await??;
            let mut files = Vec::new();
            let pk = format!("{pre}/poster.webp");
            st.storage.put(&pk, poster_webp).await?;
            files.push(pk.clone());
            // upload the HLS tree
            let hls = work.join("hls");
            let mut stack = vec![hls.clone()];
            while let Some(dir) = stack.pop() {
                let mut rd = tokio::fs::read_dir(&dir).await?;
                while let Some(e) = rd.next_entry().await? {
                    let p = e.path();
                    if e.file_type().await?.is_dir() {
                        stack.push(p);
                    } else {
                        let rel = p.strip_prefix(&hls)?.to_string_lossy().replace('\\', "/");
                        let k = format!("{pre}/hls/{rel}");
                        st.storage.put_file(&k, &p).await?;
                        files.push(k);
                    }
                }
            }
            let variants = json!({"hls": format!("{pre}/hls/master.m3u8"), "poster": pk});
            let (w, h) = video::renditions(probe.width, probe.height).last().map(|(_, w, h, _)| (*w, *h)).unwrap_or((probe.width, probe.height));
            sqlx::query("UPDATE media SET variants = $2, files = $3, width = $4, height = $5, duration_ms = $6 WHERE id = $1")
                .bind(id)
                .bind(sqlx::types::Json(variants))
                .bind(&files)
                .bind(w as i32)
                .bind(h as i32)
                .bind(probe.duration_ms.min(i32::MAX as i64) as i32)
                .execute(&st.pool)
                .await?;
            let _ = tokio::fs::remove_dir_all(&work).await;
            let _ = tokio::fs::remove_file(&src).await;
            let fresh = sqlx::query("SELECT * FROM media WHERE id = $1").bind(id).fetch_one(&st.pool).await?;
            decide(st, &fresh, verdict).await
        }
        "voice" => {
            // no speech-to-text on the server: voice notes are not AI-checked (chat reports cover them)
            let mime: String = r.get("mime");
            let (bytes, ext, duration) = if video::available(&st.cfg).await {
                let out = PathBuf::from(&st.cfg.work_dir).join("tx").join(format!("{key}.m4a"));
                tokio::fs::create_dir_all(out.parent().expect("dir")).await?;
                let probe = video::probe(&st.cfg, &src).await.ok();
                if probe.as_ref().is_none_or(|p| !p.has_audio) {
                    fail(st, id, "This voice note couldn't be read.").await?;
                    return Ok(Outcome::Done);
                }
                video::run(&st.cfg.ffmpeg, &video::voice_args(&src.to_string_lossy(), &out.to_string_lossy(), st.cfg.max_voice_secs), Duration::from_secs(120)).await?;
                let b = tokio::fs::read(&out).await?;
                let _ = tokio::fs::remove_file(&out).await;
                (b, "m4a".to_string(), probe.map(|p| p.duration_ms.min(st.cfg.max_voice_secs * 1000)))
            } else {
                let ext = match mime.as_str() {
                    "audio/mp4" | "audio/m4a" | "audio/x-m4a" | "audio/aac" => "m4a",
                    "audio/ogg" | "audio/opus" => "ogg",
                    "audio/mpeg" | "audio/mp3" => "mp3",
                    "audio/webm" | "video/webm" => "webm",
                    _ => {
                        fail(st, id, "Voice notes can be AAC, M4A, Ogg/Opus, MP3 or WebM.").await?;
                        return Ok(Outcome::Done);
                    }
                };
                (tokio::fs::read(&src).await?, ext.to_string(), None)
            };
            let k = format!("{pre}/voice.{ext}");
            st.storage.put(&k, bytes).await?;
            sqlx::query("UPDATE media SET variants = $2, files = $3, duration_ms = $4, status = 'ready', processed_at = now(), updated_at = now(), moderation = $5 WHERE id = $1")
                .bind(id)
                .bind(sqlx::types::Json(json!({"audio": k})))
                .bind(vec![k.clone()])
                .bind(duration.map(|d| d as i32))
                .bind(sqlx::types::Json(json!({"decision": "allow", "source": "none", "note": "audio is not transcribed"})))
                .execute(&st.pool)
                .await?;
            let _ = tokio::fs::remove_file(&src).await;
            notify_owner(st, id).await;
            Ok(Outcome::Done)
        }
        "file" => {
            let name: String = r.get("name");
            let ext = ext_of(&name);
            let bytes = tokio::fs::read(&src).await?;
            // the declared type must match the content for PDFs and images
            let sniff_ok = match ext.as_str() {
                "pdf" => bytes.starts_with(b"%PDF"),
                "png" => bytes.starts_with(&[0x89, b'P', b'N', b'G']),
                "jpg" | "jpeg" => bytes.starts_with(&[0xFF, 0xD8]),
                "webp" => bytes.len() > 12 && &bytes[8..12] == b"WEBP",
                "docx" | "xlsx" | "pptx" => bytes.starts_with(b"PK"),
                "txt" | "csv" => std::str::from_utf8(&bytes).is_ok(),
                _ => false,
            };
            if !sniff_ok {
                fail(st, id, "The file doesn't match its type.").await?;
                return Ok(Outcome::Done);
            }
            let verdict = if ext == "pdf" && st.ai() && bytes.len() < 30 * 1024 * 1024 {
                match moderation::classify_pdf(st, &bytes, &name).await {
                    Ok(v) => v,
                    Err(e) => return Ok(Outcome::Retry(e.to_string())),
                }
            } else if matches!(ext.as_str(), "txt" | "csv") && st.ai() {
                match moderation::classify_text(st, "file shared in a chat", &String::from_utf8_lossy(&bytes[..bytes.len().min(16_000)])).await {
                    Ok(v) => v,
                    Err(e) => return Ok(Outcome::Retry(e.to_string())),
                }
            } else if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp") {
                let b2 = bytes.clone();
                match tokio::task::spawn_blocking(move || imaging::preview(&b2)).await? {
                    Ok(p) => match check_images(st, "image file shared in a chat", &[("image/jpeg".into(), p)], &name).await {
                        Ok(v) => v,
                        Err(e) => return Ok(Outcome::Retry(e)),
                    },
                    Err(_) => {
                        fail(st, id, "This image couldn't be read.").await?;
                        return Ok(Outcome::Done);
                    }
                }
            } else {
                Verdict::allow("rules")
            };
            let mut safe: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')).take(60).collect();
            while safe.contains("..") {
                safe = safe.replace("..", ".");
            }
            let safe = if safe.trim_matches('.').is_empty() { format!("file.{ext}") } else { safe };
            let k = format!("{pre}/f/{safe}");
            let k = if k.ends_with(&format!(".{ext}")) { k } else { format!("{k}.{ext}") };
            st.storage.put(&k, bytes).await?;
            sqlx::query("UPDATE media SET variants = $2, files = $3 WHERE id = $1").bind(id).bind(sqlx::types::Json(json!({"file": k}))).bind(vec![k.clone()]).execute(&st.pool).await?;
            let _ = tokio::fs::remove_file(&src).await;
            let fresh = sqlx::query("SELECT * FROM media WHERE id = $1").bind(id).fetch_one(&st.pool).await?;
            decide(st, &fresh, verdict).await
        }
        _ => {
            fail(st, id, "Unknown media kind.").await?;
            Ok(Outcome::Done)
        }
    }
}

/// Drops uploads that never completed (24 h) and work files of failed items.
pub async fn cleanup(st: &AppState) -> anyhow::Result<usize> {
    let keys: Vec<(i64, String)> = sqlx::query_as("UPDATE media SET status = 'failed', reason = 'Upload not completed.' WHERE status = 'uploading' AND created_at < now() - interval '24 hours' RETURNING id, key").fetch_all(&st.pool).await?;
    for (_, k) in &keys {
        let _ = tokio::fs::remove_file(work_path(st, k)).await;
    }
    // unattached media older than 7 days (abandoned composer uploads)
    let stale: Vec<i64> = sqlx::query_scalar("SELECT id FROM media WHERE NOT attached AND status IN ('ready','review','rejected') AND purpose NOT IN ('avatar','cover') AND created_at < now() - interval '7 days' LIMIT 200").fetch_all(&st.pool).await?;
    for id in &stale {
        purge(st, *id).await?;
    }
    Ok(keys.len() + stale.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_and_purposes() {
        assert!(allowed("photo", "post") && allowed("video", "story") && allowed("video", "topic_video") && allowed("voice", "chat"));
        assert!(!allowed("voice", "post") && !allowed("file", "story") && !allowed("photo", "topic_video") && !allowed("video", "avatar"));
        assert!(mime_ok("photo", "image/png") && !mime_ok("photo", "image/svg+xml") && !mime_ok("photo", "image/heic"));
        assert!(mime_ok("video", "video/quicktime") && !mime_ok("video", "application/x-msdownload"));
        assert_eq!(ext_of("Report.PDF"), "pdf");
        assert_eq!(ext_of("noext"), "");
    }
}
