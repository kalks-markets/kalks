//! Challenges routes (phase 4): the client's Rewards hub (missions, XP, level, streaks, nickname, leaderboards,
//! Kalks Trader's rank widget), the public leaderboard for the website, and the Back Office's templates, missions,
//! levels and payout approvals.

use super::admin::{clean_list, merge, parse, slugify, text, write_contest};
use super::{ROLES_APPROVE, ROLES_READ, ROLES_WRITE, StaffCtx, UserCtx, paging};
use crate::audit;
use crate::calc;
use crate::challenge_calc::{self as cc, Level};
use crate::contests::{self, CONTEST_SELECT};
use crate::db;
use crate::error::{ApiError, ApiResult, invalid};
use crate::leaderboard::{self, Board};
use crate::missions;
use crate::model::{ASSET_CLASSES, AntiCheat};
use crate::money::{D, ZERO, de_opt_dec, num};
use crate::profiles;
use crate::state::AppState;
use crate::templates::{self, FromTemplate, Split};
use crate::xp;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

// ---------------------------------------------------------------- client

/// Login streak: one touch per client per server day (kept in memory so the rewards routes stay cheap).
static LOGIN_DAYS: LazyLock<Mutex<HashMap<(String, i64), chrono::NaiveDate>>> = LazyLock::new(Default::default);

/// Refreshes the profile, records today on the login streak and returns nothing. Every hub route calls it.
pub async fn seen(st: &AppState, u: &UserCtx) -> ApiResult<()> {
    profiles::touch(st, &u.tenant, u.user_id, &u.hints).await?;
    touch_login(st, &u.tenant, u.user_id).await?;
    Ok(())
}

/// Records today on the client's login streak (once per server day per client and process).
pub async fn touch_login(st: &AppState, tenant: &str, user_id: i64) -> anyhow::Result<()> {
    let today = cc::server_day(Utc::now());
    let key = (tenant.to_string(), user_id);
    let fresh = LOGIN_DAYS.lock().unwrap().get(&key) != Some(&today);
    if fresh {
        xp::touch(st, tenant, user_id, "login", today).await?;
        let mut m = LOGIN_DAYS.lock().unwrap();
        if m.len() > 100_000 {
            m.clear();
        }
        m.insert(key, today);
    }
    Ok(())
}

async fn streaks_json(st: &AppState, tenant: &str, user_id: i64) -> anyhow::Result<Value> {
    let s = db::settings(&st.pool, tenant).await?;
    let today = cc::server_day(Utc::now());
    let trading = xp::streak(&st.pool, tenant, user_id, "trading").await?;
    let login = xp::streak(&st.pool, tenant, user_id, "login").await?;
    Ok(json!({
        "trading": xp::streak_json(&trading, today, s.streak_grace_days, true),
        "login": xp::streak_json(&login, today, s.streak_grace_days, false),
        "weekXp": s.xp_streak_week,
    }))
}

async fn level_of(st: &AppState, tenant: &str, user_id: i64) -> anyhow::Result<Value> {
    let levels = xp::levels(&st.pool, tenant).await?;
    let total = xp::total(&st.pool, tenant, user_id).await?;
    Ok(xp::level_json(&levels, total))
}

/// Recent rewards: XP bookings, contest prizes and mission rewards, newest first.
async fn recent(st: &AppState, tenant: &str, user_id: i64) -> anyhow::Result<Vec<Value>> {
    let mut out: Vec<(DateTime<Utc>, Value)> = vec![];
    for r in sqlx::query("SELECT * FROM xp_ledger WHERE tenant = $1 AND user_id = $2 ORDER BY id DESC LIMIT 15").bind(tenant).bind(user_id).fetch_all(&st.pool).await? {
        let at: DateTime<Utc> = r.get("created_at");
        out.push((at, json!({"kind": "xp", "title": r.get::<String, _>("description"), "xp": r.get::<i32, _>("xp"), "amount": null, "status": "paid", "at": at})));
    }
    for r in sqlx::query(
        "SELECT e.prize_amount, e.prize_payout, e.prize_status, e.rank, c.name, COALESCE(c.finalized_at, c.ends_at) AS at FROM contest_entries e JOIN contests c ON c.id = e.contest_id
          WHERE e.tenant = $1 AND e.user_id = $2 AND e.prize_amount > 0 ORDER BY at DESC LIMIT 10",
    )
    .bind(tenant)
    .bind(user_id)
    .fetch_all(&st.pool)
    .await?
    {
        let at: DateTime<Utc> = r.get("at");
        out.push((
            at,
            json!({"kind": "prize", "title": format!("{} · rank #{}", r.get::<String, _>("name"), r.get::<Option<i32>, _>("rank").unwrap_or(0)), "xp": null,
                   "amount": num(r.get("prize_amount")), "payout": r.get::<Option<String>, _>("prize_payout"), "status": contests::client_prize_status(r.get::<&str, _>("prize_status")), "at": at}),
        ));
    }
    for r in sqlx::query(
        "SELECT p.reward_amount, p.reward_payout, p.reward_status, p.completed_at, m.name FROM mission_progress p JOIN missions m ON m.id = p.mission_id
          WHERE p.tenant = $1 AND p.user_id = $2 AND p.reward_amount > 0 AND p.completed_at IS NOT NULL ORDER BY p.completed_at DESC LIMIT 10",
    )
    .bind(tenant)
    .bind(user_id)
    .fetch_all(&st.pool)
    .await?
    {
        let at: DateTime<Utc> = r.get("completed_at");
        out.push((at, json!({"kind": "mission", "title": r.get::<String, _>("name"), "xp": null, "amount": num(r.get("reward_amount")), "payout": r.get::<Option<String>, _>("reward_payout"), "status": missions::client_reward_status(r.get::<&str, _>("reward_status")), "at": at})));
    }
    out.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(out.into_iter().take(15).map(|x| x.1).collect())
}

