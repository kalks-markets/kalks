//! Media library: upload (type sniffing, SVG checks, size limit), resized WebP variants, library entries with a
//! light and an optional dark file plus alt text per locale, usage, and public serving.
//!
//! Files are content-hashed (`media_files.id` = first 32 hex of sha256(tenant, bytes)), so a public URL always
//! shows the same bytes and is cached for a year. Replacing an asset's file gives it a new file id (new URLs);
//! content that references the asset picks the new file up on its next read. Deleting an asset hides it from the
//! library and from content; its files stay in storage (old URLs keep working, rollbacks stay possible).

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use image::{DynamicImage, ImageDecoder, ImageReader, Limits};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::io::Cursor;

use crate::api::{self, ApiError, AppState, R};
use crate::content::LOCALES;
use crate::store;

/// Variant name → target width. A variant is only made when the original is wider.
pub const VARIANTS: [(&str, u32); 3] = [("sm", 480), ("md", 1024), ("lg", 1920)];
/// Largest accepted raster (either side) and pixel count: decompression-bomb guard.
const MAX_SIDE: u32 = 12_000;
const MAX_PIXELS: u64 = 60_000_000;
/// WebP quality of the variants.
const WEBP_QUALITY: f32 = 82.0;

/* ------------------------------------------------------------------ */
/* Type sniffing and SVG checks (pure)                                 */
/* ------------------------------------------------------------------ */

/// The file type from its first bytes: PNG, JPEG, WebP or SVG only → (mime, extension).
pub fn sniff(b: &[u8]) -> Option<(&'static str, &'static str)> {
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(("image/png", "png"))
    } else if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(("image/jpeg", "jpg"))
    } else if b.len() > 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Some(("image/webp", "webp"))
    } else if looks_like_svg(b) {
        Some(("image/svg+xml", "svg"))
    } else {
        None
    }
}

/// UTF-8 text whose first element (after a BOM, whitespace, an XML declaration and comments) is `<svg`.
fn looks_like_svg(b: &[u8]) -> bool {
    let Ok(s) = std::str::from_utf8(b) else { return false };
    let mut s = s.trim_start_matches('\u{feff}').trim_start();
    loop {
        if let Some(rest) = s.strip_prefix("<?xml") {
            let Some(end) = rest.find("?>") else { return false };
            s = rest[end + 2..].trim_start();
        } else if let Some(rest) = s.strip_prefix("<!--") {
            let Some(end) = rest.find("-->") else { return false };
            s = rest[end + 3..].trim_start();
        } else {
            break;
        }
    }
    s.len() > 4 && s[..4].eq_ignore_ascii_case("<svg") && s[4..].starts_with(|c: char| c.is_whitespace() || c == '>' || c == '/')
}

/// One start tag of an SVG: lowercase name and (lowercase name, raw value) attributes.
struct Tag {
    name: String,
    attrs: Vec<(String, String)>,
}

/// Start tags of an SVG document (comments, CDATA, end tags and processing instructions skipped). Err on
/// malformed markup (an unclosed tag or quote).
fn tags(s: &str) -> Result<Vec<Tag>, &'static str> {
    let b = s.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while let Some(off) = s[i..].find('<') {
        i += off;
        let rest = &s[i..];
        if rest.starts_with("<!--") {
            i += rest.find("-->").ok_or("unclosed comment")? + 3;
            continue;
        }
        if rest.starts_with("<![CDATA[") {
            i += rest.find("]]>").ok_or("unclosed CDATA")? + 3;
            continue;
        }
        if rest.starts_with("<?") || rest.starts_with("</") || rest.starts_with("<!") {
            i += rest.find('>').ok_or("unclosed tag")? + 1;
            continue;
        }
        // start tag: name, then attributes until an unquoted '>'
        let mut j = i + 1;
        while j < b.len() && !b[j].is_ascii_whitespace() && b[j] != b'>' && b[j] != b'/' {
            j += 1;
        }
        let name = s[i + 1..j].to_ascii_lowercase();
        let mut attrs = vec![];
        loop {
            while j < b.len() && (b[j].is_ascii_whitespace() || b[j] == b'/') {
                j += 1;
            }
            if j >= b.len() {
                return Err("unclosed tag");
            }
            if b[j] == b'>' {
                j += 1;
                break;
            }
            let ks = j;
            while j < b.len() && !b[j].is_ascii_whitespace() && b[j] != b'=' && b[j] != b'>' && b[j] != b'/' {
                j += 1;
            }
            let key = s[ks..j].to_ascii_lowercase();
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            let mut val = String::new();
            if j < b.len() && b[j] == b'=' {
                j += 1;
                while j < b.len() && b[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j < b.len() && (b[j] == b'"' || b[j] == b'\'') {
                    let q = b[j];
                    let vs = j + 1;
                    j = vs + s[vs..].as_bytes().iter().position(|&c| c == q).ok_or("unclosed quote")?;
                    val = s[vs..j].to_string();
                    j += 1;
                } else {
                    let vs = j;
                    while j < b.len() && !b[j].is_ascii_whitespace() && b[j] != b'>' {
                        j += 1;
                    }
                    val = s[vs..j].to_string();
                }
            }
            if !key.is_empty() {
                attrs.push((key, val));
            }
        }
        out.push(Tag { name, attrs });
        i = j;
    }
    Ok(out)
}

