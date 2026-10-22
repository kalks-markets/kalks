//! HTTP API (internal; every route except /health needs `X-Kalks-Internal`). `X-Kalks-Tenant` picks the
//! tenant whose staff decisions (pin / hide / retag) apply; default "kalks".
//!
//! Read API (Client Area, Kalks Trader and Back Office BFFs; no user needed):
//!
//! | route                                  | query                                                        | response                                   |
//! |----------------------------------------|--------------------------------------------------------------|--------------------------------------------|
//! | `GET /v1/news`                         | `symbol, currency, country, category, sentiment, q, before (RFC 3339), limit ≤ 100, minImportance (default 20)` | `{pinned[], items[], next}` |
//! | `GET /v1/news/{id}`                    | –                                                            | `{item}`                                   |
//! | `GET /v1/news/map`                     | `hours` (1–168, default 24)                                  | `{countries[], mentions[], total, hours}`  |
//! | `GET /v1/news/sources`                 | –                                                            | `{sources[]}` enabled feeds (attribution)  |
//! | `GET /v1/calendar`                     | `from, to (RFC 3339; default this server week), currency (CSV), impact (CSV 0-3)` | `{events[], from, to, serverOffset, now, updatedAt, source}` |
//! | `GET /v1/calendar/next`                | `impact` (default 3)                                         | `{event|null}`                             |
//! | `GET /v1/calendar/{id}`                | –                                                            | `{event, history[]}` (past releases)       |
//! | `GET /v1/brief`                        | `day` (YYYY-MM-DD, default today server time)               | `{brief|null, day, model, createdAt, configured}` |
//!
//! Client routes (the CRM BFF adds `X-Kalks-User-Id`):
//! `GET /v1/me/calendar` · `POST /v1/me/calendar/reminders {eventId, minutes?}` ·
//! `DELETE /v1/me/calendar/reminders/{eventId}` · `PUT|DELETE /v1/me/calendar/alerts {highImpact, currencies[], minutes}`
//!
//! Back Office (admin BFF checks `content.read` / `content.write` and adds `X-Kalks-Staff`):
//! `GET /v1/admin/news?status&source&q&limit&offset` · `PUT /v1/admin/news/{id} {pinned?, hidden?, symbols?,
//! countries?, importance?}` · `DELETE /v1/admin/news/{id}` (reset to automatic tags) · `GET /v1/admin/sources` ·
//! `PUT /v1/admin/sources/{id} {enabled}` · `POST /v1/admin/sources/{id}/refresh` ·
//! `GET /v1/admin/calendar?from&to` · `PUT /v1/admin/calendar/{id} {actual?, impact?}` ·
//! `POST /v1/admin/calendar/refresh` · `POST /v1/admin/brief {day?}` · `GET /v1/admin/stats` · `GET /v1/admin/audit`.
//! Feeds, the calendar and the daily brief are shared by every broker, so changing a source or a calendar event,
//! fetching / refreshing them and regenerating the brief are platform tenant only (403 `forbidden` otherwise).

use axum::extract::{Path, Query, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use sqlx::postgres::PgRow;
use subtle::ConstantTimeEq;

use crate::calendar::{self, impact_label, server_offset_hours, server_week_start, surprise, to_server};
use crate::tagging::{self, COUNTRIES};
use crate::{AppState, brief, store, workers};

pub const PLATFORM_TENANT: &str = "kalks";

/* ------------------------------------------------------------------ */
/* Errors and request context                                          */
/* ------------------------------------------------------------------ */

pub struct ApiError(StatusCode, &'static str, String);

impl ApiError {
    fn new(s: StatusCode, code: &'static str, msg: impl Into<String>) -> Self {
        Self(s, code, msg.into())
    }
    fn not_found(what: &str) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", format!("{what} not found."))
    }
    fn bad(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, "validation", msg)
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        tracing::error!(error = %e, "news internal error");
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
        (self.0, Json(json!({"error": {"code": self.1, "message": self.2}}))).into_response()
    }
}

type R<T = Json<Value>> = Result<T, ApiError>;

fn is_slug(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

fn tenant(h: &HeaderMap) -> String {
    let t = h.get("x-kalks-tenant").and_then(|v| v.to_str().ok()).unwrap_or(PLATFORM_TENANT).trim().to_ascii_lowercase();
    if is_slug(&t) { t } else { PLATFORM_TENANT.into() }
}

fn user_id(h: &HeaderMap) -> R<i64> {
    h.get("x-kalks-user-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|v| *v > 0)
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "unauthorized", "Missing user."))
}

fn staff(h: &HeaderMap) -> R<String> {
    h.get("x-kalks-staff")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().chars().take(120).collect::<String>())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "unauthorized", "Missing staff member."))
}

/// Feeds, the calendar and the daily brief are shared by every broker: only the platform tenant changes, fetches or
/// regenerates them. Returns the tenant (always the platform) for the audit entry.
fn platform_only(h: &HeaderMap, msg: &str) -> R<String> {
    let t = tenant(h);
    if t != PLATFORM_TENANT {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "forbidden", msg));
    }
    Ok(t)
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
    let internal = Router::new()
        .route("/v1/news", get(news_list))
        .route("/v1/news/map", get(news_map))
        .route("/v1/news/sources", get(public_sources))
        .route("/v1/news/{id}", get(news_one))
        .route("/v1/calendar", get(calendar_list))
        .route("/v1/calendar/next", get(calendar_next))
        .route("/v1/calendar/{id}", get(calendar_one))
        .route("/v1/brief", get(brief_get))
        .route("/v1/me/calendar", get(my_calendar))
        .route("/v1/me/calendar/reminders", post(remind_add))
        .route("/v1/me/calendar/reminders/{event}", delete(remind_del))
        .route("/v1/me/calendar/alerts", put(alerts_put).delete(alerts_del))
        .route("/v1/admin/news", get(admin_news))
        .route("/v1/admin/news/{id}", put(admin_news_put).delete(admin_news_reset))
        .route("/v1/admin/sources", get(admin_sources))
        .route("/v1/admin/sources/{id}", put(admin_source_put))
        .route("/v1/admin/sources/{id}/refresh", post(admin_source_refresh))
        .route("/v1/admin/calendar", get(admin_calendar))
        .route("/v1/admin/calendar/refresh", post(admin_calendar_refresh))
        .route("/v1/admin/calendar/{id}", put(admin_calendar_put))
        .route("/v1/admin/brief", post(admin_brief))
        .route("/v1/admin/stats", get(admin_stats))
        .route("/v1/admin/audit", get(admin_audit))
        .layer(middleware::from_fn_with_state(st.clone(), require_internal));
    Router::new().route("/health", get(health)).merge(internal).with_state(st)
}

