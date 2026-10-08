//! HTTP API (internal; every route except /health needs `X-Kalks-Internal`).
//!
//! Common headers: `X-Kalks-Tenant` (tenant slug, default "kalks"). `?lang=` picks the content language
//! (default "en"; missing translations fall back to English).
//!
//! Client Area (CRM BFF adds `X-Kalks-User-Id`, and for exams `X-Kalks-User-Name` + `X-Kalks-Tenant-Name`,
//! both percent-encoded):
//!
//! | route                                   | body                         | response                                               |
//! |-----------------------------------------|------------------------------|--------------------------------------------------------|
//! | `GET  /v1/catalog`                      | –                            | `{phases[], me}`: tree with per-chapter progress        |
//! | `GET  /v1/chapters/{slug}`              | –                            | `{chapter, phase, section, prev, next, progress}`      |
//! | `POST /v1/chapters/{slug}/progress`     | `{read_pct}`                 | `{progress}` (max of stored and sent)                  |
//! | `POST /v1/chapters/{slug}/quiz`         | `{answers: [int|null]}`      | `{results[], score, total, passed, completed, …}`       |
//! | `GET  /v1/exams/{phase}`                | –                            | `{exam, unlocked, attempts[], certificate}`            |
//! | `POST /v1/exams/{phase}`                | `{answers: [int]}`           | `{score, total, pct, passed, results[], certificate}`  |
//! | `GET  /v1/me/certificates`              | –                            | `{certificates[]}`                                     |
//! | `GET  /v1/glossary?q&category`          | –                            | `{terms[], categories[]}`                              |
//! | `GET  /v1/public/certificates/{code}`   | –                            | verification view (no user header needed)             |
//! | `GET  /v1/public/certificates/{code}/svg` | –                          | the certificate image (`image/svg+xml`)                |
//!
//! Chapter quizzes are graded here, never in the browser: the chapter view has no answers. A chapter is
//! complete when its quiz is passed (>= 60%). A phase's final exam unlocks when every published chapter of the
//! phase is complete; passing it (>= the exam's pass mark) issues the phase certificate.
//!
//! Phases are independent: no phase is locked behind an earlier one. Core phases have a fundamental and a
//! technical section; product phases (`elective: true` in the catalogue and admin tree, e.g. phase 9 "Kalks FX
//! Options" with a single `options` section) can be studied, examined and certified at any time.
//!
//! Back Office (admin BFF checks `content.read` / `content.write` and adds `X-Kalks-Staff`). Writes are
//! copy-on-write overrides for the staff member's tenant; `DELETE` resets a node to the platform default:
//!
//! `GET /v1/admin/tree` · `GET|PUT|DELETE /v1/admin/nodes/{kind}/{slug}` · `POST /v1/admin/chapters` ·
//! `POST /v1/admin/reorder` · `GET /v1/admin/stats` · `GET /v1/admin/audit` · `GET /v1/admin/glossary`

use axum::extract::{Path, Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sqlx::{PgPool, Row};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use subtle::ConstantTimeEq;

use crate::cert;
use crate::config::Config;
use crate::content::{self, Question};
use crate::store::{self, Node, TreePhase};

pub const QUIZ_PASS_PCT: i64 = 60;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub cfg: Arc<Config>,
    /// Read-only gateway database for the broker's module switches (modules.rs); None = always on.
    pub gateway: Option<PgPool>,
}

/* ------------------------------------------------------------------ */
/* Errors and request context                                          */
/* ------------------------------------------------------------------ */

pub struct ApiError(StatusCode, &'static str, String, Option<Value>);

impl ApiError {
    fn new(s: StatusCode, code: &'static str, msg: impl Into<String>) -> Self {
        Self(s, code, msg.into(), None)
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
        tracing::error!(error = %e, "academy internal error");
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
            err["issues"] = extra;
        }
        (self.0, Json(json!({ "error": err }))).into_response()
    }
}

type R<T = Json<Value>> = Result<T, ApiError>;

fn tenant(h: &HeaderMap) -> String {
    let t = h.get("x-kalks-tenant").and_then(|v| v.to_str().ok()).unwrap_or("kalks").trim().to_ascii_lowercase();
    if content::is_slug(&t) { t } else { "kalks".into() }
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

/// Percent-decoded header (names can be any script; the BFF sends encodeURIComponent()).
fn header_text(h: &HeaderMap, k: &str, max: usize) -> String {
    let raw = h.get(k).and_then(|v| v.to_str().ok()).unwrap_or("");
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() + 0 && i + 2 <= bytes.len() - 1 {
            if let Ok(b) = u8::from_str_radix(&raw[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).chars().filter(|c| !c.is_control()).take(max).collect::<String>().trim().to_string()
}

#[derive(Deserialize, Default)]
pub struct LangQ {
    lang: Option<String>,
    q: Option<String>,
    category: Option<String>,
}

fn lang(q: &LangQ) -> String {
    let l = q.lang.clone().unwrap_or_default().to_ascii_lowercase();
    if (2..=5).contains(&l.len()) && l.chars().all(|c| c.is_ascii_lowercase() || c == '-') { l } else { store::BASE_LANG.into() }
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
    // client routes: refused while the broker has the academy module off (modules.rs)
    let client = Router::new()
        .route("/v1/catalog", get(catalog))
        .route("/v1/chapters/{slug}", get(chapter_view))
        .route("/v1/chapters/{slug}/progress", post(chapter_progress))
        .route("/v1/chapters/{slug}/quiz", post(chapter_quiz))
        .route("/v1/exams/{phase}", get(exam_view).post(exam_submit))
        .route("/v1/me/certificates", get(my_certificates))
        .route("/v1/glossary", get(glossary))
        .route_layer(middleware::from_fn_with_state(st.clone(), crate::modules::gate));
    let internal = Router::new()
        .merge(client)
        .route("/v1/public/certificates/{code}", get(cert_verify))
        .route("/v1/public/certificates/{code}/svg", get(cert_svg))
        .route("/v1/admin/tree", get(admin_tree))
        .route("/v1/admin/nodes/{kind}/{slug}", get(admin_node).put(admin_put).delete(admin_reset))
        .route("/v1/admin/chapters", post(admin_create_chapter))
        .route("/v1/admin/reorder", post(admin_reorder))
        .route("/v1/admin/stats", get(admin_stats))
        .route("/v1/admin/audit", get(admin_audit))
        .route("/v1/admin/glossary", get(admin_glossary))
        .layer(middleware::from_fn_with_state(st.clone(), require_internal));
    Router::new().route("/health", get(|| async { Json(json!({"status": "ok"})) })).merge(internal).with_state(st)
}

/* ------------------------------------------------------------------ */
/* Shaping                                                             */
/* ------------------------------------------------------------------ */

fn questions(v: &Value) -> Vec<Question> {
    serde_json::from_value(v.get("quiz").or_else(|| v.get("questions")).cloned().unwrap_or(Value::Null)).unwrap_or_default()
}

/// Questions without answers / explanations (what the learner sees before answering).
fn public_questions(qs: &[Question]) -> Value {
    Value::Array(qs.iter().map(|q| json!({"question": q.question, "options": q.options})).collect())
}

fn i64_of(v: &Value, k: &str) -> i64 {
    v.get(k).and_then(Value::as_i64).unwrap_or(0)
}

#[derive(Clone, Default)]
struct Prog {
    read_pct: i32,
    quiz_best: Option<i32>,
    quiz_total: Option<i32>,
    completed: Option<chrono::DateTime<chrono::Utc>>,
    updated: Option<chrono::DateTime<chrono::Utc>>,
}

impl Prog {
    fn json(&self) -> Value {
        json!({"read_pct": self.read_pct, "quiz_best": self.quiz_best, "quiz_total": self.quiz_total, "completed": self.completed.is_some(), "completed_at": self.completed})
    }
}

async fn progress_map(pool: &PgPool, tenant: &str, user: i64) -> R<HashMap<String, Prog>> {
    let rows = sqlx::query("SELECT chapter, read_pct, quiz_best, quiz_total, completed_at, updated_at FROM chapter_progress WHERE tenant = $1 AND user_id = $2")
        .bind(tenant)
        .bind(user)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (r.get::<String, _>(0), Prog { read_pct: r.get(1), quiz_best: r.get(2), quiz_total: r.get(3), completed: r.get(4), updated: r.get(5) })
        })
        .collect())
}

