//! Money in and out of an account (wallet transfers, staff adjustments, demo funding) and account settings.

use serde_json::json;

use super::{Env, Reject, Tx, metrics};
use crate::model::{Account, AccountKind, Book, Controls, TxnKind, acct_code, house_code};
use crate::money::{D, ZERO, num, r2};
use crate::specs::server_date;
use crate::state::{AccountState, Event};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Wallet → trading account.
    In,
    /// Trading account → wallet.
    Out,
}

/// Opens an account: the `account_opened` event plus the initial demo balance.
pub fn open_account(env: &Env, account: Account) -> Tx {
    let st = AccountState::new(account.clone());
    let mut tx = Tx { st, events: vec![Event::AccountOpened { account: account.clone() }], notes: vec![], audit: vec![], book_dirty: false, book_send: vec![], liquidate: false };
    if let Some(d) = &account.demo {
        let amt = d.initial_balance * account.usd_factor();
        tx.post(env, TxnKind::DemoInitial, format!("demo-initial:{}", account.login), "balance", "demo_funding", amt, None, None, None);
    }
    tx
}

/// Wallet ↔ trading account transfer (D3/D36). `amount_usd` is in USD; cent accounts receive USC (×100).
/// Double entry per currency: the wallet clearing account (USD) against the client balance, with an FX pair
/// of legs for cent accounts so every currency nets to 0.
pub fn transfer(tx: &mut Tx, env: &Env, dir: Direction, amount_usd: D, idem: &str, reference: Option<String>) -> Result<(i64, D), Reject> {
    let acc = tx.st.account.clone();
    if acc.kind == AccountKind::Demo {
        return Err(Reject::new("demo_account", "Transfers are only possible for live accounts"));
    }
    let usd = r2(amount_usd);
    if usd <= ZERO {
        return Err(Reject::new("invalid_amount", "Amount must be above 0"));
    }
    let amt = r2(usd * acc.usd_factor());
    if dir == Direction::Out {
        let w = metrics(env, &tx.st).withdrawable();
        if amt > w {
            return Err(Reject::new("insufficient_funds", format!("Not enough free funds: {} {} available", r2(w).normalize(), acc.ccy())));
        }
    } else if acc.status == crate::model::Status::Expired {
        return Err(Reject::new("account_status", "Account is expired"));
    } else if acc.status.is_retired() {
        return Err(Reject::new("account_status", format!("Account is {}", acc.status.as_str())));
    }
    let s = if dir == Direction::In { D::ONE } else { -D::ONE };
    let ccy = acc.ccy();
    let mut postings = vec![crate::model::Posting { account: acct_code(acc.login, "balance"), ccy: ccy.into(), amount: s * amt }];
    if acc.cent {
        postings.push(crate::model::Posting { account: house_code("fx", "USC"), ccy: "USC".into(), amount: -s * amt });
        postings.push(crate::model::Posting { account: house_code("fx", "USD"), ccy: "USD".into(), amount: s * usd });
    }
    postings.push(crate::model::Posting { account: house_code("wallet_clearing", "USD"), ccy: "USD".into(), amount: -s * usd });
    let txn = crate::model::LedgerTxn {
        id: env.ids.txn(),
        tenant_id: acc.tenant_id,
        idempotency_key: idem.to_string(),
        kind: if dir == Direction::In { TxnKind::TransferIn } else { TxnKind::TransferOut },
        login: acc.login,
        reference,
        reason_code: None,
        note: None,
        at: env.now,
        postings,
    };
    assert!(txn.is_balanced());
    let id = txn.id;
    tx.emit(Event::Ledger { txn });
    tx.note("balance", format!("{} {} {}", if dir == Direction::In { "Deposit" } else { "Withdrawal" }, amt.normalize(), ccy), json!({"amount": num(s * amt), "txn": id}));
    Ok((id, amt))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdjustKind {
    Deposit,
    Withdrawal,
    Adjustment,
    Credit,
    Bonus,
    /// House account capital (signed): never a client deposit. Not parseable from the admin balance API;
    /// only the house provisioning routes (api/social_house.rs) book it.
    HouseCapital,
}

impl AdjustKind {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "deposit" => Self::Deposit,
            "withdrawal" => Self::Withdrawal,
            "adjustment" | "balance" => Self::Adjustment,
            "credit" => Self::Credit,
            "bonus" => Self::Bonus,
            _ => return None,
        })
    }
}

