//! Client Area routes (`/v1/growth/me/*`).

use super::{UserCtx, paging};
use crate::bonus::{self, ClaimOpts};
use crate::calc::{self, Segment};
use crate::clients;
use crate::contests::{self, CONTEST_SELECT};
use crate::error::{ApiError, ApiResult};
use crate::loyalty;
use crate::money::{D, ZERO, num};
use crate::profiles::{self, Profile};
use crate::promos;
use crate::shares::{self, ShareReq};
use crate::state::AppState;
use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;

/// Refreshes the profile from the BFF's hints and returns it.
async fn profile(st: &AppState, u: &UserCtx) -> ApiResult<Profile> {
    profiles::touch(st, &u.tenant, u.user_id, &u.hints).await?;
    // a day the client opened the Client Area or the app (login streak, phase 4); never fails the call
    if let Err(e) = super::challenges::touch_login(st, &u.tenant, u.user_id).await {
        tracing::debug!(error = %e, "login streak not recorded");
    }
    Ok(profiles::get(st, &u.tenant, u.user_id).await?)
}

pub async fn rewards(State(st): State<AppState>, u: UserCtx) -> ApiResult<Json<Value>> {
    profile(&st, &u).await?;
    Ok(Json(loyalty::rewards(&st, &u.tenant, u.user_id).await?))
}

#[derive(Deserialize)]
pub struct PageQ {
    kind: Option<String>,
    page: Option<i64>,
    limit: Option<i64>,
}