async fn health(State(st): State<AppState>) -> Json<Value> {
    let db = sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&st.pool).await.is_ok();
    Json(json!({"service": "news", "status": if db { "ok" } else { "degraded" }, "db": db, "brief": !st.cfg.anthropic_key.is_empty() && st.cfg.brief}))
}

/* ------------------------------------------------------------------ */
/* News                                                                */
/* ------------------------------------------------------------------ */

/// Items with this tenant's overrides applied (e_* = effective values).
const ITEMS_CTE: &str = "WITH e AS (
  SELECT i.id, i.title, i.summary, i.link, i.published_at, i.category, i.sentiment, i.source_id, s.name AS source_name, s.homepage, s.enabled AS source_enabled,
         i.symbols AS auto_symbols, i.countries AS auto_countries, i.importance AS auto_importance,
         COALESCE(o.symbols, i.symbols) AS e_symbols, COALESCE(o.countries, i.countries) AS e_countries,
         COALESCE(o.currencies, i.currencies) AS e_currencies, COALESCE(o.importance, i.importance) AS e_importance,
         COALESCE(o.pinned, false) AS pinned, o.pinned_at, COALESCE(o.hidden, false) AS hidden, o.updated_by, o.updated_at AS override_at,
         (o.symbols IS NOT NULL OR o.countries IS NOT NULL OR o.importance IS NOT NULL) AS retagged
    FROM items i JOIN sources s ON s.id = i.source_id
    LEFT JOIN item_overrides o ON o.item_id = i.id AND o.tenant = $1)";

