//! The copier: consumes committed master events from the shard tap and mirrors them onto every follower's
//! copy account; tracks deposits / withdrawals on copy accounts (HWM, drawdown peak); the guard loop
//! (equity stop, max drawdown, fund protection); the scheduler (copy fee periods, PAMM rollovers, daily
//! snapshots, wallet outbox); subscription lifecycle (subscribe, stop, fee settlement).

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::Row;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::sync::mpsc;

use super::math::{self, Sizing};
use super::mirror::{self, LogEntry, MirrorCfg};
use super::{Social, Sub, next_period_end};
use crate::model::{TxnKind, acct_code};
use crate::money::{D, ZERO, r2};
use crate::shard::{Committed, ExecError, Op};
use crate::state::Event;

/// Ledger kinds that are external money flows on a copy account (not trading P&L).
pub fn is_flow(kind: TxnKind) -> bool {
    matches!(kind, TxnKind::TransferIn | TxnKind::TransferOut | TxnKind::Deposit | TxnKind::Withdrawal)
}

/// The part of a withdrawable amount that can go back to the wallet: rounded down to the cent (a transfer is in
/// cents and must never exceed the free funds; rounding to the nearest cent could ask for up to half a cent more).
pub fn returnable(withdrawable: D) -> D {
    withdrawable.max(ZERO).round_dp_with_strategy(2, rust_decimal::RoundingStrategy::ToZero)
}

impl Social {
    /// Starts the copier (after catching up from the events table) plus the guard and the scheduler.
    pub fn start(self: &Arc<Self>) {
        let (tx, rx) = mpsc::unbounded_channel();
        self.hub.shared.streams.set_tap(tx);
        let me = self.clone();
        tokio::spawn(async move { me.run_copier(rx).await });
        let me = self.clone();
        tokio::spawn(async move { me.run_guard().await });
        let me = self.clone();
        tokio::spawn(async move { me.run_scheduler().await });
    }

    async fn run_copier(self: Arc<Self>, mut rx: mpsc::UnboundedReceiver<Committed>) {
        if let Err(e) = self.catch_up().await {
            tracing::error!(error = %e, "copier catch-up failed");
        }
        while let Some(c) = rx.recv().await {
            self.on_commit(c, false).await;
        }
    }

    /// Replays events of watched logins committed after their cursor (engine restart).
    pub async fn catch_up(&self) -> anyhow::Result<()> {
        let logins: Vec<(i64, i64)> = {
            let reg = self.reg.read().unwrap();
            let mut v: Vec<(i64, i64)> = reg.masters.values().filter(|m| m.live() && !reg.subs_of(m.id).is_empty()).map(|m| (m.login, m.tenant_id)).collect();
            v.extend(reg.subs.values().filter(|s| s.copying()).map(|s| (s.login, s.tenant_id)));
            v.extend(reg.mam_watched());
            v
        };
        for (login, tenant) in logins {
            let from = self.cursor(login);
            let rows = sqlx::query("SELECT version, payload, created_at FROM events WHERE login = $1 AND version > $2 ORDER BY version").bind(login).bind(from).fetch_all(&self.pool).await?;
            if rows.is_empty() {
                continue;
            }
            let equity = self.account_brief(login).await.map(|b| b.equity).unwrap_or(ZERO);
            tracing::info!(login, from, events = rows.len(), "copier catching up");
            for r in rows {
                let ev: sqlx::types::Json<Event> = r.get("payload");
                let c = Committed { login, tenant_id: tenant, first_version: r.get("version"), at: r.get("created_at"), events: vec![ev.0], equity_usd: equity };
                self.on_commit(c, true).await;
            }
        }
        Ok(())
    }