/// `GET /v1/growth/me/hub`: everything the Rewards hub shows in one call.
pub async fn hub(State(st): State<AppState>, u: UserCtx) -> ApiResult<Json<Value>> {
    seen(&st, &u).await?;
    missions::evaluate_soon(&st, &u.tenant, u.user_id);
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!("{CONTEST_SELECT} WHERE c.tenant = $1 AND c.status IN ('scheduled','running','ended') AND c.ends_at > now() - interval '1 day' ORDER BY c.starts_at LIMIT 20")))
        .bind(&u.tenant)
        .fetch_all(&st.pool)
        .await?;
    let mine = sqlx::query("SELECT e.*, c.min_trades FROM contest_entries e JOIN contests c ON c.id = e.contest_id WHERE e.tenant = $1 AND e.user_id = $2").bind(&u.tenant).bind(u.user_id).fetch_all(&st.pool).await?;
    let contests: Vec<Value> = rows
        .iter()
        .filter(|r| matches!(contests::status_of(r).as_str(), "scheduled" | "running"))
        .map(|r| {
            let mut v = contests::contest_json(r);
            let id: i64 = r.get("id");
            v["myEntry"] = mine.iter().find(|e| e.get::<i64, _>("contest_id") == id).map(|e| contests::standing_json(e, Some(u.user_id), false, e.get("min_trades"))).unwrap_or(Value::Null);
            v
        })
        .collect();
    Ok(Json(json!({
        "level": level_of(&st, &u.tenant, u.user_id).await?,
        "streaks": streaks_json(&st, &u.tenant, u.user_id).await?,
        "missions": missions::me(&st, &u.tenant, u.user_id).await?,
        "contests": contests,
        "recent": recent(&st, &u.tenant, u.user_id).await?,
        "nickname": leaderboard::nickname_json(&st, &u.tenant, u.user_id).await?,
    })))
}

pub async fn my_missions(State(st): State<AppState>, u: UserCtx) -> ApiResult<Json<Value>> {
    seen(&st, &u).await?;
    missions::evaluate_user(&st, &u.tenant, u.user_id).await?;
    Ok(Json(json!({"items": missions::me(&st, &u.tenant, u.user_id).await?})))
}

pub async fn my_xp(State(st): State<AppState>, u: UserCtx) -> ApiResult<Json<Value>> {
    seen(&st, &u).await?;
    let rows = sqlx::query("SELECT * FROM xp_ledger WHERE tenant = $1 AND user_id = $2 ORDER BY id DESC LIMIT 50").bind(&u.tenant).bind(u.user_id).fetch_all(&st.pool).await?;
    Ok(Json(json!({
        "level": level_of(&st, &u.tenant, u.user_id).await?,
        "streaks": streaks_json(&st, &u.tenant, u.user_id).await?,
        "history": rows.iter().map(xp::tx_json).collect::<Vec<_>>(),
    })))
}

pub async fn my_nickname(State(st): State<AppState>, u: UserCtx) -> ApiResult<Json<Value>> {
    Ok(Json(leaderboard::nickname_json(&st, &u.tenant, u.user_id).await?))
}

#[derive(Deserialize)]
pub struct NickBody {
    #[serde(default)]
    nickname: Option<String>,
    #[serde(default)]
    public: Option<bool>,
}

pub async fn set_nickname(State(st): State<AppState>, u: UserCtx, Json(b): Json<NickBody>) -> ApiResult<Json<Value>> {
    if b.nickname.is_none() && b.public.is_none() {
        return Err(invalid("nickname", "Send a nickname and / or public."));
    }
    Ok(Json(leaderboard::set(&st, &u.tenant, u.user_id, b.nickname.as_deref(), b.public).await?))
}

#[derive(Deserialize)]
pub struct BoardQ {
    contest: Option<String>,
    period: Option<String>,
    limit: Option<i64>,
    tenant: Option<String>,
}

