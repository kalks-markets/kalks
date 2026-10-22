//! Loyalty points (D135): balances, tiers, the client's rewards view, catalogue redemption and expiry.

use crate::audit::{self, Actor};
use crate::bonus;
use crate::calc;

use crate::db;
use crate::deals::{earned_12m, month_start};
use crate::error::{ApiError, ApiResult};
use crate::four_eyes;
use crate::clients;
use crate::model::Tier;
use crate::money::{D, ZERO, num};
use crate::state::AppState;
use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use sqlx::Row;

pub async fn balance<'e, E: sqlx::PgExecutor<'e>>(ex: E, tenant: &str, user_id: i64) -> anyhow::Result<i64> {
    let v: Option<i64> = sqlx::query_scalar("SELECT sum(points)::bigint FROM points_ledger WHERE tenant = $1 AND user_id = $2").bind(tenant).bind(user_id).fetch_one(ex).await?;
    Ok(v.unwrap_or(0))
}

/// Serialises every points change of one client (redemptions, adjustments, expiry).
pub async fn lock_user(tx: &mut sqlx::PgConnection, tenant: &str, user_id: i64) -> anyhow::Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))").bind(format!("points:{tenant}:{user_id}")).execute(&mut *tx).await?;
    Ok(())
}

pub fn tx_json(r: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": r.get::<i64, _>("id"),
        "kind": r.get::<String, _>("kind"),
        "points": r.get::<i64, _>("points"),
        "description": r.get::<String, _>("description"),
        "login": r.get::<Option<i64>, _>("login"),
        "dealId": r.get::<Option<i64>, _>("deal_id"),
        "createdAt": r.get::<DateTime<Utc>, _>("created_at"),
    })
}

pub fn item_json(r: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": r.get::<i64, _>("id"),
        "name": r.get::<String, _>("name"),
        "description": r.get::<String, _>("description"),
        "kind": r.get::<String, _>("kind"),
        "costPoints": r.get::<i64, _>("cost_points"),
        "value": num(r.get("value")),
        "minTier": r.get::<Option<String>, _>("min_tier"),
        "stock": r.get::<Option<i32>, _>("stock"),
        "active": r.get::<bool, _>("active"),
        "sort": r.get::<i32, _>("sort"),
        "params": r.get::<sqlx::types::Json<Value>, _>("params").0,
    })
}

/// A redemption as the client sees it (a cashback redemption held for staff approval shows as `pending`).
pub fn redemption_json(r: &sqlx::postgres::PgRow) -> Value {
    let mut v = redemption_view(r);
    if matches!(v["status"].as_str(), Some("approval" | "awaiting_second_approval")) {
        v["status"] = json!("pending");
    }
    v
}

/// A redemption for the Back Office: the stored status, who decided and the four-eyes state.
pub fn redemption_admin_json(r: &sqlx::postgres::PgRow, threshold: Option<D>) -> Value {
    let mut v = redemption_view(r);
    let status: String = r.get("status");
    let value: D = r.get("value");
    let money = r.get::<String, _>("kind") == "cashback";
    v["decidedBy"] = json!(r.get::<Option<String>, _>("decided_by"));
    v["fourEyes"] = four_eyes::json_of(r, money.then_some(value), &status, threshold);
    v
}

fn redemption_view(r: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": r.get::<i64, _>("id"),
        "itemId": r.get::<i64, _>("item_id"),
        "itemName": r.get::<String, _>("item_name"),
        "kind": r.get::<String, _>("kind"),
        "points": r.get::<i64, _>("points"),
        "value": num(r.get("value")),
        "status": r.get::<String, _>("status"),
        "login": r.get::<Option<i64>, _>("login"),
        "voucherCode": r.get::<Option<String>, _>("voucher_code"),
        "grantId": r.get::<Option<i64>, _>("grant_id"),
        "error": r.get::<Option<String>, _>("error"),
        "createdAt": r.get::<DateTime<Utc>, _>("created_at"),
        "completedAt": r.get::<Option<DateTime<Utc>>, _>("completed_at"),
    })
}