fn item_json(r: &PgRow) -> Value {
    json!({
        "id": r.get::<i64, _>("id"),
        "title": r.get::<String, _>("title"),
        "summary": r.get::<String, _>("summary"),
        "link": r.get::<String, _>("link"),
        "source": {"id": r.get::<String, _>("source_id"), "name": r.get::<String, _>("source_name"), "homepage": r.get::<String, _>("homepage")},
        "publishedAt": r.get::<DateTime<Utc>, _>("published_at"),
        "countries": r.get::<Vec<String>, _>("e_countries"),
        "currencies": r.get::<Vec<String>, _>("e_currencies"),
        "symbols": r.get::<Vec<String>, _>("e_symbols"),
        "category": r.get::<String, _>("category"),
        "sentiment": r.get::<String, _>("sentiment"),
        "importance": r.get::<i32, _>("e_importance"),
        "pinned": r.get::<bool, _>("pinned"),
    })
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NewsQ {
    symbol: Option<String>,
    currency: Option<String>,
    country: Option<String>,
    category: Option<String>,
    sentiment: Option<String>,
    q: Option<String>,
    before: Option<String>,
    limit: Option<i64>,
    min_importance: Option<i32>,
    hours: Option<i64>,
}

fn opt_upper(v: &Option<String>, max: usize) -> Option<String> {
    v.as_ref().map(|s| s.trim().to_ascii_uppercase()).filter(|s| !s.is_empty() && s.len() <= max && s.chars().all(|c| c.is_ascii_alphanumeric()))
}
fn opt_lower(v: &Option<String>, max: usize) -> Option<String> {
    v.as_ref().map(|s| s.trim().to_ascii_lowercase()).filter(|s| !s.is_empty() && s.len() <= max && s.chars().all(|c| c.is_ascii_lowercase() || c == '-'))
}

async fn news_list(State(st): State<AppState>, h: HeaderMap, Query(q): Query<NewsQ>) -> R {
    let t = tenant(&h);
    let limit = q.limit.unwrap_or(30).clamp(1, 100);
    let before = q.before.as_deref().and_then(|b| DateTime::parse_from_rfc3339(b).ok()).map(|d| d.with_timezone(&Utc));
    let since = q.hours.map(|x| Utc::now() - Duration::hours(x.clamp(1, 24 * 30)));
    let search = q.q.as_ref().map(|s| s.trim().chars().take(80).collect::<String>()).filter(|s| s.len() >= 2).map(|s| format!("%{}%", s.replace(['%', '_', '\\'], "")));
    let filters = "NOT hidden AND source_enabled
        AND ($2::text IS NULL OR $2 = ANY(e_symbols)) AND ($3::text IS NULL OR $3 = ANY(e_currencies)) AND ($4::text IS NULL OR $4 = ANY(e_countries))
        AND ($5::text IS NULL OR category = $5) AND ($6::text IS NULL OR sentiment = $6) AND ($7::text IS NULL OR title ILIKE $7 OR summary ILIKE $7)
        AND (e_importance >= $8 OR pinned) AND ($10::timestamptz IS NULL OR published_at >= $10)";
    let sql = format!("{ITEMS_CTE} SELECT * FROM e WHERE {filters} AND NOT pinned AND ($9::timestamptz IS NULL OR published_at < $9) ORDER BY published_at DESC, id DESC LIMIT {}", limit + 1);
    let binds = |sql: &str| {
        sqlx::query(sqlx::AssertSqlSafe(sql.to_string()))
            .bind(t.clone())
            .bind(opt_upper(&q.symbol, 12))
            .bind(opt_upper(&q.currency, 3))
            .bind(opt_lower(&q.country, 2))
            .bind(opt_lower(&q.category, 16))
            .bind(opt_lower(&q.sentiment, 8))
            .bind(search.clone())
            .bind(q.min_importance.unwrap_or(20).clamp(0, 100))
            .bind(before)
            .bind(since)
    };
    let rows = binds(&sql).fetch_all(&st.pool).await?;
    let more = rows.len() as i64 > limit;
    let items: Vec<Value> = rows.iter().take(limit as usize).map(item_json).collect();
    let next = if more { rows.get(limit as usize - 1).map(|r| r.get::<DateTime<Utc>, _>("published_at")) } else { None };
    // pinned stories lead the first page only
    let pinned: Vec<Value> = if before.is_none() {
        let psql = format!("{ITEMS_CTE} SELECT * FROM e WHERE {filters} AND pinned AND $9::timestamptz IS NULL ORDER BY pinned_at DESC NULLS LAST LIMIT 5");
        binds(&psql).fetch_all(&st.pool).await?.iter().map(item_json).collect()
    } else {
        vec![]
    };
    Ok(Json(json!({"pinned": pinned, "items": items, "next": next})))
}

async fn news_one(State(st): State<AppState>, h: HeaderMap, Path(id): Path<i64>) -> R {
    let sql = format!("{ITEMS_CTE} SELECT * FROM e WHERE id = $2 AND NOT hidden AND source_enabled");
    let r = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(tenant(&h)).bind(id).fetch_optional(&st.pool).await?.ok_or_else(|| ApiError::not_found("Story"))?;
    Ok(Json(json!({"item": item_json(&r)})))
}

#[derive(Deserialize, Default)]
pub struct HoursQ {
    hours: Option<i64>,
}

/// World map: stories per country with tone ("sentiment heat": bullish − bearish over all, −1..1), the top
/// story per country, and instrument mentions.
async fn news_map(State(st): State<AppState>, h: HeaderMap, Query(q): Query<HoursQ>) -> R {
    let hours = q.hours.unwrap_or(24).clamp(1, 168);
    let since = Utc::now() - Duration::hours(hours);
    let sql = format!("{ITEMS_CTE} SELECT * FROM e WHERE NOT hidden AND source_enabled AND published_at >= $2 AND (e_importance >= 20 OR pinned) ORDER BY e_importance DESC, published_at DESC");
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(tenant(&h)).bind(since).fetch_all(&st.pool).await?;
    let mut by: std::collections::BTreeMap<String, (i64, i64, i64, Option<Value>)> = Default::default();
    let mut mentions: std::collections::BTreeMap<String, i64> = Default::default();
    for r in &rows {
        let sent: String = r.get("sentiment");
        for c in r.get::<Vec<String>, _>("e_countries") {
            let e = by.entry(c).or_insert((0, 0, 0, None));
            e.0 += 1;
            if sent == "bullish" {
                e.1 += 1;
            } else if sent == "bearish" {
                e.2 += 1;
            }
            if e.3.is_none() {
                e.3 = Some(json!({"id": r.get::<i64, _>("id"), "title": r.get::<String, _>("title"), "source": r.get::<String, _>("source_name"), "publishedAt": r.get::<DateTime<Utc>, _>("published_at")}));
            }
        }
        for s in r.get::<Vec<String>, _>("e_symbols") {
            *mentions.entry(s).or_default() += 1;
        }
    }
    let mut countries: Vec<Value> = by
        .into_iter()
        .map(|(c, (n, up, down, top))| {
            let name = COUNTRIES.iter().find(|(x, _, _)| *x == c).map(|(_, _, n)| *n).unwrap_or("");
            json!({"country": c, "name": name, "count": n, "bullish": up, "bearish": down, "sentiment": ((up - down) as f64 / n.max(1) as f64 * 100.0).round() / 100.0, "top": top})
        })
        .collect();
    countries.sort_by(|a, b| b["count"].as_i64().cmp(&a["count"].as_i64()));
    let mut mentions: Vec<Value> = mentions.into_iter().map(|(s, n)| json!({"symbol": s, "count": n})).collect();
    mentions.sort_by(|a, b| b["count"].as_i64().cmp(&a["count"].as_i64()).then(a["symbol"].as_str().cmp(&b["symbol"].as_str())));
    mentions.truncate(10);
    Ok(Json(json!({"hours": hours, "total": rows.len(), "countries": countries, "mentions": mentions})))
}

async fn public_sources(State(st): State<AppState>) -> R {
    let rows = sqlx::query("SELECT id, name, homepage, kind FROM sources WHERE enabled ORDER BY kind, name").fetch_all(&st.pool).await?;
    Ok(Json(json!({"sources": rows.iter().map(|r| json!({"id": r.get::<String, _>("id"), "name": r.get::<String, _>("name"), "homepage": r.get::<String, _>("homepage"), "kind": r.get::<String, _>("kind")})).collect::<Vec<_>>()})))
}

/* ------------------------------------------------------------------ */
/* Calendar                                                            */
/* ------------------------------------------------------------------ */

const EVENT_COLS: &str = "id, title, currency, country, starts_at, all_day, COALESCE(impact_override, impact) AS impact, impact AS feed_impact, impact_override, forecast, previous, actual, actual_at, actual_source, symbols, updated_at";

fn event_json(r: &PgRow) -> Value {
    let starts: DateTime<Utc> = r.get("starts_at");
    let srv = to_server(starts);
    let title: String = r.get("title");
    let (actual, forecast): (String, String) = (r.get("actual"), r.get("forecast"));
    let impact: i16 = r.get("impact");
    json!({
        "id": r.get::<i64, _>("id"),
        "title": title,
        "currency": r.get::<String, _>("currency"),
        "country": r.get::<String, _>("country"),
        "startsAt": starts,
        "serverDate": srv.format("%Y-%m-%d").to_string(),
        "serverTime": if r.get::<bool, _>("all_day") { "All day".to_string() } else { srv.format("%H:%M").to_string() },
        "allDay": r.get::<bool, _>("all_day"),
        "impact": impact,
        "impactLabel": impact_label(impact),
        "forecast": forecast,
        "previous": r.get::<String, _>("previous"),
        "actual": actual,
        "actualSource": r.get::<Option<String>, _>("actual_source"),
        "surprise": surprise(&title, &actual, &forecast),
        "lowerIsBetter": calendar::lower_is_better(&title),
        "symbols": r.get::<Vec<String>, _>("symbols"),
        "updatedAt": r.get::<DateTime<Utc>, _>("updated_at"),
    })
}

#[derive(Deserialize, Default)]
pub struct CalQ {
    from: Option<String>,
    to: Option<String>,
    currency: Option<String>,
    impact: Option<String>,
}

fn parse_time(s: &Option<String>) -> Option<DateTime<Utc>> {
    let s = s.as_deref()?.trim();
    if let Ok(d) = DateTime::parse_from_rfc3339(s) {
        return Some(d.with_timezone(&Utc));
    }
    // a bare date is a server-time day
    let d = NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()?;
    let naive = d.and_hms_opt(0, 0, 0)?;
    let guess = naive.and_utc() - Duration::hours(3);
    Some(naive.and_utc() - Duration::hours(server_offset_hours(guess) as i64))
}

fn range(q: &CalQ) -> Result<(DateTime<Utc>, DateTime<Utc>), ApiError> {
    let from = parse_time(&q.from).unwrap_or_else(|| server_week_start(Utc::now()));
    let to = parse_time(&q.to).unwrap_or(from + Duration::days(7));
    if to <= from || to - from > Duration::days(62) {
        return Err(ApiError::bad("Use a range of at most 62 days."));
    }
    Ok((from, to))
}

fn csv_upper(v: &Option<String>) -> Option<Vec<String>> {
    let out: Vec<String> = v.as_deref()?.split(',').map(|s| s.trim().to_ascii_uppercase()).filter(|s| s.len() == 3 && s.chars().all(|c| c.is_ascii_alphabetic())).take(12).collect();
    (!out.is_empty()).then_some(out)
}
fn csv_impact(v: &Option<String>) -> Option<Vec<i16>> {
    let out: Vec<i16> = v.as_deref()?.split(',').filter_map(|s| s.trim().parse::<i16>().ok()).filter(|i| (0..=3).contains(i)).collect();
    (!out.is_empty()).then_some(out)
}

async fn calendar_status(st: &AppState) -> R<Value> {
    let r = sqlx::query("SELECT url, last_fetch_at, last_ok_at, last_error, events FROM calendar_fetches WHERE id = $1").bind(workers::CALENDAR_ID).fetch_optional(&st.pool).await?;
    Ok(match r {
        Some(r) => json!({"name": "Forex Factory", "url": "https://www.forexfactory.com/calendar", "lastFetchAt": r.get::<Option<DateTime<Utc>>, _>("last_fetch_at"), "lastOkAt": r.get::<Option<DateTime<Utc>>, _>("last_ok_at"), "lastError": r.get::<Option<String>, _>("last_error"), "events": r.get::<i32, _>("events")}),
        None => json!({"name": "Forex Factory", "url": "https://www.forexfactory.com/calendar", "lastFetchAt": null, "lastOkAt": null, "lastError": null, "events": 0}),
    })
}

async fn calendar_list(State(st): State<AppState>, Query(q): Query<CalQ>) -> R {
    let (from, to) = range(&q)?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {EVENT_COLS} FROM calendar_events WHERE starts_at >= $1 AND starts_at < $2 AND ($3::text[] IS NULL OR currency = ANY($3))
           AND ($4::smallint[] IS NULL OR COALESCE(impact_override, impact) = ANY($4)) ORDER BY starts_at, COALESCE(impact_override, impact) DESC, id"
    )))
    .bind(from)
    .bind(to)
    .bind(csv_upper(&q.currency))
    .bind(csv_impact(&q.impact))
    .fetch_all(&st.pool)
    .await?;
    let src = calendar_status(&st).await?;
    Ok(Json(json!({
        "events": rows.iter().map(event_json).collect::<Vec<_>>(),
        "from": from, "to": to,
        "serverOffset": server_offset_hours(Utc::now()),
        "now": Utc::now(),
        "updatedAt": src["lastOkAt"],
        "source": src,
    })))
}