pub async fn my_leaderboard(State(st): State<AppState>, u: UserCtx, Query(q): Query<BoardQ>) -> ApiResult<Json<Value>> {
    let b = Board::parse(q.contest.as_deref(), q.period.as_deref())?;
    leaderboard::ensure(&st.pool, &u.tenant, u.user_id).await?;
    Ok(Json(leaderboard::board(&st, &u.tenant, &b, Some(u.user_id), false, q.limit.unwrap_or(100).clamp(1, 200)).await?))
}

#[derive(Deserialize)]
pub struct RankQ {
    login: Option<i64>,
}

/// Kalks Trader's rank widget (the terminal BFF passes the acting account's login).
pub async fn my_rank(State(st): State<AppState>, u: UserCtx, Query(q): Query<RankQ>) -> ApiResult<Json<Value>> {
    Ok(Json(leaderboard::rank_widget(&st, &u.tenant, u.user_id, q.login).await?))
}

// ---------------------------------------------------------------- public (website, no internal token)

fn public_tenant(headers: &HeaderMap, q: &Option<String>) -> ApiResult<String> {
    let t = q
        .clone()
        .or_else(|| headers.get("x-kalks-tenant").and_then(|v| v.to_str().ok()).map(str::to_string))
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "kalks".into());
    if t.len() > 40 || !t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(ApiError::BadRequest("Invalid tenant.".into()));
    }
    Ok(t)
}

fn cached(v: Value) -> Response {
    let mut r = Json(v).into_response();
    let h = r.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=15, s-maxage=15"));
    h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    h.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    r
}

async fn public_on(st: &AppState, tenant: &str) -> ApiResult<()> {
    if !db::settings(&st.pool, tenant).await?.public_leaderboards || !crate::modules::on(st, tenant, crate::modules::KEY).await {
        return Err(ApiError::NotFound);
    }
    Ok(())
}

/// `GET /v1/public/leaderboard?contest=<id|slug>` or `?period=week|month|all` (XP): rank, nickname, score and level
/// badge only. Clients who opted out of public boards are left out. Cached 15 s.
pub async fn public_leaderboard(State(st): State<AppState>, headers: HeaderMap, Query(q): Query<BoardQ>) -> ApiResult<Response> {
    let tenant = public_tenant(&headers, &q.tenant)?;
    public_on(&st, &tenant).await?;
    let b = Board::parse(q.contest.as_deref(), q.period.as_deref())?;
    Ok(cached(leaderboard::public_board(&st, &tenant, &b, q.limit.unwrap_or(50).clamp(1, 100)).await?))
}

/// `GET /v1/public/contests`: running, upcoming and recent contests (no entrant data). Cached by the edge.
pub async fn public_contests(State(st): State<AppState>, headers: HeaderMap, Query(q): Query<BoardQ>) -> ApiResult<Response> {
    let tenant = public_tenant(&headers, &q.tenant)?;
    public_on(&st, &tenant).await?;
    Ok(cached(leaderboard::public_contests(&st, &tenant).await?))
}

// ---------------------------------------------------------------- admin: contest templates

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateIn {
    #[serde(default)]
    key: Option<String>,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default = "live")]
    kind: String,
    #[serde(default = "cfd")]
    instrument: String,
    scoring: String,
    #[serde(default = "monday")]
    schedule: String,
    #[serde(default = "seven")]
    duration_days: i32,
    #[serde(default)]
    min_trades: i32,
    #[serde(default)]
    max_entrants: Option<i32>,
    #[serde(default)]
    account_groups: Vec<String>,
    #[serde(default)]
    asset_classes: Vec<String>,
    #[serde(default = "yes")]
    kyc_required: bool,
    #[serde(default, deserialize_with = "de_opt_dec")]
    min_equity: Option<D>,
    #[serde(default, deserialize_with = "de_opt_dec")]
    prize_pool: Option<D>,
    #[serde(default = "top3")]
    split: Split,
    #[serde(default = "wallet")]
    payout: String,
    #[serde(default)]
    rules: String,
    #[serde(default)]
    anti_cheat: AntiCheat,
    #[serde(default = "yes")]
    active: bool,
    #[serde(default = "hundred")]
    sort: i32,
}

fn live() -> String {
    "live".into()
}
fn cfd() -> String {
    "cfd".into()
}
fn monday() -> String {
    "next_monday".into()
}
fn seven() -> i32 {
    7
}
fn yes() -> bool {
    true
}
fn top3() -> Split {
    Split::Preset("top3".into())
}
fn wallet() -> String {
    "wallet".into()
}
fn hundred() -> i32 {
    100
}