    /// One committed transaction of a watched login.
    pub async fn on_commit(&self, c: Committed, catch_up: bool) {
        let last = c.first_version + c.events.len() as i64 - 1;
        if last <= self.cursor(c.login) {
            return;
        }
        // copy account: external flows move the HWM and the drawdown peak
        let sub = self.reg.read().unwrap().sub_by_login(c.login).cloned();
        if let Some(mut s) = sub {
            let mut flow = ZERO;
            for ev in &c.events {
                if let Event::Ledger { txn } = ev
                    && is_flow(txn.kind)
                {
                    flow += txn.effect(c.login, "balance");
                }
            }
            if !flow.is_zero() {
                s.net_deposits += flow;
                s.flows_since_fee += flow;
                s.peak_equity = (s.peak_equity + flow).max(ZERO);
                if s.allocation.is_zero() && flow > ZERO {
                    s.allocation = flow;
                }
                if let Err(e) = self.save_sub(&s).await {
                    tracing::error!(sub = s.id, error = %e, "saving copy flows failed");
                }
            }
        }
        // PAMM fund: closed volume is allocated to the holders by unit share for IB commissions (D64)
        let fund = self.reg.read().unwrap().fund_by_login(c.login).map(|f| (f.id, f.tenant_id));
        if let Some((fund_id, tenant)) = fund {
            for ev in &c.events {
                if let Event::PositionClosed { deal, .. } = ev
                    && deal.entry != crate::model::DealEntry::In
                {
                    self.push_ib_lots(fund_id, tenant, deal.clone()).await;
                }
            }
        }
        // master: mirror onto every follower
        let master = self.reg.read().unwrap().master_by_login(c.login).cloned();
        if let Some(m) = master
            && !m.frozen
        {
            let subs: Vec<Sub> = self.reg.read().unwrap().subs_of(m.id).into_iter().cloned().collect();
            for s in subs {
                self.mirror_into(&s, &m.nickname, m.status == "approved", &c, catch_up).await;
            }
        }
        // MAM master account: allocate onto every linked client account
        self.mam_on_commit(&c, catch_up).await;
        self.set_cursor(c.tenant_id, c.login, last).await;
    }

    async fn mirror_into(&self, s: &Sub, master: &str, master_ok: bool, c: &Committed, catch_up: bool) {
        let events: Vec<(i64, Event)> = c.events.iter().enumerate().map(|(i, e)| (c.first_version + i as i64, e.clone())).filter(|(v, _)| *v > s.start_version).collect();
        if events.is_empty() {
            return;
        }
        let cfg = MirrorCfg {
            sub_id: s.id,
            sizing: s.sizing,
            max_lot: s.max_lot,
            excluded: s.excluded.clone(),
            master: master.to_string(),
            opens: s.status == "active" && master_ok,
            catch_up,
            master_equity_usd: c.equity_usd,
            mam: None,
            auto_sl_pips: s.auto_sl_pips,
        };
        let flag = self.flag(s.id);
        let at = c.at;
        let sink: Arc<std::sync::Mutex<Vec<(i64, LogEntry)>>> = Default::default();
        let out = sink.clone();
        let op: Op = Box::new(move |tx, env| {
            if !flag.load(Ordering::SeqCst) {
                return Ok(Value::Null);
            }
            let mut log = out.lock().unwrap();
            for (v, ev) in &events {
                for e in mirror::mirror(tx, env, &cfg, *v, at, ev) {
                    log.push((*v, e));
                }
            }
            Ok(Value::Null)
        });
        let entries: Vec<(i64, LogEntry)> = match self.hub.exec(s.login, "copy", None, "", "", None, op).await {
            Ok(_) => std::mem::take(&mut *sink.lock().unwrap()),
            Err(e) => {
                tracing::error!(sub = s.id, login = s.login, error = ?e, "mirroring failed");
                vec![(c.first_version, LogEntry::new("mirror", None, "failed", format!("{e:?}")))]
            }
        };
        for (v, e) in &entries {
            let v = *v;
            if e.status != "done" {
                tracing::info!(sub = s.id, action = e.action, status = e.status, message = %e.message, "copy step");
            }
            let r = sqlx::query(
                "INSERT INTO copy_log (tenant_id, sub_id, master_login, master_version, action, master_ticket, follower_ticket, volume, status, message, master_price, follower_price, slippage_pips, delay_ms)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
            )
                .bind(s.tenant_id)
                .bind(s.id)
                .bind(c.login)
                .bind(v)
                .bind(e.action)
                .bind(e.master_ticket)
                .bind(e.follower_ticket)
                .bind(e.volume)
                .bind(e.status)
                .bind(&e.message)
                .bind(e.master_price)
                .bind(e.follower_price)
                .bind(e.slippage_pips)
                .bind(e.delay_ms)
                .execute(&self.pool)
                .await;
            if let Err(err) = r {
                tracing::warn!(error = %err, "copy log write failed");
            }
        }
        // A7: copier alerts (opened / closed in-app; skipped with the reason by email too)
        self.alert_steps(s, master, &entries).await;
    }

