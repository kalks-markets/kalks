//! Verified trade cards (Q9–Q11, Q35, Q42): created from an engine position or closing deal of the member's own
//! account, never typed in. The card stores the engine ids and a snapshot signed with HMAC-SHA256
//! (`CIRCLE_CARD_SECRET`); open cards show live P&L from market-data, and their figures are refreshed from the
//! engine (`/live`), turning into a closed card when the position is closed.
//!
//! Detail per card (Q10): `full` (entry, SL, TP, lots, money P&L) or `percent` (% move and pips only). Others
//! only see what the owner chose. "Copy this trade" (Q11) returns symbol, side, SL and TP (SL / TP only on full
//! cards) for a prefilled order ticket, and only between clients of the same broker (Q44).

use crate::error::{ApiError, ApiResult, invalid};
use crate::state::AppState;
use crate::upstream;
use chrono::{DateTime, Utc};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use sha2::Sha256;
use sqlx::Row;
use std::time::Duration;

fn f(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

/// Pip size by symbol (JPY pairs 0.01, gold 0.1, silver 0.01, indices / crypto / stocks 1, other FX 0.0001).
pub fn pip(symbol: &str) -> f64 {
    let s = symbol.to_uppercase();
    if s.starts_with("XAU") {
        0.1
    } else if s.starts_with("XAG") {
        0.01
    } else if s.contains("JPY") {
        0.01
    } else if s.len() == 6 && s.chars().all(|c| c.is_ascii_alphabetic()) && !s.starts_with("BTC") && !s.starts_with("ETH") {
        0.0001
    } else {
        1.0
    }
}

fn round(x: f64, dp: i32) -> f64 {
    let m = 10f64.powi(dp);
    (x * m).round() / m
}

/// % move and pips of a trade (positive = in the trader's favour).
pub fn moves(symbol: &str, side: &str, open: f64, price: f64) -> (f64, f64) {
    if open <= 0.0 || price <= 0.0 {
        return (0.0, 0.0);
    }
    let dir = if side == "sell" { -1.0 } else { 1.0 };
    (round((price - open) / open * 100.0 * dir, 2), round((price - open) / pip(symbol) * dir, 1))
}

pub fn sign(secret: &str, owner: i64, login: i64, ticket: Option<i64>, deal: Option<i64>, snapshot: &Value) -> String {
    let mut m = <Hmac<Sha256> as KeyInit>::new_from_slice(secret.as_bytes()).expect("hmac accepts any key size");
    m.update(format!("{owner}|{login}|{}|{}|{}", ticket.unwrap_or(0), deal.unwrap_or(0), serde_json::to_string(snapshot).unwrap_or_default()).as_bytes());
    hex::encode(m.finalize().into_bytes())
}

pub fn verify(secret: &str, r: &sqlx::postgres::PgRow) -> bool {
    let snap: Value = r.get::<sqlx::types::Json<Value>, _>("snapshot").0;
    let expected = sign(secret, r.get("owner"), r.get("login"), r.get("ticket"), r.get("deal_id"), &snap);
    crate::util::eq_ct(&expected, &r.get::<String, _>("signature"))
}

/// Snapshot of an open engine position.
pub fn from_position(acc: &Value, p: &Value) -> Value {
    let cent = acc["cent"].as_bool().unwrap_or(false);
    let usd = |x: f64| if cent { x / 100.0 } else { x };
    let symbol = p["symbol"].as_str().unwrap_or("").to_string();
    let side = p["side"].as_str().unwrap_or("buy").to_string();
    let open = f(&p["openPrice"]).unwrap_or(0.0);
    let cur = f(&p["currentPrice"]).unwrap_or(open);
    let (pct, pips) = moves(&symbol, &side, open, cur);
    let vol = f(&p["volume"]).unwrap_or(0.0);
    json!({
        "state": "open", "symbol": symbol, "side": side, "accountType": acc["type"], "option": p.get("option").is_some_and(|o| !o.is_null()),
        "volume": if cent { vol / 100.0 } else { vol }, "openPrice": open, "currentPrice": cur, "sl": f(&p["sl"]), "tp": f(&p["tp"]),
        "openTime": p["openTime"], "profit": f(&p["profit"]).map(|x| round(usd(x + f(&p["swap"]).unwrap_or(0.0) - f(&p["commission"]).unwrap_or(0.0).abs()), 2)),
        "currency": "USD", "pnlPct": pct, "pips": pips,
    })
}

/// Snapshot of a closing deal (engine history `deal_json`).
pub fn from_deal(acc: &Value, d: &Value) -> Value {
    let cent = acc["cent"].as_bool().unwrap_or(false);
    let usd = |x: f64| if cent { x / 100.0 } else { x };
    let symbol = d["symbol"].as_str().unwrap_or("").to_string();
    let side = d["positionSide"].as_str().or(d["side"].as_str()).unwrap_or("buy").to_string();
    let open = f(&d["openPrice"]).unwrap_or(0.0);
    let close = f(&d["price"]).unwrap_or(0.0);
    let (pct, pips) = moves(&symbol, &side, open, close);
    let vol = f(&d["volume"]).unwrap_or(0.0);
    let net = f(&d["profit"]).unwrap_or(0.0) + f(&d["swap"]).unwrap_or(0.0) - f(&d["commission"]).unwrap_or(0.0).abs();
    json!({
        "state": "closed", "symbol": symbol, "side": side, "accountType": acc["type"], "option": d.get("option").is_some_and(|o| !o.is_null()),
        "volume": if cent { vol / 100.0 } else { vol }, "openPrice": open, "closePrice": close, "sl": Value::Null, "tp": Value::Null,
        "openTime": d["openTime"], "closeTime": d["time"], "profit": round(usd(net), 2), "currency": "USD", "pnlPct": pct, "pips": pips,
        "reason": d["reason"], "positionTicket": d["positionTicket"],
    })
}

/// What the member can share: their accounts with open positions and closing deals of the last 30 days.
pub async fn sources(st: &AppState, tenant: &str, user: i64) -> ApiResult<Value> {
    let accounts = upstream::accounts(st, tenant, user).await.map_err(|e| ApiError::Unavailable(format!("The trading service is unavailable ({e}).")))?;
    let mut out = Vec::new();
    for a in accounts.iter().filter(|a| a["status"].as_str() != Some("archived")).take(10) {
        let Some(login) = a["login"].as_i64() else { continue };
        let detail = upstream::account_detail(st, tenant, user, login).await.ok().flatten();
        let positions: Vec<Value> = detail.as_ref().and_then(|d| d["positions"].as_array().cloned()).unwrap_or_default().iter().map(|p| {
            let s = from_position(a, p);
            json!({"ticket": p["ticket"], "symbol": s["symbol"], "side": s["side"], "volume": s["volume"], "openPrice": s["openPrice"], "sl": s["sl"], "tp": s["tp"], "profit": s["profit"], "pnlPct": s["pnlPct"], "openTime": s["openTime"]})
        }).collect();
        let deals: Vec<Value> = upstream::history(st, tenant, user, login, Some(Utc::now() - chrono::Duration::days(30)), 200)
            .await
            .ok()
            .flatten()
            .unwrap_or_default()
            .iter()
            .filter(|d| d["entry"].as_str() != Some("in"))
            .take(50)
            .map(|d| {
                let s = from_deal(a, d);
                json!({"dealId": d["id"], "symbol": s["symbol"], "side": s["side"], "volume": s["volume"], "openPrice": s["openPrice"], "closePrice": s["closePrice"], "profit": s["profit"], "pnlPct": s["pnlPct"], "closeTime": s["closeTime"]})
            })
            .collect();
        out.push(json!({"login": login, "type": a["type"], "name": a["name"], "positions": positions, "deals": deals}));
    }
    Ok(json!({"accounts": out}))
}

/// Creates a verified card from the member's own position (`ticket`) or closing deal (`dealId`).
pub async fn create(st: &AppState, tenant: &str, owner: i64, b: &Value) -> ApiResult<i64> {
    let login = b["login"].as_i64().ok_or_else(|| invalid("login", "Choose the account."))?;
    let detail = match b["detail"].as_str().unwrap_or("percent") {
        "full" => "full",
        "percent" => "percent",
        _ => return Err(invalid("detail", "detail is full or percent.")),
    };
    let unavailable = |e: anyhow::Error| ApiError::Unavailable(format!("The trading service is unavailable ({e})."));
    let acc_detail = upstream::account_detail(st, tenant, owner, login).await.map_err(unavailable)?.ok_or_else(|| invalid("login", "This isn't one of your accounts."))?;
    let acc = &acc_detail["account"];
    let (source, ticket, deal, snap) = if let Some(t) = b["ticket"].as_i64() {
        let p = acc_detail["positions"].as_array().into_iter().flatten().find(|p| p["ticket"].as_i64() == Some(t)).ok_or_else(|| invalid("ticket", "This position isn't open on the account."))?;
        ("position", Some(t), None, from_position(acc, p))
    } else if let Some(d) = b["dealId"].as_i64() {
        let deals = upstream::history(st, tenant, owner, login, Some(Utc::now() - chrono::Duration::days(365)), 1000).await.map_err(unavailable)?.unwrap_or_default();
        let deal = deals.iter().find(|x| x["id"].as_i64() == Some(d)).ok_or_else(|| invalid("dealId", "Trade not found on this account."))?;
        if deal["entry"].as_str() == Some("in") {
            return Err(invalid("dealId", "Share a closing deal (the trade must be closed)."));
        }
        ("deal", deal["positionTicket"].as_i64(), Some(d), from_deal(acc, deal))
    } else {
        return Err(invalid("ticket", "Choose an open position (ticket) or a closed trade (dealId)."));
    };
    let symbol = snap["symbol"].as_str().unwrap_or("").to_string();
    let side = snap["side"].as_str().unwrap_or("buy").to_string();
    let account_type = acc["type"].as_str().unwrap_or("live").to_string();
    let sig = sign(&st.cfg.card_secret, owner, login, ticket, deal, &snap);
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO trade_cards (owner, tenant, source, login, account_type, ticket, deal_id, symbol, side, detail, state, snapshot, signature)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) RETURNING id",
    )
    .bind(owner)
    .bind(tenant)
    .bind(source)
    .bind(login)
    .bind(&account_type)
    .bind(ticket)
    .bind(deal)
    .bind(&symbol)
    .bind(&side)
    .bind(detail)
    .bind(snap["state"].as_str().unwrap_or("closed"))
    .bind(sqlx::types::Json(&snap))
    .bind(sig)
    .fetch_one(&st.pool)
    .await?;
    Ok(id)
}

