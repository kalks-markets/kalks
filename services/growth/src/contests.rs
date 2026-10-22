//! Trading contests (D135): join (dedicated demo account or a chosen live account), live scores from engine
//! deals + floating P&L, ranks, anti-cheat flags, finalize and prize payout (wallet or trading credit).
//!
//! Options contests (O36, `instrument = 'options'`): only Kalks FX Options exits count (manual closes, stop-outs,
//! expiry settlements, knock-outs), scored on realised option P&L (no floating part), volume in contracts. Open
//! to clients who may trade options (gateway suitability) on accounts outside the system groups. Trades whose
//! opening premium is below the contest minimum add no volume or trade count; self-trades between a client's own
//! accounts (one order-book fill on both, an opposite position in the same series at the same time, or a cross at
//! the same moment) don't count at all and raise a `self_trade` flag. CFD contests are unchanged and never see
//! option deals.

use crate::audit::{self, Actor};
use crate::bonus::REASON_PRIZE;
use crate::calc::{self, RankInput};
use crate::clients::{self, EngineOutcome};
use crate::error::{ApiError, ApiResult};
use crate::four_eyes::{self, AWAITING};
use crate::loyalty;
use crate::model::{AntiCheat, Prize, contest_status, display_name, opt_num, prize_pool};
use crate::money::{D, ZERO, num};
use crate::profiles::Profile;
use crate::state::AppState;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::Row;

pub fn prizes_of(r: &sqlx::postgres::PgRow) -> Vec<Prize> {
    r.get::<sqlx::types::Json<Vec<Prize>>, _>("prizes").0
}

pub fn anti_of(r: &sqlx::postgres::PgRow) -> AntiCheat {
    serde_json::from_value(r.get::<sqlx::types::Json<Value>, _>("anti_cheat").0).unwrap_or_default()
}

pub fn status_of(r: &sqlx::postgres::PgRow) -> String {
    contest_status(r.get::<&str, _>("status"), r.get("starts_at"), r.get("ends_at"), Utc::now())
}

/// Contest row (+ `entrants` column) → JSON.
pub fn contest_json(r: &sqlx::postgres::PgRow) -> Value {
    let prizes = prizes_of(r);
    json!({
        "id": r.get::<i64, _>("id"),
        "slug": r.get::<String, _>("slug"),
        "name": r.get::<String, _>("name"),
        "description": r.get::<String, _>("description"),
        "kind": r.get::<String, _>("kind"),
        "instrument": r.get::<String, _>("instrument"),
        "minPremium": opt_num(r.get("min_premium")),
        "status": status_of(r),
        "startsAt": r.get::<DateTime<Utc>, _>("starts_at"),
        "endsAt": r.get::<DateTime<Utc>, _>("ends_at"),
        "scoring": r.get::<String, _>("scoring"),
        "minTrades": r.get::<i32, _>("min_trades"),
        "maxEntrants": r.get::<Option<i32>, _>("max_entrants"),
        "entrants": r.try_get::<i64, _>("entrants").unwrap_or(0),
        "startingBalance": opt_num(r.get("starting_balance")),
        "demoGroup": r.get::<Option<String>, _>("demo_group"),
        "accountGroups": r.get::<Vec<String>, _>("account_groups"),
        "kycRequired": r.get::<bool, _>("kyc_required"),
        "minEquity": opt_num(r.get("min_equity")),
        "prizes": prizes,
        "prizePool": num(prize_pool(&prizes)),
        "rules": r.get::<String, _>("rules"),
        "antiCheat": anti_of(r),
        "assetClasses": r.try_get::<Vec<String>, _>("asset_classes").unwrap_or_default(),
        "templateId": r.try_get::<Option<i64>, _>("template_id").ok().flatten(),
        "clonedFrom": r.try_get::<Option<i64>, _>("cloned_from").ok().flatten(),
        "refreshedAt": r.get::<Option<DateTime<Utc>>, _>("refreshed_at"),
        "finalizedAt": r.get::<Option<DateTime<Utc>>, _>("finalized_at"),
        "paidAt": r.get::<Option<DateTime<Utc>>, _>("paid_at"),
        "createdAt": r.get::<DateTime<Utc>, _>("created_at"),
        "updatedAt": r.get::<DateTime<Utc>, _>("updated_at"),
    })
}

pub const CONTEST_SELECT: &str = "SELECT c.*, (SELECT count(*) FROM contest_entries e WHERE e.contest_id = c.id) AS entrants FROM contests c";

/// Entry row → Standing. `me` = the viewing client (their own login and id are shown), `staff` shows all.
pub fn standing_json(r: &sqlx::postgres::PgRow, me: Option<i64>, staff: bool, min_trades: i32) -> Value {
    let uid: i64 = r.get("user_id");
    let mine = me == Some(uid);
    let show = mine || staff;
    let trades: i32 = r.get("trades");
    json!({
        "entryId": r.get::<i64, _>("id"),
        "userId": if show { json!(uid) } else { Value::Null },
        "login": if show { json!(r.get::<i64, _>("login")) } else { Value::Null },
        "name": r.get::<String, _>("display_name"),
        "country": r.get::<String, _>("country"),
        "rank": r.get::<Option<i32>, _>("rank"),
        "score": num(r.get("score")),
        "returnPct": num(r.get("return_pct")),
        "profit": num(r.get::<D, _>("realised") + r.get::<D, _>("floating")),
        "lots": num(r.get("lots")),
        "contracts": num(r.get("contracts")),
        "trades": trades,
        // options contests: trades left out of the score (own entry and staff only)
        "selfTrades": if show { json!(r.get::<i32, _>("self_trades")) } else { Value::Null },
        "smallTrades": if show { json!(r.get::<i32, _>("small_trades")) } else { Value::Null },
        "qualified": trades >= min_trades,
        "status": r.get::<String, _>("status"),
        "disqualifyReason": if show { json!(r.get::<Option<String>, _>("disqualify_reason")) } else { Value::Null },
        "startEquity": if show { opt_num(r.get("start_equity")) } else { Value::Null },
        "prize": opt_num(r.get("prize_amount")),
        "prizePayout": r.get::<Option<String>, _>("prize_payout"),
        // clients see a prize waiting for its second approval as waiting for approval
        "prizeStatus": if staff { json!(r.get::<String, _>("prize_status")) } else { json!(client_prize_status(r.get::<&str, _>("prize_status"))) },
        "fourEyes": if staff { four_eyes::json_of(r, r.get("prize_amount"), r.get::<&str, _>("prize_status"), None) } else { Value::Null },
        "me": mine,
        "joinedAt": r.get::<DateTime<Utc>, _>("joined_at"),
        "updatedAt": r.get::<DateTime<Utc>, _>("updated_at"),
    })
}

