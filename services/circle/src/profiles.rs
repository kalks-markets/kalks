//! Profiles: one per client (gateway user id), created on first use with a suggested @handle. Badges come from
//! the other services (KYC from the gateway session, live account and master / PAMM status from the engine,
//! Academy certificates), staff badges (Kalks team / mentor) from the Back Office. The broker and the country are
//! stored for the module switch, the restricted-country rule and same-broker links, and never shown to others.

use crate::api::Client;
use crate::error::{ApiError, ApiResult, denied};
use crate::state::AppState;
use crate::upstream;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct Profile {
    pub user_id: i64,
    pub tenant: String,
    pub handle: String,
    pub display_name: String,
    pub bio: String,
    pub avatar: Option<sqlx::types::Json<Value>>,
    pub cover: Option<sqlx::types::Json<Value>>,
    pub private: bool,
    pub lang: String,
    pub country: String,
    pub kyc_status: String,
    pub live_trader: bool,
    pub master_id: Option<i64>,
    pub master_program: Option<String>,
    pub staff_badge: Option<String>,
    pub creator: bool,
    pub certificates: sqlx::types::Json<Value>,
    pub badges_at: Option<DateTime<Utc>>,
    pub show_stats: bool,
    pub stats: sqlx::types::Json<Value>,
    pub stats_at: Option<DateTime<Utc>>,
    pub risk_score: Option<f64>,
    pub referral_code: Option<String>,
    pub show_ib_link: bool,
    pub comments_default: String,
    pub dm_policy: String,
    pub xp: i64,
    pub level: i32,
    pub status: String,
    pub banned_until: Option<DateTime<Utc>>,
    pub ban_reason: Option<String>,
    pub shadow_hidden: bool,
    pub warnings: i32,
    pub fee_discount: bool,
    pub onboarded: bool,
    pub followers_count: i64,
    pub following_count: i64,
    pub posts_count: i64,
    pub pinned_post: Option<i64>,
    pub created_at: DateTime<Utc>,
}

pub const COLS: &str = "user_id, tenant, handle, display_name, bio, avatar, cover, private, lang, country, kyc_status, live_trader, master_id, master_program,
    staff_badge, creator, certificates, badges_at, show_stats, stats, stats_at, risk_score, referral_code, show_ib_link, comments_default, dm_policy, xp, level,
    status, banned_until, ban_reason, shadow_hidden, warnings, fee_discount, onboarded, followers_count, following_count, posts_count, pinned_post, created_at";

/// The profile columns prefixed with a table alias (`a.user_id, a.tenant, ...`).
pub fn cols(alias: &str) -> String {
    COLS.split(',').map(|c| format!("{alias}.{}", c.trim())).collect::<Vec<_>>().join(", ")
}

impl Profile {
    pub fn is_banned(&self) -> bool {
        self.status == "banned" && self.banned_until.is_none_or(|u| u > Utc::now())
    }
    pub fn verified(&self) -> bool {
        self.kyc_status == "verified"
    }

    /// Badge keys in display order.
    pub fn badges(&self) -> Vec<&'static str> {
        let mut b = Vec::new();
        match self.staff_badge.as_deref() {
            Some("team") => b.push("team"),
            Some("mentor") => b.push("mentor"),
            _ => {}
        }
        if self.verified() {
            b.push("verified");
        }
        match self.master_program.as_deref() {
            Some("copy") => b.push("master"),
            Some("pamm") => b.push("pamm"),
            Some("both") => {
                b.push("master");
                b.push("pamm");
            }
            _ => {}
        }
        if self.live_trader {
            b.push("live");
        }
        if self.creator {
            b.push("creator");
        }
        if self.certificates.0.as_array().is_some_and(|a| !a.is_empty()) {
            b.push("academy");
        }
        b
    }
}

pub fn image_urls(st: &AppState, v: &Option<sqlx::types::Json<Value>>) -> Value {
    match v.as_ref().and_then(|j| j.0.as_object()) {
        Some(m) => Value::Object(m.iter().filter_map(|(k, key)| Some((k.clone(), json!(st.storage.url(key.as_str()?))))).collect()),
        None => Value::Null,
    }
}

/// Compact author card embedded in posts, comments, messages and lists.
pub fn card(st: &AppState, p: &Profile) -> Value {
    json!({
        "id": p.user_id,
        "handle": p.handle,
        "displayName": p.display_name,
        "avatar": image_urls(st, &p.avatar),
        "badges": p.badges(),
        "level": p.level,
        "private": p.private,
    })
}

