//! Routing suggestions (`routing_suggestions`): what the routing rules and the client scores would move to the other
//! book. While the tenant's routing mode is `recommend` (launch, 100 % B-book) this is the only effect of the rules
//! besides the label on the ticket; in `execute` mode the rules route new trades themselves and the queue still shows
//! open positions the rules (or a changed score) would now put elsewhere.
//!
//! Every 30 seconds [`detect`] proposes:
//! - **position** moves: an open CFD position whose matching rule (with today's facts, split by its ticket) names
//!   another book (dealer quick routes `RQ-…` are for new trades only and are left out);
//! - **account** moves: a live account scored `toxic` or `likely_profitable` with positions on the B-book (to A), or
//!   `likely_losing` with positions on the A-book (to B).
//!
//! A dealer approves (api/risk.rs: the existing book transfer, audited) or rejects (never proposed again for that
//! position; an account again after 7 days). Pending suggestions whose position closed or already moved expire.

use serde_json::{Value, json};
use sqlx::Row;
use std::collections::HashSet;
use std::sync::Arc;

use crate::api::AppState;
use crate::model::{AccountKind, Book};
use crate::rules::{RouteCtx, match_rule};
use crate::shard::ScanFn;

pub fn row_json(r: &sqlx::postgres::PgRow) -> Value {
    let t = |c: &str| r.get::<Option<chrono::DateTime<chrono::Utc>>, _>(c);
    json!({
        "id": r.get::<i64, _>("id").to_string(),
        "kind": r.get::<String, _>("kind"),
        "login": r.get::<i64, _>("login").to_string(),
        "userId": r.get::<i64, _>("user_id").to_string(),
        "ticket": r.get::<Option<i64>, _>("ticket").map(|t| t.to_string()),
        "symbol": r.get::<Option<String>, _>("symbol"),
        "volume": r.get::<Option<crate::money::D>, _>("volume").map(crate::money::num),
        "from": r.get::<String, _>("from_book").trim(),
        "to": r.get::<String, _>("to_book").trim(),
        "reasons": r.get::<sqlx::types::Json<Value>, _>("reasons").0,
        "rule": r.get::<Option<String>, _>("rule"),
        "score": r.get::<Option<crate::money::D>, _>("score").map(crate::money::num),
        "category": r.get::<Option<String>, _>("category"),
        "status": r.get::<String, _>("status"),
        "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "decidedAt": t("decided_at"),
        "decidedBy": r.get::<Option<String>, _>("decided_by"),
        "note": r.get::<Option<String>, _>("note"),
        "result": r.get::<Option<sqlx::types::Json<Value>>, _>("result").map(|j| j.0),
    })
}

pub async fn list(st: &AppState, tenant_id: i64, status: &str, limit: i64) -> anyhow::Result<Vec<Value>> {
    let rows = sqlx::query("SELECT * FROM routing_suggestions WHERE tenant_id = $1 AND ($2 = 'all' OR status = $2) ORDER BY created_at DESC, id DESC LIMIT $3")
        .bind(tenant_id)
        .bind(status)
        .bind(limit.clamp(1, 1000))
        .fetch_all(&st.pool)
        .await?;
    Ok(rows.iter().map(row_json).collect())
}

fn reason(label: &str, detail: impl Into<String>) -> Value {
    json!({"label": label, "detail": detail.into()})
}

/// The candidates of one tenant (positions and accounts), from a scan of the live accounts.
pub async fn candidates(st: &AppState, tenant_id: i64) -> Vec<Value> {
    let risk = st.hub.shared.risk.clone();
    let f: ScanFn = Arc::new(move |a, env| {
        if a.account.kind != AccountKind::Live || a.positions.is_empty() {
            return vec![];
        }
        let acc = &a.account;
        let mut out = Vec::new();
        let lite = risk.score(acc.login);
        for p in a.positions.values().filter(|p| p.option.is_none()) {
            let facts = crate::engine::trade::route_facts(env, a, &p.symbol, lite.as_ref());
            let ctx = RouteCtx { login: acc.login, group: &acc.group, symbol: &p.symbol, volume: p.volume, id: p.ticket, facts };
            let rules: Vec<_> = env.tenant.routing_rules.iter().filter(|r| !r.id.starts_with("RQ-")).cloned().collect();
            if let Some((r, book)) = match_rule(&rules, &ctx)
                && book != p.book
            {
                let conds = r.conditions.iter().map(|c| format!("{} {} {}", c.field, c.op, c.value)).collect::<Vec<_>>().join(&format!(" {} ", r.join));
                let mut reasons = vec![reason(&format!("Rule {} · {}", r.id, r.name), format!("Matches: {conds}"))];
                if r.action.pct < 100.0 {
                    reasons.push(reason("Split", format!("{:.0}% of tickets go to {}-book; ticket #{} falls in that share", r.action.pct, r.action.book.as_str(), p.ticket)));
                }
                if let Some(s) = &lite {
                    reasons.push(reason("Client score", format!("{:.0}/100 ({})", s.score, s.category.replace('_', " "))));
                }
                out.push(json!({"kind": "position", "login": acc.login, "userId": acc.user_id, "ticket": p.ticket, "symbol": p.symbol, "volume": p.volume.to_string(), "from": p.book.as_str(), "to": book.as_str(), "rule": r.id, "reasons": reasons, "score": lite.as_ref().map(|s| s.score), "category": lite.as_ref().map(|s| s.category.clone())}));
            }
        }
        if let Some(s) = &lite {
            let (to, from) = match s.category.as_str() {
                "toxic" | "likely_profitable" => (Book::A, Book::B),
                "likely_losing" => (Book::B, Book::A),
                _ => return out,
            };
            let on_from: Vec<&crate::model::Position> = a.positions.values().filter(|p| p.option.is_none() && p.book == from).collect();
            if !on_from.is_empty() {
                let lots: crate::money::D = on_from.iter().map(|p| p.volume).sum();
                let why = match s.category.as_str() {
                    "toxic" => "Toxic-flow signals: the house should not keep this flow on the B-book",
                    "likely_profitable" => "The client is likely to keep winning: hedge the flow on the A-book",
                    _ => "The client is likely to keep losing: the flow can stay with the house",
                };
                let mut reasons = vec![reason("Client score", format!("{:.0}/100 ({})", s.score, s.category.replace('_', " "))), reason("Why", why)];
                if let Some(w) = s.win_rate {
                    reasons.push(reason("Win rate", format!("{w:.0}%")));
                }
                if let Some(h) = s.avg_hold_secs {
                    reasons.push(reason("Average hold", format!("{h:.0} s")));
                }
                reasons.push(reason("Open positions", format!("{} on {}-book, {} lots", on_from.len(), from.as_str(), lots.normalize())));
                out.push(json!({"kind": "account", "login": acc.login, "userId": acc.user_id, "ticket": null, "symbol": null, "volume": lots.to_string(), "from": from.as_str(), "to": to.as_str(), "rule": null, "reasons": reasons, "score": s.score, "category": s.category}));
            }
        }
        out
    });
    st.hub.scan(tenant_id, f).await
}