pub fn client_prize_status(s: &str) -> &str {
    if s == four_eyes::AWAITING { "approval" } else { s }
}

pub const ORDER: &str = "ORDER BY (rank IS NULL), rank, joined_at, id";

pub async fn leaderboard(st: &AppState, contest_id: i64, me: Option<i64>, staff: bool, limit: i64) -> anyhow::Result<(Vec<Value>, Option<Value>)> {
    let min: i32 = sqlx::query_scalar("SELECT min_trades FROM contests WHERE id = $1").bind(contest_id).fetch_one(&st.pool).await?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT * FROM contest_entries WHERE contest_id = $1 {ORDER} LIMIT $2"))).bind(contest_id).bind(limit).fetch_all(&st.pool).await?;
    let mut list: Vec<Value> = rows.iter().map(|r| standing_json(r, me, staff, min)).collect();
    let mine_row = match me {
        Some(u) => sqlx::query("SELECT * FROM contest_entries WHERE contest_id = $1 AND user_id = $2").bind(contest_id).bind(u).fetch_optional(&st.pool).await?,
        None => None,
    };
    let mut mine = mine_row.as_ref().map(|r| standing_json(r, me, staff, min));
    if !staff {
        // clients see nicknames and level badges, never another client's name or country (phase 4)
        let tenant: String = sqlx::query_scalar("SELECT tenant FROM contests WHERE id = $1").bind(contest_id).fetch_one(&st.pool).await?;
        let mut users: Vec<i64> = rows.iter().map(|r| r.get::<i64, _>("user_id")).collect();
        users.extend(mine_row.iter().map(|r| r.get::<i64, _>("user_id")));
        let nicks = crate::leaderboard::many(&st.pool, &tenant, &users).await?;
        let xps = crate::xp::totals(&st.pool, &tenant, &users).await?;
        let levels = crate::xp::levels(&st.pool, &tenant).await?;
        let patch = |v: &mut Value, uid: i64| {
            v["name"] = json!(nicks.get(&uid).map(|n| n.nickname.clone()).unwrap_or_else(|| "Trader".into()));
            v["level"] = crate::xp::badge_json(&levels, *xps.get(&uid).unwrap_or(&0));
            if me != Some(uid) {
                v["country"] = Value::Null;
            }
        };
        for (v, r) in list.iter_mut().zip(rows.iter()) {
            patch(v, r.get("user_id"));
        }
        if let (Some(v), Some(r)) = (mine.as_mut(), mine_row.as_ref()) {
            patch(v, r.get("user_id"));
        }
    }
    Ok((list, mine))
}

/// Joins a contest. Demo: opens a dedicated demo account (credentials returned once). Live: the chosen account.
pub async fn join(st: &AppState, tenant: &str, user_id: i64, contest_id: i64, login: Option<i64>, profile: &Profile) -> ApiResult<Value> {
    let c = sqlx::query(sqlx::AssertSqlSafe(format!("{CONTEST_SELECT} WHERE c.id = $1 AND c.tenant = $2"))).bind(contest_id).bind(tenant).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    let status = status_of(&c);
    if status != "scheduled" && status != "running" {
        return Err(ApiError::Conflict { code: "contest_closed", message: "Registration for this contest is closed.".into() });
    }
    if c.get::<Option<i32>, _>("max_entrants").is_some_and(|m| c.get::<i64, _>("entrants") >= m as i64) {
        return Err(ApiError::Conflict { code: "contest_closed", message: "This contest is full.".into() });
    }
    if c.get::<bool, _>("kyc_required") && profile.kyc != "verified" {
        return Err(ApiError::Conflict { code: "not_eligible", message: "Verify your identity to join this contest.".into() });
    }
    let joined: Option<i64> = sqlx::query_scalar("SELECT id FROM contest_entries WHERE contest_id = $1 AND user_id = $2").bind(contest_id).bind(user_id).fetch_optional(&st.pool).await?;
    if joined.is_some() {
        return Err(ApiError::Conflict { code: "already_joined", message: "You have already joined this contest.".into() });
    }
    let kind: String = c.get("kind");
    let name: String = c.get("name");
    let options = c.get::<String, _>("instrument") == "options";
    if options {
        // only clients who may trade Kalks FX Options (the options intro accepted), checked before any account opens
        match clients::options_eligible(st, tenant, user_id).await {
            Ok(true) => {}
            Ok(false) => {
                return Err(ApiError::Conflict {
                    code: "options_intro_required",
                    message: "Options contests are for clients who can trade Kalks FX Options. Take the 1-minute options intro in the Client Area first.".into(),
                });
            }
            Err(e) => return Err(ApiError::Unavailable(format!("Options eligibility can't be checked right now ({e}). Please try again shortly."))),
        }
    }
    let running = status == "running";
    let (acc_login, start_equity, credentials) = if kind == "demo" {
        let balance: D = c.get::<Option<D>, _>("starting_balance").unwrap_or(D::from(10_000));
        let group = c.get::<Option<String>, _>("demo_group").unwrap_or_else(|| "standard".into());
        let (acc, creds) = clients::open_demo(st, tenant, user_id, &group, balance, &format!("Contest · {name}").chars().take(60).collect::<String>())
            .await
            .map_err(|m| ApiError::Conflict { code: "not_eligible", message: m })?;
        crate::deals::cache_account(st, tenant, &acc).await?;
        let mut creds = creds;
        creds["login"] = json!(acc.login);
        (acc.login, Some(acc.equity_ex_options_usd().max(acc.usd(acc.balance))), Some(creds))
    } else {
        let l = login.ok_or_else(|| crate::error::invalid("login", "Choose the live account to compete with."))?;
        let acc = clients::account(st, tenant, l).await.map_err(|e| ApiError::Unavailable(e.to_string()))?.ok_or_else(|| crate::error::invalid("login", "Account not found."))?;
        if acc.user_id != user_id || acc.kind != "live" {
            return Err(crate::error::invalid("login", "Choose one of your live accounts."));
        }
        let groups: Vec<String> = c.get("account_groups");
        if !groups.is_empty() && !groups.iter().any(|g| g.eq_ignore_ascii_case(&acc.group)) {
            return Err(crate::error::invalid("login", "This contest isn't open to that account type."));
        }
        if options && calc::options_system_group(&acc.group) {
            return Err(crate::error::invalid("login", "Options aren't available on copy-trading, PAMM, MAM or prop accounts. Choose another live account."));
        }
        let eq = acc.equity_ex_options_usd();
        if c.get::<Option<D>, _>("min_equity").is_some_and(|m| eq < m) {
            return Err(crate::error::invalid("login", format!("The account needs at least ${} equity.", c.get::<D, _>("min_equity").normalize())));
        }
        crate::deals::cache_account(st, tenant, &acc).await?;
        (acc.login, running.then_some(eq), None)
    };
    let mut tx = st.pool.begin().await?;
    let c2 = sqlx::query("SELECT max_entrants FROM contests WHERE id = $1 FOR UPDATE").bind(contest_id).fetch_one(&mut *tx).await?;
    if let Some(m) = c2.get::<Option<i32>, _>("max_entrants") {
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM contest_entries WHERE contest_id = $1").bind(contest_id).fetch_one(&mut *tx).await?;
        if n >= m as i64 {
            return Err(ApiError::Conflict { code: "contest_closed", message: "This contest is full.".into() });
        }
    }
    let r = sqlx::query(
        "INSERT INTO contest_entries (contest_id, tenant, user_id, login, display_name, country, start_equity) VALUES ($1,$2,$3,$4,$5,$6,$7)
         ON CONFLICT DO NOTHING RETURNING *",
    )
    .bind(contest_id)
    .bind(tenant)
    .bind(user_id)
    .bind(acc_login)
    .bind(display_name(&profile.first_name, &profile.last_name))
    .bind(&profile.country)
    .bind(start_equity)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(ApiError::Conflict { code: "already_joined", message: "You have already joined this contest (or that account is entered).".into() })?;
    audit::record(&mut *tx, tenant, &Actor { id: format!("user:{user_id}"), name: None }, "contest.join", Some(format!("contest:{contest_id}")), None, Some(json!({"login": acc_login})), None).await?;
    tx.commit().await?;
    // phase 4: XP for entering a live contest (once per contest), the nickname others will see, join missions
    if kind == "live" {
        let s = crate::db::settings(&st.pool, tenant).await?;
        crate::xp::award(st, tenant, user_id, s.xp_contest_join, "contest", &format!("join:{contest_id}"), &format!("Joined {name}")).await?;
    }
    crate::leaderboard::ensure(&st.pool, tenant, user_id).await?;
    crate::missions::evaluate_soon(st, tenant, user_id);
    let mut out = json!({"entry": standing_json(&r, Some(user_id), false, c.get("min_trades"))});
    if let Some(cr) = credentials {
        out["credentials"] = cr;
    }
    Ok(out)
}

