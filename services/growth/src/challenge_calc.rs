//! Pure challenge maths (phase 4): prize-pool splits, contest windows from a template schedule, mission periods,
//! daily streaks with grace, levels from XP and auto-generated leaderboard nicknames. No I/O; unit-tested.
//!
//! Days, weeks and months follow the broker's server time (GMT+3, like the trading day), not UTC.

use crate::model::Prize;
use crate::money::{D, ZERO, r2};
use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc, Weekday};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Server time offset (GMT+3).
pub const SERVER_OFFSET_HOURS: i64 = 3;

/// The server-time calendar day of an instant.
pub fn server_day(t: DateTime<Utc>) -> NaiveDate {
    (t + Duration::hours(SERVER_OFFSET_HOURS)).date_naive()
}

/// 00:00 server time of a day, as UTC.
pub fn day_start(d: NaiveDate) -> DateTime<Utc> {
    Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap()) - Duration::hours(SERVER_OFFSET_HOURS)
}

// ---------------------------------------------------------------- prize-pool splits

/// One band of a split: ranks `from..=to` share `pct` % of the pool (each rank gets pct ÷ ranks).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Band {
    pub rank_from: i32,
    pub rank_to: i32,
    #[serde(with = "rust_decimal::serde::float")]
    pub pct: D,
}

/// Split presets offered by the wizard: (key, label, bands as (from, to, pct of the pool)).
pub const SPLIT_PRESETS: &[(&str, &str, &[(i32, i32, i64)])] = &[
    ("winner", "Winner takes all", &[(1, 1, 100)]),
    ("top3", "Top 3 · 50 / 30 / 20", &[(1, 1, 50), (2, 2, 30), (3, 3, 20)]),
    ("top5", "Top 5 · 40 / 25 / 15 / 12 / 8", &[(1, 1, 40), (2, 2, 25), (3, 3, 15), (4, 4, 12), (5, 5, 8)]),
    ("top10", "Top 10 ladder", &[(1, 1, 25), (2, 2, 18), (3, 3, 13), (4, 4, 10), (5, 5, 8), (6, 6, 7), (7, 7, 6), (8, 8, 5), (9, 10, 8)]),
    ("top20", "Top 20 · wide", &[(1, 1, 20), (2, 2, 14), (3, 3, 10), (4, 5, 14), (6, 10, 20), (11, 20, 22)]),
];

pub fn preset(key: &str) -> Option<Vec<Band>> {
    SPLIT_PRESETS.iter().find(|p| p.0 == key).map(|p| p.2.iter().map(|(f, t, pct)| Band { rank_from: *f, rank_to: *t, pct: D::from(*pct) }).collect())
}

pub fn presets_json() -> serde_json::Value {
    serde_json::Value::Array(
        SPLIT_PRESETS
            .iter()
            .map(|(k, l, b)| serde_json::json!({"key": k, "label": l, "bands": b.iter().map(|(f, t, p)| serde_json::json!({"rankFrom": f, "rankTo": t, "pct": p})).collect::<Vec<_>>()}))
            .collect(),
    )
}

/// Bands must cover ranks in order without overlap and add up to 100 %.
pub fn validate_bands(b: &[Band]) -> Result<(), String> {
    if b.is_empty() || b.len() > 50 {
        return Err("Between 1 and 50 bands.".into());
    }
    let mut last = 0;
    let mut total = ZERO;
    for x in b {
        if x.rank_from <= last || x.rank_to < x.rank_from || x.rank_to > 1000 {
            return Err("Bands must go down the table in order (rank from ≤ to, no overlap, ≤ 1000).".into());
        }
        if x.pct <= ZERO {
            return Err("Every band needs a share above 0 %.".into());
        }
        total += x.pct;
        last = x.rank_to;
    }
    if (total - D::from(100)).abs() > D::new(1, 2) {
        return Err(format!("The shares add up to {}%, not 100%.", total.normalize()));
    }
    Ok(())
}