/// Card JSON for a viewer: full figures only on `full` cards (or to the owner); `live` = market-data price.
pub fn json(st: &AppState, r: &sqlx::postgres::PgRow, viewer: i64, live: Option<f64>) -> Value {
    let owner: i64 = r.get("owner");
    let snap: Value = r.get::<sqlx::types::Json<Value>, _>("snapshot").0;
    let detail: String = r.get("detail");
    let state: String = r.get("state");
    let full = detail == "full" || owner == viewer;
    let symbol: String = r.get("symbol");
    let side: String = r.get("side");
    let mut v = json!({
        "id": r.get::<i64, _>("id"),
        "verified": verify(&st.cfg.card_secret, r),
        "source": r.get::<String, _>("source"),
        "state": state,
        "symbol": symbol,
        "side": side,
        "accountType": r.get::<String, _>("account_type"),
        "detail": detail,
        "openTime": snap["openTime"],
        "closeTime": snap["closeTime"],
        "pnlPct": snap["pnlPct"],
        "pips": snap["pips"],
        "option": snap["option"],
        "refreshedAt": r.get::<DateTime<Utc>, _>("refreshed_at"),
    });
    if full {
        for k in ["volume", "openPrice", "closePrice", "currentPrice", "sl", "tp", "profit", "currency"] {
            v[k] = snap[k].clone();
        }
    }
    if state == "open"
        && let Some(px) = live
        && let Some(open) = snap["openPrice"].as_f64()
    {
        let (pct, pips) = moves(&symbol, &side, open, px);
        v["live"] = json!({"price": if full { json!(px) } else { Value::Null }, "pnlPct": pct, "pips": pips});
    } else {
        v["live"] = Value::Null;
    }
    v
}