#[derive(Deserialize, Default)]
pub struct NextQ {
    impact: Option<i16>,
}

async fn calendar_next(State(st): State<AppState>, Query(q): Query<NextQ>) -> R {
    let r = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT {EVENT_COLS} FROM calendar_events WHERE starts_at > now() AND NOT all_day AND COALESCE(impact_override, impact) >= $1 ORDER BY starts_at LIMIT 1")))
        .bind(q.impact.unwrap_or(3).clamp(1, 3))
        .fetch_optional(&st.pool)
        .await?;
    Ok(Json(json!({"event": r.as_ref().map(event_json), "serverOffset": server_offset_hours(Utc::now()), "now": Utc::now()})))
}

async fn calendar_one(State(st): State<AppState>, Path(id): Path<i64>) -> R {
    let r = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT {EVENT_COLS} FROM calendar_events WHERE id = $1"))).bind(id).fetch_optional(&st.pool).await?.ok_or_else(|| ApiError::not_found("Event"))?;
    let (ccy, title, starts): (String, String, DateTime<Utc>) = (r.get("currency"), r.get("title"), r.get("starts_at"));
    let hist = sqlx::query("SELECT starts_at, actual, forecast, previous FROM calendar_events WHERE currency = $1 AND title = $2 AND starts_at < $3 ORDER BY starts_at DESC LIMIT 11")
        .bind(&ccy)
        .bind(&title)
        .bind(starts)
        .fetch_all(&st.pool)
        .await?;
    let history: Vec<Value> = hist
        .iter()
        .rev()
        .map(|h| json!({"startsAt": h.get::<DateTime<Utc>, _>("starts_at"), "actual": h.get::<String, _>("actual"), "forecast": h.get::<String, _>("forecast"), "previous": h.get::<String, _>("previous")}))
        .collect();
    Ok(Json(json!({"event": event_json(&r), "history": history})))
}

/* ------------------------------------------------------------------ */
/* Brief                                                               */
/* ------------------------------------------------------------------ */

#[derive(Deserialize, Default)]
pub struct DayQ {
    day: Option<String>,
}

async fn brief_get(State(st): State<AppState>, Query(q): Query<DayQ>) -> R {
    let day = q.day.as_deref().and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()).unwrap_or_else(brief::today);
    // today's brief, or the latest one when today's isn't written yet
    let r = sqlx::query("SELECT day, model, body, created_at FROM briefs WHERE day <= $1 ORDER BY day DESC LIMIT 1").bind(day).fetch_optional(&st.pool).await?;
    let configured = st.cfg.brief && !st.cfg.anthropic_key.is_empty();
    Ok(Json(match r {
        Some(r) => json!({"brief": r.get::<Value, _>("body"), "day": r.get::<NaiveDate, _>("day"), "model": r.get::<String, _>("model"), "createdAt": r.get::<DateTime<Utc>, _>("created_at"), "configured": configured}),
        None => json!({"brief": null, "day": day, "model": null, "createdAt": null, "configured": configured}),
    }))
}

/* ------------------------------------------------------------------ */
/* Client reminders and alerts                                         */
/* ------------------------------------------------------------------ */

async fn my_calendar(State(st): State<AppState>, h: HeaderMap) -> R {
    let (t, u) = (tenant(&h), user_id(&h)?);
    let rem: Vec<i64> = sqlx::query_scalar("SELECT r.event_id FROM calendar_reminders r JOIN calendar_events e ON e.id = r.event_id WHERE r.tenant = $1 AND r.user_id = $2 AND e.starts_at > now() - interval '1 day'")
        .bind(&t)
        .bind(u)
        .fetch_all(&st.pool)
        .await?;
    let a = sqlx::query("SELECT high_impact, currencies, minutes FROM calendar_alerts WHERE tenant = $1 AND user_id = $2").bind(&t).bind(u).fetch_optional(&st.pool).await?;
    Ok(Json(json!({
        "reminders": rem,
        "alerts": a.map(|a| json!({"highImpact": a.get::<bool, _>("high_impact"), "currencies": a.get::<Vec<String>, _>("currencies"), "minutes": a.get::<i32, _>("minutes")})),
    })))
}