async fn touch_day(pool: &PgPool, tenant: &str, user: i64) {
    let _ = sqlx::query("INSERT INTO learning_days (tenant, user_id, day) VALUES ($1, $2, (now() AT TIME ZONE 'utc')::date) ON CONFLICT DO NOTHING")
        .bind(tenant)
        .bind(user)
        .execute(pool)
        .await;
}

async fn streak(pool: &PgPool, tenant: &str, user: i64) -> R<(i64, Vec<String>)> {
    let days: Vec<chrono::NaiveDate> = sqlx::query_scalar("SELECT day FROM learning_days WHERE tenant = $1 AND user_id = $2 AND day > (now() AT TIME ZONE 'utc')::date - 400 ORDER BY day DESC")
        .bind(tenant)
        .bind(user)
        .fetch_all(pool)
        .await?;
    let today = chrono::Utc::now().date_naive();
    let set: HashSet<chrono::NaiveDate> = days.iter().copied().collect();
    let mut d = if set.contains(&today) { today } else { today - chrono::Duration::days(1) };
    let mut n = 0;
    while set.contains(&d) {
        n += 1;
        d -= chrono::Duration::days(1);
    }
    let week: Vec<String> = (0..7).rev().map(|i| today - chrono::Duration::days(i)).filter(|x| set.contains(x)).map(|x| x.to_string()).collect();
    Ok((n, week))
}

fn locate<'a>(tree: &'a [TreePhase], slug: &str) -> Option<(usize, usize, usize)> {
    for (pi, p) in tree.iter().enumerate() {
        for (si, s) in p.sections.iter().enumerate() {
            if let Some(ci) = s.chapters.iter().position(|c| c.slug == slug) {
                return Some((pi, si, ci));
            }
        }
    }
    None
}

fn chapter_card(c: &Node, p: Option<&Prog>) -> Value {
    json!({
        "slug": c.slug, "title": c.s("title"), "summary": c.s("summary"), "minutes": i64_of(&c.data, "minutes"),
        "order": c.ord, "questions": c.data.get("quiz").and_then(Value::as_array).map(|a| a.len()).unwrap_or(0),
        "progress": p.cloned().unwrap_or_default().json(),
    })
}

/* ------------------------------------------------------------------ */
/* Client Area                                                         */
/* ------------------------------------------------------------------ */

