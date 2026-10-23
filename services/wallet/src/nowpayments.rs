//! NOWPayments (nowpayments.io) API client and IPN signature check. Business rules: `ops::nowpayments`.
//!
//! API (https://documenter.getpostman.com/view/7907941/2s93JusNJt): base `https://api.nowpayments.io/v1` (sandbox
//! `https://api-sandbox.nowpayments.io/v1`), header `x-api-key`. Used here: `GET /status`, `GET /merchant/coins`,
//! `GET /min-amount`, `POST /payment`, `GET /payment/{id}`.
//!
//! IPN: NOWPayments POSTs the payment as JSON with `x-nowpayments-sig` = hex HMAC-SHA512, keyed by the IPN secret, of
//! the body re-serialised with its keys sorted. The documented reference is Node's
//! `JSON.stringify(sortObject(params))`, where `sortObject` recurses into every object **and array** (an array becomes
//! an object keyed "0", "1", …). `canonical` reproduces that byte for byte (JS number formatting, JS key order,
//! JSON.stringify string escaping); the PHP / Python examples of the docs (arrays kept, non-ASCII escaped) are accepted
//! too. The comparison is constant-time.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use ring::hmac;
use serde_json::{Value, json};
use subtle::ConstantTimeEq;

use crate::config::Config;
use crate::money::D;

pub const API: &str = "https://api.nowpayments.io/v1";
pub const SANDBOX_API: &str = "https://api-sandbox.nowpayments.io/v1";

/* ------------------------------------------------------------------ */
/* IPN signature                                                       */
/* ------------------------------------------------------------------ */

#[derive(Clone, Copy)]
struct Style {
    /// Node's sortObject: arrays become objects keyed by index; integer-like keys come first (JS property order).
    node: bool,
    /// Python `json.dumps` / PHP `json_encode` default: non-ASCII escaped as \uXXXX.
    ascii: bool,
}

/// JS `Number.prototype.toString()` of a double (what `JSON.stringify` writes).
pub fn js_number(x: f64) -> String {
    if x == 0.0 {
        return "0".into();
    }
    if !x.is_finite() {
        return "null".into();
    }
    let neg = x < 0.0;
    // Rust's `{:e}` is the shortest representation that round-trips, like JS: "1.2345e4"
    let e = format!("{:e}", x.abs());
    let (mant, exp) = e.split_once('e').unwrap_or((&e, "0"));
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n = exp.parse::<i32>().unwrap_or(0) + 1;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let e1 = n - 1;
        let sign = if e1 < 0 { '-' } else { '+' };
        if k == 1 { format!("{digits}e{sign}{}", e1.abs()) } else { format!("{}.{}e{sign}{}", &digits[..1], &digits[1..], e1.abs()) }
    };
    if neg { format!("-{body}") } else { body }
}

fn js_string(s: &str, ascii: bool, out: &mut String) {
    // serde_json escapes exactly what JSON.stringify escapes (", \, control characters; \b \f \n \r \t short forms)
    let quoted = serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into());
    if !ascii {
        out.push_str(&quoted);
        return;
    }
    for c in quoted.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            let mut buf = [0u16; 2];
            for u in c.encode_utf16(&mut buf) {
                out.push_str(&format!("\\u{u:04x}"));
            }
        }
    }
}

/// Canonical array index ("0", "17"; not "01"): JS orders these keys first, numerically.
fn array_index(k: &str) -> Option<u32> {
    if k.is_empty() || (k.len() > 1 && k.starts_with('0')) || !k.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    k.parse::<u32>().ok().filter(|n| *n < u32::MAX)
}

fn write(v: &Value, st: Style, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&js_number(n.as_f64().unwrap_or(0.0))),
        Value::String(s) => js_string(s, st.ascii, out),
        Value::Array(a) if st.node => {
            let entries: Vec<(String, &Value)> = a.iter().enumerate().map(|(i, x)| (i.to_string(), x)).collect();
            write_object(entries, st, out);
        }
        Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write(x, st, out);
            }
            out.push(']');
        }
        Value::Object(o) => write_object(o.iter().map(|(k, v)| (k.clone(), v)).collect(), st, out),
    }
}