pub fn deleted_card(id: i64) -> Value {
    json!({"id": id, "handle": null, "displayName": "Kalks member", "avatar": null, "badges": [], "level": 1, "private": true})
}

pub async fn by_id(st: &AppState, id: i64) -> ApiResult<Option<Profile>> {
    Ok(sqlx::query_as::<_, Profile>(sqlx::AssertSqlSafe(format!("SELECT {COLS} FROM profiles WHERE user_id = $1"))).bind(id).fetch_optional(&st.pool).await?)
}

pub async fn by_handle(st: &AppState, handle: &str) -> ApiResult<Profile> {
    let h = handle.trim().trim_start_matches('@').to_lowercase();
    // numeric "handles" address a member by id (links from notifications of members who changed their handle)
    let q = if let Ok(id) = h.parse::<i64>() {
        sqlx::query_as::<_, Profile>(sqlx::AssertSqlSafe(format!("SELECT {COLS} FROM profiles WHERE user_id = $1"))).bind(id).fetch_optional(&st.pool).await?
    } else {
        sqlx::query_as::<_, Profile>(sqlx::AssertSqlSafe(format!("SELECT {COLS} FROM profiles WHERE handle = $1"))).bind(&h).fetch_optional(&st.pool).await?
    };
    q.ok_or(ApiError::NotFound)
}

pub async fn many(st: &AppState, ids: &[i64]) -> ApiResult<HashMap<i64, Profile>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut v: Vec<i64> = ids.to_vec();
    v.sort_unstable();
    v.dedup();
    let rows = sqlx::query_as::<_, Profile>(sqlx::AssertSqlSafe(format!("SELECT {COLS} FROM profiles WHERE user_id = ANY($1)"))).bind(&v).fetch_all(&st.pool).await?;
    Ok(rows.into_iter().map(|p| (p.user_id, p)).collect())
}

pub async fn cards(st: &AppState, ids: &[i64]) -> ApiResult<HashMap<i64, Value>> {
    Ok(many(st, ids).await?.iter().map(|(id, p)| (*id, card(st, p))).collect())
}

/// Settings, cached 10 s (read on every request for the country rule).
pub async fn settings(st: &AppState) -> crate::db::Settings {
    if let Some(v) = st.cache.get("settings", Duration::from_secs(10))
        && let Ok(s) = serde_json::from_value(v)
    {
        return s;
    }
    let s = crate::db::settings(&st.pool).await.unwrap_or_default();
    st.cache.put("settings", serde_json::to_value(&s).unwrap_or_default());
    s
}

/// Restricted countries: the service list (env), the Back Office list and the broker's own blocked countries.
pub async fn check_country(st: &AppState, c: &Client) -> ApiResult<()> {
    let s = settings(st).await;
    let broker = crate::modules::blocked_countries(st, &c.tenant).await;
    for cc in [c.country.as_deref(), c.ip_country.as_deref()].into_iter().flatten() {
        if st.cfg.restricted_countries.iter().any(|x| x == cc) || s.restricted_countries.iter().any(|x| x.eq_ignore_ascii_case(cc)) || broker.iter().any(|x| x == cc) {
            return Err(denied("restricted_country", "Kalks Circle isn't available in your country."));
        }
    }
    Ok(())
}

