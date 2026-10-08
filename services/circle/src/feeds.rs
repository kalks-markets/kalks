//! Feeds and discovery (Q17–Q20, Q48): Following (newest first), For you (ranked), hashtag / cashtag / topic
//! feeds, Explore, search, leaderboards and trending tags.
//!
//! For you ranking, per candidate post of the last 72 hours:
//!   affinity (follow 1.0, followed-by-follows 0.35, + recent interactions)
//! + engagement (log of likes, votes, comments, reposts, quotes, saves)
//! + verified performance (the author's risk-adjusted return) + quality (media / trade card / substance)
//! + language match + featured / helpful,
//!   × freshness (18-hour half-life blend) × the KYC-verified boost,
//!   − spam / low quality (reports, warnings, empty posts), with already-seen posts pushed down and at most two
//!   posts of one author in a row. The ordered list is kept 10 minutes per member so paging is stable.

use crate::api::{Me, Page, paged};
use crate::error::{ApiError, ApiResult};
use crate::posts::{self, VISIBLE};
use crate::profiles;
use crate::state::AppState;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::Row;
use sqlx::postgres::PgRow;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// `published_at` (µs) + id cursor.
pub fn parse_cursor(c: &Option<String>) -> (DateTime<Utc>, i64) {
    c.as_deref()
        .and_then(|c| {
            let (t, id) = c.split_once(':')?;
            Some((DateTime::from_timestamp_micros(t.parse().ok()?)?, id.parse().ok()?))
        })
        .unwrap_or((DateTime::from_timestamp(253_402_300_799, 0).expect("year 9999"), i64::MAX))
}

pub fn cursor_of(v: &Value) -> Option<String> {
    let t = v["publishedAt"].as_str().and_then(|s| DateTime::parse_from_rfc3339(s).ok())?;
    Some(format!("{}:{}", t.timestamp_micros(), v["id"].as_i64()?))
}

async fn page_of(st: &AppState, me: &Me, rows: Vec<PgRow>, limit: i64, filter: bool) -> ApiResult<Value> {
    let more = rows.len() as i64 > limit;
    let mut rows = rows;
    rows.truncate(limit as usize);
    let last = rows.last().map(|r| format!("{}:{}", r.get::<Option<DateTime<Utc>>, _>("published_at").unwrap_or_else(Utc::now).timestamp_micros(), r.get::<i64, _>("id")));
    let rows = if filter { posts::feed_filter(rows, &profiles::exclusions(st, me.id()).await?, me.id()) } else { rows };
    let items = posts::render(st, me, &rows).await?;
    Ok(json!({"items": items, "nextCursor": if more { last } else { None }}))
}

pub async fn following(st: &AppState, me: &Me, page: &Page) -> ApiResult<Value> {
    let limit = page.limit(20, 50);
    let (at, id) = parse_cursor(&page.cursor);
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT p.* FROM posts p JOIN profiles a ON a.user_id = p.author
         WHERE p.status = 'published' AND (p.author = $1 OR p.author IN (SELECT followee FROM follows WHERE follower = $1 AND status = 'active'))
           AND {VISIBLE} AND (p.published_at, p.id) < ($2, $3)
         ORDER BY p.published_at DESC, p.id DESC LIMIT $4"
    )))
    .bind(me.id())
    .bind(at)
    .bind(id)
    .bind(limit + 1)
    .fetch_all(&st.pool)
    .await?;
    page_of(st, me, rows, limit, true).await
}

// ---------------------------------------------------------------- For you

#[derive(Clone, Debug, Default)]
pub struct Cand {
    pub id: i64,
    pub author: i64,
    pub age_hours: f64,
    pub engagement: f64,
    pub reports: i64,
    pub has_media: bool,
    pub has_card: bool,
    pub body_len: usize,
    pub lang: String,
    pub featured: bool,
    pub helpful: bool,
    pub author_verified: bool,
    pub author_risk: Option<f64>,
    pub author_warnings: i64,
}

#[derive(Default)]
pub struct Ctx {
    pub followed: HashSet<i64>,
    pub second: HashSet<i64>,
    pub interactions: HashMap<i64, i64>,
    pub seen: HashSet<i64>,
    pub lang: String,
    pub verified_boost: f64,
}