async fn catalog(State(st): State<AppState>, h: HeaderMap, Query(q): Query<LangQ>) -> R {
    let (t, user, lang) = (tenant(&h), user_id(&h)?, lang(&q));
    let tree = store::course(&st.pool, &t, &lang, true).await?;
    let prog = progress_map(&st.pool, &t, user).await?;
    let exams: HashMap<String, (i32, bool, i64)> = sqlx::query("SELECT phase, max(pct), bool_or(passed), count(*) FROM exam_attempts WHERE tenant = $1 AND user_id = $2 GROUP BY phase")
        .bind(&t)
        .bind(user)
        .fetch_all(&st.pool)
        .await?
        .into_iter()
        .map(|r| (r.get::<String, _>(0), (r.get::<i32, _>(1), r.get::<bool, _>(2), r.get::<i64, _>(3))))
        .collect();
    let certs: HashMap<String, (String, chrono::DateTime<chrono::Utc>)> =
        sqlx::query("SELECT phase, code, issued_at FROM certificates WHERE tenant = $1 AND user_id = $2 AND NOT revoked")
            .bind(&t)
            .bind(user)
            .fetch_all(&st.pool)
            .await?
            .into_iter()
            .map(|r| (r.get::<String, _>(0), (r.get::<String, _>(1), r.get(2))))
            .collect();

    let (mut done_all, mut total_all, mut minutes_done, mut minutes_all) = (0usize, 0usize, 0i64, 0i64);
    let mut quiz_pcts = vec![];
    let mut phases = vec![];
    for p in &tree {
        let chapters: Vec<&Node> = p.chapters().collect();
        let done = chapters.iter().filter(|c| prog.get(&c.slug).is_some_and(|x| x.completed.is_some())).count();
        done_all += done;
        total_all += chapters.len();
        for c in &chapters {
            let m = i64_of(&c.data, "minutes");
            minutes_all += m;
            if let Some(x) = prog.get(&c.slug) {
                if x.completed.is_some() {
                    minutes_done += m;
                }
                if let (Some(b), Some(t)) = (x.quiz_best, x.quiz_total)
                    && t > 0
                {
                    quiz_pcts.push(b as f64 * 100.0 / t as f64);
                }
            }
        }
        let exam = p.exam.as_ref().map(|e| {
            let (best, passed, attempts) = exams.get(&p.node.slug).copied().unwrap_or((0, false, 0));
            json!({
                "questions": e.data.get("questions").and_then(Value::as_array).map(|a| a.len()).unwrap_or(0),
                "pass_mark": i64_of(&e.data, "pass_mark"), "unlocked": done == chapters.len() && !chapters.is_empty(),
                "best_pct": if attempts > 0 { Some(best) } else { None }, "passed": passed, "attempts": attempts,
            })
        });
        phases.push(json!({
            "slug": p.node.slug, "order": p.node.ord, "title": p.node.s("title"), "level": p.node.s("level"), "summary": p.node.s("summary"),
            "elective": p.is_elective(),
            "minutes": chapters.iter().map(|c| i64_of(&c.data, "minutes")).sum::<i64>(),
            "progress": {"done": done, "total": chapters.len()},
            "sections": p.sections.iter().map(|s| json!({
                "slug": s.node.slug, "track": s.node.s("track"), "title": s.node.s("title"), "summary": s.node.s("summary"),
                "chapters": s.chapters.iter().map(|c| chapter_card(c, prog.get(&c.slug))).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "exam": exam,
            "certificate": certs.get(&p.node.slug).map(|(code, at)| json!({"code": code, "issued_at": at})),
        }));
    }
    // continue where the learner left off: most recently touched chapter that isn't complete, else the next one
    let flat: Vec<(&TreePhase, &Node)> = tree.iter().flat_map(|p| p.chapters().map(move |c| (p, c))).collect();
    let recent = flat.iter().filter(|(_, c)| prog.get(&c.slug).is_some_and(|x| x.completed.is_none())).max_by_key(|(_, c)| prog.get(&c.slug).and_then(|x| x.updated));
    let next = recent.or_else(|| flat.iter().find(|(_, c)| prog.get(&c.slug).is_none_or(|x| x.completed.is_none())));
    let cont = next.map(|(p, c)| {
        json!({"slug": c.slug, "title": c.s("title"), "minutes": i64_of(&c.data, "minutes"), "phase": {"slug": p.node.slug, "order": p.node.ord, "title": p.node.s("title")},
               "read_pct": prog.get(&c.slug).map(|x| x.read_pct).unwrap_or(0), "started": prog.contains_key(&c.slug)})
    });
    let (streak_days, week) = streak(&st.pool, &t, user).await?;
    Ok(Json(json!({
        "lang": lang,
        "phases": phases,
        "me": {
            "chapters_done": done_all, "chapters_total": total_all, "minutes_done": minutes_done, "minutes_total": minutes_all,
            "quiz_avg": if quiz_pcts.is_empty() { None } else { Some((quiz_pcts.iter().sum::<f64>() / quiz_pcts.len() as f64).round()) },
            "certificates": certs.len(), "streak": streak_days, "active_days": week, "continue": cont,
        }
    })))
}

async fn chapter_view(State(st): State<AppState>, h: HeaderMap, Path(slug): Path<String>, Query(q): Query<LangQ>) -> R {
    let (t, user, lang) = (tenant(&h), user_id(&h)?, lang(&q));
    let tree = store::course(&st.pool, &t, &lang, true).await?;
    let (pi, si, ci) = locate(&tree, &slug).ok_or_else(|| ApiError::not_found("Chapter"))?;
    let node = store::node(&st.pool, &t, &lang, "chapter", &slug).await?.ok_or_else(|| ApiError::not_found("Chapter"))?;
    let flat: Vec<&Node> = tree.iter().flat_map(|p| p.chapters()).collect();
    let idx = flat.iter().position(|c| c.slug == slug).unwrap_or(0);
    let nav = |c: Option<&&Node>| c.map(|c| json!({"slug": c.slug, "title": c.s("title")}));
    let p = &tree[pi];
    let s = &p.sections[si];
    let prog = progress_map(&st.pool, &t, user).await?;
    let qs = questions(&node.data);
    Ok(Json(json!({
        "chapter": {
            "slug": node.slug, "title": node.s("title"), "summary": node.s("summary"), "body": node.s("body"),
            "takeaways": node.data.get("takeaways").cloned().unwrap_or(json!([])), "practice": node.data.get("practice").cloned().unwrap_or(Value::Null),
            "minutes": i64_of(&node.data, "minutes"), "words": i64_of(&node.data, "words"), "quiz": public_questions(&qs),
            "updated_at": node.updated_at, "lang": node.lang,
        },
        "phase": {"slug": p.node.slug, "order": p.node.ord, "title": p.node.s("title"), "level": p.node.s("level"), "elective": p.is_elective()},
        "section": {"slug": s.node.slug, "track": s.node.s("track"), "title": s.node.s("title"), "index": ci + 1, "count": s.chapters.len(),
                    "chapters": s.chapters.iter().map(|c| json!({"slug": c.slug, "title": c.s("title"), "completed": prog.get(&c.slug).is_some_and(|x| x.completed.is_some())})).collect::<Vec<_>>()},
        "prev": nav(if idx > 0 { flat.get(idx - 1) } else { None }),
        "next": nav(flat.get(idx + 1)),
        "progress": prog.get(&slug).cloned().unwrap_or_default().json(),
    })))
}

/// Phase slug of a published chapter (for progress rows), or 404.
async fn chapter_phase(st: &AppState, t: &str, lang: &str, slug: &str) -> R<(Vec<TreePhase>, usize)> {
    let tree = store::course(&st.pool, t, lang, true).await?;
    let (pi, _, _) = locate(&tree, slug).ok_or_else(|| ApiError::not_found("Chapter"))?;
    Ok((tree, pi))
}

#[derive(Deserialize)]
struct ProgressBody {
    read_pct: i32,
}

async fn chapter_progress(State(st): State<AppState>, h: HeaderMap, Path(slug): Path<String>, Query(q): Query<LangQ>, Json(b): Json<ProgressBody>) -> R {
    let (t, user, lang) = (tenant(&h), user_id(&h)?, lang(&q));
    let (tree, pi) = chapter_phase(&st, &t, &lang, &slug).await?;
    let pct = b.read_pct.clamp(0, 100);
    let r = sqlx::query(
        "INSERT INTO chapter_progress (tenant, user_id, chapter, phase, read_pct) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (tenant, user_id, chapter) DO UPDATE SET read_pct = GREATEST(chapter_progress.read_pct, EXCLUDED.read_pct), phase = EXCLUDED.phase, updated_at = now()
         RETURNING read_pct, quiz_best, quiz_total, completed_at",
    )
    .bind(&t)
    .bind(user)
    .bind(&slug)
    .bind(&tree[pi].node.slug)
    .bind(pct)
    .fetch_one(&st.pool)
    .await?;
    touch_day(&st.pool, &t, user).await;
    let p = Prog { read_pct: r.get(0), quiz_best: r.get(1), quiz_total: r.get(2), completed: r.get(3), updated: None };
    Ok(Json(json!({ "progress": p.json() })))
}

#[derive(Deserialize)]
struct AnswersBody {
    answers: Vec<Option<i64>>,
}

/// Grades answered questions. Returns (per-question results, score, all answered).
fn grade(qs: &[Question], answers: &[Option<i64>]) -> (Vec<Value>, i64, bool) {
    let mut results = vec![];
    let mut score = 0;
    let mut all = true;
    for (i, q) in qs.iter().enumerate() {
        match answers.get(i).copied().flatten() {
            Some(a) if a >= 0 && (a as usize) < q.options.len() => {
                let ok = a as usize == q.answer;
                score += i64::from(ok);
                results.push(json!({"index": i, "choice": a, "correct": ok, "answer": q.answer, "explanation": q.explanation}));
            }
            _ => all = false,
        }
    }
    (results, score, all)
}

async fn chapter_quiz(State(st): State<AppState>, h: HeaderMap, Path(slug): Path<String>, Query(q): Query<LangQ>, Json(b): Json<AnswersBody>) -> R {
    let (t, user, lang) = (tenant(&h), user_id(&h)?, lang(&q));
    let (tree, pi) = chapter_phase(&st, &t, &lang, &slug).await?;
    let node = store::node(&st.pool, &t, &lang, "chapter", &slug).await?.ok_or_else(|| ApiError::not_found("Chapter"))?;
    let qs = questions(&node.data);
    if qs.is_empty() {
        return Err(ApiError::bad("This chapter has no quiz."));
    }
    if b.answers.len() > qs.len() {
        return Err(ApiError::bad("Too many answers."));
    }
    let (results, score, all) = grade(&qs, &b.answers);
    let total = qs.len() as i64;
    let passed = all && score * 100 >= QUIZ_PASS_PCT * total;
    let mut completed_now = false;
    let mut completed = false;
    if all {
        let r = sqlx::query(
            "INSERT INTO chapter_progress (tenant, user_id, chapter, phase, read_pct, quiz_best, quiz_total, attempts, completed_at)
             VALUES ($1, $2, $3, $4, 0, $5, $6, 1, CASE WHEN $7 THEN now() END)
             ON CONFLICT (tenant, user_id, chapter) DO UPDATE SET
               quiz_best = GREATEST(COALESCE(chapter_progress.quiz_best, 0), EXCLUDED.quiz_best), quiz_total = EXCLUDED.quiz_total,
               attempts = chapter_progress.attempts + 1, phase = EXCLUDED.phase,
               completed_at = COALESCE(chapter_progress.completed_at, EXCLUDED.completed_at), updated_at = now()
             RETURNING completed_at, (xmax = 0) AS inserted, completed_at >= now() - interval '1 second' AS fresh",
        )
        .bind(&t)
        .bind(user)
        .bind(&slug)
        .bind(&tree[pi].node.slug)
        .bind(score as i32)
        .bind(total as i32)
        .bind(passed)
        .fetch_one(&st.pool)
        .await?;
        let at: Option<chrono::DateTime<chrono::Utc>> = r.get(0);
        completed = at.is_some();
        completed_now = passed && r.get::<Option<bool>, _>(2).unwrap_or(false);
        touch_day(&st.pool, &t, user).await;
    }
    let prog = progress_map(&st.pool, &t, user).await?;
    let chapters: Vec<&Node> = tree[pi].chapters().collect();
    let done = chapters.iter().filter(|c| prog.get(&c.slug).is_some_and(|x| x.completed.is_some())).count();
    Ok(Json(json!({
        "results": results, "answered": results.len(), "score": score, "total": total, "all_answered": all, "passed": passed,
        "pass_pct": QUIZ_PASS_PCT, "completed": completed, "completed_now": completed_now,
        "phase": {"slug": tree[pi].node.slug, "done": done, "total": chapters.len(), "exam_unlocked": done == chapters.len()},
    })))
}

async fn exam_parts(st: &AppState, t: &str, lang: &str, user: i64, phase: &str) -> R<(TreePhase, Node, usize, usize)> {
    let tree = store::course(&st.pool, t, lang, true).await?;
    let p = tree.into_iter().find(|p| p.node.slug == phase).ok_or_else(|| ApiError::not_found("Phase"))?;
    let exam = store::node(&st.pool, t, lang, "exam", phase).await?.filter(|e| e.published).ok_or_else(|| ApiError::not_found("Exam"))?;
    let prog = progress_map(&st.pool, t, user).await?;
    let total = p.chapters().count();
    let done = p.chapters().filter(|c| prog.get(&c.slug).is_some_and(|x| x.completed.is_some())).count();
    Ok((p, exam, done, total))
}

async fn cert_json(pool: &PgPool, t: &str, user: i64, phase: &str) -> R<Value> {
    let r = sqlx::query("SELECT code, issued_at, score_pct FROM certificates WHERE tenant = $1 AND user_id = $2 AND phase = $3 AND NOT revoked")
        .bind(t)
        .bind(user)
        .bind(phase)
        .fetch_optional(pool)
        .await?;
    Ok(r.map(|r| json!({"code": r.get::<String, _>(0), "issued_at": r.get::<chrono::DateTime<chrono::Utc>, _>(1), "score_pct": r.get::<i32, _>(2)})).unwrap_or(Value::Null))
}

async fn exam_view(State(st): State<AppState>, h: HeaderMap, Path(phase): Path<String>, Query(q): Query<LangQ>) -> R {
    let (t, user, lang) = (tenant(&h), user_id(&h)?, lang(&q));
    let (p, exam, done, total) = exam_parts(&st, &t, &lang, user, &phase).await?;
    let qs = questions(&exam.data);
    let attempts: Vec<Value> = sqlx::query("SELECT pct, passed, created_at FROM exam_attempts WHERE tenant = $1 AND user_id = $2 AND phase = $3 ORDER BY created_at DESC LIMIT 10")
        .bind(&t)
        .bind(user)
        .bind(&phase)
        .fetch_all(&st.pool)
        .await?
        .into_iter()
        .map(|r| json!({"pct": r.get::<i32, _>(0), "passed": r.get::<bool, _>(1), "at": r.get::<chrono::DateTime<chrono::Utc>, _>(2)}))
        .collect();
    Ok(Json(json!({
        "phase": {"slug": p.node.slug, "order": p.node.ord, "title": p.node.s("title"), "level": p.node.s("level"), "elective": p.is_elective()},
        "exam": {"pass_mark": i64_of(&exam.data, "pass_mark"), "questions": public_questions(&qs), "count": qs.len()},
        "unlocked": done == total && total > 0, "chapters_done": done, "chapters_total": total,
        "attempts": attempts, "certificate": cert_json(&st.pool, &t, user, &phase).await?,
    })))
}

async fn exam_submit(State(st): State<AppState>, h: HeaderMap, Path(phase): Path<String>, Query(q): Query<LangQ>, Json(b): Json<AnswersBody>) -> R {
    let (t, user, lang) = (tenant(&h), user_id(&h)?, lang(&q));
    let (p, exam, done, total) = exam_parts(&st, &t, &lang, user, &phase).await?;
    if done < total || total == 0 {
        return Err(ApiError::new(StatusCode::CONFLICT, "exam_locked", format!("Complete all {total} chapters of this phase first ({done} done).")));
    }
    let qs = questions(&exam.data);
    let (results, score, all) = grade(&qs, &b.answers);
    if !all || b.answers.len() != qs.len() {
        return Err(ApiError::bad("Answer every question before submitting."));
    }
    let n = qs.len() as i64;
    let pct = ((score * 100) as f64 / n as f64).round() as i32;
    let pass_mark = i64_of(&exam.data, "pass_mark").clamp(1, 100) as i32;
    let passed = pct >= pass_mark;
    sqlx::query("INSERT INTO exam_attempts (tenant, user_id, phase, score, total, pct, passed, answers) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(&t)
        .bind(user)
        .bind(&phase)
        .bind(score as i32)
        .bind(n as i32)
        .bind(pct)
        .bind(passed)
        .bind(json!(b.answers))
        .execute(&st.pool)
        .await?;
    touch_day(&st.pool, &t, user).await;
    let mut issued = false;
    if passed {
        let learner = header_text(&h, "x-kalks-user-name", 80);
        let learner = if learner.is_empty() { "Kalks learner".to_string() } else { learner };
        let brand = header_text(&h, "x-kalks-tenant-name", 60);
        let brand = if brand.is_empty() { "Kalks".to_string() } else { brand };
        for _ in 0..3 {
            let r = sqlx::query(
                "INSERT INTO certificates (code, tenant, tenant_name, user_id, learner_name, phase, phase_order, phase_title, level, score_pct)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT (tenant, user_id, phase) DO NOTHING",
            )
            .bind(cert::new_code())
            .bind(&t)
            .bind(&brand)
            .bind(user)
            .bind(&learner)
            .bind(&phase)
            .bind(p.node.ord)
            .bind(p.node.s("title"))
            .bind(p.node.s("level"))
            .bind(pct)
            .execute(&st.pool)
            .await;
            match r {
                Ok(x) => {
                    issued = x.rows_affected() == 1;
                    break;
                }
                Err(sqlx::Error::Database(e)) if e.constraint() == Some("certificates_pkey") => continue,
                Err(e) => return Err(e.into()),
            }
        }
    }
    Ok(Json(json!({
        "score": score, "total": n, "pct": pct, "pass_mark": pass_mark, "passed": passed, "results": results,
        "certificate": cert_json(&st.pool, &t, user, &phase).await?, "certificate_issued": issued,
    })))
}

async fn my_certificates(State(st): State<AppState>, h: HeaderMap) -> R {
    let (t, user) = (tenant(&h), user_id(&h)?);
    let rows = sqlx::query("SELECT code, phase, phase_order, phase_title, level, score_pct, issued_at, learner_name FROM certificates WHERE tenant = $1 AND user_id = $2 AND NOT revoked ORDER BY phase_order")
        .bind(&t)
        .bind(user)
        .fetch_all(&st.pool)
        .await?;
    let list: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            let code: String = r.get(0);
            json!({"code": code, "phase": r.get::<String, _>(1), "phase_order": r.get::<i32, _>(2), "phase_title": r.get::<String, _>(3), "level": r.get::<String, _>(4),
                   "score_pct": r.get::<i32, _>(5), "issued_at": r.get::<chrono::DateTime<chrono::Utc>, _>(6), "learner_name": r.get::<String, _>(7),
                   "verify_url": format!("{}/certificate/{code}", st.cfg.verify_base_url)})
        })
        .collect();
    Ok(Json(json!({ "certificates": list })))
}

fn term_json(n: &Node) -> Value {
    json!({"slug": n.slug, "term": n.s("term"), "category": n.s("category"), "definition": n.s("definition"), "related": n.data.get("related").cloned().unwrap_or(json!([]))})
}

async fn glossary(State(st): State<AppState>, h: HeaderMap, Query(q): Query<LangQ>) -> R {
    let (t, lang) = (tenant(&h), lang(&q));
    let mut ns: Vec<Node> = store::nodes(&st.pool, &t, &lang, &["term"], true).await?.into_iter().filter(|n| n.published).collect();
    ns.sort_by_key(|n| n.s("term").to_lowercase());
    let mut cats: HashMap<String, usize> = HashMap::new();
    for n in &ns {
        *cats.entry(n.s("category")).or_default() += 1;
    }
    let needle = q.q.clone().unwrap_or_default().trim().to_lowercase();
    let cat = q.category.clone().unwrap_or_default();
    let names: HashMap<String, String> = ns.iter().map(|n| (n.slug.clone(), n.s("term"))).collect();
    let terms: Vec<Value> = ns
        .iter()
        .filter(|n| cat.is_empty() || n.s("category") == cat)
        .filter(|n| needle.is_empty() || n.s("term").to_lowercase().contains(&needle) || n.s("definition").to_lowercase().contains(&needle))
        .map(|n| {
            let mut v = term_json(n);
            let rel: Vec<Value> = v["related"].as_array().cloned().unwrap_or_default().iter().filter_map(|r| r.as_str()).filter_map(|r| names.get(r).map(|name| json!({"slug": r, "term": name}))).collect();
            v["related"] = Value::Array(rel);
            v
        })
        .collect();
    let mut categories: Vec<Value> = cats.into_iter().map(|(k, v)| json!({"name": k, "count": v})).collect();
    categories.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(Json(json!({"terms": terms, "categories": categories, "total": ns.len()})))
}

async fn cert_row(pool: &PgPool, code: &str) -> R<sqlx::postgres::PgRow> {
    if !cert::valid_code(code) {
        return Err(ApiError::not_found("Certificate"));
    }
    sqlx::query("SELECT code, tenant_name, learner_name, phase_order, phase_title, level, score_pct, issued_at, revoked FROM certificates WHERE code = $1")
        .bind(code)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ApiError::not_found("Certificate"))
}

async fn cert_verify(State(st): State<AppState>, Path(code): Path<String>) -> R {
    let r = cert_row(&st.pool, &code).await?;
    Ok(Json(json!({
        "code": r.get::<String, _>(0), "tenant_name": r.get::<String, _>(1), "learner_name": r.get::<String, _>(2), "phase_order": r.get::<i32, _>(3),
        "phase_title": r.get::<String, _>(4), "level": r.get::<String, _>(5), "score_pct": r.get::<i32, _>(6),
        "issued_at": r.get::<chrono::DateTime<chrono::Utc>, _>(7), "revoked": r.get::<bool, _>(8), "valid": !r.get::<bool, _>(8),
    })))
}

async fn cert_svg(State(st): State<AppState>, Path(code): Path<String>) -> Result<Response, ApiError> {
    let r = cert_row(&st.pool, &code).await?;
    let code: String = r.get(0);
    let issued: chrono::DateTime<chrono::Utc> = r.get(7);
    let verify = format!("{}/certificate/{code}", st.cfg.verify_base_url);
    let verify_short = verify.split("://").nth(1).unwrap_or(&verify).to_string();
    let body = cert::svg(&cert::CertView {
        code: &code,
        tenant_name: &r.get::<String, _>(1),
        learner: &r.get::<String, _>(2),
        phase_order: r.get(3),
        phase_title: &r.get::<String, _>(4),
        level: &r.get::<String, _>(5),
        score_pct: r.get(6),
        issued: &issued.format("%d %B %Y").to_string(),
        verify_url: &verify_short,
        revoked: r.get(8),
    });
    let mut res = body.into_response();
    res.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("image/svg+xml; charset=utf-8"));
    res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=300"));
    res.headers_mut().insert("content-security-policy", HeaderValue::from_static("default-src 'none'; style-src 'unsafe-inline'"));
    Ok(res)
}