fn minutes(v: &Value) -> Result<i32, ApiError> {
    match v.get("minutes") {
        None | Some(Value::Null) => Ok(15),
        Some(m) => m.as_i64().filter(|m| [5, 10, 15, 30, 60].contains(m)).map(|m| m as i32).ok_or_else(|| ApiError::bad("minutes must be 5, 10, 15, 30 or 60.")),
    }
}

async fn remind_add(State(st): State<AppState>, h: HeaderMap, Json(b): Json<Value>) -> R {
    let (t, u) = (tenant(&h), user_id(&h)?);
    let id = b.get("eventId").and_then(Value::as_i64).ok_or_else(|| ApiError::bad("eventId is required."))?;
    let m = minutes(&b)?;
    let starts: Option<DateTime<Utc>> = sqlx::query_scalar("SELECT starts_at FROM calendar_events WHERE id = $1").bind(id).fetch_optional(&st.pool).await?;
    let starts = starts.ok_or_else(|| ApiError::not_found("Event"))?;
    if starts <= Utc::now() {
        return Err(ApiError::bad("This event has already started."));
    }
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM calendar_reminders r JOIN calendar_events e ON e.id = r.event_id WHERE r.tenant = $1 AND r.user_id = $2 AND r.sent_at IS NULL AND e.starts_at > now()").bind(&t).bind(u).fetch_one(&st.pool).await?;
    if n >= 100 {
        return Err(ApiError::bad("You have 100 upcoming reminders. Remove some first."));
    }
    sqlx::query("INSERT INTO calendar_reminders (tenant, user_id, event_id, minutes) VALUES ($1,$2,$3,$4) ON CONFLICT (tenant, user_id, event_id) DO UPDATE SET minutes = EXCLUDED.minutes, sent_at = NULL")
        .bind(&t)
        .bind(u)
        .bind(id)
        .bind(m)
        .execute(&st.pool)
        .await?;
    Ok(Json(json!({"eventId": id, "minutes": m, "remindAt": starts - Duration::minutes(m as i64)})))
}

async fn remind_del(State(st): State<AppState>, h: HeaderMap, Path(event): Path<i64>) -> R {
    let (t, u) = (tenant(&h), user_id(&h)?);
    sqlx::query("DELETE FROM calendar_reminders WHERE tenant = $1 AND user_id = $2 AND event_id = $3").bind(&t).bind(u).bind(event).execute(&st.pool).await?;
    Ok(Json(json!({"ok": true})))
}

async fn alerts_put(State(st): State<AppState>, h: HeaderMap, Json(b): Json<Value>) -> R {
    let (t, u) = (tenant(&h), user_id(&h)?);
    let high = b.get("highImpact").and_then(Value::as_bool).unwrap_or(true);
    let currencies: Vec<String> = b.get("currencies").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(|s| s.trim().to_ascii_uppercase()).filter(|s| s.len() == 3 && s.chars().all(|c| c.is_ascii_alphabetic())).take(12).collect()).unwrap_or_default();
    if !high && currencies.is_empty() {
        return Err(ApiError::bad("Choose high-impact events or at least one currency."));
    }
    let m = minutes(&b)?;
    sqlx::query("INSERT INTO calendar_alerts (tenant, user_id, high_impact, currencies, minutes) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (tenant, user_id) DO UPDATE SET high_impact = EXCLUDED.high_impact, currencies = EXCLUDED.currencies, minutes = EXCLUDED.minutes, updated_at = now()")
        .bind(&t)
        .bind(u)
        .bind(high)
        .bind(&currencies)
        .bind(m)
        .execute(&st.pool)
        .await?;
    Ok(Json(json!({"alerts": {"highImpact": high, "currencies": currencies, "minutes": m}})))
}

async fn alerts_del(State(st): State<AppState>, h: HeaderMap) -> R {
    let (t, u) = (tenant(&h), user_id(&h)?);
    sqlx::query("DELETE FROM calendar_alerts WHERE tenant = $1 AND user_id = $2").bind(&t).bind(u).execute(&st.pool).await?;
    Ok(Json(json!({"alerts": null})))
}

/* ------------------------------------------------------------------ */
/* Back Office                                                         */
/* ------------------------------------------------------------------ */

#[derive(Deserialize, Default)]
pub struct AdminNewsQ {
    status: Option<String>,
    source: Option<String>,
    q: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

fn admin_item_json(r: &PgRow) -> Value {
    let mut v = item_json(r);
    v["hidden"] = json!(r.get::<bool, _>("hidden"));
    v["retagged"] = json!(r.get::<bool, _>("retagged"));
    v["sourceEnabled"] = json!(r.get::<bool, _>("source_enabled"));
    v["auto"] = json!({"symbols": r.get::<Vec<String>, _>("auto_symbols"), "countries": r.get::<Vec<String>, _>("auto_countries"), "importance": r.get::<i32, _>("auto_importance")});
    v["updatedBy"] = json!(r.get::<Option<String>, _>("updated_by"));
    v["updatedAt"] = json!(r.get::<Option<DateTime<Utc>>, _>("override_at"));
    v
}

async fn admin_news(State(st): State<AppState>, h: HeaderMap, Query(q): Query<AdminNewsQ>) -> R {
    staff(&h)?;
    let t = tenant(&h);
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let offset = q.offset.unwrap_or(0).clamp(0, 10_000);
    let status = match q.status.as_deref() {
        Some("pinned") => "pinned",
        Some("hidden") => "hidden",
        Some("visible") => "NOT hidden",
        Some("retagged") => "retagged",
        _ => "true",
    };
    let search = q.q.as_ref().map(|s| s.trim().chars().take(80).collect::<String>()).filter(|s| s.len() >= 2).map(|s| format!("%{}%", s.replace(['%', '_', '\\'], "")));
    let source = q.source.as_ref().filter(|s| is_slug(s)).cloned();
    let where_ = format!("{status} AND ($2::text IS NULL OR source_id = $2) AND ($3::text IS NULL OR title ILIKE $3 OR summary ILIKE $3)");
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!("{ITEMS_CTE} SELECT * FROM e WHERE {where_} ORDER BY pinned DESC, published_at DESC, id DESC LIMIT {limit} OFFSET {offset}")))
        .bind(&t)
        .bind(&source)
        .bind(&search)
        .fetch_all(&st.pool)
        .await?;
    let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!("{ITEMS_CTE} SELECT count(*) FROM e WHERE {where_}"))).bind(&t).bind(&source).bind(&search).fetch_one(&st.pool).await?;
    Ok(Json(json!({"items": rows.iter().map(admin_item_json).collect::<Vec<_>>(), "total": total, "limit": limit, "offset": offset})))
}