pub fn score(c: &Cand, x: &Ctx) -> f64 {
    let affinity = if x.followed.contains(&c.author) { 1.0 } else if x.second.contains(&c.author) { 0.35 } else { 0.0 } + (x.interactions.get(&c.author).copied().unwrap_or(0).min(10) as f64) * 0.05;
    let engagement = (1.0 + c.engagement.max(0.0)).ln();
    let perf = c.author_risk.map(|r| (r.clamp(-2.0, 5.0) + 2.0) / 7.0).unwrap_or(0.0) * 0.8;
    let quality = if c.has_media || c.has_card { 0.2 } else { 0.0 } + if c.body_len > 80 { 0.1 } else { 0.0 };
    let lang = if c.lang == x.lang { 0.4 } else { 0.0 };
    let featured = if c.featured { 2.0 } else { 0.0 } + if c.helpful { 0.5 } else { 0.0 };
    let fresh = (-c.age_hours.max(0.0) / 18.0).exp();
    let boost = if c.author_verified { x.verified_boost.max(1.0) } else { 1.0 };
    let low = c.body_len < 15 && !c.has_media && !c.has_card;
    let spam = c.reports as f64 * 0.3 + c.author_warnings as f64 * 0.2 + if low { 0.3 } else { 0.0 };
    let mut s = (1.2 * affinity + 0.6 * engagement + perf + quality + lang + featured) * (0.35 + 0.65 * fresh) * boost - spam;
    if x.seen.contains(&c.id) {
        s *= 0.15;
    }
    s
}

/// Ranks candidates, then spreads authors (at most two in a row).
pub fn rank(mut cands: Vec<Cand>, x: &Ctx) -> Vec<i64> {
    cands.sort_by(|a, b| score(b, x).partial_cmp(&score(a, x)).unwrap_or(std::cmp::Ordering::Equal).then(b.id.cmp(&a.id)));
    let mut out: Vec<Cand> = Vec::with_capacity(cands.len());
    let mut held: Vec<Cand> = Vec::new();
    for c in cands {
        let n = out.len();
        if n >= 2 && out[n - 1].author == c.author && out[n - 2].author == c.author {
            held.push(c);
            continue;
        }
        out.push(c);
        // place held posts as soon as they fit
        let mut i = 0;
        while i < held.len() {
            let n = out.len();
            if !(n >= 2 && out[n - 1].author == held[i].author && out[n - 2].author == held[i].author) {
                let h = held.remove(i);
                out.push(h);
            } else {
                i += 1;
            }
        }
    }
    out.extend(held);
    out.into_iter().map(|c| c.id).collect()
}

async fn for_you_ids(st: &AppState, me: &Me) -> ApiResult<Vec<i64>> {
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT p.id, p.author, p.published_at, p.likes + p.bulls + p.bears + 2 * p.comments + 3 * (p.reposts + p.quotes) + 2 * p.saves AS eng,
                p.reports, cardinality(p.media) > 0 AS has_media, p.trade_card IS NOT NULL AS has_card, length(p.body) AS body_len, p.lang,
                COALESCE(p.featured_until > now(), false) AS featured, p.helpful, a.kyc_status = 'verified' AS verified, a.risk_score, a.warnings, p.body
         FROM posts p JOIN profiles a ON a.user_id = p.author
         WHERE p.status = 'published' AND p.published_at > now() - interval '72 hours' AND p.author <> $1 AND p.kind <> 'repost' AND {VISIBLE}
         ORDER BY p.published_at DESC LIMIT 800"
    )))
    .bind(me.id())
    .fetch_all(&st.pool)
    .await?;
    let ex = profiles::exclusions(st, me.id()).await?;
    let followed: HashSet<i64> = sqlx::query_scalar("SELECT followee FROM follows WHERE follower = $1 AND status = 'active'").bind(me.id()).fetch_all(&st.pool).await?.into_iter().collect();
    let second: HashSet<i64> = sqlx::query_scalar(
        "SELECT DISTINCT f2.followee FROM follows f1 JOIN follows f2 ON f2.follower = f1.followee AND f2.status = 'active' WHERE f1.follower = $1 AND f1.status = 'active' LIMIT 5000",
    )
    .bind(me.id())
    .fetch_all(&st.pool)
    .await?
    .into_iter()
    .collect();
    let interactions: HashMap<i64, i64> = sqlx::query("SELECT p.author, count(*) AS n FROM reactions r JOIN posts p ON p.id = r.post_id WHERE r.user_id = $1 AND r.created_at > now() - interval '30 days' GROUP BY p.author")
        .bind(me.id())
        .fetch_all(&st.pool)
        .await?
        .iter()
        .map(|r| (r.get("author"), r.get("n")))
        .collect();
    let seen: HashSet<i64> = sqlx::query_scalar("SELECT post_id FROM seen WHERE user_id = $1 AND at > now() - interval '3 days'").bind(me.id()).fetch_all(&st.pool).await?.into_iter().collect();
    let s = profiles::settings(st).await;
    let now = Utc::now();
    let cands: Vec<Cand> = rows
        .iter()
        .filter(|r| {
            let a: i64 = r.get("author");
            !ex.muted_posts.contains(&a) && !ex.blocked.contains(&a) && !crate::text::has_hidden_word(&r.get::<String, _>("body"), &ex.hidden_words)
        })
        .map(|r| Cand {
            id: r.get("id"),
            author: r.get("author"),
            age_hours: (now - r.get::<DateTime<Utc>, _>("published_at")).num_seconds() as f64 / 3600.0,
            engagement: r.get::<i64, _>("eng") as f64,
            reports: r.get::<i32, _>("reports") as i64,
            has_media: r.get("has_media"),
            has_card: r.get("has_card"),
            body_len: r.get::<i32, _>("body_len").max(0) as usize,
            lang: r.get("lang"),
            featured: r.get("featured"),
            helpful: r.get("helpful"),
            author_verified: r.get("verified"),
            author_risk: r.get("risk_score"),
            author_warnings: r.get::<i32, _>("warnings") as i64,
        })
        .collect();
    let ctx = Ctx { followed, second, interactions, seen, lang: me.p.lang.clone(), verified_boost: s.verified_boost };
    Ok(rank(cands, &ctx))
}