/* ------------------------------------------------------------------ */
/* Back Office CMS                                                     */
/* ------------------------------------------------------------------ */

const KINDS: [&str; 5] = ["phase", "section", "chapter", "exam", "term"];

fn check_kind(kind: &str) -> R<()> {
    if KINDS.contains(&kind) { Ok(()) } else { Err(ApiError::not_found("Content type")) }
}

type Stat = (i64, i64, Option<f64>);

async fn chapter_stats(pool: &PgPool, t: &str) -> R<HashMap<String, Stat>> {
    Ok(sqlx::query(
        "SELECT chapter, count(*), count(completed_at), avg(CASE WHEN quiz_total > 0 THEN quiz_best * 100.0 / quiz_total END)::float8
         FROM chapter_progress WHERE tenant = $1 GROUP BY chapter",
    )
    .bind(t)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| (r.get::<String, _>(0), (r.get::<i64, _>(1), r.get::<i64, _>(2), r.get::<Option<f64>, _>(3))))
    .collect())
}

fn meta(n: &Node) -> Value {
    json!({"slug": n.slug, "published": n.published, "source": n.source(), "updated_at": n.updated_at, "updated_by": n.updated_by, "lang": n.lang, "order": n.ord, "parent": n.parent})
}

fn merge(a: &mut Value, b: Value) {
    if let (Some(a), Value::Object(b)) = (a.as_object_mut(), b) {
        for (k, v) in b {
            a.insert(k, v);
        }
    }
}