/// Recomputes one contest: start snapshots, realised / floating P&L, scores, flags and ranks.
pub async fn refresh(st: &AppState, contest_id: i64) -> anyhow::Result<()> {
    let c = sqlx::query("SELECT * FROM contests WHERE id = $1").bind(contest_id).fetch_one(&st.pool).await?;
    let status = status_of(&c);
    if !matches!(status.as_str(), "running" | "ended") {
        return Ok(());
    }
    let tenant: String = c.get("tenant");
    let starts: DateTime<Utc> = c.get("starts_at");
    let ends: DateTime<Utc> = c.get("ends_at");
    let scoring: String = c.get("scoring");
    let min_trades: i32 = c.get("min_trades");
    let options = c.get::<String, _>("instrument") == "options";
    let min_premium: Option<D> = c.get("min_premium");
    let account_kind: String = c.get("kind");
    let ac = anti_of(&c);
    let live_now = Utc::now() < ends;
    let entries = sqlx::query("SELECT * FROM contest_entries WHERE contest_id = $1").bind(contest_id).fetch_all(&st.pool).await?;
    let mut rank_in = Vec::with_capacity(entries.len());
    for e in &entries {
        let id: i64 = e.get("id");
        let login: i64 = e.get("login");
        let user: i64 = e.get("user_id");
        let mut start_eq: Option<D> = e.get("start_equity");
        let mut floating: D = e.get("floating");
        // options contests score realised option P&L only: the account is read just for the start snapshot
        if live_now && (!options || start_eq.is_none()) {
            match clients::account(st, &tenant, login).await {
                Ok(Some(acc)) => {
                    // CFD contests: CFD floating only (options are excluded from CFD contests, O34)
                    if !options {
                        floating = acc.floating_usd();
                    }
                    if start_eq.is_none() {
                        start_eq = Some(acc.equity_ex_options_usd());
                    }
                }
                Ok(None) => {}
                Err(e) => tracing::debug!(error = %e, login, "contest account read failed"),
            }
        }
        if options {
            floating = ZERO;
            classify_option_trades(st, &tenant, id, user, login, &account_kind, starts, !live_now, min_premium).await?;
        }
        let rows = sqlx::query("SELECT deal_id, profit, lots, contracts, hold_seconds, excluded FROM contest_trades WHERE entry_id = $1").bind(id).fetch_all(&st.pool).await?;
        let excluded = |t: &sqlx::postgres::PgRow| t.get::<Option<String>, _>("excluded");
        // P&L: every trade but self-trades; volume and trade count: only trades that fully count
        let pnl_rows: Vec<&sqlx::postgres::PgRow> = rows.iter().filter(|t| excluded(t).as_deref() != Some("self_trade")).collect();
        let counted: Vec<&sqlx::postgres::PgRow> = rows.iter().filter(|t| excluded(t).is_none()).collect();
        let self_deals: Vec<i64> = rows.iter().filter(|t| excluded(t).as_deref() == Some("self_trade")).map(|t| t.get::<i64, _>("deal_id")).collect();
        let small = rows.iter().filter(|t| excluded(t).as_deref() == Some("min_premium")).count();
        let realised: D = pnl_rows.iter().map(|t| t.get::<D, _>("profit")).sum();
        let lots: D = counted.iter().map(|t| t.get::<D, _>("lots")).sum();
        let contracts: D = counted.iter().map(|t| t.get::<D, _>("contracts")).sum();
        let (score, ret) = calc::contest_score(&scoring, realised, floating, if options { contracts } else { lots }, start_eq);
        sqlx::query(
            "UPDATE contest_entries SET start_equity = $2, realised = $3, floating = $4, lots = $5, trades = $6, score = $7, return_pct = $8, contracts = $9,
               self_trades = $10, small_trades = $11, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(start_eq)
        .bind(realised)
        .bind(floating)
        .bind(lots)
        .bind(counted.len() as i32)
        .bind(score)
        .bind(ret)
        .bind(contracts)
        .bind(self_deals.len() as i32)
        .bind(small as i32)
        .execute(&st.pool)
        .await?;
        // anti-cheat
        let facts: Vec<(D, i64)> = pnl_rows.iter().map(|t| (t.get::<D, _>("profit"), t.get::<i64, _>("hold_seconds"))).collect();
        for (kind, details) in calc::trade_flags(&facts, &ac) {
            add_flag(st, contest_id, id, kind, "medium", details).await?;
        }
        if !self_deals.is_empty() {
            let shown: Vec<i64> = self_deals.iter().take(20).copied().collect();
            add_flag(st, contest_id, id, "self_trade", "high", json!({"trades": self_deals.len(), "deals": shown})).await?;
        }
        let mut disqualified = e.get::<String, _>("status") == "disqualified";
        if live_now && !disqualified {
            if let Ok(ledger) = clients::ledger(st, &tenant, login, user, starts).await {
                let changes: Vec<&clients::LedgerItem> = ledger
                    .iter()
                    .filter(|l| l.at >= starts && l.at < ends && l.sub_ledger == "balance" && calc::is_balance_change(&l.kind) && !l.reason_code.as_deref().is_some_and(|r| r.starts_with("GRW-")))
                    .collect();
                if let Some(first) = changes.first() {
                    let fresh = add_flag(st, contest_id, id, "balance_change", "high", json!({"kind": first.kind, "amount": num(first.amount), "at": first.at, "count": changes.len()})).await?;
                    if fresh && ac.disqualify_on_balance_change {
                        sqlx::query("UPDATE contest_entries SET status = 'disqualified', disqualify_reason = 'Balance changed during the contest (deposit, withdrawal or refill)' WHERE id = $1").bind(id).execute(&st.pool).await?;
                        sqlx::query("UPDATE contest_flags SET status = 'disqualified', resolved_by = 'system', resolved_at = now() WHERE entry_id = $1 AND kind = 'balance_change'").bind(id).execute(&st.pool).await?;
                        disqualified = true;
                    }
                }
            }
        }
        rank_in.push(RankInput { entry_id: id, score, trades: counted.len() as i64, disqualified, joined_at: e.get("joined_at") });
    }
    for (id, rank, _) in calc::rank(&rank_in, min_trades as i64) {
        sqlx::query("UPDATE contest_entries SET rank = $2 WHERE id = $1").bind(id).bind(rank).execute(&st.pool).await?;
    }
    sqlx::query("UPDATE contests SET refreshed_at = now(), status = CASE WHEN status IN ('scheduled','running') THEN $2 ELSE status END WHERE id = $1").bind(contest_id).bind(&status).execute(&st.pool).await?;
    Ok(())
}

/// Options contests: decides for each trade of an entry whether it counts (`excluded` = NULL), is a self-trade or
/// is below the minimum premium, from the client's option legs on their other accounts of the same type (live / demo):
/// every closed leg growth has recorded, plus (`open_legs`, once the contest has ended) the legs still open in the
/// engine. Re-run at every refresh, so a self-trade is caught once the other account's leg closes.
#[allow(clippy::too_many_arguments)]
async fn classify_option_trades(st: &AppState, tenant: &str, entry_id: i64, user_id: i64, login: i64, account_kind: &str, starts: DateTime<Utc>, open_legs: bool, min_premium: Option<D>) -> anyhow::Result<()> {
    let trades = sqlx::query(
        "SELECT t.deal_id, t.premium, t.excluded, d.symbol, d.side, d.open_time, d.close_time, d.fill_id FROM contest_trades t JOIN deals d ON d.deal_id = t.deal_id WHERE t.entry_id = $1",
    )
    .bind(entry_id)
    .fetch_all(&st.pool)
    .await?;
    if trades.is_empty() {
        return Ok(());
    }
    let symbols: Vec<String> = trades.iter().map(|t| t.get::<String, _>("symbol")).collect();
    let fills: Vec<String> = trades.iter().filter_map(|t| t.get::<Option<String>, _>("fill_id")).collect();
    let mut others: Vec<calc::OptLeg> = sqlx::query(
        "SELECT login, symbol, side, open_time, close_time, fill_id FROM deals
         WHERE tenant = $1 AND user_id = $2 AND login <> $3 AND instrument = 'option' AND NOT reversed AND account_kind = $4
           AND close_time >= $5 AND (symbol = ANY($6) OR fill_id = ANY($7))",
    )
    .bind(tenant)
    .bind(user_id)
    .bind(login)
    .bind(account_kind)
    .bind(starts - chrono::Duration::seconds(calc::SELF_TRADE_WINDOW_SECS))
    .bind(&symbols)
    .bind(&fills)
    .fetch_all(&st.pool)
    .await?
    .iter()
    .map(|r| calc::OptLeg { login: r.get("login"), symbol: r.get("symbol"), side: r.get("side"), open: r.get("open_time"), close: Some(r.get("close_time")), fill_id: r.get("fill_id") })
    .collect();
    if open_legs {
        // the other leg may still be open when the contest ends: ask the engine (an error stops the refresh, so a
        // contest is never finalized without this check)
        for acc in clients::accounts_of(st, tenant, user_id, Some(account_kind)).await? {
            if acc.login == login {
                continue;
            }
            for (symbol, side, open) in clients::open_positions(st, tenant, acc.login).await? {
                if symbols.iter().any(|s| s.eq_ignore_ascii_case(&symbol)) {
                    others.push(calc::OptLeg { login: acc.login, symbol, side, open, close: None, fill_id: None });
                }
            }
        }
    }
    for t in &trades {
        let leg = calc::OptLeg { login, symbol: t.get("symbol"), side: t.get("side"), open: t.get("open_time"), close: Some(t.get("close_time")), fill_id: t.get("fill_id") };
        let want = calc::option_exclusion(calc::is_self_trade(&leg, &others), t.get("premium"), min_premium);
        if want != t.get::<Option<String>, _>("excluded").as_deref() {
            sqlx::query("UPDATE contest_trades SET excluded = $3 WHERE entry_id = $1 AND deal_id = $2").bind(entry_id).bind(t.get::<i64, _>("deal_id")).bind(want).execute(&st.pool).await?;
        }
    }
    Ok(())
}

/// Returns true when the flag is new.
async fn add_flag(st: &AppState, contest_id: i64, entry_id: i64, kind: &str, severity: &str, details: Value) -> anyhow::Result<bool> {
    let n = sqlx::query("INSERT INTO contest_flags (contest_id, entry_id, kind, severity, details) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (entry_id, kind) DO UPDATE SET details = EXCLUDED.details WHERE contest_flags.status = 'open' RETURNING (xmax = 0) AS inserted")
        .bind(contest_id)
        .bind(entry_id)
        .bind(kind)
        .bind(severity)
        .bind(sqlx::types::Json(details))
        .fetch_optional(&st.pool)
        .await?;
    Ok(n.is_some_and(|r| r.get::<bool, _>("inserted")))
}

/// Refreshes every running contest, and ended ones once more after their end.
pub async fn tick(st: &AppState) -> anyhow::Result<usize> {
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM contests WHERE status IN ('scheduled','running','ended') AND starts_at <= now()
           AND (ends_at > now() OR refreshed_at IS NULL OR refreshed_at < ends_at + interval '2 minutes')",
    )
    .fetch_all(&st.pool)
    .await?;
    for id in &ids {
        if let Err(e) = refresh(st, *id).await {
            tracing::warn!(contest = id, error = %e, "contest refresh failed");
        }
    }
    auto_finalize_tick(st).await?;
    Ok(ids.len())
}

