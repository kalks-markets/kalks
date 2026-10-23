use std::env;
use std::fmt;

/// Runtime configuration (env vars; the repo-root `.env.local` and `.env.tron` are loaded in development).
/// `Debug` is implemented by hand so secrets never reach logs.
#[derive(Clone)]
pub struct Config {
    pub bind: String,
    pub database_url: String,
    /// Shared secret the BFFs / internal services send in `X-Kalks-Internal`. Empty = check disabled (dev only).
    pub internal_token: String,
    pub dev_mode: bool,
    pub json_logs: bool,
    /// Run the watcher / scanner / recovery loops (exactly one instance in production).
    pub workers: bool,
    pub poll_secs: u64,
    pub scan_secs: u64,
    pub bsc_rpc_urls: Vec<String>,
    pub trongrid_url: String,
    pub trongrid_api_key: String,
    /// Seed values for chain_settings (first start only; afterwards the audited admin setting wins).
    pub bsc_address: String,
    pub tron_address: String,
    pub bsc_confirmations: i32,
    pub tron_confirmations: i32,
    pub trading_url: String,
    pub trading_token: String,
    pub gateway_url: String,
    pub gateway_token: String,
    /// Support / notifications service (`POST /v1/notify`). Empty = notifications stay in-app only.
    pub support_url: String,
    pub support_token: String,
    /// IB service (`POST /v1/ib/events/deposit`: real deposits for first deposit / CPA). Empty = not reported
    /// (development default; the events stay queued and are sent once it is set).
    pub ib_url: String,
    pub ib_token: String,
    /// `WALLET_SECRETS_KEY` (64 hex): encrypts provider secrets stored in the database (NOWPayments API key and IPN
    /// secret, src/secrets.rs). Required to save or use them in production; development falls back to a fixed key.
    pub secrets_key: String,
    /// `WALLET_NOWPAYMENTS_API_URL`: overrides the NOWPayments API base (tests and local mocks). Empty = the official
    /// API, or its sandbox when the Back Office "Sandbox" switch is on.
    pub nowpayments_api_url: String,
    /// `WALLET_NOWPAYMENTS_IPN_URL`: the default public IPN callback (the Client Area route that forwards to this
    /// service); the Back Office can override it per broker.
    pub nowpayments_ipn_url: String,
}

/// Masks the password in a connection URL (`postgres://user:secret@host` → `postgres://user:***@host`).
pub fn redact_url(url: &str) -> String {
    match (url.find("://"), url.rfind('@')) {
        (Some(s), Some(at)) if at > s + 3 => {
            let creds = &url[s + 3..at];
            match creds.find(':') {
                Some(c) => format!("{}{}:***{}", &url[..s + 3], &creds[..c], &url[at..]),
                None => url.to_string(),
            }
        }
        _ => url.to_string(),
    }
}

/// Hides API keys that some RPC providers put into the URL path or query.
fn redact_rpc(url: &str) -> String {
    let base = redact_url(url);
    match base.find('?') {
        Some(q) => format!("{}?***", &base[..q]),
        None => {
            // https://host/<long key> → https://host/***
            let parts: Vec<&str> = base.splitn(4, '/').collect();
            if parts.len() == 4 && parts[3].len() >= 20 { format!("{}//{}/***", parts[0], parts[2]) } else { base }
        }
    }
}

fn redact(v: &str) -> &'static str {
    if v.is_empty() { "<empty>" } else { "<redacted>" }
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("bind", &self.bind)
            .field("database_url", &redact_url(&self.database_url))
            .field("internal_token", &redact(&self.internal_token))
            .field("dev_mode", &self.dev_mode)
            .field("json_logs", &self.json_logs)
            .field("workers", &self.workers)
            .field("poll_secs", &self.poll_secs)
            .field("scan_secs", &self.scan_secs)
            .field("bsc_rpc_urls", &self.bsc_rpc_urls.iter().map(|u| redact_rpc(u)).collect::<Vec<_>>())
            .field("trongrid_url", &redact_rpc(&self.trongrid_url))
            .field("trongrid_api_key", &redact(&self.trongrid_api_key))
            .field("bsc_address", &self.bsc_address)
            .field("tron_address", &self.tron_address)
            .field("bsc_confirmations", &self.bsc_confirmations)
            .field("tron_confirmations", &self.tron_confirmations)
            .field("trading_url", &self.trading_url)
            .field("trading_token", &redact(&self.trading_token))
            .field("gateway_url", &self.gateway_url)
            .field("gateway_token", &redact(&self.gateway_token))
            .field("support_url", &self.support_url)
            .field("support_token", &redact(&self.support_token))
            .field("ib_url", &self.ib_url)
            .field("ib_token", &redact(&self.ib_token))
            .field("secrets_key", &redact(&self.secrets_key))
            .field("nowpayments_api_url", &redact_rpc(&self.nowpayments_api_url))
            .field("nowpayments_ipn_url", &self.nowpayments_ipn_url)
            .finish()
    }
}

fn var(key: &str, default: &str) -> String {
    env::var(key).ok().filter(|v| !v.trim().is_empty()).map(|v| v.trim().to_string()).unwrap_or_else(|| default.to_string())
}

/// The public IPN route of the Client Area (apps/crm/app/api/payments/nowpayments/ipn), which forwards to this service.
pub const DEFAULT_NOWPAYMENTS_IPN_URL: &str = "https://app.kalkstrade.com/api/payments/nowpayments/ipn";