async fn admin_tree(State(st): State<AppState>, h: HeaderMap, Query(q): Query<LangQ>) -> R {
    let (t, lang) = (tenant(&h), lang(&q));
    let tree = store::course(&st.pool, &t, &lang, false).await?;
    let stats = chapter_stats(&st.pool, &t).await?;
    let langs: Vec<String> = sqlx::query_scalar("SELECT DISTINCT lang FROM content_nodes ORDER BY lang").fetch_all(&st.pool).await?;
    let phases: Vec<Value> = tree
        .iter()
        .map(|p| {
            let mut v = meta(&p.node);
            merge(&mut v, json!({"title": p.node.s("title"), "level": p.node.s("level"), "summary": p.node.s("summary"), "elective": p.is_elective()}));
            v["exam"] = p.exam.as_ref().map(|e| {
                let mut x = meta(e);
                merge(&mut x, json!({"pass_mark": i64_of(&e.data, "pass_mark"), "questions": e.data.get("questions").and_then(Value::as_array).map(|a| a.len()).unwrap_or(0)}));
                x
            }).unwrap_or(Value::Null);
            v["sections"] = Value::Array(
                p.sections
                    .iter()
                    .map(|s| {
                        let mut sv = meta(&s.node);
                        merge(&mut sv, json!({"title": s.node.s("title"), "summary": s.node.s("summary"), "track": s.node.s("track")}));
                        sv["chapters"] = Value::Array(
                            s.chapters
                                .iter()
                                .map(|c| {
                                    let mut cv = meta(c);
                                    let (learners, completed, quiz) = stats.get(&c.slug).copied().unwrap_or((0, 0, None));
                                    merge(&mut cv, json!({
                                        "title": c.s("title"), "summary": c.s("summary"), "words": i64_of(&c.data, "words"), "minutes": i64_of(&c.data, "minutes"),
                                        "questions": c.data.get("quiz").and_then(Value::as_array).map(|a| a.len()).unwrap_or(0),
                                        "learners": learners, "completed": completed, "quiz_avg": quiz.map(|x| x.round()),
                                    }));
                                    cv
                                })
                                .collect(),
                        );
                        sv
                    })
                    .collect(),
            );
            v
        })
        .collect();
    Ok(Json(json!({"tenant": t, "lang": lang, "languages": langs, "phases": phases})))
}