/// Brokers with `autoFinalize`: contests ended at least `autoFinalizeDelayMinutes` ago, scored after their end and
/// with no open anti-cheat flag are finalized by the service (then auto-pay pays the prizes below the threshold).
pub async fn auto_finalize_tick(st: &AppState) -> anyhow::Result<usize> {
    let mut n = 0;
    for tenant in crate::db::tenants(&st.pool).await? {
        let s = crate::db::settings(&st.pool, &tenant).await?;
        if !s.auto_finalize {
            continue;
        }
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT c.id FROM contests c WHERE c.tenant = $1 AND c.status IN ('scheduled','running','ended')
               AND c.ends_at + make_interval(mins => $2) <= now() AND c.refreshed_at >= c.ends_at
               AND NOT EXISTS (SELECT 1 FROM contest_flags f WHERE f.contest_id = c.id AND f.status = 'open') ORDER BY c.ends_at LIMIT 20",
        )
        .bind(&tenant)
        .bind(s.auto_finalize_delay_minutes as i32)
        .fetch_all(&st.pool)
        .await?;
        for id in ids {
            match finalize(st, id, &Actor::system()).await {
                Ok(()) => n += 1,
                Err(e) => tracing::warn!(contest = id, error = ?e, "auto-finalize failed"),
            }
        }
    }
    Ok(n)
}