/// A reference an SVG may contain: same-document fragments and embedded raster images only.
fn safe_ref(v: &str) -> bool {
    let v: String = v.chars().filter(|c| !c.is_whitespace() && !c.is_control()).collect::<String>().to_ascii_lowercase();
    v.is_empty() || v.starts_with('#') || ["data:image/png", "data:image/jpeg", "data:image/jpg", "data:image/webp", "data:image/gif"].iter().any(|p| v.starts_with(p))
}

/// Every `url(...)` in CSS text points inside the document (or at an embedded raster).
fn safe_css(css: &str) -> bool {
    let low = css.to_ascii_lowercase();
    if low.contains("@import") || low.contains("expression(") || low.contains("javascript:") || low.contains("behavior:") {
        return false;
    }
    let mut rest = low.as_str();
    while let Some(p) = rest.find("url(") {
        rest = &rest[p + 4..];
        let end = rest.find(')').unwrap_or(rest.len());
        let inner = rest[..end].trim().trim_matches(|c| c == '"' || c == '\'');
        if !safe_ref(inner) {
            return false;
        }
        rest = &rest[end..];
    }
    true
}

/// Checks an SVG upload. SVGs are rejected (not rewritten) when they contain anything that can run code or load
/// something from elsewhere: scripts, event handlers, `javascript:` links, foreignObject / iframe / embed /
/// object, DOCTYPE or entities, external `href` / `src`, or CSS `@import` / external `url()`.
pub fn check_svg(b: &[u8]) -> Result<(), String> {
    let s = std::str::from_utf8(b).map_err(|_| "The SVG is not valid UTF-8 text.".to_string())?;
    let low = s.to_ascii_lowercase();
    for (needle, why) in [
        ("<!doctype", "a DOCTYPE"),
        ("<!entity", "XML entities"),
        ("javascript:", "a javascript: link"),
        ("vbscript:", "a vbscript: link"),
        ("data:text", "an embedded document"),
        ("data:image/svg", "an embedded SVG"),
    ] {
        if low.contains(needle) {
            return Err(format!("The SVG contains {why}. Export it again without scripts or external references."));
        }
    }
    let tags = tags(s).map_err(|e| format!("The SVG is not well-formed ({e})."))?;
    if tags.first().is_none_or(|t| t.name != "svg") {
        return Err("The file is not an SVG image.".into());
    }
    // style blocks: the text between <style> and </style> (CDATA included)
    let mut rest = low.as_str();
    while let Some(p) = rest.find("<style") {
        rest = &rest[p..];
        let end = rest.find("</style").unwrap_or(rest.len());
        if !safe_css(&rest[..end]) {
            return Err("The SVG's styles load something from elsewhere. Remove @import and external url() references.".into());
        }
        rest = &rest[end..];
    }
    const BANNED: [&str; 8] = ["script", "foreignobject", "iframe", "embed", "object", "handler", "listener", "audio"];
    for t in &tags {
        let local = t.name.rsplit(':').next().unwrap_or(&t.name);
        if BANNED.contains(&local) || local == "video" {
            return Err(format!("The SVG contains a <{}> element, which isn't allowed.", t.name));
        }
        for (k, v) in &t.attrs {
            let local_k = k.rsplit(':').next().unwrap_or(k);
            if local_k.starts_with("on") {
                return Err(format!("The SVG contains an event handler ({k}), which isn't allowed."));
            }
            if matches!(local_k, "href" | "src" | "action" | "formaction") && !safe_ref(v) {
                return Err(format!("The SVG links to an external resource ({k}). Only #fragment and embedded PNG / JPEG / WebP images are allowed."));
            }
            if local_k == "style" && !safe_css(v) {
                return Err("The SVG's styles load something from elsewhere.".into());
            }
            // presentation attributes can carry url(): fill="url(#grad)" is fine, an external URL is not
            if v.to_ascii_lowercase().contains("url(") && !safe_css(v) {
                return Err(format!("The SVG attribute {k} points at an external resource."));
            }
        }
    }
    Ok(())
}