    /* ---------------- subscription lifecycle ---------------- */

    /// Stops a subscription: no more mirroring, then (optionally) closes everything, settles the fee for the
    /// period so far and moves the balance back to the wallet.
    ///
    /// Stopping an already stopped subscription changes nothing but the optional return of the balance: it keeps
    /// the first stop's reason and time and never closes anything, because positions the client chose to keep
    /// (`close: false`) are ordinary trades of theirs from then on (a repeated client stop, a stale screen or a
    /// Back Office stop must not close them).
    pub async fn stop_sub(&self, id: i64, reason: &str, close: bool, return_funds: bool) -> anyhow::Result<Value> {
        let _g = self.sub_lock.lock().await;
        let Some(mut s) = self.reg.read().unwrap().subs.get(&id).cloned() else { anyhow::bail!("not found") };
        let was = s.status.clone();
        // the flag first: any mirrored action already queued behind this stop becomes a no-op
        self.flag(id).store(false, Ordering::SeqCst);
        let close = close && was != "stopped";
        if was != "stopped" {
            s.status = "stopped".into();
            s.stop_reason = Some(reason.to_string());
            s.stopped_at = Some(Utc::now());
            self.save_sub(&s).await?;
        }
        let (mut closed, mut failed) = (Vec::new(), Vec::new());
        if close {
            let r = reason.to_string();
            let op: Op = Box::new(move |tx, env| {
                let (d, f) = mirror::close_all(tx, env, id, &r);
                Ok(json!({"closed": d, "failed": f.iter().map(|(t, e)| json!({"ticket": t, "error": e})).collect::<Vec<_>>()}))
            });
            match self.hub.exec(s.login, "copy", None, "", "", None, op).await {
                Ok(d) => {
                    closed = d.value["closed"].as_array().cloned().unwrap_or_default();
                    failed = d.value["failed"].as_array().cloned().unwrap_or_default();
                }
                Err(e) => failed.push(json!({"ticket": null, "error": format!("{e:?}")})),
            }
        }
        if was != "stopped" {
            drop(_g);
            if let Err(e) = self.settle_copy(id, Utc::now(), true).await {
                tracing::error!(sub = id, error = %e, "fee settlement on stop failed");
            }
        }
        let (returned, return_error) = if return_funds { self.return_balance(id, &s).await } else { (None, None) };
        tracing::info!(sub = id, reason, closed = closed.len(), failed = failed.len(), "subscription stopped");
        Ok(json!({"closed": closed, "failed": failed, "returned": crate::money::num_opt(returned), "returnError": return_error}))
    }

    /// Moves the withdrawable balance of a stopped subscription's copy account back to the wallet. The amount is
    /// rounded down to the cent (`returnable`), so it never exceeds what the engine lets leave the account. With
    /// kept positions the free margin moves with every price, so a refusal for insufficient funds is retried
    /// once on a fresh reading (with its own idempotency key).
    async fn return_balance(&self, id: i64, s: &Sub) -> (Option<D>, Option<String>) {
        let mut error = None;
        for attempt in 0..2 {
            let Some(b) = self.account_brief(s.login).await else { break };
            let amt = returnable(b.withdrawable);
            if amt <= ZERO {
                break;
            }
            let key = if attempt == 0 { format!("copy:return:{id}:{}", b.version) } else { format!("copy:return:{id}:{}:retry", b.version) };
            match self.wallet.from_trading(&self.slug(s.tenant_id), &key, s.user_id, s.login, amt).await {
                Ok(_) => return (Some(amt), None),
                Err(e) => {
                    let again = e.code == "insufficient_funds";
                    error = Some(e.message);
                    if !again {
                        break;
                    }
                }
            }
        }
        (None, error)
    }