async fn admin_node(State(st): State<AppState>, h: HeaderMap, Path((kind, slug)): Path<(String, String)>, Query(q): Query<LangQ>) -> R {
    check_kind(&kind)?;
    let (t, lang) = (tenant(&h), lang(&q));
    let n = store::node(&st.pool, &t, &lang, &kind, &slug).await?.ok_or_else(|| ApiError::not_found("Item"))?;
    let default = if n.tenant != store::DEFAULT_TENANT {
        store::node(&st.pool, store::DEFAULT_TENANT, &lang, &kind, &slug).await?.map(|d| json!({"data": d.data, "published": d.published, "order": d.ord}))
    } else {
        None
    };
    let mut v = meta(&n);
    v["kind"] = json!(kind);
    v["data"] = n.data.clone();
    v["default"] = default.unwrap_or(Value::Null);
    Ok(Json(json!({ "node": v })))
}

#[derive(Deserialize)]
struct PutBody {
    lang: Option<String>,
    parent: Option<String>,
    order: Option<i32>,
    published: Option<bool>,
    data: Option<Value>,
}

const DATA_KEYS: &[(&str, &[&str])] = &[
    ("phase", &["title", "level", "summary"]),
    ("section", &["title", "summary", "track"]),
    ("chapter", &["title", "summary", "body", "takeaways", "practice", "quiz"]),
    ("exam", &["pass_mark", "questions"]),
    ("term", &["term", "category", "definition", "related"]),
];

/// Validates and normalises node data for a kind. Returns (errors, warnings).
fn validate(kind: &str, data: &mut Value, publishing: bool) -> (Vec<String>, Vec<String>) {
    let mut errors = vec![];
    let mut warnings = vec![];
    let s = |k: &str| data.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
    match kind {
        "phase" => {
            if s("title").is_empty() {
                errors.push("Title is required.".into());
            }
            if !content::LEVELS.contains(&s("level").as_str()) {
                errors.push(format!("Level must be one of {}.", content::LEVELS.join(", ")));
            }
        }
        "section" => {
            if s("title").is_empty() {
                errors.push("Title is required.".into());
            }
            if !content::is_track(&s("track")) {
                errors.push(format!("Track must be one of {}.", content::TRACKS.join(", ")));
            }
        }
        "chapter" => {
            if s("title").is_empty() {
                errors.push("Title is required.".into());
            }
            let body = s("body");
            let words = content::word_count(&body);
            content::check_body("Body", &body, &mut errors);
            let qs: Result<Vec<Question>, _> = serde_json::from_value(data.get("quiz").cloned().unwrap_or(json!([])));
            match qs {
                Ok(qs) => {
                    content::check_questions("Quiz", &qs, &mut errors);
                    if publishing && qs.is_empty() {
                        errors.push("A published chapter needs a quiz.".into());
                    }
                }
                Err(e) => errors.push(format!("Quiz is not valid: {e}")),
            }
            if publishing && body.is_empty() {
                errors.push("A published chapter needs a body.".into());
            }
            if !body.is_empty() && !(content::MIN_WORDS..=content::MAX_WORDS).contains(&words) {
                warnings.push(format!("{words} words; chapters are normally {}–{} words.", content::MIN_WORDS, content::MAX_WORDS));
            }
            if let Some(tk) = data.get("takeaways")
                && !tk.is_array()
            {
                errors.push("Key takeaways must be a list.".into());
            }
            data["words"] = json!(words);
            data["minutes"] = json!(content::minutes_for(words));
        }
        "exam" => {
            let qs: Result<Vec<Question>, _> = serde_json::from_value(data.get("questions").cloned().unwrap_or(json!([])));
            match qs {
                Ok(qs) => {
                    content::check_questions("Exam", &qs, &mut errors);
                    if qs.is_empty() {
                        errors.push("The exam needs at least one question.".into());
                    }
                }
                Err(e) => errors.push(format!("Questions are not valid: {e}")),
            }
            let pm = data.get("pass_mark").and_then(Value::as_i64).unwrap_or(0);
            if !(50..=100).contains(&pm) {
                errors.push("Pass mark must be 50–100%.".into());
            }
        }
        "term" => {
            if s("term").is_empty() || s("definition").is_empty() {
                errors.push("Term and definition are required.".into());
            }
            if !content::CATEGORIES.contains(&s("category").as_str()) {
                errors.push(format!("Category must be one of {}.", content::CATEGORIES.join(", ")));
            }
        }
        _ => {}
    }
    (errors, warnings)
}

async fn admin_put(State(st): State<AppState>, h: HeaderMap, Path((kind, slug)): Path<(String, String)>, Json(b): Json<PutBody>) -> R {
    check_kind(&kind)?;
    let (t, who) = (tenant(&h), staff(&h)?);
    let lang = lang(&LangQ { lang: b.lang.clone(), ..Default::default() });
    let cur = store::node(&st.pool, &t, &lang, &kind, &slug).await?.ok_or_else(|| ApiError::not_found("Item"))?;
    let mut data = cur.data.clone();
    if let Some(Value::Object(patch)) = b.data {
        let allowed = DATA_KEYS.iter().find(|(k, _)| *k == kind).map(|(_, v)| *v).unwrap_or(&[]);
        let mut clean = Map::new();
        for (k, v) in patch {
            if allowed.contains(&k.as_str()) {
                clean.insert(k, v);
            }
        }
        merge(&mut data, Value::Object(clean));
    }
    // the track decides the phase's shape (core fundamental + technical, or a product phase such as options)
    if kind == "section" && data.get("track") != cur.data.get("track") {
        return Err(ApiError::bad("A section's track can't be changed."));
    }
    let published = b.published.unwrap_or(cur.published);
    let mut parent = cur.parent.clone();
    if let Some(p) = b.parent.filter(|p| *p != cur.parent) {
        let want = match kind.as_str() {
            "chapter" => "section",
            "section" => "phase",
            _ => return Err(ApiError::bad("This item can't be moved.")),
        };
        store::node(&st.pool, &t, &lang, want, &p).await?.ok_or_else(|| ApiError::bad(format!("Unknown {want} `{p}`.")))?;
        parent = p;
    }
    let (errors, warnings) = validate(&kind, &mut data, published);
    if !errors.is_empty() {
        return Err(ApiError(StatusCode::UNPROCESSABLE_ENTITY, "validation", errors[0].clone(), Some(json!(errors))));
    }
    let ord = b.order.unwrap_or(cur.ord);
    store::put_override(&st.pool, &t, &lang, &kind, &slug, &parent, ord, published, &data, &who).await?;
    let action = if cur.published != published { if published { "publish" } else { "unpublish" } } else { "edit" };
    store::audit(&st.pool, &t, &who, action, &kind, &slug, &lang, json!({"title": data.get("title").or_else(|| data.get("term"))})).await;
    let n = store::node(&st.pool, &t, &lang, &kind, &slug).await?.ok_or_else(|| ApiError::not_found("Item"))?;
    let mut v = meta(&n);
    v["kind"] = json!(kind);
    v["data"] = n.data;
    Ok(Json(json!({"node": v, "warnings": warnings})))
}