/// Width / height of an SVG from its root `width` / `height` (plain numbers or px), else its viewBox.
pub fn svg_size(b: &[u8]) -> (Option<i32>, Option<i32>) {
    let Ok(s) = std::str::from_utf8(b) else { return (None, None) };
    let Ok(tags) = tags(s) else { return (None, None) };
    let Some(root) = tags.into_iter().find(|t| t.name == "svg") else { return (None, None) };
    let get = |k: &str| root.attrs.iter().find(|(a, _)| a == k).map(|(_, v)| v.trim().to_string());
    let num = |v: Option<String>| v.and_then(|v| v.trim_end_matches("px").parse::<f64>().ok()).filter(|x| *x > 0.0 && *x < 100_000.0).map(|x| x.round() as i32);
    let (w, h) = (num(get("width")), num(get("height")));
    if w.is_some() && h.is_some() {
        return (w, h);
    }
    let vb: Vec<f64> = get("viewbox").unwrap_or_default().split(|c: char| c.is_whitespace() || c == ',').filter_map(|x| x.parse().ok()).collect();
    if vb.len() == 4 && vb[2] > 0.0 && vb[3] > 0.0 {
        return (Some(vb[2].round() as i32), Some(vb[3].round() as i32));
    }
    (w, h)
}

/* ------------------------------------------------------------------ */
/* Processing (pure; runs on a blocking thread)                        */
/* ------------------------------------------------------------------ */

/// One stored rendition of a file.
pub struct Rendition {
    pub variant: &'static str,
    pub mime: &'static str,
    pub ext: &'static str,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub bytes: Vec<u8>,
}

