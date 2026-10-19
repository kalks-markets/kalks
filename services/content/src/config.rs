use std::env;
use std::fmt;

/// Runtime configuration (env vars; the repo-root `.env.local` is loaded in development).
#[derive(Clone)]
pub struct Config {
    pub bind: String,
    pub database_url: String,
    /// Shared secret the BFFs send in `X-Kalks-Internal`. Empty = check disabled (dev only).
    pub internal_token: String,
    /// Signs preview tokens (drafts on the public read). Defaults to a key derived from the internal token.
    pub preview_secret: String,
    /// Public base URL of this service as browsers reach it (media URLs in content responses), e.g.
    /// https://api.kalkstrade.com. Default: http://127.0.0.1:8105 (the local bind).
    pub public_url: String,
    /// `disk` (default) or `r2` (Cloudflare R2 / any S3-compatible bucket; storage.rs).
    pub storage: String,
    /// Disk storage root (`CONTENT_STORAGE_DIR`, default ~/.kalks-data/content). Files here are public by design.
    pub storage_dir: String,
    pub r2: R2Config,
    pub max_upload_bytes: usize,
    pub dev_mode: bool,
    pub json_logs: bool,
}

/// R2 / S3 settings (`R2_*`); only read when `CONTENT_STORAGE=r2`.
#[derive(Clone, Default)]
pub struct R2Config {
    /// `https://<account id>.r2.cloudflarestorage.com` (or any S3 endpoint).
    pub endpoint: String,
    pub bucket: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    /// R2 uses "auto".
    pub region: String,
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
            .field("internal_token", &if self.internal_token.is_empty() { "<empty>" } else { "<redacted>" })
            .field("public_url", &self.public_url)
            .field("storage", &self.storage)
            .field("storage_dir", &self.storage_dir)
            .field("r2_bucket", &self.r2.bucket)
            .field("max_upload_bytes", &self.max_upload_bytes)
            .field("dev_mode", &self.dev_mode)
            .finish()
    }
}

fn var(key: &str, default: &str) -> String {
    env::var(key).ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| default.to_string())
}

fn home() -> String {
    env::var("HOME").unwrap_or_else(|_| ".".into())
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let dev_mode = var("CONTENT_ENV", "development") != "production";
        let internal_token = var("CONTENT_INTERNAL_TOKEN", "");
        if internal_token.is_empty() && !dev_mode {
            anyhow::bail!("CONTENT_INTERNAL_TOKEN is required in production");
        }
        // default: same server/credentials as the gateway, database kalks_content
        let database_url = match env::var("CONTENT_DATABASE_URL").ok().filter(|v| !v.trim().is_empty()) {
            Some(u) => u,
            None => {
                let g = var("GATEWAY_DATABASE_URL", "postgres://postgres@127.0.0.1:5433/kalks_core");
                match g.rfind('/') {
                    Some(i) if i > g.find("://").map(|x| x + 2).unwrap_or(0) => {
                        let (base, tail) = g.split_at(i);
                        let q = tail.find('?').map(|j| &tail[j..]).unwrap_or("");
                        format!("{base}/kalks_content{q}")
                    }
                    _ => "postgres://postgres@127.0.0.1:5433/kalks_content".into(),
                }
            }
        };
        let bind = var("CONTENT_BIND", "127.0.0.1:8105");
        let storage = var("CONTENT_STORAGE", "disk").to_ascii_lowercase();
        if storage != "disk" && storage != "r2" {
            anyhow::bail!("CONTENT_STORAGE must be disk or r2");
        }
        let r2 = R2Config {
            endpoint: var("R2_ENDPOINT", "").trim_end_matches('/').to_string(),
            bucket: var("R2_BUCKET", ""),
            access_key_id: var("R2_ACCESS_KEY_ID", ""),
            secret_access_key: var("R2_SECRET_ACCESS_KEY", ""),
            region: var("R2_REGION", "auto"),
        };
        if storage == "r2" && (r2.endpoint.is_empty() || r2.bucket.is_empty() || r2.access_key_id.is_empty() || r2.secret_access_key.is_empty()) {
            anyhow::bail!("CONTENT_STORAGE=r2 needs R2_ENDPOINT, R2_BUCKET, R2_ACCESS_KEY_ID and R2_SECRET_ACCESS_KEY");
        }
        let preview_secret = var("CONTENT_PREVIEW_SECRET", &format!("preview:{internal_token}"));
        Ok(Self {
            public_url: var("CONTENT_PUBLIC_URL", &format!("http://{bind}")).trim_end_matches('/').to_string(),
            bind,
            database_url,
            internal_token,
            preview_secret,
            storage,
            storage_dir: var("CONTENT_STORAGE_DIR", &format!("{}/.kalks-data/content", home())),
            r2,
            max_upload_bytes: var("CONTENT_MAX_UPLOAD_MB", "10").parse::<usize>().unwrap_or(10).clamp(1, 50) * 1024 * 1024,
            dev_mode,
            json_logs: var("CONTENT_LOG_FORMAT", if dev_mode { "text" } else { "json" }) == "json",
        })
    }

    /// Test configuration: disk storage under `storage_dir`, token "test-token".
    pub fn for_tests(database_url: &str, storage_dir: &str) -> Self {
        Self {
            bind: "127.0.0.1:0".into(),
            database_url: database_url.into(),
            internal_token: "test-token".into(),
            preview_secret: "test-preview-secret".into(),
            public_url: "https://api.test".into(),
            storage: "disk".into(),
            storage_dir: storage_dir.into(),
            r2: R2Config::default(),
            max_upload_bytes: 10 * 1024 * 1024,
            dev_mode: true,
            json_logs: false,
        }
    }
}