    /// Crystallises the performance fee of a subscription (period end, or `final` on stop): HWM maths in
    /// `math::copy_fee`, fee debited from the copy account and recorded pending approval (D76).
    pub async fn settle_copy(&self, id: i64, now: DateTime<Utc>, final_: bool) -> anyhow::Result<Option<D>> {
        let _g = self.sub_lock.lock().await;
        let Some(mut s) = self.reg.read().unwrap().subs.get(&id).cloned() else { return Ok(None) };
        let Some(b) = self.account_brief(s.login).await else { return Ok(None) };
        let calc = math::copy_fee(s.perf_fee_pct, s.hwm, s.flows_since_fee, b.equity);
        let mut charged = None;
        if calc.fee > ZERO && b.equity > ZERO {
            let key = format!("perf:copy:{id}:{}", now.timestamp());
            let fee = calc.fee;
            let k = key.clone();
            let op: Op = Box::new(move |tx, env| {
                let f = fee * tx.st.account.usd_factor();
                let txn = tx.post(env, TxnKind::PerformanceFee, k.clone(), "balance", "perf_fees", -f, Some(k), None, Some("Copy trading performance fee".into()));
                tx.note("balance", format!("Performance fee {} {}", f.normalize(), tx.st.account.ccy()), json!({"amount": crate::money::num(-f), "txn": txn}));
                Ok(json!({"txn": txn}))
            });
            match self.hub.exec(s.login, "system", None, "", "", None, op).await {
                Ok(_) | Err(ExecError::Duplicate(_)) => {}
                Err(e) => anyhow::bail!("fee debit failed: {e:?}"),
            }
            let settings = self.settings(s.tenant_id);
            let (cut, to_master) = math::split_fee(fee, settings.platform_cut_pct);
            sqlx::query(
                "INSERT INTO social_fees (tenant_id, source, master_id, sub_id, payer_user_id, login, ledger_key, amount, platform_cut, master_amount, period_start, period_end, hwm_before, hwm_after, equity, status)
                 VALUES ($1,'copy',$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,'pending') ON CONFLICT DO NOTHING",
            )
            .bind(s.tenant_id)
            .bind(s.master_id)
            .bind(s.id)
            .bind(s.user_id)
            .bind(s.login)
            .bind(&key)
            .bind(fee)
            .bind(cut)
            .bind(to_master)
            .bind(s.last_fee_at.unwrap_or(s.created_at))
            .bind(now)
            .bind(calc.hwm_before)
            .bind(calc.hwm_after)
            .bind(r2(b.equity))
            .execute(&self.pool)
            .await?;
            s.fees_paid += fee;
            charged = Some(fee);
            tracing::info!(sub = id, fee = %fee, equity = %b.equity, hwm = %calc.hwm_before, "copy performance fee charged");
        }
        s.hwm = calc.hwm_after;
        s.flows_since_fee = ZERO;
        s.last_fee_at = Some(now);
        if !final_ {
            s.next_fee_at = next_period_end(&s.fee_period, now);
        }
        s.last_equity = Some(b.equity - charged.unwrap_or(ZERO));
        self.save_sub(&s).await?;
        Ok(charged)
    }