pub async fn points(State(st): State<AppState>, u: UserCtx, Query(q): Query<PageQ>) -> ApiResult<Json<Value>> {
    let (page, limit, off) = paging(q.page, q.limit, 25, 200);
    let kind = q.kind.filter(|k| !k.is_empty() && k != "all");
    let rows = sqlx::query("SELECT *, count(*) OVER () AS total FROM points_ledger WHERE tenant = $1 AND user_id = $2 AND ($3::text IS NULL OR kind = $3) ORDER BY id DESC OFFSET $4 LIMIT $5")
        .bind(&u.tenant)
        .bind(u.user_id)
        .bind(&kind)
        .bind(off)
        .bind(limit)
        .fetch_all(&st.pool)
        .await?;
    let total = rows.first().map(|r| r.get::<i64, _>("total")).unwrap_or(0);
    Ok(Json(json!({"items": rows.iter().map(loyalty::tx_json).collect::<Vec<_>>(), "page": page, "limit": limit, "total": total})))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedeemBody {
    item_id: i64,
    login: Option<i64>,
}

pub async fn redeem(State(st): State<AppState>, u: UserCtx, Json(b): Json<RedeemBody>) -> ApiResult<Json<Value>> {
    profile(&st, &u).await?;
    Ok(Json(loyalty::redeem(&st, &u.tenant, u.user_id, b.item_id, b.login).await?))
}

pub async fn redemptions(State(st): State<AppState>, u: UserCtx) -> ApiResult<Json<Value>> {
    let rows = sqlx::query("SELECT * FROM redemptions WHERE tenant = $1 AND user_id = $2 ORDER BY id DESC LIMIT 100").bind(&u.tenant).bind(u.user_id).fetch_all(&st.pool).await?;
    Ok(Json(json!({"items": rows.iter().map(loyalty::redemption_json).collect::<Vec<_>>()})))
}

pub async fn vouchers(State(st): State<AppState>, u: UserCtx) -> ApiResult<Json<Value>> {
    sqlx::query("UPDATE vouchers SET status = 'expired' WHERE tenant = $1 AND user_id = $2 AND status = 'active' AND expires_at <= now()").bind(&u.tenant).bind(u.user_id).execute(&st.pool).await?;
    let rows = sqlx::query("SELECT * FROM vouchers WHERE tenant = $1 AND user_id = $2 ORDER BY id DESC LIMIT 100").bind(&u.tenant).bind(u.user_id).fetch_all(&st.pool).await?;
    Ok(Json(json!({"items": rows.iter().map(loyalty::voucher_json).collect::<Vec<_>>()})))
}

pub async fn cashback(State(st): State<AppState>, u: UserCtx) -> ApiResult<Json<Value>> {
    profile(&st, &u).await?;
    Ok(Json(crate::cashback::me(&st, &u.tenant, u.user_id).await?))
}

pub async fn enrol(State(st): State<AppState>, u: UserCtx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let ok: Option<bool> = sqlx::query_scalar("SELECT opt_in FROM cashback_programmes WHERE id = $1 AND tenant = $2 AND active AND (ends_at IS NULL OR ends_at > now())").bind(id).bind(&u.tenant).fetch_optional(&st.pool).await?;
    if ok.is_none() {
        return Err(ApiError::NotFound);
    }
    sqlx::query("INSERT INTO cashback_enrolments (tenant, programme_id, user_id) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING").bind(&u.tenant).bind(id).bind(u.user_id).execute(&st.pool).await?;
    Ok(Json(json!({"enrolled": true})))
}

// ---------------------------------------------------------------- promotions

pub async fn promotions(State(st): State<AppState>, u: UserCtx) -> ApiResult<Json<Value>> {
    let p = profile(&st, &u).await?;
    let seg = p.segment();
    let rows = sqlx::query(
        "SELECT c.*, (SELECT count(*) FROM bonus_grants g WHERE g.campaign_id = c.id AND g.status NOT IN ('cancelled','failed')) AS claims,
                (SELECT count(*) FROM bonus_grants g WHERE g.campaign_id = c.id AND g.user_id = $2 AND g.status NOT IN ('cancelled','failed')) AS mine
         FROM bonus_campaigns c WHERE c.tenant = $1 AND c.visibility = 'public' AND c.status = 'active' AND c.starts_at <= now() AND (c.ends_at IS NULL OR c.ends_at > now())
         ORDER BY c.id DESC",
    )
    .bind(&u.tenant)
    .bind(u.user_id)
    .fetch_all(&st.pool)
    .await?;
    let campaigns: Vec<Value> = rows
        .iter()
        .map(|r| {
            let mine: i64 = r.get("mine");
            let check = calc::check_limits(&bonus::limits_of(r, r.get("claims"), mine), &seg, Utc::now());
            let mut v = bonus::campaign_json(r);
            v["eligible"] = json!(check.is_ok());
            v["reason"] = json!(check.err().map(|e| e.1));
            v["claimed"] = json!(mine > 0);
            v
        })
        .collect();
    let grants = sqlx::query(sqlx::AssertSqlSafe(format!("{} WHERE g.tenant = $1 AND g.user_id = $2 ORDER BY g.id DESC LIMIT 50", bonus::GRANT_SELECT))).bind(&u.tenant).bind(u.user_id).fetch_all(&st.pool).await?;
    let uses = sqlx::query("SELECT r.*, p.kind FROM promo_redemptions r LEFT JOIN promo_codes p ON p.id = r.promo_id WHERE r.tenant = $1 AND r.user_id = $2 ORDER BY r.id DESC LIMIT 50")
        .bind(&u.tenant)
        .bind(u.user_id)
        .fetch_all(&st.pool)
        .await?;
    Ok(Json(json!({
        "campaigns": campaigns,
        "grants": grants.iter().map(bonus::grant_json).collect::<Vec<_>>(),
        "promoHistory": uses.iter().map(promos::use_json).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize, Default)]
pub struct LoginBody {
    login: Option<i64>,
}

pub async fn claim(State(st): State<AppState>, u: UserCtx, Path(id): Path<i64>, body: Option<Json<LoginBody>>) -> ApiResult<Json<Value>> {
    let p = profile(&st, &u).await?;
    let login = body.and_then(|b| b.0.login);
    let groups: Option<Vec<String>> = sqlx::query_scalar("SELECT account_groups FROM bonus_campaigns WHERE id = $1 AND tenant = $2").bind(id).bind(&u.tenant).fetch_optional(&st.pool).await?;
    let groups = groups.ok_or(ApiError::NotFound)?;
    let account = match login {
        Some(l) => Some(bonus::check_account(&st, &u.tenant, u.user_id, l, &groups).await?),
        None => None,
    };
    let mut tx = st.pool.begin().await?;
    let opts = ClaimOpts { source: "claim", require_public: true, amount_override: None, note: None, skip_limits: false };
    let gid = bonus::claim_in(&mut tx, &u.tenant, u.user_id, id, account.as_ref(), &p.segment(), &opts).await?;
    crate::audit::record(&mut *tx, &u.tenant, &crate::audit::Actor { id: format!("user:{}", u.user_id), name: None }, "bonus.claim", Some(format!("grant:{gid}")), None, Some(json!({"campaign": id, "login": login})), None).await?;
    tx.commit().await?;
    st.wake.notify_one();
    Ok(Json(json!({"grant": bonus::grant_by_id(&st, gid).await?})))
}

#[derive(Deserialize)]
pub struct PromoBody {
    code: String,
    login: Option<i64>,
}

pub async fn promo(State(st): State<AppState>, u: UserCtx, Json(b): Json<PromoBody>) -> ApiResult<Json<Value>> {
    let p = profile(&st, &u).await?;
    Ok(Json(promos::redeem(&st, &u.tenant, u.user_id, &b.code, b.login, &p.segment()).await?))
}

// ---------------------------------------------------------------- contests

pub async fn contests(State(st): State<AppState>, u: UserCtx) -> ApiResult<Json<Value>> {
    profile(&st, &u).await?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!("{CONTEST_SELECT} WHERE c.tenant = $1 AND c.status NOT IN ('draft','cancelled') ORDER BY c.starts_at DESC LIMIT 100")))
        .bind(&u.tenant)
        .fetch_all(&st.pool)
        .await?;
    let mine = sqlx::query("SELECT e.*, c.min_trades FROM contest_entries e JOIN contests c ON c.id = e.contest_id WHERE e.tenant = $1 AND e.user_id = $2").bind(&u.tenant).bind(u.user_id).fetch_all(&st.pool).await?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            let mut v = contests::contest_json(r);
            let id: i64 = r.get("id");
            v["myEntry"] = mine.iter().find(|e| e.get::<i64, _>("contest_id") == id).map(|e| contests::standing_json(e, Some(u.user_id), false, e.get("min_trades"))).unwrap_or(Value::Null);
            v
        })
        .collect();
    let prizes: D = mine.iter().filter(|e| e.get::<String, _>("prize_status") == "paid").filter_map(|e| e.get::<Option<D>, _>("prize_amount")).sum();
    let finishes = mine.iter().filter(|e| e.get::<Option<D>, _>("prize_amount").is_some_and(|p| p > ZERO)).count();
    let best = mine.iter().filter(|e| {
        let cid: i64 = e.get("contest_id");
        rows.iter().any(|r| r.get::<i64, _>("id") == cid && matches!(contests::status_of(r).as_str(), "finalized" | "paid"))
    }).filter_map(|e| e.get::<Option<i32>, _>("rank")).min();
    let active = items.iter().filter(|c| c["status"] == "running").count();
    Ok(Json(json!({"items": items, "stats": {"entered": mine.len(), "prizesWon": num(prizes), "prizeFinishes": finishes, "bestRank": best, "active": active}})))
}