async fn write_template(st: &AppState, s: &StaffCtx, id: Option<i64>, mut t: TemplateIn, before: Option<Value>) -> ApiResult<Json<Value>> {
    t.name = text(&t.name, "name", 80, true)?;
    t.description = text(&t.description, "description", 1000, false)?;
    t.rules = text(&t.rules, "rules", 5000, false)?;
    if !matches!(t.kind.as_str(), "live" | "demo") {
        return Err(invalid("kind", "live or demo."));
    }
    if !calc::CONTEST_INSTRUMENTS.contains(&t.instrument.as_str()) {
        return Err(invalid("instrument", "cfd or options."));
    }
    if !calc::scoring_allowed(&t.instrument, &t.scoring) {
        return Err(invalid("scoring", "A scoring this instrument supports."));
    }
    if !matches!(t.schedule.as_str(), "next_monday" | "next_month" | "next_weekend" | "tomorrow") {
        return Err(invalid("schedule", "next_monday, next_month, next_weekend or tomorrow."));
    }
    if !(1..=366).contains(&t.duration_days) {
        return Err(invalid("durationDays", "1–366 days."));
    }
    if !(0..=10_000).contains(&t.min_trades) || t.max_entrants.is_some_and(|m| m < 1) {
        return Err(invalid("minTrades", "Min trades 0–10000, max entrants positive."));
    }
    t.account_groups = clean_list(&t.account_groups, false);
    t.asset_classes = clean_list(&t.asset_classes, false);
    if let Some(a) = t.asset_classes.iter().find(|a| !ASSET_CLASSES.contains(&a.as_str())) {
        return Err(invalid("assetClasses", format!("Unknown asset class '{a}'.")));
    }
    let pool = t.prize_pool.unwrap_or(ZERO);
    if pool < ZERO || pool > D::from(1_000_000) {
        return Err(invalid("prizePool", "0 – 1 000 000 USD."));
    }
    t.split.bands().map_err(|m| invalid("split", m))?;
    if t.payout != "wallet" && t.payout != "credit" {
        return Err(invalid("payout", "wallet or credit."));
    }
    if t.min_equity.is_some_and(|m| m < ZERO) {
        return Err(invalid("minEquity", "Cannot be negative."));
    }
    let key = t.key.as_deref().map(slugify).filter(|k| !k.is_empty()).unwrap_or_else(|| format!("{}-{}", slugify(&t.name), db::random_code(4).to_lowercase()));
    let q = match id {
        None => sqlx::query(
            "INSERT INTO contest_templates (tenant, key, name, description, kind, instrument, scoring, schedule, duration_days, min_trades, max_entrants, account_groups, asset_classes,
               kyc_required, min_equity, prize_pool, split, payout, rules, anti_cheat, active, sort, created_by)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23) RETURNING *",
        ),
        Some(_) => sqlx::query(
            "UPDATE contest_templates SET key = $2, name = $3, description = $4, kind = $5, instrument = $6, scoring = $7, schedule = $8, duration_days = $9, min_trades = $10,
               max_entrants = $11, account_groups = $12, asset_classes = $13, kyc_required = $14, min_equity = $15, prize_pool = $16, split = $17, payout = $18, rules = $19,
               anti_cheat = $20, active = $21, sort = $22, updated_at = now()
             WHERE tenant = $1 AND id = $24 AND $23::text IS NOT NULL RETURNING *",
        ),
    };
    let row = q
        .bind(&s.tenant)
        .bind(&key)
        .bind(&t.name)
        .bind(&t.description)
        .bind(&t.kind)
        .bind(&t.instrument)
        .bind(&t.scoring)
        .bind(&t.schedule)
        .bind(t.duration_days)
        .bind(t.min_trades)
        .bind(t.max_entrants)
        .bind(&t.account_groups)
        .bind(&t.asset_classes)
        .bind(t.kyc_required)
        .bind(t.min_equity)
        .bind(pool)
        .bind(sqlx::types::Json(&t.split))
        .bind(&t.payout)
        .bind(&t.rules)
        .bind(sqlx::types::Json(&t.anti_cheat))
        .bind(t.active)
        .bind(t.sort)
        .bind(s.actor().label())
        .bind(id.unwrap_or(0))
        .fetch_optional(&st.pool)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(d) if d.code().as_deref() == Some("23505") => invalid("key", "A template with that key exists."),
            _ => ApiError::Internal(e.into()),
        })?
        .ok_or(ApiError::NotFound)?;
    let v = templates::template_json(&row);
    audit::record(&st.pool, &s.tenant, &s.actor(), if id.is_some() { "template.update" } else { "template.create" }, Some(format!("template:{}", row.get::<i64, _>("id"))), before, Some(v.clone()), None).await?;
    Ok(Json(json!({"template": v})))
}

pub async fn list_templates(State(st): State<AppState>, s: StaffCtx) -> ApiResult<Json<Value>> {
    s.require(ROLES_READ)?;
    templates::seed(&st.pool, &s.tenant).await?;
    let rows = sqlx::query("SELECT * FROM contest_templates WHERE tenant = $1 ORDER BY active DESC, sort, id").bind(&s.tenant).fetch_all(&st.pool).await?;
    let settings = db::settings(&st.pool, &s.tenant).await?;
    Ok(Json(json!({"items": rows.iter().map(templates::template_json).collect::<Vec<_>>(), "presets": cc::presets_json(), "autoPayBelow": num(settings.auto_pay_below), "assetClasses": ASSET_CLASSES})))
}