pub async fn for_you(st: &AppState, me: &Me, page: &Page) -> ApiResult<Value> {
    let limit = page.limit(20, 50) as usize;
    let key = format!("fy:{}", me.id());
    let offset = page.cursor.as_deref().and_then(|c| c.strip_prefix("fy:")).and_then(|c| c.parse::<usize>().ok());
    let ids: Vec<i64> = match (offset, st.cache.get(&key, Duration::from_secs(600))) {
        (Some(_), Some(v)) => serde_json::from_value(v).unwrap_or_default(),
        _ => {
            let ids = for_you_ids(st, me).await?;
            st.cache.put(&key, json!(ids));
            ids
        }
    };
    let offset = offset.unwrap_or(0);
    let slice: Vec<i64> = ids.iter().skip(offset).take(limit).copied().collect();
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT p.* FROM posts p JOIN profiles a ON a.user_id = p.author WHERE p.id = ANY($2) AND {VISIBLE}")))
        .bind(me.id())
        .bind(&slice)
        .fetch_all(&st.pool)
        .await?;
    let mut by_id: HashMap<i64, PgRow> = rows.into_iter().map(|r| (r.get::<i64, _>("id"), r)).collect();
    let ordered: Vec<PgRow> = slice.iter().filter_map(|i| by_id.remove(i)).collect();
    let items = posts::render(st, me, &ordered).await?;
    let next = (offset + limit < ids.len()).then(|| format!("fy:{}", offset + limit));
    // nothing ranked yet (new member, quiet day): fall back to the latest public posts
    if ids.is_empty() && offset == 0 {
        return latest(st, me, page).await;
    }
    Ok(json!({"items": items, "nextCursor": next}))
}

async fn latest(st: &AppState, me: &Me, page: &Page) -> ApiResult<Value> {
    let limit = page.limit(20, 50);
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT p.* FROM posts p JOIN profiles a ON a.user_id = p.author WHERE p.status = 'published' AND p.kind <> 'repost' AND {VISIBLE} ORDER BY p.published_at DESC, p.id DESC LIMIT $2"
    )))
    .bind(me.id())
    .bind(limit)
    .fetch_all(&st.pool)
    .await?;
    let rows = posts::feed_filter(rows, &profiles::exclusions(st, me.id()).await?, me.id());
    Ok(json!({"items": posts::render(st, me, &rows).await?, "nextCursor": null}))
}

// ---------------------------------------------------------------- tag / symbol / topic / profile feeds

pub async fn hashtag(st: &AppState, me: &Me, tag: &str, page: &Page) -> ApiResult<Value> {
    let tag = tag.trim().trim_start_matches('#').to_lowercase();
    if tag.is_empty() || tag.chars().count() > 50 {
        return Err(ApiError::NotFound);
    }
    let limit = page.limit(20, 50);
    let (at, id) = parse_cursor(&page.cursor);
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT p.* FROM posts p JOIN profiles a ON a.user_id = p.author WHERE p.status = 'published' AND $2 = ANY(p.hashtags) AND {VISIBLE}
           AND (p.published_at, p.id) < ($3, $4) ORDER BY p.published_at DESC, p.id DESC LIMIT $5"
    )))
    .bind(me.id())
    .bind(&tag)
    .bind(at)
    .bind(id)
    .bind(limit + 1)
    .fetch_all(&st.pool)
    .await?;
    let mut v = page_of(st, me, rows, limit, true).await?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM posts WHERE status = 'published' AND $1 = ANY(hashtags)").bind(&tag).fetch_one(&st.pool).await?;
    v["tag"] = json!({"tag": tag, "posts": count});
    Ok(v)
}