fn write_object(mut entries: Vec<(String, &Value)>, st: Style, out: &mut String) {
    // Array.prototype.sort(): UTF-16 code unit order
    entries.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
    if st.node {
        // a JS object enumerates integer-like keys first (ascending), then the others in insertion (= sorted) order
        let (mut ints, rest): (Vec<_>, Vec<_>) = entries.into_iter().partition(|(k, _)| array_index(k).is_some());
        ints.sort_by_key(|(k, _)| array_index(k));
        entries = ints.into_iter().chain(rest).collect();
    }
    out.push('{');
    for (i, (k, v)) in entries.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        js_string(k, st.ascii, out);
        out.push(':');
        write(v, st, out);
    }
    out.push('}');
}

/// The documented string to sign: `JSON.stringify(sortObject(body))` (Node).
pub fn canonical(v: &Value) -> String {
    let mut out = String::new();
    write(v, Style { node: true, ascii: false }, &mut out);
    out
}

pub fn sign(secret: &str, message: &str) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA512, secret.trim().as_bytes());
    crate::chain::hex_encode(hmac::sign(&key, message.as_bytes()).as_ref())
}

/// Checks `x-nowpayments-sig` against the raw IPN body. False for a missing / malformed signature, a body that isn't
/// a JSON object, or an empty secret.
pub fn verify(secret: &str, raw: &[u8], signature: &str) -> bool {
    let secret = secret.trim();
    let sig = signature.trim().to_ascii_lowercase();
    if secret.is_empty() || sig.len() != 128 || !sig.bytes().all(|b| b.is_ascii_hexdigit()) {
        return false;
    }
    let Ok(v) = serde_json::from_slice::<Value>(raw) else { return false };
    if !v.is_object() {
        return false;
    }
    let mut ok = false;
    for st in [Style { node: true, ascii: false }, Style { node: false, ascii: false }, Style { node: false, ascii: true }] {
        let mut msg = String::new();
        write(&v, st, &mut msg);
        // every variant is computed and compared, in constant time
        ok |= bool::from(sign(secret, &msg).as_bytes().ct_eq(sig.as_bytes()));
    }
    ok
}

/* ------------------------------------------------------------------ */
/* Coins                                                               */
/* ------------------------------------------------------------------ */

/// The owner's suggested coins (all editable in the Back Office; the admin picks from the merchant's enabled coins).
pub const DEFAULT_COINS: &[&str] = &["usdttrc20", "usdtbsc", "usdterc20", "btc", "eth", "ltc", "trx", "bnbbsc"];

/// Display name, network and icon of a NOWPayments ticker (unknown tickers: the upper-case ticker).
pub fn coin_info(code: &str) -> Value {
    let (name, network, icon): (&str, &str, &str) = match code {
        "usdttrc20" => ("USDT", "TRON (TRC20)", "usdt"),
        "usdtbsc" => ("USDT", "BNB Smart Chain (BEP20)", "usdt"),
        "usdterc20" => ("USDT", "Ethereum (ERC20)", "usdt"),
        "usdtsol" => ("USDT", "Solana", "usdt"),
        "usdtmatic" => ("USDT", "Polygon", "usdt"),
        "usdtton" => ("USDT", "TON", "usdt"),
        "usdc" => ("USDC", "Ethereum (ERC20)", "usdc"),
        "usdcbsc" => ("USDC", "BNB Smart Chain (BEP20)", "usdc"),
        "usdcsol" => ("USDC", "Solana", "usdc"),
        "btc" => ("Bitcoin", "Bitcoin", "btc"),
        "eth" => ("Ethereum", "Ethereum", "eth"),
        "ltc" => ("Litecoin", "Litecoin", "ltc"),
        "trx" => ("TRON", "TRON", "trx"),
        "bnbbsc" => ("BNB", "BNB Smart Chain (BEP20)", "bnb"),
        "bnbmainnet" => ("BNB", "BNB Beacon Chain", "bnb"),
        "sol" => ("Solana", "Solana", "sol"),
        "doge" => ("Dogecoin", "Dogecoin", "doge"),
        "xrp" => ("XRP", "XRP Ledger", "xrp"),
        "ton" => ("Toncoin", "TON", "ton"),
        "bch" => ("Bitcoin Cash", "Bitcoin Cash", "bch"),
        "dash" => ("Dash", "Dash", "dash"),
        "xlm" => ("Stellar", "Stellar", "xlm"),
        "ada" => ("Cardano", "Cardano", "ada"),
        "matic" | "maticmainnet" => ("Polygon", "Polygon", "matic"),
        _ => ("", "", ""),
    };
    let ticker = match code {
        c if c.starts_with("usdt") => "USDT".to_string(),
        c if c.starts_with("usdc") => "USDC".to_string(),
        "bnbbsc" | "bnbmainnet" => "BNB".into(),
        "maticmainnet" => "MATIC".into(),
        c => c.to_uppercase(),
    };
    json!({
        "code": code,
        "ticker": ticker,
        "name": if name.is_empty() { ticker.clone() } else { name.to_string() },
        "network": network,
        "icon": if icon.is_empty() { code.to_string() } else { icon.to_string() },
        // the network's coin, shown as a small badge on tokens (USDT on TRON → trx)
        "chain_icon": match code {
            "usdttrc20" => Some("trx"),
            "usdtbsc" | "usdcbsc" => Some("bnb"),
            "usdterc20" | "usdc" => Some("eth"),
            "usdtsol" | "usdcsol" => Some("sol"),
            "usdtmatic" => Some("matic"),
            _ => None,
        },
        // these networks need the memo / destination tag NOWPayments returns as payin_extra_id
        "needs_extra_id": matches!(code, "xrp" | "xlm" | "bnbmainnet" | "eos" | "ton" | "usdtton" | "ava" | "atom" | "hbar" | "algo"),
    })
}