    /// Staff review of a pending copy / PAMM / MAM fee (D76), in one transaction. Approve queues the master's
    /// payout (`fee − platform cut`, wallet kind `copy_fee` / `mam_fee`). Reject refunds the whole fee to the
    /// payer's wallet (the follower, investor or MAM client: `refund`, key `fee-refund:<id>`); nothing goes to
    /// the master and the platform keeps no cut. `None` when the fee is not pending.
    pub async fn review_fee(&self, tenant: i64, id: i64, approve: bool, reviewer: &str, note: &str) -> anyhow::Result<Option<sqlx::postgres::PgRow>> {
        let mut t = self.pool.begin().await?;
        let Some(r) = sqlx::query("UPDATE social_fees SET status = $3, reviewed_by = $4, note = $5, updated_at = now() WHERE id = $1 AND tenant_id = $2 AND status = 'pending' RETURNING *")
            .bind(id)
            .bind(tenant)
            .bind(if approve { "approved" } else { "rejected" })
            .bind(reviewer)
            .bind(note)
            .fetch_optional(&mut *t)
            .await?
        else {
            return Ok(None);
        };
        if approve {
            let master_id: i64 = r.get("master_id");
            let master_user = self.reg.read().unwrap().masters.get(&master_id).map(|m| m.user_id).unwrap_or(0);
            let amt: D = r.get("master_amount");
            if amt > ZERO {
                let (kind, memo) = if r.get::<String, _>("source") == "mam" { ("mam_fee", "MAM fee payout") } else { ("copy_fee", "Performance fee payout") };
                super::wallet::enqueue(&mut *t, tenant, &format!("fee:{id}"), master_user, amt, super::wallet::CREDIT, kind, &format!("fee:{id}"), memo).await?;
            } else {
                sqlx::query("UPDATE social_fees SET status = 'paid', paid_at = now() WHERE id = $1").bind(id).execute(&mut *t).await?;
            }
        } else {
            let amt: D = r.get("amount");
            let memo = if r.get::<String, _>("source") == "mam" { "MAM fee refunded (rejected by staff)" } else { "Performance fee refunded (rejected by staff)" };
            super::wallet::enqueue(&mut *t, tenant, &format!("fee-refund:{id}"), r.get("payer_user_id"), amt, super::wallet::CREDIT, "refund", &format!("fee-refund:{id}"), memo).await?;
        }
        t.commit().await?;
        Ok(Some(r))
    }

    /* ---------------- guard (D70, D74) ---------------- */