/// Crowd sentiment of a symbol: bull / bear votes on its posts of the last 24 hours.
pub async fn crowd(st: &AppState, symbol: &str) -> ApiResult<(i64, i64)> {
    let r = sqlx::query("SELECT COALESCE(sum(bulls),0)::bigint AS b, COALESCE(sum(bears),0)::bigint AS s FROM posts WHERE status = 'published' AND $1 = ANY(cashtags) AND published_at > now() - interval '24 hours'")
        .bind(symbol)
        .fetch_one(&st.pool)
        .await?;
    Ok((r.get("b"), r.get("s")))
}

pub async fn cashtag(st: &AppState, me: &Me, symbol: &str, page: &Page) -> ApiResult<Value> {
    let symbol = symbol.trim().trim_start_matches('$').to_uppercase();
    if !crate::upstream::is_symbol(&symbol) {
        return Err(ApiError::NotFound);
    }
    let limit = page.limit(20, 50);
    let (at, id) = parse_cursor(&page.cursor);
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT p.* FROM posts p JOIN profiles a ON a.user_id = p.author WHERE p.status = 'published' AND $2 = ANY(p.cashtags) AND {VISIBLE}
           AND (p.published_at, p.id) < ($3, $4) ORDER BY p.published_at DESC, p.id DESC LIMIT $5"
    )))
    .bind(me.id())
    .bind(&symbol)
    .bind(at)
    .bind(id)
    .bind(limit + 1)
    .fetch_all(&st.pool)
    .await?;
    let mut v = page_of(st, me, rows, limit, true).await?;
    if page.cursor.is_none() {
        let quotes = crate::upstream::quotes(st, std::slice::from_ref(&symbol)).await;
        let names = crate::upstream::instruments(st).await;
        let (bulls, bears) = crowd(st, &symbol).await?;
        let room: Option<i64> = sqlx::query_scalar("SELECT id FROM conversations WHERE kind = 'room' AND symbol = $1 AND status = 'active'").bind(&symbol).fetch_optional(&st.pool).await?;
        v["symbol"] = json!({
            "symbol": symbol, "name": names.get(&symbol), "quote": quotes.get(&symbol), "known": names.is_empty() || names.contains_key(&symbol),
            "crowd": {"bulls": bulls, "bears": bears, "bullPct": if bulls + bears > 0 { json!((bulls as f64 * 1000.0 / (bulls + bears) as f64).round() / 10.0) } else { Value::Null }},
            "roomId": room, "riskLine": crate::text::RISK_LINE,
        });
    }
    Ok(v)
}

pub async fn topic_videos(st: &AppState, me: &Me, key: &str, sort: &str, page: &Page) -> ApiResult<Value> {
    let topic = sqlx::query("SELECT key, title, description FROM topics WHERE key = $1 AND active").bind(key).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    let limit = page.limit(20, 50);
    let rows = if sort == "top" {
        let offset = page.cursor.as_deref().and_then(|c| c.parse::<i64>().ok()).unwrap_or(0).max(0);
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT p.* FROM posts p JOIN profiles a ON a.user_id = p.author WHERE p.status = 'published' AND p.topic = $2 AND p.kind = 'video' AND {VISIBLE}
             ORDER BY (p.likes + 2 * p.comments + 3 * p.reposts + 2 * p.saves + p.views / 20) DESC, p.id DESC OFFSET $3 LIMIT $4"
        )))
        .bind(me.id())
        .bind(key)
        .bind(offset)
        .bind(limit + 1)
        .fetch_all(&st.pool)
        .await?;
        let more = rows.len() as i64 > limit;
        let mut rows = rows;
        rows.truncate(limit as usize);
        let rows = posts::feed_filter(rows, &profiles::exclusions(st, me.id()).await?, me.id());
        let items = posts::render(st, me, &rows).await?;
        return Ok(json!({"topic": {"key": topic.get::<String, _>("key"), "title": topic.get::<String, _>("title"), "description": topic.get::<String, _>("description")}, "items": items, "nextCursor": if more { Some((offset + limit).to_string()) } else { None }}));
    } else {
        let (at, id) = parse_cursor(&page.cursor);
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT p.* FROM posts p JOIN profiles a ON a.user_id = p.author WHERE p.status = 'published' AND p.topic = $2 AND p.kind = 'video' AND {VISIBLE}
               AND (p.published_at, p.id) < ($3, $4) ORDER BY p.published_at DESC, p.id DESC LIMIT $5"
        )))
        .bind(me.id())
        .bind(key)
        .bind(at)
        .bind(id)
        .bind(limit + 1)
        .fetch_all(&st.pool)
        .await?
    };
    let mut v = page_of(st, me, rows, limit, true).await?;
    v["topic"] = json!({"key": topic.get::<String, _>("key"), "title": topic.get::<String, _>("title"), "description": topic.get::<String, _>("description")});
    Ok(v)
}