/// Prize rows from a pool and a split. Each rank gets `pool × pct ÷ 100 ÷ ranks in the band`, rounded down to the
/// cent; the cents lost to rounding go to rank 1, so the rows always add up to the pool exactly.
pub fn split_pool(pool: D, bands: &[Band], payout: &str) -> Vec<Prize> {
    if pool <= ZERO {
        return vec![];
    }
    let mut rows: Vec<Prize> = bands
        .iter()
        .map(|b| {
            let n = D::from(b.rank_to - b.rank_from + 1);
            let each = (pool * b.pct / D::from(100) / n).trunc_with_scale(2);
            Prize { rank_from: b.rank_from, rank_to: b.rank_to, amount: each, payout: payout.to_string() }
        })
        .filter(|p| p.amount > ZERO)
        .collect();
    let total: D = rows.iter().map(|p| p.amount * D::from(p.rank_to - p.rank_from + 1)).sum();
    let rest = r2(pool - total);
    if rest > ZERO
        && let Some(first) = rows.first_mut()
    {
        if first.rank_from == first.rank_to {
            first.amount += rest;
        } else {
            // the first band shares several ranks: give rank 1 its own row with the rest
            let top = Prize { rank_from: first.rank_from, rank_to: first.rank_from, amount: first.amount + rest, payout: first.payout.clone() };
            first.rank_from += 1;
            rows.insert(0, top);
        }
    }
    rows
}

// ---------------------------------------------------------------- contest windows

/// The next window for a template schedule, from `now`: (start, end).
pub fn next_window(schedule: &str, duration_days: i64, now: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
    let today = server_day(now);
    let days = duration_days.clamp(1, 366);
    match schedule {
        "next_month" => {
            let (y, m) = if today.month() == 12 { (today.year() + 1, 1) } else { (today.year(), today.month() + 1) };
            let start = NaiveDate::from_ymd_opt(y, m, 1).unwrap();
            let (y2, m2) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
            (day_start(start), day_start(NaiveDate::from_ymd_opt(y2, m2, 1).unwrap()))
        }
        "next_weekend" => {
            let mut d = today + Duration::days(1);
            while d.weekday() != Weekday::Sat {
                d += Duration::days(1);
            }
            (day_start(d), day_start(d + Duration::days(2)))
        }
        "tomorrow" => {
            let d = today + Duration::days(1);
            (day_start(d), day_start(d + Duration::days(days)))
        }
        _ => {
            // next_monday
            let mut d = today + Duration::days(1);
            while d.weekday() != Weekday::Mon {
                d += Duration::days(1);
            }
            (day_start(d), day_start(d + Duration::days(days)))
        }
    }
}

/// The window of a clone: the same length, starting at the original's end (or the next whole day after now when
/// that is already past).
pub fn clone_window(starts: DateTime<Utc>, ends: DateTime<Utc>, now: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
    let len = ends - starts;
    let mut s = ends;
    if s <= now {
        s = day_start(server_day(now) + Duration::days(1));
    }
    (s, s + len)
}

// ---------------------------------------------------------------- mission periods

/// The period of a mission schedule containing `now`: (key, start, end). `once` = the whole mission life.
pub fn period(schedule: &str, now: DateTime<Utc>) -> (String, DateTime<Utc>, DateTime<Utc>) {
    let today = server_day(now);
    match schedule {
        "daily" => (today.format("%Y-%m-%d").to_string(), day_start(today), day_start(today + Duration::days(1))),
        "weekly" => {
            let monday = today - Duration::days(today.weekday().num_days_from_monday() as i64);
            let iso = today.iso_week();
            (format!("{}-W{:02}", iso.year(), iso.week()), day_start(monday), day_start(monday + Duration::days(7)))
        }
        // a range PostgreSQL can store (chrono's MIN / MAX are out of its timestamptz range)
        _ => ("once".into(), DateTime::<Utc>::UNIX_EPOCH, Utc.with_ymd_and_hms(9999, 12, 31, 0, 0, 0).unwrap()),
    }
}

// ---------------------------------------------------------------- streaks

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Streak {
    pub current: i32,
    pub best: i32,
    pub last_day: Option<NaiveDate>,
    pub grace_used: Option<NaiveDate>,
}

