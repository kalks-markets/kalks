//! Content slots: (tenant, surface, page, slot, locale) entries with a draft and a published value, versions
//! (publish / rollback / unpublish; the last 50 per slot locale are kept), English fallback and the public read.
//!
//! Value shapes by kind (what `PUT entry` takes and the public read returns in `value`):
//!
//! | kind          | value                                     | limits                                         |
//! |---------------|-------------------------------------------|------------------------------------------------|
//! | `text`        | `{text}`                                  | 2 000 characters                               |
//! | `placeholder` | `{text}` (input placeholders, short hints) | 200 characters                                |
//! | `rich_text`   | `{markdown}`                              | 20 000 characters (the apps render it)          |
//! | `link`        | `{label, href}`                           | href: `/path`, `#anchor`, `https://`, `http://`, `mailto:` |
//! | `image`       | `{media}` (library asset id)              | read back resolved: `{media, alt, width, height, url, srcset, variants, dark}` |

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::collections::{BTreeMap, HashSet};

use crate::api::{self, ApiError, AppState, R};
use crate::{media, preview, store};

pub const SURFACES: [&str; 4] = ["website", "app", "trader", "email"];
/// The 22 platform locales; English is the fallback for every other.
pub const LOCALES: [&str; 22] = ["en", "hi", "ar", "ur", "fa", "es", "pt", "fr", "de", "it", "ru", "tr", "id", "ms", "vi", "th", "zh", "ja", "ko", "bn", "ta", "sw"];
pub const BASE_LOCALE: &str = "en";
pub const KINDS: [&str; 5] = ["text", "rich_text", "image", "link", "placeholder"];
/// Versions kept per slot locale.
pub const KEEP_VERSIONS: i64 = 50;

/* ------------------------------------------------------------------ */
/* Keys and values (pure)                                              */
/* ------------------------------------------------------------------ */

/// Page key: `home`, `global`, `markets-forex`, `auth` …
pub fn is_page(s: &str) -> bool {
    (1..=64).contains(&s.len()) && s.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit()) && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Slot key: dotted lowercase path, e.g. `hero.title`, `faq.3.answer`.
pub fn is_slot(s: &str) -> bool {
    (1..=96).contains(&s.len()) && s.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit()) && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.'))
}

/// A requested locale mapped onto the platform list (`pt-BR` → `pt`, `zh_Hans` → `zh`); unknown → English.
pub fn norm_locale(s: &str) -> &'static str {
    let base = s.trim().split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
    LOCALES.iter().find(|l| **l == base).copied().unwrap_or(BASE_LOCALE)
}

fn text_of(v: &Map<String, Value>, k: &str, max: usize, multiline: bool) -> Result<String, String> {
    let Some(s) = v.get(k).and_then(Value::as_str) else { return Err(format!("`{k}` is required text.")) };
    let s: String = s.chars().filter(|c| !c.is_control() || (multiline && (*c == '\n' || *c == '\t'))).collect::<String>().replace("\r\n", "\n");
    let s = s.trim().to_string();
    if s.chars().count() > max {
        return Err(format!("`{k}` can be up to {max} characters."));
    }
    Ok(s)
}

fn safe_href(h: &str) -> bool {
    let low = h.to_ascii_lowercase();
    (h.starts_with('/') && !h.starts_with("//")) || h.starts_with('#') || low.starts_with("https://") || low.starts_with("http://") || low.starts_with("mailto:")
}

/// Validates and normalises a value for a kind (image references are checked against the library separately).
pub fn validate_value(kind: &str, v: &Value) -> Result<Value, String> {
    let Some(o) = v.as_object() else { return Err("value must be an object.".into()) };
    match kind {
        "text" => Ok(json!({"text": text_of(o, "text", 2000, true)?})),
        "placeholder" => Ok(json!({"text": text_of(o, "text", 200, false)?})),
        "rich_text" => Ok(json!({"markdown": text_of(o, "markdown", 20_000, true)?})),
        "link" => {
            let label = text_of(o, "label", 200, false)?;
            let href = text_of(o, "href", 1000, false)?;
            if !safe_href(&href) || href.chars().any(char::is_whitespace) {
                return Err("href must be a /path, #anchor, https://, http:// or mailto: link.".into());
            }
            Ok(json!({"label": label, "href": href}))
        }
        "image" => {
            let id = o.get("media").and_then(Value::as_str).unwrap_or("");
            if !media::is_asset_id(id) {
                return Err("`media` must be a media library id.".into());
            }
            Ok(json!({"media": id}))
        }
        _ => Err(format!("Unknown kind {kind}.")),
    }
}

/// Which stored value a reader gets for one slot: the locale's own (draft first when previewing), else English's.
/// `rows`: locale → (draft, published).
pub fn pick<'a>(rows: &'a BTreeMap<String, (Option<Value>, Option<Value>)>, locale: &str, preview: bool) -> Option<(&'a str, &'a Value)> {
    let get = |l: &str| {
        rows.get_key_value(l).and_then(|(k, (d, p))| {
            let v = if preview { d.as_ref().or(p.as_ref()) } else { p.as_ref() };
            v.map(|v| (k.as_str(), v))
        })
    };
    get(locale).or_else(|| if locale != BASE_LOCALE { get(BASE_LOCALE) } else { None })
}

