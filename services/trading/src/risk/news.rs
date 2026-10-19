//! High-impact news windows from the news service's economic calendar (`services/news`, `GET /v1/calendar`).
//!
//! Every 5 minutes the engine reads the events from yesterday to a week ahead (`NEWS_URL`, `NEWS_INTERNAL_TOKEN`;
//! nothing happens when the service is unreachable: the windows simply stay empty and the risk desk says so). A
//! symbol is in a window when an event of impact ≥ `minImpact` on one of its currencies starts within
//! `minutesBefore` … `minutesAfter` of now; the configured actions (margin multiplier, no new pending orders) apply
//! in `enforce` mode only.

use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};

use super::{NewsEvent, NewsLimit};
use crate::api::AppState;
use crate::specs::Specs;

fn base_url() -> String {
    std::env::var("NEWS_URL").ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| "http://127.0.0.1:8103".into()).trim_end_matches('/').to_string()
}

/// `GET /v1/calendar?from&to&impact` on the news service.
pub async fn fetch(from: DateTime<Utc>, to: DateTime<Utc>, impact: &str) -> anyhow::Result<Value> {
    let token = std::env::var("NEWS_INTERNAL_TOKEN").unwrap_or_default();
    let ts = |t: DateTime<Utc>| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let url = format!("{}/v1/calendar?from={}&to={}&impact={}", base_url(), ts(from), ts(to), impact.replace(',', "%2C"));
    let res = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()?
        .get(url)
        .header("x-kalks-internal", token)
        .send()
        .await?;
    anyhow::ensure!(res.status().is_success(), "news calendar answered {}", res.status());
    Ok(res.json().await?)
}

/// Timed events of the calendar response (all-day entries have no release time and are skipped).
pub fn parse_calendar(v: &Value) -> Vec<NewsEvent> {
    v["events"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|e| !e["allDay"].as_bool().unwrap_or(false))
                .filter_map(|e| {
                    Some(NewsEvent {
                        id: e["id"].as_i64()?,
                        title: e["title"].as_str().unwrap_or("").chars().take(120).collect(),
                        currency: e["currency"].as_str()?.to_uppercase(),
                        impact: e["impact"].as_i64().unwrap_or(0).clamp(0, 3) as u8,
                        starts_at: e["startsAt"].as_str().and_then(|s| DateTime::parse_from_rfc3339(s).ok())?.with_timezone(&Utc),
                        symbols: e["symbols"].as_array().map(|s| s.iter().filter_map(|x| x.as_str().map(str::to_uppercase)).collect()).unwrap_or_default(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Polls the calendar every 5 minutes (every minute until the first success).
pub async fn run(st: AppState) {
    loop {
        let now = Utc::now();
        let ok = match fetch(now - Duration::days(1), now + Duration::days(7), "1,2,3").await {
            Ok(v) => {
                let ev = parse_calendar(&v);
                tracing::debug!(events = ev.len(), "news calendar loaded");
                st.hub.shared.risk.set_news(ev, now);
                true
            }
            Err(e) => {
                tracing::warn!(error = %e, "news calendar unavailable: news windows stay empty until it answers");
                false
            }
        };
        tokio::time::sleep(std::time::Duration::from_secs(if ok { 300 } else { 60 })).await;
    }
}

/// The windows of the coming two days (and those still open) for the desk: `GET /v1/dealing/risk/news`.
pub fn windows_json(st: &AppState, limits: Option<&NewsLimit>, specs: &Specs, now: DateTime<Utc>) -> Value {
    let n = st.hub.shared.risk.news.read().unwrap();
    let (before, after, min_impact) = limits.map(|l| (l.minutes_before as i64, l.minutes_after as i64, l.min_impact)).unwrap_or((2, 2, 3));
    let mut out: Vec<Value> = n
        .events
        .iter()
        .filter(|e| e.impact >= min_impact && e.starts_at + Duration::minutes(after) >= now - Duration::hours(1) && e.starts_at <= now + Duration::hours(48))
        .map(|e| {
            let from = e.starts_at - Duration::minutes(before);
            let to = e.starts_at + Duration::minutes(after);
            let symbols: Vec<&str> = specs.all().filter(|s| s.core && e.affects(s)).map(|s| s.symbol.as_str()).take(40).collect();
            json!({"id": e.id.to_string(), "title": e.title, "currency": e.currency, "impact": e.impact, "startsAt": e.starts_at, "from": from, "to": to, "active": now >= from && now <= to, "symbols": symbols})
        })
        .collect();
    out.sort_by(|a, b| a["startsAt"].as_str().cmp(&b["startsAt"].as_str()));
    json!({"windows": out, "configured": n.configured, "lastOkAt": n.last_ok, "minutesBefore": before, "minutesAfter": after, "minImpact": min_impact})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_events_parse_and_windows_cover_the_symbol_currencies() {
        let v = json!({"events": [
            {"id": 7, "title": "Non-Farm Employment Change", "currency": "USD", "impact": 3, "startsAt": "2026-09-28T12:30:00Z", "allDay": false, "symbols": ["EURUSD", "XAUUSD"]},
            {"id": 8, "title": "Bank Holiday", "currency": "JPY", "impact": 1, "startsAt": "2026-09-28T00:00:00Z", "allDay": true, "symbols": []},
            {"id": 9, "title": "CPI y/y", "currency": "gbp", "impact": 2, "startsAt": "2026-09-28T06:00:00Z", "allDay": false}
        ]});
        let ev = parse_calendar(&v);
        assert_eq!(ev.len(), 2);
        assert_eq!(ev[1].currency, "GBP");
        let specs = crate::specs::test_specs();
        let risk = crate::risk::RiskCtx::default();
        risk.set_news(ev, Utc::now());
        let at = |s: &str| DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc);
        let eur = specs.get("EURUSD").unwrap();
        let jpy = specs.get("USDJPY").unwrap();
        let gbpjpy = specs.get("GBPJPY").or_else(|| specs.get("EURGBP")).unwrap();
        // 2 minutes before / after NFP, impact ≥ 3
        assert!(risk.news_hit(eur, at("2026-09-28T12:28:00Z"), 2, 2, 3).is_some());
        assert!(risk.news_hit(eur, at("2026-09-28T12:32:00Z"), 2, 2, 3).is_some());
        assert!(risk.news_hit(eur, at("2026-09-28T12:27:00Z"), 2, 2, 3).is_none());
        assert!(risk.news_hit(jpy, at("2026-09-28T12:30:00Z"), 2, 2, 3).is_some(), "USDJPY moves on USD news");
        assert_eq!(risk.news_hit(eur, at("2026-09-28T12:29:00Z"), 2, 2, 3).unwrap().minutes_to, 1);
        // the GBP release is impact 2: only with min impact 2
        assert!(risk.news_hit(gbpjpy, at("2026-09-28T06:00:00Z"), 2, 2, 3).is_none());
        assert!(risk.news_hit(gbpjpy, at("2026-09-28T06:00:00Z"), 2, 2, 2).is_some());
        assert_eq!(risk.news_minutes(eur, at("2026-09-28T11:30:00Z"), 3), Some(60));
    }
}