fn string_list(v: &Value, key: &str, check: impl Fn(&str) -> bool, max: usize) -> Result<Option<Vec<String>>, ApiError> {
    match v.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(vec![])),
        Some(Value::Array(a)) => {
            let mut out: Vec<String> = vec![];
            for x in a {
                let s = x.as_str().ok_or_else(|| ApiError::bad(format!("{key} must be a list of strings.")))?.trim().to_string();
                if !check(&s) {
                    return Err(ApiError::bad(format!("Unknown value in {key}: {s}")));
                }
                if !out.contains(&s) {
                    out.push(s);
                }
            }
            if out.len() > max {
                return Err(ApiError::bad(format!("At most {max} {key}.")));
            }
            Ok(Some(out))
        }
        _ => Err(ApiError::bad(format!("{key} must be a list."))),
    }
}

async fn admin_news_put(State(st): State<AppState>, h: HeaderMap, Path(id): Path<i64>, Json(b): Json<Value>) -> R {
    let who = staff(&h)?;
    let t = tenant(&h);
    let row = sqlx::query("SELECT symbols, countries FROM items WHERE id = $1").bind(id).fetch_optional(&st.pool).await?.ok_or_else(|| ApiError::not_found("Story"))?;
    let cur = sqlx::query("SELECT pinned, hidden, symbols, countries, importance FROM item_overrides WHERE tenant = $1 AND item_id = $2").bind(&t).bind(id).fetch_optional(&st.pool).await?;
    let get_bool = |k: &str| -> Result<Option<bool>, ApiError> {
        match b.get(k) {
            None => Ok(None),
            Some(Value::Bool(x)) => Ok(Some(*x)),
            _ => Err(ApiError::bad(format!("{k} must be true or false."))),
        }
    };
    let pinned = get_bool("pinned")?.unwrap_or_else(|| cur.as_ref().map(|c| c.get("pinned")).unwrap_or(false));
    let hidden = get_bool("hidden")?.unwrap_or_else(|| cur.as_ref().map(|c| c.get("hidden")).unwrap_or(false));
    if pinned && hidden {
        return Err(ApiError::bad("A hidden story can't be pinned."));
    }
    let symbols = match string_list(&b, "symbols", tagging::is_symbol, 6)? {
        Some(s) => Some(s),
        None => cur.as_ref().and_then(|c| c.get::<Option<Vec<String>>, _>("symbols")),
    };
    let countries = match string_list(&b, "countries", tagging::is_country, 6)? {
        Some(s) => Some(s),
        None => cur.as_ref().and_then(|c| c.get::<Option<Vec<String>>, _>("countries")),
    };
    let importance = match b.get("importance") {
        None => cur.as_ref().and_then(|c| c.get::<Option<i32>, _>("importance")),
        Some(Value::Null) => None,
        Some(v) => Some(v.as_i64().filter(|x| (0..=100).contains(x)).ok_or_else(|| ApiError::bad("importance is 0–100."))? as i32),
    };
    // effective currencies follow the effective countries + FX instruments
    let currencies = if symbols.is_some() || countries.is_some() {
        let c = countries.clone().unwrap_or_else(|| row.get("countries"));
        let s = symbols.clone().unwrap_or_else(|| row.get("symbols"));
        let mut set: std::collections::BTreeSet<String> = c.iter().filter_map(|x| tagging::currency_of(x)).filter(|x| ["USD", "EUR", "GBP", "JPY", "AUD", "CAD", "CHF", "NZD", "CNY", "INR"].contains(x)).map(str::to_string).collect();
        for x in &s {
            if tagging::asset_class(x) == Some("forex") {
                set.insert(x[..3].to_string());
                set.insert(x[3..].to_string());
            }
        }
        Some(set.into_iter().collect::<Vec<_>>())
    } else {
        None
    };
    sqlx::query(
        "INSERT INTO item_overrides (tenant, item_id, pinned, pinned_at, hidden, symbols, countries, currencies, importance, updated_by)
         VALUES ($1,$2,$3, CASE WHEN $3 THEN now() END, $4,$5,$6,$7,$8,$9)
         ON CONFLICT (tenant, item_id) DO UPDATE SET pinned = EXCLUDED.pinned,
           pinned_at = CASE WHEN EXCLUDED.pinned AND NOT item_overrides.pinned THEN now() WHEN EXCLUDED.pinned THEN item_overrides.pinned_at END,
           hidden = EXCLUDED.hidden, symbols = EXCLUDED.symbols, countries = EXCLUDED.countries, currencies = EXCLUDED.currencies,
           importance = EXCLUDED.importance, updated_by = EXCLUDED.updated_by, updated_at = now()",
    )
    .bind(&t)
    .bind(id)
    .bind(pinned)
    .bind(hidden)
    .bind(&symbols)
    .bind(&countries)
    .bind(&currencies)
    .bind(importance)
    .bind(&who)
    .execute(&st.pool)
    .await?;
    let mut detail = serde_json::Map::new();
    for k in ["pinned", "hidden", "symbols", "countries", "importance"] {
        if let Some(v) = b.get(k) {
            detail.insert(k.into(), v.clone());
        }
    }
    let action = if b.get("pinned").is_some() { if pinned { "news.pin" } else { "news.unpin" } } else if b.get("hidden").is_some() { if hidden { "news.hide" } else { "news.unhide" } } else { "news.retag" };
    store::audit(&st.pool, &t, &who, action, &format!("item:{id}"), Value::Object(detail)).await?;
    let r = sqlx::query(sqlx::AssertSqlSafe(format!("{ITEMS_CTE} SELECT * FROM e WHERE id = $2"))).bind(&t).bind(id).fetch_one(&st.pool).await?;
    Ok(Json(json!({"item": admin_item_json(&r)})))
}

async fn admin_news_reset(State(st): State<AppState>, h: HeaderMap, Path(id): Path<i64>) -> R {
    let who = staff(&h)?;
    let t = tenant(&h);
    sqlx::query("DELETE FROM item_overrides WHERE tenant = $1 AND item_id = $2").bind(&t).bind(id).execute(&st.pool).await?;
    store::audit(&st.pool, &t, &who, "news.reset", &format!("item:{id}"), json!({})).await?;
    let r = sqlx::query(sqlx::AssertSqlSafe(format!("{ITEMS_CTE} SELECT * FROM e WHERE id = $2"))).bind(&t).bind(id).fetch_optional(&st.pool).await?.ok_or_else(|| ApiError::not_found("Story"))?;
    Ok(Json(json!({"item": admin_item_json(&r)})))
}