/* ------------------------------------------------------------------ */
/* Request shapes                                                      */
/* ------------------------------------------------------------------ */

#[derive(Deserialize, Default)]
pub struct KeyQ {
    surface: Option<String>,
    page: Option<String>,
    slot: Option<String>,
    locale: Option<String>,
    tenant: Option<String>,
    preview: Option<String>,
}

fn surface_of(s: Option<&str>) -> R<String> {
    let s = s.unwrap_or("").trim();
    if SURFACES.contains(&s) { Ok(s.to_string()) } else { Err(ApiError::bad(format!("surface must be one of {}.", SURFACES.join(", ")))) }
}

fn page_of(s: Option<&str>) -> R<String> {
    let s = s.unwrap_or("").trim();
    if is_page(s) { Ok(s.to_string()) } else { Err(ApiError::bad("page must be a lowercase key (letters, digits, - and _).")) }
}

fn slot_of(s: Option<&str>) -> R<String> {
    let s = s.unwrap_or("").trim();
    if is_slot(s) { Ok(s.to_string()) } else { Err(ApiError::bad("slot must be a lowercase dotted key, e.g. hero.title.")) }
}

fn locale_of(s: Option<&str>) -> R<String> {
    let s = s.unwrap_or("").trim();
    if LOCALES.contains(&s) { Ok(s.to_string()) } else { Err(ApiError::bad(format!("locale must be one of {}.", LOCALES.join(", ")))) }
}

fn s<'a>(b: &'a Map<String, Value>, k: &str) -> Option<&'a str> {
    b.get(k).and_then(Value::as_str)
}

/// (surface, page, slot, locale) from a JSON body.
fn key4(b: &Map<String, Value>) -> R<(String, String, String, String)> {
    Ok((surface_of(s(b, "surface"))?, page_of(s(b, "page"))?, slot_of(s(b, "slot"))?, locale_of(s(b, "locale"))?))
}

fn target(surface: &str, page: &str, slot: &str, locale: Option<&str>) -> String {
    match locale {
        Some(l) => format!("{surface}/{page}/{slot}@{l}"),
        None => format!("{surface}/{page}/{slot}"),
    }
}

/* ------------------------------------------------------------------ */
/* Staff: reads                                                        */
/* ------------------------------------------------------------------ */

pub async fn meta(h: HeaderMap) -> R {
    api::require(&h, "content.read")?;
    Ok(Json(json!({"surfaces": SURFACES, "locales": LOCALES, "base_locale": BASE_LOCALE, "kinds": KINDS, "keep_versions": KEEP_VERSIONS})))
}

/// Every slot of a tenant (optionally one surface) grouped by page, with per-locale state.
pub async fn tree(State(st): State<AppState>, h: HeaderMap, Query(q): Query<KeyQ>) -> R {
    api::require(&h, "content.read")?;
    let t = api::tenant(&h);
    let surface = match q.surface.as_deref() {
        Some(x) if !x.is_empty() => Some(surface_of(Some(x))?),
        _ => None,
    };
    let slots = sqlx::query("SELECT surface, page, slot, kind, label FROM content_slots WHERE tenant = $1 AND ($2::text IS NULL OR surface = $2) ORDER BY surface, page, slot")
        .bind(&t)
        .bind(&surface)
        .fetch_all(&st.pool)
        .await?;
    let entries = sqlx::query(
        "SELECT surface, page, slot, locale, draft IS NOT NULL, published IS NOT NULL, version, draft_at, published_at
           FROM content_entries WHERE tenant = $1 AND ($2::text IS NULL OR surface = $2)",
    )
    .bind(&t)
    .bind(&surface)
    .fetch_all(&st.pool)
    .await?;
    let mut states: BTreeMap<(String, String, String), Map<String, Value>> = BTreeMap::new();
    for r in &entries {
        states.entry((r.get(0), r.get(1), r.get(2))).or_default().insert(
            r.get::<String, _>(3),
            json!({"draft": r.get::<bool, _>(4), "published": r.get::<bool, _>(5), "version": r.get::<i32, _>(6),
                   "draft_at": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>(7), "published_at": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>(8)}),
        );
    }
    let mut pages: Vec<Value> = vec![];
    for r in &slots {
        let (su, pg, sl): (String, String, String) = (r.get(0), r.get(1), r.get(2));
        let locales = states.remove(&(su.clone(), pg.clone(), sl.clone())).unwrap_or_default();
        let item = json!({"slot": sl, "kind": r.get::<String, _>(3), "label": r.get::<String, _>(4), "locales": locales});
        match pages.last_mut() {
            Some(p) if p["surface"] == su.as_str() && p["page"] == pg.as_str() => p["slots"].as_array_mut().unwrap().push(item),
            _ => pages.push(json!({"surface": su, "page": pg, "slots": [item]})),
        }
    }
    Ok(Json(json!({"tenant": t, "pages": pages})))
}

