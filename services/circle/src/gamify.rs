//! Trader levels (XP) and achievement badges (Q43).
//!
//! XP: published post +5, trade card shared +10, like / vote received +1, comment received +2, new follower +3,
//! helpful post +50. Level = ⌊√(XP / 25)⌋ + 1 (max 50): level 2 at 25 XP, 5 at 400, 10 at 2 025.

use crate::state::AppState;
use serde_json::{Value, json};

pub const XP_POST: i64 = 5;
pub const XP_TRADE_CARD: i64 = 10;
pub const XP_LIKE: i64 = 1;
pub const XP_COMMENT: i64 = 2;
pub const XP_FOLLOWER: i64 = 3;
pub const XP_HELPFUL: i64 = 50;

/// Achievement keys (the apps translate `circle.achievement.<key>`).
pub const ACHIEVEMENTS: &[(&str, &str)] = &[
    ("first_post", "First post"),
    ("first_trade_card", "First verified trade shared"),
    ("first_story", "First story"),
    ("first_video", "First video"),
    ("followers_10", "10 followers"),
    ("followers_100", "100 followers"),
    ("followers_1000", "1,000 followers"),
    ("helpful_1", "Helpful post"),
    ("helpful_10", "10 helpful posts"),
    ("verified", "Verified trader"),
    ("live_trader", "Live trader"),
    ("academy_certificate", "Academy certificate"),
    ("top_creator", "Top creator"),
    ("level_10", "Level 10"),
];

pub fn level_for(xp: i64) -> i32 {
    (((xp.max(0) as f64) / 25.0).sqrt().floor() as i32 + 1).min(50)
}

/// XP needed to reach `level`.
pub fn threshold(level: i32) -> i64 {
    let l = (level.max(1) - 1) as i64;
    l * l * 25
}

pub fn progress(xp: i64) -> Value {
    let level = level_for(xp);
    json!({"level": level, "xp": xp, "levelStart": threshold(level), "nextLevel": if level >= 50 { Value::Null } else { json!(threshold(level + 1)) }})
}

/// Adds XP and recomputes the level (and the level achievement).
pub async fn xp(st: &AppState, user: i64, amount: i64) -> anyhow::Result<()> {
    if amount == 0 {
        return Ok(());
    }
    let new_xp: Option<i64> = sqlx::query_scalar("UPDATE profiles SET xp = GREATEST(xp + $2, 0) WHERE user_id = $1 RETURNING xp").bind(user).bind(amount).fetch_optional(&st.pool).await?;
    if let Some(x) = new_xp {
        let level = level_for(x);
        sqlx::query("UPDATE profiles SET level = $2 WHERE user_id = $1 AND level <> $2").bind(user).bind(level).execute(&st.pool).await?;
        if level >= 10 {
            award(st, user, "level_10").await?;
        }
    }
    Ok(())
}

/// Awards an achievement once; notifies the member the first time.
pub async fn award(st: &AppState, user: i64, key: &str) -> anyhow::Result<bool> {
    if !ACHIEVEMENTS.iter().any(|(k, _)| *k == key) {
        return Ok(false);
    }
    let inserted = sqlx::query("INSERT INTO achievements (user_id, key) VALUES ($1, $2) ON CONFLICT DO NOTHING").bind(user).bind(key).execute(&st.pool).await?.rows_affected() > 0;
    if inserted {
        let title = ACHIEVEMENTS.iter().find(|(k, _)| *k == key).map(|(_, t)| *t).unwrap_or(key);
        crate::notify::event(st, crate::notify::Ev { to: user, kind: "achievement", actor: None, post: None, comment: None, story: None, conversation: None, data: json!({"key": key, "title": title}) }).await?;
    }
    Ok(inserted)
}

/// Follower milestones.
pub async fn followers_milestones(st: &AppState, user: i64, followers: i64) -> anyhow::Result<()> {
    for (n, key) in [(10, "followers_10"), (100, "followers_100"), (1000, "followers_1000")] {
        if followers >= n {
            award(st, user, key).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels() {
        assert_eq!(level_for(0), 1);
        assert_eq!(level_for(24), 1);
        assert_eq!(level_for(25), 2);
        assert_eq!(level_for(400), 5);
        assert_eq!(level_for(2025), 10);
        assert_eq!(level_for(10_000_000), 50);
        assert_eq!(threshold(2), 25);
        assert_eq!(progress(30)["nextLevel"], 100);
    }
}