pub async fn contest(State(st): State<AppState>, u: UserCtx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let r = sqlx::query(sqlx::AssertSqlSafe(format!("{CONTEST_SELECT} WHERE c.id = $1 AND c.tenant = $2 AND c.status <> 'draft'"))).bind(id).bind(&u.tenant).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    let (board, mine) = contests::leaderboard(&st, id, Some(u.user_id), false, 100).await?;
    let entrants: i64 = r.get("entrants");
    Ok(Json(json!({"contest": contests::contest_json(&r), "leaderboard": board, "myEntry": mine, "entrants": entrants})))
}

pub async fn join(State(st): State<AppState>, u: UserCtx, Path(id): Path<i64>, body: Option<Json<LoginBody>>) -> ApiResult<Json<Value>> {
    let p = profile(&st, &u).await?;
    Ok(Json(contests::join(&st, &u.tenant, u.user_id, id, body.and_then(|b| b.0.login), &p).await?))
}

// ---------------------------------------------------------------- banners (D121)

pub fn banner_view(r: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": r.get::<i64, _>("id"),
        "title": r.get::<String, _>("title"),
        "body": r.get::<String, _>("body"),
        "ctaLabel": r.get::<Option<String>, _>("cta_label"),
        "ctaUrl": r.get::<Option<String>, _>("cta_url"),
        "imageUrl": r.get::<Option<String>, _>("image_url"),
        "tone": r.get::<String, _>("tone"),
        "placement": r.get::<String, _>("placement"),
        "dismissible": r.get::<bool, _>("dismissible"),
    })
}