/// Validates an upload and produces what to store: the original (`orig`, bytes unchanged) and, for rasters
/// wider than each target, WebP variants (`sm` / `md` / `lg`). JPEG EXIF orientation is applied to the variants.
pub fn process(bytes: &[u8], max_bytes: usize) -> Result<Vec<Rendition>, ApiError> {
    if bytes.is_empty() {
        return Err(ApiError::bad("The file is empty."));
    }
    if bytes.len() > max_bytes {
        return Err(ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, "too_large", format!("Images can be up to {} MB.", max_bytes / 1024 / 1024)));
    }
    let Some((mime, ext)) = sniff(bytes) else {
        return Err(ApiError::new(StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported", "Only PNG, JPEG, WebP and SVG images can be uploaded."));
    };
    if mime == "image/svg+xml" {
        check_svg(bytes).map_err(ApiError::bad)?;
        let (w, h) = svg_size(bytes);
        return Ok(vec![Rendition { variant: "orig", mime, ext, width: w, height: h, bytes: bytes.to_vec() }]);
    }
    let img = decode(bytes).map_err(|e| ApiError::bad(format!("The image can't be read ({e}).")))?;
    let (w, h) = (img.width(), img.height());
    let mut out = vec![Rendition { variant: "orig", mime, ext, width: Some(w as i32), height: Some(h as i32), bytes: bytes.to_vec() }];
    for (name, target) in VARIANTS {
        if w <= target {
            continue;
        }
        let nh = ((h as f64) * (target as f64) / (w as f64)).round().max(1.0) as u32;
        let small = img.resize_exact(target, nh, image::imageops::FilterType::CatmullRom);
        out.push(Rendition { variant: name, mime: "image/webp", ext: "webp", width: Some(target as i32), height: Some(nh as i32), bytes: encode_webp(&small)? });
    }
    Ok(out)
}

fn decode(bytes: &[u8]) -> anyhow::Result<DynamicImage> {
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_PIXELS * 4 + 64 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    let (w, h) = decoder.dimensions();
    if (w as u64) * (h as u64) > MAX_PIXELS {
        anyhow::bail!("larger than {} megapixels", MAX_PIXELS / 1_000_000);
    }
    let orientation = decoder.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    Ok(img)
}

fn encode_webp(img: &DynamicImage) -> Result<Vec<u8>, ApiError> {
    let mem = if img.color().has_alpha() {
        let rgba = img.to_rgba8();
        webp::Encoder::from_rgba(rgba.as_raw(), rgba.width(), rgba.height()).encode(WEBP_QUALITY)
    } else {
        let rgb = img.to_rgb8();
        webp::Encoder::from_rgb(rgb.as_raw(), rgb.width(), rgb.height()).encode(WEBP_QUALITY)
    };
    if mem.is_empty() {
        return Err(anyhow::anyhow!("webp encoding produced no data").into());
    }
    Ok(mem.to_vec())
}

/// Content-hashed file id (per tenant: two brokers uploading the same picture get separate files).
pub fn file_id(tenant: &str, bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(tenant.as_bytes());
    h.update([0u8]);
    h.update(bytes);
    h.finalize().iter().take(16).map(|b| format!("{b:02x}")).collect()
}

fn new_asset_id() -> String {
    let mut b = [0u8; 9];
    getrandom::fill(&mut b).expect("OS randomness unavailable");
    format!("m{}", b.iter().map(|x| format!("{x:02x}")).collect::<String>())
}

pub fn is_asset_id(s: &str) -> bool {
    s.len() == 19 && s.starts_with('m') && s[1..].chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

fn is_file_id(s: &str) -> bool {
    s.len() == 32 && s.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

/* ------------------------------------------------------------------ */
/* Storage of files                                                    */
/* ------------------------------------------------------------------ */

/// Processes and stores an upload (deduplicated by content), returning its file id.
async fn store_file(st: &AppState, tenant: &str, staff: &str, bytes: Bytes) -> R<String> {
    let id = file_id(tenant, &bytes);
    let exists: Option<i32> = sqlx::query_scalar("SELECT 1 FROM media_files WHERE id = $1").bind(&id).fetch_optional(&st.pool).await?;
    if exists.is_some() {
        return Ok(id);
    }
    let max = st.cfg.max_upload_bytes;
    let len = bytes.len();
    let renditions = tokio::task::spawn_blocking(move || process(&bytes, max)).await.map_err(|e| anyhow::anyhow!(e))??;
    let mut variants = Map::new();
    let orig = &renditions[0];
    for r in &renditions {
        let key = format!("{tenant}/{id}/{}.{}", r.variant, r.ext);
        st.storage.put(&key, r.bytes.clone(), r.mime).await?;
        variants.insert(r.variant.into(), json!({"key": key, "mime": r.mime, "width": r.width, "height": r.height, "bytes": r.bytes.len()}));
    }
    sqlx::query("INSERT INTO media_files (id, tenant, mime, bytes, width, height, variants, created_by) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (id) DO NOTHING")
        .bind(&id)
        .bind(tenant)
        .bind(orig.mime)
        .bind(len as i64)
        .bind(orig.width)
        .bind(orig.height)
        .bind(Value::Object(variants))
        .bind(staff)
        .execute(&st.pool)
        .await?;
    Ok(id)
}

/* ------------------------------------------------------------------ */
/* Shaping                                                             */
/* ------------------------------------------------------------------ */

pub fn file_url(base: &str, id: &str, variant: &str) -> String {
    format!("{base}/v1/public/media/{id}/{variant}")
}

/// Public view of a file: URL of the original, variant URLs and a `srcset` (variants + original width).
pub fn file_json(base: &str, id: &str, mime: &str, bytes: i64, width: Option<i32>, height: Option<i32>, variants: &Value) -> Value {
    let mut vs = Map::new();
    let mut srcset: Vec<(i32, String)> = vec![];
    for (name, _) in VARIANTS {
        if let Some(v) = variants.get(name) {
            let w = v.get("width").and_then(Value::as_i64).unwrap_or(0) as i32;
            let url = file_url(base, id, name);
            srcset.push((w, format!("{url} {w}w")));
            vs.insert(name.into(), json!({"url": url, "width": w, "height": v.get("height"), "bytes": v.get("bytes")}));
        }
    }
    let url = file_url(base, id, "orig");
    if let Some(w) = width
        && mime != "image/svg+xml"
    {
        srcset.push((w, format!("{url} {w}w")));
    }
    srcset.sort_by_key(|x| x.0);
    json!({
        "id": id, "mime": mime, "bytes": bytes, "width": width, "height": height, "url": url,
        "variants": vs, "srcset": srcset.into_iter().map(|x| x.1).collect::<Vec<_>>().join(", "),
    })
}

/// Library assets of a tenant by id (deleted ones excluded), shaped for the API.
pub async fn assets_by_id(pool: &PgPool, base: &str, tenant: &str, ids: &[String]) -> R<HashMap<String, Value>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!("{ASSET_SELECT} WHERE a.tenant = $1 AND a.id = ANY($2) AND a.deleted_at IS NULL")))
        .bind(tenant)
        .bind(ids)
        .fetch_all(pool)
        .await?;
    Ok(rows.iter().map(|r| (r.get::<String, _>("id"), asset_json(base, r))).collect())
}

const ASSET_SELECT: &str = "SELECT a.id, a.name, a.alt, a.created_by, a.created_at, a.updated_by, a.updated_at,
    l.id AS l_id, l.mime AS l_mime, l.bytes AS l_bytes, l.width AS l_width, l.height AS l_height, l.variants AS l_variants,
    d.id AS d_id, d.mime AS d_mime, d.bytes AS d_bytes, d.width AS d_width, d.height AS d_height, d.variants AS d_variants
  FROM media_assets a JOIN media_files l ON l.id = a.light_file LEFT JOIN media_files d ON d.id = a.dark_file";