/// Staff balance / credit / bonus change with a reason (D117/D118). `amount` is signed, account currency.
pub fn adjust(tx: &mut Tx, env: &Env, kind: AdjustKind, amount: D, idem: &str, reason_code: &str, note: &str) -> Result<i64, Reject> {
    let amount = r2(amount);
    if amount.is_zero() {
        return Err(Reject::new("invalid_amount", "Amount must not be 0"));
    }
    let m = metrics(env, &tx.st);
    let (sub, house, tk, amt) = match kind {
        AdjustKind::Deposit => ("balance", "external", TxnKind::Deposit, amount.abs()),
        AdjustKind::Withdrawal => {
            if amount.abs() > m.withdrawable() {
                return Err(Reject::new("insufficient_funds", format!("Not enough free funds: {} available", r2(m.withdrawable()).normalize())));
            }
            ("balance", "external", TxnKind::Withdrawal, -amount.abs())
        }
        AdjustKind::Adjustment => ("balance", "adjustments", TxnKind::Adjustment, amount),
        AdjustKind::Credit => {
            if tx.st.credit + amount < ZERO {
                return Err(Reject::new("invalid_amount", format!("Credit cannot go below 0 (current {})", tx.st.credit.normalize())));
            }
            ("credit", "credit_issued", TxnKind::Credit, amount)
        }
        AdjustKind::Bonus => {
            if tx.st.bonus + amount < ZERO {
                return Err(Reject::new("invalid_amount", format!("Bonus cannot go below 0 (current {})", tx.st.bonus.normalize())));
            }
            ("bonus", "bonus_issued", TxnKind::Bonus, amount)
        }
        AdjustKind::HouseCapital => {
            if amount < ZERO && amount.abs() > m.withdrawable() {
                return Err(Reject::new("insufficient_funds", format!("Not enough free funds: {} available", r2(m.withdrawable()).normalize())));
            }
            ("balance", "house_capital", TxnKind::HouseCapital, amount)
        }
    };
    let id = tx.post(env, tk, idem.to_string(), sub, house, amt, None, Some(reason_code.to_string()), Some(note.to_string())).expect("non-zero amount");
    tx.note("balance", format!("{} {} {}", tk.as_str(), amt.normalize(), tx.st.account.ccy()), json!({"amount": num(amt), "kind": sub, "txn": id}));
    Ok(id)
}

/// Back Office "Balance & credit" operation (manual adjustment, requested through the wallet service).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdjustOp {
    /// Add funds to the balance.
    Add,
    /// Deduct funds from the balance.
    Deduct,
    /// Give credit (non-withdrawable, counts toward equity and margin, D29).
    CreditIn,
    /// Take credit back.
    CreditOut,
}

impl AdjustOp {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "add" => Self::Add,
            "deduct" => Self::Deduct,
            "credit_in" => Self::CreditIn,
            "credit_out" => Self::CreditOut,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Deduct => "deduct",
            Self::CreditIn => "credit_in",
            Self::CreditOut => "credit_out",
        }
    }
    pub fn is_credit(self) -> bool {
        matches!(self, Self::CreditIn | Self::CreditOut)
    }
}

/// Reason categories of a manual adjustment. Only `deposit` on Add and `withdrawal` on Deduct are real money
/// (ledger kinds `deposit` / `withdrawal`, counted as client deposits / FTDs by the reports); every other
/// category is booked as `adjustment` (balance) or `credit` and is never counted as a deposit.
pub const ADJUST_CATEGORIES: &[&str] = &["deposit", "withdrawal", "correction", "compensation", "bonus", "fee", "chargeback", "other"];

pub struct StaffAdjust<'a> {
    pub op: AdjustOp,
    pub category: &'a str,
    /// Positive, in the account currency (USC for cent accounts).
    pub amount: D,
    /// Super Admin override of the free-margin limit (never of negative balance protection or the credit held).
    pub force: bool,
    pub key: &'a str,
    pub reason_code: &'a str,
    /// Client-visible statement text (ledger note); falls back to a neutral label.
    pub statement: &'a str,
}