/// Whether a banner targets this segment. `account_types`: the client's account kinds (`none` when empty);
/// None = unknown (engine unavailable): account-type targeted banners are skipped.
pub fn banner_matches(r: &sqlx::postgres::PgRow, seg: &Segment, account_types: Option<&[String]>) -> bool {
    let countries: Vec<String> = r.get("countries");
    if !countries.is_empty() && !countries.iter().any(|c| c.eq_ignore_ascii_case(&seg.country)) {
        return false;
    }
    let kyc: Vec<String> = r.get("kyc");
    if !kyc.is_empty() && !kyc.iter().any(|k| k == &seg.kyc) {
        return false;
    }
    let types: Vec<String> = r.get("account_types");
    if !types.is_empty() {
        let Some(mine) = account_types else { return false };
        if !types.iter().any(|t| mine.contains(t)) {
            return false;
        }
    }
    if let Some(days) = r.get::<Option<i32>, _>("new_users_days") {
        if !seg.signed_up_at.is_some_and(|t| Utc::now() - t <= Duration::days(days as i64)) {
            return false;
        }
    }
    true
}

pub const BANNER_LIVE: &str = "SELECT * FROM banners WHERE tenant = $1 AND active AND starts_at <= now() AND (ends_at IS NULL OR ends_at > now()) AND ($2::text IS NULL OR placement = $2) ORDER BY priority DESC, id DESC";

#[derive(Deserialize)]
pub struct BannerQ {
    placement: Option<String>,
}

pub async fn banners(State(st): State<AppState>, u: UserCtx, Query(q): Query<BannerQ>) -> ApiResult<Json<Value>> {
    let p = profile(&st, &u).await?;
    let placement = q.placement.filter(|p| matches!(p.as_str(), "dashboard" | "wallet" | "rewards" | "terminal"));
    let rows = sqlx::query(BANNER_LIVE).bind(&u.tenant).bind(&placement).fetch_all(&st.pool).await?;
    if rows.is_empty() {
        return Ok(Json(json!({"items": []})));
    }
    let dismissed: Vec<i64> = sqlx::query_scalar("SELECT DISTINCT banner_id FROM banner_events WHERE user_id = $1 AND kind = 'dismiss'").bind(u.user_id).fetch_all(&st.pool).await?;
    let types: Option<Vec<String>> = if rows.iter().any(|r| !r.get::<Vec<String>, _>("account_types").is_empty()) {
        match clients::accounts_of(&st, &u.tenant, u.user_id, None).await {
            Ok(accs) => {
                let mut t: Vec<String> = accs.iter().map(|a| a.kind.clone()).collect();
                if t.is_empty() {
                    t.push("none".into());
                }
                Some(t)
            }
            Err(e) => {
                tracing::debug!(error = %e, "accounts unavailable for banner targeting");
                None
            }
        }
    } else {
        Some(vec![])
    };
    let seg = p.segment();
    let items: Vec<Value> = rows.iter().filter(|r| !dismissed.contains(&r.get::<i64, _>("id"))).filter(|r| banner_matches(r, &seg, types.as_deref())).map(banner_view).collect();
    Ok(Json(json!({"items": items})))
}

#[derive(Deserialize)]
pub struct EventBody {
    kind: String,
}

pub async fn banner_event(State(st): State<AppState>, u: UserCtx, Path(id): Path<i64>, Json(b): Json<EventBody>) -> ApiResult<Json<Value>> {
    if !matches!(b.kind.as_str(), "impression" | "click" | "dismiss") {
        return Err(crate::error::invalid("kind", "kind must be impression, click or dismiss."));
    }
    let n = sqlx::query("INSERT INTO banner_events (banner_id, user_id, kind, day) SELECT id, $3, $4, current_date FROM banners WHERE id = $1 AND tenant = $2 ON CONFLICT DO NOTHING")
        .bind(id)
        .bind(&u.tenant)
        .bind(u.user_id)
        .bind(&b.kind)
        .execute(&st.pool)
        .await?;
    let _ = n;
    Ok(Json(json!({"ok": true})))
}

// ---------------------------------------------------------------- shares (D136)

pub async fn shares(State(st): State<AppState>, u: UserCtx) -> ApiResult<Json<Value>> {
    let rows = sqlx::query("SELECT * FROM shares WHERE tenant = $1 AND user_id = $2 ORDER BY created_at DESC LIMIT 50").bind(&u.tenant).bind(u.user_id).fetch_all(&st.pool).await?;
    Ok(Json(json!({"items": rows.iter().map(|r| shares::share_json(r, false)).collect::<Vec<_>>()})))
}

pub async fn create_share(State(st): State<AppState>, u: UserCtx, Json(b): Json<ShareReq>) -> ApiResult<Json<Value>> {
    let p = profile(&st, &u).await?;
    Ok(Json(shares::create(&st, &u.tenant, u.user_id, &b, &p).await?))
}