/// Freezes ranks and prizes after the end.
pub async fn finalize(st: &AppState, contest_id: i64, actor: &Actor) -> ApiResult<()> {
    let c = sqlx::query("SELECT * FROM contests WHERE id = $1").bind(contest_id).fetch_one(&st.pool).await?;
    if status_of(&c) != "ended" {
        return Err(ApiError::Conflict { code: "state", message: "A contest can be finalized only after it ends.".into() });
    }
    // the last scores (options contests: with the open-leg self-trade check, which needs the engine)
    refresh(st, contest_id).await.map_err(|e| ApiError::Unavailable(format!("Scores could not be refreshed before finalizing ({e}). Please try again shortly.")))?;
    let min_trades: i32 = c.get("min_trades");
    let entries = sqlx::query("SELECT id, score, trades, status, joined_at FROM contest_entries WHERE contest_id = $1").bind(contest_id).fetch_all(&st.pool).await?;
    let input: Vec<RankInput> = entries
        .iter()
        .map(|e| RankInput { entry_id: e.get("id"), score: e.get("score"), trades: e.get::<i32, _>("trades") as i64, disqualified: e.get::<String, _>("status") == "disqualified", joined_at: e.get("joined_at") })
        .collect();
    let ranks = calc::rank(&input, min_trades as i64);
    let prizes = calc::allocate_prizes(&prizes_of(&c), &ranks);
    let tenant: String = c.get("tenant");
    // phase 4: prizes below the broker's auto-pay threshold are paid now, the rest wait for staff approval
    // (threshold 0 = every prize waits for staff, as before). Four-eyes: auto-pay never releases a prize at or
    // above `fourEyesThreshold` (it waits for two staff members).
    let settings = crate::db::settings(&st.pool, &tenant).await?;
    let threshold = settings.auto_pay_below;
    let limit = four_eyes::auto_pay_limit(threshold, settings.four_eyes_threshold);
    let mut tx = st.pool.begin().await?;
    let locked: String = sqlx::query_scalar("SELECT status FROM contests WHERE id = $1 FOR UPDATE").bind(contest_id).fetch_one(&mut *tx).await?;
    if matches!(locked.as_str(), "finalized" | "paid" | "cancelled") {
        return Err(ApiError::Conflict { code: "state", message: "This contest is already finalized.".into() });
    }
    for (id, rank, _) in &ranks {
        sqlx::query("UPDATE contest_entries SET rank = $2, prize_amount = NULL, prize_payout = NULL, prize_status = 'none' WHERE id = $1").bind(id).bind(rank).execute(&mut *tx).await?;
    }
    let mut auto = 0;
    for (id, amount, payout) in &prizes {
        let status = prize_decision(*amount, threshold, settings.four_eyes_threshold);
        if limit.is_some() && status == "pending" {
            auto += 1;
        }
        sqlx::query("UPDATE contest_entries SET prize_amount = $2, prize_payout = $3, prize_status = $4 WHERE id = $1").bind(id).bind(amount).bind(payout).bind(status).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE contests SET status = 'finalized', finalized_at = now(), finalized_by = $2, updated_at = now() WHERE id = $1").bind(contest_id).bind(actor.label()).execute(&mut *tx).await?;
    let total: D = prizes.iter().map(|p| p.1).sum();
    audit::record(&mut *tx, &tenant, actor, "contest.finalize", Some(format!("contest:{contest_id}")), None, Some(json!({"winners": prizes.len(), "prizes": num(total), "autoPay": auto, "autoPayBelow": num(threshold), "fourEyesThreshold": num(settings.four_eyes_threshold)})), None).await?;
    tx.commit().await?;
    if let (true, Some(limit)) = (auto > 0, limit) {
        pay_with(st, contest_id, &Actor::system(), Some(limit)).await?;
    }
    Ok(())
}

/// Prize status at finalize: `pending` (paid by the next payout run: at once when auto-pay applies, else when staff
/// press Pay) or `approval` (at or above a non-zero auto-pay threshold, or at or above the four-eyes threshold: staff
/// with `marketing.approve` pay it; at or above the four-eyes threshold two different staff members).
pub fn prize_decision(amount: D, auto_pay_below: D, four_eyes_threshold: D) -> &'static str {
    if four_eyes::needs_two(amount, four_eyes_threshold) {
        return "approval";
    }
    if auto_pay_below <= ZERO || amount < auto_pay_below { "pending" } else { "approval" }
}