pub fn voucher_json(r: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": r.get::<i64, _>("id"),
        "code": r.get::<String, _>("code"),
        "kind": r.get::<String, _>("kind"),
        "pct": num(r.get("pct")),
        "appliesTo": r.get::<String, _>("applies_to"),
        "status": r.get::<String, _>("status"),
        "expiresAt": r.get::<DateTime<Utc>, _>("expires_at"),
        "usedAt": r.get::<Option<DateTime<Utc>>, _>("used_at"),
        "source": r.get::<String, _>("source"),
    })
}

fn tier_json(t: &Tier) -> Value {
    json!({"key": t.key, "name": t.name, "rank": t.rank, "minPoints": t.min_points, "multiplier": num(t.multiplier), "perks": t.perks})
}

/// The client's Rewards view.
pub async fn rewards(st: &AppState, tenant: &str, user_id: i64) -> anyhow::Result<Value> {
    let settings = db::settings(&st.pool, tenant).await?;
    let tiers = db::tiers(&st.pool, tenant).await?;
    let rules = db::rules(&st.pool, tenant).await?;
    let bal = balance(&st.pool, tenant, user_id).await?;
    let earned = earned_12m(&st.pool, tenant, user_id).await?;
    let ms = month_start(Utc::now());
    let r = sqlx::query(
        "SELECT COALESCE(sum(points) FILTER (WHERE points > 0 AND kind IN ('earn','bonus','promo')), 0)::bigint AS lifetime,
                COALESCE(sum(points) FILTER (WHERE kind = 'earn' AND created_at >= $3), 0)::bigint AS month,
                COALESCE(sum(lots) FILTER (WHERE kind = 'earn' AND created_at >= $3), 0) AS lots_month,
                max(created_at) FILTER (WHERE kind = 'earn') AS last_earn
         FROM points_ledger WHERE tenant = $1 AND user_id = $2",
    )
    .bind(tenant)
    .bind(user_id)
    .bind(ms)
    .fetch_one(&st.pool)
    .await?;
    let last_earn: Option<DateTime<Utc>> = r.get("last_earn");
    let expiring = match last_earn {
        Some(t) if bal > 0 => {
            let at = t + Duration::days(30 * settings.points_expiry_months);
            (at - Utc::now() < Duration::days(31)).then(|| json!({"points": bal, "at": at}))
        }
        _ => None,
    };
    let tier = calc::tier_for(&tiers, earned).cloned().unwrap_or_else(|| tiers[0].clone());
    let next = calc::next_tier(&tiers, &tier).map(|n| json!({"key": n.key, "name": n.name, "minPoints": n.min_points, "pointsToGo": (n.min_points - earned).max(0)}));
    let catalogue = sqlx::query("SELECT * FROM catalogue WHERE tenant = $1 AND active ORDER BY sort, id").bind(tenant).fetch_all(&st.pool).await?;
    let recent = sqlx::query("SELECT * FROM points_ledger WHERE tenant = $1 AND user_id = $2 ORDER BY id DESC LIMIT 10").bind(tenant).bind(user_id).fetch_all(&st.pool).await?;
    let series = sqlx::query(
        "SELECT to_char(d, 'YYYY-MM-DD') AS day, COALESCE((SELECT sum(points) FROM points_ledger p WHERE p.tenant = $1 AND p.user_id = $2 AND p.kind = 'earn'
            AND p.created_at >= d AND p.created_at < d + interval '1 day'), 0)::bigint AS points
         FROM generate_series(date_trunc('day', now()) - interval '29 days', date_trunc('day', now()), interval '1 day') d ORDER BY d",
    )
    .bind(tenant)
    .bind(user_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(json!({
        "points": {
            "balance": bal, "lifetime": r.get::<i64, _>("lifetime"), "earnedThisMonth": r.get::<i64, _>("month"),
            "lotsThisMonth": num(r.get("lots_month")), "earned12m": earned, "expiringSoon": expiring,
        },
        "tier": tier_json(&tier),
        "nextTier": next,
        "tiers": tiers.iter().map(tier_json).collect::<Vec<_>>(),
        "rules": rules.iter().filter(|r| r.active).map(|r| r.json()).collect::<Vec<_>>(),
        "pointValue": num(settings.point_value),
        "minHoldSeconds": settings.min_hold_seconds,
        "pointsExpiryMonths": settings.points_expiry_months,
        "catalogue": catalogue.iter().map(item_json).collect::<Vec<_>>(),
        "recent": recent.iter().map(tx_json).collect::<Vec<_>>(),
        "series": series.iter().map(|s| json!({"day": s.get::<String, _>("day"), "points": s.get::<i64, _>("points")})).collect::<Vec<_>>(),
    }))
}

