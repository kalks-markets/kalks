//! Leaderboards (phase 4): nicknames, contest and XP boards for the Client Area, the app, Kalks Trader and the
//! public website. Other people only ever see a nickname, a rank, a score and a level badge: never a name, email,
//! login, user id or country. A client can rename their nickname and choose whether it shows on the public
//! website; clients who opted out (or, when the broker's `nicknamePublicDefault` is off, never opted in) are left
//! out of public boards entirely.

use crate::challenge_calc::{self as cc, Level};
use crate::contests;
use crate::db;
use crate::error::{ApiError, ApiResult, invalid};
use crate::money::num;
use crate::state::AppState;
use crate::xp;
use chrono::{DateTime, Datelike, Utc};
use serde_json::{Value, json};
use sqlx::Row;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct Nick {
    pub nickname: String,
    /// None = the broker's default.
    pub public: Option<bool>,
    pub custom: bool,
}

/// The client's nickname, created (auto-generated, unique per broker) on first use.
pub async fn ensure(pool: &sqlx::PgPool, tenant: &str, user_id: i64) -> anyhow::Result<Nick> {
    if let Some(r) = sqlx::query("SELECT nickname, public, custom FROM nicknames WHERE tenant = $1 AND user_id = $2").bind(tenant).bind(user_id).fetch_optional(pool).await? {
        return Ok(Nick { nickname: r.get("nickname"), public: r.get("public"), custom: r.get("custom") });
    }
    for salt in 0..20u32 {
        let n = cc::auto_nickname(tenant, user_id, salt);
        let r = sqlx::query("INSERT INTO nicknames (tenant, user_id, nickname) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING").bind(tenant).bind(user_id).bind(&n).execute(pool).await?;
        if r.rows_affected() > 0 {
            return Ok(Nick { nickname: n, public: None, custom: false });
        }
        // a concurrent call created this client's row: read it
        if let Some(r) = sqlx::query("SELECT nickname, public, custom FROM nicknames WHERE tenant = $1 AND user_id = $2").bind(tenant).bind(user_id).fetch_optional(pool).await? {
            return Ok(Nick { nickname: r.get("nickname"), public: r.get("public"), custom: r.get("custom") });
        }
    }
    anyhow::bail!("no free nickname")
}

/// Nicknames of many clients (the deterministic auto nickname for clients without a row yet).
pub async fn many(pool: &sqlx::PgPool, tenant: &str, users: &[i64]) -> anyhow::Result<HashMap<i64, Nick>> {
    let rows = sqlx::query("SELECT user_id, nickname, public, custom FROM nicknames WHERE tenant = $1 AND user_id = ANY($2)").bind(tenant).bind(users).fetch_all(pool).await?;
    let mut m: HashMap<i64, Nick> = rows.iter().map(|r| (r.get::<i64, _>("user_id"), Nick { nickname: r.get("nickname"), public: r.get("public"), custom: r.get("custom") })).collect();
    for u in users {
        m.entry(*u).or_insert_with(|| Nick { nickname: cc::auto_nickname(tenant, *u, 0), public: None, custom: false });
    }
    Ok(m)
}

pub async fn nickname_json(st: &AppState, tenant: &str, user_id: i64) -> anyhow::Result<Value> {
    let n = ensure(&st.pool, tenant, user_id).await?;
    let d = db::settings(&st.pool, tenant).await?.nickname_public_default;
    Ok(json!({"nickname": n.nickname, "custom": n.custom, "public": n.public.unwrap_or(d), "publicChoice": n.public, "publicDefault": d, "suggestion": cc::auto_nickname(tenant, user_id, (Utc::now().timestamp() % 997) as u32 + 1)}))
}

