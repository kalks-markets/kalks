use std::env;
use std::fmt;

/// Runtime configuration (env vars; the repo-root `.env.local` and `.env.claude` are loaded in development).
#[derive(Clone)]
pub struct Config {
    pub bind: String,
    pub database_url: String,
    /// Shared secret callers send in `X-Kalks-Internal`. Empty = check disabled (development only).
    pub internal_token: String,
    pub production: bool,
    /// HMAC key that signs trade-card snapshots (defaults to the internal token).
    pub card_secret: String,

    // ---- media
    /// `local` (disk, served by Caddy under /circle/media/*) or `s3` (S3-compatible, e.g. Cloudflare R2).
    pub storage: String,
    pub media_dir: String,
    /// Public URL prefix of local media (`/circle/media` on the app + trade hosts).
    pub media_url: String,
    /// Private scratch space: upload parts, transcoder work directories.
    pub work_dir: String,
    pub s3_endpoint: String,
    pub s3_bucket: String,
    pub s3_region: String,
    pub s3_access_key: String,
    pub s3_secret_key: String,
    /// CDN base for S3 media (e.g. https://media.kalkstrade.com).
    pub s3_public_url: String,
    pub ffmpeg: String,
    pub ffprobe: String,
    pub max_photo_bytes: i64,
    pub max_video_bytes: i64,
    pub max_voice_bytes: i64,
    pub max_file_bytes: i64,
    pub chunk_bytes: i64,
    /// Video length limits (seconds): posts / stories, topic videos.
    pub max_video_secs: i64,
    pub max_topic_video_secs: i64,
    pub max_voice_secs: i64,

    // ---- AI
    pub anthropic_key: String,
    pub anthropic_url: String,
    pub ai_model: String,
    pub moderation_model: String,
    /// What happens to images / videos when no AI check is possible: `review` (staff queue) or `publish`.
    pub moderation_fallback: String,

    // ---- other services
    pub gateway_url: String,
    pub gateway_token: String,
    pub trading_url: String,
    pub trading_token: String,
    pub market_data_url: String,
    pub academy_url: String,
    pub academy_token: String,
    pub growth_url: String,
    pub growth_token: String,
    pub notify_url: String,
    pub notify_token: String,

    // ---- push (FCM HTTP v1)
    pub fcm_service_account: String,
    pub fcm_project_id: String,
    pub fcm_api_url: String,

    /// Countries (ISO alpha-2, lower case) that can't use Circle (same rule as trading). Staff can add more in
    /// the Back Office settings.
    pub restricted_countries: Vec<String>,
    /// Broker whose staff moderate the whole community (other brokers' staff see their own clients only).
    pub platform_tenant: String,

    pub workers: bool,
    pub log_json: bool,
}

fn var(key: &str, default: &str) -> String {
    env::var(key).ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| default.to_string())
}

fn url(key: &str, default: &str) -> String {
    var(key, default).trim_end_matches('/').to_string()
}

fn num(key: &str, default: i64) -> i64 {
    var(key, &default.to_string()).parse().unwrap_or(default)
}

fn redact(v: &str) -> &'static str {
    if v.is_empty() { "<empty>" } else { "<redacted>" }
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
            .field("internal_token", &redact(&self.internal_token))
            .field("production", &self.production)
            .field("storage", &self.storage)
            .field("media_dir", &self.media_dir)
            .field("media_url", &self.media_url)
            .field("s3_endpoint", &self.s3_endpoint)
            .field("s3_bucket", &self.s3_bucket)
            .field("s3_access_key", &redact(&self.s3_access_key))
            .field("s3_secret_key", &redact(&self.s3_secret_key))
            .field("s3_public_url", &self.s3_public_url)
            .field("ffmpeg", &self.ffmpeg)
            .field("anthropic_key", &redact(&self.anthropic_key))
            .field("ai_model", &self.ai_model)
            .field("moderation_model", &self.moderation_model)
            .field("moderation_fallback", &self.moderation_fallback)
            .field("gateway_url", &self.gateway_url)
            .field("trading_url", &self.trading_url)
            .field("market_data_url", &self.market_data_url)
            .field("academy_url", &self.academy_url)
            .field("growth_url", &self.growth_url)
            .field("notify_url", &self.notify_url)
            .field("fcm", &!self.fcm_service_account.is_empty())
            .field("restricted_countries", &self.restricted_countries)
            .field("workers", &self.workers)
            .finish()
    }
}