/// The caller's profile, created on first use (handle suggested from the name, display name = first name), and
/// refreshed with what the session says (broker, country, KYC, referral code).
pub async fn ensure(st: &AppState, c: &Client) -> ApiResult<Profile> {
    let first = c.name.split_whitespace().next().unwrap_or("").to_string();
    let display = if first.is_empty() { "Trader".to_string() } else { crate::util::clean(&first, 40) };
    let lang = c.locale.clone().unwrap_or_else(|| "en".into());
    let existing = by_id(st, c.id).await?;
    if let Some(p) = existing {
        let changed = p.tenant != c.tenant
            || c.country.as_ref().is_some_and(|x| *x != p.country)
            || c.kyc.as_ref().is_some_and(|x| *x != p.kyc_status)
            || (c.referral.is_some() && c.referral != p.referral_code);
        if changed {
            let row = sqlx::query_as::<_, Profile>(sqlx::AssertSqlSafe(format!(
                "UPDATE profiles SET tenant = $2, country = COALESCE($3, country), kyc_status = COALESCE($4, kyc_status), referral_code = COALESCE($5, referral_code),
                     last_seen_at = now(), updated_at = now() WHERE user_id = $1 RETURNING {COLS}"
            )))
            .bind(c.id)
            .bind(&c.tenant)
            .bind(&c.country)
            .bind(&c.kyc)
            .bind(&c.referral)
            .fetch_one(&st.pool)
            .await?;
            return Ok(row);
        }
        // last seen at most every 5 minutes
        sqlx::query("UPDATE profiles SET last_seen_at = now() WHERE user_id = $1 AND last_seen_at < now() - interval '5 minutes'").bind(c.id).execute(&st.pool).await?;
        return Ok(p);
    }
    let base = crate::text::handle_base(&c.name);
    for attempt in 0..8 {
        let handle = if attempt == 0 { base.clone() } else { format!("{}{}", base, 1000 + (u32::from_str_radix(&crate::util::token(2), 16).unwrap_or(0) % 9000)) };
        let r = sqlx::query_as::<_, Profile>(sqlx::AssertSqlSafe(format!(
            "INSERT INTO profiles (user_id, tenant, handle, display_name, lang, country, kyc_status, referral_code)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (user_id) DO NOTHING RETURNING {COLS}"
        )))
        .bind(c.id)
        .bind(&c.tenant)
        .bind(&handle)
        .bind(&display)
        .bind(&lang)
        .bind(c.country.clone().unwrap_or_default())
        .bind(c.kyc.clone().unwrap_or_else(|| "unverified".into()))
        .bind(&c.referral)
        .fetch_optional(&st.pool)
        .await;
        match r {
            Ok(Some(p)) => return Ok(p),
            // created concurrently by another request
            Ok(None) => return by_id(st, c.id).await?.ok_or(ApiError::NotFound),
            Err(sqlx::Error::Database(e)) if e.constraint() == Some("profiles_handle") => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(ApiError::Unavailable("Couldn't create your profile. Please try again.".into()))
}

/// Refreshes the engine / Academy badges at most every 10 minutes (best effort, in the background).
pub fn refresh_badges_later(st: &AppState, p: &Profile) {
    if p.badges_at.is_some_and(|t| Utc::now() - t < chrono::Duration::minutes(10)) {
        return;
    }
    let st = st.clone();
    let (id, tenant) = (p.user_id, p.tenant.clone());
    tokio::spawn(async move {
        if let Err(e) = refresh_badges(&st, id, &tenant).await {
            tracing::debug!(error = %e, user = id, "badge refresh failed");
        }
    });
}

pub async fn refresh_badges(st: &AppState, id: i64, tenant: &str) -> anyhow::Result<()> {
    // mark first so parallel requests don't all call the services
    sqlx::query("UPDATE profiles SET badges_at = now() WHERE user_id = $1").bind(id).execute(&st.pool).await?;
    let accounts = upstream::accounts(st, tenant, id).await;
    let master = upstream::master_me(st, tenant, id).await;
    let certs = upstream::certificates(st, tenant, id).await;
    if let Ok(acc) = &accounts {
        let live = acc.iter().any(|a| a["type"].as_str() == Some("live"));
        sqlx::query("UPDATE profiles SET live_trader = $2 WHERE user_id = $1").bind(id).bind(live).execute(&st.pool).await?;
        if live {
            crate::gamify::award(st, id, "live_trader").await?;
        }
    }
    let kyc: Option<String> = sqlx::query_scalar("SELECT kyc_status FROM profiles WHERE user_id = $1").bind(id).fetch_optional(&st.pool).await?;
    if kyc.as_deref() == Some("verified") {
        crate::gamify::award(st, id, "verified").await?;
    }
    if let Ok(m) = &master {
        let (mid, program) = match m {
            Some(m) if matches!(m["status"].as_str(), Some("approved")) => {
                let p = m["program"].as_str().unwrap_or("copy");
                let p = if p == "pamm" && m["fund"].is_null() { "copy" } else { p };
                (m["id"].as_i64(), Some(if ["copy", "pamm", "both"].contains(&p) { p.to_string() } else { "copy".into() }))
            }
            _ => (None, None),
        };
        sqlx::query("UPDATE profiles SET master_id = $2, master_program = $3 WHERE user_id = $1").bind(id).bind(mid).bind(program).execute(&st.pool).await?;
    }
    if let Ok(c) = &certs {
        sqlx::query("UPDATE profiles SET certificates = $2 WHERE user_id = $1").bind(id).bind(sqlx::types::Json(json!(c))).execute(&st.pool).await?;
        if !c.is_empty() {
            crate::gamify::award(st, id, "academy_certificate").await?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- relationships

#[derive(Default, Debug, Clone)]
pub struct Rel {
    pub following: bool,
    pub requested: bool,
    pub followed_by: bool,
    pub blocked: bool,
    pub blocked_by: bool,
    pub muted: bool,
    pub restricted: bool,
    pub close_friend: bool,
    pub bell: bool,
}

impl Rel {
    pub fn json(&self) -> Value {
        json!({
            "following": self.following, "requested": self.requested, "followedBy": self.followed_by, "blocked": self.blocked,
            "muted": self.muted, "restricted": self.restricted, "closeFriend": self.close_friend, "bell": self.bell,
        })
    }
}

pub async fn rel(st: &AppState, viewer: i64, target: i64) -> ApiResult<Rel> {
    if viewer == target {
        return Ok(Rel::default());
    }
    let r = sqlx::query(
        "SELECT
            (SELECT status FROM follows WHERE follower = $1 AND followee = $2) AS out_status,
            (SELECT bell FROM follows WHERE follower = $1 AND followee = $2) AS bell,
            EXISTS (SELECT 1 FROM follows WHERE follower = $2 AND followee = $1 AND status = 'active') AS followed_by,
            EXISTS (SELECT 1 FROM blocks WHERE blocker = $1 AND blocked = $2) AS blocked,
            EXISTS (SELECT 1 FROM blocks WHERE blocker = $2 AND blocked = $1) AS blocked_by,
            EXISTS (SELECT 1 FROM mutes WHERE muter = $1 AND muted = $2) AS muted,
            EXISTS (SELECT 1 FROM restricts WHERE restrictor = $1 AND restricted = $2) AS restricted,
            EXISTS (SELECT 1 FROM close_friends WHERE owner = $1 AND friend = $2) AS close_friend",
    )
    .bind(viewer)
    .bind(target)
    .fetch_one(&st.pool)
    .await?;
    let out: Option<String> = r.get("out_status");
    Ok(Rel {
        following: out.as_deref() == Some("active"),
        requested: out.as_deref() == Some("requested"),
        followed_by: r.get("followed_by"),
        blocked: r.get("blocked"),
        blocked_by: r.get("blocked_by"),
        muted: r.get("muted"),
        restricted: r.get("restricted"),
        close_friend: r.get("close_friend"),
        bell: r.get::<Option<bool>, _>("bell").unwrap_or(false),
    })
}

/// Whether two members blocked each other (either way).
pub async fn blocked_between(st: &AppState, a: i64, b: i64) -> ApiResult<bool> {
    Ok(sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM blocks WHERE (blocker = $1 AND blocked = $2) OR (blocker = $2 AND blocked = $1))").bind(a).bind(b).fetch_one(&st.pool).await?)
}

/// Members the viewer must not see: blocked either way. Muted members are hidden from feeds and story trays.
#[derive(Default, Clone)]
pub struct Excl {
    pub blocked: HashSet<i64>,
    pub muted_posts: HashSet<i64>,
    pub muted_stories: HashSet<i64>,
    pub hidden_words: Vec<String>,
}

pub async fn exclusions(st: &AppState, viewer: i64) -> ApiResult<Excl> {
    let blocked: Vec<i64> = sqlx::query_scalar("SELECT blocked FROM blocks WHERE blocker = $1 UNION SELECT blocker FROM blocks WHERE blocked = $1").bind(viewer).fetch_all(&st.pool).await?;
    let mutes = sqlx::query("SELECT muted, posts, stories FROM mutes WHERE muter = $1").bind(viewer).fetch_all(&st.pool).await?;
    let words: Vec<String> = sqlx::query_scalar("SELECT word FROM hidden_words WHERE user_id = $1").bind(viewer).fetch_all(&st.pool).await?;
    Ok(Excl {
        blocked: blocked.into_iter().collect(),
        muted_posts: mutes.iter().filter(|r| r.get::<bool, _>("posts")).map(|r| r.get("muted")).collect(),
        muted_stories: mutes.iter().filter(|r| r.get::<bool, _>("stories")).map(|r| r.get("muted")).collect(),
        hidden_words: words,
    })
}

/// Whether `viewer` may see `p`'s content: not blocked either way, not banned / shadow-hidden (except to
/// themselves), and following when the profile is private.
pub async fn can_view(st: &AppState, viewer: i64, p: &Profile) -> ApiResult<bool> {
    if viewer == p.user_id {
        return Ok(true);
    }
    if p.is_banned() || p.shadow_hidden {
        return Ok(false);
    }
    if blocked_between(st, viewer, p.user_id).await? {
        return Ok(false);
    }
    if p.private {
        return Ok(is_follower(st, viewer, p.user_id).await?);
    }
    Ok(true)
}

pub async fn is_follower(st: &AppState, follower: i64, followee: i64) -> ApiResult<bool> {
    Ok(sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM follows WHERE follower = $1 AND followee = $2 AND status = 'active')").bind(follower).bind(followee).fetch_one(&st.pool).await?)
}

/// Full profile view for `viewer`. Private profiles show the header only until the viewer follows.
pub async fn profile_json(st: &AppState, viewer: &crate::api::Me, p: &Profile) -> ApiResult<Value> {
    let me = viewer.id() == p.user_id;
    let r = rel(st, viewer.id(), p.user_id).await?;
    if r.blocked_by || (!me && (p.is_banned() || p.shadow_hidden)) {
        return Err(ApiError::NotFound);
    }
    let visible = me || (!r.blocked && (!p.private || r.following));
    let same_broker = viewer.p.tenant == p.tenant;
    let achievements: Vec<Value> = sqlx::query("SELECT key, awarded_at FROM achievements WHERE user_id = $1 ORDER BY awarded_at")
        .bind(p.user_id)
        .fetch_all(&st.pool)
        .await?
        .iter()
        .map(|r| json!({"key": r.get::<String, _>("key"), "awardedAt": r.get::<DateTime<Utc>, _>("awarded_at")}))
        .collect();
    let mut v = card(st, p);
    let o = v.as_object_mut().expect("card is an object");
    o.insert("bio".into(), json!(p.bio));
    o.insert("cover".into(), image_urls(st, &p.cover));
    o.insert("joinedAt".into(), json!(p.created_at));
    o.insert("counts".into(), json!({"followers": p.followers_count, "following": p.following_count, "posts": p.posts_count}));
    o.insert("level".into(), json!(p.level));
    o.insert("xp".into(), json!(p.xp));
    o.insert("levelProgress".into(), crate::gamify::progress(p.xp));
    o.insert("achievements".into(), json!(achievements));
    o.insert("certificates".into(), p.certificates.0.clone());
    o.insert("stats".into(), if (p.show_stats && visible) || me { stats_public(&p.stats.0, p.stats_at) } else { Value::Null });
    o.insert("showStats".into(), json!(p.show_stats));
    o.insert("relationship".into(), if me { Value::Null } else { r.json() });
    o.insert("isMe".into(), json!(me));
    o.insert("visible".into(), json!(visible));
    o.insert("pinnedPost".into(), json!(if visible { p.pinned_post } else { None }));
    // Copy button and IB link only between clients of the same broker (the broker itself is never shown)
    o.insert("copy".into(), if !me && same_broker && p.master_id.is_some() { json!({"masterId": p.master_id, "program": p.master_program}) } else { Value::Null });
    o.insert("ibLink".into(), if !me && same_broker && p.show_ib_link && p.referral_code.is_some() { json!({"referralCode": p.referral_code}) } else { Value::Null });
    if me {
        o.insert("settings".into(), json!({
            "showIbLink": p.show_ib_link, "commentsDefault": p.comments_default, "dmPolicy": p.dm_policy, "lang": p.lang, "onboarded": p.onboarded,
            "hasReferralCode": p.referral_code.is_some(), "feeDiscountEligible": p.fee_discount, "warnings": p.warnings,
        }));
    }
    Ok(v)
}

/// The figures shown on a profile (engine-verified, see stats.rs).
pub fn stats_public(s: &Value, at: Option<DateTime<Utc>>) -> Value {
    if s.as_object().is_none_or(|m| m.is_empty()) {
        return Value::Null;
    }
    json!({
        "winRate": s["winRate"], "monthlyReturnPct": s["monthlyReturnPct"], "maxDrawdownPct": s["maxDrawdownPct"], "trades": s["trades"],
        "returnPct90d": s["returnPct90d"], "riskAdjusted": s["riskAdjusted"], "monthly": s["monthly"], "computedAt": at, "verified": true,
    })
}
