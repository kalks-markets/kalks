//! File outputs: CSV and XLSX for any table set, and the branded PDF statement.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use rust_xlsxwriter::{Color, Format, FormatAlign, FormatBorder, Workbook};
use serde_json::Value;

use crate::logo;
use crate::pdf::{A4_H, A4_W, Doc, Font, Page, Rgb, fit, text_width};
use crate::statement::{DealRow, Statement, reason_label};
use crate::time;
use crate::upstream::num;

#[derive(Clone, Debug)]
pub enum Cell {
    Text(String),
    Num(f64, u8),
    Int(i64),
    Time(DateTime<Utc>),
    Empty,
}

impl Cell {
    pub fn money(d: Decimal) -> Cell {
        Cell::Num(d.to_f64().unwrap_or(0.0), 2)
    }
    pub fn text(s: impl Into<String>) -> Cell {
        Cell::Text(s.into())
    }
    fn csv(&self) -> String {
        match self {
            Cell::Text(s) => {
                // spreadsheet formula injection: free text never starts with = + - @
                let mut s = s.clone();
                if s.starts_with(['=', '+', '-', '@', '\t', '\r']) {
                    s.insert(0, '\'');
                }
                if s.contains([',', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s }
            }
            Cell::Num(x, dp) => format!("{:.*}", *dp as usize, x),
            Cell::Int(i) => i.to_string(),
            Cell::Time(t) => time::fmt_server(*t),
            Cell::Empty => String::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Table {
    pub name: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<Cell>>,
}

impl Table {
    pub fn new(name: &str, headers: &[&str]) -> Self {
        Self { name: name.to_string(), headers: headers.iter().map(|s| s.to_string()).collect(), rows: vec![] }
    }
}

/// One CSV; several tables are written one after another with a title line and a blank line between.
pub fn csv(tables: &[Table]) -> Vec<u8> {
    let mut out = String::from("\u{feff}");
    let multi = tables.len() > 1;
    for (i, t) in tables.iter().enumerate() {
        if multi {
            if i > 0 {
                out.push_str("\r\n");
            }
            out.push_str(&Cell::Text(t.name.clone()).csv());
            out.push_str("\r\n");
        }
        out.push_str(&t.headers.iter().map(|h| Cell::Text(h.clone()).csv()).collect::<Vec<_>>().join(","));
        out.push_str("\r\n");
        for r in &t.rows {
            out.push_str(&r.iter().map(Cell::csv).collect::<Vec<_>>().join(","));
            out.push_str("\r\n");
        }
    }
    out.into_bytes()
}

fn sheet_name(s: &str) -> String {
    let clean: String = s.chars().filter(|c| !"[]:*?/\\".contains(*c)).collect();
    clean.chars().take(31).collect()
}

/// One sheet per table, header row styled and frozen, times as server-time text, numbers as numbers.
pub fn xlsx(tables: &[Table]) -> anyhow::Result<Vec<u8>> {
    let mut wb = Workbook::new();
    let head = Format::new().set_bold().set_font_color(Color::White).set_background_color(Color::RGB(0x15151a)).set_border_bottom(FormatBorder::Thin);
    let money = Format::new().set_num_format("#,##0.00");
    let n5 = Format::new().set_num_format("0.00000");
    let right = Format::new().set_align(FormatAlign::Right);
    for t in tables {
        let ws = wb.add_worksheet();
        ws.set_name(sheet_name(&t.name))?;
        for (c, h) in t.headers.iter().enumerate() {
            ws.write_string_with_format(0, c as u16, h, &head)?;
            ws.set_column_width(c as u16, (h.len() as f64 + 4.0).max(12.0))?;
        }
        ws.set_freeze_panes(1, 0)?;
        for (r, row) in t.rows.iter().enumerate() {
            let r = r as u32 + 1;
            for (c, cell) in row.iter().enumerate() {
                let c = c as u16;
                match cell {
                    Cell::Text(s) => {
                        ws.write_string(r, c, s)?;
                    }
                    Cell::Num(x, dp) => {
                        ws.write_number_with_format(r, c, *x, if *dp <= 2 { &money } else { &n5 })?;
                    }
                    Cell::Int(i) => {
                        ws.write_number(r, c, *i as f64)?;
                    }
                    Cell::Time(tm) => {
                        ws.write_string_with_format(r, c, time::fmt_server(*tm), &right)?;
                    }
                    Cell::Empty => {}
                }
            }
        }
    }
    Ok(wb.save_to_buffer()?)
}

/* ------------------------------------------------------------------ */
/* Statement tables (CSV / XLSX)                                       */
/* ------------------------------------------------------------------ */

fn side_label(d: &DealRow) -> String {
    d.position_side.clone()
}

pub fn statement_tables(s: &Statement, sections: &Sections) -> Vec<Table> {
    let mut v = vec![];
    let mut sum = Table::new("Summary", &["Item", "Amount", "Currency"]);
    let cur = s.account.currency.clone();
    let add = |t: &mut Table, k: &str, d: Decimal| t.rows.push(vec![Cell::text(k), Cell::money(d), Cell::text(cur.clone())]);
    let m = &s.summary;
    sum.rows.push(vec![Cell::text("Account"), Cell::Int(s.account.login), Cell::text(format!("{} · {}", s.account.group_name, s.account.kind))]);
    sum.rows.push(vec![Cell::text("Period from (server time)"), Cell::Time(s.from), Cell::Empty]);
    sum.rows.push(vec![Cell::text("Period to (server time)"), Cell::Time(s.to), Cell::Empty]);
    add(&mut sum, "Opening balance", m.opening_balance);
    add(&mut sum, "Deposits", m.deposits);
    add(&mut sum, "Withdrawals", m.withdrawals);
    add(&mut sum, "Closed trade results (incl. swap)", m.trade_results);
    add(&mut sum, "Commission", m.commission);
    add(&mut sum, "Performance fees", m.performance_fees);
    if has_options(s) {
        add(&mut sum, "Option premiums (paid - / received +)", m.option_premiums);
        add(&mut sum, "Option settlements", m.option_settlements);
    }
    add(&mut sum, "Adjustments", m.adjustments);
    add(&mut sum, "Closing balance", m.closing_balance);
    add(&mut sum, "Credit and bonus", m.closing_credit);
    add(&mut sum, "Net trading result (realised)", m.net_pnl);
    v.push(sum);

    let mut tr = Table::new("Closed trades", &["Close time", "Deal", "Position", "Symbol", "Type", "Volume", "Open time", "Open price", "Close price", "Commission", "Swap", "Profit", "Net", "Reason"]);
    for d in &s.trades {
        tr.rows.push(vec![
            Cell::Time(d.time),
            Cell::Int(d.id),
            Cell::Int(d.position_ticket),
            Cell::text(&d.symbol),
            Cell::text(side_label(d)),
            Cell::Num(d.volume.to_f64().unwrap_or(0.0), 2),
            d.open_time.map(Cell::Time).unwrap_or(Cell::Empty),
            d.open_price.map(|p| Cell::Num(p.to_f64().unwrap_or(0.0), 5)).unwrap_or(Cell::Empty),
            Cell::Num(d.price.to_f64().unwrap_or(0.0), 5),
            Cell::money(-d.commission),
            Cell::money(d.swap),
            Cell::money(d.profit),
            Cell::money(d.net()),
            Cell::text(reason_label(&d.reason)),
        ]);
    }
    v.push(tr);

    if has_options(s) {
        let o = &s.options;
        let mut os = Table::new("Options summary", &["Item", "Amount", "Currency"]);
        add(&mut os, "Premiums paid", o.premiums_paid);
        add(&mut os, "Premiums received", o.premiums_received);
        add(&mut os, "Settlements received (expiry payouts and knock-out rebates)", o.settlements_received);
        add(&mut os, "Settlements paid (sold options expired in the money)", o.settlements_paid);
        add(&mut os, "Option commission", o.commission);
        add(&mut os, "Realised option P&L", o.realised_pnl);
        add(&mut os, "Net option result (P&L - commission)", o.net);
        v.push(os);
        let mut ot = Table::new(
            "Options",
            &["Time", "Deal", "Position", "Series", "Underlying", "Type", "Strike", "Expiry", "Side", "Event", "Contracts", "Premium", "Fixing", "Cash", "Commission", "Realised P&L", "Note"],
        );
        for d in &o.deals {
            ot.rows.push(vec![
                Cell::Time(d.time),
                Cell::Int(d.id),
                Cell::Int(d.position_ticket),
                Cell::text(&d.symbol),
                Cell::text(d.option_text("underlying")),
                Cell::text(option_right(d)),
                Cell::text(d.option_text("strike")),
                Cell::text(d.option_text("expiry")),
                Cell::text(&d.side),
                Cell::text(option_event(d)),
                Cell::Num(d.volume.to_f64().unwrap_or(0.0), 2),
                Cell::Num(d.price.to_f64().unwrap_or(0.0), 5),
                d.option_fixing().map(|x| Cell::Num(x.to_f64().unwrap_or(0.0), 5)).unwrap_or(Cell::Empty),
                Cell::money(d.option_cash()),
                Cell::money(-d.option_commission()),
                if d.is_exit() { Cell::money(d.profit) } else { Cell::Empty },
                Cell::text(if d.reversed { "reversed (correction)" } else { "" }),
            ]);
        }
        v.push(ot);
    }

    if sections.deals {
        let mut dt = Table::new("Deals", &["Time", "Deal", "Position", "Symbol", "Direction", "Entry", "Volume", "Price", "Commission", "Swap", "Profit", "Reason", "Comment"]);
        for d in &s.deals {
            // CFD commission is booked on the entry deal; an option deal charges its own (opens and closes)
            let comm = if d.is_option() { d.option_commission() } else if d.entry == "in" { d.commission } else { Decimal::ZERO };
            dt.rows.push(vec![
                Cell::Time(d.time),
                Cell::Int(d.id),
                Cell::Int(d.position_ticket),
                Cell::text(&d.symbol),
                Cell::text(&d.side),
                Cell::text(&d.entry),
                Cell::Num(d.volume.to_f64().unwrap_or(0.0), 2),
                Cell::Num(d.price.to_f64().unwrap_or(0.0), 5),
                Cell::money(-comm),
                Cell::money(d.swap),
                Cell::money(d.profit),
                Cell::text(reason_label(&d.reason)),
                Cell::text(if d.reversed { format!("reversed {}", d.comment) } else { d.comment.clone() }),
            ]);
        }
        v.push(dt);
    }

    let mut lg = Table::new("Ledger", &["Time", "Transaction", "Type", "Sub-ledger", "Amount", "Balance", "Reference", "Note"]);
    for l in &s.ledger {
        lg.rows.push(vec![
            Cell::Time(l.at),
            Cell::Int(l.txn),
            Cell::text(&l.label),
            Cell::text(&l.sub_ledger),
            Cell::money(l.amount),
            Cell::money(l.balance),
            Cell::text(l.reference.clone().unwrap_or_default()),
            Cell::text(l.note.clone().unwrap_or_default()),
        ]);
    }
    v.push(lg);

    if sections.charges {
        let c = &s.charges;
        let mut ch = Table::new("Charges", &["Charge", "Amount", "Currency", "Note"]);
        let mut row = |k: &str, d: Decimal, note: &str| ch.rows.push(vec![Cell::text(k), Cell::money(d), Cell::text(cur.clone()), Cell::text(note)]);
        row("Commission", c.commission, "CFDs: round turn per lot, charged when a position opens. Options: per contract on every trade (open and close)");
        row("Swap paid", c.swap_paid, "Overnight financing charged on closed positions");
        row("Swap earned", c.swap_earned, "Positive overnight financing");
        row("Performance fees", c.performance_fees, "Copy trading / PAMM");
        row("Total charges", c.total, "Commission + swap paid + performance fees");
        row("Spread cost (estimate)", c.spread_estimate, "Informational: already included in the prices you traded at");
        v.push(ch);
    }

    if sections.open {
        let mut op = Table::new("Open positions", &["Position", "Open time", "Symbol", "Type", "Volume", "Open price", "S/L", "T/P", "Market price", "Swap", "Profit"]);
        for p in &s.positions {
            op.rows.push(vec![
                Cell::Int(p["ticket"].as_i64().unwrap_or(0)),
                p["openTime"].as_str().and_then(|x| DateTime::parse_from_rfc3339(x).ok()).map(|t| Cell::Time(t.with_timezone(&Utc))).unwrap_or(Cell::Empty),
                Cell::text(p["symbol"].as_str().unwrap_or("")),
                Cell::text(p["side"].as_str().unwrap_or("")),
                Cell::Num(num(&p["volume"]), 2),
                Cell::Num(num(&p["openPrice"]), 5),
                if p["sl"].is_null() { Cell::Empty } else { Cell::Num(num(&p["sl"]), 5) },
                if p["tp"].is_null() { Cell::Empty } else { Cell::Num(num(&p["tp"]), 5) },
                Cell::Num(num(&p["currentPrice"]), 5),
                Cell::Num(num(&p["swap"]), 2),
                Cell::Num(num(&p["profit"]), 2),
            ]);
        }
        v.push(op);
        let mut po = Table::new("Pending orders", &["Order", "Placed", "Symbol", "Type", "Volume", "Price", "S/L", "T/P", "Expiry"]);
        for o in &s.orders {
            po.rows.push(vec![
                Cell::Int(o["ticket"].as_i64().unwrap_or(0)),
                o["placedAt"].as_str().and_then(|x| DateTime::parse_from_rfc3339(x).ok()).map(|t| Cell::Time(t.with_timezone(&Utc))).unwrap_or(Cell::Empty),
                Cell::text(o["symbol"].as_str().unwrap_or("")),
                Cell::text(order_type(o)),
                Cell::Num(num(&o["volume"]), 2),
                Cell::Num(num(&o["price"]), 5),
                if o["sl"].is_null() { Cell::Empty } else { Cell::Num(num(&o["sl"]), 5) },
                if o["tp"].is_null() { Cell::Empty } else { Cell::Num(num(&o["tp"]), 5) },
                Cell::text(o["expiryAt"].as_str().or(o["expiry"].as_str()).unwrap_or("GTC")),
            ]);
        }
        v.push(po);
    }
    v
}

/// The statement has options activity: option deals, option ledger postings or open option positions.
pub fn has_options(s: &Statement) -> bool {
    !s.options.is_empty() || !s.summary.option_premiums.is_zero() || !s.summary.option_settlements.is_zero() || s.positions.iter().any(|p| p["option"].is_object())
}

fn option_right(d: &DealRow) -> &'static str {
    match d.option_text("right").as_str() {
        "call" | "C" => "Call",
        "put" | "P" => "Put",
        _ => match d.symbol.rsplit('-').next() {
            Some("C") => "Call",
            Some("P") => "Put",
            _ => "",
        },
    }
}

/// What an option deal was: an open (buy / sell), a close, or the expiry / knock-out / stop-out that ended it.
fn option_event(d: &DealRow) -> String {
    if !d.is_exit() {
        return if d.side == "buy" { "Bought (open)".into() } else { "Sold (open)".into() };
    }
    match d.reason.as_str() {
        "client" => "Closed".into(),
        r => reason_label(r).into(),
    }
}

fn order_type(o: &Value) -> String {
    let t = o["type"].as_str().unwrap_or("").replace('_', " ");
    format!("{} {}", o["side"].as_str().unwrap_or(""), t)
}

#[derive(Clone, Copy, Debug)]
pub struct Sections {
    pub open: bool,
    pub charges: bool,
    pub deals: bool,
}

impl Default for Sections {
    fn default() -> Self {
        Sections { open: true, charges: true, deals: true }
    }
}

/* ------------------------------------------------------------------ */
/* PDF statement                                                       */
/* ------------------------------------------------------------------ */

const INK: Rgb = Rgb::hex(0x15151a);
const INK2: Rgb = Rgb::hex(0x4a4a55);
const INK3: Rgb = Rgb::hex(0x8a8a96);
const LINE: Rgb = Rgb::hex(0xdedee4);
const BAND: Rgb = Rgb::hex(0xf4f4f6);
const EMBER: Rgb = Rgb::hex(0xff5a1f);
const UP: Rgb = Rgb::hex(0x0f8a5f);
const DOWN: Rgb = Rgb::hex(0xc8352b);
const M: f64 = 36.0; // page margin

pub fn fmt_money(d: Decimal) -> String {
    let neg = d < Decimal::ZERO;
    let s = format!("{:.2}", d.abs().round_dp(2));
    let (int, frac) = s.split_once('.').unwrap_or((&s, "00"));
    let mut g = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            g.push(' ');
        }
        g.push(c);
    }
    format!("{}{g}.{frac}", if neg { "-" } else { "" })
}

fn fnum(x: f64, dp: usize) -> String {
    format!("{x:.dp$}")
}

fn price(d: Decimal) -> String {
    let s = d.normalize().to_string();
    if s.contains('.') { s } else { format!("{s}.00") }
}

struct Writer {
    doc: Doc,
    y: f64,
    title: String,
    footer: String,
}

#[derive(Clone, Copy)]
enum Align {
    L,
    R,
}

struct Col {
    head: &'static str,
    w: f64,
    align: Align,
}

impl Writer {
    fn page(&mut self) -> &mut Page {
        self.doc.pages.last_mut().unwrap()
    }