/// Hidden fixed campaign that loyalty bonus-credit redemptions open their grants under.
pub async fn system_campaign(tx: &mut sqlx::PgConnection, tenant: &str) -> anyhow::Result<i64> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))").bind(format!("growth-syscampaign:{tenant}")).execute(&mut *tx).await?;
    if let Some(id) = sqlx::query_scalar::<_, i64>("SELECT id FROM bonus_campaigns WHERE tenant = $1 AND created_by = 'system:loyalty'").bind(tenant).fetch_optional(&mut *tx).await? {
        return Ok(id);
    }
    Ok(sqlx::query_scalar(
        "INSERT INTO bonus_campaigns (tenant, name, description, terms, kind, release_per_lot, expiry_days, visibility, status, per_user_limit, created_by)
         VALUES ($1, 'Loyalty reward bonus', 'Bonus credit redeemed with loyalty points.', 'Released to balance per lot traded; the unreleased part is removed at expiry or on a withdrawal.',
                 'fixed', 5, 60, 'code_only', 'active', 0, 'system:loyalty') RETURNING id",
    )
    .bind(tenant)
    .fetch_one(&mut *tx)
    .await?)
}

pub fn voucher_code() -> String {
    format!("KV-{}", db::random_code(8))
}

/// Redeems a catalogue item: points are deducted with the redemption in one transaction.
pub async fn redeem(st: &AppState, tenant: &str, user_id: i64, item_id: i64, login: Option<i64>) -> ApiResult<Value> {
    let item = sqlx::query("SELECT * FROM catalogue WHERE id = $1 AND tenant = $2").bind(item_id).bind(tenant).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    let kind: String = item.get("kind");
    let params: Value = item.get::<sqlx::types::Json<Value>, _>("params").0;
    // bonus credit needs a live account of the client (checked before the transaction: engine call)
    let account = if kind == "bonus_credit" {
        let l = login.ok_or_else(|| crate::error::invalid("login", "Choose the live account for the bonus."))?;
        Some(bonus::check_account(st, tenant, user_id, l, &[]).await?)
    } else {
        None
    };
    let tiers = db::tiers(&st.pool, tenant).await?;
    let mut tx = st.pool.begin().await?;
    lock_user(&mut tx, tenant, user_id).await?;
    let item = sqlx::query("SELECT * FROM catalogue WHERE id = $1 FOR UPDATE").bind(item_id).fetch_one(&mut *tx).await?;
    if !item.get::<bool, _>("active") {
        return Err(ApiError::Conflict { code: "not_eligible", message: "This reward is no longer available.".into() });
    }
    if item.get::<Option<i32>, _>("stock").is_some_and(|s| s <= 0) {
        return Err(ApiError::Conflict { code: "out_of_stock", message: "This reward is out of stock.".into() });
    }
    if let Some(min) = item.get::<Option<String>, _>("min_tier") {
        let earned = earned_12m(&mut *tx, tenant, user_id).await?;
        let cur = calc::tier_for(&tiers, earned).map(|t| t.rank).unwrap_or(1);
        let need = tiers.iter().find(|t| t.key == min).map(|t| t.rank).unwrap_or(1);
        if cur < need {
            return Err(ApiError::Conflict { code: "not_eligible", message: format!("This reward needs the {} tier.", tiers.iter().find(|t| t.key == min).map(|t| t.name.as_str()).unwrap_or(&min)) });
        }
    }
    let cost: i64 = item.get("cost_points");
    let bal = balance(&mut *tx, tenant, user_id).await?;
    if bal < cost {
        return Err(ApiError::Conflict { code: "insufficient_points", message: format!("You need {} more points.", cost - bal) });
    }
    let value: D = item.get("value");
    let name: String = item.get("name");
    let rid: i64 = sqlx::query_scalar("INSERT INTO redemptions (tenant, user_id, item_id, item_name, kind, points, value, login) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id")
        .bind(tenant)
        .bind(user_id)
        .bind(item_id)
        .bind(&name)
        .bind(&kind)
        .bind(cost)
        .bind(value)
        .bind(account.as_ref().map(|a| a.login))
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO points_ledger (tenant, user_id, kind, points, ref, description) VALUES ($1,$2,'redeem',$3,$4,$5)")
        .bind(tenant)
        .bind(user_id)
        .bind(-cost)
        .bind(format!("redemption:{rid}"))
        .bind(format!("Redeemed · {name}"))
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE catalogue SET stock = stock - 1 WHERE id = $1 AND stock IS NOT NULL").bind(item_id).execute(&mut *tx).await?;
    // four-eyes: a cashback redemption at or above the threshold waits for two staff members (no wallet credit yet)
    let held = kind == "cashback" && four_eyes::needs_two(value, db::settings(&st.pool, tenant).await?.four_eyes_threshold);
    match kind.as_str() {
        "cashback" if held => {
            sqlx::query("UPDATE redemptions SET status = 'approval' WHERE id = $1").bind(rid).execute(&mut *tx).await?;
        }
        "cashback" => {
            sqlx::query("INSERT INTO wallet_credits (tenant, user_id, idem_key, kind, amount, ref, note) VALUES ($1,$2,$3,'adjustment',$4,$5,$6)")
                .bind(tenant)
                .bind(user_id)
                .bind(format!("growth:redeem:{rid}"))
                .bind(value)
                .bind(format!("redemption:{rid}"))
                .bind(format!("Loyalty reward: {name}"))
                .execute(&mut *tx)
                .await?;
        }
        "bonus_credit" => {
            let acc = account.as_ref().unwrap();
            let campaign = system_campaign(&mut tx, tenant).await?;
            let rpl = params["releasePerLot"].as_f64().and_then(crate::money::from_f64).filter(|d| *d > ZERO).unwrap_or(D::from(5));
            let exp = params["expiryDays"].as_i64().filter(|d| *d > 0).unwrap_or(60) as i32;
            let gid = bonus::queue_grant(&mut tx, tenant, campaign, user_id, "redemption", acc, value, None, None, rpl, exp, Some(&format!("redemption:{rid}"))).await?;
            sqlx::query("UPDATE redemptions SET grant_id = $2, status = 'completed', completed_at = now() WHERE id = $1").bind(rid).bind(gid).execute(&mut *tx).await?;
        }
        _ => {
            let code = voucher_code();
            let days = params["validDays"].as_i64().filter(|d| *d > 0).unwrap_or(90);
            sqlx::query("INSERT INTO vouchers (tenant, user_id, code, pct, applies_to, source, expires_at) VALUES ($1,$2,$3,$4,$5,'redemption', now() + make_interval(days => $6))")
                .bind(tenant)
                .bind(user_id)
                .bind(&code)
                .bind(value)
                .bind(params["appliesTo"].as_str().unwrap_or("any"))
                .bind(days as i32)
                .execute(&mut *tx)
                .await?;
            sqlx::query("UPDATE redemptions SET voucher_code = $2, status = 'completed', completed_at = now() WHERE id = $1").bind(rid).bind(&code).execute(&mut *tx).await?;
        }
    }
    audit::record(&mut *tx, tenant, &Actor { id: format!("user:{user_id}"), name: None }, "loyalty.redeem", Some(format!("redemption:{rid}")), None, Some(json!({"item": item_id, "points": cost, "kind": kind, "heldForApproval": held})), None).await?;
    tx.commit().await?;
    st.wake.notify_one();
    let r = sqlx::query("SELECT * FROM redemptions WHERE id = $1").bind(rid).fetch_one(&st.pool).await?;
    let bal = balance(&st.pool, tenant, user_id).await?;
    Ok(json!({"redemption": redemption_json(&r), "balance": bal}))
}