pub async fn create_template(State(st): State<AppState>, s: StaffCtx, Json(body): Json<Value>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    write_template(&st, &s, None, parse(body)?, None).await
}

async fn load_template(st: &AppState, s: &StaffCtx, id: i64) -> ApiResult<sqlx::postgres::PgRow> {
    sqlx::query("SELECT * FROM contest_templates WHERE id = $1 AND tenant = $2").bind(id).bind(&s.tenant).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)
}

pub async fn patch_template(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>, Json(body): Json<Value>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    let before = templates::template_json(&load_template(&st, &s, id).await?);
    write_template(&st, &s, Some(id), parse(merge(before.clone(), &body))?, Some(before)).await
}

/// `POST /admin/contest-templates/{id}/contest`: the wizard's publish step (dates, pool and split chosen).
pub async fn contest_from_template(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>, Json(body): Json<Value>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    let t = templates::template_json(&load_template(&st, &s, id).await?);
    let o: FromTemplate = parse(body)?;
    let input = templates::contest_input(&t, &o, Utc::now()).map_err(|(f, m)| invalid(f, m))?;
    write_contest(&st, &s, None, parse(input)?, None).await
}

/// `POST /admin/contests/{id}/duplicate`: a draft copy for the next window of the same length.
pub async fn duplicate_contest(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    let r = sqlx::query(sqlx::AssertSqlSafe(format!("{CONTEST_SELECT} WHERE c.id = $1 AND c.tenant = $2"))).bind(id).bind(&s.tenant).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    let c = contests::contest_json(&r);
    let (start, end) = cc::clone_window(r.get("starts_at"), r.get("ends_at"), Utc::now());
    let mut body = c.clone();
    for k in ["id", "slug", "status", "entrants", "prizePool", "refreshedAt", "finalizedAt", "paidAt", "createdAt", "updatedAt"] {
        if let Some(o) = body.as_object_mut() {
            o.remove(k);
        }
    }
    body["startsAt"] = json!(start.to_rfc3339());
    body["endsAt"] = json!(end.to_rfc3339());
    body["status"] = json!("draft");
    body["clonedFrom"] = json!(id);
    write_contest(&st, &s, None, parse(body)?, None).await
}

#[derive(Deserialize, Default)]
pub struct SaveTemplate {
    #[serde(default)]
    name: Option<String>,
}

/// `POST /admin/contests/{id}/save-template`: keeps a contest's setup as a reusable template.
pub async fn save_as_template(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>, body: Option<Json<SaveTemplate>>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    let r = sqlx::query(sqlx::AssertSqlSafe(format!("{CONTEST_SELECT} WHERE c.id = $1 AND c.tenant = $2"))).bind(id).bind(&s.tenant).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    let c = contests::contest_json(&r);
    let pool = crate::money::value_dec(&c["prizePool"]).unwrap_or(ZERO);
    let prizes = contests::prizes_of(&r);
    // the contest's prize table as bands (shares of its pool)
    let split = if pool > ZERO && !prizes.is_empty() {
        let mut bands: Vec<cc::Band> = prizes
            .iter()
            .map(|p| cc::Band { rank_from: p.rank_from, rank_to: p.rank_to, pct: (p.amount * D::from(p.rank_to - p.rank_from + 1) * D::from(100) / pool).round_dp(4) })
            .collect();
        bands.sort_by_key(|b| b.rank_from);
        let total: D = bands.iter().map(|b| b.pct).sum();
        if let Some(f) = bands.first_mut() {
            f.pct += D::from(100) - total;
        }
        Split::Bands(bands)
    } else {
        Split::Preset("top3".into())
    };
    let days = ((r.get::<DateTime<Utc>, _>("ends_at") - r.get::<DateTime<Utc>, _>("starts_at")).num_hours() as f64 / 24.0).ceil().max(1.0) as i32;
    let t = json!({
        "name": body.and_then(|b| b.0.name).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| c["name"].as_str().unwrap_or("").to_string()),
        "description": c["description"], "kind": c["kind"], "instrument": c["instrument"], "scoring": c["scoring"], "schedule": "tomorrow", "durationDays": days,
        "minTrades": c["minTrades"], "maxEntrants": c["maxEntrants"], "accountGroups": c["accountGroups"], "assetClasses": c["assetClasses"], "kycRequired": c["kycRequired"],
        "minEquity": c["minEquity"], "prizePool": c["prizePool"], "split": split, "payout": prizes.first().map(|p| p.payout.clone()).unwrap_or_else(|| "wallet".into()),
        "rules": c["rules"], "antiCheat": c["antiCheat"],
    });
    write_template(&st, &s, None, parse(t)?, None).await
}