/// A coin ticker as NOWPayments writes it (lower-case letters and digits).
pub fn clean_coin(raw: &str) -> Option<String> {
    let c = raw.trim().to_ascii_lowercase();
    (!c.is_empty() && c.len() <= 24 && c.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())).then_some(c)
}

/* ------------------------------------------------------------------ */
/* HTTP client                                                         */
/* ------------------------------------------------------------------ */

static HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| reqwest::Client::builder().timeout(Duration::from_secs(20)).build().expect("http client"));

#[derive(Debug)]
pub enum NpError {
    /// NOWPayments answered with an error (status, its message).
    Api(u16, String),
    /// Timeout, connection or an unreadable answer.
    Network(String),
}

impl std::fmt::Display for NpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NpError::Api(s, m) => write!(f, "NOWPayments HTTP {s}: {m}"),
            NpError::Network(m) => write!(f, "NOWPayments unreachable: {m}"),
        }
    }
}

impl std::error::Error for NpError {}

pub struct Np {
    base: String,
    key: String,
}

impl Np {
    pub fn new(cfg: &Config, sandbox: bool, api_key: String) -> Self {
        let base = if !cfg.nowpayments_api_url.is_empty() {
            cfg.nowpayments_api_url.clone()
        } else if sandbox {
            SANDBOX_API.into()
        } else {
            API.into()
        };
        Self { base, key: api_key }
    }

    async fn call(&self, method: reqwest::Method, path: &str, body: Option<&Value>) -> Result<Value, NpError> {
        let mut rb = HTTP.request(method, format!("{}{path}", self.base)).header("x-api-key", &self.key).header("accept", "application/json");
        if let Some(b) = body {
            rb = rb.json(b);
        }
        // never log the URL with a query or the key: reqwest errors are stripped of the URL
        let res = rb.send().await.map_err(|e| NpError::Network(if e.is_timeout() { "timeout".into() } else { e.without_url().to_string() }))?;
        let status = res.status().as_u16();
        let text = res.text().await.map_err(|e| NpError::Network(e.without_url().to_string()))?;
        let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if !(200..300).contains(&status) {
            let msg = v.get("message").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| format!("HTTP {status}"));
            return Err(NpError::Api(status, msg.chars().take(300).collect()));
        }
        if v.is_null() {
            return Err(NpError::Network("unreadable answer".into()));
        }
        Ok(v)
    }

    /// `GET /status` → `{"message": "OK"}` (no key needed, but sent).
    pub async fn status(&self) -> Result<String, NpError> {
        let v = self.call(reqwest::Method::GET, "/status", None).await?;
        Ok(v.get("message").and_then(Value::as_str).unwrap_or("").to_string())
    }

    /// `GET /merchant/coins` → the coins enabled in the merchant's dashboard ("coins settings"), lower-case tickers.
    pub async fn merchant_coins(&self) -> Result<Vec<String>, NpError> {
        let v = self.call(reqwest::Method::GET, "/merchant/coins", None).await?;
        let list = v.get("selectedCurrencies").or_else(|| v.get("currencies")).and_then(Value::as_array).cloned().unwrap_or_default();
        let mut out: Vec<String> = list
            .iter()
            .filter_map(|c| c.as_str().map(str::to_string).or_else(|| c.get("code").and_then(Value::as_str).map(str::to_string)))
            .filter_map(|c| clean_coin(&c))
            .collect();
        out.sort();
        out.dedup();
        Ok(out)
    }

    /// `GET /min-amount?currency_from=<coin>&currency_to=<coin>&fiat_equivalent=usd&is_fee_paid_by_user=…` →
    /// (min amount in the coin, its USD equivalent).
    pub async fn min_amount(&self, coin: &str, fee_paid_by_user: bool) -> Result<(Option<D>, Option<D>), NpError> {
        let path = format!("/min-amount?currency_from={coin}&currency_to={coin}&fiat_equivalent=usd&is_fixed_rate={fee_paid_by_user}&is_fee_paid_by_user={fee_paid_by_user}");
        let v = self.call(reqwest::Method::GET, &path, None).await?;
        Ok((dec(v.get("min_amount")), dec(v.get("fiat_equivalent"))))
    }

    /// `POST /payment`.
    pub async fn create_payment(&self, body: &Value) -> Result<Value, NpError> {
        self.call(reqwest::Method::POST, "/payment", Some(body)).await
    }

    /// `GET /payment/{id}`: the authoritative state of a payment (re-fetched before anything is credited).
    pub async fn payment(&self, id: &str) -> Result<Value, NpError> {
        if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
            return Err(NpError::Api(400, "invalid payment id".into()));
        }
        self.call(reqwest::Method::GET, &format!("/payment/{id}"), None).await
    }
}