async fn slot_row(pool: &PgPool, t: &str, su: &str, pg: &str, sl: &str) -> R<Option<(String, String)>> {
    Ok(sqlx::query("SELECT kind, label FROM content_slots WHERE tenant = $1 AND surface = $2 AND page = $3 AND slot = $4")
        .bind(t)
        .bind(su)
        .bind(pg)
        .bind(sl)
        .fetch_optional(pool)
        .await?
        .map(|r| (r.get(0), r.get(1))))
}

/// One slot with every locale's draft / published value and the library assets they reference.
pub async fn slot_view(State(st): State<AppState>, h: HeaderMap, Query(q): Query<KeyQ>) -> R {
    api::require(&h, "content.read")?;
    let t = api::tenant(&h);
    let (su, pg, sl) = (surface_of(q.surface.as_deref())?, page_of(q.page.as_deref())?, slot_of(q.slot.as_deref())?);
    let Some((kind, label)) = slot_row(&st.pool, &t, &su, &pg, &sl).await? else { return Err(ApiError::not_found("Slot")) };
    let rows = sqlx::query(
        "SELECT locale, draft, draft_by, draft_at, published, version, published_by, published_at FROM content_entries
          WHERE tenant = $1 AND surface = $2 AND page = $3 AND slot = $4",
    )
    .bind(&t)
    .bind(&su)
    .bind(&pg)
    .bind(&sl)
    .fetch_all(&st.pool)
    .await?;
    let mut entries = Map::new();
    let mut ids = HashSet::new();
    for r in &rows {
        let (d, p): (Option<Value>, Option<Value>) = (r.get(1), r.get(4));
        for v in [&d, &p].into_iter().flatten() {
            if let Some(id) = v.get("media").and_then(Value::as_str) {
                ids.insert(id.to_string());
            }
        }
        entries.insert(
            r.get::<String, _>(0),
            json!({"draft": d, "draft_by": r.get::<Option<String>, _>(2), "draft_at": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>(3),
                   "published": p, "version": r.get::<i32, _>(5), "published_by": r.get::<Option<String>, _>(6), "published_at": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>(7)}),
        );
    }
    let ids: Vec<String> = ids.into_iter().collect();
    let media = media::assets_by_id(&st.pool, &st.cfg.public_url, &t, &ids).await?;
    Ok(Json(json!({"slot": {"surface": su, "page": pg, "slot": sl, "kind": kind, "label": label}, "entries": entries, "media": media})))
}

pub async fn versions(State(st): State<AppState>, h: HeaderMap, Query(q): Query<KeyQ>) -> R {
    api::require(&h, "content.read")?;
    let t = api::tenant(&h);
    let (su, pg, sl, lo) = (surface_of(q.surface.as_deref())?, page_of(q.page.as_deref())?, slot_of(q.slot.as_deref())?, locale_of(q.locale.as_deref())?);
    let rows = sqlx::query(
        "SELECT version, action, from_version, value, by_staff, at FROM content_versions
          WHERE tenant = $1 AND surface = $2 AND page = $3 AND slot = $4 AND locale = $5 ORDER BY version DESC LIMIT $6",
    )
    .bind(&t)
    .bind(&su)
    .bind(&pg)
    .bind(&sl)
    .bind(&lo)
    .bind(KEEP_VERSIONS)
    .fetch_all(&st.pool)
    .await?;
    let current: Option<i32> = sqlx::query_scalar("SELECT version FROM content_entries WHERE tenant = $1 AND surface = $2 AND page = $3 AND slot = $4 AND locale = $5")
        .bind(&t)
        .bind(&su)
        .bind(&pg)
        .bind(&sl)
        .bind(&lo)
        .fetch_optional(&st.pool)
        .await?;
    let list: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({"version": r.get::<i32, _>(0), "action": r.get::<String, _>(1), "from_version": r.get::<Option<i32>, _>(2), "value": r.get::<Option<Value>, _>(3),
                   "by": r.get::<String, _>(4), "at": r.get::<chrono::DateTime<chrono::Utc>, _>(5)})
        })
        .collect();
    Ok(Json(json!({"versions": list, "current": current.unwrap_or(0)})))
}

pub async fn audit(State(st): State<AppState>, h: HeaderMap) -> R {
    api::require(&h, "content.read")?;
    let t = api::tenant(&h);
    let rows = sqlx::query("SELECT staff, action, target, detail, at FROM content_audit WHERE tenant = $1 ORDER BY at DESC, id DESC LIMIT 100").bind(&t).fetch_all(&st.pool).await?;
    let list: Vec<Value> = rows
        .iter()
        .map(|r| json!({"staff": r.get::<String, _>(0), "action": r.get::<String, _>(1), "target": r.get::<String, _>(2), "detail": r.get::<Value, _>(3), "at": r.get::<chrono::DateTime<chrono::Utc>, _>(4)}))
        .collect();
    Ok(Json(json!({"audit": list})))
}

/* ------------------------------------------------------------------ */
/* Staff: drafts                                                       */
/* ------------------------------------------------------------------ */

/// One entry to save as a draft.
struct DraftIn {
    surface: String,
    page: String,
    slot: String,
    locale: String,
    kind: String,
    label: Option<String>,
    value: Value,
}