/// Balance, credit, equity, margin and limits of the account right now (account currency).
pub fn funds_snapshot(env: &Env, st: &AccountState) -> serde_json::Value {
    let m = metrics(env, st);
    json!({
        "balance": num(r2(m.balance)), "credit": num(r2(m.credit)), "bonus": num(r2(m.bonus)), "equity": num(r2(m.equity)),
        "margin": num(r2(m.margin)), "freeMargin": num(r2(m.free_margin)), "withdrawable": num(r2(m.withdrawable())),
        "marginLevel": m.level.map(|l| num(r2(l))).unwrap_or(serde_json::Value::Null), "currency": st.account.ccy(),
    })
}

/// The most a staff member may deduct / take back right now: (limit without force, limit with force).
/// Deduct: the free own funds (`withdrawable`), or with force the whole balance (negative balance protection
/// is always on, so a forced deduction never takes the balance below 0). Take credit: the free margin capped
/// at the credit held, or with force the credit held.
pub fn adjust_limits(env: &Env, st: &AccountState, op: AdjustOp) -> (D, D) {
    let m = metrics(env, st);
    match op {
        AdjustOp::Deduct => (r2(m.withdrawable()), r2(m.balance.max(ZERO))),
        AdjustOp::CreditOut => (r2(m.free_margin.max(ZERO).min(m.credit)), r2(m.credit)),
        AdjustOp::Add | AdjustOp::CreditIn => (D::MAX, D::MAX),
    }
}

/// Books a manual balance / credit adjustment. Returns the ledger txn id.
/// - Add: `deposit` (house `external`) for category deposit, else `adjustment` (house `adjustments`).
/// - Deduct: `withdrawal` (house `external`) for category withdrawal, else `adjustment`; limited to the free own
///   funds unless `force` (then to the balance: never below 0).
/// - Give / take credit: `credit` (house `credit_issued`); taking back is limited to the credit held and,
///   unless `force`, to the free margin (open positions keep their margin).
/// - Demo accounts book every leg against `demo_funding` as `adjustment` / `credit`: never real money.
///
/// After the posting the margin level is re-checked (a forced deduction can trigger margin call or stop-out).
pub fn staff_adjust(tx: &mut Tx, env: &Env, a: StaffAdjust) -> Result<i64, Reject> {
    if !ADJUST_CATEGORIES.contains(&a.category) {
        return Err(Reject::new("invalid_category", format!("Unknown reason {}", a.category)));
    }
    let amount = r2(a.amount);
    if amount <= ZERO || amount != a.amount {
        return Err(Reject::new("invalid_amount", "Amount must be above 0 with at most 2 decimals"));
    }
    let acc = tx.st.account.clone();
    let ccy = acc.ccy();
    if a.op == AdjustOp::Add && a.category == "withdrawal" {
        return Err(Reject::new("invalid_category", "“Withdrawal (paid externally)” is a deduction"));
    }
    if a.op == AdjustOp::Deduct && a.category == "deposit" {
        return Err(Reject::new("invalid_category", "“Deposit (external payment received)” adds funds"));
    }
    let (strict, forced) = adjust_limits(env, &tx.st, a.op);
    match a.op {
        AdjustOp::Deduct if amount > strict => {
            if !a.force {
                return Err(Reject::new(
                    "insufficient_funds",
                    format!("Deducting {} {ccy} exceeds the free funds: {} {ccy} can be deducted (balance {}, free margin {})", amount.normalize(), strict.normalize(), r2(tx.st.balance).normalize(), r2(metrics(env, &tx.st).free_margin).normalize()),
                ));
            }
            if amount > forced {
                return Err(Reject::new("negative_balance", format!("Negative balance protection: a forced deduction can take at most the balance, {} {ccy}", forced.normalize())));
            }
        }
        AdjustOp::CreditOut if amount > forced => {
            return Err(Reject::new("insufficient_credit", format!("The account holds {} {ccy} credit; you can't take back more", forced.normalize())));
        }
        AdjustOp::CreditOut if amount > strict && !a.force => {
            return Err(Reject::new("insufficient_funds", format!("Taking back {} {ccy} credit exceeds the free margin: {} {ccy} can be taken while the positions are open", amount.normalize(), strict.normalize())));
        }
        _ => {}
    }
    let demo = acc.kind == AccountKind::Demo;
    let (sub, house, kind, signed) = match a.op {
        AdjustOp::Add if a.category == "deposit" && !demo => ("balance", "external", TxnKind::Deposit, amount),
        AdjustOp::Deduct if a.category == "withdrawal" && !demo => ("balance", "external", TxnKind::Withdrawal, -amount),
        AdjustOp::Add => ("balance", if demo { "demo_funding" } else { "adjustments" }, TxnKind::Adjustment, amount),
        AdjustOp::Deduct => ("balance", if demo { "demo_funding" } else { "adjustments" }, TxnKind::Adjustment, -amount),
        AdjustOp::CreditIn => ("credit", if demo { "demo_funding" } else { "credit_issued" }, TxnKind::Credit, amount),
        AdjustOp::CreditOut => ("credit", if demo { "demo_funding" } else { "credit_issued" }, TxnKind::Credit, -amount),
    };
    let statement = a.statement.trim();
    let statement = if statement.is_empty() {
        match a.op {
            AdjustOp::Add if kind == TxnKind::Deposit => "Deposit",
            AdjustOp::Deduct if kind == TxnKind::Withdrawal => "Withdrawal",
            AdjustOp::Add | AdjustOp::Deduct => "Balance adjustment",
            AdjustOp::CreditIn => "Credit",
            AdjustOp::CreditOut => "Credit removed",
        }
    } else {
        statement
    };
    let id = tx.post(env, kind, a.key.to_string(), sub, house, signed, None, Some(a.reason_code.to_string()), Some(statement.to_string())).expect("non-zero amount");
    let what = match a.op {
        AdjustOp::Add => "added to your balance",
        AdjustOp::Deduct => "deducted from your balance",
        AdjustOp::CreditIn => "credit added",
        AdjustOp::CreditOut => "credit removed",
    };
    tx.note("balance", format!("{} {ccy} {what}", amount.normalize()), json!({"amount": num(signed), "kind": sub, "txn": id}));
    super::risk::check_margin(tx, env);
    Ok(id)
}