/// A decimal from a NOWPayments JSON value (a number or a numeric string; "null" and garbage = None).
pub fn dec(v: Option<&Value>) -> Option<D> {
    let text = match v? {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.trim().to_string(),
        _ => return None,
    };
    crate::money::parse(&text).or_else(|| D::from_scientific(&text).ok()).filter(|d| *d >= D::ZERO).map(|d| d.normalize())
}

/// A string or number field as text ("5745459419" / 5745459419 → "5745459419").
pub fn text(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) if !s.trim().is_empty() && s != "null" => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/* ------------------------------------------------------------------ */
/* Minimum amounts (cached 10 minutes)                                 */
/* ------------------------------------------------------------------ */

type MinKey = (i64, bool, String, bool);
static MINS: LazyLock<Mutex<HashMap<MinKey, (Instant, Option<D>)>>> = LazyLock::new(Default::default);
const MIN_TTL: Duration = Duration::from_secs(600);

/// The USD minimum of a coin for this merchant (None = unknown: NOWPayments unreachable or no answer).
pub async fn min_usd(np: &Np, tenant_id: i64, sandbox: bool, coin: &str, fee_paid_by_user: bool) -> Option<D> {
    let key = (tenant_id, sandbox, coin.to_string(), fee_paid_by_user);
    if let Some((at, v)) = MINS.lock().unwrap().get(&key).cloned()
        && at.elapsed() < MIN_TTL
    {
        return v;
    }
    let v = match np.min_amount(coin, fee_paid_by_user).await {
        Ok((_, usd)) => usd.map(|u| crate::money::ceil_dp(u, 2)),
        Err(e) => {
            tracing::warn!(coin, error = %e, "NOWPayments min-amount failed");
            return None;
        }
    };
    let mut m = MINS.lock().unwrap();
    if m.len() > 2000 {
        m.clear();
    }
    m.insert(key, (Instant::now(), v));
    v
}

/// Forgets cached minimums and merchant coins (settings changed).
pub fn forget_mins(tenant_id: i64) {
    MINS.lock().unwrap().retain(|k, _| k.0 != tenant_id);
    COINS.lock().unwrap().retain(|k, _| k.0 != tenant_id);
}

static COINS: LazyLock<Mutex<HashMap<(i64, bool), (Instant, Vec<String>)>>> = LazyLock::new(Default::default);