fn asset_json(base: &str, r: &sqlx::postgres::PgRow) -> Value {
    let light = file_json(base, &r.get::<String, _>("l_id"), &r.get::<String, _>("l_mime"), r.get("l_bytes"), r.get("l_width"), r.get("l_height"), &r.get::<Value, _>("l_variants"));
    let dark = r.get::<Option<String>, _>("d_id").map(|id| {
        file_json(base, &id, &r.get::<String, _>("d_mime"), r.get("d_bytes"), r.get("d_width"), r.get("d_height"), &r.get::<Value, _>("d_variants"))
    });
    json!({
        "id": r.get::<String, _>("id"), "name": r.get::<String, _>("name"), "alt": r.get::<Value, _>("alt"),
        "light": light, "dark": dark,
        "created_by": r.get::<String, _>("created_by"), "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updated_by": r.get::<String, _>("updated_by"), "updated_at": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    })
}

async fn asset_or_404(st: &AppState, tenant: &str, id: &str) -> R<Value> {
    if !is_asset_id(id) {
        return Err(ApiError::not_found("Image"));
    }
    assets_by_id(&st.pool, &st.cfg.public_url, tenant, &[id.to_string()]).await?.remove(id).ok_or_else(|| ApiError::not_found("Image"))
}

/// Where an asset is used: content entries whose draft or published value references it.
pub async fn usage(pool: &PgPool, tenant: &str, id: &str) -> R<Vec<Value>> {
    let rows = sqlx::query(
        "SELECT surface, page, slot, locale, COALESCE(published->>'media' = $2, false) AS live, COALESCE(draft->>'media' = $2, false) AS in_draft
           FROM content_entries WHERE tenant = $1 AND (published->>'media' = $2 OR draft->>'media' = $2)
          ORDER BY surface, page, slot, locale",
    )
    .bind(tenant)
    .bind(id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| json!({"surface": r.get::<String, _>(0), "page": r.get::<String, _>(1), "slot": r.get::<String, _>(2), "locale": r.get::<String, _>(3), "published": r.get::<bool, _>(4), "draft": r.get::<bool, _>(5)}))
        .collect())
}

fn clean_name(s: &str) -> String {
    let base = s.rsplit(['/', '\\']).next().unwrap_or(s);
    let base = base.rsplit_once('.').map(|x| x.0).filter(|b| !b.is_empty()).unwrap_or(base);
    let n: String = base.chars().filter(|c| !c.is_control()).take(120).collect::<String>().trim().to_string();
    if n.is_empty() { "Image".into() } else { n }
}

/* ------------------------------------------------------------------ */
/* Handlers                                                            */
/* ------------------------------------------------------------------ */

#[derive(Deserialize, Default)]
pub struct ListQ {
    q: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
    name: Option<String>,
    theme: Option<String>,
}

pub async fn list(State(st): State<AppState>, h: HeaderMap, Query(q): Query<ListQ>) -> R {
    api::require(&h, "content.read")?;
    let t = api::tenant(&h);
    let limit = q.limit.unwrap_or(60).clamp(1, 200);
    let offset = q.offset.unwrap_or(0).clamp(0, 100_000);
    let like = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(|s| format!("%{}%", s.replace(['%', '_'], "").to_lowercase()));
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "{ASSET_SELECT} WHERE a.tenant = $1 AND a.deleted_at IS NULL AND ($2::text IS NULL OR lower(a.name) LIKE $2) ORDER BY a.updated_at DESC LIMIT $3 OFFSET $4"
    )))
    .bind(&t)
    .bind(&like)
    .bind(limit)
    .bind(offset)
    .fetch_all(&st.pool)
    .await?;
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM media_assets a WHERE a.tenant = $1 AND a.deleted_at IS NULL AND ($2::text IS NULL OR lower(a.name) LIKE $2)")
        .bind(&t)
        .bind(&like)
        .fetch_one(&st.pool)
        .await?;
    Ok(Json(json!({"assets": rows.iter().map(|r| asset_json(&st.cfg.public_url, r)).collect::<Vec<_>>(), "total": total})))
}

/// New library entry from an upload. The same image uploaded again returns the existing entry (`existing: true`).
pub async fn upload(State(st): State<AppState>, h: HeaderMap, Query(q): Query<ListQ>, body: Bytes) -> R<Response> {
    let who = api::require(&h, "content.write")?;
    let t = api::tenant(&h);
    let file_name = h.get("x-file-name").and_then(|v| v.to_str().ok()).map(api::percent_decode).unwrap_or_default();
    let fid = store_file(&st, &t, &who, body).await?;
    let existing: Option<String> = sqlx::query_scalar("SELECT id FROM media_assets WHERE tenant = $1 AND light_file = $2 AND deleted_at IS NULL ORDER BY created_at LIMIT 1")
        .bind(&t)
        .bind(&fid)
        .fetch_optional(&st.pool)
        .await?;
    if let Some(id) = existing {
        let asset = asset_or_404(&st, &t, &id).await?;
        return Ok((StatusCode::OK, Json(json!({"asset": asset, "existing": true}))).into_response());
    }
    let id = new_asset_id();
    let name = clean_name(q.name.as_deref().filter(|s| !s.trim().is_empty()).unwrap_or(&file_name));
    sqlx::query("INSERT INTO media_assets (id, tenant, name, light_file, created_by, updated_by) VALUES ($1,$2,$3,$4,$5,$5)")
        .bind(&id)
        .bind(&t)
        .bind(&name)
        .bind(&fid)
        .bind(&who)
        .execute(&st.pool)
        .await?;
    let asset = asset_or_404(&st, &t, &id).await?;
    Ok((StatusCode::CREATED, Json(json!({"asset": asset, "existing": false}))).into_response())
}

