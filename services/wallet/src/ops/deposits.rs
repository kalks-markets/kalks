//! USDT deposits: intents, client submissions, on-chain verification + confirmations (watcher), scanner for
//! unclaimed transfers, and the staff unmatched / review queue.

use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use sqlx::Row;
use sqlx::postgres::PgRow;

use super::{notify, random_id};
use crate::audit::{self, Entry};
use crate::chain::{ChainId, TxStatus, confirmations};
use crate::error::{ApiError, ApiResult};
use crate::ledger::{self, Leg, NewTxn, PostError};
use crate::money::{D, WALLET_DP, check_amount, floor_dp, s, s_opt};
use crate::settings;
use crate::state::{AppState, StaffCtx};

/// A client-submitted hash that is still not on chain after this long fails.
const NOT_FOUND_TTL_HOURS: i64 = 24;
/// Block time window around an intent for automatic crediting.
const INTENT_EARLY_MIN: i64 = 10;
const INTENT_LATE_HOURS: i64 = 24;

pub fn parse_chain(raw: &str) -> ApiResult<ChainId> {
    ChainId::parse(raw).ok_or_else(|| ApiError::validation("chain", "chain must be bsc or tron"))
}

/* ------------------------------------------------------------------ */
/* Views                                                               */
/* ------------------------------------------------------------------ */

pub fn intent_json(r: &PgRow) -> Value {
    let chain = ChainId::parse(r.get::<String, _>("chain").as_str()).unwrap_or(ChainId::Tron);
    json!({
        "id": r.get::<String, _>("id"),
        "user_id": r.get::<i64, _>("user_id"),
        "chain": chain.as_str(),
        "network": chain.network(),
        "currency": r.get::<String, _>("currency"),
        "amount": s(r.get::<D, _>("amount")),
        "address": r.get::<String, _>("address"),
        "token_contract": r.get::<String, _>("token_contract"),
        "decimals": chain.usdt_decimals(),
        "evm_chain_id": chain.evm_chain_id(),
        "status": r.get::<String, _>("status"),
        "expires_at": r.get::<DateTime<Utc>, _>("expires_at"),
        "created_at": r.get::<DateTime<Utc>, _>("created_at"),
    })
}

pub fn deposit_json(r: &PgRow) -> Value {
    let chain = ChainId::parse(r.get::<String, _>("chain").as_str()).unwrap_or(ChainId::Tron);
    let hash: String = r.get("tx_hash");
    json!({
        "id": r.get::<i64, _>("id"),
        "user_id": r.get::<Option<i64>, _>("user_id"),
        "intent_id": r.get::<Option<String>, _>("intent_id"),
        "chain": chain.as_str(),
        "network": chain.network(),
        "currency": r.get::<String, _>("currency"),
        "tx_hash": hash,
        "explorer_url": chain.explorer_tx(&hash),
        "from_address": r.get::<Option<String>, _>("from_address"),
        "to_address": r.get::<Option<String>, _>("to_address"),
        "amount": s_opt(r.get::<Option<D>, _>("amount")),
        "expected_amount": s_opt(r.get::<Option<D>, _>("expected_amount")),
        "block_number": r.get::<Option<i64>, _>("block_number"),
        "block_time": r.get::<Option<DateTime<Utc>>, _>("block_time"),
        "confirmations": r.get::<i32, _>("confirmations"),
        "required_confirmations": r.get::<i32, _>("required_confirmations"),
        "status": r.get::<String, _>("status"),
        "review_reason": r.get::<Option<String>, _>("review_reason"),
        "failure_reason": r.get::<Option<String>, _>("failure_reason"),
        "source": r.get::<String, _>("source"),
        "suggested_user_id": r.get::<Option<i64>, _>("suggested_user_id"),
        "assigned_by": r.get::<Option<String>, _>("assigned_by"),
        "ledger_txn_id": r.get::<Option<i64>, _>("ledger_txn_id"),
        "credited_at": r.get::<Option<DateTime<Utc>>, _>("credited_at"),
        "created_at": r.get::<DateTime<Utc>, _>("created_at"),
        "updated_at": r.get::<DateTime<Utc>, _>("updated_at"),
    })
}