fn home() -> String {
    env::var("HOME").unwrap_or_else(|_| ".".into())
}

pub fn parse_countries(s: &str) -> Vec<String> {
    s.split([',', ' ', ';']).map(|c| c.trim().to_ascii_lowercase()).filter(|c| c.len() == 2 && c.chars().all(|x| x.is_ascii_alphabetic())).collect()
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let production = var("CIRCLE_ENV", "development") == "production";
        let internal_token = var("CIRCLE_INTERNAL_TOKEN", "");
        if production && internal_token.is_empty() {
            anyhow::bail!("CIRCLE_INTERNAL_TOKEN is required when CIRCLE_ENV=production");
        }
        let storage = var("CIRCLE_STORAGE", "local").to_lowercase();
        if storage == "s3" {
            for k in ["CIRCLE_S3_ENDPOINT", "CIRCLE_S3_BUCKET", "CIRCLE_S3_ACCESS_KEY", "CIRCLE_S3_SECRET_KEY", "CIRCLE_S3_PUBLIC_URL"] {
                if var(k, "").is_empty() {
                    anyhow::bail!("{k} is required when CIRCLE_STORAGE=s3");
                }
            }
        }
        let ai_model = var("CIRCLE_AI_MODEL", "claude-opus-5-5");
        let card_secret = var("CIRCLE_CARD_SECRET", &internal_token);
        if production && card_secret.is_empty() {
            anyhow::bail!("CIRCLE_CARD_SECRET (or CIRCLE_INTERNAL_TOKEN) is required in production");
        }
        let data = format!("{}/.kalks-data/circle", home());
        Ok(Self {
            bind: var("CIRCLE_BIND", "127.0.0.1:8105"),
            database_url: var("CIRCLE_DATABASE_URL", "postgres://postgres@127.0.0.1:5433/kalks_circle"),
            internal_token,
            production,
            card_secret: if card_secret.is_empty() { "dev-circle-card-secret".into() } else { card_secret },
            storage,
            media_dir: var("CIRCLE_MEDIA_DIR", &format!("{data}/media")),
            media_url: url("CIRCLE_MEDIA_URL", "/circle/media"),
            work_dir: var("CIRCLE_WORK_DIR", &format!("{data}/work")),
            s3_endpoint: url("CIRCLE_S3_ENDPOINT", ""),
            s3_bucket: var("CIRCLE_S3_BUCKET", ""),
            s3_region: var("CIRCLE_S3_REGION", "auto"),
            s3_access_key: var("CIRCLE_S3_ACCESS_KEY", ""),
            s3_secret_key: var("CIRCLE_S3_SECRET_KEY", ""),
            s3_public_url: url("CIRCLE_S3_PUBLIC_URL", ""),
            ffmpeg: var("CIRCLE_FFMPEG", "ffmpeg"),
            ffprobe: var("CIRCLE_FFPROBE", "ffprobe"),
            max_photo_bytes: num("CIRCLE_MAX_PHOTO_MB", 20).clamp(1, 50) * 1024 * 1024,
            max_video_bytes: num("CIRCLE_MAX_VIDEO_MB", 500).clamp(10, 4096) * 1024 * 1024,
            max_voice_bytes: num("CIRCLE_MAX_VOICE_MB", 10).clamp(1, 50) * 1024 * 1024,
            max_file_bytes: num("CIRCLE_MAX_FILE_MB", 25).clamp(1, 100) * 1024 * 1024,
            chunk_bytes: num("CIRCLE_CHUNK_MB", 8).clamp(1, 32) * 1024 * 1024,
            max_video_secs: num("CIRCLE_MAX_VIDEO_SECS", 60).clamp(5, 600),
            max_topic_video_secs: num("CIRCLE_MAX_TOPIC_VIDEO_SECS", 600).clamp(30, 3600),
            max_voice_secs: num("CIRCLE_MAX_VOICE_SECS", 300).clamp(10, 1800),
            anthropic_key: var("ANTHROPIC_API_KEY", ""),
            anthropic_url: url("ANTHROPIC_API_URL", "https://api.anthropic.com"),
            moderation_model: var("CIRCLE_MODERATION_MODEL", &ai_model),
            ai_model,
            moderation_fallback: if var("CIRCLE_MODERATION_FALLBACK", "review") == "publish" { "publish".into() } else { "review".into() },
            gateway_url: url("GATEWAY_URL", "http://127.0.0.1:8080"),
            gateway_token: var("GATEWAY_INTERNAL_TOKEN", ""),
            trading_url: url("TRADING_URL", "http://127.0.0.1:8090"),
            trading_token: var("TRADING_INTERNAL_TOKEN", ""),
            market_data_url: url("MARKET_DATA_URL", "http://127.0.0.1:8081"),
            academy_url: url("ACADEMY_URL", "http://127.0.0.1:8098"),
            academy_token: var("ACADEMY_INTERNAL_TOKEN", ""),
            growth_url: url("GROWTH_URL", "http://127.0.0.1:8101"),
            growth_token: var("GROWTH_INTERNAL_TOKEN", ""),
            notify_url: url("SUPPORT_URL", "http://127.0.0.1:8100"),
            notify_token: var("SUPPORT_INTERNAL_TOKEN", ""),
            fcm_service_account: var("FCM_SERVICE_ACCOUNT_FILE", ""),
            fcm_project_id: var("FCM_PROJECT_ID", ""),
            fcm_api_url: url("FCM_API_URL", "https://fcm.googleapis.com"),
            restricted_countries: parse_countries(&var("CIRCLE_RESTRICTED_COUNTRIES", "kp,ir,sy,cu")),
            platform_tenant: var("CIRCLE_PLATFORM_TENANT", "kalks").to_lowercase(),
            workers: var("CIRCLE_WORKERS", "true") != "false",
            log_json: var("CIRCLE_LOG_FORMAT", "json") == "json",
        })
    }

    /// Configuration for tests: no workers, no upstream services, no AI key, local storage under `dir`.
    pub fn for_tests(database_url: &str, dir: &str) -> Self {
        let dead = "http://127.0.0.1:9".to_string();
        Self {
            bind: String::new(),
            database_url: database_url.to_string(),
            internal_token: String::new(),
            production: false,
            card_secret: "test-card-secret".into(),
            storage: "local".into(),
            media_dir: format!("{dir}/media"),
            media_url: "/circle/media".into(),
            work_dir: format!("{dir}/work"),
            s3_endpoint: String::new(),
            s3_bucket: String::new(),
            s3_region: "auto".into(),
            s3_access_key: String::new(),
            s3_secret_key: String::new(),
            s3_public_url: String::new(),
            ffmpeg: "ffmpeg".into(),
            ffprobe: "ffprobe".into(),
            max_photo_bytes: 20 * 1024 * 1024,
            max_video_bytes: 100 * 1024 * 1024,
            max_voice_bytes: 10 * 1024 * 1024,
            max_file_bytes: 5 * 1024 * 1024,
            chunk_bytes: 1024 * 1024,
            max_video_secs: 60,
            max_topic_video_secs: 600,
            max_voice_secs: 300,
            anthropic_key: String::new(),
            anthropic_url: dead.clone(),
            ai_model: "claude-opus-5-5".into(),
            moderation_model: "claude-opus-5-5".into(),
            moderation_fallback: "publish".into(),
            gateway_url: dead.clone(),
            gateway_token: String::new(),
            trading_url: dead.clone(),
            trading_token: String::new(),
            market_data_url: dead.clone(),
            academy_url: dead.clone(),
            academy_token: String::new(),
            growth_url: dead.clone(),
            growth_token: String::new(),
            notify_url: dead.clone(),
            notify_token: String::new(),
            fcm_service_account: String::new(),
            fcm_project_id: String::new(),
            fcm_api_url: dead,
            restricted_countries: vec!["kp".into(), "ir".into()],
            platform_tenant: "kalks".into(),
            workers: false,
            log_json: false,
        }
    }
}