async fn admin_sources(State(st): State<AppState>, h: HeaderMap) -> R {
    staff(&h)?;
    let rows = sqlx::query(
        "SELECT s.*, (SELECT count(*) FROM items i WHERE i.source_id = s.id AND i.published_at > now() - interval '24 hours') AS last24h FROM sources s ORDER BY s.enabled DESC, s.kind, s.name",
    )
    .fetch_all(&st.pool)
    .await?;
    let infoway_live = st.cfg.infoway && !st.cfg.infoway_key.is_empty();
    let list: Vec<Value> = rows
        .iter()
        .map(|r| {
            let kind: String = r.get("kind");
            json!({
                "id": r.get::<String, _>("id"), "name": r.get::<String, _>("name"), "url": r.get::<String, _>("url"), "homepage": r.get::<String, _>("homepage"),
                "country": r.get::<String, _>("country"), "kind": kind, "enabled": r.get::<bool, _>("enabled"), "terms": r.get::<String, _>("terms"),
                "intervalSecs": r.get::<i32, _>("interval_secs"), "lastFetchAt": r.get::<Option<DateTime<Utc>>, _>("last_fetch_at"),
                "lastOkAt": r.get::<Option<DateTime<Utc>>, _>("last_ok_at"), "lastError": r.get::<Option<String>, _>("last_error"),
                "items": r.get::<i32, _>("items_total"), "last24h": r.get::<i64, _>("last24h"),
                "connected": if kind == "provider" { json!(infoway_live) } else { Value::Null },
            })
        })
        .collect();
    Ok(Json(json!({"sources": list, "canToggle": tenant(&h) == PLATFORM_TENANT})))
}

async fn admin_source_put(State(st): State<AppState>, h: HeaderMap, Path(id): Path<String>, Json(b): Json<Value>) -> R {
    let who = staff(&h)?;
    let t = platform_only(&h, "Feed sources are managed by the platform.")?;
    let enabled = b.get("enabled").and_then(Value::as_bool).ok_or_else(|| ApiError::bad("enabled must be true or false."))?;
    let r = sqlx::query("UPDATE sources SET enabled = $2, updated_by = $3, updated_at = now(), last_fetch_at = CASE WHEN $2 THEN NULL ELSE last_fetch_at END WHERE id = $1").bind(&id).bind(enabled).bind(&who).execute(&st.pool).await?;
    if r.rows_affected() == 0 {
        return Err(ApiError::not_found("Source"));
    }
    store::audit(&st.pool, &t, &who, if enabled { "source.enable" } else { "source.disable" }, &format!("source:{id}"), json!({"enabled": enabled})).await?;
    admin_sources(State(st), h).await
}

async fn admin_source_refresh(State(st): State<AppState>, h: HeaderMap, Path(id): Path<String>) -> R {
    let who = staff(&h)?;
    let t = platform_only(&h, "Feed sources are fetched by the platform.")?;
    let _g = st.refresh.try_lock().map_err(|_| ApiError::new(StatusCode::TOO_MANY_REQUESTS, "busy", "A refresh is already running."))?;
    let last: Option<Option<DateTime<Utc>>> = sqlx::query_scalar("SELECT last_fetch_at FROM sources WHERE id = $1 AND kind <> 'provider'").bind(&id).fetch_optional(&st.pool).await?;
    let last = last.ok_or_else(|| ApiError::not_found("Source"))?;
    if last.is_some_and(|l| Utc::now() - l < Duration::seconds(60)) {
        return Err(ApiError::new(StatusCode::TOO_MANY_REQUESTS, "too_soon", "This feed was fetched less than a minute ago."));
    }
    let res = workers::fetch_source(&st, &id).await;
    store::audit(&st.pool, &t, &who, "source.refresh", &format!("source:{id}"), json!({"ok": res.is_ok()})).await?;
    match res {
        Ok(msg) => Ok(Json(json!({"ok": true, "result": msg}))),
        Err(e) => Err(ApiError::new(StatusCode::BAD_GATEWAY, "fetch_failed", format!("The feed could not be fetched: {e}"))),
    }
}

async fn admin_calendar(State(st): State<AppState>, h: HeaderMap, Query(q): Query<CalQ>) -> R {
    staff(&h)?;
    let (from, to) = range(&q)?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT {EVENT_COLS} FROM calendar_events WHERE starts_at >= $1 AND starts_at < $2 ORDER BY starts_at, id"))).bind(from).bind(to).fetch_all(&st.pool).await?;
    let events: Vec<Value> = rows
        .iter()
        .map(|r| {
            let mut v = event_json(r);
            v["feedImpact"] = json!(r.get::<i16, _>("feed_impact"));
            v["impactOverride"] = json!(r.get::<Option<i16>, _>("impact_override"));
            v["actualAt"] = json!(r.get::<Option<DateTime<Utc>>, _>("actual_at"));
            v
        })
        .collect();
    let reminders: i64 = sqlx::query_scalar("SELECT count(*) FROM calendar_reminders r JOIN calendar_events e ON e.id = r.event_id WHERE r.tenant = $1 AND e.starts_at >= $2 AND e.starts_at < $3").bind(tenant(&h)).bind(from).bind(to).fetch_one(&st.pool).await?;
    let subscribers: i64 = sqlx::query_scalar("SELECT count(*) FROM calendar_alerts WHERE tenant = $1").bind(tenant(&h)).fetch_one(&st.pool).await?;
    Ok(Json(json!({
        "events": events, "from": from, "to": to, "serverOffset": server_offset_hours(Utc::now()), "now": Utc::now(),
        "source": calendar_status(&st).await?, "cooldown": workers::calendar_cooldown(&st).await?,
        "reminders": reminders, "subscribers": subscribers, "licensedActuals": !st.cfg.te_key.is_empty(),
    })))
}