/// Staff points adjustment (signed). A negative adjustment cannot take the balance below zero.
pub async fn adjust(st: &AppState, tenant: &str, user_id: i64, points: i64, note: &str, actor: &Actor) -> ApiResult<i64> {
    let mut tx = st.pool.begin().await?;
    lock_user(&mut tx, tenant, user_id).await?;
    let bal = balance(&mut *tx, tenant, user_id).await?;
    if bal + points < 0 {
        return Err(crate::error::invalid("points", format!("The client has {bal} points.")));
    }
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM points_ledger WHERE tenant = $1 AND kind = 'adjust'").bind(tenant).fetch_one(&mut *tx).await?;
    sqlx::query("INSERT INTO points_ledger (tenant, user_id, kind, points, ref, description) VALUES ($1,$2,'adjust',$3,$4,$5)")
        .bind(tenant)
        .bind(user_id)
        .bind(points)
        .bind(format!("adjust:{}:{}", Utc::now().timestamp_micros(), n))
        .bind(format!("Adjustment · {note}"))
        .execute(&mut *tx)
        .await?;
    audit::record(&mut *tx, tenant, actor, "loyalty.adjust", Some(format!("user:{user_id}")), Some(json!({"balance": bal})), Some(json!({"balance": bal + points, "points": points})), Some(note)).await?;
    tx.commit().await?;
    Ok(bal + points)
}