/// A member's posts: tab `posts` (default, pinned first), `media`, `trades`, `videos`, `reposts`.
pub async fn profile_posts(st: &AppState, me: &Me, handle: &str, tab: &str, page: &Page) -> ApiResult<Value> {
    let p = profiles::by_handle(st, handle).await?;
    if !profiles::can_view(st, me.id(), &p).await? {
        return Err(crate::error::denied("private_profile", "This profile is private. Follow to see their posts."));
    }
    let own = p.user_id == me.id();
    let cond = match tab {
        "media" => "cardinality(p.media) > 0 AND p.kind <> 'video'",
        "trades" => "p.trade_card IS NOT NULL",
        "videos" => "p.kind = 'video'",
        "reposts" => "p.kind = 'repost'",
        _ => "true",
    };
    let status = if own { "p.status IN ('published','pending','review','rejected')" } else { "p.status = 'published'" };
    let limit = page.limit(20, 50);
    let before = page.before_id();
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT p.* FROM posts p JOIN profiles a ON a.user_id = p.author WHERE p.author = $2 AND {status} AND ({cond}) AND {VISIBLE} AND p.id < $3
         ORDER BY p.id DESC LIMIT $4"
    )))
    .bind(me.id())
    .bind(p.user_id)
    .bind(before)
    .bind(limit + 1)
    .fetch_all(&st.pool)
    .await?;
    let items = posts::render(st, me, &rows).await?;
    let mut v = paged(items, limit, |x| x["id"].as_i64().map(|i| i.to_string()));
    if (tab.is_empty() || tab == "posts") && page.cursor.is_none() {
        if let Some(pin) = p.pinned_post {
            let pr = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT p.* FROM posts p JOIN profiles a ON a.user_id = p.author WHERE p.id = $2 AND p.status = 'published' AND {VISIBLE}"))).bind(me.id()).bind(pin).fetch_all(&st.pool).await?;
            v["pinned"] = posts::render(st, me, &pr).await?.into_iter().next().unwrap_or(Value::Null);
        } else {
            v["pinned"] = Value::Null;
        }
    }
    Ok(v)
}

pub async fn chapter_thread(st: &AppState, me: &Me, chapter: &str, page: &Page) -> ApiResult<Value> {
    let limit = page.limit(20, 50);
    let (at, id) = parse_cursor(&page.cursor);
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT p.* FROM posts p JOIN profiles a ON a.user_id = p.author WHERE p.status = 'published' AND p.academy_chapter = $2 AND {VISIBLE}
           AND (p.published_at, p.id) < ($3, $4) ORDER BY (a.staff_badge IS NOT NULL) DESC, p.published_at DESC, p.id DESC LIMIT $5"
    )))
    .bind(me.id())
    .bind(chapter)
    .bind(at)
    .bind(id)
    .bind(limit + 1)
    .fetch_all(&st.pool)
    .await?;
    let mut v = page_of(st, me, rows, limit, true).await?;
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM posts WHERE status = 'published' AND academy_chapter = $1").bind(chapter).fetch_one(&st.pool).await?;
    v["chapter"] = json!({"chapter": chapter, "posts": n});
    Ok(v)
}

// ---------------------------------------------------------------- Explore, search, leaderboards

/// Recomputes trending hashtags and cashtags (posts of the last 24 hours, decayed engagement).
pub async fn compute_trending(st: &AppState) -> anyhow::Result<()> {
    let mut tx = st.pool.begin().await?;
    sqlx::query("DELETE FROM trending").execute(&mut *tx).await?;
    for (kind, col) in [("hashtag", "hashtags"), ("cashtag", "cashtags")] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO trending (kind, tag, posts, score)
             SELECT '{kind}', t, count(*), sum(exp(-extract(epoch FROM (now() - p.published_at)) / 43200.0)
                    * (1 + ln(1 + p.likes + p.bulls + p.bears + 2 * p.comments + 3 * (p.reposts + p.quotes))))
             FROM posts p JOIN profiles a ON a.user_id = p.author, unnest(p.{col}) AS t
             WHERE p.status = 'published' AND p.published_at > now() - interval '24 hours' AND NOT p.shadow AND NOT a.shadow_hidden
             GROUP BY t ORDER BY 4 DESC LIMIT 50"
        )))
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn trending(st: &AppState, kind: &str, n: i64) -> ApiResult<Vec<Value>> {
    let rows = sqlx::query("SELECT tag, posts, score FROM trending WHERE kind = $1 ORDER BY score DESC LIMIT $2").bind(kind).bind(n).fetch_all(&st.pool).await?;
    let mut out: Vec<Value> = rows.iter().map(|r| json!({"tag": r.get::<String, _>("tag"), "posts": r.get::<i64, _>("posts"), "score": (r.get::<f64, _>("score") * 100.0).round() / 100.0})).collect();
    if kind == "cashtag" {
        let syms: Vec<String> = out.iter().filter_map(|v| v["tag"].as_str().map(str::to_string)).collect();
        let q = crate::upstream::quotes(st, &syms).await;
        for v in &mut out {
            let s = v["tag"].as_str().unwrap_or("").to_string();
            v["quote"] = q.get(&s).cloned().unwrap_or(Value::Null);
        }
    }
    Ok(out)
}