/// Days between `from` (exclusive) and `to` (exclusive) that count as missed: every day, or only weekdays when
/// weekends never break the streak (trading).
fn missed_days(from: NaiveDate, to: NaiveDate, skip_weekends: bool) -> i64 {
    let mut d = from + Duration::days(1);
    let mut n = 0;
    while d < to {
        if !(skip_weekends && matches!(d.weekday(), Weekday::Sat | Weekday::Sun)) {
            n += 1;
        }
        d += Duration::days(1);
    }
    n
}

/// A day of activity on a streak. Same day: unchanged. Next counted day: +1. Up to `grace_days` missed days are
/// forgiven when no grace was used in the last 7 days (the grace day is remembered); otherwise the streak restarts
/// at 1. A day before the last one (late data) changes nothing. Returns the new state and whether it moved.
pub fn advance_streak(s: &Streak, day: NaiveDate, grace_days: i64, skip_weekends: bool) -> (Streak, bool) {
    let mut n = s.clone();
    match s.last_day {
        Some(last) if day <= last => return (n, false),
        Some(last) => {
            let missed = missed_days(last, day, skip_weekends);
            let grace_ok = s.grace_used.is_none_or(|g| (day - g).num_days() >= 7);
            if missed == 0 {
                n.current += 1;
            } else if missed <= grace_days && grace_ok {
                n.current += 1;
                n.grace_used = Some(day);
            } else {
                n.current = 1;
            }
        }
        None => n.current = 1,
    }
    n.last_day = Some(day);
    n.best = n.best.max(n.current);
    (n, true)
}

/// The streak as shown today: still alive (today, the last counted day before it, or within an unused grace),
/// else broken (0).
pub fn streak_alive(s: &Streak, today: NaiveDate, grace_days: i64, skip_weekends: bool) -> i32 {
    let Some(last) = s.last_day else { return 0 };
    if last >= today {
        return s.current;
    }
    let missed = missed_days(last, today, skip_weekends);
    let grace_ok = s.grace_used.is_none_or(|g| (today - g).num_days() >= 7);
    if missed == 0 || (missed <= grace_days && grace_ok) { s.current } else { 0 }
}

// ---------------------------------------------------------------- levels

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Level {
    pub level: i32,
    pub name: String,
    pub min_xp: i64,
    #[serde(default = "bronze")]
    pub badge: String,
}

fn bronze() -> String {
    "bronze".into()
}

pub const BADGES: &[&str] = &["bronze", "silver", "gold", "platinum", "diamond", "crown", "rocket", "gem"];

pub fn default_levels() -> Vec<Level> {
    let l = |level, name: &str, min_xp, badge: &str| Level { level, name: name.into(), min_xp, badge: badge.into() };
    vec![
        l(1, "Rookie", 0, "bronze"),
        l(2, "Trader", 100, "bronze"),
        l(3, "Pro", 300, "silver"),
        l(4, "Expert", 700, "silver"),
        l(5, "Elite", 1_500, "gold"),
        l(6, "Master", 3_000, "platinum"),
        l(7, "Grandmaster", 6_000, "diamond"),
        l(8, "Legend", 12_000, "crown"),
    ]
}

pub fn validate_levels(levels: &[Level]) -> Result<(), String> {
    if levels.is_empty() || levels.len() > 100 {
        return Err("Between 1 and 100 levels.".into());
    }
    let mut v = levels.to_vec();
    v.sort_by_key(|l| l.level);
    if v[0].min_xp != 0 {
        return Err("Level 1 must start at 0 XP.".into());
    }
    for (i, l) in v.iter().enumerate() {
        if l.level != i as i32 + 1 {
            return Err("Levels must be 1, 2, 3 … without gaps.".into());
        }
        if l.name.trim().is_empty() || l.name.chars().count() > 40 {
            return Err("Every level needs a name (≤ 40 chars).".into());
        }
        if !BADGES.contains(&l.badge.as_str()) {
            return Err(format!("Badge must be one of {}.", BADGES.join(", ")));
        }
        if i > 0 && l.min_xp <= v[i - 1].min_xp {
            return Err("XP thresholds must increase with the level.".into());
        }
    }
    Ok(())
}