/// Expires the balance of clients with no earn for `pointsExpiryMonths`.
pub async fn expiry_tick(st: &AppState, tenant: &str) -> anyhow::Result<usize> {
    let s = db::settings(&st.pool, tenant).await?;
    let cutoff = Utc::now() - Duration::days(30 * s.points_expiry_months);
    let users: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT user_id, sum(points)::bigint FROM points_ledger WHERE tenant = $1 GROUP BY user_id
         HAVING sum(points) > 0 AND COALESCE(max(created_at) FILTER (WHERE kind IN ('earn','bonus','promo')), min(created_at)) < $2",
    )
    .bind(tenant)
    .bind(cutoff)
    .fetch_all(&st.pool)
    .await?;
    let day = Utc::now().format("%Y-%m-%d").to_string();
    for (user, _) in &users {
        let mut tx = st.pool.begin().await?;
        lock_user(&mut tx, tenant, *user).await?;
        let bal = balance(&mut *tx, tenant, *user).await?;
        if bal > 0 {
            sqlx::query("INSERT INTO points_ledger (tenant, user_id, kind, points, ref, description) VALUES ($1,$2,'expire',$3,$4,'Points expired (no trading activity)') ON CONFLICT DO NOTHING")
                .bind(tenant)
                .bind(user)
                .bind(-bal)
                .bind(format!("expire:{day}"))
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
    }
    Ok(users.len())
}

/// Wallet credit helper used by redemptions and prizes.
pub async fn queue_wallet_credit(tx: &mut sqlx::PgConnection, tenant: &str, user_id: i64, key: &str, kind: &str, amount: D, reference: &str, note: &str) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO wallet_credits (tenant, user_id, idem_key, kind, amount, ref, note) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (idem_key) DO NOTHING")
        .bind(tenant)
        .bind(user_id)
        .bind(key)
        .bind(kind)
        .bind(amount)
        .bind(reference)
        .bind(note)
        .execute(&mut *tx)
        .await?;
    Ok(())
}