/// D8: user-triggered refill back to the initial demo balance, capped at N per server day.
pub fn demo_refill(tx: &mut Tx, env: &Env) -> Result<D, Reject> {
    let acc = tx.st.account.clone();
    let d = acc.demo.clone().ok_or_else(|| Reject::new("not_demo", "Refill is only available on demo accounts"))?;
    if acc.status == crate::model::Status::Expired {
        return Err(Reject::new("account_status", "This demo account has expired"));
    }
    if acc.status.is_retired() {
        return Err(Reject::new("account_status", format!("This account is {}", acc.status.as_str())));
    }
    let day = server_date(env.now);
    let used = if tx.st.refill_day == Some(day) { tx.st.refills } else { 0 };
    if used >= d.refills_per_day {
        return Err(Reject::new("refill_limit", format!("Refill limit reached ({} per day)", d.refills_per_day)));
    }
    let target = d.initial_balance * acc.usd_factor();
    let amt = r2(target - tx.st.balance);
    if amt <= ZERO {
        return Err(Reject::new("refill_not_needed", "Balance is already at or above the initial demo balance"));
    }
    tx.emit(Event::RefillCounted { day });
    tx.post(env, TxnKind::DemoRefill, format!("demo-refill:{}:{}:{}", acc.login, day, used + 1), "balance", "demo_funding", amt, None, None, None);
    Ok(amt)
}

/// D15: leverage from the group list, only while flat.
pub fn change_leverage(tx: &mut Tx, env: &Env, leverage: u32, staff: bool) -> Result<(u32, u32), Reject> {
    if !env.group.leverages.contains(&leverage) {
        return Err(Reject::new("invalid_leverage", format!("Leverage 1:{leverage} is not offered in this group")));
    }
    if !tx.st.positions.is_empty() && !staff {
        return Err(Reject::new("positions_open", "Leverage can only be changed when there are no open positions"));
    }
    let from = tx.st.account.leverage;
    if from == leverage {
        return Err(Reject::new("no_change", "Nothing changed"));
    }
    let mut a = tx.st.account.clone();
    a.leverage = leverage;
    tx.emit(Event::AccountUpdated { account: a, change: format!("leverage 1:{from} → 1:{leverage}") });
    Ok((from, leverage))
}