async fn admin_calendar_put(State(st): State<AppState>, h: HeaderMap, Path(id): Path<i64>, Json(b): Json<Value>) -> R {
    let who = staff(&h)?;
    let t = platform_only(&h, "The calendar is managed by the platform.")?;
    let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM calendar_events WHERE id = $1").bind(id).fetch_optional(&st.pool).await?;
    exists.ok_or_else(|| ApiError::not_found("Event"))?;
    if let Some(a) = b.get("actual") {
        let a = a.as_str().ok_or_else(|| ApiError::bad("actual must be text (empty clears it)."))?.trim();
        if a.chars().count() > 20 || (!a.is_empty() && calendar::figure(a).is_none() && !a.chars().all(|c| c.is_alphanumeric() || " .%-+".contains(c))) {
            return Err(ApiError::bad("Enter the actual value like 4.35%, 250K or -1.2M."));
        }
        store::set_actual(&st.pool, id, a, "staff").await?;
    }
    if let Some(i) = b.get("impact") {
        let v: Option<i16> = if i.is_null() { None } else { Some(i.as_i64().filter(|x| (0..=3).contains(x)).ok_or_else(|| ApiError::bad("impact is 0–3 or null."))? as i16) };
        sqlx::query("UPDATE calendar_events SET impact_override = $2, updated_at = now() WHERE id = $1").bind(id).bind(v).execute(&st.pool).await?;
    }
    store::audit(&st.pool, &t, &who, "calendar.edit", &format!("event:{id}"), b.clone()).await?;
    let r = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT {EVENT_COLS} FROM calendar_events WHERE id = $1"))).bind(id).fetch_one(&st.pool).await?;
    let mut v = event_json(&r);
    v["feedImpact"] = json!(r.get::<i16, _>("feed_impact"));
    v["impactOverride"] = json!(r.get::<Option<i16>, _>("impact_override"));
    Ok(Json(json!({"event": v})))
}

async fn admin_calendar_refresh(State(st): State<AppState>, h: HeaderMap) -> R {
    let who = staff(&h)?;
    let t = platform_only(&h, "The calendar is managed by the platform.")?;
    let _g = st.refresh.try_lock().map_err(|_| ApiError::new(StatusCode::TOO_MANY_REQUESTS, "busy", "A refresh is already running."))?;
    let wait = workers::calendar_cooldown(&st).await?;
    if wait > 0 {
        return Err(ApiError::new(StatusCode::TOO_MANY_REQUESTS, "too_soon", format!("The calendar export allows two requests per five minutes. Try again in {} s.", wait)));
    }
    let res = workers::refresh_calendar(&st).await;
    store::audit(&st.pool, &t, &who, "calendar.refresh", "calendar", json!({"ok": res.is_ok()})).await?;
    match res {
        Ok(msg) => Ok(Json(json!({"ok": true, "result": msg}))),
        Err(e) => Err(ApiError::new(StatusCode::BAD_GATEWAY, "fetch_failed", format!("The calendar could not be fetched: {e}"))),
    }
}

async fn admin_brief(State(st): State<AppState>, h: HeaderMap, body: Option<Json<Value>>) -> R {
    let who = staff(&h)?;
    // one brief per day for every broker's clients (no per-broker brief): a white-label editor must not replace it
    let t = platform_only(&h, "The daily brief is shared by every broker and is managed by the platform.")?;
    if st.cfg.anthropic_key.is_empty() {
        return Err(ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "unavailable", "The AI brief is not configured (ANTHROPIC_API_KEY)."));
    }
    let day = body.as_ref().and_then(|b| b.get("day")).and_then(Value::as_str).and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()).unwrap_or_else(brief::today);
    if day != brief::today() {
        return Err(ApiError::bad("Only today's brief can be regenerated."));
    }
    let _g = st.refresh.try_lock().map_err(|_| ApiError::new(StatusCode::TOO_MANY_REQUESTS, "busy", "A refresh is already running."))?;
    let b = brief::generate(&st, day, &who).await.map_err(|e| ApiError::new(StatusCode::BAD_GATEWAY, "ai_error", e.to_string()))?;
    store::audit(&st.pool, &t, &who, "brief.generate", &format!("brief:{day}"), json!({})).await?;
    Ok(Json(json!({"brief": b, "day": day})))
}

async fn admin_stats(State(st): State<AppState>, h: HeaderMap) -> R {
    staff(&h)?;
    let t = tenant(&h);
    let r = sqlx::query(
        "SELECT (SELECT count(*) FROM items WHERE published_at > now() - interval '24 hours') AS day,
                (SELECT count(*) FROM items) AS total,
                (SELECT count(*) FROM item_overrides WHERE tenant = $1 AND pinned) AS pinned,
                (SELECT count(*) FROM item_overrides WHERE tenant = $1 AND hidden) AS hidden,
                (SELECT count(*) FROM sources WHERE enabled) AS sources_on,
                (SELECT count(*) FROM sources) AS sources_total,
                (SELECT count(*) FROM sources WHERE enabled AND last_error IS NOT NULL) AS failing,
                (SELECT count(*) FROM calendar_events WHERE starts_at >= $2 AND starts_at < $2 + interval '7 days') AS week,
                (SELECT count(*) FROM calendar_events WHERE starts_at >= $2 AND starts_at < $2 + interval '7 days' AND COALESCE(impact_override, impact) = 3) AS high",
    )
    .bind(&t)
    .bind(server_week_start(Utc::now()))
    .fetch_one(&st.pool)
    .await?;
    let brief_today: Option<DateTime<Utc>> = sqlx::query_scalar("SELECT created_at FROM briefs WHERE day = $1").bind(brief::today()).fetch_optional(&st.pool).await?;
    Ok(Json(json!({
        "news": {"last24h": r.get::<i64, _>("day"), "total": r.get::<i64, _>("total"), "pinned": r.get::<i64, _>("pinned"), "hidden": r.get::<i64, _>("hidden")},
        "sources": {"enabled": r.get::<i64, _>("sources_on"), "total": r.get::<i64, _>("sources_total"), "failing": r.get::<i64, _>("failing")},
        "calendar": {"week": r.get::<i64, _>("week"), "high": r.get::<i64, _>("high"), "source": calendar_status(&st).await?},
        "brief": {"configured": st.cfg.brief && !st.cfg.anthropic_key.is_empty(), "today": brief_today, "model": st.cfg.ai_model},
    })))
}

async fn admin_audit(State(st): State<AppState>, h: HeaderMap) -> R {
    staff(&h)?;
    let rows = sqlx::query("SELECT staff, action, target, detail, at FROM audit WHERE tenant = $1 ORDER BY at DESC LIMIT 100").bind(tenant(&h)).fetch_all(&st.pool).await?;
    Ok(Json(json!({"entries": rows.iter().map(|r| json!({"staff": r.get::<String, _>("staff"), "action": r.get::<String, _>("action"), "target": r.get::<String, _>("target"), "detail": r.get::<Value, _>("detail"), "at": r.get::<DateTime<Utc>, _>("at")})).collect::<Vec<_>>()})))
}