/// Staff Pay (bulk, `marketing.approve`): every unpaid prize of the contest (pending, waiting for approval or
/// failed), with the four-eyes rule per prize:
/// - below `fourEyesThreshold`: paid now (as before);
/// - at or above it, not yet approved: this press is the first approval → `awaiting_second_approval` (nothing paid);
/// - `awaiting_second_approval`: paid when the caller is not the first approver, otherwise left waiting and counted
///   in `skippedSameStaff` (409 `four_eyes_same_staff` when that is all the press could do);
/// - a prize that already had its two approvals and failed (or is queued at the wallet) is retried by anyone.
pub async fn pay(st: &AppState, contest_id: i64, actor: &Actor) -> ApiResult<Value> {
    pay_with(st, contest_id, actor, None).await
}

/// Contest facts the payment of one prize needs.
struct PayCtx {
    tenant: String,
    name: String,
    kind: String,
}

async fn pay_ctx(st: &AppState, contest_id: i64) -> ApiResult<PayCtx> {
    let c = sqlx::query("SELECT * FROM contests WHERE id = $1").bind(contest_id).fetch_one(&st.pool).await?;
    let stored: String = c.get("status");
    if stored != "finalized" && stored != "paid" {
        return Err(ApiError::Conflict { code: "state", message: "Finalize the contest before paying prizes.".into() });
    }
    Ok(PayCtx { tenant: c.get("tenant"), name: c.get("name"), kind: c.get("kind") })
}

const PRIZE_SELECT: &str = "SELECT e.*, EXISTS (SELECT 1 FROM wallet_credits w WHERE w.idem_key = 'growth:prize:' || e.id AND w.status <> 'failed') AS queued FROM contest_entries e";

/// What a staff approval does to one prize.
enum Step {
    /// pay it now (below the threshold, already approved twice, or this is the second approval)
    Pay,
    /// record the first approval (at or above the threshold)
    First,
    /// the caller gave the first approval
    SameStaff,
    /// nothing to do
    Skip,
}

fn step_for(w: &sqlx::postgres::PgRow, me: &str, threshold: D) -> Step {
    let status: String = w.get("prize_status");
    let first: Option<String> = w.get("first_approved_by_id");
    let prize: D = w.get::<Option<D>, _>("prize_amount").unwrap_or(ZERO);
    if prize <= ZERO {
        return Step::Skip;
    }
    match status.as_str() {
        AWAITING if first.as_deref() == Some(me) => Step::SameStaff,
        AWAITING => Step::Pay,
        "pending" | "approval" | "failed" => {
            // approved twice already (released, then failed or queued at the wallet): retry without new approvals
            let released = first.is_some() || w.get::<bool, _>("queued");
            if four_eyes::needs_two(prize, threshold) && !released { Step::First } else { Step::Pay }
        }
        _ => Step::Skip,
    }
}