/// Moves the account to another group (`env.group` = its current one). Mode (netting/hedging) can only change while
/// flat, the cent flag never changes (the ledger currency is fixed at opening), and neither does the product: a CFD
/// account never becomes an Options account or the reverse (CFD / Options account split).
pub fn change_group(tx: &mut Tx, env: &Env, new: &crate::rules::Group) -> Result<(String, String), Reject> {
    let a0 = tx.st.account.clone();
    if new.code == a0.group {
        return Err(Reject::new("no_change", "Nothing changed"));
    }
    if new.cent != a0.cent {
        return Err(Reject::new("invalid_group", "An account cannot move between cent and standard currency groups"));
    }
    if new.product != env.group.product {
        return Err(Reject::new(
            "product_mismatch",
            match env.group.product {
                crate::rules::Product::Cfd => format!("A CFD account can't move to {}, an Options group: open an Options account instead", new.name),
                crate::rules::Product::Options => format!("An Options account can't move to {}, a CFD group: open a CFD account instead", new.name),
            },
        ));
    }
    if !new.allows(a0.kind.as_str()) {
        return Err(Reject::new("invalid_group", format!("Group {} does not accept {} accounts", new.name, a0.kind.as_str())));
    }
    let flat = tx.st.positions.is_empty() && tx.st.orders.is_empty() && tx.st.book.is_idle();
    let mode_changes = (new.mode == crate::model::Mode::Netting) != (a0.mode == crate::model::Mode::Netting);
    if mode_changes && !flat {
        return Err(Reject::new("positions_open", "Close all positions and orders before switching between netting and hedging"));
    }
    let mut a = a0.clone();
    a.group = new.code.clone();
    a.mode = new.mode;
    if !new.leverages.contains(&a.leverage) {
        if !tx.st.positions.is_empty() {
            return Err(Reject::new("invalid_leverage", format!("Leverage 1:{} is not offered in {}; close positions first", a.leverage, new.name)));
        }
        a.leverage = new.default_leverage;
    }
    tx.emit(Event::AccountUpdated { account: a, change: format!("group {} → {}", a0.group, new.code) });
    Ok((a0.group, new.code.clone()))
}

/// "CFD" / "Options" in refusal messages.
pub fn product_name(p: crate::rules::Product) -> &'static str {
    match p {
        crate::rules::Product::Cfd => "CFD",
        crate::rules::Product::Options => "Options",
    }
}

pub fn set_status(tx: &mut Tx, status: crate::model::Status) -> Result<(), Reject> {
    if tx.st.account.status == status {
        return Err(Reject::new("no_change", "Nothing changed"));
    }
    let mut a = tx.st.account.clone();
    let from = a.status;
    a.status = status;
    tx.emit(Event::AccountUpdated { account: a, change: format!("status {} → {}", from.as_str(), status.as_str()) });
    Ok(())
}

pub fn set_controls(tx: &mut Tx, controls: Controls) -> Result<Controls, Reject> {
    let before = tx.st.account.controls.clone();
    let mut a = tx.st.account.clone();
    a.controls = controls;
    tx.emit(Event::AccountUpdated { account: a, change: "dealer controls".into() });
    Ok(before)
}

pub fn set_route(tx: &mut Tx, book: Option<Book>) -> Result<Option<Book>, Reject> {
    let before = tx.st.account.route_override;
    if before == book {
        return Err(Reject::new("no_change", "Nothing changed"));
    }
    let mut a = tx.st.account.clone();
    a.route_override = book;
    tx.emit(Event::AccountUpdated { account: a, change: format!("route {:?} → {:?}", before, book) });
    Ok(before)
}