// ---------------------------------------------------------------- admin: missions

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MissionIn {
    name: String,
    #[serde(default)]
    description: String,
    kind: String,
    #[serde(default = "once")]
    schedule: String,
    #[serde(deserialize_with = "crate::money::de_dec")]
    target: D,
    #[serde(default)]
    params: Value,
    #[serde(default)]
    xp: i32,
    #[serde(default, deserialize_with = "de_opt_dec")]
    reward_amount: Option<D>,
    #[serde(default)]
    reward_payout: Option<String>,
    #[serde(default = "yes")]
    kyc_required: bool,
    #[serde(default = "yes")]
    active: bool,
    #[serde(default)]
    starts_at: Option<String>,
    #[serde(default)]
    ends_at: Option<String>,
    #[serde(default = "hundred")]
    sort: i32,
}

fn once() -> String {
    "once".into()
}

fn clean_params(v: &Value) -> ApiResult<Value> {
    let mut out = serde_json::Map::new();
    if let Some(l) = v.get("minLots").filter(|x| !x.is_null()) {
        let x = l.as_f64().filter(|x| (0.0..=100.0).contains(x)).ok_or_else(|| invalid("params", "minLots must be 0–100."))?;
        out.insert("minLots".into(), json!(x));
    }
    if let Some(h) = v.get("minHoldSeconds").filter(|x| !x.is_null()) {
        let x = h.as_i64().filter(|x| (0..=86_400).contains(x)).ok_or_else(|| invalid("params", "minHoldSeconds must be 0–86400."))?;
        out.insert("minHoldSeconds".into(), json!(x));
    }
    if let Some(a) = v.get("assetClasses").and_then(Value::as_array) {
        let list: Vec<String> = clean_list(&a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect::<Vec<_>>(), false);
        if let Some(bad) = list.iter().find(|x| !ASSET_CLASSES.contains(&x.as_str())) {
            return Err(invalid("params", format!("Unknown asset class '{bad}'.")));
        }
        if !list.is_empty() {
            out.insert("assetClasses".into(), json!(list));
        }
    }
    Ok(Value::Object(out))
}

async fn write_mission(st: &AppState, s: &StaffCtx, id: Option<i64>, mut m: MissionIn, before: Option<Value>) -> ApiResult<Json<Value>> {
    m.name = text(&m.name, "name", 80, true)?;
    m.description = text(&m.description, "description", 500, false)?;
    missions::check(&m.kind, &m.schedule, m.target, m.reward_amount, m.reward_payout.as_deref())?;
    if !(0..=100_000).contains(&m.xp) {
        return Err(invalid("xp", "0–100000 XP."));
    }
    if m.xp == 0 && m.reward_amount.is_none() {
        return Err(invalid("xp", "Give the mission XP or a reward."));
    }
    let params = clean_params(&m.params)?;
    let starts = super::parse_time(&m.starts_at)?.unwrap_or_else(Utc::now);
    let ends = super::parse_time(&m.ends_at)?;
    if ends.is_some_and(|e| e <= starts) {
        return Err(invalid("endsAt", "The end must be after the start."));
    }
    let payout = m.reward_amount.map(|_| m.reward_payout.clone().unwrap_or_else(|| "wallet".into()));
    let q = match id {
        None => sqlx::query(
            "INSERT INTO missions (tenant, name, description, kind, schedule, target, params, xp, reward_amount, reward_payout, kyc_required, active, starts_at, ends_at, sort, created_by)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16) RETURNING *",
        ),
        Some(_) => sqlx::query(
            "UPDATE missions SET name = $2, description = $3, kind = $4, schedule = $5, target = $6, params = $7, xp = $8, reward_amount = $9, reward_payout = $10, kyc_required = $11,
               active = $12, starts_at = $13, ends_at = $14, sort = $15, updated_at = now()
             WHERE tenant = $1 AND id = $17 AND $16::text IS NOT NULL RETURNING *",
        ),
    };
    let row = q
        .bind(&s.tenant)
        .bind(&m.name)
        .bind(&m.description)
        .bind(&m.kind)
        .bind(&m.schedule)
        .bind(m.target)
        .bind(sqlx::types::Json(&params))
        .bind(m.xp)
        .bind(m.reward_amount)
        .bind(&payout)
        .bind(m.kyc_required)
        .bind(m.active)
        .bind(starts)
        .bind(ends)
        .bind(m.sort)
        .bind(s.actor().label())
        .bind(id.unwrap_or(0))
        .fetch_optional(&st.pool)
        .await?
        .ok_or(ApiError::NotFound)?;
    let v = missions::mission_json(&row);
    audit::record(&st.pool, &s.tenant, &s.actor(), if id.is_some() { "mission.update" } else { "mission.create" }, Some(format!("mission:{}", row.get::<i64, _>("id"))), before, Some(v.clone()), None).await?;
    Ok(Json(json!({"mission": v})))
}