/// Staff decision on a cashback redemption held for approval (`approval` or `awaiting_second_approval`). Four-eyes
/// like the other payouts: at or above `fourEyesThreshold` the first approval only records it, the second (another
/// staff member, else 409 `four_eyes_same_staff`) queues the wallet credit `growth:redeem:<redemption>`. Reject
/// (reason required, either approver): `rejected`, the points go back to the client and the stock is restored.
pub async fn decide_redemption(st: &AppState, tenant: &str, id: i64, approve: bool, actor: &Actor, note: &str) -> ApiResult<Value> {
    let threshold = db::settings(&st.pool, tenant).await?.four_eyes_threshold;
    let mut tx = st.pool.begin().await?;
    let r = sqlx::query("SELECT * FROM redemptions WHERE id = $1 AND tenant = $2 FOR UPDATE").bind(id).bind(tenant).fetch_optional(&mut *tx).await?.ok_or(ApiError::NotFound)?;
    let status: String = r.get("status");
    if !matches!(status.as_str(), "approval" | four_eyes::AWAITING) {
        return Err(ApiError::Conflict { code: "state", message: "This redemption isn't waiting for a decision.".into() });
    }
    let user: i64 = r.get("user_id");
    let value: D = r.get("value");
    let points: i64 = r.get("points");
    let item_name: String = r.get("item_name");
    let first_id: Option<String> = r.get("first_approved_by_id");
    let first_name: Option<String> = r.get("first_approved_by_name");
    let note_opt = Some(note.trim()).filter(|n| !n.is_empty());
    let target = Some(format!("redemption:{id}"));
    let mut release = false;
    if approve {
        if status == four_eyes::AWAITING {
            if first_id.as_deref() == Some(four_eyes::staff_id(actor).as_str()) {
                return Err(four_eyes::same_staff());
            }
            if !four_eyes::claim_second(&mut tx, "redemptions", "status", id, actor, "pending").await? {
                return Err(ApiError::Conflict { code: "state", message: "This redemption isn't waiting for a decision.".into() });
            }
            let mut after = four_eyes::approvers(first_id.as_deref(), first_name.as_deref(), actor);
            after["value"] = num(value);
            audit::record(&mut *tx, tenant, actor, "redemption.approve", target, Some(json!({"status": status})), Some(after), note_opt).await?;
            release = true;
        } else if four_eyes::needs_two(value, threshold) && first_id.is_none() {
            four_eyes::record_first(&mut tx, "redemptions", "status", id, &status, actor, note_opt, threshold).await?;
            let after = json!({"value": num(value), "status": four_eyes::AWAITING, "fourEyesThreshold": num(threshold)});
            audit::record(&mut *tx, tenant, actor, "redemption.approve_first", target, Some(json!({"status": status})), Some(after), note_opt).await?;
        } else {
            sqlx::query("UPDATE redemptions SET status = 'pending' WHERE id = $1 AND status = $2").bind(id).bind(&status).execute(&mut *tx).await?;
            audit::record(&mut *tx, tenant, actor, "redemption.approve", target, Some(json!({"status": status})), Some(json!({"value": num(value)})), note_opt).await?;
            release = true;
        }
        if release {
            queue_wallet_credit(&mut tx, tenant, user, &format!("growth:redeem:{id}"), "adjustment", value, &format!("redemption:{id}"), &format!("Loyalty reward: {item_name}")).await?;
            sqlx::query("UPDATE redemptions SET decided_by = $2 WHERE id = $1").bind(id).bind(actor.label()).execute(&mut *tx).await?;
        }
    } else {
        lock_user(&mut tx, tenant, user).await?;
        sqlx::query("UPDATE redemptions SET status = 'rejected', error = $2, decided_by = $3, completed_at = now() WHERE id = $1").bind(id).bind(note).bind(actor.label()).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO points_ledger (tenant, user_id, kind, points, ref, description) VALUES ($1,$2,'adjust',$3,$4,$5) ON CONFLICT DO NOTHING")
            .bind(tenant)
            .bind(user)
            .bind(points)
            .bind(format!("redemption:{id}:rejected"))
            .bind(format!("Refund · {item_name} (not approved)"))
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE catalogue SET stock = stock + 1 WHERE id = $1 AND stock IS NOT NULL").bind(r.get::<i64, _>("item_id")).execute(&mut *tx).await?;
        let before = json!({"status": status, "firstApprovedById": first_id, "firstApprovedBy": first_name});
        let after = json!({"status": "rejected", "from": status, "value": num(value), "pointsReturned": points});
        audit::record(&mut *tx, tenant, actor, "redemption.reject", target, Some(before), Some(after), Some(note)).await?;
    }
    tx.commit().await?;
    if release {
        st.wake.notify_one();
        let _ = crate::payouts::wallet_tick(st).await;
    } else if !approve {
        clients::notify(st, tenant, user, "reward.rejected", "Reward not approved".into(), format!("{item_name} wasn't approved; {points} points are back in your balance."), "/rewards/loyalty");
    }
    let r = sqlx::query("SELECT * FROM redemptions WHERE id = $1").bind(id).fetch_one(&st.pool).await?;
    Ok(json!({"redemption": redemption_admin_json(&r, Some(threshold))}))
}