/// Public, active, not blocked: `$1` = viewer.
const LISTABLE: &str = "a.status = 'active' AND NOT a.shadow_hidden AND a.user_id <> $1
    AND NOT EXISTS (SELECT 1 FROM blocks b WHERE (b.blocker = $1 AND b.blocked = a.user_id) OR (b.blocker = a.user_id AND b.blocked = $1))";

pub async fn traders(st: &AppState, viewer: i64, n: i64) -> ApiResult<Vec<Value>> {
    let s = profiles::settings(st).await;
    let rows = sqlx::query_as::<_, profiles::Profile>(sqlx::AssertSqlSafe(format!(
        "SELECT {} FROM profiles a WHERE a.show_stats AND a.risk_score IS NOT NULL AND NOT a.private AND {LISTABLE}
         ORDER BY (EXISTS (SELECT 1 FROM features f WHERE f.kind = 'profile' AND f.target = a.user_id AND (f.until IS NULL OR f.until > now()))) DESC,
                  (a.risk_score * 0.7 + ln(1 + a.followers_count) * 0.3) * (CASE WHEN a.kyc_status = 'verified' THEN $3 ELSE 1 END) DESC
         LIMIT $2",
        profiles::cols("a")
    )))
    .bind(viewer)
    .bind(n)
    .bind(s.verified_boost)
    .fetch_all(&st.pool)
    .await?;
    Ok(rows.iter().map(|p| json!({"profile": profiles::card(st, p), "stats": profiles::stats_public(&p.stats.0, p.stats_at), "followers": p.followers_count})).collect())
}

pub async fn creators(st: &AppState, viewer: i64, n: i64) -> ApiResult<Vec<Value>> {
    let ids: Vec<i64> = sqlx::query_scalar(crate::rewards::TOP_CREATORS).bind(n * 2).fetch_all(&st.pool).await?;
    let featured: Vec<i64> = sqlx::query_scalar("SELECT target FROM features WHERE kind = 'profile' AND (until IS NULL OR until > now())").fetch_all(&st.pool).await?;
    let ex = profiles::exclusions(st, viewer).await?;
    let mut order: Vec<i64> = featured.iter().chain(ids.iter()).copied().filter(|i| *i != viewer && !ex.blocked.contains(i)).collect();
    let mut seen = HashSet::new();
    order.retain(|i| seen.insert(*i));
    let profs = profiles::many(st, &order).await?;
    Ok(order
        .iter()
        .filter_map(|i| profs.get(i))
        .filter(|p| p.status == "active" && !p.shadow_hidden)
        .take(n as usize)
        .map(|p| json!({"profile": profiles::card(st, p), "followers": p.followers_count, "featured": featured.contains(&p.user_id)}))
        .collect())
}