/// Pays one prize (wallet credit queued with key `growth:prize:<entry>`, or engine credit `growth:prize:<entry>:c`).
/// Returns whether it was paid / queued.
async fn pay_entry(st: &AppState, cx: &PayCtx, w: &sqlx::postgres::PgRow) -> anyhow::Result<bool> {
    let id: i64 = w.get("id");
    let user: i64 = w.get("user_id");
    let prize: D = w.get("prize_amount");
    let rank: i32 = w.get::<Option<i32>, _>("rank").unwrap_or(0);
    let (tenant, name) = (&cx.tenant, &cx.name);
    let note = format!("{name}: prize for rank #{rank}");
    if w.get::<Option<String>, _>("prize_payout").as_deref() == Some("credit") {
        let login = if cx.kind == "live" {
            Some(w.get::<i64, _>("login"))
        } else {
            clients::accounts_of(st, tenant, user, Some("live")).await.ok().and_then(|v| v.first().map(|a| a.login))
        };
        let Some(login) = login else {
            sqlx::query("UPDATE contest_entries SET prize_status = 'failed', prize_error = 'The client has no live account for a credit prize' WHERE id = $1").bind(id).execute(&st.pool).await?;
            return Ok(false);
        };
        let cent = clients::account(st, tenant, login).await.ok().flatten().is_some_and(|a| a.cent);
        match clients::post_balance(st, tenant, login, cent, "credit", prize, &format!("growth:prize:{id}:c"), REASON_PRIZE, &note).await {
            EngineOutcome::Booked => {
                sqlx::query("UPDATE contest_entries SET prize_status = 'paid', prize_ref = $2, prize_error = NULL WHERE id = $1").bind(id).bind(format!("credit:{login}")).execute(&st.pool).await?;
                clients::notify(st, tenant, user, "contest.prize", format!("{name}: prize credited"), format!("${} trading credit on account {login}.", prize.normalize()), "/rewards");
                Ok(true)
            }
            EngineOutcome::Retry(m) | EngineOutcome::Fail(m) => {
                sqlx::query("UPDATE contest_entries SET prize_status = 'failed', prize_error = $2 WHERE id = $1").bind(id).bind(&m).execute(&st.pool).await?;
                Ok(false)
            }
        }
    } else {
        let mut tx = st.pool.begin().await?;
        loyalty::queue_wallet_credit(&mut tx, tenant, user, &format!("growth:prize:{id}"), "adjustment", prize, &format!("prize:{id}"), &note).await?;
        sqlx::query("UPDATE wallet_credits SET status = 'pending', next_try_at = now() WHERE idem_key = $1 AND status = 'failed'").bind(format!("growth:prize:{id}")).execute(&mut *tx).await?;
        sqlx::query("UPDATE contest_entries SET prize_status = 'pending', prize_error = NULL WHERE id = $1").bind(id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }
}

/// Moves one prize on its four-eyes path under a conditional UPDATE: the first approval, or the second approval's
/// claim (`awaiting_second_approval` → `pending`). `Some(true)` = the caller may pay it now, `Some(false)` = first
/// approval recorded, `None` = someone else decided it meanwhile.
#[allow(clippy::too_many_arguments)]
async fn advance(st: &AppState, tenant: &str, contest_id: i64, w: &sqlx::postgres::PgRow, step: &Step, actor: &Actor, note: Option<&str>, threshold: D) -> anyhow::Result<Option<bool>> {
    let id: i64 = w.get("id");
    let status: String = w.get("prize_status");
    let amount = opt_num(w.get("prize_amount"));
    let mut tx = st.pool.begin().await?;
    let out = match step {
        Step::First => {
            if !four_eyes::record_first(&mut tx, "contest_entries", "prize_status", id, &status, actor, note, threshold).await? {
                return Ok(None);
            }
            audit::record(
                &mut *tx,
                tenant,
                actor,
                "prize.approve_first",
                Some(format!("contest_entry:{id}")),
                Some(json!({"status": status})),
                Some(json!({"contest": contest_id, "amount": amount, "status": AWAITING, "fourEyesThreshold": num(threshold)})),
                note,
            )
            .await?;
            Some(false)
        }
        Step::Pay if status == AWAITING => {
            if !four_eyes::claim_second(&mut tx, "contest_entries", "prize_status", id, actor, "pending").await? {
                return Ok(None);
            }
            let mut after = four_eyes::approvers(w.get::<Option<&str>, _>("first_approved_by_id"), w.get::<Option<&str>, _>("first_approved_by_name"), actor);
            after["contest"] = json!(contest_id);
            after["amount"] = amount;
            audit::record(&mut *tx, tenant, actor, "prize.approve", Some(format!("contest_entry:{id}")), Some(json!({"status": AWAITING})), Some(after), note).await?;
            Some(true)
        }
        _ => Some(true),
    };
    tx.commit().await?;
    Ok(out)
}

/// The contest is `paid` once no prize waits for an approval any more.
async fn mark_paid(st: &AppState, contest_id: i64) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE contests SET status = 'paid', paid_at = COALESCE(paid_at, now()), updated_at = now()
          WHERE id = $1 AND status IN ('finalized', 'paid')
            AND NOT EXISTS (SELECT 1 FROM contest_entries e WHERE e.contest_id = $1 AND e.prize_status IN ('approval', 'awaiting_second_approval'))",
    )
    .bind(contest_id)
    .execute(&st.pool)
    .await?;
    Ok(())
}

/// `below` = auto-pay (system): only pending / failed prizes under that amount and under the four-eyes threshold.
/// Every payout is idempotent (wallet key `growth:prize:<entry>`, engine key `growth:prize:<entry>:c`).
pub async fn pay_with(st: &AppState, contest_id: i64, actor: &Actor, below: Option<D>) -> ApiResult<Value> {
    let cx = pay_ctx(st, contest_id).await?;
    let tenant = cx.tenant.clone();
    let threshold = crate::db::settings(&st.pool, &tenant).await?.four_eyes_threshold;
    let me = four_eyes::staff_id(actor);
    let candidates = match below {
        None => sqlx::query(sqlx::AssertSqlSafe(format!(
            "{PRIZE_SELECT} WHERE e.contest_id = $1 AND e.prize_status IN ('pending','approval','awaiting_second_approval','failed') AND e.prize_amount > 0 ORDER BY e.rank"
        )))
        .bind(contest_id)
        .fetch_all(&st.pool)
        .await?,
        Some(t) => sqlx::query(sqlx::AssertSqlSafe(format!(
            "{PRIZE_SELECT} WHERE e.contest_id = $1 AND e.prize_status IN ('pending','failed') AND e.prize_amount > 0 AND e.prize_amount < $2 AND e.prize_amount < $3 ORDER BY e.rank"
        )))
        .bind(contest_id)
        .bind(t)
        .bind(threshold)
        .fetch_all(&st.pool)
        .await?,
    };
    let (mut paid, mut failed, mut amount, mut first, mut same) = (0, 0, ZERO, 0, 0);
    let mut winners = vec![];
    for w in candidates {
        let step = step_for(&w, &me, threshold);
        if below.is_some() {
            // auto-pay never gives an approval (the query already leaves four-eyes prizes out)
            if matches!(step, Step::Pay) {
                winners.push(w);
            }
            continue;
        }
        match step {
            Step::SameStaff => same += 1,
            Step::Skip => {}
            Step::First | Step::Pay => match advance(st, &tenant, contest_id, &w, &step, actor, None, threshold).await? {
                Some(true) => winners.push(w),
                Some(false) => first += 1,
                None => {}
            },
        }
    }
    if below.is_none() && winners.is_empty() && first == 0 && same > 0 {
        return Err(four_eyes::same_staff());
    }
    for w in &winners {
        let prize: D = w.get("prize_amount");
        if pay_entry(st, &cx, w).await? {
            paid += 1;
            amount += prize;
        } else {
            failed += 1;
        }
    }
    // paid once nothing waits for approval any more (auto-pay and four-eyes leave the larger prizes to staff)
    mark_paid(st, contest_id).await?;
    let awaiting: i64 = sqlx::query_scalar("SELECT count(*) FROM contest_entries WHERE contest_id = $1 AND prize_status = 'awaiting_second_approval'").bind(contest_id).fetch_one(&st.pool).await?;
    let action = if below.is_some() { "contest.autopay" } else { "contest.pay" };
    let summary = json!({"paid": paid, "failed": failed, "amount": num(amount), "below": below.map(num), "firstApproved": first, "skippedSameStaff": same, "awaitingSecond": awaiting});
    audit::record(&st.pool, &tenant, actor, action, Some(format!("contest:{contest_id}")), None, Some(summary), None).await?;
    st.wake.notify_one();
    // wallet prizes are paid by the payout loop; run it now so the response reflects them
    let _ = crate::payouts::wallet_tick(st).await;
    Ok(json!({"paid": paid, "failed": failed, "amount": num(amount), "firstApproved": first, "skippedSameStaff": same, "awaitingSecond": awaiting, "fourEyesThreshold": num(threshold)}))
}