pub async fn list_missions(State(st): State<AppState>, s: StaffCtx) -> ApiResult<Json<Value>> {
    s.require(ROLES_READ)?;
    missions::seed(&st.pool, &s.tenant).await?;
    let rows = sqlx::query("SELECT * FROM missions WHERE tenant = $1 ORDER BY active DESC, sort, id").bind(&s.tenant).fetch_all(&st.pool).await?;
    let stats: Vec<(i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT mission_id, count(*) FILTER (WHERE completed_at IS NOT NULL), count(*) FILTER (WHERE completed_at > now() - interval '7 days'), count(DISTINCT user_id)
           FROM mission_progress WHERE tenant = $1 GROUP BY mission_id",
    )
    .bind(&s.tenant)
    .fetch_all(&st.pool)
    .await?;
    let settings = db::settings(&st.pool, &s.tenant).await?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            let mut v = missions::mission_json(r);
            let id: i64 = r.get("id");
            let x = stats.iter().find(|x| x.0 == id).copied().unwrap_or((id, 0, 0, 0));
            v["completions"] = json!(x.1);
            v["completions7d"] = json!(x.2);
            v["participants"] = json!(x.3);
            v
        })
        .collect();
    Ok(Json(json!({
        "items": items,
        "kinds": missions::KINDS,
        "defaults": {"minLots": num(settings.mission_min_lots), "minHoldSeconds": settings.mission_min_hold_seconds, "autoPayBelow": num(settings.auto_pay_below)},
    })))
}

pub async fn create_mission(State(st): State<AppState>, s: StaffCtx, Json(body): Json<Value>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    write_mission(&st, &s, None, parse(body)?, None).await
}

pub async fn patch_mission(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>, Json(body): Json<Value>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    let r = sqlx::query("SELECT * FROM missions WHERE id = $1 AND tenant = $2").bind(id).bind(&s.tenant).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    let before = missions::mission_json(&r);
    write_mission(&st, &s, Some(id), parse(merge(before.clone(), &body))?, Some(before)).await
}

#[derive(Deserialize)]
pub struct RewardsQ {
    status: Option<String>,
    page: Option<i64>,
    limit: Option<i64>,
}

/// `GET /admin/payouts?status=open|approval|awaiting_second_approval|pending|failed|paid|kyc|rejected|all`: mission
/// rewards and contest prizes in one list (what waits for a second approval first, then approval). `open` = both
/// waiting states. Each row carries `fourEyes` (first approver, threshold).
pub async fn payouts(State(st): State<AppState>, s: StaffCtx, Query(q): Query<RewardsQ>) -> ApiResult<Json<Value>> {
    s.require(ROLES_READ)?;
    let (page, limit, off) = paging(q.page, q.limit, 50, 200);
    let open = q.status.as_deref() == Some("open");
    let status = q.status.filter(|x| matches!(x.as_str(), "approval" | "awaiting_second_approval" | "pending" | "failed" | "paid" | "kyc" | "rejected"));
    let rows = sqlx::query(
        "SELECT x.*, f.first_name, f.last_name, count(*) OVER () AS total FROM (
           SELECT 'mission' AS source, p.id, p.user_id, p.reward_amount AS amount, p.reward_payout AS payout, p.reward_status AS status, p.completed_at AS at,
                  m.name AS title, p.reward_error AS error, NULL::bigint AS contest_id,
                  p.first_approved_by_id, p.first_approved_by_name, p.first_approved_at, p.first_approval_note, p.four_eyes_threshold
             FROM mission_progress p JOIN missions m ON m.id = p.mission_id WHERE p.tenant = $1 AND p.reward_amount > 0 AND p.completed_at IS NOT NULL
           UNION ALL
           SELECT 'contest', e.id, e.user_id, e.prize_amount, e.prize_payout, e.prize_status, COALESCE(c.finalized_at, c.ends_at), c.name || ' · rank #' || COALESCE(e.rank, 0), e.prize_error, c.id,
                  e.first_approved_by_id, e.first_approved_by_name, e.first_approved_at, e.first_approval_note, e.four_eyes_threshold
             FROM contest_entries e JOIN contests c ON c.id = e.contest_id WHERE e.tenant = $1 AND e.prize_amount > 0
         ) x LEFT JOIN profiles f ON f.tenant = $1 AND f.user_id = x.user_id
         WHERE ($2::text IS NULL OR x.status = $2) AND (NOT $5 OR x.status IN ('approval', 'awaiting_second_approval'))
         ORDER BY CASE x.status WHEN 'awaiting_second_approval' THEN 0 WHEN 'approval' THEN 1 WHEN 'failed' THEN 2 WHEN 'kyc' THEN 3 WHEN 'pending' THEN 4 ELSE 5 END, x.at DESC OFFSET $3 LIMIT $4",
    )
    .bind(&s.tenant)
    .bind(&status)
    .bind(off)
    .bind(limit)
    .bind(open)
    .fetch_all(&st.pool)
    .await?;
    let total = rows.first().map(|r| r.get::<i64, _>("total")).unwrap_or(0);
    let settings = db::settings(&st.pool, &s.tenant).await?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            let first: Option<String> = r.try_get("first_name").ok();
            let last: Option<String> = r.try_get("last_name").ok();
            json!({
                "source": r.get::<String, _>("source"), "id": r.get::<i64, _>("id"), "contestId": r.get::<Option<i64>, _>("contest_id"), "userId": r.get::<i64, _>("user_id"),
                "name": format!("{} {}", first.unwrap_or_default(), last.unwrap_or_default()).trim(),
                "amount": num(r.get("amount")), "payout": r.get::<Option<String>, _>("payout"), "status": r.get::<String, _>("status"),
                "title": r.get::<String, _>("title"), "error": r.get::<Option<String>, _>("error"), "at": r.get::<Option<DateTime<Utc>>, _>("at"),
                "fourEyes": crate::four_eyes::json_of(r, r.get("amount"), r.get::<&str, _>("status"), Some(settings.four_eyes_threshold)),
            })
        })
        .collect();
    Ok(Json(json!({"items": items, "total": total, "page": page, "limit": limit, "autoPayBelow": num(settings.auto_pay_below), "fourEyesThreshold": num(settings.four_eyes_threshold)})))
}