pub async fn detail(State(st): State<AppState>, h: HeaderMap, Path(id): Path<String>) -> R {
    api::require(&h, "content.read")?;
    let t = api::tenant(&h);
    let asset = asset_or_404(&st, &t, &id).await?;
    Ok(Json(json!({"asset": asset, "usage": usage(&st.pool, &t, &id).await?})))
}

/// Name and alt text (`alt`: {locale: text}; an empty text removes that locale).
pub async fn update(State(st): State<AppState>, h: HeaderMap, Path(id): Path<String>, body: Bytes) -> R {
    let who = api::require(&h, "content.write")?;
    let t = api::tenant(&h);
    let cur = asset_or_404(&st, &t, &id).await?;
    let b = api::object(&body)?;
    let name = match b.get("name") {
        Some(Value::String(s)) => clean_name(s),
        None => cur["name"].as_str().unwrap_or("Image").to_string(),
        _ => return Err(ApiError::bad("name must be text.")),
    };
    let mut alt = cur["alt"].as_object().cloned().unwrap_or_default();
    if let Some(a) = b.get("alt") {
        let Some(a) = a.as_object() else { return Err(ApiError::bad("alt must be an object of locale → text.")) };
        for (loc, v) in a {
            if !LOCALES.contains(&loc.as_str()) {
                return Err(ApiError::bad(format!("Unknown locale {loc}.")));
            }
            let Some(text) = v.as_str() else { return Err(ApiError::bad("alt texts must be text.")) };
            let text: String = text.chars().filter(|c| !c.is_control()).take(300).collect::<String>().trim().to_string();
            if text.is_empty() {
                alt.remove(loc);
            } else {
                alt.insert(loc.clone(), Value::String(text));
            }
        }
    }
    sqlx::query("UPDATE media_assets SET name = $3, alt = $4, updated_by = $5, updated_at = now() WHERE tenant = $1 AND id = $2")
        .bind(&t)
        .bind(&id)
        .bind(&name)
        .bind(Value::Object(alt))
        .bind(&who)
        .execute(&st.pool)
        .await?;
    Ok(Json(json!({"asset": asset_or_404(&st, &t, &id).await?})))
}

/// The permission a file change needs: an asset shown on a live page changes clients' screens at once, so it
/// needs content.publish; an unused or draft-only asset only content.write.
async fn file_change_perm(st: &AppState, h: &HeaderMap, tenant: &str, id: &str) -> R<(String, bool)> {
    let live = usage(&st.pool, tenant, id).await?.iter().any(|u| u["published"] == true);
    let who = api::require(h, if live { "content.publish" } else { "content.write" })?;
    Ok((who, live))
}

fn theme(q: &ListQ) -> R<&'static str> {
    match q.theme.as_deref().unwrap_or("light") {
        "light" => Ok("light"),
        "dark" => Ok("dark"),
        _ => Err(ApiError::bad("theme must be light or dark.")),
    }
}

/// Replaces the light file or sets the dark-theme file of an asset.
pub async fn replace_file(State(st): State<AppState>, h: HeaderMap, Path(id): Path<String>, Query(q): Query<ListQ>, body: Bytes) -> R {
    let t = api::tenant(&h);
    let th = theme(&q)?;
    asset_or_404(&st, &t, &id).await?;
    let (who, live) = file_change_perm(&st, &h, &t, &id).await?;
    let fid = store_file(&st, &t, &who, body).await?;
    let col = if th == "dark" { "dark_file" } else { "light_file" };
    sqlx::query(sqlx::AssertSqlSafe(format!("UPDATE media_assets SET {col} = $3, updated_by = $4, updated_at = now() WHERE tenant = $1 AND id = $2")))
        .bind(&t)
        .bind(&id)
        .bind(&fid)
        .bind(&who)
        .execute(&st.pool)
        .await?;
    store::audit(&st.pool, &t, &who, "media.replace", &id, json!({"theme": th, "file": fid, "live": live})).await;
    Ok(Json(json!({"asset": asset_or_404(&st, &t, &id).await?})))
}