    fn new_page(&mut self) {
        let title = self.title.clone();
        let p = self.doc.add_page();
        p.rect(0.0, 0.0, A4_W, 3.0, EMBER);
        logo::draw(p, M, 16.0, 16.0, EMBER, INK);
        p.text_right(A4_W - M, 29.0, &title, Font::Regular, 8.0, INK3);
        p.line(M, 42.0, A4_W - M, 42.0, 0.5, LINE);
        self.y = 58.0;
    }

    fn ensure(&mut self, h: f64) {
        if self.y + h > A4_H - 48.0 {
            self.new_page();
        }
    }

    fn section(&mut self, title: &str, sub: &str) {
        self.ensure(44.0);
        self.y += 10.0;
        let y = self.y;
        let p = self.page();
        p.rect(M, y - 9.0, 3.0, 12.0, EMBER);
        p.text(M + 9.0, y + 1.0, title, Font::Bold, 11.0, INK);
        if !sub.is_empty() {
            let w = text_width(title, Font::Bold, 11.0);
            p.text(M + 16.0 + w, y + 1.0, sub, Font::Regular, 8.0, INK3);
        }
        self.y += 12.0;
    }

    fn table(&mut self, cols: &[Col], rows: &[Vec<(String, Option<Rgb>)>], totals: Option<Vec<(String, Option<Rgb>)>>) {
        let width: f64 = cols.iter().map(|c| c.w).sum();
        let scale = (A4_W - 2.0 * M) / width;
        let head = |w: &mut Writer| {
            let y = w.y;
            let p = w.page();
            p.rect(M, y, A4_W - 2.0 * M, 15.0, INK);
            let mut x = M;
            for c in cols {
                let cw = c.w * scale;
                match c.align {
                    Align::L => p.text(x + 3.0, y + 10.5, c.head, Font::Bold, 6.8, Rgb::hex(0xffffff)),
                    Align::R => p.text_right(x + cw - 3.0, y + 10.5, c.head, Font::Bold, 6.8, Rgb::hex(0xffffff)),
                }
                x += cw;
            }
            w.y += 15.0;
        };
        self.ensure(30.0);
        head(self);
        if rows.is_empty() {
            let y = self.y;
            self.page().text(M + 3.0, y + 11.0, "No records in this period.", Font::Regular, 7.5, INK3);
            self.y += 16.0;
        }
        for (i, r) in rows.iter().enumerate() {
            if self.y + 13.0 > A4_H - 48.0 {
                self.new_page();
                head(self);
            }
            let y = self.y;
            let p = self.page();
            if i % 2 == 1 {
                p.rect(M, y, A4_W - 2.0 * M, 13.0, BAND);
            }
            let mut x = M;
            for (c, (txt, color)) in cols.iter().zip(r.iter()) {
                let cw = c.w * scale;
                let s = fit(txt, Font::Regular, 7.0, cw - 5.0);
                match c.align {
                    Align::L => p.text(x + 3.0, y + 9.2, &s, Font::Regular, 7.0, color.unwrap_or(INK2)),
                    Align::R => p.text_right(x + cw - 3.0, y + 9.2, &s, Font::Regular, 7.0, color.unwrap_or(INK2)),
                }
                x += cw;
            }
            self.y += 13.0;
        }
        if let Some(t) = totals {
            self.ensure(16.0);
            let y = self.y;
            let p = self.page();
            p.line(M, y, A4_W - M, y, 0.8, INK);
            let mut x = M;
            for (c, (txt, color)) in cols.iter().zip(t.iter()) {
                let cw = c.w * scale;
                match c.align {
                    Align::L => p.text(x + 3.0, y + 10.0, txt, Font::Bold, 7.2, color.unwrap_or(INK)),
                    Align::R => p.text_right(x + cw - 3.0, y + 10.0, txt, Font::Bold, 7.2, color.unwrap_or(INK)),
                }
                x += cw;
            }
            self.y += 15.0;
        }
        self.y += 6.0;
    }

