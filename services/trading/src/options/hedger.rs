//! House delta hedger (O27): the house is the counterparty of every live option trade (B-book), so it is short the
//! clients' delta. Every 10 s, per tenant with live options switched on, the hedger sums the delta of the live
//! clients' option positions per underlying (units of the underlying, from the marks), adds the CFD position of
//! the tenant's **hedge account** on that symbol, and when the remaining house delta is worth more than
//! `OPTIONS_HEDGE_LIMIT_USD` it places a CFD market order on the hedge account that brings it back to zero
//! (rounded down to the CFD lot step).
//!
//! The hedge account is a normal live account of the house user (`OPTIONS_HEDGE_USER_ID`) opened through the same
//! path as the social house accounts, with house capital (`OPTIONS_HEDGE_CAPITAL`, ledger kind `house_capital`,
//! never a client deposit). Its trades are internal B-book CFD trades today; an LP adapter can take them later.
//! Every order is recorded in `option_hedges`. Stale snapshots, closed markets and missing prices skip a pass.

use chrono::Utc;
use serde_json::{Value, json};
use sqlx::Row;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use super::OptionPricing;
use crate::api::AppState;
use crate::engine::funds::{self, AdjustKind};
use crate::engine::trade::{self, OrderReq, PlaceResult};
use crate::model::{AccountKind, Side, Source};
use crate::money::{D, ZERO, from_f64, num};
use crate::rules::TenantConfig;
use crate::shard::Op;

/// The tenant's hedge account, opened on first use.
pub async fn hedge_account(st: &AppState, t: &TenantConfig) -> anyhow::Result<i64> {
    if let Some(l) = sqlx::query_scalar::<_, i64>("SELECT login FROM option_hedge_accounts WHERE tenant_id = $1").bind(t.tenant_id).fetch_optional(&st.pool).await? {
        return Ok(l);
    }
    let cfg = &st.cfg;
    // the hedge trades CFDs: always a CFD group (CFD / Options account split)
    let cfd = |g: &&crate::rules::Group| g.product == crate::rules::Product::Cfd;
    let pick = |code: &str| t.groups.get(code).filter(|g| g.enabled && g.allows("live") && !g.cent).filter(cfd).map(|g| g.code.clone());
    let group = pick(&cfg.options_hedge_group)
        .or_else(|| t.groups.values().filter(cfd).filter(|g| g.enabled && g.allows("live") && !g.cent && g.mode == crate::model::Mode::Netting && !g.code.starts_with("prop") && !matches!(g.code.as_str(), "copy" | "copy-netting" | "pamm" | "mam")).map(|g| g.code.clone()).min())
        .or_else(|| pick("standard"))
        .ok_or_else(|| anyhow::anyhow!("no live USD group for the hedge account"))?;
    let (login, _, _) = st.social.open_account(t.tenant_id, cfg.options_hedge_user, &group, "Options delta hedge (house)").await.map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let capital = D::from(cfg.options_hedge_capital.max(0));
    if capital > ZERO {
        let key = format!("options-hedge:{}:capital:initial", t.tenant_id);
        let op: Op = Box::new(move |tx, env| funds::adjust(tx, env, AdjustKind::HouseCapital, capital, &key, "HOUSE", "Options delta hedge capital").map(|id| json!({"txn": id})));
        st.hub.exec(login, "system", None, "HOUSE", "options hedge account", None, op).await.map_err(|e| anyhow::anyhow!("{e:?}"))?;
    }
    sqlx::query("INSERT INTO option_hedge_accounts (tenant_id, login, user_id, group_code) VALUES ($1,$2,$3,$4) ON CONFLICT (tenant_id) DO NOTHING")
        .bind(t.tenant_id)
        .bind(login)
        .bind(cfg.options_hedge_user)
        .bind(&group)
        .execute(&st.pool)
        .await?;
    let l: i64 = sqlx::query("SELECT login FROM option_hedge_accounts WHERE tenant_id = $1").bind(t.tenant_id).fetch_one(&st.pool).await?.get("login");
    tracing::info!(tenant = t.tenant_id, login = l, %group, "options hedge account opened");
    Ok(l)
}