pub async fn get(st: &AppState, id: i64) -> ApiResult<sqlx::postgres::PgRow> {
    sqlx::query("SELECT * FROM trade_cards WHERE id = $1").bind(id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)
}

/// Re-reads an open card's position from the engine (as its owner): new figures, or closed with the final result
/// once the position is gone. Cached 5 s per card.
pub async fn refresh(st: &AppState, id: i64) -> ApiResult<sqlx::postgres::PgRow> {
    let r = get(st, id).await?;
    if r.get::<String, _>("state") != "open" || st.cache.get(&format!("card:{id}"), Duration::from_secs(5)).is_some() {
        return Ok(r);
    }
    st.cache.put(&format!("card:{id}"), json!(true));
    let (owner, tenant, login, ticket): (i64, String, i64, Option<i64>) = (r.get("owner"), r.get("tenant"), r.get("login"), r.get("ticket"));
    let Some(t) = ticket else { return Ok(r) };
    let Ok(Some(d)) = upstream::account_detail(st, &tenant, owner, login).await else { return Ok(r) };
    let acc = &d["account"];
    let snap = if let Some(p) = d["positions"].as_array().into_iter().flatten().find(|p| p["ticket"].as_i64() == Some(t)) {
        from_position(acc, p)
    } else {
        // closed: the closing deal of this position
        let deals = upstream::history(st, &tenant, owner, login, Some(r.get::<DateTime<Utc>, _>("created_at") - chrono::Duration::days(1)), 1000).await.ok().flatten().unwrap_or_default();
        match deals.iter().find(|x| x["positionTicket"].as_i64() == Some(t) && x["entry"].as_str() != Some("in")) {
            Some(deal) => {
                let mut s = from_deal(acc, deal);
                // keep the SL / TP the trade was shared with
                let old: Value = r.get::<sqlx::types::Json<Value>, _>("snapshot").0;
                s["sl"] = old["sl"].clone();
                s["tp"] = old["tp"].clone();
                s
            }
            None => return Ok(r),
        }
    };
    let deal_id = if snap["state"] == "closed" { r.get::<Option<i64>, _>("deal_id") } else { None };
    let sig = sign(&st.cfg.card_secret, owner, login, ticket, deal_id, &snap);
    sqlx::query("UPDATE trade_cards SET snapshot = $2, signature = $3, state = $4, deal_id = $5, refreshed_at = now() WHERE id = $1")
        .bind(id)
        .bind(sqlx::types::Json(&snap))
        .bind(sig)
        .bind(snap["state"].as_str().unwrap_or("open"))
        .bind(deal_id)
        .execute(&st.pool)
        .await?;
    get(st, id).await
}