/// The level for an XP total (levels sorted by level) and the next one.
pub fn level_for(levels: &[Level], xp: i64) -> (Level, Option<Level>) {
    let mut cur = levels.first().cloned().unwrap_or(Level { level: 1, name: "Rookie".into(), min_xp: 0, badge: "bronze".into() });
    let mut next = None;
    for l in levels {
        if l.min_xp <= xp {
            cur = l.clone();
        } else {
            next = Some(l.clone());
            break;
        }
    }
    (cur, next)
}

/// Progress inside the current level, 0–100.
pub fn level_progress(cur: &Level, next: Option<&Level>, xp: i64) -> f64 {
    match next {
        Some(n) if n.min_xp > cur.min_xp => (((xp - cur.min_xp) as f64 / (n.min_xp - cur.min_xp) as f64) * 100.0).clamp(0.0, 100.0),
        _ => 100.0,
    }
}

// ---------------------------------------------------------------- nicknames

const ADJECTIVES: &[&str] = &[
    "Swift", "Bold", "Calm", "Sharp", "Lucky", "Silent", "Brave", "Rapid", "Steady", "Clever", "Golden", "Iron", "Bright", "Quiet", "Wild", "Prime", "Noble", "Keen", "Cosmic", "Amber",
];
const NOUNS: &[&str] = &["Falcon", "Tiger", "Wolf", "Hawk", "Lynx", "Orca", "Fox", "Raven", "Bison", "Cobra", "Eagle", "Panther", "Shark", "Stag", "Otter", "Mako", "Comet", "Atlas", "Ember", "Pulse"];

/// A deterministic nickname for a client ("SwiftFalcon482"): never derived from the name, email or login, and
/// stable, so a leaderboard can show it before the client ever opened the Client Area. `salt` > 0 picks another
/// one when a nickname is taken.
pub fn auto_nickname(tenant: &str, user_id: i64, salt: u32) -> String {
    let h = Sha256::digest(format!("kalks-nick:{tenant}:{user_id}:{salt}").as_bytes());
    let a = ADJECTIVES[h[0] as usize % ADJECTIVES.len()];
    let n = NOUNS[h[1] as usize % NOUNS.len()];
    let d = (u16::from_be_bytes([h[2], h[3]]) % 900) + 100;
    format!("{a}{n}{d}")
}