/// Live clients' option delta per underlying, in units of the underlying (the house holds the opposite).
pub async fn client_delta(st: &AppState, tenant_id: i64, exclude: Option<i64>) -> BTreeMap<String, f64> {
    let rows = st
        .hub
        .scan(
            tenant_id,
            Arc::new(move |a, env| {
                if a.account.kind != AccountKind::Live || Some(a.login()) == exclude {
                    return vec![];
                }
                a.positions
                    .values()
                    // order-book positions have another account on the other side, not the house
                    .filter(|p| !p.on_book())
                    .filter_map(|p| {
                        let t = p.option.as_ref()?;
                        let q = env.options.mark(&env.tenant.slug, &env.group.code, t, env.now)?;
                        Some(json!({"u": t.underlying, "d": q.delta * super::f(p.volume * t.contract_size * p.side.sign())}))
                    })
                    .collect()
            }),
        )
        .await;
    let mut out: BTreeMap<String, f64> = BTreeMap::new();
    for r in rows {
        if let (Some(u), Some(d)) = (r["u"].as_str(), r["d"].as_f64()) {
            *out.entry(u.to_string()).or_default() += d;
        }
    }
    out
}

/// Option delta of the tenant's live market-maker account per underlying (units of the underlying).
pub async fn mm_delta(st: &AppState, tenant_id: i64) -> BTreeMap<String, f64> {
    let Some(login) = st.hub.shared.books.mm.login(tenant_id, AccountKind::Live) else { return BTreeMap::new() };
    let v = st
        .hub
        .read(
            login,
            Box::new(move |x| {
                let Some((a, env)) = x else { return Value::Null };
                let mut m = serde_json::Map::new();
                for p in a.positions.values() {
                    let Some(t) = p.option.as_ref() else { continue };
                    let Some(q) = env.options.mark(&env.tenant.slug, &env.group.code, t, env.now) else { continue };
                    let d = q.delta * super::f(p.volume * t.contract_size * p.side.sign());
                    let cur = m.get(&t.underlying).and_then(Value::as_f64).unwrap_or(0.0);
                    m.insert(t.underlying.clone(), json!(cur + d));
                }
                Value::Object(m)
            }),
        )
        .await;
    v.as_object().map(|o| o.iter().filter_map(|(k, v)| v.as_f64().map(|x| (k.clone(), x))).collect()).unwrap_or_default()
}

/// The hedge account's net CFD units per symbol.
pub async fn hedge_units(st: &AppState, login: i64) -> BTreeMap<String, f64> {
    let specs = st.hub.shared.specs.load();
    let v = st
        .hub
        .read(
            login,
            Box::new(move |x| {
                let Some((a, _)) = x else { return Value::Null };
                let mut m = serde_json::Map::new();
                for p in a.positions.values().filter(|p| p.option.is_none()) {
                    let size = specs.get(&p.symbol).map(|s| s.contract_size).unwrap_or(ZERO);
                    let u = super::f(p.volume * size * p.side.sign());
                    let cur = m.get(&p.symbol).and_then(Value::as_f64).unwrap_or(0.0);
                    m.insert(p.symbol.clone(), json!(cur + u));
                }
                Value::Object(m)
            }),
        )
        .await;
    v.as_object().map(|o| o.iter().filter_map(|(k, v)| v.as_f64().map(|x| (k.clone(), x))).collect()).unwrap_or_default()
}