/// Renames and / or sets the public choice. Unique per broker (case-insensitive).
pub async fn set(st: &AppState, tenant: &str, user_id: i64, nickname: Option<&str>, public: Option<bool>) -> ApiResult<Value> {
    ensure(&st.pool, tenant, user_id).await?;
    if let Some(n) = nickname {
        let n = cc::check_nickname(n).map_err(|m| invalid("nickname", m))?;
        let r = sqlx::query("UPDATE nicknames SET nickname = $3, custom = true, updated_at = now() WHERE tenant = $1 AND user_id = $2").bind(tenant).bind(user_id).bind(&n).execute(&st.pool).await;
        match r {
            Err(sqlx::Error::Database(d)) if d.code().as_deref() == Some("23505") => return Err(ApiError::Conflict { code: "nickname_taken", message: "That nickname is taken. Try another one.".into() }),
            Err(e) => return Err(ApiError::Internal(e.into())),
            Ok(_) => {}
        }
    }
    if let Some(p) = public {
        sqlx::query("UPDATE nicknames SET public = $3, updated_at = now() WHERE tenant = $1 AND user_id = $2").bind(tenant).bind(user_id).bind(p).execute(&st.pool).await?;
    }
    crate::audit::record(&st.pool, tenant, &crate::audit::Actor { id: format!("user:{user_id}"), name: None }, "nickname.update", Some(format!("user:{user_id}")), None, Some(json!({"nickname": nickname, "public": public})), None).await?;
    invalidate(tenant);
    Ok(nickname_json(st, tenant, user_id).await?)
}

fn visible(n: &Nick, default: bool) -> bool {
    n.public.unwrap_or(default)
}

// ---------------------------------------------------------------- boards

/// Which board: one contest, or XP earned in a period (`week`, `month`, `all`).
#[derive(Clone, Debug, PartialEq)]
pub enum Board {
    Contest(String),
    Period(String),
}

impl Board {
    pub fn parse(contest: Option<&str>, period: Option<&str>) -> ApiResult<Self> {
        match (contest.map(str::trim).filter(|c| !c.is_empty()), period.map(str::trim).filter(|p| !p.is_empty())) {
            (Some(c), _) => {
                if c.len() > 60 || !c.chars().all(|x| x.is_ascii_alphanumeric() || x == '-') {
                    return Err(invalid("contest", "Unknown contest."));
                }
                Ok(Board::Contest(c.to_lowercase()))
            }
            (None, Some(p)) if matches!(p, "week" | "month" | "all") => Ok(Board::Period(p.into())),
            (None, Some(_)) => Err(invalid("period", "period must be week, month or all.")),
            (None, None) => Ok(Board::Period("week".into())),
        }
    }
}

/// A leaderboard. `me` = the viewing client (their row is marked and returned as `me` even when hidden or below the
/// limit); `public` = the website view (opted-out clients left out, no `me`).
pub async fn board(st: &AppState, tenant: &str, b: &Board, me: Option<i64>, public: bool, limit: i64) -> ApiResult<Value> {
    let settings = db::settings(&st.pool, tenant).await?;
    let levels = xp::levels(&st.pool, tenant).await?;
    match b {
        Board::Contest(key) => contest_board(st, tenant, key, me, public, limit, settings.nickname_public_default, &levels).await,
        Board::Period(p) => xp_board(st, tenant, p, me, public, limit, settings.nickname_public_default, &levels).await,
    }
}

fn row_json(rank: Option<i32>, nick: &str, score: Value, level: Value, mine: bool, public: bool) -> Value {
    let mut v = json!({"rank": rank, "nickname": nick, "score": score, "level": level});
    if !public {
        v["me"] = json!(mine);
    }
    v
}