#[derive(Deserialize, Default)]
pub struct DecideBody {
    #[serde(default)]
    note: String,
}

/// `POST /admin/missions/rewards/{id}/approve|reject` (marketing.approve).
pub async fn decide_mission_reward(State(st): State<AppState>, s: StaffCtx, Path((id, action)): Path<(i64, String)>, body: Option<Json<DecideBody>>) -> ApiResult<Json<Value>> {
    s.require(ROLES_APPROVE)?;
    let note = body.map(|b| b.0.note).unwrap_or_default();
    match action.as_str() {
        "approve" => missions::decide_reward(&st, &s.tenant, id, true, &s.actor(), &note).await.map(Json),
        "reject" => {
            if note.trim().is_empty() {
                return Err(invalid("note", "Say why the reward is rejected."));
            }
            missions::decide_reward(&st, &s.tenant, id, false, &s.actor(), &note).await.map(Json)
        }
        _ => Err(ApiError::NotFound),
    }
}

// ---------------------------------------------------------------- admin: levels, XP

pub async fn get_levels(State(st): State<AppState>, s: StaffCtx) -> ApiResult<Json<Value>> {
    s.require(ROLES_READ)?;
    let levels = xp::levels(&st.pool, &s.tenant).await?;
    let counts: Vec<(i64,)> = sqlx::query_as("SELECT sum(xp)::bigint FROM xp_ledger WHERE tenant = $1 GROUP BY user_id").bind(&s.tenant).fetch_all(&st.pool).await?;
    let mut per: HashMap<i32, i64> = HashMap::new();
    for (x,) in &counts {
        *per.entry(cc::level_for(&levels, *x).0.level).or_default() += 1;
    }
    let items: Vec<Value> = levels.iter().map(|l| json!({"level": l.level, "name": l.name, "minXp": l.min_xp, "badge": l.badge, "clients": per.get(&l.level).copied().unwrap_or(0)})).collect();
    Ok(Json(json!({"levels": items, "badges": cc::BADGES})))
}

#[derive(Deserialize)]
pub struct LevelsBody {
    levels: Vec<Level>,
}

pub async fn put_levels(State(st): State<AppState>, s: StaffCtx, Json(b): Json<LevelsBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    cc::validate_levels(&b.levels).map_err(|m| invalid("levels", m))?;
    let before = xp::levels(&st.pool, &s.tenant).await?;
    let mut sorted = b.levels.clone();
    sorted.sort_by_key(|l| l.level);
    let mut tx = st.pool.begin().await?;
    xp::replace_levels(&mut tx, &s.tenant, &sorted).await?;
    audit::record(&mut *tx, &s.tenant, &s.actor(), "levels.update", None, Some(json!(before)), Some(json!(sorted)), None).await?;
    tx.commit().await?;
    Ok(Json(json!({"levels": sorted})))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XpAdjust {
    user_id: i64,
    xp: i64,
    note: String,
}

pub async fn adjust_xp(State(st): State<AppState>, s: StaffCtx, Json(b): Json<XpAdjust>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    if b.xp == 0 || b.xp.abs() > 100_000 {
        return Err(invalid("xp", "±1 – 100000 XP."));
    }
    let note = text(&b.note, "note", 200, true)?;
    let reference = format!("adjust:{}", db::random_code(12));
    let mut tx = st.pool.begin().await?;
    xp::award_in(&mut tx, &s.tenant, b.user_id, b.xp, "adjust", &reference, &note).await?;
    audit::record(&mut *tx, &s.tenant, &s.actor(), "xp.adjust", Some(format!("user:{}", b.user_id)), None, Some(json!({"xp": b.xp})), Some(&note)).await?;
    tx.commit().await?;
    Ok(Json(json!({"xp": xp::total(&st.pool, &s.tenant, b.user_id).await?})))
}