/// Inserts new suggestions and expires stale ones. Returns how many were added.
pub async fn detect(st: &AppState, tenant_id: i64) -> anyhow::Result<usize> {
    let cands = candidates(st, tenant_id).await;
    let pool = &st.pool;
    let mut added = 0;
    let mut live_keys: HashSet<(String, i64, i64)> = HashSet::new();
    for c in &cands {
        let kind = c["kind"].as_str().unwrap_or("position").to_string();
        let login = c["login"].as_i64().unwrap_or(0);
        let ticket = c["ticket"].as_i64();
        live_keys.insert((kind.clone(), login, ticket.unwrap_or(0)));
        // a dealer said no: never again for the position, not within a week for the account
        let refused: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM routing_suggestions WHERE tenant_id = $1 AND kind = $2 AND login = $3 AND COALESCE(ticket, 0) = $4 AND status = 'rejected'
               AND ($2 = 'position' OR decided_at > now() - interval '7 days'))",
        )
        .bind(tenant_id)
        .bind(&kind)
        .bind(login)
        .bind(ticket.unwrap_or(0))
        .fetch_one(pool)
        .await?;
        if refused {
            continue;
        }
        let r = sqlx::query(
            "INSERT INTO routing_suggestions (tenant_id, kind, login, user_id, ticket, symbol, volume, from_book, to_book, reasons, rule, score, category)
             VALUES ($1,$2,$3,$4,$5,$6,$7::numeric,$8,$9,$10,$11,$12::numeric,$13)
             ON CONFLICT (tenant_id, kind, login, (COALESCE(ticket, 0))) WHERE status = 'pending' DO NOTHING",
        )
        .bind(tenant_id)
        .bind(&kind)
        .bind(login)
        .bind(c["userId"].as_i64().unwrap_or(0))
        .bind(ticket)
        .bind(c["symbol"].as_str())
        .bind(c["volume"].as_str())
        .bind(c["from"].as_str().unwrap_or("B"))
        .bind(c["to"].as_str().unwrap_or("A"))
        .bind(sqlx::types::Json(&c["reasons"]))
        .bind(c["rule"].as_str())
        .bind(c["score"].as_f64().map(|s| format!("{s}")))
        .bind(c["category"].as_str())
        .execute(pool)
        .await?;
        added += r.rows_affected() as usize;
    }
    // pending suggestions no longer proposed (closed, moved, score changed) expire
    let pending = sqlx::query("SELECT id, kind, login, COALESCE(ticket, 0) AS t FROM routing_suggestions WHERE tenant_id = $1 AND status = 'pending'").bind(tenant_id).fetch_all(pool).await?;
    let stale: Vec<i64> = pending.iter().filter(|r| !live_keys.contains(&(r.get::<String, _>("kind"), r.get::<i64, _>("login"), r.get::<i64, _>("t")))).map(|r| r.get("id")).collect();
    if !stale.is_empty() {
        sqlx::query("UPDATE routing_suggestions SET status = 'expired', decided_at = now(), decided_by = 'system', note = 'no longer applies' WHERE id = ANY($1) AND status = 'pending'").bind(&stale).execute(pool).await?;
    }
    if added > 0 {
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM routing_suggestions WHERE tenant_id = $1 AND status = 'pending'").bind(tenant_id).fetch_one(pool).await?;
        super::alerts::raise(
            st,
            tenant_id,
            super::alerts::AlertSpec {
                key: "routing:pending".into(),
                kind: "routing",
                level: super::alerts::Level::Info,
                title: format!("{n} routing suggestion(s) waiting"),
                message: format!("{added} new; review them in Trading › Routing suggestions"),
                symbol: None,
                login: None,
                data: json!({"pending": n, "added": added}),
            },
        )
        .await;
    }
    Ok(added)
}

pub async fn run(st: AppState) {
    tokio::time::sleep(std::time::Duration::from_secs(20)).await;
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
    loop {
        tick.tick().await;
        for t in st.hub.shared.registry.all() {
            if let Err(e) = detect(&st, t.tenant_id).await {
                tracing::warn!(tenant = t.tenant_id, error = %e, "routing suggestions pass failed");
            }
        }
    }
}