    /// Two-column key/value grid (label left, value right-aligned in each half).
    fn kv_grid(&mut self, items: &[(&str, String, Option<Rgb>)]) {
        let half = (A4_W - 2.0 * M - 16.0) / 2.0;
        let rows = items.len().div_ceil(2);
        self.ensure(rows as f64 * 15.0 + 4.0);
        for (i, (k, v, c)) in items.iter().enumerate() {
            let col = (i % 2) as f64;
            let row = (i / 2) as f64;
            let x = M + col * (half + 16.0);
            let y = self.y + row * 15.0;
            let p = self.page();
            p.text(x, y + 10.0, k, Font::Regular, 8.0, INK3);
            p.text_right(x + half, y + 10.0, v, Font::Bold, 8.5, c.unwrap_or(INK));
            p.line(x, y + 14.0, x + half, y + 14.0, 0.4, LINE);
        }
        self.y += rows as f64 * 15.0 + 6.0;
    }
}

fn tone(d: Decimal) -> Option<Rgb> {
    if d > Decimal::ZERO {
        Some(UP)
    } else if d < Decimal::ZERO {
        Some(DOWN)
    } else {
        None
    }
}

fn f64dec(x: f64) -> Decimal {
    Decimal::from_f64_retain(x).unwrap_or_default().round_dp(2)
}

/// The Options section (O46): premiums paid / received, settlements with their fixing, commission and realised
/// P&L, then every option deal of the period.
fn options_pdf(w: &mut Writer, s: &Statement, cur: &str) {
    let o = &s.options;
    w.section("Options", &format!("Kalks FX Options, amounts in {cur}"));
    w.kv_grid(&[
        ("Premiums paid", fmt_money(o.premiums_paid), None),
        ("Premiums received", fmt_money(o.premiums_received), None),
        ("Settlements received", fmt_money(o.settlements_received), None),
        ("Settlements paid", fmt_money(o.settlements_paid), None),
        ("Commission", fmt_money(o.commission), None),
        ("Realised option P&L", fmt_money(o.realised_pnl), tone(o.realised_pnl)),
        ("Positions opened / closed", format!("{} / {}", o.opened, o.closed), None),
        ("Expired / knocked out", format!("{} / {}", o.expired, o.knocked_out), None),
        ("Net option result (P&L - commission)", fmt_money(o.net), tone(o.net)),
    ]);
    let cols = [
        Col { head: "Time", w: 60.0, align: Align::L },
        Col { head: "Deal", w: 38.0, align: Align::L },
        Col { head: "Series", w: 92.0, align: Align::L },
        Col { head: "Event", w: 52.0, align: Align::L },
        Col { head: "Contracts", w: 34.0, align: Align::R },
        Col { head: "Premium", w: 40.0, align: Align::R },
        Col { head: "Fixing", w: 40.0, align: Align::R },
        Col { head: "Cash", w: 48.0, align: Align::R },
        Col { head: "Commission", w: 42.0, align: Align::R },
        Col { head: "P&L", w: 44.0, align: Align::R },
    ];
    let rows: Vec<Vec<(String, Option<Rgb>)>> = o
        .deals
        .iter()
        .map(|d| {
            let cash = d.option_cash();
            vec![
                (time::fmt_server(d.time), None),
                (d.id.to_string(), None),
                (d.symbol.clone(), Some(INK)),
                (if d.reversed { format!("{} (reversed)", option_event(d)) } else { option_event(d) }, None),
                (fnum(d.volume.to_f64().unwrap_or(0.0), 2), None),
                (if d.is_settlement() { String::new() } else { price(d.price) }, None),
                (d.option_fixing().map(price).unwrap_or_default(), None),
                (fmt_money(cash), tone(cash)),
                (fmt_money(-d.option_commission()), None),
                (if d.is_exit() { fmt_money(d.profit) } else { String::new() }, if d.is_exit() { tone(d.profit) } else { None }),
            ]
        })
        .collect();
    let live: Vec<&DealRow> = o.deals.iter().filter(|d| !d.reversed).collect();
    let cash: Decimal = live.iter().map(|d| d.option_cash()).sum();
    let blank = || (String::new(), None);
    w.table(
        &cols,
        &rows,
        Some(vec![("Total".into(), None), blank(), blank(), blank(), blank(), blank(), blank(), (fmt_money(cash), tone(cash)), (fmt_money(-o.commission), None), (fmt_money(o.realised_pnl), tone(o.realised_pnl))]),
    );
}

pub fn statement_pdf(s: &Statement, company: &str, site: &str, support: &str, sections: &Sections) -> Vec<u8> {
    let cur = s.account.currency.clone();
    let period = format!("{} - {}", time::fmt_server(s.from), time::fmt_server(s.to - chrono::Duration::seconds(1)));
    let mut w = Writer {
        doc: Doc::new(&format!("{company} statement {} {}", s.account.login, period)),
        y: 0.0,
        title: format!("Account {} · {}", s.account.login, period),
        footer: format!("{company} · {site} · {support}"),
    };
    w.new_page();
    // title block
    {
        let y = w.y;
        let p = w.page();
        p.text(M, y + 18.0, "Account Statement", Font::Bold, 20.0, INK);
        p.text(M, y + 34.0, &format!("Period {period} (server time, GMT+2/+3)"), Font::Regular, 8.5, INK3);
        p.text_right(A4_W - M, y + 18.0, &format!("#{}", s.account.login), Font::Bold, 16.0, EMBER);
        p.text_right(A4_W - M, y + 34.0, &format!("{} · {}", if s.account.kind == "demo" { "Demo" } else { "Live" }, s.account.group_name), Font::Regular, 8.5, INK3);
        w.y += 50.0;
    }
    w.kv_grid(&[
        ("Name", s.client_name.clone(), None),
        ("Account", s.account.login.to_string(), None),
        ("Email", s.client_email.clone(), None),
        ("Currency", cur.clone(), None),
        ("Group", format!("{} ({})", s.account.group_name, s.account.mode), None),
        ("Leverage", format!("1:{}", s.account.leverage), None),
        ("Generated", format!("{} server time", time::fmt_server(s.generated_at)), None),
        ("Status", s.account.status.clone(), None),
    ]);

    // summary
    let m = &s.summary;
    w.section("Summary", &format!("amounts in {cur}"));
    w.kv_grid(&[
        ("Opening balance", fmt_money(m.opening_balance), None),
        ("Closing balance", fmt_money(m.closing_balance), None),
        ("Deposits", fmt_money(m.deposits), None),
        ("Withdrawals", fmt_money(m.withdrawals), None),
        ("Closed trade results (incl. swap)", fmt_money(m.trade_results), tone(m.trade_results)),
        ("Commission", fmt_money(m.commission), tone(m.commission)),
        ("Performance fees", fmt_money(m.performance_fees), tone(m.performance_fees)),
        ("Adjustments", fmt_money(m.adjustments), None),
        ("Net trading result (realised)", fmt_money(m.net_pnl), tone(m.net_pnl)),
        ("Credit and bonus", fmt_money(m.closing_credit), None),
        ("Floating P&L (now)", s.floating.map(fmt_money).unwrap_or_else(|| "-".into()), s.floating.and_then(tone)),
        ("Equity (now)", s.equity.map(fmt_money).unwrap_or_else(|| "-".into()), None),
        ("Margin (now)", s.margin.map(fmt_money).unwrap_or_else(|| "-".into()), None),
        ("Free margin (now)", s.free_margin.map(fmt_money).unwrap_or_else(|| "-".into()), None),
    ]);
    if has_options(s) {
        w.kv_grid(&[
            ("Option premiums (paid - / received +)", fmt_money(m.option_premiums), tone(m.option_premiums)),
            ("Option settlements", fmt_money(m.option_settlements), tone(m.option_settlements)),
        ]);
    }
    let st = &s.stats;
    let k = if s.account.cent { 100.0 } else { 1.0 };
    w.kv_grid(&[
        ("Closed trades", st.trades.to_string(), None),
        ("Win rate", format!("{:.1}%", st.win_rate), None),
        ("Gross profit", fmt_money(f64dec(st.gross_profit * k)), Some(UP)),
        ("Gross loss", fmt_money(f64dec(-st.gross_loss * k)), Some(DOWN)),
        ("Profit factor", st.profit_factor.map(|x| fnum(x, 2)).unwrap_or_else(|| if st.wins > 0 { "no losses".into() } else { "-".into() }), None),
        ("Expectancy per trade", fmt_money(f64dec(st.expectancy * k)), None),
        ("Largest profit trade", st.best.as_ref().map(|t| fmt_money(f64dec(t.net * k))).unwrap_or_else(|| "-".into()), None),
        ("Largest loss trade", st.worst.as_ref().map(|t| fmt_money(f64dec(t.net * k))).unwrap_or_else(|| "-".into()), None),
    ]);

    // closed trades
    w.section("Closed trades", &format!("{} positions", s.trades.len()));
    let cols = [
        Col { head: "Open time", w: 62.0, align: Align::L },
        Col { head: "Position", w: 40.0, align: Align::L },
        Col { head: "Type", w: 24.0, align: Align::L },
        Col { head: "Volume", w: 30.0, align: Align::R },
        Col { head: "Symbol", w: 40.0, align: Align::L },
        Col { head: "Price", w: 42.0, align: Align::R },
        Col { head: "Close time", w: 62.0, align: Align::L },
        Col { head: "Price", w: 42.0, align: Align::R },
        Col { head: "Commission", w: 38.0, align: Align::R },
        Col { head: "Swap", w: 32.0, align: Align::R },
        Col { head: "Profit", w: 40.0, align: Align::R },
    ];
    let rows: Vec<Vec<(String, Option<Rgb>)>> = s
        .trades
        .iter()
        .map(|d| {
            vec![
                (d.open_time.map(time::fmt_server).unwrap_or_default(), None),
                (d.position_ticket.to_string(), None),
                (d.position_side.clone(), None),
                (fnum(d.volume.to_f64().unwrap_or(0.0), 2), None),
                (d.symbol.clone(), Some(INK)),
                (d.open_price.map(price).unwrap_or_default(), None),
                (time::fmt_server(d.time), None),
                (price(d.price), None),
                (fmt_money(-d.commission), None),
                (fmt_money(d.swap), None),
                (fmt_money(d.profit), tone(d.profit)),
            ]
        })
        .collect();
    let tc: Decimal = s.trades.iter().map(|d| d.commission).sum();
    let ts: Decimal = s.trades.iter().map(|d| d.swap).sum();
    let tp: Decimal = s.trades.iter().map(|d| d.profit).sum();
    let tv: f64 = s.trades.iter().map(|d| d.volume.to_f64().unwrap_or(0.0)).sum();
    let blank = || (String::new(), None);
    w.table(
        &cols,
        &rows,
        Some(vec![("Total".into(), None), blank(), blank(), (fnum(tv, 2), None), blank(), blank(), blank(), blank(), (fmt_money(-tc), None), (fmt_money(ts), None), (fmt_money(tp), tone(tp))]),
    );

    if has_options(s) {
        options_pdf(&mut w, s, &cur);
    }

    if sections.open {
        w.section("Open positions", "at generation time");
        let cols = [
            Col { head: "Open time", w: 62.0, align: Align::L },
            Col { head: "Position", w: 42.0, align: Align::L },
            Col { head: "Type", w: 24.0, align: Align::L },
            Col { head: "Volume", w: 30.0, align: Align::R },
            Col { head: "Symbol", w: 42.0, align: Align::L },
            Col { head: "Price", w: 44.0, align: Align::R },
            Col { head: "S / L", w: 40.0, align: Align::R },
            Col { head: "T / P", w: 40.0, align: Align::R },
            Col { head: "Market", w: 44.0, align: Align::R },
            Col { head: "Swap", w: 32.0, align: Align::R },
            Col { head: "Profit", w: 40.0, align: Align::R },
        ];
        let opt = |v: &Value| if v.is_null() { String::new() } else { fnum(num(v), 5).trim_end_matches('0').trim_end_matches('.').to_string() };
        let px = |v: &Value| fnum(num(v), 5).trim_end_matches('0').trim_end_matches('.').to_string();
        let rows: Vec<Vec<(String, Option<Rgb>)>> = s
            .positions
            .iter()
            .map(|p| {
                let pr = f64dec(num(&p["profit"]));
                vec![
                    (p["openTime"].as_str().and_then(|x| DateTime::parse_from_rfc3339(x).ok()).map(|t| time::fmt_server(t.with_timezone(&Utc))).unwrap_or_default(), None),
                    (p["ticket"].to_string(), None),
                    (p["side"].as_str().unwrap_or("").to_string(), None),
                    (fnum(num(&p["volume"]), 2), None),
                    (p["symbol"].as_str().unwrap_or("").to_string(), Some(INK)),
                    (px(&p["openPrice"]), None),
                    (opt(&p["sl"]), None),
                    (opt(&p["tp"]), None),
                    (px(&p["currentPrice"]), None),
                    (fmt_money(f64dec(num(&p["swap"]))), None),
                    (fmt_money(pr), tone(pr)),
                ]
            })
            .collect();
        let fl: Decimal = s.positions.iter().map(|p| f64dec(num(&p["profit"]))).sum();
        w.table(&cols, &rows, if s.positions.is_empty() { None } else { Some(vec![("Floating".into(), None), blank(), blank(), blank(), blank(), blank(), blank(), blank(), blank(), blank(), (fmt_money(fl), tone(fl))]) });

        w.section("Pending orders", "at generation time");
        let cols = [
            Col { head: "Placed", w: 62.0, align: Align::L },
            Col { head: "Order", w: 44.0, align: Align::L },
            Col { head: "Type", w: 60.0, align: Align::L },
            Col { head: "Volume", w: 34.0, align: Align::R },
            Col { head: "Symbol", w: 44.0, align: Align::L },
            Col { head: "Price", w: 46.0, align: Align::R },
            Col { head: "S / L", w: 44.0, align: Align::R },
            Col { head: "T / P", w: 44.0, align: Align::R },
            Col { head: "Expiry", w: 62.0, align: Align::L },
        ];
        let rows: Vec<Vec<(String, Option<Rgb>)>> = s
            .orders
            .iter()
            .map(|o| {
                vec![
                    (o["placedAt"].as_str().and_then(|x| DateTime::parse_from_rfc3339(x).ok()).map(|t| time::fmt_server(t.with_timezone(&Utc))).unwrap_or_default(), None),
                    (o["ticket"].to_string(), None),
                    (order_type(o), None),
                    (fnum(num(&o["volume"]), 2), None),
                    (o["symbol"].as_str().unwrap_or("").to_string(), Some(INK)),
                    (opt(&o["price"]), None),
                    (opt(&o["sl"]), None),
                    (opt(&o["tp"]), None),
                    (o["expiryAt"].as_str().and_then(|x| DateTime::parse_from_rfc3339(x).ok()).map(|t| time::fmt_server(t.with_timezone(&Utc))).unwrap_or_else(|| o["expiry"].as_str().unwrap_or("GTC").to_string()), None),
                ]
            })
            .collect();
        w.table(&cols, &rows, None);
    }

    // ledger
    w.section("Balance operations", "ledger");
    let cols = [
        Col { head: "Time", w: 70.0, align: Align::L },
        Col { head: "Transaction", w: 46.0, align: Align::L },
        Col { head: "Type", w: 90.0, align: Align::L },
        Col { head: "Comment", w: 140.0, align: Align::L },
        Col { head: "Amount", w: 60.0, align: Align::R },
        Col { head: "Balance", w: 64.0, align: Align::R },
    ];
    let rows: Vec<Vec<(String, Option<Rgb>)>> = s
        .ledger
        .iter()
        .map(|l| {
            let comment = [l.note.clone().unwrap_or_default(), l.reference.clone().unwrap_or_default()].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
            vec![
                (time::fmt_server(l.at), None),
                (l.txn.to_string(), None),
                (if l.sub_ledger == "balance" { l.label.clone() } else { format!("{} ({})", l.label, l.sub_ledger) }, Some(INK)),
                (comment, None),
                (fmt_money(l.amount), tone(l.amount)),
                (fmt_money(l.balance), None),
            ]
        })
        .collect();
    w.table(
        &cols,
        &rows,
        Some(vec![("Opening".into(), None), (fmt_money(m.opening_balance), None), blank(), ("Closing balance".into(), None), (fmt_money(m.closing_balance - m.opening_balance), None), (fmt_money(m.closing_balance), None)]),
    );

    if sections.charges {
        let c = &s.charges;
        w.section("Charges", &format!("in {cur}"));
        w.kv_grid(&[
            ("Commission", fmt_money(c.commission), None),
            ("Swap paid", fmt_money(c.swap_paid), None),
            ("Performance fees", fmt_money(c.performance_fees), None),
            ("Swap earned", fmt_money(c.swap_earned), None),
            ("Total charges", fmt_money(c.total), Some(DOWN)),
            ("Spread cost (estimate, in prices)", fmt_money(c.spread_estimate), Some(INK3)),
        ]);
    }

    // reconciliation + disclaimer
    w.section("Totals", "");
    let r = &s.reconciliation;
    w.kv_grid(&[
        ("Opening balance + movements", fmt_money(m.opening_balance + r.summary_balance_change), None),
        ("Closing balance (ledger)", fmt_money(m.closing_balance), None),
        ("Trade results: ledger / deals", format!("{} / {}", fmt_money(r.ledger_trade_results), fmt_money(r.deal_trade_results)), None),
        ("Reconciled", if r.ok { "Yes".into() } else { "See notes".into() }, Some(if r.ok { UP } else { DOWN })),
    ]);
    for n in &r.notes {
        w.ensure(12.0);
        let y = w.y;
        w.page().text(M, y + 8.0, &fit(n, Font::Regular, 7.5, A4_W - 2.0 * M), Font::Regular, 7.5, DOWN);
        w.y += 12.0;
    }
    w.ensure(52.0);
    let y = w.y + 8.0;
    let lines = [
        "Times are server time (GMT+3 during US daylight saving time, GMT+2 otherwise). Open positions, pending orders, equity and margin",
        "are shown as at the time this statement was generated. The spread cost is an estimate for information; it is already included in",
        "the prices of your trades. Option premiums are paid / received in cash when a trade opens or closes; option results are realised",
        "when a position is closed, expires or knocks out. Trading leveraged products and options carries a high level of risk and may not",
        "be suitable for all investors.",
    ];
    for (i, l) in lines.iter().enumerate() {
        w.page().text(M, y + i as f64 * 10.0, l, Font::Regular, 6.8, INK3);
    }

    // footers with page numbers
    let n = w.doc.pages.len();
    let footer = w.footer.clone();
    for (i, p) in w.doc.pages.iter_mut().enumerate() {
        p.line(M, A4_H - 32.0, A4_W - M, A4_H - 32.0, 0.4, LINE);
        p.text(M, A4_H - 20.0, &footer, Font::Regular, 7.0, INK3);
        p.text_right(A4_W - M, A4_H - 20.0, &format!("Page {} of {n}", i + 1), Font::Regular, 7.0, INK3);
    }
    w.doc.render()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_escapes_and_blocks_formulas() {
        let mut t = Table::new("T", &["a", "b"]);
        t.rows.push(vec![Cell::text("=HYPERLINK(1)"), Cell::text("x,\"y\"")]);
        t.rows.push(vec![Cell::Num(-1.5, 2), Cell::Int(7)]);
        let s = String::from_utf8(csv(&[t])).unwrap();
        assert!(s.contains("'=HYPERLINK(1)"));
        assert!(s.contains("\"x,\"\"y\"\"\""));
        assert!(s.contains("-1.50,7"));
    }

    #[test]
    fn xlsx_is_a_zip() {
        let mut t = Table::new("Sheet/one", &["a"]);
        t.rows.push(vec![Cell::Num(1.25, 2)]);
        let b = xlsx(&[t]).unwrap();
        assert_eq!(&b[..2], b"PK");
    }

    /// A statement with a CFD round trip and an option bought, partly sold back and expired in the money.
    fn options_statement() -> Statement {
        use crate::statement::{AccountInfo, DealRow, Input, build};
        use chrono::TimeZone;
        use std::str::FromStr;
        let d = |x: &str| Decimal::from_str(x).unwrap();
        let t = |h: u32| Utc.with_ymd_and_hms(2026, 10, 9, h, 0, 0).unwrap();
        let account = AccountInfo {
            login: 10000001,
            user_id: 1,
            kind: "live".into(),
            group: "standard".into(),
            group_name: "Standard".into(),
            product: "cfd".into(),
            currency: "USD".into(),
            cent: false,
            leverage: 500,
            mode: "hedging".into(),
            name: "Test".into(),
            status: "active".into(),
            created_at: t(0),
        };
        let row = |id: i64, entry: &str, side: &str, reason: &str, vol: &str, price: &str, profit: &str, comm: &str, option: Option<Value>, h: u32| DealRow {
            id,
            position_ticket: if option.is_some() { 500 } else { 600 },
            symbol: if option.is_some() { "EURUSD-20261009-1.1650-C".into() } else { "EURUSD".into() },
            side: side.into(),
            position_side: "buy".into(),
            entry: entry.into(),
            volume: d(vol),
            price: d(price),
            profit: d(profit),
            swap: Decimal::ZERO,
            commission: d(comm),
            reason: reason.into(),
            time: t(h),
            open_price: Some(d("0.0052")),
            open_time: Some(t(1)),
            comment: String::new(),
            reversed: false,
            option,
        };
        let o = |cash: f64, charged: f64, fixing: Value| Some(serde_json::json!({"series": "EURUSD-20261009-1.1650-C", "underlying": "EURUSD", "right": "call", "strike": 1.165, "expiry": "2026-10-09", "cash": cash, "fixing": fixing, "commissionCharged": charged}));
        let l = |h, txn, kind: &str, amt: &str| (t(h), txn, "balance".to_string(), kind.to_string(), d(amt), None, None);
        let (summary, charges, reconciliation, ledger, trades, deals, options) = build(Input {
            account: account.clone(),
            from: t(0),
            to: t(23),
            opening_balance: d("1000"),
            opening_credit: Decimal::ZERO,
            ledger: vec![
                l(1, 1, "option_premium", "-104"),
                l(1, 2, "commission", "-0.50"),
                l(2, 3, "commission", "-7"),
                l(3, 4, "option_premium", "60"),
                l(3, 5, "commission", "-0.25"),
                l(4, 6, "trade_pnl", "20"),
                l(14, 7, "option_settlement", "62"),
            ],
            deals: vec![
                row(10, "in", "buy", "client", "2", "0.0052", "0", "0.50", o(-104.0, 0.5, Value::Null), 1),
                row(20, "in", "buy", "client", "1", "1.16", "0", "7", None, 2),
                row(11, "out", "sell", "client", "1", "0.0060", "8", "0.50", o(60.0, 0.25, Value::Null), 3),
                row(21, "out", "sell", "client", "1", "1.162", "20", "7", None, 4),
                row(12, "out", "sell", "expiry", "1", "0.0062", "10", "0.25", o(62.0, 0.0, serde_json::json!(1.1712)), 14),
            ],
            spread: Decimal::ZERO,
        });
        assert!(reconciliation.ok, "{:?}", reconciliation.notes);
        let tr = crate::statement::to_trades(1, &deals.iter().filter(|x| x.is_exit()).cloned().collect::<Vec<_>>(), false, |_| 0.0);
        Statement {
            account,
            client_name: "Test Client".into(),
            client_email: "t@example.com".into(),
            from: t(0),
            to: t(23),
            generated_at: t(23),
            summary,
            charges,
            reconciliation,
            stats: crate::metrics::trade_stats(&tr),
            trades,
            options,
            deals,
            ledger,
            positions: vec![],
            orders: vec![],
            equity: None,
            margin: None,
            floating: None,
            free_margin: None,
        }
    }

    #[test]
    fn statement_tables_and_pdf_have_an_options_section() {
        let s = options_statement();
        assert!(has_options(&s));
        let tables = statement_tables(&s, &Sections::default());
        let names: Vec<&str> = tables.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"Options") && names.contains(&"Options summary"), "{names:?}");
        let csv = String::from_utf8(csv(&tables)).unwrap();
        for needle in ["Option premiums (paid - / received +),-44.00", "Option settlements,62.00", "Premiums paid,104.00", "Premiums received,60.00", "Settlements received (expiry payouts and knock-out rebates),62.00", "Realised option P&L,18.00", "Option premium", "Option settlement", "Bought (open)", "Expired", "Call", "1.17120"] {
            assert!(csv.contains(needle), "missing {needle:?} in\n{csv}");
        }
        // the closed-trades table is CFD only; premiums are never deposits / withdrawals / adjustments
        let closed = tables.iter().find(|t| t.name == "Closed trades").unwrap();
        assert_eq!(closed.rows.len(), 1);
        assert!(csv.contains("Deposits,0.00") && csv.contains("Withdrawals,0.00") && csv.contains("Adjustments,0.00"));
        assert!(csv.contains("Net trading result (realised),30.25"), "{csv}"); // CFD 20 + options 18 − commission 7.75
        // the PDF: decompress the content streams and look for the section
        let pdf = statement_pdf(&s, "Kalks", "kalks.com", "support@kalks.com", &Sections::default());
        let mut text = String::new();
        let mut i = 0;
        while let Some(p) = pdf[i..].windows(7).position(|w| w == b"stream\n") {
            let start = i + p + 7;
            let end = start + pdf[start..].windows(9).position(|w| w == b"endstream").unwrap();
            let mut dec = flate2::read::ZlibDecoder::new(&pdf[start..end]);
            let mut out = Vec::new();
            if std::io::Read::read_to_end(&mut dec, &mut out).is_ok() {
                text.push_str(&String::from_utf8_lossy(&out));
            }
            i = end + 9; // past "endstream"
        }
        for needle in ["(Options) Tj", "(Premiums paid) Tj", "(Settlements received) Tj", "(Realised option P&L) Tj", "(Expired) Tj", "(Option premium) Tj", "(Option settlement) Tj", "(1.1712) Tj"] {
            assert!(text.contains(needle), "PDF is missing {needle}");
        }
        // a CFD-only statement has no options section
        let mut plain = s.clone();
        plain.options = Default::default();
        plain.summary.option_premiums = Decimal::ZERO;
        plain.summary.option_settlements = Decimal::ZERO;
        assert!(!has_options(&plain));
        assert!(!statement_tables(&plain, &Sections::default()).iter().any(|t| t.name.starts_with("Options")));
    }

    #[test]
    fn money_format() {
        use std::str::FromStr;
        assert_eq!(fmt_money(Decimal::from_str("-1234567.891").unwrap()), "-1 234 567.89");
        assert_eq!(fmt_money(Decimal::from_str("0").unwrap()), "0.00");
    }
}
