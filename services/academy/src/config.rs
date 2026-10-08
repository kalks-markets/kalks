use std::env;
use std::fmt;

/// Runtime configuration (env vars; the repo-root `.env.local` is loaded in development).
#[derive(Clone)]
pub struct Config {
    pub bind: String,
    pub database_url: String,
    /// Shared secret the BFFs send in `X-Kalks-Internal`. Empty = check disabled (dev only).
    pub internal_token: String,
    /// Root of the versioned course content (`content/academy`).
    pub content_dir: String,
    /// Public Client Area base URL printed on certificates (`<url>/certificate/<code>`).
    pub verify_base_url: String,
    pub dev_mode: bool,
    pub json_logs: bool,
    /// Read-only gateway database for module switches (`ACADEMY_GATEWAY_DATABASE_URL`, else `GATEWAY_DATABASE_URL`);
    /// empty = the Academy is always on.
    pub gateway_database_url: String,
}

fn redact_url(url: &str) -> String {
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

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("bind", &self.bind)
            .field("database_url", &redact_url(&self.database_url))
            .field("gateway_database_url", &redact_url(&self.gateway_database_url))
            .field("internal_token", &if self.internal_token.is_empty() { "<empty>" } else { "<redacted>" })
            .field("content_dir", &self.content_dir)
            .field("verify_base_url", &self.verify_base_url)
            .field("dev_mode", &self.dev_mode)
            .finish()
    }
}

fn var(key: &str, default: &str) -> String {
    env::var(key).ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| default.to_string())
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let dev_mode = var("ACADEMY_ENV", "development") != "production";
        let internal_token = var("ACADEMY_INTERNAL_TOKEN", "");
        if internal_token.is_empty() && !dev_mode {
            anyhow::bail!("ACADEMY_INTERNAL_TOKEN is required in production");
        }
        // default: same server/credentials as the gateway, database kalks_academy
        let database_url = match env::var("ACADEMY_DATABASE_URL").ok().filter(|v| !v.trim().is_empty()) {
            Some(u) => u,
            None => {
                let g = var("GATEWAY_DATABASE_URL", "postgres://postgres@127.0.0.1:5433/kalks_core");
                match g.rfind('/') {
                    Some(i) if i > g.find("://").map(|x| x + 2).unwrap_or(0) => {
                        let (base, tail) = g.split_at(i);
                        let q = tail.find('?').map(|j| &tail[j..]).unwrap_or("");
                        format!("{base}/kalks_academy{q}")
                    }
                    _ => "postgres://postgres@127.0.0.1:5433/kalks_academy".into(),
                }
            }
        };
        Ok(Self {
            bind: var("ACADEMY_BIND", "127.0.0.1:8098"),
            database_url,
            internal_token,
            content_dir: var("ACADEMY_CONTENT_DIR", concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/academy")),
            verify_base_url: var("ACADEMY_VERIFY_URL", "http://localhost:3000").trim_end_matches('/').to_string(),
            dev_mode,
            json_logs: var("ACADEMY_LOG_FORMAT", if dev_mode { "text" } else { "json" }) == "json",
            gateway_database_url: var("ACADEMY_GATEWAY_DATABASE_URL", &var("GATEWAY_DATABASE_URL", "")),
        })
    }
}