/// Removes the dark-theme file (the light file then shows in both themes).
pub async fn remove_file(State(st): State<AppState>, h: HeaderMap, Path(id): Path<String>, Query(q): Query<ListQ>) -> R {
    let t = api::tenant(&h);
    if theme(&q)? != "dark" {
        return Err(ApiError::bad("Only the dark file can be removed; replace the light file instead."));
    }
    asset_or_404(&st, &t, &id).await?;
    let (who, live) = file_change_perm(&st, &h, &t, &id).await?;
    sqlx::query("UPDATE media_assets SET dark_file = NULL, updated_by = $3, updated_at = now() WHERE tenant = $1 AND id = $2").bind(&t).bind(&id).bind(&who).execute(&st.pool).await?;
    store::audit(&st.pool, &t, &who, "media.remove_dark", &id, json!({"live": live})).await;
    Ok(Json(json!({"asset": asset_or_404(&st, &t, &id).await?})))
}

/// Deletes an asset from the library; refused while any draft or published content uses it.
pub async fn delete(State(st): State<AppState>, h: HeaderMap, Path(id): Path<String>) -> R {
    let who = api::require(&h, "content.publish")?;
    let t = api::tenant(&h);
    let asset = asset_or_404(&st, &t, &id).await?;
    let used = usage(&st.pool, &t, &id).await?;
    if !used.is_empty() {
        return Err(ApiError::conflict("in_use", "This image is used in content. Replace it there first.").with(json!({"usage": used})));
    }
    sqlx::query("UPDATE media_assets SET deleted_at = now(), updated_by = $3 WHERE tenant = $1 AND id = $2").bind(&t).bind(&id).bind(&who).execute(&st.pool).await?;
    store::audit(&st.pool, &t, &who, "media.delete", &id, json!({"name": asset["name"]})).await;
    Ok(Json(json!({"ok": true})))
}