pub async fn explore(st: &AppState, me: &Me) -> ApiResult<Value> {
    let featured_posts = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT p.* FROM posts p JOIN profiles a ON a.user_id = p.author WHERE p.status = 'published' AND p.featured_until > now() AND {VISIBLE} ORDER BY p.featured_until DESC LIMIT 10"
    )))
    .bind(me.id())
    .fetch_all(&st.pool)
    .await?;
    let topics = sqlx::query(
        "SELECT t.key, t.title, t.description, (SELECT count(*) FROM posts p WHERE p.topic = t.key AND p.status = 'published' AND p.kind = 'video') AS videos,
                (SELECT max(id) FROM posts p WHERE p.topic = t.key AND p.status = 'published' AND p.kind = 'video') AS latest
         FROM topics t WHERE t.active ORDER BY t.position, t.key",
    )
    .fetch_all(&st.pool)
    .await?;
    let latest_ids: Vec<i64> = topics.iter().filter_map(|t| t.get::<Option<i64>, _>("latest")).collect();
    let latest_rows = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT p.* FROM posts p JOIN profiles a ON a.user_id = p.author WHERE p.id = ANY($2) AND {VISIBLE}"))).bind(me.id()).bind(&latest_ids).fetch_all(&st.pool).await?;
    let latest: HashMap<i64, Value> = posts::render(st, me, &latest_rows).await?.into_iter().map(|v| (v["id"].as_i64().unwrap_or(0), v)).collect();
    let suggested = sqlx::query_as::<_, profiles::Profile>(sqlx::AssertSqlSafe(format!(
        "SELECT {} FROM profiles a WHERE a.user_id IN (
            SELECT f2.followee FROM follows f1 JOIN follows f2 ON f2.follower = f1.followee AND f2.status = 'active'
            WHERE f1.follower = $1 AND f1.status = 'active')
           AND a.user_id NOT IN (SELECT followee FROM follows WHERE follower = $1) AND {LISTABLE}
         ORDER BY a.followers_count DESC LIMIT 10",
        profiles::cols("a")
    )))
    .bind(me.id())
    .fetch_all(&st.pool)
    .await?;
    Ok(json!({
        "announcements": announcements(st).await?,
        "featuredPosts": posts::render(st, me, &featured_posts).await?,
        "traders": traders(st, me.id(), 10).await?,
        "creators": creators(st, me.id(), 10).await?,
        "trendingHashtags": trending(st, "hashtag", 10).await?,
        "trendingCashtags": trending(st, "cashtag", 10).await?,
        "topics": topics.iter().map(|t| json!({
            "key": t.get::<String, _>("key"), "title": t.get::<String, _>("title"), "description": t.get::<String, _>("description"), "videos": t.get::<i64, _>("videos"),
            "latest": t.get::<Option<i64>, _>("latest").and_then(|l| latest.get(&l).cloned()),
        })).collect::<Vec<_>>(),
        "suggested": suggested.iter().map(|p| profiles::card(st, p)).collect::<Vec<_>>(),
    }))
}

pub async fn announcements(st: &AppState) -> ApiResult<Vec<Value>> {
    Ok(sqlx::query("SELECT * FROM announcements WHERE starts_at <= now() AND (ends_at IS NULL OR ends_at > now()) ORDER BY pinned DESC, starts_at DESC LIMIT 5")
        .fetch_all(&st.pool)
        .await?
        .iter()
        .map(|r| json!({"id": r.get::<i64, _>("id"), "title": r.get::<String, _>("title"), "body": r.get::<String, _>("body"), "link": r.get::<Option<String>, _>("link"), "pinned": r.get::<bool, _>("pinned"), "startsAt": r.get::<DateTime<Utc>, _>("starts_at")}))
        .collect())
}

fn like_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

pub async fn search(st: &AppState, me: &Me, q: &str, kind: &str) -> ApiResult<Value> {
    let q = crate::util::clean(q, 60);
    if q.chars().count() < 1 {
        return Ok(json!({"users": [], "hashtags": [], "cashtags": [], "posts": []}));
    }
    let mut out = json!({"users": [], "hashtags": [], "cashtags": [], "posts": []});
    let bare = q.trim_start_matches(['@', '#', '$']).to_lowercase();
    if kind == "all" || kind == "users" {
        let users = sqlx::query_as::<_, profiles::Profile>(sqlx::AssertSqlSafe(format!(
            "SELECT {} FROM profiles a WHERE (a.handle LIKE $2 || '%' OR lower(a.display_name) LIKE $2 || '%') AND {LISTABLE}
             ORDER BY (a.handle = $3) DESC, a.followers_count DESC LIMIT 20",
            profiles::cols("a")
        )))
        .bind(me.id())
        .bind(like_escape(&bare))
        .bind(&bare)
        .fetch_all(&st.pool)
        .await?;
        out["users"] = json!(users.iter().map(|p| profiles::card(st, p)).collect::<Vec<_>>());
    }
    if kind == "all" || kind == "hashtags" {
        let tags: Vec<(String, i64)> = sqlx::query_as(
            "SELECT t, count(*) FROM posts p, unnest(p.hashtags) AS t WHERE p.status = 'published' AND p.published_at > now() - interval '90 days' AND t LIKE $1 || '%'
             GROUP BY t ORDER BY count(*) DESC LIMIT 20",
        )
        .bind(like_escape(&bare))
        .fetch_all(&st.pool)
        .await?;
        out["hashtags"] = json!(tags.iter().map(|(t, n)| json!({"tag": t, "posts": n})).collect::<Vec<_>>());
    }
    if kind == "all" || kind == "cashtags" {
        let up = bare.to_uppercase();
        let names = crate::upstream::instruments(st).await;
        let mut syms: Vec<(String, String)> = names.iter().filter(|(s, n)| s.starts_with(&up) || n.to_uppercase().contains(&up)).map(|(s, n)| (s.clone(), n.clone())).collect();
        syms.sort_by(|a, b| (!a.0.starts_with(&up)).cmp(&!b.0.starts_with(&up)).then(a.0.cmp(&b.0)));
        syms.truncate(20);
        if syms.is_empty() && crate::upstream::is_symbol(&up) {
            syms.push((up.clone(), String::new()));
        }
        out["cashtags"] = json!(syms.iter().map(|(s, n)| json!({"symbol": s, "name": n})).collect::<Vec<_>>());
    }
    if (kind == "all" || kind == "posts") && q.chars().count() >= 2 {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT p.* FROM posts p JOIN profiles a ON a.user_id = p.author WHERE p.status = 'published' AND p.published_at > now() - interval '90 days'
               AND p.body ILIKE '%' || $2 || '%' AND {VISIBLE} ORDER BY p.published_at DESC LIMIT 20"
        )))
        .bind(me.id())
        .bind(like_escape(&q))
        .fetch_all(&st.pool)
        .await?;
        let rows = posts::feed_filter(rows, &profiles::exclusions(st, me.id()).await?, me.id());
        out["posts"] = json!(posts::render(st, me, &rows).await?);
    }
    Ok(out)
}