#[allow(clippy::too_many_arguments)]
async fn contest_board(st: &AppState, tenant: &str, key: &str, me: Option<i64>, public: bool, limit: i64, default: bool, levels: &[Level]) -> ApiResult<Value> {
    let c = match key.parse::<i64>() {
        Ok(id) => sqlx::query(sqlx::AssertSqlSafe(format!("{} WHERE c.tenant = $1 AND c.id = $2 AND c.status NOT IN ('draft','cancelled')", contests::CONTEST_SELECT))).bind(tenant).bind(id).fetch_optional(&st.pool).await?,
        Err(_) => sqlx::query(sqlx::AssertSqlSafe(format!("{} WHERE c.tenant = $1 AND c.slug = $2 AND c.status NOT IN ('draft','cancelled')", contests::CONTEST_SELECT))).bind(tenant).bind(key).fetch_optional(&st.pool).await?,
    }
    .ok_or(ApiError::NotFound)?;
    let cid: i64 = c.get("id");
    let scoring: String = c.get("scoring");
    // ranked entries only (disqualified ones have no rank), in rank order
    let rows = sqlx::query("SELECT user_id, rank, score, return_pct, realised, floating, lots, contracts, trades FROM contest_entries WHERE contest_id = $1 AND rank IS NOT NULL ORDER BY rank LIMIT 1000").bind(cid).fetch_all(&st.pool).await?;
    let users: Vec<i64> = rows.iter().map(|r| r.get::<i64, _>("user_id")).collect();
    let nicks = many(&st.pool, tenant, &users).await?;
    let xps = xp::totals(&st.pool, tenant, &users).await?;
    let mut items = vec![];
    let mut mine = None;
    for r in &rows {
        let u: i64 = r.get("user_id");
        let n = &nicks[&u];
        let row = row_json(r.get("rank"), &n.nickname, num(r.get("score")), xp::badge_json(levels, *xps.get(&u).unwrap_or(&0)), me == Some(u), public);
        if me == Some(u) {
            mine = Some(row.clone());
        }
        if public && !visible(n, default) {
            continue;
        }
        if (items.len() as i64) < limit {
            items.push(row);
        }
    }
    let c_json = contests::contest_json(&c);
    let mut out = json!({
        "board": "contest",
        "contest": {"id": cid, "slug": c_json["slug"], "name": c_json["name"], "status": c_json["status"], "kind": c_json["kind"], "scoring": scoring,
                    "startsAt": c_json["startsAt"], "endsAt": c_json["endsAt"], "prizePool": c_json["prizePool"], "entrants": c_json["entrants"], "prizes": c_json["prizes"]},
        "items": items,
        "updatedAt": c.get::<Option<DateTime<Utc>>, _>("refreshed_at"),
    });
    if !public {
        out["me"] = mine.unwrap_or(Value::Null);
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
async fn xp_board(st: &AppState, tenant: &str, period: &str, me: Option<i64>, public: bool, limit: i64, default: bool, levels: &[Level]) -> ApiResult<Value> {
    let now = Utc::now();
    let since = match period {
        "week" => cc::period("weekly", now).1,
        "month" => cc::day_start(cc::server_day(now).with_day(1).unwrap_or(cc::server_day(now))),
        _ => DateTime::<Utc>::UNIX_EPOCH,
    };
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT user_id, sum(xp)::bigint AS x FROM xp_ledger WHERE tenant = $1 AND created_at >= $2 GROUP BY user_id HAVING sum(xp) > 0 ORDER BY x DESC, min(created_at), user_id LIMIT 1000",
    )
    .bind(tenant)
    .bind(since)
    .fetch_all(&st.pool)
    .await?;
    let users: Vec<i64> = rows.iter().map(|r| r.0).collect();
    let nicks = many(&st.pool, tenant, &users).await?;
    let totals = xp::totals(&st.pool, tenant, &users).await?;
    let mut items = vec![];
    let mut mine = None;
    for (i, (u, x)) in rows.iter().enumerate() {
        let n = &nicks[u];
        let row = row_json(Some(i as i32 + 1), &n.nickname, json!(x), xp::badge_json(levels, *totals.get(u).unwrap_or(&0)), me == Some(*u), public);
        if me == Some(*u) {
            mine = Some(row.clone());
        }
        if public && !visible(n, default) {
            continue;
        }
        if (items.len() as i64) < limit {
            items.push(row);
        }
    }
    let mut out = json!({"board": "xp", "period": period, "since": if period == "all" { Value::Null } else { json!(since) }, "scoring": "xp", "items": items, "updatedAt": now});
    if !public {
        out["me"] = match (mine, me) {
            (Some(m), _) => m,
            (None, Some(u)) => {
                let total = xp::total(&st.pool, tenant, u).await?;
                let n = ensure(&st.pool, tenant, u).await?;
                row_json(None, &n.nickname, json!(0), xp::badge_json(levels, total), true, false)
            }
            _ => Value::Null,
        };
    }
    Ok(out)
}

// ---------------------------------------------------------------- public cache

type Cache = HashMap<String, (Instant, Value)>;
static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(Default::default);
pub const PUBLIC_TTL_SECS: u64 = 15;

/// The public board, cached `PUBLIC_TTL_SECS` per (broker, board, limit) so the website can't load the database.
pub async fn public_board(st: &AppState, tenant: &str, b: &Board, limit: i64) -> ApiResult<Value> {
    let key = format!("{tenant}|{b:?}|{limit}");
    if let Some((at, v)) = CACHE.lock().unwrap().get(&key).cloned()
        && at.elapsed().as_secs() < PUBLIC_TTL_SECS
    {
        return Ok(v);
    }
    let v = board(st, tenant, b, None, true, limit).await?;
    let mut c = CACHE.lock().unwrap();
    if c.len() > 500 {
        c.clear();
    }
    c.insert(key, (Instant::now(), v.clone()));
    Ok(v)
}

/// Drops a broker's cached public boards (a client renamed or opted out: it shows at once).
pub fn invalidate(tenant: &str) {
    let p = format!("{tenant}|");
    CACHE.lock().unwrap().retain(|k, _| !k.starts_with(&p));
}

/// Public list of contests (website): running, upcoming and finished in the last 30 days. No entrant data.
pub async fn public_contests(st: &AppState, tenant: &str) -> anyhow::Result<Value> {
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "{} WHERE c.tenant = $1 AND c.status NOT IN ('draft','cancelled') AND c.ends_at > now() - interval '30 days' ORDER BY c.starts_at DESC LIMIT 50",
        contests::CONTEST_SELECT
    )))
    .bind(tenant)
    .fetch_all(&st.pool)
    .await?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            let c = contests::contest_json(r);
            json!({"id": c["id"], "slug": c["slug"], "name": c["name"], "description": c["description"], "status": c["status"], "kind": c["kind"], "instrument": c["instrument"],
                   "scoring": c["scoring"], "startsAt": c["startsAt"], "endsAt": c["endsAt"], "prizePool": c["prizePool"], "prizes": c["prizes"], "entrants": c["entrants"],
                   "kycRequired": c["kycRequired"], "minTrades": c["minTrades"]})
        })
        .collect();
    Ok(json!({"items": items, "updatedAt": Utc::now()}))
}