/* ------------------------------------------------------------------ */
/* Client: intents and submissions                                     */
/* ------------------------------------------------------------------ */

pub async fn create_intent(st: &AppState, tenant_id: i64, user_id: i64, chain_raw: &str, amount: D, ip: Option<&str>) -> ApiResult<Value> {
    if user_id <= 0 {
        return Err(ApiError::validation("user_id", "user_id is required"));
    }
    // deposits disabled in the Back Office (client restrictions)
    crate::users::gate(st, &st.tenants.slug_of(tenant_id).unwrap_or_else(|| "kalks".into()), user_id, "deposits").await?;
    let chain = parse_chain(chain_raw)?;
    let cfg = settings::chain(&st.pool, tenant_id, chain).await?.filter(|c| c.deposits_enabled).ok_or_else(|| ApiError::unprocessable("chain_disabled", format!("Deposits on {} are not available", chain.network())))?;
    let amount = check_amount(amount, WALLET_DP).map_err(|m| ApiError::validation("amount", m))?;
    if amount < cfg.min_deposit {
        return Err(ApiError::unprocessable("below_minimum", format!("The minimum deposit is {} USDT", s(cfg.min_deposit))));
    }
    let open: i64 = sqlx::query_scalar("SELECT count(*) FROM deposit_intents WHERE tenant_id = $1 AND user_id = $2 AND created_at > now() - interval '1 hour'")
        .bind(tenant_id)
        .bind(user_id)
        .fetch_one(&st.pool)
        .await?;
    if open >= 30 {
        return Err(ApiError::Coded { status: axum::http::StatusCode::TOO_MANY_REQUESTS, code: "rate_limited", message: "Too many deposit requests. Please try again later.".into() });
    }
    let limits = settings::limits(&st.pool, tenant_id).await?;
    let id = random_id("dep_");
    let r = sqlx::query(
        "INSERT INTO deposit_intents (id, tenant_id, user_id, chain, currency, amount, address, token_contract, ip, expires_at)
         VALUES ($1, $2, $3, $4, 'USDT', $5, $6, $7, $8, now() + make_interval(mins => $9)) RETURNING *",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(user_id)
    .bind(chain.as_str())
    .bind(amount)
    .bind(&cfg.receiving_address)
    .bind(chain.usdt_contract_display())
    .bind(ip)
    .bind(limits.intent_ttl_minutes)
    .fetch_one(&st.pool)
    .await?;
    let mut v = intent_json(&r);
    v["confirmations"] = json!(cfg.confirmations);
    Ok(v)
}

pub async fn get_intent(st: &AppState, tenant_id: i64, user_id: Option<i64>, id: &str) -> ApiResult<Value> {
    let r = sqlx::query("SELECT * FROM deposit_intents WHERE tenant_id = $1 AND id = $2 AND ($3::bigint IS NULL OR user_id = $3)")
        .bind(tenant_id)
        .bind(id)
        .bind(user_id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| ApiError::not_found("Deposit request"))?;
    let dep = sqlx::query("SELECT * FROM deposits WHERE tenant_id = $1 AND intent_id = $2 ORDER BY id DESC LIMIT 1").bind(tenant_id).bind(id).fetch_optional(&st.pool).await?;
    Ok(json!({"intent": intent_json(&r), "deposit": dep.as_ref().map(deposit_json)}))
}

pub async fn submit(st: &AppState, tenant_id: i64, user_id: i64, intent_id: &str, tx_hash: &str) -> ApiResult<Value> {
    crate::users::gate(st, &st.tenants.slug_of(tenant_id).unwrap_or_else(|| "kalks".into()), user_id, "deposits").await?;
    let intent = sqlx::query("SELECT * FROM deposit_intents WHERE tenant_id = $1 AND id = $2 AND user_id = $3")
        .bind(tenant_id)
        .bind(intent_id.trim())
        .bind(user_id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| ApiError::not_found("Deposit request"))?;
    let chain = parse_chain(intent.get::<String, _>("chain").as_str())?;
    let hash = chain.normalize_hash(tx_hash).ok_or_else(|| ApiError::validation("tx_hash", "Enter the transaction hash (64 hexadecimal characters)"))?;
    let expires: DateTime<Utc> = intent.get("expires_at");
    let expected: D = intent.get("amount");

    // already known on this chain?
    if let Some(existing) = sqlx::query("SELECT * FROM deposits WHERE chain = $1 AND tx_hash = $2").bind(chain.as_str()).bind(&hash).fetch_optional(&st.pool).await? {
        let same_intent = existing.get::<Option<String>, _>("intent_id").as_deref() == Some(intent_id.trim());
        if same_intent {
            return Ok(deposit_json(&existing));
        }
        let unclaimed = existing.get::<String, _>("status") == "unmatched" && existing.get::<Option<i64>, _>("user_id").is_none() && existing.get::<i64, _>("tenant_id") == tenant_id;
        if unclaimed {
            // the scanner saw it first: the client claims it; the auto-credit rules still apply
            let r = sqlx::query(
                "UPDATE deposits SET user_id = $1, intent_id = $2, expected_amount = $3, status = 'confirming', updated_at = now()
                 WHERE id = $4 AND status = 'unmatched' AND user_id IS NULL RETURNING *",
            )
            .bind(user_id)
            .bind(intent_id.trim())
            .bind(expected)
            .bind(existing.get::<i64, _>("id"))
            .fetch_optional(&st.pool)
            .await?;
            if let Some(r) = r {
                sqlx::query("UPDATE deposit_intents SET status = 'submitted' WHERE id = $1 AND status = 'open'").bind(intent_id.trim()).execute(&st.pool).await?;
                return Ok(deposit_json(&r));
            }
        }
        return Err(ApiError::conflict("tx_already_used", "This transaction was already submitted"));
    }
    let status: String = intent.get("status");
    if status != "open" && status != "expired" {
        return Err(ApiError::conflict("invalid_state", "A transaction was already submitted for this deposit request. Start a new deposit."));
    }
    if Utc::now() > expires + Duration::hours(INTENT_LATE_HOURS) {
        return Err(ApiError::unprocessable("intent_expired", "This deposit request has expired. Start a new deposit."));
    }
    let required = settings::chain(&st.pool, tenant_id, chain).await?.map(|c| c.confirmations).unwrap_or(20);
    let r = sqlx::query(
        "INSERT INTO deposits (tenant_id, user_id, intent_id, chain, tx_hash, expected_amount, required_confirmations, status, source)
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending', 'client') ON CONFLICT (chain, tx_hash) DO NOTHING RETURNING *",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(intent_id.trim())
    .bind(chain.as_str())
    .bind(&hash)
    .bind(expected)
    .bind(required)
    .fetch_optional(&st.pool)
    .await?
    .ok_or_else(|| ApiError::conflict("tx_already_used", "This transaction was already submitted"))?;
    sqlx::query("UPDATE deposit_intents SET status = 'submitted' WHERE id = $1").bind(intent_id.trim()).execute(&st.pool).await?;
    tracing::info!(tenant_id, user_id, chain = %chain, tx = %hash, "deposit submitted");
    Ok(deposit_json(&r))
}

/* ------------------------------------------------------------------ */
/* Watcher: verify, count confirmations, credit                        */
/* ------------------------------------------------------------------ */

/// Re-checks one deposit on chain. Safe to call repeatedly; crediting is idempotent (ledger key per tx).
pub async fn process(st: &AppState, id: i64) -> anyhow::Result<()> {
    let Some(d) = sqlx::query("SELECT * FROM deposits WHERE id = $1").bind(id).fetch_optional(&st.pool).await? else { return Ok(()) };
    let status: String = d.get("status");
    if status != "pending" && status != "confirming" {
        return Ok(());
    }
    let tenant_id: i64 = d.get("tenant_id");
    let Some(chain) = ChainId::parse(d.get::<String, _>("chain").as_str()) else { return Ok(()) };
    let Ok(client) = st.chain(chain) else { return Ok(()) };
    let hash: String = d.get("tx_hash");
    let lookup = client.lookup(&hash).await?;
    let since: DateTime<Utc> = d.get::<Option<DateTime<Utc>>, _>("rechecked_at").unwrap_or_else(|| d.get("created_at"));
    match lookup.status {
        TxStatus::NotFound => {
            if Utc::now() - since > Duration::hours(NOT_FOUND_TTL_HOURS) {
                fail(st, id, "Transaction not found on chain").await?;
            } else {
                // not mined yet (or re-orged out): back to pending
                sqlx::query("UPDATE deposits SET status = 'pending', confirmations = 0, checks = checks + 1, last_checked_at = now(), updated_at = now() WHERE id = $1 AND status IN ('pending', 'confirming')")
                    .bind(id)
                    .execute(&st.pool)
                    .await?;
            }
            return Ok(());
        }
        TxStatus::Failed => return fail(st, id, "The transaction failed on chain").await,
        TxStatus::Success => {}
    }
    let receiving = settings::receiving_addresses(&st.pool, tenant_id, chain).await?;
    let paying: Vec<_> = lookup.usdt_to(chain, &receiving).collect();
    let Some(first) = paying.first() else {
        return fail(st, id, "This transaction does not send USDT to the company deposit address").await;
    };
    let amount: D = paying.iter().map(|t| t.amount).sum();
    let block = lookup.block.unwrap_or_default();
    let head = client.head().await?;
    let conf = confirmations(head, block);
    let cfg = settings::chain(&st.pool, tenant_id, chain).await?;
    let required = cfg.as_ref().map(|c| c.confirmations).unwrap_or(d.get("required_confirmations"));
    sqlx::query(
        "UPDATE deposits SET status = 'confirming', from_address = $2, to_address = $3, token_contract = $4, amount = $5, block_number = $6, block_time = $7,
                confirmations = $8, required_confirmations = $9, log_index = $10, checks = checks + 1, last_checked_at = now(), updated_at = now()
         WHERE id = $1 AND status IN ('pending', 'confirming')",
    )
    .bind(id)
    .bind(&first.from)
    .bind(&first.to)
    .bind(&first.token)
    .bind(amount)
    .bind(block)
    .bind(lookup.block_time)
    .bind(conf)
    .bind(required)
    .bind(first.log_index)
    .execute(&st.pool)
    .await?;
    if conf < required {
        return Ok(());
    }
    let Some(user_id) = d.get::<Option<i64>, _>("user_id") else { return Ok(()) };

    // automatic crediting rules (skipped when staff assigned the deposit)
    if d.get::<Option<String>, _>("assigned_by").is_none() {
        let mut reasons: Vec<String> = vec![];
        if let Some(intent_id) = d.get::<Option<String>, _>("intent_id")
            && let Some(intent) = sqlx::query("SELECT amount, created_at, expires_at FROM deposit_intents WHERE id = $1").bind(&intent_id).fetch_optional(&st.pool).await?
        {
            let expected: D = intent.get("amount");
            if floor_dp(amount, WALLET_DP) != expected {
                reasons.push(format!("Amount {} USDT differs from the requested {} USDT", s(amount), s(expected)));
            }
            if let Some(bt) = lookup.block_time {
                let from: DateTime<Utc> = intent.get::<DateTime<Utc>, _>("created_at") - Duration::minutes(INTENT_EARLY_MIN);
                let to: DateTime<Utc> = intent.get::<DateTime<Utc>, _>("expires_at") + Duration::hours(INTENT_LATE_HOURS);
                if bt < from || bt > to {
                    reasons.push("The transaction time is outside the deposit request window".into());
                }
            }
        } else {
            reasons.push("No deposit request".into());
        }
        let other: Option<i64> = sqlx::query_scalar(
            "SELECT user_id FROM deposits WHERE tenant_id = $1 AND chain = $2 AND from_address = $3 AND user_id IS NOT NULL AND user_id <> $4 AND status = 'credited' LIMIT 1",
        )
        .bind(tenant_id)
        .bind(chain.as_str())
        .bind(&first.from)
        .bind(user_id)
        .fetch_optional(&st.pool)
        .await?;
        if let Some(o) = other {
            reasons.push(format!("The sender address was used by another client (#{o})"));
        }
        if let Some(c) = &cfg
            && amount < c.min_deposit
        {
            reasons.push(format!("Below the minimum deposit of {} USDT", s(c.min_deposit)));
        }
        if !reasons.is_empty() {
            let reason = reasons.join("; ");
            sqlx::query("UPDATE deposits SET status = 'review', review_reason = $2, updated_at = now() WHERE id = $1 AND status = 'confirming'").bind(id).bind(&reason).execute(&st.pool).await?;
            tracing::warn!(deposit = id, %reason, "deposit held for review");
            return Ok(());
        }
    }
    credit(st, id).await
}

async fn fail(st: &AppState, id: i64, reason: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE deposits SET status = 'failed', failure_reason = $2, checks = checks + 1, last_checked_at = now(), updated_at = now() WHERE id = $1 AND status IN ('pending', 'confirming')")
        .bind(id)
        .bind(reason)
        .execute(&st.pool)
        .await?;
    tracing::warn!(deposit = id, reason, "deposit failed");
    Ok(())
}

/// Credits a confirmed deposit (status confirming, user set, amount known) in one database transaction.
pub async fn credit(st: &AppState, id: i64) -> anyhow::Result<()> {
    let mut tx = st.pool.begin().await?;
    let Some(d) = sqlx::query("SELECT * FROM deposits WHERE id = $1 AND status = 'confirming' FOR UPDATE").bind(id).fetch_optional(&mut *tx).await? else { return Ok(()) };
    let (Some(user_id), Some(amount)) = (d.get::<Option<i64>, _>("user_id"), d.get::<Option<D>, _>("amount")) else { return Ok(()) };
    let tenant_id: i64 = d.get("tenant_id");
    let chain: String = d.get("chain");
    let hash: String = d.get("tx_hash");
    let credit = floor_dp(amount, WALLET_DP);
    if credit <= D::ZERO {
        return Ok(());
    }
    let ccy: String = d.get("currency");
    let txn = match ledger::post(
        &mut tx,
        NewTxn {
            tenant_id,
            key: format!("deposit:{chain}:{hash}"),
            kind: "deposit".into(),
            user_id: Some(user_id),
            currency: ccy.clone(),
            reference: Some(hash.clone()),
            note: Some(format!("USDT deposit · {}", ChainId::parse(&chain).map(|c| c.network()).unwrap_or(""))),
            actor: d.get::<Option<String>, _>("assigned_by").unwrap_or_else(|| "system".into()),
            request: None,
            legs: vec![Leg::available(user_id, &ccy, credit), Leg::sys(&ccy, &format!("deposit:{chain}"), -credit)],
        },
    )
    .await
    {
        Ok(t) => t,
        Err(PostError::Duplicate) => {
            // booked by an earlier run that failed before updating the row: link it
            drop(tx);
            let t: Option<i64> = sqlx::query_scalar("SELECT id FROM ledger_txns WHERE tenant_id = $1 AND idempotency_key = $2").bind(tenant_id).bind(format!("deposit:{chain}:{hash}")).fetch_optional(&st.pool).await?;
            sqlx::query("UPDATE deposits SET status = 'credited', ledger_txn_id = $2, credited_at = COALESCE(credited_at, now()), updated_at = now() WHERE id = $1").bind(id).bind(t).execute(&st.pool).await?;
            if let Some(t) = t {
                crate::ib::record(&st.pool, t).await?;
            }
            return Ok(());
        }
        Err(PostError::Insufficient) => anyhow::bail!("unexpected insufficient funds on a deposit credit"),
        Err(PostError::Db(e)) => return Err(e),
    };
    sqlx::query("UPDATE deposits SET status = 'credited', ledger_txn_id = $2, credited_at = now(), updated_at = now() WHERE id = $1").bind(id).bind(txn).execute(&mut *tx).await?;
    if let Some(intent) = d.get::<Option<String>, _>("intent_id") {
        sqlx::query("UPDATE deposit_intents SET status = 'completed' WHERE id = $1").bind(intent).execute(&mut *tx).await?;
    }
    // real money in: reported to the IB service after the commit (first deposit / CPA)
    crate::ib::record(&mut *tx, txn).await?;
    notify(&mut tx, tenant_id, user_id, "deposit.credited", "Deposit credited", &format!("{} USDT was credited to your wallet.", s(credit)), json!({"deposit_id": id, "tx_hash": hash, "chain": chain})).await?;
    tx.commit().await?;
    tracing::info!(deposit = id, tenant_id, user_id, amount = %credit, %chain, "deposit credited");
    Ok(())
}

/// Expires open intents past their expiry.
pub async fn expire_intents(st: &AppState) -> anyhow::Result<u64> {
    Ok(sqlx::query("UPDATE deposit_intents SET status = 'expired' WHERE status = 'open' AND expires_at < now()").execute(&st.pool).await?.rows_affected())
}

/* ------------------------------------------------------------------ */
/* Scanner: incoming transfers nobody claimed                          */
/* ------------------------------------------------------------------ */

/// Scans the tenant's active receiving address on `chain` and records unknown incoming USDT transfers as
/// `unmatched` (with a suggested client by sender address). A transfer whose sender belongs to a client with
/// an open deposit request of exactly that amount is claimed for that request (still subject to the
/// auto-credit rules). Returns the number of new deposits.
pub async fn scan(st: &AppState, tenant_id: i64, chain: ChainId) -> anyhow::Result<usize> {
    let Ok(client) = st.chain(chain) else { return Ok(0) };
    let Some(cfg) = settings::chain(&st.pool, tenant_id, chain).await? else { return Ok(0) };
    let address = cfg.receiving_address.clone();
    let cursor: Option<i64> = sqlx::query_scalar("SELECT position FROM chain_cursors WHERE tenant_id = $1 AND chain = $2 AND address = $3")
        .bind(tenant_id)
        .bind(chain.as_str())
        .bind(&address)
        .fetch_optional(&st.pool)
        .await?;
    let (items, next) = client.incoming(&address, cursor).await?;
    let mut added = 0;
    for it in items {
        let Some(hash) = chain.normalize_hash(&it.tx_hash) else { continue };
        let suggested: Option<i64> = sqlx::query_scalar(
            "SELECT user_id FROM deposits WHERE tenant_id = $1 AND chain = $2 AND from_address = $3 AND user_id IS NOT NULL ORDER BY id DESC LIMIT 1",
        )
        .bind(tenant_id)
        .bind(chain.as_str())
        .bind(&it.from)
        .fetch_optional(&st.pool)
        .await?;
        // an open request of the same client and amount → claim it
        let intent: Option<(String, i64, D)> = match suggested {
            Some(uid) => sqlx::query_as(
                "SELECT id, user_id, amount FROM deposit_intents WHERE tenant_id = $1 AND user_id = $2 AND chain = $3 AND status = 'open' AND amount = $4
                   AND expires_at + interval '24 hours' > now() ORDER BY created_at DESC LIMIT 1",
            )
            .bind(tenant_id)
            .bind(uid)
            .bind(chain.as_str())
            .bind(floor_dp(it.amount, WALLET_DP))
            .fetch_optional(&st.pool)
            .await?,
            None => None,
        };
        let (status, user_id, intent_id, expected) = match &intent {
            Some((iid, uid, amt)) => ("confirming", Some(*uid), Some(iid.clone()), Some(*amt)),
            None => ("unmatched", None, None, None),
        };
        let r = sqlx::query(
            "INSERT INTO deposits (tenant_id, user_id, intent_id, chain, tx_hash, log_index, from_address, to_address, token_contract, amount, expected_amount,
                                   block_number, block_time, required_confirmations, status, source, suggested_user_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, 'scanner', $16) ON CONFLICT (chain, tx_hash) DO NOTHING",
        )
        .bind(tenant_id)
        .bind(user_id)
        .bind(&intent_id)
        .bind(chain.as_str())
        .bind(&hash)
        .bind(it.log_index)
        .bind(&it.from)
        .bind(&it.to)
        .bind(chain.usdt_contract())
        .bind(it.amount)
        .bind(expected)
        .bind(it.block)
        .bind(it.time)
        .bind(cfg.confirmations)
        .bind(status)
        .bind(suggested)
        .execute(&st.pool)
        .await?;
        if r.rows_affected() == 1 {
            added += 1;
            if let Some(iid) = &intent_id {
                sqlx::query("UPDATE deposit_intents SET status = 'submitted' WHERE id = $1 AND status = 'open'").bind(iid).execute(&st.pool).await?;
            }
            tracing::info!(tenant_id, %chain, tx = %hash, amount = %it.amount, claimed = intent_id.is_some(), "incoming transfer detected");
        }
    }
    sqlx::query(
        "INSERT INTO chain_cursors (tenant_id, chain, address, position) VALUES ($1, $2, $3, $4)
         ON CONFLICT (tenant_id, chain, address) DO UPDATE SET position = EXCLUDED.position, updated_at = now()",
    )
    .bind(tenant_id)
    .bind(chain.as_str())
    .bind(&address)
    .bind(next)
    .execute(&st.pool)
    .await?;
    Ok(added)
}

/* ------------------------------------------------------------------ */
/* Staff: unmatched / review queue                                     */
/* ------------------------------------------------------------------ */

/// Assigns an unmatched deposit (or approves a held one) for `user_id`. It is credited as soon as it has the
/// required confirmations (at once if it already has them); the auto-credit rules are skipped.
pub async fn assign(st: &AppState, s_ctx: &StaffCtx, id: i64, user_id: i64, reason: &str) -> ApiResult<Value> {
    let tenant_id = s_ctx.ctx.tenant.id;
    if user_id <= 0 {
        return Err(ApiError::validation("user_id", "Choose the client"));
    }
    let reason = reason.trim();
    if reason.len() < 3 {
        return Err(ApiError::validation("reason", "Enter a reason"));
    }
    let mut tx = st.pool.begin().await?;
    let d = sqlx::query("SELECT * FROM deposits WHERE id = $1 AND tenant_id = $2 FOR UPDATE").bind(id).bind(tenant_id).fetch_optional(&mut *tx).await?.ok_or_else(|| ApiError::not_found("Deposit"))?;
    let status: String = d.get("status");
    if status != "unmatched" && status != "review" {
        return Err(ApiError::conflict("invalid_state", format!("A {status} deposit can't be assigned")));
    }
    if status == "review" && d.get::<Option<i64>, _>("user_id").is_some_and(|u| u != user_id) {
        return Err(ApiError::conflict("invalid_state", "This deposit belongs to another client; reject it instead"));
    }
    let before = deposit_json(&d);
    let r = sqlx::query("UPDATE deposits SET user_id = $2, status = 'confirming', assigned_by = $3, review_reason = NULL, updated_at = now() WHERE id = $1 RETURNING *")
        .bind(id)
        .bind(user_id)
        .bind(s_ctx.staff.tag())
        .fetch_one(&mut *tx)
        .await?;
    audit::staff(&mut tx, s_ctx, Entry { action: if status == "unmatched" { "wallet.deposit.assigned" } else { "wallet.deposit.approved" }, target_kind: "deposit", target_id: id.to_string(), reason: Some(reason), before: Some(before), after: Some(deposit_json(&r)) }).await?;
    tx.commit().await?;
    // verify on chain now; credits immediately when already confirmed
    if let Err(e) = process(st, id).await {
        tracing::warn!(deposit = id, error = %e, "post-assign check failed; the watcher retries");
    }
    let r = sqlx::query("SELECT * FROM deposits WHERE id = $1").bind(id).fetch_one(&st.pool).await?;
    Ok(deposit_json(&r))
}

pub async fn reject(st: &AppState, s_ctx: &StaffCtx, id: i64, reason: &str) -> ApiResult<Value> {
    let reason = reason.trim();
    if reason.len() < 3 {
        return Err(ApiError::validation("reason", "Enter a reason"));
    }
    let mut tx = st.pool.begin().await?;
    let d = sqlx::query("SELECT * FROM deposits WHERE id = $1 AND tenant_id = $2 FOR UPDATE").bind(id).bind(s_ctx.ctx.tenant.id).fetch_optional(&mut *tx).await?.ok_or_else(|| ApiError::not_found("Deposit"))?;
    let status: String = d.get("status");
    if !matches!(status.as_str(), "unmatched" | "review" | "pending" | "failed") {
        return Err(ApiError::conflict("invalid_state", format!("A {status} deposit can't be rejected")));
    }
    let r = sqlx::query("UPDATE deposits SET status = 'rejected', failure_reason = $2, updated_at = now() WHERE id = $1 RETURNING *").bind(id).bind(reason).fetch_one(&mut *tx).await?;
    audit::staff(&mut tx, s_ctx, Entry { action: "wallet.deposit.rejected", target_kind: "deposit", target_id: id.to_string(), reason: Some(reason), before: Some(deposit_json(&d)), after: Some(deposit_json(&r)) }).await?;
    if let Some(uid) = r.get::<Option<i64>, _>("user_id") {
        notify(&mut tx, s_ctx.ctx.tenant.id, uid, "deposit.rejected", "Deposit not credited", &format!("Your deposit {} was not credited: {reason}. Contact support if you need help.", crate::chain::ChainId::parse(r.get::<String, _>("chain").as_str()).map(|c| c.network()).unwrap_or("")), json!({"deposit_id": id})).await?;
    }
    tx.commit().await?;
    Ok(deposit_json(&r))
}

/// Sends a failed / pending deposit back to verification (e.g. after an RPC outage).
pub async fn recheck(st: &AppState, s_ctx: &StaffCtx, id: i64) -> ApiResult<Value> {
    let mut tx = st.pool.begin().await?;
    let d = sqlx::query("SELECT * FROM deposits WHERE id = $1 AND tenant_id = $2 FOR UPDATE").bind(id).bind(s_ctx.ctx.tenant.id).fetch_optional(&mut *tx).await?.ok_or_else(|| ApiError::not_found("Deposit"))?;
    let status: String = d.get("status");
    if !matches!(status.as_str(), "failed" | "pending" | "confirming") {
        return Err(ApiError::conflict("invalid_state", format!("A {status} deposit can't be re-checked")));
    }
    if d.get::<Option<i64>, _>("user_id").is_none() {
        return Err(ApiError::conflict("invalid_state", "Assign the deposit to a client first"));
    }
    sqlx::query("UPDATE deposits SET status = 'pending', failure_reason = NULL, rechecked_at = now(), updated_at = now() WHERE id = $1").bind(id).execute(&mut *tx).await?;
    audit::staff(&mut tx, s_ctx, Entry { action: "wallet.deposit.recheck", target_kind: "deposit", target_id: id.to_string(), reason: None, before: Some(json!({"status": status})), after: Some(json!({"status": "pending"})) }).await?;
    tx.commit().await?;
    if let Err(e) = process(st, id).await {
        tracing::warn!(deposit = id, error = %e, "recheck failed; the watcher retries");
    }
    let r = sqlx::query("SELECT * FROM deposits WHERE id = $1").bind(id).fetch_one(&st.pool).await?;
    Ok(deposit_json(&r))
}