pub async fn leaderboard(st: &AppState, me: &Me, board: &str) -> ApiResult<Value> {
    match board {
        "traders" => {
            let items: Vec<Value> = traders(st, me.id(), 50).await?.into_iter().enumerate().map(|(i, mut v)| {
                v["rank"] = json!(i + 1);
                v
            }).collect();
            Ok(json!({"board": "traders", "metric": "riskAdjusted", "items": items, "note": "Engine-verified figures of members who share their stats. Past performance is not a guide to future results."}))
        }
        "creators" => {
            let items: Vec<Value> = creators(st, me.id(), 50).await?.into_iter().enumerate().map(|(i, mut v)| {
                v["rank"] = json!(i + 1);
                v
            }).collect();
            Ok(json!({"board": "creators", "metric": "engagement30d", "items": items}))
        }
        _ => Err(ApiError::NotFound),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(id: i64, author: i64, age: f64, eng: f64) -> Cand {
        Cand { id, author, age_hours: age, engagement: eng, body_len: 120, lang: "en".into(), ..Default::default() }
    }

    #[test]
    fn ranks_follows_engagement_freshness_and_spreads_authors() {
        let mut x = Ctx { lang: "en".into(), verified_boost: 1.25, ..Default::default() };
        x.followed.insert(1);
        // followed author beats a stranger with the same post
        assert!(score(&c(1, 1, 1.0, 5.0), &x) > score(&c(2, 2, 1.0, 5.0), &x));
        // engagement and freshness count
        assert!(score(&c(3, 2, 1.0, 200.0), &x) > score(&c(4, 2, 1.0, 0.0), &x));
        assert!(score(&c(5, 2, 1.0, 10.0), &x) > score(&c(6, 2, 48.0, 10.0), &x));
        // verified performance and KYC boost
        let mut good = c(7, 3, 2.0, 10.0);
        good.author_risk = Some(3.0);
        good.author_verified = true;
        assert!(score(&good, &x) > score(&c(8, 4, 2.0, 10.0), &x));
        // spam demotion and seen penalty
        let mut spam = c(9, 5, 1.0, 10.0);
        spam.reports = 5;
        assert!(score(&spam, &x) < score(&c(10, 6, 1.0, 10.0), &x));
        x.seen.insert(11);
        assert!(score(&c(11, 6, 1.0, 10.0), &x) < score(&c(12, 6, 1.0, 10.0), &x));
        // language preference
        let mut de = c(13, 7, 1.0, 10.0);
        de.lang = "de".into();
        assert!(score(&de, &x) < score(&c(14, 7, 1.0, 10.0), &x));
        // one author can't fill the feed: at most two in a row
        let many: Vec<Cand> = (0..5).map(|i| c(100 + i, 1, 1.0, 1000.0)).chain([c(200, 2, 1.0, 1.0), c(201, 3, 1.0, 1.0)]).collect();
        let ids = rank(many, &x);
        assert_eq!(ids, vec![104, 103, 201, 102, 101, 200, 100]);
    }

    #[test]
    fn cursors() {
        let v = json!({"id": 42, "publishedAt": "2026-10-09T10:00:00Z"});
        let c = cursor_of(&v).unwrap();
        let (t, id) = parse_cursor(&Some(c));
        assert_eq!((t.to_rfc3339(), id), ("2026-10-09T10:00:00+00:00".to_string(), 42));
        assert_eq!(parse_cursor(&None).1, i64::MAX);
        assert_eq!(parse_cursor(&Some("junk".into())).1, i64::MAX);
    }
}