/// Kalks Trader's rank widget: the client's running (or just ended) contests entered with `login`, plus the level.
pub async fn rank_widget(st: &AppState, tenant: &str, user_id: i64, login: Option<i64>) -> anyhow::Result<Value> {
    let rows = sqlx::query(
        "SELECT c.id, c.name, c.scoring, c.ends_at, c.status, e.rank, e.score, e.trades, c.min_trades, e.login,
                (SELECT count(*) FROM contest_entries x WHERE x.contest_id = c.id AND x.rank IS NOT NULL) AS ranked
           FROM contest_entries e JOIN contests c ON c.id = e.contest_id
          WHERE e.tenant = $1 AND e.user_id = $2 AND ($3::bigint IS NULL OR e.login = $3) AND e.status = 'active'
            AND c.status NOT IN ('draft','cancelled') AND c.starts_at <= now() AND c.ends_at > now() - interval '1 day'
          ORDER BY c.ends_at LIMIT 5",
    )
    .bind(tenant)
    .bind(user_id)
    .bind(login)
    .fetch_all(&st.pool)
    .await?;
    let levels = xp::levels(&st.pool, tenant).await?;
    let total = xp::total(&st.pool, tenant, user_id).await?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({"id": r.get::<i64, _>("id"), "name": r.get::<String, _>("name"), "scoring": r.get::<String, _>("scoring"), "endsAt": r.get::<DateTime<Utc>, _>("ends_at"),
                   "rank": r.get::<Option<i32>, _>("rank"), "ranked": r.get::<i64, _>("ranked"), "score": num(r.get("score")), "trades": r.get::<i32, _>("trades"),
                   "minTrades": r.get::<i32, _>("min_trades"), "login": r.get::<i64, _>("login"), "running": r.get::<DateTime<Utc>, _>("ends_at") > Utc::now()})
        })
        .collect();
    Ok(json!({"contests": items, "level": xp::level_json(&levels, total), "updatedAt": Utc::now()}))
}