    async fn run_guard(self: Arc<Self>) {
        let mut tick = tokio::time::interval(Duration::from_secs(2));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            self.guard_once().await;
        }
    }

    pub async fn guard_once(&self) {
        let subs: Vec<Sub> = self.reg.read().unwrap().subs.values().filter(|s| s.copying()).cloned().collect();
        for s in subs {
            let Some(b) = self.account_brief(s.login).await else { continue };
            let mut breach = None;
            {
                let mut reg = self.reg.write().unwrap();
                if let Some(x) = reg.subs.get_mut(&s.id) {
                    x.last_equity = Some(b.equity);
                    x.last_balance = Some(b.balance);
                    x.positions = b.positions;
                    x.orders = b.orders;
                    if b.equity > x.peak_equity {
                        x.peak_equity = r2(b.equity);
                    }
                    if x.net_deposits > ZERO {
                        breach = mirror::breach(b.equity, x.peak_equity, x.equity_stop, x.max_dd_pct);
                    }
                }
            }
            if let Some(reason) = breach {
                tracing::warn!(sub = s.id, login = s.login, reason, equity = %b.equity, "follower protection hit: stopping the subscription");
                match self.stop_sub(s.id, reason, true, false).await {
                    Ok(_) => self.alert_protection(&s, reason, b.equity).await,
                    Err(e) => tracing::error!(sub = s.id, error = %e, "protective stop failed"),
                }
            }
        }
        // persist raised peaks now and then (cheap: only when they moved)
        let peaks: Vec<(i64, D)> = self.reg.read().unwrap().subs.values().filter(|s| s.copying()).map(|s| (s.id, s.peak_equity)).collect();
        for (id, p) in peaks {
            let _ = sqlx::query("UPDATE copy_subscriptions SET peak_equity = $2 WHERE id = $1 AND peak_equity < $2").bind(id).bind(p).execute(&self.pool).await;
        }
        self.guard_funds().await;
        self.mam_guard().await;
    }

    /* ---------------- scheduler ---------------- */

    async fn run_scheduler(self: Arc<Self>) {
        let mut tick = tokio::time::interval(Duration::from_secs(20));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut n: u64 = 0;
        loop {
            tick.tick().await;
            n += 1;
            let now = Utc::now();
            self.run_due(now, false).await;
            // A8: copies whose new terms were not accepted in time pause
            self.pause_expired_terms(now).await;
            if n % 30 == 1 {
                let w = self.snapshot_all().await;
                tracing::debug!(written = w, "social snapshots");
            }
            let slugs = self.hub.shared.registry.clone();
            super::wallet::flush(&self.pool, &self.wallet, |t| slugs.get(t).map(|x| x.slug.clone()).unwrap_or_else(|| "kalks".into())).await;
        }
    }

    /// Fee periods and PAMM rollovers that are due (or all of them with `force`). Returns (funds, subs, fees).
    pub async fn run_due(&self, now: DateTime<Utc>, force: bool) -> (usize, usize, usize) {
        let due_subs: Vec<i64> = self.reg.read().unwrap().subs.values().filter(|s| s.copying() && (force || s.next_fee_at <= now)).map(|s| s.id).collect();
        let mut fees = 0;
        let nsubs = due_subs.len();
        for id in due_subs {
            match self.settle_copy(id, now, false).await {
                Ok(Some(_)) => fees += 1,
                Ok(None) => {}
                Err(e) => tracing::error!(sub = id, error = %e, "copy fee settlement failed"),
            }
        }
        let due_funds: Vec<i64> = self.reg.read().unwrap().funds.values().filter(|f| f.status != "closed" && (force || f.next_rollover_at <= now)).map(|f| f.id).collect();
        let nfunds = due_funds.len();
        for id in due_funds {
            match self.rollover(id, "scheduled").await {
                Ok(v) => fees += v["fees"].as_array().map(|a| a.len()).unwrap_or(0),
                Err(e) => tracing::error!(fund = id, error = %e, "PAMM rollover failed"),
            }
        }
        let (nlinks, mam_fees) = self.mam_run_due(now, force).await;
        (nfunds, nsubs + nlinks, fees + mam_fees)
    }

    /* ---------------- subscribe ---------------- */

    /// Opens the dedicated copy account and registers the subscription (D71). Funding is done by the caller.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_sub(&self, tenant: i64, user: i64, master_id: i64, sizing: Sizing, allocation: D, max_lot: Option<D>, equity_stop: Option<D>, max_dd_pct: Option<D>, excluded: Vec<String>) -> anyhow::Result<Sub> {
        let m = self.reg.read().unwrap().masters.get(&master_id).cloned().ok_or_else(|| anyhow::anyhow!("master not found"))?;
        let mb = self.account_brief(m.login).await.ok_or_else(|| anyhow::anyhow!("master account not found"))?;
        let group = if mb.netting { "copy-netting" } else { "copy" };
        let (login, _, _) = self.open_account(tenant, user, group, &format!("Copy · {}", m.nickname)).await.map_err(|e| anyhow::anyhow!("{e:?}"))?;
        let now = Utc::now();
        let next = next_period_end(&m.fee_period, now);
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO copy_subscriptions (tenant_id, master_id, user_id, login, sizing_mode, sizing_value, max_lot, equity_stop, max_dd_pct, excluded_symbols, status,
                perf_fee_pct, fee_period, allocation, start_version, next_fee_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'active',$11,$12,$13,$14,$15) RETURNING id",
        )
        .bind(tenant)
        .bind(master_id)
        .bind(user)
        .bind(login)
        .bind(sizing.mode.as_str())
        .bind(sizing.value)
        .bind(max_lot)
        .bind(equity_stop)
        .bind(max_dd_pct)
        .bind(&excluded)
        .bind(m.perf_fee_pct)
        .bind(&m.fee_period)
        .bind(ZERO)
        .bind(mb.version)
        .bind(next)
        .fetch_one(&self.pool)
        .await?;
        let row = sqlx::query("SELECT * FROM copy_subscriptions WHERE id = $1").bind(id).fetch_one(&self.pool).await?;
        let mut s = super::sub_from(&row);
        s.allocation = ZERO;
        // cursors: the master from now on; the copy account from its opening (so the first deposit counts)
        self.set_cursor(tenant, m.login, mb.version.max(self.cursor(m.login))).await;
        self.set_cursor(tenant, login, 1).await;
        {
            let mut reg = self.reg.write().unwrap();
            reg.flags.insert(id, Arc::new(std::sync::atomic::AtomicBool::new(true)));
            reg.subs.insert(id, s.clone());
        }
        self.rewatch();
        let _ = allocation;
        tracing::info!(sub = id, master = master_id, login, user, "copy subscription created");
        Ok(s)
    }

    /// Recent copy log of a subscription.
    pub async fn copy_log(&self, sub: i64, limit: i64) -> Vec<Value> {
        let rows = sqlx::query("SELECT at, action, master_ticket, follower_ticket, volume, status, message, master_price, follower_price, slippage_pips, delay_ms FROM copy_log WHERE sub_id = $1 ORDER BY id DESC LIMIT $2")
            .bind(sub)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .unwrap_or_default();
        rows.iter()
            .map(|r| {
                json!({"at": r.get::<DateTime<Utc>, _>("at"), "action": r.get::<String, _>("action"), "masterTicket": r.get::<Option<i64>, _>("master_ticket"),
                       "followerTicket": r.get::<Option<i64>, _>("follower_ticket"), "volume": crate::money::num_opt(r.get::<Option<D>, _>("volume")),
                       "status": r.get::<String, _>("status"), "message": r.get::<String, _>("message"),
                       "masterPrice": crate::money::num_opt(r.get::<Option<D>, _>("master_price")), "followerPrice": crate::money::num_opt(r.get::<Option<D>, _>("follower_price")),
                       "slippagePips": crate::money::num_opt(r.get::<Option<D>, _>("slippage_pips")), "delayMs": r.get::<Option<i64>, _>("delay_ms")})
            })
            .collect()
    }

    /// A10 execution report of a subscription: copied opens / closes with the master's and the follower's price,
    /// slippage (pips, positive = worse for the follower) and delay, plus averages.
    pub async fn execution_report(&self, sub: i64, limit: i64) -> Value {
        let rows = sqlx::query(
            "SELECT at, action, master_ticket, follower_ticket, volume, master_price, follower_price, slippage_pips, delay_ms FROM copy_log
             WHERE sub_id = $1 AND status = 'done' AND master_price IS NOT NULL ORDER BY id DESC LIMIT $2",
        )
        .bind(sub)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .unwrap_or_default();
        let items: Vec<Value> = rows
            .iter()
            .map(|r| {
                json!({"at": r.get::<DateTime<Utc>, _>("at"), "action": r.get::<String, _>("action"), "masterTicket": r.get::<Option<i64>, _>("master_ticket"),
                       "followerTicket": r.get::<Option<i64>, _>("follower_ticket"), "volume": crate::money::num_opt(r.get::<Option<D>, _>("volume")),
                       "masterPrice": crate::money::num_opt(r.get::<Option<D>, _>("master_price")), "followerPrice": crate::money::num_opt(r.get::<Option<D>, _>("follower_price")),
                       "slippagePips": crate::money::num_opt(r.get::<Option<D>, _>("slippage_pips")), "delayMs": r.get::<Option<i64>, _>("delay_ms")})
            })
            .collect();
        let slips: Vec<D> = rows.iter().filter_map(|r| r.get::<Option<D>, _>("slippage_pips")).collect();
        let delays: Vec<i64> = rows.iter().filter_map(|r| r.get::<Option<i64>, _>("delay_ms")).collect();
        let avg_slip = (!slips.is_empty()).then(|| crate::money::rdp(slips.iter().copied().sum::<D>() / D::from(slips.len() as i64), 2));
        let avg_delay = (!delays.is_empty()).then(|| delays.iter().sum::<i64>() / delays.len() as i64);
        let max_delay = delays.iter().max().copied();
        json!({"items": items, "summary": {"trades": items.len(), "avgSlippagePips": crate::money::num_opt(avg_slip), "avgDelayMs": avg_delay, "maxDelayMs": max_delay,
                                             "worstSlippagePips": crate::money::num_opt(slips.iter().copied().max())}})
    }

    /// Flows (USD) booked on `login` inside [from, to): deposits − withdrawals (and house capital on house accounts).
    pub async fn flows(&self, login: i64, from: DateTime<Utc>, to: DateTime<Utc>) -> D {
        sqlx::query_scalar::<_, Option<D>>(
            "SELECT sum(p.amount) FROM ledger_txns t JOIN ledger_postings p ON p.txn_id = t.id
             WHERE t.login = $1 AND p.account_code = $2 AND t.kind IN ('transfer_in','transfer_out','deposit','withdrawal','demo_initial','demo_refill','house_capital')
               AND t.created_at >= $3 AND t.created_at < $4",
        )
        .bind(login)
        .bind(acct_code(login, "balance"))
        .bind(from)
        .bind(to)
        .fetch_one(&self.pool)
        .await
        .ok()
        .flatten()
        .unwrap_or(ZERO)
    }

    /// Units each holder of a fund held at `at`: Σ of the unit ledger up to then. Units only move at rollovers
    /// (invest / redeem / fee) and stop-loss redemptions, all written to the append-only `pamm_unit_ledger`, so
    /// this is the holdings after the last move at or before `at`.
    pub async fn units_at(&self, fund_id: i64, at: DateTime<Utc>) -> Vec<(i64, D)> {
        sqlx::query_as(
            "SELECT i.user_id, sum(l.units) FROM pamm_unit_ledger l JOIN pamm_investors i ON i.id = l.investor_id
             WHERE l.fund_id = $1 AND l.at <= $2 GROUP BY i.user_id HAVING sum(l.units) > 0 ORDER BY i.user_id",
        )
        .bind(fund_id)
        .bind(at)
        .fetch_all(&self.pool)
        .await
        .unwrap_or_default()
    }

    /// IB lots of a closed fund deal per holder (D64): the deal volume split by the units held when the position
    /// OPENED (an investor who bought in after the open was not in the trade; one who redeemed while it was open
    /// was), largest remainder so the shares add up to the volume exactly (`math::split_lots`).
    /// The master's own units are included on purpose: the manager's capital is traded too, so the master's
    /// referrer earns on that share like on any investor's. The fund account's own deal is never paid (the IB
    /// service disqualifies group `pamm`), so the volume is counted once.
    pub async fn ib_lot_shares(&self, fund_id: i64, volume: D, open_time: DateTime<Utc>) -> Vec<(i64, D)> {
        math::split_lots(volume, &self.units_at(fund_id, open_time).await)
    }

    /// Splits a closed fund deal across the fund's holders (`ib_lot_shares`) and pushes each share to the IB
    /// service (`POST /v1/ib/events/lots`, idempotent on deal + user). Best effort with retries.
    async fn push_ib_lots(&self, fund_id: i64, tenant: i64, deal: crate::model::Deal) {
        let Some(ib) = self.ib.get().cloned() else { return };
        let shares = self.ib_lot_shares(fund_id, deal.volume, deal.open_time).await;
        let slug = self.slug(tenant);
        for (user, lots) in shares {
            let body = json!({"source": "pamm", "dealId": deal.id, "userId": user, "symbol": deal.symbol, "side": deal.position_side.as_str(), "lots": lots.normalize().to_string(),
                              "openTime": deal.open_time, "closeTime": deal.time, "login": deal.login});
            let (ib, slug) = (ib.clone(), slug.clone());
            tokio::spawn(async move {
                for attempt in 0..5u64 {
                    match ib.post(&slug, "/v1/ib/events/lots", &body).await {
                        Ok(_) => return,
                        Err(e) if e.is_rejection() => {
                            tracing::warn!(error = %e, "IB refused a PAMM lots event");
                            return;
                        }
                        Err(e) => {
                            tracing::debug!(error = %e, attempt, "IB lots push failed, retrying");
                            tokio::time::sleep(Duration::from_secs(2u64.pow(attempt as u32 + 1))).await;
                        }
                    }
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::returnable;
    use crate::money::D;
    use std::str::FromStr;

    fn d(s: &str) -> D {
        D::from_str(s).unwrap()
    }

    #[test]
    fn the_returned_balance_never_exceeds_the_free_funds() {
        // free margin with open positions has more than two decimals: round down, never to the nearest cent
        assert_eq!(returnable(d("490.137")), d("490.13"));
        assert_eq!(returnable(d("490.1349999")), d("490.13"));
        assert_eq!(returnable(d("599.74")), d("599.74"));
        assert_eq!(returnable(d("0.009")), d("0"));
        assert_eq!(returnable(d("-3.5")), d("0"));
    }
}