/// "Copy this trade": the prefilled ticket (same broker only; SL / TP on full cards).
pub fn copy_payload(r: &sqlx::postgres::PgRow) -> Value {
    let snap: Value = r.get::<sqlx::types::Json<Value>, _>("snapshot").0;
    let full = r.get::<String, _>("detail") == "full";
    let symbol: String = r.get("symbol");
    let side: String = r.get("side");
    let (sl, tp) = if full { (snap["sl"].as_f64(), snap["tp"].as_f64()) } else { (None, None) };
    let mut q = format!("?symbol={symbol}&side={side}");
    if let Some(x) = sl {
        q.push_str(&format!("&sl={x}"));
    }
    if let Some(x) = tp {
        q.push_str(&format!("&tp={x}"));
    }
    json!({"symbol": symbol, "side": side, "sl": sl, "tp": tp, "state": r.get::<String, _>("state"), "terminalPath": format!("/{q}"), "note": "Set your own size. Not investment advice."})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pips_and_moves() {
        assert_eq!(pip("EURUSD"), 0.0001);
        assert_eq!(pip("USDJPY"), 0.01);
        assert_eq!(pip("XAUUSD"), 0.1);
        assert_eq!(pip("NAS100"), 1.0);
        assert_eq!(pip("BTCUSD"), 1.0);
        assert_eq!(moves("EURUSD", "buy", 1.1000, 1.1050), (0.45, 50.0));
        assert_eq!(moves("XAUUSD", "sell", 2400.0, 2380.0), (0.83, 200.0));
        assert_eq!(moves("EURUSD", "buy", 0.0, 1.0), (0.0, 0.0));
    }

    #[test]
    fn snapshots_and_signatures() {
        let acc = json!({"type": "live", "cent": true});
        let d = json!({"id": 9, "symbol": "EURUSD", "side": "sell", "positionSide": "buy", "entry": "out", "volume": 100.0, "openPrice": 1.1, "price": 1.11, "profit": 1000.0, "swap": -10.0, "commission": -50.0, "time": "2026-10-01T10:00:00Z", "openTime": "2026-10-01T09:00:00Z", "reason": "tp", "positionTicket": 77});
        let s = from_deal(&acc, &d);
        assert_eq!((s["side"].as_str(), s["volume"].as_f64(), s["profit"].as_f64(), s["pips"].as_f64()), (Some("buy"), Some(1.0), Some(9.4), Some(100.0)));
        let a = sign("k", 1, 2, Some(3), None, &s);
        assert_eq!(a, sign("k", 1, 2, Some(3), None, &s));
        let mut tampered = s.clone();
        tampered["profit"] = json!(9999.0);
        assert_ne!(a, sign("k", 1, 2, Some(3), None, &tampered));
        assert_ne!(a, sign("other", 1, 2, Some(3), None, &s));
    }
}