async fn admin_reset(State(st): State<AppState>, h: HeaderMap, Path((kind, slug)): Path<(String, String)>, Query(q): Query<LangQ>) -> R {
    check_kind(&kind)?;
    let (t, who, lang) = (tenant(&h), staff(&h)?, lang(&q));
    if t == store::DEFAULT_TENANT {
        return Err(ApiError::bad("The platform default can't be reset."));
    }
    let n = sqlx::query("DELETE FROM content_nodes WHERE tenant = $1 AND lang = $2 AND kind = $3 AND slug = $4")
        .bind(&t)
        .bind(&lang)
        .bind(&kind)
        .bind(&slug)
        .execute(&st.pool)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found("Override"));
    }
    store::audit(&st.pool, &t, &who, "reset", &kind, &slug, &lang, json!({})).await;
    let back = store::node(&st.pool, &t, &lang, &kind, &slug).await?;
    Ok(Json(json!({"reset": true, "removed": back.is_none()})))
}

#[derive(Deserialize)]
struct CreateBody {
    lang: Option<String>,
    section: String,
    title: String,
    summary: Option<String>,
    body: Option<String>,
}

fn slugify(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').chars().take(60).collect::<String>().trim_end_matches('-').to_string()
}

async fn admin_create_chapter(State(st): State<AppState>, h: HeaderMap, Json(b): Json<CreateBody>) -> R {
    let (t, who) = (tenant(&h), staff(&h)?);
    let lang = lang(&LangQ { lang: b.lang.clone(), ..Default::default() });
    let title = b.title.trim().to_string();
    if title.is_empty() || title.chars().count() > 140 {
        return Err(ApiError::bad("Title is required (up to 140 characters)."));
    }
    let section = store::node(&st.pool, &t, &lang, "section", &b.section).await?.ok_or_else(|| ApiError::bad("Unknown section."))?;
    // "p1-technical" -> "p1", "p9-options" -> "p9"
    let prefix = content::TRACKS.iter().find_map(|t| section.slug.strip_suffix(&format!("-{t}"))).unwrap_or(&section.slug).to_string();
    let base = format!("{prefix}-{}", slugify(&title));
    let base = if content::is_slug(&base) { base } else { format!("{prefix}-chapter") };
    let mut slug = base.clone();
    let mut i = 2;
    while sqlx::query_scalar::<_, i32>("SELECT 1 FROM content_nodes WHERE kind = 'chapter' AND slug = $1 LIMIT 1").bind(&slug).fetch_optional(&st.pool).await?.is_some() {
        slug = format!("{base}-{i}");
        i += 1;
    }
    let siblings = store::nodes(&st.pool, &t, &lang, &["chapter"], false).await?;
    let ord = siblings.iter().filter(|c| c.parent == section.slug).map(|c| c.ord).max().unwrap_or(0) + 1;
    let mut data = json!({"title": title, "summary": b.summary.unwrap_or_default(), "body": b.body.unwrap_or_default(), "takeaways": [], "practice": null, "quiz": []});
    let (errors, _) = validate("chapter", &mut data, false);
    if !errors.is_empty() {
        return Err(ApiError(StatusCode::UNPROCESSABLE_ENTITY, "validation", errors[0].clone(), Some(json!(errors))));
    }
    store::put_override(&st.pool, &t, &lang, "chapter", &slug, &section.slug, ord, false, &data, &who).await?;
    store::audit(&st.pool, &t, &who, "create", "chapter", &slug, &lang, json!({"title": data["title"], "section": section.slug})).await;
    Ok(Json(json!({"slug": slug, "order": ord})))
}

#[derive(Deserialize)]
struct ReorderBody {
    lang: Option<String>,
    kind: String,
    parent: String,
    slugs: Vec<String>,
}

async fn admin_reorder(State(st): State<AppState>, h: HeaderMap, Json(b): Json<ReorderBody>) -> R {
    let (t, who) = (tenant(&h), staff(&h)?);
    let lang = lang(&LangQ { lang: b.lang.clone(), ..Default::default() });
    if !["chapter", "section", "phase"].contains(&b.kind.as_str()) || b.slugs.is_empty() || b.slugs.len() > 100 {
        return Err(ApiError::bad("Invalid reorder request."));
    }
    let all = store::nodes(&st.pool, &t, &lang, &[b.kind.as_str()], true).await?;
    let siblings: HashMap<&str, &Node> = all.iter().filter(|n| n.parent == b.parent).map(|n| (n.slug.as_str(), n)).collect();
    if b.slugs.iter().any(|s| !siblings.contains_key(s.as_str())) {
        return Err(ApiError::bad("Every item must belong to the same parent."));
    }
    for (i, s) in b.slugs.iter().enumerate() {
        let n = siblings[s.as_str()];
        let ord = i as i32 + 1;
        if n.ord != ord {
            store::put_override(&st.pool, &t, &lang, &n.kind, &n.slug, &n.parent, ord, n.published, &n.data, &who).await?;
        }
    }
    store::audit(&st.pool, &t, &who, "reorder", &b.kind, &b.parent, &lang, json!({"slugs": b.slugs})).await;
    Ok(Json(json!({"ok": true})))
}