/// One prize as the approval queue shows it.
pub async fn prize_json(st: &AppState, entry_id: i64) -> anyhow::Result<Value> {
    let r = sqlx::query("SELECT * FROM contest_entries WHERE id = $1").bind(entry_id).fetch_one(&st.pool).await?;
    let tenant: String = r.get("tenant");
    let threshold = crate::db::settings(&st.pool, &tenant).await?.four_eyes_threshold;
    let status: String = r.get("prize_status");
    Ok(json!({
        "entryId": entry_id,
        "contestId": r.get::<i64, _>("contest_id"),
        "userId": r.get::<i64, _>("user_id"),
        "rank": r.get::<Option<i32>, _>("rank"),
        "amount": opt_num(r.get("prize_amount")),
        "payout": r.get::<Option<String>, _>("prize_payout"),
        "status": status,
        "error": r.get::<Option<String>, _>("prize_error"),
        "fourEyes": four_eyes::json_of(&r, r.get("prize_amount"), &status, Some(threshold)),
    }))
}

/// Staff approval of one prize (`marketing.approve`): the bulk Pay's four-eyes rule for one prize, answered with
/// errors: 409 `four_eyes_same_staff` for the first approver, 409 `state` when it doesn't wait for a decision.
pub async fn approve_prize(st: &AppState, contest_id: i64, entry_id: i64, actor: &Actor, note: Option<&str>) -> ApiResult<Value> {
    let cx = pay_ctx(st, contest_id).await?;
    let threshold = crate::db::settings(&st.pool, &cx.tenant).await?.four_eyes_threshold;
    let w = sqlx::query(sqlx::AssertSqlSafe(format!("{PRIZE_SELECT} WHERE e.id = $1 AND e.contest_id = $2")))
        .bind(entry_id)
        .bind(contest_id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or(ApiError::NotFound)?;
    let step = step_for(&w, &four_eyes::staff_id(actor), threshold);
    let state_err = || ApiError::Conflict { code: "state", message: "This prize isn't waiting for a decision.".into() };
    match step {
        Step::SameStaff => return Err(four_eyes::same_staff()),
        Step::Skip => return Err(state_err()),
        Step::First | Step::Pay => match advance(st, &cx.tenant, contest_id, &w, &step, actor, note, threshold).await? {
            None => return Err(state_err()),
            Some(false) => {}
            Some(true) => {
                let ok = pay_entry(st, &cx, &w).await?;
                let summary = json!({"entry": entry_id, "paid": if ok { 1 } else { 0 }, "failed": if ok { 0 } else { 1 }, "amount": opt_num(w.get("prize_amount"))});
                audit::record(&st.pool, &cx.tenant, actor, "contest.pay", Some(format!("contest:{contest_id}")), None, Some(summary), note).await?;
                mark_paid(st, contest_id).await?;
                st.wake.notify_one();
                let _ = crate::payouts::wallet_tick(st).await;
            }
        },
    }
    Ok(json!({"prize": prize_json(st, entry_id).await?}))
}

/// Staff rejection of a prize that waits for a decision (approval, first approval given, failed, or pending and not
/// yet queued at the wallet). Either approver may reject; the reason is required.
pub async fn reject_prize(st: &AppState, contest_id: i64, entry_id: i64, actor: &Actor, note: &str) -> ApiResult<Value> {
    let cx = pay_ctx(st, contest_id).await?;
    let mut tx = st.pool.begin().await?;
    let w = sqlx::query(sqlx::AssertSqlSafe(format!("{PRIZE_SELECT} WHERE e.id = $1 AND e.contest_id = $2 FOR UPDATE OF e")))
        .bind(entry_id)
        .bind(contest_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ApiError::NotFound)?;
    let status: String = w.get("prize_status");
    let queued: bool = w.get("queued");
    let open = matches!(status.as_str(), "approval" | AWAITING | "failed" | "pending") && !queued && w.get::<Option<D>, _>("prize_amount").is_some_and(|a| a > ZERO);
    if !open {
        return Err(ApiError::Conflict { code: "state", message: "This prize isn't waiting for a decision (it may already be on its way to the client).".into() });
    }
    sqlx::query("UPDATE contest_entries SET prize_status = 'rejected', prize_error = $2 WHERE id = $1").bind(entry_id).bind(note).execute(&mut *tx).await?;
    let before = json!({"status": status, "firstApprovedById": w.get::<Option<String>, _>("first_approved_by_id"), "firstApprovedBy": w.get::<Option<String>, _>("first_approved_by_name")});
    let after = json!({"contest": contest_id, "amount": opt_num(w.get("prize_amount")), "status": "rejected", "from": status});
    audit::record(&mut *tx, &cx.tenant, actor, "prize.reject", Some(format!("contest_entry:{entry_id}")), Some(before), Some(after), Some(note)).await?;
    tx.commit().await?;
    mark_paid(st, contest_id).await?;
    Ok(json!({"prize": prize_json(st, entry_id).await?}))
}

pub fn flag_json(r: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": r.get::<i64, _>("id"),
        "entryId": r.get::<i64, _>("entry_id"),
        "kind": r.get::<String, _>("kind"),
        "severity": r.get::<String, _>("severity"),
        "details": r.get::<sqlx::types::Json<Value>, _>("details").0,
        "status": r.get::<String, _>("status"),
        "createdAt": r.get::<DateTime<Utc>, _>("created_at"),
        "resolvedBy": r.get::<Option<String>, _>("resolved_by"),
        "resolvedAt": r.get::<Option<DateTime<Utc>>, _>("resolved_at"),
        "note": r.get::<Option<String>, _>("note"),
        "name": r.try_get::<String, _>("display_name").ok(),
        "login": r.try_get::<i64, _>("login").ok(),
        "userId": r.try_get::<i64, _>("user_id").ok(),
    })
}