/// Archives a flat account (B1): status `archived`, with the prior status kept for restore. Credit and bonus
/// still on the account are forfeited (booked back to the house). Ok(false) = already archived (no change).
pub fn archive(tx: &mut Tx, env: &Env, by: &str, reason_code: &str, client_restorable: bool) -> Result<bool, Reject> {
    use crate::model::{Lifecycle, Status};
    let a0 = tx.st.account.clone();
    match a0.status {
        Status::Archived => return Ok(false),
        Status::Closed => return Err(Reject::new("account_status", "This account is closed")),
        _ => {}
    }
    if !tx.st.positions.is_empty() || !tx.st.orders.is_empty() || !tx.st.book.is_idle() {
        return Err(Reject::new("not_empty", "Close all positions and cancel all orders before archiving"));
    }
    let v = tx.st.version;
    for (sub, house, kind, amt) in [("credit", "credit_issued", TxnKind::Credit, tx.st.credit), ("bonus", "bonus_issued", TxnKind::Bonus, tx.st.bonus)] {
        if amt > ZERO {
            tx.post(env, kind, format!("archive:{}:{sub}:{v}", a0.login), sub, house, -amt, None, Some(reason_code.to_string()), Some(format!("{sub} forfeited on archive")));
        }
    }
    let mut a = a0.clone();
    a.status = Status::Archived;
    a.lifecycle = Some(Lifecycle { prior_status: a0.status, archived_at: env.now, reason_code: reason_code.to_string(), by: by.to_string(), client_restorable, closed_at: None });
    tx.emit(Event::AccountUpdated { account: a, change: format!("status {} → archived", a0.status.as_str()) });
    Ok(true)
}

/// Restores an archived account to its prior status (an expired demo comes back active). `client` = the
/// account owner asks, which needs `client_restorable`.
pub fn restore(tx: &mut Tx, client: bool) -> Result<crate::model::Status, Reject> {
    use crate::model::Status;
    let a0 = tx.st.account.clone();
    if a0.status != Status::Archived {
        return Err(Reject::new("account_status", format!("Only archived accounts can be restored (this one is {})", a0.status.as_str())));
    }
    let lc = a0.lifecycle.clone();
    if client && !lc.as_ref().is_some_and(|l| l.client_restorable) {
        return Err(Reject::new("not_restorable", "This account was archived by the broker; contact support to restore it"));
    }
    let to = match lc.map(|l| l.prior_status).unwrap_or(Status::Active) {
        Status::Expired | Status::Archived | Status::Closed => Status::Active,
        s => s,
    };
    let mut a = a0;
    a.status = to;
    a.lifecycle = None;
    tx.emit(Event::AccountUpdated { account: a, change: format!("status archived → {} (restored)", to.as_str()) });
    Ok(to)
}

/// Client nickname (≤ 32 characters).
pub fn rename(tx: &mut Tx, name: &str) -> Result<(), Reject> {
    let name = name.trim();
    if name.chars().count() > 32 {
        return Err(Reject::new("invalid_name", "The name can be at most 32 characters"));
    }
    if tx.st.account.name == name {
        return Ok(());
    }
    let mut a = tx.st.account.clone();
    a.name = name.to_string();
    tx.emit(Event::AccountUpdated { account: a, change: "name".into() });
    Ok(())
}