/// The coins enabled in the merchant account (cached 10 minutes); None when NOWPayments can't be asked.
pub async fn merchant_coins_cached(np: &Np, tenant_id: i64, sandbox: bool) -> Option<Vec<String>> {
    if let Some((at, v)) = COINS.lock().unwrap().get(&(tenant_id, sandbox)).cloned()
        && at.elapsed() < MIN_TTL
    {
        return Some(v);
    }
    match np.merchant_coins().await {
        Ok(v) => {
            COINS.lock().unwrap().insert((tenant_id, sandbox), (Instant::now(), v.clone()));
            Some(v)
        }
        Err(e) => {
            tracing::warn!(error = %e, "NOWPayments merchant coins failed");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "kalks-test-ipn-secret";

    // Vectors from NOWPayments' documented Node.js code: crypto.createHmac('sha512', secret).update(JSON.stringify(sortObject(body)))
    const DOC_BODY: &str = r#"{"payment_id":123456789,"parent_payment_id":987654321,"invoice_id":null,"payment_status":"finished","pay_address":"address","payin_extra_id":null,"price_amount":1,"price_currency":"usd","pay_amount":15,"actually_paid":15,"actually_paid_at_fiat":0,"pay_currency":"trx","order_id":null,"order_description":null,"purchase_id":"123456789","outcome_amount":14.8106,"outcome_currency":"trx","payment_extra_ids":null,"fee":{"currency":"btc","depositFee":0.09853637216235617,"withdrawalFee":0,"serviceFee":0}}"#;
    const DOC_CANONICAL: &str = r#"{"actually_paid":15,"actually_paid_at_fiat":0,"fee":{"currency":"btc","depositFee":0.09853637216235617,"serviceFee":0,"withdrawalFee":0},"invoice_id":null,"order_description":null,"order_id":null,"outcome_amount":14.8106,"outcome_currency":"trx","parent_payment_id":987654321,"pay_address":"address","pay_amount":15,"pay_currency":"trx","payin_extra_id":null,"payment_extra_ids":null,"payment_id":123456789,"payment_status":"finished","price_amount":1,"price_currency":"usd","purchase_id":"123456789"}"#;
    const DOC_SIG: &str = "d48851ca34b6890b89fb601948982824f4e5a8c339033c2a26b825bd294c34ade39f748ecb232e7b080bb1324a7a809df5bf4365b2ece11a502426c86dd3286f";

    const NESTED_BODY: &str = r#"{"updated_at":1729700000000,"payment_status":"partially_paid","payment_id":"5745459419","order_id":"np_0a1b2c3d4e5f60718293a4b5","price_amount":100.5,"pay_amount":0.0017070286,"actually_paid":1e-7,"big":1e21,"whole":15.0,"payment_extra_ids":[5513339153,42],"fee":{"serviceFee":0,"depositFee":1.5,"currency":"usdttrc20"},"order_description":"Dépôt Kalks 💰 a/b \"q\"\n"}"#;
    const NESTED_CANONICAL: &str = r#"{"actually_paid":1e-7,"big":1e+21,"fee":{"currency":"usdttrc20","depositFee":1.5,"serviceFee":0},"order_description":"Dépôt Kalks 💰 a/b \"q\"\n","order_id":"np_0a1b2c3d4e5f60718293a4b5","pay_amount":0.0017070286,"payment_extra_ids":{"0":5513339153,"1":42},"payment_id":"5745459419","payment_status":"partially_paid","price_amount":100.5,"updated_at":1729700000000,"whole":15}"#;
    const NESTED_SIG: &str = "00345ebb5869123931640852f55f3e947277042f1b77b5c66d6490ea40ab8cff6f9af6d71cd7e759d7987d86e3108b5f632119d8ab71e359002d1427d1f85138";

    #[test]
    fn canonical_matches_node() {
        assert_eq!(canonical(&serde_json::from_str(DOC_BODY).unwrap()), DOC_CANONICAL);
        assert_eq!(canonical(&serde_json::from_str(NESTED_BODY).unwrap()), NESTED_CANONICAL);
        assert_eq!(sign(SECRET, DOC_CANONICAL), DOC_SIG);
        assert_eq!(sign(SECRET, NESTED_CANONICAL), NESTED_SIG);
    }

    #[test]
    fn verifies_valid_signatures() {
        assert!(verify(SECRET, DOC_BODY.as_bytes(), DOC_SIG));
        assert!(verify(SECRET, NESTED_BODY.as_bytes(), NESTED_SIG));
        // upper-case hex and surrounding spaces in the header, whitespace in the body
        assert!(verify(SECRET, DOC_BODY.as_bytes(), &format!(" {} ", DOC_SIG.to_uppercase())));
        let pretty = serde_json::to_string_pretty(&serde_json::from_str::<Value>(DOC_BODY).unwrap()).unwrap();
        assert!(verify(SECRET, pretty.as_bytes(), DOC_SIG));
        // the secret as pasted with a trailing newline
        assert!(verify(&format!("{SECRET}\n"), DOC_BODY.as_bytes(), DOC_SIG));
    }

    #[test]
    fn rejects_tampering_and_wrong_secrets() {
        let tampered = DOC_BODY.replace(r#""actually_paid":15"#, r#""actually_paid":16"#);
        assert!(!verify(SECRET, tampered.as_bytes(), DOC_SIG));
        let status = DOC_BODY.replace("finished", "partially_paid");
        assert!(!verify(SECRET, status.as_bytes(), DOC_SIG));
        let nested = NESTED_BODY.replace(r#""depositFee":1.5"#, r#""depositFee":1.6"#);
        assert!(!verify(SECRET, nested.as_bytes(), NESTED_SIG));
        assert!(!verify("another-secret", DOC_BODY.as_bytes(), DOC_SIG));
        assert!(!verify("", DOC_BODY.as_bytes(), DOC_SIG));
        assert!(!verify(SECRET, DOC_BODY.as_bytes(), ""));
        assert!(!verify(SECRET, DOC_BODY.as_bytes(), &DOC_SIG[..127]));
        assert!(!verify(SECRET, DOC_BODY.as_bytes(), &"z".repeat(128)));
        assert!(!verify(SECRET, b"not json", DOC_SIG));
        assert!(!verify(SECRET, b"[1,2]", &sign(SECRET, "[1,2]")));
    }

    #[test]
    fn sorts_nested_keys_recursively() {
        let v: Value = serde_json::from_str(r#"{"b":{"z":1,"a":{"y":true,"c":null}},"a":[{"k":2,"j":1}],"10":1,"9":2,"B":3}"#).unwrap();
        // integer-like keys first (JS object order), then UTF-16 order ("B" < "a" < "b"); arrays become index objects
        assert_eq!(canonical(&v), r#"{"9":2,"10":1,"B":3,"a":{"0":{"j":1,"k":2}},"b":{"a":{"c":null,"y":true},"z":1}}"#);
        // the PHP / Python style (arrays kept, non-ASCII escaped) is accepted too
        let mut py = String::new();
        write(&serde_json::from_str(r#"{"b":"é","a":[{"k":2,"j":1}]}"#).unwrap(), Style { node: false, ascii: true }, &mut py);
        assert_eq!(py, "{\"a\":[{\"j\":1,\"k\":2}],\"b\":\"\\u00e9\"}");
        assert!(verify(SECRET, r#"{"b":"é","a":[{"k":2,"j":1}]}"#.as_bytes(), &sign(SECRET, &py)));
    }

    #[test]
    fn js_numbers() {
        for (x, s) in [(15.0, "15"), (0.1, "0.1"), (1e-7, "1e-7"), (1e21, "1e+21"), (123456789.0, "123456789"), (0.000001, "0.000001"), (-2.5, "-2.5"), (1.5e300, "1.5e+300"), (5513339153.0, "5513339153"), (0.09853637216235617, "0.09853637216235617")] {
            assert_eq!(js_number(x), s, "{x}");
        }
    }

    #[test]
    fn decimals_and_ids() {
        assert_eq!(dec(Some(&json!(0.17070286))), Some(D::from_str_exact("0.17070286").unwrap()));
        assert_eq!(dec(Some(&json!("50"))), Some(D::from(50)));
        assert_eq!(dec(Some(&json!(1e-7))), Some(D::from_str_exact("0.0000001").unwrap()));
        assert_eq!(dec(Some(&json!(null))), None);
        assert_eq!(dec(Some(&json!("null"))), None);
        assert_eq!(text(Some(&json!(5745459419u64))), Some("5745459419".into()));
        assert_eq!(text(Some(&json!("5745459419"))), Some("5745459419".into()));
        assert_eq!(clean_coin(" USDTTRC20 "), Some("usdttrc20".into()));
        assert_eq!(clean_coin("usdt-trc20"), None);
        assert_eq!(coin_info("usdtbsc")["ticker"], "USDT");
        assert_eq!(coin_info("xrp")["needs_extra_id"], true);
        assert_eq!(coin_info("newcoin")["name"], "NEWCOIN");
    }
}