pub const DEFAULT_BSC_RPCS: &str = "https://bsc-rpc.publicnode.com,https://bsc-dataseed.binance.org,https://bsc-dataseed1.defibit.io";

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let dev_mode = var("WALLET_ENV", "development") != "production";
        let internal_token = var("WALLET_INTERNAL_TOKEN", "");
        if internal_token.is_empty() && !dev_mode {
            anyhow::bail!("WALLET_INTERNAL_TOKEN is required in production");
        }
        Ok(Self {
            bind: var("WALLET_BIND", "127.0.0.1:8095"),
            database_url: var("WALLET_DATABASE_URL", "postgres://postgres@127.0.0.1:5433/kalks_wallet"),
            internal_token,
            dev_mode,
            json_logs: var("WALLET_LOG_FORMAT", "json") == "json",
            workers: var("WALLET_WORKERS", "true") != "false",
            poll_secs: var("WALLET_POLL_SECS", "10").parse().unwrap_or(10u64).clamp(2, 600),
            scan_secs: var("WALLET_SCAN_SECS", "60").parse().unwrap_or(60u64).clamp(10, 3600),
            bsc_rpc_urls: var("WALLET_BSC_RPC_URLS", DEFAULT_BSC_RPCS).split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
            trongrid_url: var("WALLET_TRONGRID_URL", "https://api.trongrid.io").trim_end_matches('/').to_string(),
            trongrid_api_key: var("TRONGRID_API_KEY", ""),
            bsc_address: var("WALLET_BSC_ADDRESS", ""),
            tron_address: var("WALLET_TRON_ADDRESS", ""),
            bsc_confirmations: var("WALLET_BSC_CONFIRMATIONS", "15").parse().unwrap_or(15).clamp(1, 500),
            tron_confirmations: var("WALLET_TRON_CONFIRMATIONS", "20").parse().unwrap_or(20).clamp(1, 500),
            trading_url: var("TRADING_URL", "http://127.0.0.1:8090").trim_end_matches('/').to_string(),
            trading_token: var("TRADING_INTERNAL_TOKEN", ""),
            gateway_url: var("GATEWAY_URL", "http://127.0.0.1:8080").trim_end_matches('/').to_string(),
            gateway_token: var("GATEWAY_INTERNAL_TOKEN", ""),
            support_url: var("SUPPORT_URL", "http://127.0.0.1:8100").trim_end_matches('/').to_string(),
            support_token: var("SUPPORT_INTERNAL_TOKEN", ""),
            ib_url: var("IB_URL", if dev_mode { "" } else { "http://127.0.0.1:8096" }).trim_end_matches('/').to_string(),
            ib_token: var("IB_INTERNAL_TOKEN", ""),
            secrets_key: var("WALLET_SECRETS_KEY", ""),
            nowpayments_api_url: var("WALLET_NOWPAYMENTS_API_URL", "").trim_end_matches('/').to_string(),
            nowpayments_ipn_url: var("WALLET_NOWPAYMENTS_IPN_URL", DEFAULT_NOWPAYMENTS_IPN_URL),
        })
    }

    /// A config for tests (no network, no secrets).
    pub fn for_tests(database_url: &str) -> Self {
        Self {
            bind: "127.0.0.1:0".into(),
            database_url: database_url.into(),
            internal_token: "test-internal-token".into(),
            dev_mode: true,
            json_logs: false,
            workers: false,
            poll_secs: 2,
            scan_secs: 10,
            bsc_rpc_urls: vec![],
            trongrid_url: String::new(),
            trongrid_api_key: String::new(),
            bsc_address: "0x11e9373d598703F83582e34378E086EbEEC5da11".into(),
            tron_address: "TU7PHUS22Hw632YsnAyjxNh4gu3u8PzcHZ".into(),
            bsc_confirmations: 15,
            tron_confirmations: 20,
            trading_url: String::new(),
            trading_token: String::new(),
            gateway_url: String::new(),
            gateway_token: String::new(),
            support_url: String::new(),
            support_token: String::new(),
            ib_url: String::new(),
            ib_token: String::new(),
            secrets_key: "5e".repeat(32),
            nowpayments_api_url: String::new(),
            nowpayments_ipn_url: DEFAULT_NOWPAYMENTS_IPN_URL.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_secrets() {
        assert_eq!(redact_url("postgres://kalks:s3cret@127.0.0.1:5432/kalks_wallet"), "postgres://kalks:***@127.0.0.1:5432/kalks_wallet");
        assert_eq!(redact_url("postgres://postgres@127.0.0.1:5433/kalks_wallet"), "postgres://postgres@127.0.0.1:5433/kalks_wallet");
        assert_eq!(redact_rpc("https://bsc.example.com/v1/?apikey=abc"), "https://bsc.example.com/v1/?***");
        assert_eq!(redact_rpc("https://rpc.example.com/0123456789abcdef0123456789"), "https://rpc.example.com/***");
        assert_eq!(redact_rpc("https://bsc-dataseed.binance.org"), "https://bsc-dataseed.binance.org");
        let mut c = Config::for_tests("postgres://u:pw@h/db");
        c.trongrid_api_key = "secret-key".into();
        c.ib_token = "ib-secret".into();
        c.secrets_key = "ab".repeat(32);
        let dbg = format!("{c:?}");
        assert!(!dbg.contains("pw@") && !dbg.contains("secret-key") && !dbg.contains("test-internal-token") && !dbg.contains("ib-secret") && !dbg.contains("abab"));
    }
}