/// A client-chosen nickname: 3–20 letters, digits, `_`, `-` or `.`, starting with a letter or digit; no `@`
/// (emails) and not all digits (account numbers).
pub fn check_nickname(v: &str) -> Result<String, &'static str> {
    let n = v.trim();
    let len = n.chars().count();
    if !(3..=20).contains(&len) {
        return Err("Use 3–20 characters.");
    }
    if !n.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.')) {
        return Err("Use letters, digits, _ - or . only.");
    }
    if !n.chars().next().is_some_and(|c| c.is_alphanumeric()) {
        return Err("Start with a letter or digit.");
    }
    if n.chars().all(|c| c.is_ascii_digit()) {
        return Err("A nickname can't be only digits.");
    }
    const RESERVED: &[&str] = &["admin", "kalks", "support", "staff", "moderator", "system", "official"];
    let l = n.to_lowercase();
    if RESERVED.iter().any(|r| l.contains(r)) {
        return Err("That nickname is reserved.");
    }
    Ok(n.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::dec;

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }
    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn presets_add_up_and_split_exactly() {
        for (k, _, _) in SPLIT_PRESETS {
            validate_bands(&preset(k).unwrap()).unwrap_or_else(|e| panic!("{k}: {e}"));
        }
        let p = split_pool(dec("1000"), &preset("top3").unwrap(), "wallet");
        assert_eq!(p.iter().map(|x| x.amount).collect::<Vec<_>>(), vec![dec("500"), dec("300"), dec("200")]);
        // 9-10 share 8 % of 1000 = 40 each; total exactly the pool
        let p = split_pool(dec("1000"), &preset("top10").unwrap(), "credit");
        let total: D = p.iter().map(|x| x.amount * D::from(x.rank_to - x.rank_from + 1)).sum();
        assert_eq!(total, dec("1000"));
        assert_eq!(p.last().unwrap().amount, dec("40"));
        assert!(p.iter().all(|x| x.payout == "credit"));
        crate::model::validate_prizes(&p).unwrap();
        // rounding cents go to rank 1
        let p = split_pool(dec("100"), &[Band { rank_from: 1, rank_to: 3, pct: dec("100") }], "wallet");
        assert_eq!(p[0], Prize { rank_from: 1, rank_to: 1, amount: dec("33.34"), payout: "wallet".into() });
        assert_eq!(p[1], Prize { rank_from: 2, rank_to: 3, amount: dec("33.33"), payout: "wallet".into() });
        assert!(split_pool(ZERO, &preset("top3").unwrap(), "wallet").is_empty());
        assert!(validate_bands(&[Band { rank_from: 1, rank_to: 1, pct: dec("60") }]).is_err(), "must add up to 100");
        assert!(validate_bands(&[Band { rank_from: 2, rank_to: 2, pct: dec("50") }, Band { rank_from: 1, rank_to: 1, pct: dec("50") }]).is_err(), "in order");
    }

    #[test]
    fn template_windows_follow_server_time() {
        // Wednesday 2026-10-21 10:00 UTC (13:00 server time)
        let now = at("2026-10-21T10:00:00Z");
        let (s, e) = next_window("next_monday", 7, now);
        assert_eq!(s, at("2026-10-25T21:00:00Z"), "Monday 26 Oct 00:00 GMT+3");
        assert_eq!(e - s, Duration::days(7));
        let (s, e) = next_window("next_month", 0, now);
        assert_eq!((s, e), (at("2026-10-31T21:00:00Z"), at("2026-11-30T21:00:00Z")));
        let (s, e) = next_window("next_weekend", 0, now);
        assert_eq!((s, e), (at("2026-10-23T21:00:00Z"), at("2026-10-25T21:00:00Z")), "Saturday 00:00 to Monday 00:00");
        let (s, _) = next_window("tomorrow", 3, now);
        assert_eq!(s, at("2026-10-21T21:00:00Z"));
        // December rolls the year
        let (s, e) = next_window("next_month", 0, at("2026-12-10T00:00:00Z"));
        assert_eq!((s, e), (at("2026-12-31T21:00:00Z"), at("2027-01-31T21:00:00Z")));
        // 23:30 UTC Sunday is already Monday in server time: the next Monday is a week later
        let (s, _) = next_window("next_monday", 7, at("2026-10-25T22:30:00Z"));
        assert_eq!(s, at("2026-11-01T21:00:00Z"));
        // clones: same length, from the original end or from tomorrow
        let (s, e) = clone_window(at("2026-10-01T00:00:00Z"), at("2026-10-08T00:00:00Z"), at("2026-10-05T00:00:00Z"));
        assert_eq!((s, e), (at("2026-10-08T00:00:00Z"), at("2026-10-15T00:00:00Z")));
        let (s, e) = clone_window(at("2026-10-01T00:00:00Z"), at("2026-10-08T00:00:00Z"), at("2026-10-21T10:00:00Z"));
        assert_eq!((s, e - s), (at("2026-10-21T21:00:00Z"), Duration::days(7)));
    }

    #[test]
    fn mission_periods() {
        let now = at("2026-10-21T22:30:00Z"); // Thursday 01:30 server time
        let (k, s, e) = period("daily", now);
        assert_eq!((k.as_str(), s, e), ("2026-10-22", at("2026-10-21T21:00:00Z"), at("2026-10-22T21:00:00Z")));
        let (k, s, e) = period("weekly", now);
        assert_eq!((k.as_str(), s, e), ("2026-W43", at("2026-10-18T21:00:00Z"), at("2026-10-25T21:00:00Z")));
        assert_eq!(period("once", now).0, "once");
        // ISO week of 1 Jan 2027 (a Friday) belongs to 2026-W53
        assert_eq!(period("weekly", at("2027-01-01T12:00:00Z")).0, "2026-W53");
    }

    #[test]
    fn streaks_with_grace_and_weekends() {
        let s0 = Streak::default();
        let (s1, moved) = advance_streak(&s0, day("2026-10-19"), 1, false);
        assert!(moved);
        assert_eq!((s1.current, s1.best), (1, 1));
        let (same, moved) = advance_streak(&s1, day("2026-10-19"), 1, false);
        assert!(!moved);
        assert_eq!(same, s1, "same day: unchanged");
        let (s2, _) = advance_streak(&s1, day("2026-10-20"), 1, false);
        assert_eq!(s2.current, 2);
        // one missed day: grace keeps it, remembered
        let (s3, _) = advance_streak(&s2, day("2026-10-22"), 1, false);
        assert_eq!((s3.current, s3.grace_used), (3, Some(day("2026-10-22"))));
        // another miss within 7 days of the grace: restart
        let (s4, _) = advance_streak(&s3, day("2026-10-24"), 1, false);
        assert_eq!((s4.current, s4.best), (1, 3));
        // two missed days with 1 grace day: restart
        let (s5, _) = advance_streak(&s2, day("2026-10-23"), 1, false);
        assert_eq!(s5.current, 1);
        // no grace configured
        let (s6, _) = advance_streak(&s2, day("2026-10-22"), 0, false);
        assert_eq!(s6.current, 1);
        // late data never rewinds
        let (s7, moved) = advance_streak(&s2, day("2026-10-01"), 1, false);
        assert!(!moved);
        assert_eq!(s7, s2);
        // trading: Friday -> Monday is consecutive (weekend skipped), no grace used
        let fri = Streak { current: 4, best: 4, last_day: Some(day("2026-10-23")), grace_used: None };
        let (mon, _) = advance_streak(&fri, day("2026-10-26"), 0, true);
        assert_eq!((mon.current, mon.grace_used), (5, None));
        // display: alive on the next day, within grace, broken after
        assert_eq!(streak_alive(&s2, day("2026-10-21"), 1, false), 2);
        assert_eq!(streak_alive(&s2, day("2026-10-22"), 1, false), 2, "one missed day, grace available");
        assert_eq!(streak_alive(&s2, day("2026-10-23"), 1, false), 0);
        assert_eq!(streak_alive(&fri, day("2026-10-26"), 0, true), 4, "weekend doesn't break a trading streak");
        assert_eq!(streak_alive(&s0, day("2026-10-26"), 1, false), 0);
    }

    #[test]
    fn levels_from_xp() {
        let l = default_levels();
        validate_levels(&l).unwrap();
        assert_eq!(level_for(&l, 0).0.level, 1);
        assert_eq!(level_for(&l, 99).0.level, 1);
        let (cur, next) = level_for(&l, 100);
        assert_eq!((cur.level, next.as_ref().map(|n| n.level)), (2, Some(3)));
        assert_eq!(level_progress(&cur, next.as_ref(), 200), 50.0);
        let (top, none) = level_for(&l, 1_000_000);
        assert_eq!((top.level, none), (8, None));
        assert_eq!(level_progress(&top, None, 1_000_000), 100.0);
        let mut bad = default_levels();
        bad[3].min_xp = 50;
        assert!(validate_levels(&bad).is_err());
        let mut bad = default_levels();
        bad[0].badge = "emoji".into();
        assert!(validate_levels(&bad).is_err());
    }

    #[test]
    fn nicknames_are_stable_and_safe() {
        let a = auto_nickname("kalks", 42, 0);
        assert_eq!(a, auto_nickname("kalks", 42, 0));
        assert_ne!(a, auto_nickname("kalks", 42, 1));
        assert!(check_nickname(&a).is_ok());
        assert_eq!(check_nickname("  Pip_Hunter ").unwrap(), "Pip_Hunter");
        assert!(check_nickname("ab").is_err());
        assert!(check_nickname("me@mail.com").is_err());
        assert!(check_nickname("80412337").is_err(), "looks like a login");
        assert!(check_nickname("KalksAdmin").is_err());
        assert!(check_nickname("_under").is_err());
        assert!(check_nickname("نجم_التداول").is_ok(), "non-Latin letters are fine");
    }
}