/// One hedging pass for a tenant. Returns the orders placed.
pub async fn hedge_tenant(st: &AppState, t: &TenantConfig) -> anyhow::Result<usize> {
    let opts = &st.hub.shared.options;
    let now = st.hub.shared.clock.now();
    let Some(snap) = opts.snapshot() else { return Ok(0) };
    if !snap.enabled(&t.slug, true) || opts.stale(now) {
        return Ok(0);
    }
    let mut clients = client_delta(st, t.tenant_id, None).await;
    // the Kalks market maker's order-book positions are the house's own exposure (docs §4 "delta hedge"): the
    // house holds −clients (house venue) + the MM's delta
    for (u, d) in mm_delta(st, t.tenant_id).await {
        *clients.entry(u).or_default() -= d;
    }
    if clients.is_empty() {
        return Ok(0);
    }
    let login = hedge_account(st, t).await?;
    let hedge = hedge_units(st, login).await;
    let limit = st.cfg.options_hedge_limit_usd.max(0) as f64;
    let mut placed = 0;
    for (u, client) in clients {
        let Some(spec) = st.hub.shared.specs.load().get(&u).cloned() else { continue };
        if !spec.is_open(now) {
            continue;
        }
        let Some((spot, _)) = opts.spot(&u) else { continue };
        let Some(usdq) = opts.usd_per(&spec.quote_ccy) else { continue };
        // the house is short the clients' delta; the hedge account holds `have`
        let have = hedge.get(&u).copied().unwrap_or(0.0);
        let net = -client + have;
        if (net * spot * usdq).abs() <= limit {
            continue;
        }
        let size = super::f(spec.contract_size);
        let step = super::f(spec.lot_step);
        if size <= 0.0 || step <= 0.0 {
            continue;
        }
        let lots = ((net.abs() / size) / step).floor() * step;
        let Some(lots) = from_f64(lots).map(|l| crate::money::rdp(l, 8)).filter(|l| *l >= spec.lot_min) else { continue };
        let lots = lots.min(spec.lot_max);
        let side = if net > 0.0 { Side::Sell } else { Side::Buy };
        let sym = u.clone();
        let op: Op = Box::new(move |tx, env| {
            let mut req = OrderReq::market(&sym, side, lots);
            req.source = Source::System;
            req.platform = "Options hedger".into();
            req.comment = "options delta hedge".into();
            match trade::place_order(tx, env, req)? {
                PlaceResult::Filled { price, position_ticket, .. } => Ok(json!({"price": num(price), "ticket": position_ticket})),
                other => Ok(json!({"result": format!("{other:?}")})),
            }
        });
        let res = st.hub.exec(login, "system", None, "OPTIONS-HEDGE", "options delta hedge", None, op).await;
        let (status, price, error) = match &res {
            Ok(d) => ("filled", d.value["price"].as_f64().and_then(from_f64), None),
            Err(e) => ("failed", None, Some(format!("{e:?}"))),
        };
        let _ = sqlx::query("INSERT INTO option_hedges (tenant_id, login, symbol, side, volume, price, net_delta, hedge_before, status, error) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
            .bind(t.tenant_id)
            .bind(login)
            .bind(&u)
            .bind(side.as_str())
            .bind(lots)
            .bind(price)
            .bind(from_f64(-client).unwrap_or(ZERO))
            .bind(from_f64(have).unwrap_or(ZERO))
            .bind(status)
            .bind(&error)
            .execute(&st.pool)
            .await;
        match res {
            Ok(_) => {
                placed += 1;
                tracing::info!(tenant = t.tenant_id, %u, side = side.as_str(), lots = %lots, house_delta = -client, hedge = have, "options delta hedged");
            }
            Err(e) => tracing::warn!(tenant = t.tenant_id, %u, error = ?e, "options delta hedge order failed"),
        }
    }
    Ok(placed)
}

/// Runs forever (single engine instance).
pub async fn run(st: AppState) {
    tokio::time::sleep(Duration::from_secs(20)).await;
    let mut failing: BTreeMap<i64, bool> = BTreeMap::new();
    loop {
        for t in st.hub.shared.registry.all() {
            match hedge_tenant(&st, &t).await {
                Ok(_) => {
                    failing.remove(&t.tenant_id);
                }
                Err(e) => {
                    if failing.insert(t.tenant_id, true).is_none() {
                        tracing::warn!(tenant = t.tenant_id, error = %e, "options hedger pass failed");
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(10)).await;
        let _ = Utc::now();
    }
}