async fn admin_stats(State(st): State<AppState>, h: HeaderMap, Query(q): Query<LangQ>) -> R {
    let (t, lang) = (tenant(&h), lang(&q));
    let pool = &st.pool;
    let learners: i64 = sqlx::query_scalar("SELECT count(DISTINCT user_id) FROM chapter_progress WHERE tenant = $1").bind(&t).fetch_one(pool).await?;
    let active7: i64 = sqlx::query_scalar("SELECT count(DISTINCT user_id) FROM learning_days WHERE tenant = $1 AND day > (now() AT TIME ZONE 'utc')::date - 7").bind(&t).fetch_one(pool).await?;
    let completions: i64 = sqlx::query_scalar("SELECT count(*) FROM chapter_progress WHERE tenant = $1 AND completed_at IS NOT NULL").bind(&t).fetch_one(pool).await?;
    let completions30: i64 = sqlx::query_scalar("SELECT count(*) FROM chapter_progress WHERE tenant = $1 AND completed_at > now() - interval '30 days'").bind(&t).fetch_one(pool).await?;
    let quiz_avg: Option<f64> = sqlx::query_scalar("SELECT avg(quiz_best * 100.0 / quiz_total)::float8 FROM chapter_progress WHERE tenant = $1 AND quiz_total > 0").bind(&t).fetch_one(pool).await?;
    let ex = sqlx::query("SELECT count(*), count(*) FILTER (WHERE passed), avg(pct)::float8 FROM exam_attempts WHERE tenant = $1").bind(&t).fetch_one(pool).await?;
    let certs: i64 = sqlx::query_scalar("SELECT count(*) FROM certificates WHERE tenant = $1 AND NOT revoked").bind(&t).fetch_one(pool).await?;
    let tree = store::course(pool, &t, &lang, true).await?;
    let cstats = chapter_stats(pool, &t).await?;
    let per_phase_learners: HashMap<String, i64> = sqlx::query("SELECT phase, count(DISTINCT user_id) FROM chapter_progress WHERE tenant = $1 GROUP BY phase")
        .bind(&t)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|r| (r.get(0), r.get(1)))
        .collect();
    let per_phase_certs: HashMap<String, i64> = sqlx::query("SELECT phase, count(*) FROM certificates WHERE tenant = $1 AND NOT revoked GROUP BY phase")
        .bind(&t)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|r| (r.get(0), r.get(1)))
        .collect();
    let per_phase_exam: HashMap<String, (i64, i64)> = sqlx::query("SELECT phase, count(*), count(*) FILTER (WHERE passed) FROM exam_attempts WHERE tenant = $1 GROUP BY phase")
        .bind(&t)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|r| (r.get(0), (r.get(1), r.get(2))))
        .collect();
    let phases: Vec<Value> = tree
        .iter()
        .map(|p| {
            let chapters: Vec<&Node> = p.chapters().collect();
            let comp: i64 = chapters.iter().map(|c| cstats.get(&c.slug).map(|x| x.1).unwrap_or(0)).sum();
            let (attempts, passed) = per_phase_exam.get(&p.node.slug).copied().unwrap_or((0, 0));
            json!({"slug": p.node.slug, "order": p.node.ord, "title": p.node.s("title"), "level": p.node.s("level"), "chapters": chapters.len(),
                   "learners": per_phase_learners.get(&p.node.slug).copied().unwrap_or(0), "completions": comp,
                   "exam_attempts": attempts, "exam_passed": passed, "certificates": per_phase_certs.get(&p.node.slug).copied().unwrap_or(0)})
        })
        .collect();
    let titles: HashMap<String, (String, String)> = tree.iter().flat_map(|p| p.chapters().map(move |c| (c.slug.clone(), (c.s("title"), p.node.s("title"))))).collect();
    let mut top: Vec<Value> = cstats
        .iter()
        .filter_map(|(slug, (l, c, qa))| titles.get(slug).map(|(title, phase)| json!({"slug": slug, "title": title, "phase": phase, "learners": l, "completed": c, "quiz_avg": qa.map(|x| x.round())})))
        .collect();
    top.sort_by(|a, b| b["learners"].as_i64().cmp(&a["learners"].as_i64()).then_with(|| a["slug"].as_str().cmp(&b["slug"].as_str())));
    top.truncate(8);
    let recent: Vec<Value> = sqlx::query("SELECT code, learner_name, phase_order, phase_title, score_pct, issued_at FROM certificates WHERE tenant = $1 ORDER BY issued_at DESC LIMIT 8")
        .bind(&t)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|r| json!({"code": r.get::<String, _>(0), "learner_name": r.get::<String, _>(1), "phase_order": r.get::<i32, _>(2), "phase_title": r.get::<String, _>(3), "score_pct": r.get::<i32, _>(4), "issued_at": r.get::<chrono::DateTime<chrono::Utc>, _>(5)}))
        .collect();
    Ok(Json(json!({
        "learners": learners, "active_7d": active7, "completions": completions, "completions_30d": completions30,
        "quiz_avg": quiz_avg.map(|x| x.round()), "exam_attempts": ex.get::<i64, _>(0), "exam_passed": ex.get::<i64, _>(1),
        "exam_avg": ex.get::<Option<f64>, _>(2).map(|x| x.round()), "certificates": certs,
        "phases": phases, "top_chapters": top, "recent_certificates": recent,
    })))
}

async fn admin_audit(State(st): State<AppState>, h: HeaderMap) -> R {
    let t = tenant(&h);
    let rows = sqlx::query("SELECT staff, action, kind, slug, lang, detail, at FROM content_audit WHERE tenant = $1 ORDER BY at DESC LIMIT 50").bind(&t).fetch_all(&st.pool).await?;
    let list: Vec<Value> = rows
        .into_iter()
        .map(|r| json!({"staff": r.get::<String, _>(0), "action": r.get::<String, _>(1), "kind": r.get::<String, _>(2), "slug": r.get::<String, _>(3), "lang": r.get::<String, _>(4), "detail": r.get::<Value, _>(5), "at": r.get::<chrono::DateTime<chrono::Utc>, _>(6)}))
        .collect();
    Ok(Json(json!({ "audit": list })))
}

async fn admin_glossary(State(st): State<AppState>, h: HeaderMap, Query(q): Query<LangQ>) -> R {
    let (t, lang) = (tenant(&h), lang(&q));
    let mut ns = store::nodes(&st.pool, &t, &lang, &["term"], true).await?;
    ns.sort_by_key(|n| n.s("term").to_lowercase());
    let terms: Vec<Value> = ns
        .iter()
        .map(|n| {
            let mut v = term_json(n);
            merge(&mut v, meta(n));
            v
        })
        .collect();
    Ok(Json(json!({ "terms": terms })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(answer: usize) -> Question {
        Question { question: "Q".into(), options: vec!["a".into(), "b".into(), "c".into(), "d".into()], answer, explanation: "E".into(), chapter: None }
    }

    #[test]
    fn grading() {
        let qs = vec![q(0), q(1), q(2), q(3)];
        let (r, s, all) = grade(&qs, &[Some(0), Some(1), None]);
        assert_eq!((r.len(), s, all), (2, 2, false));
        let (r, s, all) = grade(&qs, &[Some(0), Some(0), Some(2), Some(3)]);
        assert_eq!((r.len(), s, all), (4, 3, true));
        assert!(s * 100 >= QUIZ_PASS_PCT * 4);
        let (_, _, all) = grade(&qs, &[Some(9), Some(0), Some(0), Some(0)]);
        assert!(!all, "out-of-range answers count as unanswered");
    }

    #[test]
    fn slugify_titles() {
        assert_eq!(slugify("Gold & real yields: 2026!"), "gold-real-yields-2026");
        assert_eq!(slugify("  ---  "), "");
    }

    #[test]
    fn percent_decoding() {
        let mut h = HeaderMap::new();
        h.insert("x-kalks-user-name", HeaderValue::from_static("Jos%C3%A9%20P%C3%A9rez"));
        assert_eq!(header_text(&h, "x-kalks-user-name", 80), "José Pérez");
        h.insert("x-kalks-user-name", HeaderValue::from_static("100%"));
        assert_eq!(header_text(&h, "x-kalks-user-name", 80), "100%");
    }

    #[test]
    fn chapter_validation() {
        let mut d = json!({"title": "T", "body": "## A\n\nsome words", "quiz": [], "takeaways": []});
        let (e, w) = validate("chapter", &mut d, true);
        assert!(e.iter().any(|x| x.contains("quiz")), "{e:?}");
        assert_eq!(w.len(), 1);
        assert_eq!(d["words"], json!(3));
        let mut d = json!({"title": "T", "body": "<script>x</script>", "quiz": [{"question": "q", "options": ["a", "b", "c"], "answer": 5, "explanation": "e"}]});
        let (e, _) = validate("chapter", &mut d, false);
        assert_eq!(e.len(), 2, "{e:?}");
    }

    #[test]
    fn section_tracks() {
        for t in ["fundamental", "technical", "options"] {
            let (e, _) = validate("section", &mut json!({"title": "T", "track": t}), true);
            assert!(e.is_empty(), "{t}: {e:?}");
        }
        let (e, _) = validate("section", &mut json!({"title": "T", "track": "macro"}), true);
        assert_eq!(e, vec!["Track must be one of fundamental, technical, options.".to_string()]);
        let (e, _) = validate("section", &mut json!({"title": "T"}), true);
        assert_eq!(e.len(), 1, "{e:?}");
    }
}