/// Public file serving. `variant` may carry an extension (`md.webp`).
pub async fn serve(State(st): State<AppState>, Path((file, variant)): Path<(String, String)>) -> Response {
    let variant = variant.split('.').next().unwrap_or("").to_string();
    if !is_file_id(&file) || !matches!(variant.as_str(), "orig" | "sm" | "md" | "lg") {
        return ApiError::not_found("File").into_response();
    }
    let row = match sqlx::query("SELECT mime, variants FROM media_files WHERE id = $1").bind(&file).fetch_optional(&st.pool).await {
        Ok(Some(r)) => r,
        Ok(None) => return ApiError::not_found("File").into_response(),
        Err(e) => return ApiError::from(e).into_response(),
    };
    let variants: Value = row.get("variants");
    // the requested variant, else the next larger one, else the original
    let order = ["sm", "md", "lg", "orig"];
    let start = order.iter().position(|v| *v == variant).unwrap_or(3);
    let Some(v) = order[start..].iter().find_map(|k| variants.get(*k)) else {
        return ApiError::not_found("File").into_response();
    };
    let (Some(key), Some(mime)) = (v.get("key").and_then(Value::as_str), v.get("mime").and_then(Value::as_str)) else {
        return ApiError::not_found("File").into_response();
    };
    let bytes = match st.storage.get(key).await {
        Ok(Some(b)) => b,
        Ok(None) => return ApiError::not_found("File").into_response(),
        Err(e) => return ApiError::from(e).into_response(),
    };
    let mut res = bytes.into_response();
    let hd = res.headers_mut();
    hd.insert(header::CONTENT_TYPE, HeaderValue::from_str(mime).unwrap_or(HeaderValue::from_static("application/octet-stream")));
    hd.insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=31536000, immutable"));
    hd.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    hd.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    hd.insert("cross-origin-resource-policy", HeaderValue::from_static("cross-origin"));
    // an SVG opened on its own can't run anything (scripts were refused at upload; this is the second wall)
    hd.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("default-src 'none'; img-src data:; style-src 'unsafe-inline'; sandbox"));
    hd.insert(header::ETAG, HeaderValue::from_str(&format!("\"{file}-{variant}\"")).unwrap_or(HeaderValue::from_static("\"x\"")));
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_allowed_types_only() {
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\nrest").unwrap().0, "image/png");
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]).unwrap().0, "image/jpeg");
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 ").unwrap().0, "image/webp");
        assert_eq!(sniff(b"<?xml version=\"1.0\"?>\n<!-- x --><svg xmlns=\"http://www.w3.org/2000/svg\"/>").unwrap().0, "image/svg+xml");
        assert_eq!(sniff("\u{feff}  <svg>".as_bytes()).unwrap().0, "image/svg+xml");
        assert!(sniff(b"GIF89a....").is_none());
        assert!(sniff(b"%PDF-1.7").is_none());
        assert!(sniff(b"<html><svg></svg></html>").is_none());
        assert!(sniff(b"<svgfoo>").is_none());
        assert!(sniff(b"MZ\x90\0").is_none());
    }

    #[test]
    fn clean_svgs_pass() {
        let ok = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 120 40">
            <defs><linearGradient id="g"><stop offset="0" stop-color="#f60"/></linearGradient>
            <style><![CDATA[ .a { fill: url(#g); } ]]></style></defs>
            <!-- <script>ignored comment text is not markup</script> -->
            <rect class="a" width="120" height="40" fill="url(#g)"/><use xlink:href="#g"/>
            <image href="data:image/png;base64,iVBORw0KGgo="/></svg>"##;
        assert_eq!(check_svg(ok.as_bytes()), Ok(()));
        assert_eq!(svg_size(ok.as_bytes()), (Some(120), Some(40)));
        assert_eq!(svg_size(br#"<svg width="64px" height="32" viewBox="0 0 1 1"/>"#), (Some(64), Some(32)));
    }

    #[test]
    fn dangerous_svgs_are_rejected() {
        let bad = [
            r#"<svg><script>alert(1)</script></svg>"#,
            r#"<svg><SCRIPT href="x.js"/></svg>"#,
            r#"<svg onload="alert(1)"></svg>"#,
            r#"<svg><rect ONCLICK='x()'/></svg>"#,
            r#"<svg><a href="javascript:alert(1)"><text>x</text></a></svg>"#,
            r#"<svg><a href=" java&#x09;script:alert(1)">x</a></svg>"#,
            r#"<svg><image href="https://evil.example/x.png"/></svg>"#,
            r#"<svg><use xlink:href="other.svg#a"/></svg>"#,
            r#"<svg><foreignObject><div>x</div></foreignObject></svg>"#,
            r#"<svg><style>@import url(https://evil.example/a.css);</style></svg>"#,
            r#"<svg><rect style="fill:url(https://evil.example/p)"/></svg>"#,
            r#"<svg><rect fill="url(http://evil.example/#p)"/></svg>"#,
            r#"<!DOCTYPE svg [<!ENTITY x "y">]><svg>&x;</svg>"#,
            r#"<svg><image href="data:image/svg+xml;base64,PHN2Zz4="/></svg>"#,
            r#"<svg><iframe src="x"/></svg>"#,
            r#"<svg><rect width="1"#,
            r#"<html><body/></html>"#,
        ];
        for b in bad {
            assert!(check_svg(b.as_bytes()).is_err(), "should reject: {b}");
        }
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_fn(w, h, |x, y| image::Rgba([(x % 256) as u8, (y % 256) as u8, 120, 255]));
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn makes_webp_variants_for_wide_rasters() {
        let r = process(&png(1200, 600), 10 << 20).ok().unwrap();
        let names: Vec<_> = r.iter().map(|x| x.variant).collect();
        assert_eq!(names, ["orig", "sm", "md"], "no lg: the original is narrower than 1920");
        assert_eq!(r[0].mime, "image/png");
        assert_eq!((r[1].width, r[1].height), (Some(480), Some(240)));
        assert_eq!((r[2].width, r[2].height), (Some(1024), Some(512)));
        for v in &r[1..] {
            assert_eq!(v.mime, "image/webp");
            assert!(v.bytes.len() > 12 && &v.bytes[0..4] == b"RIFF" && &v.bytes[8..12] == b"WEBP");
            let back = image::load_from_memory(&v.bytes).unwrap();
            assert_eq!(back.width() as i32, v.width.unwrap());
        }
        // small images keep only the original
        assert_eq!(process(&png(300, 300), 10 << 20).ok().unwrap().len(), 1);
    }

    #[test]
    fn rejects_bad_uploads() {
        let e = |r: Result<Vec<Rendition>, ApiError>| r.err().map(|e| e.1).unwrap_or("ok");
        assert_eq!(e(process(b"", 100)), "validation");
        assert_eq!(e(process(&png(10, 10), 10)), "too_large");
        assert_eq!(e(process(b"%PDF-1.7 hello", 1 << 20)), "unsupported");
        // a PNG signature with garbage after it
        assert_eq!(e(process(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDRgarbage", 1 << 20)), "validation");
        assert_eq!(e(process(b"<svg onload=\"x()\"/>", 1 << 20)), "validation");
    }

    #[test]
    fn ids() {
        assert_eq!(file_id("a", b"x"), file_id("a", b"x"));
        assert_ne!(file_id("a", b"x"), file_id("b", b"x"), "per tenant");
        assert!(is_file_id(&file_id("a", b"x")));
        assert!(is_asset_id(&new_asset_id()));
        assert!(!is_asset_id("m../../etc"));
        assert_eq!(clean_name("C:\\pics\\Hero banner.final.png"), "Hero banner.final");
    }
}
