//! Risk alerts to a Telegram chat (`TRADING_TELEGRAM_BOT_TOKEN`, `TRADING_TELEGRAM_CHAT_ID`; a no-op when either is
//! unset). Rate limited: one message per alert key per 15 minutes, at most 20 messages a minute overall (the rest is
//! dropped and counted in the log). Messages say what, where and how much; a client appears by login only.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

struct Limiter {
    per_key: HashMap<String, Instant>,
    window: Vec<Instant>,
}

static LIMITER: LazyLock<Mutex<Limiter>> = LazyLock::new(|| Mutex::new(Limiter { per_key: HashMap::new(), window: Vec::new() }));

pub const PER_KEY: Duration = Duration::from_secs(15 * 60);
pub const PER_MINUTE: usize = 20;

/// The bot token and chat id from the environment (None = Telegram off).
pub fn config() -> Option<(String, String)> {
    let token = std::env::var("TRADING_TELEGRAM_BOT_TOKEN").ok().filter(|v| !v.trim().is_empty())?;
    let chat = std::env::var("TRADING_TELEGRAM_CHAT_ID").ok().filter(|v| !v.trim().is_empty())?;
    Some((token.trim().to_string(), chat.trim().to_string()))
}

/// May a message for `key` go out now? (records it when yes)
pub fn allow(key: &str, now: Instant) -> bool {
    let mut l = LIMITER.lock().unwrap();
    l.window.retain(|t| now.duration_since(*t) < Duration::from_secs(60));
    if l.window.len() >= PER_MINUTE {
        return false;
    }
    if l.per_key.get(key).is_some_and(|t| now.duration_since(*t) < PER_KEY) {
        return false;
    }
    if l.per_key.len() > 10_000 {
        l.per_key.retain(|_, t| now.duration_since(*t) < PER_KEY);
    }
    l.per_key.insert(key.to_string(), now);
    l.window.push(now);
    true
}

/// The message text: `Kalks risk · <broker>`, the level and title, then the message.
pub fn text(tenant: &str, level: &str, title: &str, message: &str) -> String {
    let tag = match level {
        "red" => "RED",
        "amber" => "AMBER",
        _ => "INFO",
    };
    format!("Kalks risk · {tenant}\n{tag}: {title}\n{message}")
}

/// Sends in the background (never blocks the caller; failures are logged without the token).
pub fn send(key: &str, tenant: &str, level: &str, title: &str, message: &str) {
    let Some((token, chat)) = config() else { return };
    if !allow(key, Instant::now()) {
        tracing::debug!(key, "telegram alert rate limited");
        return;
    }
    let body = serde_json::json!({"chat_id": chat, "text": text(tenant, level, title, message), "disable_web_page_preview": true});
    tokio::spawn(async move {
        let url = format!("https://api.telegram.org/bot{token}/sendMessage");
        let r = reqwest::Client::builder().timeout(Duration::from_secs(10)).build().map(|c| c.post(url).json(&body).send());
        match r {
            Ok(f) => match f.await {
                Ok(res) if res.status().is_success() => {}
                Ok(res) => tracing::warn!(status = %res.status(), "telegram alert refused"),
                Err(e) => tracing::warn!(error = %e.without_url(), "telegram alert failed"),
            },
            Err(e) => tracing::warn!(error = %e, "telegram client"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_per_key_and_per_minute() {
        let t0 = Instant::now();
        assert!(allow("k-test-a", t0));
        assert!(!allow("k-test-a", t0 + Duration::from_secs(60)), "same key within 15 minutes");
        assert!(allow("k-test-a", t0 + PER_KEY + Duration::from_secs(1)));
        let t1 = t0 + Duration::from_secs(3600);
        let sent = (0..40).filter(|i| allow(&format!("k-burst-{i}"), t1)).count();
        assert_eq!(sent, PER_MINUTE);
        assert!(allow("k-burst-late", t1 + Duration::from_secs(61)));
        assert_eq!(text("kalks", "red", "EURUSD exposure", "120% of cap"), "Kalks risk · kalks\nRED: EURUSD exposure\n120% of cap");
    }
}
