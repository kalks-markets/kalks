//! Cashback programmes (D135 cashback centre): views. Accruals are made by the deal ingest, payouts by
//! `payouts::cashback_batch` / `cashback_tick`.

use crate::deals::month_start;
use crate::model::opt_num;
use crate::money::{D, num};
use crate::state::AppState;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::Row;

pub fn programme_json(r: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": r.get::<i64, _>("id"),
        "name": r.get::<String, _>("name"),
        "description": r.get::<String, _>("description"),
        "assetClasses": r.get::<Vec<String>, _>("asset_classes"),
        "symbols": r.get::<Vec<String>, _>("symbols"),
        "accountGroups": r.get::<Vec<String>, _>("account_groups"),
        "usdPerLot": num(r.get("usd_per_lot")),
        "maxPerMonth": opt_num(r.get("max_per_month")),
        "optIn": r.get::<bool, _>("opt_in"),
        "active": r.get::<bool, _>("active"),
        "startsAt": r.get::<DateTime<Utc>, _>("starts_at"),
        "endsAt": r.get::<Option<DateTime<Utc>>, _>("ends_at"),
        "createdAt": r.get::<DateTime<Utc>, _>("created_at"),
    })
}

pub fn accrual_json(r: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": r.get::<i64, _>("id"),
        "programmeId": r.get::<i64, _>("programme_id"),
        "programme": r.get::<String, _>("programme"),
        "userId": r.get::<i64, _>("user_id"),
        "dealId": r.get::<i64, _>("deal_id"),
        "login": r.get::<i64, _>("login"),
        "symbol": r.get::<String, _>("symbol"),
        "lots": num(r.get("lots")),
        "amount": num(r.get("amount")),
        "status": r.get::<String, _>("status"),
        "payoutId": r.get::<Option<i64>, _>("payout_id"),
        "createdAt": r.get::<DateTime<Utc>, _>("created_at"),
    })
}

/// Clients see a payout held for staff approval (four-eyes) as `pending`.
pub fn client_payout_status(s: &str) -> &str {
    match s {
        "approval" | "awaiting_second_approval" => "pending",
        _ => s,
    }
}

pub async fn me(st: &AppState, tenant: &str, user_id: i64) -> anyhow::Result<Value> {
    let ms = month_start(Utc::now());
    let progs = sqlx::query(
        "SELECT p.*, (e.user_id IS NOT NULL) AS enrolled,
                COALESCE((SELECT sum(lots) FROM cashback_accruals a WHERE a.programme_id = p.id AND a.user_id = $2 AND a.status NOT IN ('void', 'rejected') AND a.created_at >= $3), 0) AS lots_month,
                COALESCE((SELECT sum(amount) FROM cashback_accruals a WHERE a.programme_id = p.id AND a.user_id = $2 AND a.status NOT IN ('void', 'rejected') AND a.created_at >= $3), 0) AS earned_month
         FROM cashback_programmes p LEFT JOIN cashback_enrolments e ON e.programme_id = p.id AND e.user_id = $2
         WHERE p.tenant = $1 AND p.active AND (p.ends_at IS NULL OR p.ends_at > now()) ORDER BY p.id",
    )
    .bind(tenant)
    .bind(user_id)
    .bind(ms)
    .fetch_all(&st.pool)
    .await?;
    let programmes: Vec<Value> = progs
        .iter()
        .map(|r| {
            let mut v = programme_json(r);
            let opt_in: bool = r.get("opt_in");
            v["enrolled"] = json!(!opt_in || r.get::<bool, _>("enrolled"));
            v["lotsMonth"] = num(r.get("lots_month"));
            v["earnedMonth"] = num(r.get("earned_month"));
            v
        })
        .collect();
    let t = sqlx::query(
        "SELECT COALESCE(sum(amount) FILTER (WHERE status = 'accrued'), 0) AS accrued, COALESCE(sum(amount) FILTER (WHERE status = 'paid'), 0) AS paid,
                COALESCE(sum(amount) FILTER (WHERE status NOT IN ('void', 'rejected') AND created_at >= $3), 0) AS month, COALESCE(sum(amount) FILTER (WHERE status NOT IN ('void', 'rejected')), 0) AS lifetime
         FROM cashback_accruals WHERE tenant = $1 AND user_id = $2",
    )
    .bind(tenant)
    .bind(user_id)
    .bind(ms)
    .fetch_one(&st.pool)
    .await?;
    let accruals = sqlx::query(
        "SELECT a.*, p.name AS programme FROM cashback_accruals a JOIN cashback_programmes p ON p.id = a.programme_id WHERE a.tenant = $1 AND a.user_id = $2 ORDER BY a.id DESC LIMIT 100",
    )
    .bind(tenant)
    .bind(user_id)
    .fetch_all(&st.pool)
    .await?;
    let payouts = sqlx::query("SELECT * FROM cashback_payouts WHERE tenant = $1 AND user_id = $2 ORDER BY id DESC LIMIT 50").bind(tenant).bind(user_id).fetch_all(&st.pool).await?;
    let series = sqlx::query(
        "SELECT to_char(d, 'YYYY-MM-DD') AS day, COALESCE((SELECT sum(amount) FROM cashback_accruals a WHERE a.tenant = $1 AND a.user_id = $2 AND a.status NOT IN ('void', 'rejected')
            AND a.created_at >= d AND a.created_at < d + interval '1 day'), 0) AS amount
         FROM generate_series(date_trunc('day', now()) - interval '29 days', date_trunc('day', now()), interval '1 day') d ORDER BY d",
    )
    .bind(tenant)
    .bind(user_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(json!({
        "programmes": programmes,
        "totals": {"accrued": num(t.get::<D, _>("accrued")), "paid": num(t.get::<D, _>("paid")), "month": num(t.get::<D, _>("month")), "lifetime": num(t.get::<D, _>("lifetime"))},
        "accruals": accruals.iter().map(accrual_json).collect::<Vec<_>>(),
        "payouts": payouts.iter().map(|p| json!({
            "id": p.get::<i64, _>("id"), "amount": num(p.get("amount")), "status": client_payout_status(p.get::<&str, _>("status")),
            "createdAt": p.get::<DateTime<Utc>, _>("created_at"), "paidAt": p.get::<Option<DateTime<Utc>>, _>("paid_at"),
        })).collect::<Vec<_>>(),
        "series": series.iter().map(|s| json!({"day": s.get::<String, _>("day"), "amount": num(s.get("amount"))})).collect::<Vec<_>>(),
    }))
}