/// Closes an account permanently (B1/B12, after the closure request was approved): status `closed`, final for the
/// client; the login is never reused. The account must be flat with no balance left (C3); credit and bonus still
/// on it are forfeited like on archive. An archived account keeps its archive date and the status it had before
/// archiving (what a Super Admin reopen goes back to). Ok(false) = already closed.
pub fn close_account(tx: &mut Tx, env: &Env, by: &str, reason_code: &str) -> Result<bool, Reject> {
    use crate::model::{Lifecycle, Status};
    let a0 = tx.st.account.clone();
    if a0.status == Status::Closed {
        return Ok(false);
    }
    if !tx.st.positions.is_empty() || !tx.st.orders.is_empty() || !tx.st.book.is_idle() {
        return Err(Reject::new("not_empty", "Close all positions and cancel all orders before closing the account"));
    }
    if r2(tx.st.balance) != ZERO {
        return Err(Reject::new("balance_remaining", format!("The account still holds {} {}; move it out first", r2(tx.st.balance).normalize(), a0.ccy())));
    }
    let v = tx.st.version;
    for (sub, house, kind, amt) in [("credit", "credit_issued", TxnKind::Credit, tx.st.credit), ("bonus", "bonus_issued", TxnKind::Bonus, tx.st.bonus)] {
        if amt > ZERO {
            tx.post(env, kind, format!("close:{}:{sub}:{v}", a0.login), sub, house, -amt, None, Some(reason_code.to_string()), Some(format!("{sub} forfeited on closure")));
        }
    }
    let (prior, archived_at) = match (&a0.lifecycle, a0.status) {
        (Some(l), Status::Archived) => (l.prior_status, l.archived_at),
        _ => (a0.status, env.now),
    };
    let mut a = a0.clone();
    a.status = Status::Closed;
    a.lifecycle = Some(Lifecycle { prior_status: prior, archived_at, reason_code: reason_code.to_string(), by: by.to_string(), client_restorable: false, closed_at: Some(env.now) });
    tx.emit(Event::AccountUpdated { account: a, change: format!("status {} → closed", a0.status.as_str()) });
    Ok(true)
}

/// Reopens a closed account (C12, Super Admin with four-eyes) to the status it had before it was retired (an
/// expired demo comes back active).
pub fn reopen(tx: &mut Tx) -> Result<crate::model::Status, Reject> {
    use crate::model::Status;
    let a0 = tx.st.account.clone();
    if a0.status != Status::Closed {
        return Err(Reject::new("account_status", format!("Only closed accounts can be reopened (this one is {})", a0.status.as_str())));
    }
    let to = match a0.lifecycle.as_ref().map(|l| l.prior_status).unwrap_or(Status::Active) {
        Status::Expired | Status::Archived | Status::Closed => Status::Active,
        s => s,
    };
    let mut a = a0;
    a.status = to;
    a.lifecycle = None;
    tx.emit(Event::AccountUpdated { account: a, change: format!("status closed → {} (reopened)", to.as_str()) });
    Ok(to)
}

/// D8 extension: refill a demo account to a chosen balance (100 – 1 000 000 USD), counted against the same daily
/// refill limit. Topping up books `demo_refill`; a lower target books the difference back to `demo_funding` (the
/// client can also start over with less).
pub fn demo_refill_to(tx: &mut Tx, env: &Env, target_usd: D) -> Result<D, Reject> {
    let acc = tx.st.account.clone();
    let d = acc.demo.clone().ok_or_else(|| Reject::new("not_demo", "Refill is only available on demo accounts"))?;
    if acc.status == crate::model::Status::Expired {
        return Err(Reject::new("account_status", "This demo account has expired"));
    }
    if acc.status.is_retired() {
        return Err(Reject::new("account_status", format!("This account is {}", acc.status.as_str())));
    }
    let target_usd = r2(target_usd);
    if target_usd < D::from(100) || target_usd > D::from(1_000_000) {
        return Err(Reject::new("invalid_amount", "Choose a demo balance between 100 and 1,000,000 USD"));
    }
    if !tx.st.positions.is_empty() || !tx.st.orders.is_empty() || !tx.st.book.is_idle() {
        return Err(Reject::new("positions_open", "Close all positions and orders before setting a new demo balance"));
    }
    let day = server_date(env.now);
    let used = if tx.st.refill_day == Some(day) { tx.st.refills } else { 0 };
    if used >= d.refills_per_day {
        return Err(Reject::new("refill_limit", format!("Refill limit reached ({} per day)", d.refills_per_day)));
    }
    let amt = r2(target_usd * acc.usd_factor() - tx.st.balance);
    if amt.is_zero() {
        return Err(Reject::new("refill_not_needed", "The balance is already at this amount"));
    }
    tx.emit(Event::RefillCounted { day });
    let kind = if amt > ZERO { TxnKind::DemoRefill } else { TxnKind::Adjustment };
    tx.post(env, kind, format!("demo-refill:{}:{}:{}", acc.login, day, used + 1), "balance", "demo_funding", amt, None, None, Some("Demo balance reset".into()));
    Ok(amt)
}