fn draft_in(surface: String, b: &Map<String, Value>) -> R<DraftIn> {
    let kind = s(b, "kind").unwrap_or("").to_string();
    if !KINDS.contains(&kind.as_str()) {
        return Err(ApiError::bad(format!("kind must be one of {}.", KINDS.join(", "))));
    }
    let value = validate_value(&kind, b.get("value").unwrap_or(&Value::Null)).map_err(ApiError::bad)?;
    let label = s(b, "label").map(|l| l.chars().filter(|c| !c.is_control()).take(120).collect::<String>().trim().to_string());
    Ok(DraftIn { surface, page: page_of(s(b, "page"))?, slot: slot_of(s(b, "slot"))?, locale: locale_of(s(b, "locale"))?, kind, label, value })
}

/// Saves a draft (creating the slot on first use). A draft equal to the published value clears the draft.
async fn save_one(tx: &mut Transaction<'_, Postgres>, t: &str, who: &str, d: &DraftIn) -> R<()> {
    let existing: Option<String> = sqlx::query_scalar("SELECT kind FROM content_slots WHERE tenant = $1 AND surface = $2 AND page = $3 AND slot = $4 FOR UPDATE")
        .bind(t)
        .bind(&d.surface)
        .bind(&d.page)
        .bind(&d.slot)
        .fetch_optional(&mut **tx)
        .await?;
    match existing {
        Some(k) if k != d.kind => return Err(ApiError::conflict("kind_mismatch", format!("{} is a {k} slot.", target(&d.surface, &d.page, &d.slot, None)))),
        Some(_) => {
            if let Some(l) = d.label.as_ref().filter(|l| !l.is_empty()) {
                sqlx::query("UPDATE content_slots SET label = $5 WHERE tenant = $1 AND surface = $2 AND page = $3 AND slot = $4")
                    .bind(t)
                    .bind(&d.surface)
                    .bind(&d.page)
                    .bind(&d.slot)
                    .bind(l)
                    .execute(&mut **tx)
                    .await?;
            }
        }
        None => {
            sqlx::query("INSERT INTO content_slots (tenant, surface, page, slot, kind, label, created_by) VALUES ($1,$2,$3,$4,$5,$6,$7)")
                .bind(t)
                .bind(&d.surface)
                .bind(&d.page)
                .bind(&d.slot)
                .bind(&d.kind)
                .bind(d.label.clone().unwrap_or_default())
                .bind(who)
                .execute(&mut **tx)
                .await?;
        }
    }
    sqlx::query(
        "INSERT INTO content_entries (tenant, surface, page, slot, locale, draft, draft_by, draft_at) VALUES ($1,$2,$3,$4,$5,$6,$7, now())
         ON CONFLICT (tenant, surface, page, slot, locale) DO UPDATE SET
           draft = CASE WHEN content_entries.published = EXCLUDED.draft THEN NULL ELSE EXCLUDED.draft END,
           draft_by = EXCLUDED.draft_by, draft_at = EXCLUDED.draft_at",
    )
    .bind(t)
    .bind(&d.surface)
    .bind(&d.page)
    .bind(&d.slot)
    .bind(&d.locale)
    .bind(&d.value)
    .bind(who)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Image values must point at a live asset of this tenant.
async fn check_media(pool: &PgPool, t: &str, values: &[&Value]) -> R<()> {
    let ids: Vec<String> = values.iter().filter_map(|v| v.get("media").and_then(Value::as_str)).map(str::to_string).collect();
    if ids.is_empty() {
        return Ok(());
    }
    let found: Vec<String> = sqlx::query_scalar("SELECT id FROM media_assets WHERE tenant = $1 AND id = ANY($2) AND deleted_at IS NULL").bind(t).bind(&ids).fetch_all(pool).await?;
    if let Some(missing) = ids.iter().find(|i| !found.contains(i)) {
        return Err(ApiError::bad(format!("Image {missing} is not in the media library.")));
    }
    Ok(())
}

pub async fn save_draft(State(st): State<AppState>, h: HeaderMap, body: Bytes) -> R {
    let who = api::require(&h, "content.write")?;
    let t = api::tenant(&h);
    let b = api::object(&body)?;
    let d = draft_in(surface_of(s(&b, "surface"))?, &b)?;
    check_media(&st.pool, &t, &[&d.value]).await?;
    let mut tx = st.pool.begin().await?;
    save_one(&mut tx, &t, &who, &d).await?;
    tx.commit().await?;
    Ok(Json(json!({"ok": true})))
}

pub async fn discard(State(st): State<AppState>, h: HeaderMap, body: Bytes) -> R {
    api::require(&h, "content.write")?;
    let t = api::tenant(&h);
    let (su, pg, sl, lo) = key4(&api::object(&body)?)?;
    sqlx::query("UPDATE content_entries SET draft = NULL, draft_by = NULL, draft_at = NULL WHERE tenant = $1 AND surface = $2 AND page = $3 AND slot = $4 AND locale = $5")
        .bind(&t)
        .bind(&su)
        .bind(&pg)
        .bind(&sl)
        .bind(&lo)
        .execute(&st.pool)
        .await?;
    // an entry with neither a draft nor a published value or history is just noise
    sqlx::query(
        "DELETE FROM content_entries e WHERE tenant = $1 AND surface = $2 AND page = $3 AND slot = $4 AND locale = $5 AND draft IS NULL AND published IS NULL
           AND NOT EXISTS (SELECT 1 FROM content_versions v WHERE v.tenant = e.tenant AND v.surface = e.surface AND v.page = e.page AND v.slot = e.slot AND v.locale = e.locale)",
    )
    .bind(&t)
    .bind(&su)
    .bind(&pg)
    .bind(&sl)
    .bind(&lo)
    .execute(&st.pool)
    .await?;
    Ok(Json(json!({"ok": true})))
}

/* ------------------------------------------------------------------ */
/* Staff: publish, unpublish, rollback, delete                         */
/* ------------------------------------------------------------------ */

/// Records a new published value (or NULL) as the next version of a slot locale and prunes old versions.
async fn set_published(tx: &mut Transaction<'_, Postgres>, t: &str, who: &str, key: (&str, &str, &str, &str), value: Option<&Value>, action: &str, from: Option<i32>) -> R<i32> {
    let (su, pg, sl, lo) = key;
    let next: i32 = sqlx::query_scalar("SELECT COALESCE(max(version), 0) + 1 FROM content_versions WHERE tenant = $1 AND surface = $2 AND page = $3 AND slot = $4 AND locale = $5")
        .bind(t)
        .bind(su)
        .bind(pg)
        .bind(sl)
        .bind(lo)
        .fetch_one(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO content_versions (tenant, surface, page, slot, locale, version, action, from_version, value, by_staff) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
        .bind(t)
        .bind(su)
        .bind(pg)
        .bind(sl)
        .bind(lo)
        .bind(next)
        .bind(action)
        .bind(from)
        .bind(value)
        .bind(who)
        .execute(&mut **tx)
        .await?;
    let clear_draft = action == "publish";
    sqlx::query(
        "UPDATE content_entries SET published = $6, version = $7, published_by = $8, published_at = now(),
                draft = CASE WHEN $9 THEN NULL ELSE draft END, draft_by = CASE WHEN $9 THEN NULL ELSE draft_by END, draft_at = CASE WHEN $9 THEN NULL ELSE draft_at END
          WHERE tenant = $1 AND surface = $2 AND page = $3 AND slot = $4 AND locale = $5",
    )
    .bind(t)
    .bind(su)
    .bind(pg)
    .bind(sl)
    .bind(lo)
    .bind(value)
    .bind(next)
    .bind(who)
    .bind(clear_draft)
    .execute(&mut **tx)
    .await?;
    sqlx::query("DELETE FROM content_versions WHERE tenant = $1 AND surface = $2 AND page = $3 AND slot = $4 AND locale = $5 AND version <= $6")
        .bind(t)
        .bind(su)
        .bind(pg)
        .bind(sl)
        .bind(lo)
        .bind(next as i64 - KEEP_VERSIONS)
        .execute(&mut **tx)
        .await?;
    Ok(next)
}

/// Publishes every draft in scope (a page, one slot, one slot locale). Returns what was published.
async fn publish_scope(tx: &mut Transaction<'_, Postgres>, t: &str, who: &str, su: &str, pg: &str, sl: Option<&str>, lo: Option<&str>) -> R<Vec<Value>> {
    let rows = sqlx::query(
        "SELECT slot, locale, draft FROM content_entries WHERE tenant = $1 AND surface = $2 AND page = $3 AND draft IS NOT NULL
           AND ($4::text IS NULL OR slot = $4) AND ($5::text IS NULL OR locale = $5) ORDER BY slot, locale FOR UPDATE",
    )
    .bind(t)
    .bind(su)
    .bind(pg)
    .bind(sl)
    .bind(lo)
    .fetch_all(&mut **tx)
    .await?;
    let mut done = vec![];
    for r in &rows {
        let (slot, locale, draft): (String, String, Value) = (r.get(0), r.get(1), r.get(2));
        let v = set_published(tx, t, who, (su, pg, &slot, &locale), Some(&draft), "publish", None).await?;
        done.push(json!({"slot": slot, "locale": locale, "version": v}));
    }
    Ok(done)
}

pub async fn publish(State(st): State<AppState>, h: HeaderMap, body: Bytes) -> R {
    let who = api::require(&h, "content.publish")?;
    let t = api::tenant(&h);
    let b = api::object(&body)?;
    let (su, pg) = (surface_of(s(&b, "surface"))?, page_of(s(&b, "page"))?);
    let sl = s(&b, "slot").map(|x| slot_of(Some(x))).transpose()?;
    let lo = s(&b, "locale").map(|x| locale_of(Some(x))).transpose()?;
    // drafts may point at an image deleted since they were saved
    let drafts: Vec<Value> = sqlx::query_scalar(
        "SELECT draft FROM content_entries WHERE tenant = $1 AND surface = $2 AND page = $3 AND draft IS NOT NULL AND ($4::text IS NULL OR slot = $4) AND ($5::text IS NULL OR locale = $5)",
    )
    .bind(&t)
    .bind(&su)
    .bind(&pg)
    .bind(&sl)
    .bind(&lo)
    .fetch_all(&st.pool)
    .await?;
    check_media(&st.pool, &t, &drafts.iter().collect::<Vec<_>>()).await?;
    let mut tx = st.pool.begin().await?;
    let done = publish_scope(&mut tx, &t, &who, &su, &pg, sl.as_deref(), lo.as_deref()).await?;
    if !done.is_empty() {
        store::audit(&mut *tx, &t, &who, "content.publish", &format!("{su}/{pg}"), json!({"entries": done})).await;
    }
    tx.commit().await?;
    Ok(Json(json!({"published": done})))
}

pub async fn unpublish(State(st): State<AppState>, h: HeaderMap, body: Bytes) -> R {
    let who = api::require(&h, "content.publish")?;
    let t = api::tenant(&h);
    let (su, pg, sl, lo) = key4(&api::object(&body)?)?;
    let mut tx = st.pool.begin().await?;
    let cur: Option<Option<Value>> = sqlx::query_scalar("SELECT published FROM content_entries WHERE tenant = $1 AND surface = $2 AND page = $3 AND slot = $4 AND locale = $5 FOR UPDATE")
        .bind(&t)
        .bind(&su)
        .bind(&pg)
        .bind(&sl)
        .bind(&lo)
        .fetch_optional(&mut *tx)
        .await?;
    if !matches!(cur, Some(Some(_))) {
        return Err(ApiError::conflict("not_published", "This entry isn't published."));
    }
    let v = set_published(&mut tx, &t, &who, (&su, &pg, &sl, &lo), None, "unpublish", None).await?;
    store::audit(&mut *tx, &t, &who, "content.unpublish", &target(&su, &pg, &sl, Some(&lo)), json!({"version": v})).await;
    tx.commit().await?;
    Ok(Json(json!({"version": v})))
}

/// Publishes the value of an earlier version again (as a new version; the draft is kept).
pub async fn rollback(State(st): State<AppState>, h: HeaderMap, body: Bytes) -> R {
    let who = api::require(&h, "content.publish")?;
    let t = api::tenant(&h);
    let b = api::object(&body)?;
    let (su, pg, sl, lo) = key4(&b)?;
    let Some(ver) = b.get("version").and_then(Value::as_i64).filter(|v| *v > 0 && *v < i32::MAX as i64) else { return Err(ApiError::bad("version is required.")) };
    let mut tx = st.pool.begin().await?;
    let _lock: Option<i32> = sqlx::query_scalar("SELECT version FROM content_entries WHERE tenant = $1 AND surface = $2 AND page = $3 AND slot = $4 AND locale = $5 FOR UPDATE")
        .bind(&t)
        .bind(&su)
        .bind(&pg)
        .bind(&sl)
        .bind(&lo)
        .fetch_optional(&mut *tx)
        .await?;
    let old: Option<Option<Value>> = sqlx::query_scalar("SELECT value FROM content_versions WHERE tenant = $1 AND surface = $2 AND page = $3 AND slot = $4 AND locale = $5 AND version = $6")
        .bind(&t)
        .bind(&su)
        .bind(&pg)
        .bind(&sl)
        .bind(&lo)
        .bind(ver as i32)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(value) = old else { return Err(ApiError::not_found("Version")) };
    if let Some(v) = &value {
        check_media(&st.pool, &t, &[v]).await?;
    }
    let v = set_published(&mut tx, &t, &who, (&su, &pg, &sl, &lo), value.as_ref(), "rollback", Some(ver as i32)).await?;
    store::audit(&mut *tx, &t, &who, "content.rollback", &target(&su, &pg, &sl, Some(&lo)), json!({"to": ver, "version": v})).await;
    tx.commit().await?;
    Ok(Json(json!({"version": v, "value": value})))
}

/// Deletes a slot: every locale, draft and version (the app shows its built-in default again).
pub async fn slot_delete(State(st): State<AppState>, h: HeaderMap, Query(q): Query<KeyQ>) -> R {
    let who = api::require(&h, "content.publish")?;
    let t = api::tenant(&h);
    let (su, pg, sl) = (surface_of(q.surface.as_deref())?, page_of(q.page.as_deref())?, slot_of(q.slot.as_deref())?);
    let mut tx = st.pool.begin().await?;
    let gone = sqlx::query("DELETE FROM content_slots WHERE tenant = $1 AND surface = $2 AND page = $3 AND slot = $4").bind(&t).bind(&su).bind(&pg).bind(&sl).execute(&mut *tx).await?;
    if gone.rows_affected() == 0 {
        return Err(ApiError::not_found("Slot"));
    }
    sqlx::query("DELETE FROM content_versions WHERE tenant = $1 AND surface = $2 AND page = $3 AND slot = $4").bind(&t).bind(&su).bind(&pg).bind(&sl).execute(&mut *tx).await?;
    store::audit(&mut *tx, &t, &who, "content.delete", &target(&su, &pg, &sl, None), json!({})).await;
    tx.commit().await?;
    Ok(Json(json!({"ok": true})))
}

/// Bulk draft save (seed scripts, migrations), optionally publishing the pages it touched.
pub async fn import(State(st): State<AppState>, h: HeaderMap, body: Bytes) -> R {
    let b = api::object(&body)?;
    let publish = b.get("publish").and_then(Value::as_bool).unwrap_or(false);
    let who = api::require(&h, if publish { "content.publish" } else { "content.write" })?;
    let t = api::tenant(&h);
    let su = surface_of(s(&b, "surface"))?;
    let Some(list) = b.get("entries").and_then(Value::as_array).filter(|l| !l.is_empty() && l.len() <= 500) else {
        return Err(ApiError::bad("entries must be a list of 1 to 500 entries."));
    };
    let mut items = vec![];
    for (i, e) in list.iter().enumerate() {
        let Some(o) = e.as_object() else { return Err(ApiError::bad(format!("entries[{i}] must be an object."))) };
        items.push(draft_in(su.clone(), o).map_err(|e| ApiError::bad(format!("entries[{i}]: {}", e.2)))?);
    }
    check_media(&st.pool, &t, &items.iter().map(|d| &d.value).collect::<Vec<_>>()).await?;
    let mut tx = st.pool.begin().await?;
    for d in &items {
        save_one(&mut tx, &t, &who, d).await?;
    }
    let mut published = vec![];
    if publish {
        let mut seen = HashSet::new();
        for d in &items {
            if seen.insert((d.page.clone(), d.slot.clone(), d.locale.clone())) {
                for mut p in publish_scope(&mut tx, &t, &who, &su, &d.page, Some(&d.slot), Some(&d.locale)).await? {
                    p["page"] = json!(d.page);
                    published.push(p);
                }
            }
        }
        if !published.is_empty() {
            store::audit(&mut *tx, &t, &who, "content.import", &su, json!({"entries": published.len()})).await;
        }
    }
    tx.commit().await?;
    Ok(Json(json!({"saved": items.len(), "published": published})))
}

pub async fn preview_token(State(st): State<AppState>, h: HeaderMap, body: Bytes) -> R {
    api::require(&h, "content.read")?;
    let t = api::tenant(&h);
    let su = surface_of(s(&api::object(&body)?, "surface"))?;
    let exp = chrono::Utc::now().timestamp() + preview::TTL_SECS;
    Ok(Json(json!({"token": preview::issue(&st.cfg.preview_secret, &t, &su, exp), "expires_at": chrono::DateTime::from_timestamp(exp, 0)})))
}

/* ------------------------------------------------------------------ */
/* Public read                                                         */
/* ------------------------------------------------------------------ */

/// An image value for readers: the asset resolved (URLs, srcset, dark file) with alt text in the locale.
fn resolve_image(asset: &Value, locale: &str) -> Value {
    let alt = asset["alt"].get(locale).or_else(|| asset["alt"].get(BASE_LOCALE)).and_then(Value::as_str).unwrap_or("");
    let l = &asset["light"];
    let dark = asset["dark"].as_object().map(|d| json!({"url": d["url"], "srcset": d["srcset"], "width": d["width"], "height": d["height"], "variants": d["variants"]}));
    json!({"media": asset["id"], "alt": alt, "url": l["url"], "srcset": l["srcset"], "width": l["width"], "height": l["height"], "mime": l["mime"], "variants": l["variants"], "dark": dark})
}

pub async fn public_read(State(st): State<AppState>, h: HeaderMap, Query(q): Query<KeyQ>) -> R<Response> {
    let su = surface_of(q.surface.as_deref())?;
    let t = match q.tenant.as_deref().map(|x| x.trim().to_ascii_lowercase()) {
        Some(x) if api::is_slug(&x) => x,
        _ => api::tenant(&h),
    };
    let locale = norm_locale(q.locale.as_deref().unwrap_or(BASE_LOCALE));
    let pages: Option<Vec<String>> = match q.page.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        None => None,
        Some(p) => {
            let list: Vec<String> = p.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect();
            if list.len() > 10 || !list.iter().all(|x| is_page(x)) {
                return Err(ApiError::bad("page must be up to 10 comma-separated page keys."));
            }
            Some(list)
        }
    };
    let preview = match q.preview.as_deref().filter(|p| !p.is_empty()) {
        None => false,
        Some(tok) if preview::verify(&st.cfg.preview_secret, tok, &t, &su, chrono::Utc::now().timestamp()) => true,
        Some(_) => return Err(ApiError::new(StatusCode::FORBIDDEN, "preview_expired", "This preview link has expired. Open the preview again from the Back Office.")),
    };
    let rows = sqlx::query(
        "SELECT e.page, e.slot, s.kind, e.locale, e.draft, e.published FROM content_entries e
           JOIN content_slots s USING (tenant, surface, page, slot)
          WHERE e.tenant = $1 AND e.surface = $2 AND ($3::text[] IS NULL OR e.page = ANY($3)) AND e.locale = ANY($4)
            AND (e.published IS NOT NULL OR $5)",
    )
    .bind(&t)
    .bind(&su)
    .bind(&pages)
    .bind(vec![locale.to_string(), BASE_LOCALE.to_string()])
    .bind(preview)
    .fetch_all(&st.pool)
    .await?;
    // (page, slot) → kind + locale → (draft, published)
    type Rows = BTreeMap<String, (Option<Value>, Option<Value>)>;
    let mut by_slot: BTreeMap<(String, String), (String, Rows)> = BTreeMap::new();
    for r in &rows {
        let e = by_slot.entry((r.get(0), r.get(1))).or_insert_with(|| (r.get(2), BTreeMap::new()));
        e.1.insert(r.get(3), (r.get(4), r.get(5)));
    }
    let mut picked: Vec<(String, String, String, String, Value)> = vec![];
    let mut ids = vec![];
    for ((page, slot), (kind, locs)) in &by_slot {
        if let Some((loc, v)) = pick(locs, locale, preview) {
            if let Some(id) = v.get("media").and_then(Value::as_str) {
                ids.push(id.to_string());
            }
            picked.push((page.clone(), slot.clone(), kind.clone(), loc.to_string(), v.clone()));
        }
    }
    let assets = media::assets_by_id(&st.pool, &st.cfg.public_url, &t, &ids).await?;
    let mut out: BTreeMap<String, Map<String, Value>> = BTreeMap::new();
    for p in pages.iter().flatten() {
        out.entry(p.clone()).or_default();
    }
    for (page, slot, kind, loc, v) in picked {
        let value = if kind == "image" {
            // a deleted image leaves the slot empty: the app shows its default
            match v.get("media").and_then(Value::as_str).and_then(|id| assets.get(id)) {
                Some(a) => resolve_image(a, locale),
                None => continue,
            }
        } else {
            v
        };
        out.entry(page).or_default().insert(slot, json!({"kind": kind, "locale": loc, "value": value}));
    }
    let body = serde_json::to_vec(&json!({"tenant": t, "surface": su, "locale": locale, "preview": preview, "pages": out})).map_err(anyhow::Error::from)?;
    let etag = format!("W/\"{}\"", Sha256::digest(&body).iter().take(12).map(|b| format!("{b:02x}")).collect::<String>());
    let cache = if preview { "private, no-store" } else { "public, max-age=60, stale-while-revalidate=300" };
    let not_modified = !preview && h.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()).is_some_and(|v| v.split(',').any(|x| x.trim() == etag));
    let mut res = if not_modified { StatusCode::NOT_MODIFIED.into_response() } else { ([(header::CONTENT_TYPE, "application/json")], body).into_response() };
    let hd = res.headers_mut();
    hd.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    hd.insert(header::ETAG, HeaderValue::from_str(&etag).map_err(anyhow::Error::from)?);
    hd.insert(header::VARY, HeaderValue::from_static("x-kalks-tenant"));
    hd.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys() {
        assert!(is_page("home") && is_page("markets-forex") && is_page("global") && !is_page("Home") && !is_page("a/b") && !is_page("-x"));
        assert!(is_slot("hero.title") && is_slot("faq.3.answer") && !is_slot("hero title") && !is_slot(".x") && !is_slot(""));
        assert_eq!(norm_locale("pt-BR"), "pt");
        assert_eq!(norm_locale("zh_Hans"), "zh");
        assert_eq!(norm_locale("AR"), "ar");
        assert_eq!(norm_locale("xx"), "en");
        assert_eq!(LOCALES.len(), 22);
    }

    #[test]
    fn values_are_validated() {
        assert_eq!(validate_value("text", &json!({"text": "  Hi\r\nthere "})).unwrap(), json!({"text": "Hi\nthere"}));
        assert!(validate_value("text", &json!({"text": "x".repeat(2001)})).is_err());
        assert!(validate_value("placeholder", &json!({"text": "x".repeat(201)})).is_err());
        assert!(validate_value("rich_text", &json!({"markdown": "# Hi"})).is_ok());
        assert!(validate_value("link", &json!({"label": "Open", "href": "/register"})).is_ok());
        assert!(validate_value("link", &json!({"label": "Open", "href": "https://kalkstrade.com/x"})).is_ok());
        for bad in ["javascript:alert(1)", "//evil.example", "data:text/html,x", "ftp://x", "/a b"] {
            assert!(validate_value("link", &json!({"label": "x", "href": bad})).is_err(), "{bad}");
        }
        assert!(validate_value("image", &json!({"media": "m0123456789abcdef01"})).is_ok());
        assert!(validate_value("image", &json!({"media": "../x"})).is_err());
        assert!(validate_value("video", &json!({})).is_err());
        assert!(validate_value("text", &json!("plain")).is_err());
    }

    #[test]
    fn locale_fallback_and_preview() {
        let mut rows = BTreeMap::new();
        rows.insert("en".to_string(), (Some(json!({"text": "en draft"})), Some(json!({"text": "en live"}))));
        rows.insert("ar".to_string(), (Some(json!({"text": "ar draft"})), None));
        assert_eq!(pick(&rows, "en", false), Some(("en", &json!({"text": "en live"}))));
        // Arabic has only a draft: readers get English, a preview gets the Arabic draft
        assert_eq!(pick(&rows, "ar", false), Some(("en", &json!({"text": "en live"}))));
        assert_eq!(pick(&rows, "ar", true), Some(("ar", &json!({"text": "ar draft"}))));
        assert_eq!(pick(&rows, "de", true), Some(("en", &json!({"text": "en draft"}))));
        let empty = BTreeMap::new();
        assert_eq!(pick(&empty, "de", false), None);
    }
}
