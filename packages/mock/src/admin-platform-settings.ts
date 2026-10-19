/**
 * Back Office → Settings mock data (general, integrations, payments, countries,
 * webhooks, maintenance). All exports are prefixed `SET_`.
 * Mirrors docs/INTEGRATIONS.md — keep both in sync.
 */
import { PEOPLE } from "./people";
import { seeded } from "./rng";

/* ------------------------------------------------------------------ */
/* General & branding                                                  */
/* ------------------------------------------------------------------ */

export const SET_GENERAL = {
  brandName: "Kalks Markets",
  legalName: "Kalks Markets Ltd",
  regNumber: "HY00724-KM",
  regulator: "FSA Seychelles · SD118",
  address: "Suite 305, Griffith Corporate Centre, Kingstown, St. Vincent & the Grenadines",
  timezone: "GMT+3",
  defaultLanguage: "en",
  baseCurrency: "USD",
  primary: "#FF5A1F",
  accent: "#E9B949",
  supportEmail: "support@kalkstrade.com",
  complianceEmail: "compliance@kalkstrade.com",
  supportPhone: "+971 4 568 2210",
  whatsapp: "+971 50 118 4420",
  liveChatHours: "24/5 · Mon 00:00 – Sat 00:00 GMT+3",
  riskWarning:
    "CFDs are complex instruments and come with a high risk of losing money rapidly due to leverage. You should consider whether you understand how CFDs work and whether you can afford to take the high risk of losing your money.",
};

export interface SetDomain {
  host: string;
  purpose: string;
  dns: "verified" | "pending";
  ssl: "active" | "pending";
  expires: string;
}

export const SET_DOMAINS: SetDomain[] = [
  { host: "app.kalkstrade.com", purpose: "Client Area", dns: "verified", ssl: "active", expires: "2026-12-19" },
  { host: "trade.kalkstrade.com", purpose: "Web terminal", dns: "verified", ssl: "active", expires: "2026-12-19" },
  { host: "admin.kalkstrade.com", purpose: "Back Office", dns: "verified", ssl: "active", expires: "2026-12-19" },
  { host: "api.kalkstrade.com", purpose: "Public API & webhooks", dns: "verified", ssl: "active", expires: "2026-12-19" },
  { host: "status.kalkstrade.com", purpose: "Status page", dns: "pending", ssl: "pending", expires: "—" },
];

export const SET_TIMEZONES = [
  { value: "GMT+3", label: "GMT+3 · Server time (MT-style)" },
  { value: "GMT+2", label: "GMT+2 · Winter server time" },
  { value: "GMT+0", label: "GMT+0 · UTC" },
  { value: "GMT+4", label: "GMT+4 · Dubai" },
  { value: "GMT+5:30", label: "GMT+5:30 · Mumbai" },
  { value: "GMT+8", label: "GMT+8 · Singapore" },
];

export const SET_LANGUAGES = [
  { code: "en", label: "English", flag: "gb" },
  { code: "ar", label: "العربية", flag: "ae", rtl: true },
  { code: "hi", label: "हिन्दी", flag: "in" },
  { code: "ur", label: "اردو", flag: "pk", rtl: true },
  { code: "es", label: "Español", flag: "es" },
  { code: "pt", label: "Português", flag: "br" },
  { code: "tr", label: "Türkçe", flag: "tr" },
  { code: "vi", label: "Tiếng Việt", flag: "vn" },
  { code: "id", label: "Bahasa Indonesia", flag: "id" },
  { code: "zh", label: "中文", flag: "cn" },
];

/* ------------------------------------------------------------------ */
/* Integrations hub                                                    */
/* ------------------------------------------------------------------ */

export type SetIntegrationStatus = "connected" | "not_configured" | "error" | "disabled";
export type SetLaunchNeed = "required" | "recommended" | "optional" | "post-launch";

export interface SetField {
  key: string;
  label: string;
  type: "text" | "secret" | "url" | "number" | "select" | "toggle" | "list" | "readonly";
  value?: string;
  /** full value revealed by the eye toggle (secrets only) */
  reveal?: string;
  list?: string[];
  options?: string[];
  placeholder?: string;
  hint?: string;
  required?: boolean;
  suffix?: string;
  /** only shown when the `mode` select of the card equals this */
  when?: string;
  span?: 2;
}

export interface SetIntegration {
  id: string;
  name: string;
  provider: string;
  category: SetCategoryKey;
  icon: string; // Icon3D name, or "coin:trx"
  status: SetIntegrationStatus;
  need: SetLaunchNeed;
  description: string;
  docsUrl: string;
  whereToGet: string;
  lastChecked?: string;
  latencyMs?: number;
  error?: string;
  note?: string;
  modes?: { key: string; options: string[]; value: string; label: string };
  fields: SetField[];
  meta?: { label: string; value: string; tone?: "up" | "down" | "warn" | "gold" | "info" | "neutral" }[];
}

export type SetCategoryKey = "market" | "messaging" | "identity" | "payments" | "ai" | "infra" | "growth";

export const SET_CATEGORIES: { key: SetCategoryKey; label: string; blurb: string }[] = [
  { key: "market", label: "Market data & trading", blurb: "Prices, charts, calendar and liquidity" },
  { key: "messaging", label: "Messaging & OTP", blurb: "Transactional email and SMS/WhatsApp one-time codes" },
  { key: "identity", label: "Identity & compliance", blurb: "KYC and social sign-in" },
  { key: "payments", label: "Payments & blockchain", blurb: "USDT-TRC20 deposits and withdrawals" },
  { key: "ai", label: "AI", blurb: "Support bot, AI coach and knowledge base" },
  { key: "infra", label: "Infrastructure", blurb: "Storage and error monitoring" },
  { key: "growth", label: "Engagement & attribution", blurb: "Web push notifications and analytics pixels for UTM campaigns" },
];

export const SET_INTEGRATIONS: SetIntegration[] = [
  {
    id: "infoways",
    name: "Market data feed",
    provider: "Infoways",
    category: "market",
    icon: "satellite_antenna",
    status: "connected",
    need: "required",
    description: "Real-time quotes for forex, metals, indices, energies, crypto and stocks. Streams into the pricing engine and every terminal.",
    docsUrl: "https://docs.infoways.io",
    whereToGet: "Infoways client portal → API → Keys (commercial redistribution licence required).",
    lastChecked: "2026-09-24T15:42:00Z",
    latencyMs: 38,
    meta: [
      { label: "Symbols streaming", value: "28 / 28", tone: "up" },
      { label: "Ticks / sec", value: "1,842" },
    ],
    fields: [
      { key: "apiKey", label: "API key", type: "secret", value: "iw_live_••••••••••••••••4c9A", reveal: "iw_live_EXAMPLEEXAMPLEEXAMPL4c9A", required: true },
      { key: "ws", label: "WebSocket URL", type: "url", value: "wss://stream.infoways.io/v2/quotes", required: true },
      { key: "rest", label: "REST URL", type: "url", value: "https://api.infoways.io/v2", required: true },
      { key: "mapping", label: "Symbols mapping", type: "readonly", value: "28 symbols mapped · Global symbol master", hint: "Edit in Brokers → Global symbols" },
      { key: "backup", label: "Backup feed provider", type: "select", value: "Twelve Data", options: ["None", "Twelve Data", "Polygon.io", "Finnhub"] },
      { key: "backupKey", label: "Backup feed API key", type: "secret", value: "td_••••••••••••e21f", reveal: "td_EXAMPLEEXAMPLe21f" },
    ],
  },
  {
    id: "calendar",
    name: "Economic calendar & news",
    provider: "FXStreet",
    category: "market",
    icon: "calendar",
    status: "error",
    need: "recommended",
    description: "Economic events and headlines for the client dashboard, the terminal and News curation.",
    docsUrl: "https://www.fxstreet.com/services/economic-calendar-api",
    whereToGet: "Provider sales / dashboard → API credentials. RSS feeds are public URLs from each publisher.",
    lastChecked: "2026-09-24T15:40:00Z",
    error: "RSS feed returned HTTP 403 — feeds.reuters.com/reuters/businessNews",
    meta: [{ label: "Feeds healthy", value: "3 / 4", tone: "warn" }],
    fields: [
      { key: "provider", label: "Calendar provider", type: "select", value: "FXStreet", options: ["FXStreet", "Trading Economics", "Finnhub"], required: true },
      { key: "apiKey", label: "Provider API key", type: "secret", value: "fxs_••••••••••••81bd", reveal: "fxs_EXAMPLEEXAMP81bd", required: true },
      {
        key: "rss",
        label: "RSS feed URLs",
        type: "list",
        span: 2,
        list: ["https://www.fxstreet.com/rss/news", "https://www.investing.com/rss/news_1.rss", "https://feeds.reuters.com/reuters/businessNews", "https://cointelegraph.com/rss"],
      },
    ],
  },
  {
    id: "lp",
    name: "Liquidity provider",
    provider: "FIX 4.4 session",
    category: "market",
    icon: "bank",
    status: "disabled",
    need: "post-launch",
    description: "A-book routing to an external LP over FIX. Disabled — Kalks runs B-book at launch; routing rules live in Trading → Book & routing.",
    docsUrl: "https://www.fixtrading.org/standards/fix-4-4/",
    whereToGet: "From your LP / prime-of-prime onboarding pack (session sheet).",
    note: "B-book at launch",
    fields: [
      { key: "host", label: "FIX host", type: "text", placeholder: "fix.lp-provider.com" },
      { key: "port", label: "Port", type: "number", placeholder: "9880" },
      { key: "sender", label: "SenderCompID", type: "text", placeholder: "KALKS_UAT" },
      { key: "target", label: "TargetCompID", type: "text", placeholder: "LPPRIME" },
    ],
  },
  {
    id: "email",
    name: "Transactional email",
    provider: "Amazon SES",
    category: "messaging",
    icon: "receipt",
    status: "connected",
    need: "required",
    description: "Welcome, OTP, deposit and withdrawal emails. Templates live in Content → Email templates.",
    docsUrl: "https://docs.aws.amazon.com/ses/latest/dg/send-email-smtp.html",
    whereToGet: "AWS console → SES → verified identity (kalkstrade.com) → SMTP settings → create SMTP credentials. Or an API key from SendGrid / Postmark.",
    lastChecked: "2026-09-24T15:41:00Z",
    latencyMs: 212,
    meta: [
      { label: "SPF", value: "Pass", tone: "up" },
      { label: "DKIM", value: "Pass", tone: "up" },
      { label: "DMARC", value: "p=quarantine", tone: "up" },
    ],
    modes: { key: "mode", label: "Delivery", options: ["SMTP", "API"], value: "SMTP" },
    fields: [
      { key: "host", label: "SMTP host", type: "text", value: "email-smtp.eu-central-1.amazonaws.com", required: true, when: "SMTP" },
      { key: "port", label: "Port", type: "number", value: "587", required: true, when: "SMTP" },
      { key: "user", label: "Username", type: "text", value: "AKIAEXAMPLE•••••••", required: true, when: "SMTP" },
      { key: "pass", label: "Password", type: "secret", value: "••••••••••••••••••••", reveal: "EXAMPLE-smtp-password-EXAMPLE0", required: true, when: "SMTP" },
      { key: "tls", label: "STARTTLS", type: "toggle", value: "on", when: "SMTP" },
      { key: "provider", label: "API provider", type: "select", value: "Amazon SES", options: ["Amazon SES", "SendGrid", "Postmark"], when: "API" },
      { key: "apiKey", label: "API key", type: "secret", placeholder: "Paste provider API key", required: true, when: "API" },
      { key: "fromName", label: "From name", type: "text", value: "Kalks Markets", required: true },
      { key: "fromEmail", label: "From email", type: "text", value: "no-reply@kalkstrade.com", required: true },
    ],
  },
  {
    id: "sms",
    name: "SMS & WhatsApp OTP",
    provider: "Twilio",
    category: "messaging",
    icon: "speech_balloon",
    status: "connected",
    need: "required",
    description: "One-time codes for sign-up, withdrawals and 2FA fallback. WhatsApp first, SMS as fallback.",
    docsUrl: "https://www.twilio.com/docs/verify",
    whereToGet: "Twilio console → Account info (SID + auth token); Messaging → Senders for the sender ID; WhatsApp template approved in Meta Business Manager.",
    lastChecked: "2026-09-24T15:39:00Z",
    latencyMs: 184,
    meta: [
      { label: "Delivery 7d", value: "98.6%", tone: "up" },
      { label: "Balance", value: "$412.08" },
    ],
    fields: [
      { key: "provider", label: "Provider", type: "select", value: "Twilio", options: ["Twilio", "MSG91", "Gupshup"], required: true },
      { key: "sid", label: "Account SID", type: "text", value: "AC7f3e••••••••••••••••••••1b90", required: true },
      { key: "token", label: "Auth token", type: "secret", value: "••••••••••••••••••••••••••••c4d2", reveal: "EXAMPLEEXAMPLEEXAMPLEEXAMPLEc4d2", required: true },
      { key: "sender", label: "Sender ID", type: "text", value: "KALKS", required: true },
      { key: "wa", label: "WhatsApp template", type: "text", value: "kalks_otp_v2", hint: "Approved · en, ar, hi" },
      { key: "waNumber", label: "WhatsApp number", type: "text", value: "+971 50 118 4420" },
    ],
  },
  {
    id: "push",
    name: "Web push",
    provider: "VAPID",
    category: "growth",
    icon: "bell",
    status: "not_configured",
    need: "recommended",
    description: "Browser and PWA notifications for price alerts, margin calls and deposit confirmations.",
    docsUrl: "https://web.dev/articles/push-notifications-web-push-protocol",
    whereToGet: "Generate a key pair here (or `npx web-push generate-vapid-keys`). No external account needed.",
    fields: [
      { key: "public", label: "VAPID public key", type: "text", placeholder: "BEl6…", required: true, span: 2 },
      { key: "private", label: "VAPID private key", type: "secret", placeholder: "Keep secret", required: true },
      { key: "subject", label: "Subject (mailto)", type: "text", value: "mailto:ops@kalkstrade.com", required: true },
    ],
  },
  {
    id: "sumsub",
    name: "KYC verification",
    provider: "Sumsub",
    category: "identity",
    icon: "identification_card",
    status: "connected",
    need: "required",
    description: "ID document, liveness and proof-of-address checks. Results land in Clients → KYC queue.",
    docsUrl: "https://docs.sumsub.com/reference/authentication",
    whereToGet: "Sumsub dashboard → Dev space → App tokens (token + secret); Webhooks → create webhook to the URL below; Levels → level name.",
    lastChecked: "2026-09-24T15:38:00Z",
    latencyMs: 96,
    meta: [
      { label: "Auto-approved 30d", value: "82%", tone: "up" },
      { label: "Webhook", value: "Healthy", tone: "up" },
    ],
    fields: [
      { key: "token", label: "App token", type: "secret", value: "prd:••••••••••••••••••••Qw8Z", reveal: "prd:EXAMPLEEXAMPLEEXAMPLQw8Z", required: true },
      { key: "secret", label: "Secret key", type: "secret", value: "••••••••••••••••••••••••", reveal: "EXAMPLEEXAMPLEEXAMPLE000", required: true },
      { key: "whSecret", label: "Webhook secret", type: "secret", value: "••••••••••••••••", reveal: "whsec_EXAMPLE00000", required: true },
      { key: "level", label: "Level name", type: "text", value: "kalks-basic-kyc-level", required: true },
      { key: "webhook", label: "Webhook URL (paste into Sumsub)", type: "readonly", value: "https://api.kalkstrade.com/webhooks/sumsub", span: 2 },
    ],
  },
  {
    id: "google",
    name: "Google sign-in",
    provider: "Google OAuth 2.0",
    category: "identity",
    icon: "key",
    status: "connected",
    need: "optional",
    description: "One-tap sign-up and login with Google on app.kalkstrade.com.",
    docsUrl: "https://developers.google.com/identity/protocols/oauth2/web-server",
    whereToGet: "Google Cloud console → APIs & Services → Credentials → OAuth client ID (Web). Add the redirect URL below.",
    lastChecked: "2026-09-24T15:30:00Z",
    latencyMs: 71,
    meta: [{ label: "Sign-ups via Google 30d", value: "31%" }],
    fields: [
      { key: "clientId", label: "Client ID", type: "text", value: "418027733915-q8r1…apps.googleusercontent.com", required: true, span: 2 },
      { key: "secret", label: "Client secret", type: "secret", value: "GOCSPX-••••••••••••••••Xs2", reveal: "GOCSPX-EXAMPLEEXAMPLEEXAMPXs2", required: true },
      { key: "redirect", label: "Redirect URL", type: "readonly", value: "https://app.kalkstrade.com/auth/callback/google" },
    ],
  },
  {
    id: "tron",
    name: "USDT-TRC20 blockchain",
    provider: "TronGrid",
    category: "payments",
    icon: "coin:trx",
    status: "connected",
    need: "required",
    description: "Per-client deposit addresses (HD-derived), confirmation watcher, sweeps to the hot wallet and withdrawals.",
    docsUrl: "https://developers.tron.network/reference/select-network",
    whereToGet: "trongrid.io → dashboard → create API key. Hot wallet and xpub from your custody setup (hardware wallet / MPC). Cold wallet address from treasury.",
    lastChecked: "2026-09-24T15:42:00Z",
    latencyMs: 142,
    meta: [
      { label: "Hot wallet", value: "184,220.51 USDT", tone: "gold" },
      { label: "Energy", value: "OK · 1.2M", tone: "up" },
    ],
    fields: [
      { key: "apiKey", label: "TronGrid API key", type: "secret", value: "••••••••-••••-••••-••••-••••••d71e", reveal: "00000000-0000-0000-0000-000000e4d71e", required: true },
      { key: "node", label: "Full node URL", type: "url", value: "https://api.trongrid.io", required: true },
      { key: "hot", label: "Hot wallet address", type: "text", value: "TQ7xH2m9LkP4vR8sWc3nYb6JdE1fA59KfE", required: true, span: 2 },
      { key: "xpub", label: "HD xpub", type: "secret", value: "xpub6C••••••••••••••••••••••••••••Vq3r", reveal: "xpub6CEXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMPLEVq3r", required: true, span: 2 },
      { key: "conf", label: "Confirmations required", type: "number", value: "20", suffix: "blocks", required: true },
      { key: "sweep", label: "Sweep threshold", type: "number", value: "500", suffix: "USDT", required: true },
      { key: "cold", label: "Cold wallet address", type: "text", value: "TLa2f6VPqDgRE67v1736s7bJ8Ray5wYjU7", required: true, span: 2 },
    ],
  },
  {
    id: "claude",
    name: "AI assistant",
    provider: "Anthropic Claude",
    category: "ai",
    icon: "sparkles",
    status: "connected",
    need: "recommended",
    description: "Powers the support bot (answers first, hands over to agents), the AI coach and knowledge-base search.",
    docsUrl: "https://docs.claude.com/en/api/getting-started",
    whereToGet: "console.anthropic.com → Settings → API keys → Create key. Set a spend limit on the workspace too.",
    lastChecked: "2026-09-24T15:41:00Z",
    latencyMs: 640,
    meta: [
      { label: "Spend this month", value: "$1,284 / $3,000", tone: "gold" },
      { label: "Bot resolution", value: "61%", tone: "up" },
    ],
    fields: [
      { key: "apiKey", label: "Claude API key", type: "secret", value: "sk-ant-api03-••••••••••••••••7f2Q", reveal: "sk-ant-api03-EXAMPLE-EXAMPLE-EXAMPLE-EXA7f2Q", required: true, span: 2 },
      { key: "model", label: "Model", type: "text", value: "claude-sonnet-5", required: true, hint: "Default" },
      { key: "budget", label: "Monthly budget", type: "number", value: "3000", suffix: "USD", required: true },
    ],
  },
  {
    id: "s3",
    name: "Object storage",
    provider: "S3 / MinIO",
    category: "infra",
    icon: "package",
    status: "connected",
    need: "required",
    description: "KYC documents, statements, invoices, avatars and report exports. Server-side encrypted.",
    docsUrl: "https://min.io/docs/minio/linux/developers/minio-drivers.html",
    whereToGet: "AWS IAM (or MinIO console) → create a bucket + an access key limited to that bucket.",
    lastChecked: "2026-09-24T15:40:00Z",
    latencyMs: 24,
    meta: [
      { label: "Used", value: "412 GB" },
      { label: "Encryption", value: "SSE-S3", tone: "up" },
    ],
    fields: [
      { key: "endpoint", label: "Endpoint", type: "url", value: "https://s3.eu-central-1.amazonaws.com", required: true },
      { key: "bucket", label: "Bucket", type: "text", value: "kalks-prod-private", required: true },
      { key: "access", label: "Access key", type: "text", value: "AKIAEXAMPLE•••••••••", required: true },
      { key: "secret", label: "Secret key", type: "secret", value: "••••••••••••••••••••••••••••••••", reveal: "EXAMPLE-secret-access-key-EXAMPLE000", required: true },
    ],
  },
  {
    id: "sentry",
    name: "Error monitoring",
    provider: "Sentry",
    category: "infra",
    icon: "shield",
    status: "connected",
    need: "recommended",
    description: "Crash and performance monitoring for the Client Area, terminal, Back Office and API.",
    docsUrl: "https://docs.sentry.io/platforms/javascript/guides/nextjs/",
    whereToGet: "sentry.io → Project → Settings → Client Keys (DSN).",
    lastChecked: "2026-09-24T15:35:00Z",
    latencyMs: 58,
    meta: [{ label: "Unresolved issues", value: "14", tone: "warn" }],
    fields: [
      { key: "dsn", label: "Sentry DSN", type: "secret", value: "https://•••••••@o0.ingest.sentry.io/0", reveal: "https://EXAMPLE@o0.ingest.sentry.io/0", required: true, span: 2 },
      { key: "env", label: "Environment", type: "select", value: "production", options: ["production", "staging"] },
      { key: "rate", label: "Traces sample rate", type: "number", value: "0.2" },
    ],
  },
  {
    id: "analytics",
    name: "Web analytics & pixels",
    provider: "GA4 + Meta Pixel",
    category: "growth",
    icon: "bar_chart",
    status: "not_configured",
    need: "optional",
    description: "Sends sign-up, KYC and FTD conversions with UTM parameters for campaign attribution.",
    docsUrl: "https://developers.google.com/analytics/devguides/collection/ga4",
    whereToGet: "GA4 admin → Data streams → Measurement ID. Meta Events Manager → Pixel ID + Conversions API token.",
    fields: [
      { key: "ga4", label: "GA4 measurement ID", type: "text", placeholder: "G-XXXXXXXXXX" },
      { key: "pixel", label: "Meta Pixel ID", type: "text", placeholder: "1234567890" },
      { key: "capi", label: "Conversions API token", type: "secret", placeholder: "EAAG…", span: 2 },
    ],
  },
];

export const SET_GO_LIVE: { id: string; label: string; detail: string; done: boolean; href: string }[] = [
  { id: "dns", label: "Domains & SSL", detail: "app / trade / admin / api verified", done: true, href: "/settings" },
  { id: "feed", label: "Market data streaming", detail: "Infoways live · 28 symbols", done: true, href: "/settings/integrations" },
  { id: "email", label: "Email sender authenticated", detail: "SPF, DKIM, DMARC pass", done: true, href: "/settings/integrations" },
  { id: "otp", label: "OTP delivery tested", detail: "WhatsApp + SMS fallback", done: true, href: "/settings/integrations" },
  { id: "kyc", label: "KYC level & webhook", detail: "Sumsub basic level live", done: true, href: "/settings/integrations" },
  { id: "wallet", label: "Hot & cold wallets set", detail: "Hot wallet holds TRX for energy", done: true, href: "/settings/integrations" },
  { id: "payments", label: "Payment limits reviewed", detail: "USDT-TRC20 min/max & auto-approve", done: true, href: "/settings/payments" },
  { id: "legal", label: "Legal documents published", detail: "7 documents · v1.0", done: true, href: "/content/legal" },
  { id: "news", label: "Fix news feed error", detail: "1 RSS feed returning 403", done: false, href: "/settings/integrations" },
  { id: "push", label: "Web push keys", detail: "Generate VAPID key pair", done: false, href: "/settings/integrations" },
  { id: "status", label: "Status page domain", detail: "status.kalkstrade.com DNS pending", done: false, href: "/settings" },
];

/* ------------------------------------------------------------------ */
/* Payment methods                                                     */
/* ------------------------------------------------------------------ */

export interface SetPaymentMethod {
  id: string;
  name: string;
  network: string;
  icon: string; // coin name or Icon3D name prefixed "3d:"
  status: "active" | "soon";
  eta?: string;
  minDeposit: number;
  maxDeposit: number;
  minWithdrawal: number;
  maxWithdrawal: number;
  dailyLimit: number;
  fee: string;
  processing: string;
  volume30d: number;
  txCount30d: number;
}

export const SET_PAYMENT_METHODS: SetPaymentMethod[] = [
  { id: "usdt-trc20", name: "USDT", network: "TRON · TRC20", icon: "usdt", status: "active", minDeposit: 10, maxDeposit: 250000, minWithdrawal: 20, maxWithdrawal: 100000, dailyLimit: 250000, fee: "1 USDT withdrawal", processing: "~1 min · 20 confirmations", volume30d: 18_420_880, txCount30d: 12_904 },
  { id: "usdt-erc20", name: "USDT", network: "Ethereum · ERC20", icon: "eth", status: "soon", eta: "Q4 2026", minDeposit: 50, maxDeposit: 250000, minWithdrawal: 50, maxWithdrawal: 100000, dailyLimit: 250000, fee: "Network gas", processing: "~3 min", volume30d: 0, txCount30d: 0 },
  { id: "btc", name: "Bitcoin", network: "BTC mainnet", icon: "btc", status: "soon", eta: "Q4 2026", minDeposit: 50, maxDeposit: 500000, minWithdrawal: 100, maxWithdrawal: 250000, dailyLimit: 500000, fee: "Network fee", processing: "~30 min · 3 conf.", volume30d: 0, txCount30d: 0 },
  { id: "card", name: "Visa / Mastercard", network: "Card acquiring", icon: "3d:credit_card", status: "soon", eta: "Q1 2027", minDeposit: 20, maxDeposit: 10000, minWithdrawal: 20, maxWithdrawal: 10000, dailyLimit: 20000, fee: "2.5% deposit", processing: "Instant", volume30d: 0, txCount30d: 0 },
  { id: "wire", name: "Bank wire", network: "SWIFT / SEPA", icon: "3d:bank", status: "soon", eta: "Q1 2027", minDeposit: 500, maxDeposit: 1000000, minWithdrawal: 500, maxWithdrawal: 1000000, dailyLimit: 1000000, fee: "$25 withdrawal", processing: "1–3 business days", volume30d: 0, txCount30d: 0 },
  { id: "local", name: "Local payments", network: "UPI · PIX · FPX", icon: "3d:mobile_phone", status: "soon", eta: "2027", minDeposit: 10, maxDeposit: 5000, minWithdrawal: 10, maxWithdrawal: 5000, dailyLimit: 10000, fee: "1.5%", processing: "Instant", volume30d: 0, txCount30d: 0 },
];

export const SET_GROUP_LIMITS = [
  { group: "Standard", tier: "Verified", maxDeposit: 50000, maxWithdrawal: 25000, autoApprove: 500 },
  { group: "Pro", tier: "Verified", maxDeposit: 250000, maxWithdrawal: 100000, autoApprove: 1000 },
  { group: "Cent", tier: "Verified", maxDeposit: 5000, maxWithdrawal: 5000, autoApprove: 200 },
  { group: "ECN", tier: "Verified + EDD", maxDeposit: 250000, maxWithdrawal: 100000, autoApprove: 2500 },
  { group: "Prop", tier: "Verified", maxDeposit: 10000, maxWithdrawal: 50000, autoApprove: 0 },
  { group: "Unverified", tier: "Email only", maxDeposit: 2000, maxWithdrawal: 0, autoApprove: 0 },
];

/* ------------------------------------------------------------------ */
/* Countries                                                           */
/* ------------------------------------------------------------------ */

export interface SetCountryRule {
  code: string;
  name: string;
  level: "blocked" | "restricted";
  reason: string;
  signup: boolean; // blocked?
  login: boolean;
  deposits: boolean;
  attempts30d: number;
  addedBy: string;
  added: string;
}

export const SET_COUNTRY_RULES: SetCountryRule[] = [
  { code: "us", name: "United States", level: "blocked", reason: "No US licence (CFTC / NFA)", signup: true, login: true, deposits: true, attempts30d: 1842, addedBy: PEOPLE[12]!.name, added: "2025-11-02" },
  { code: "kp", name: "North Korea", level: "blocked", reason: "FATF call for action · OFAC", signup: true, login: true, deposits: true, attempts30d: 3, addedBy: PEOPLE[12]!.name, added: "2025-11-02" },
  { code: "ir", name: "Iran", level: "blocked", reason: "FATF call for action · OFAC", signup: true, login: true, deposits: true, attempts30d: 412, addedBy: PEOPLE[12]!.name, added: "2025-11-02" },
  { code: "mm", name: "Myanmar", level: "blocked", reason: "FATF call for action", signup: true, login: true, deposits: true, attempts30d: 27, addedBy: PEOPLE[12]!.name, added: "2025-11-02" },
  { code: "sy", name: "Syria", level: "blocked", reason: "OFAC / EU sanctions", signup: true, login: true, deposits: true, attempts30d: 58, addedBy: PEOPLE[12]!.name, added: "2025-11-02" },
  { code: "cu", name: "Cuba", level: "blocked", reason: "OFAC sanctions", signup: true, login: true, deposits: true, attempts30d: 11, addedBy: PEOPLE[12]!.name, added: "2025-11-02" },
  { code: "ca", name: "Canada", level: "blocked", reason: "Provincial securities rules", signup: true, login: false, deposits: true, attempts30d: 296, addedBy: PEOPLE[9]!.name, added: "2026-01-14" },
  { code: "be", name: "Belgium", level: "blocked", reason: "FSMA retail CFD ban", signup: true, login: false, deposits: true, attempts30d: 74, addedBy: PEOPLE[9]!.name, added: "2026-02-20" },
  { code: "ru", name: "Russia", level: "restricted", reason: "EDD required · sanctions screening", signup: false, login: false, deposits: false, attempts30d: 0, addedBy: PEOPLE[12]!.name, added: "2026-03-08" },
  { code: "ng", name: "Nigeria", level: "restricted", reason: "FATF grey list · EDD above $10k", signup: false, login: false, deposits: false, attempts30d: 0, addedBy: PEOPLE[12]!.name, added: "2026-03-08" },
  { code: "pk", name: "Pakistan", level: "restricted", reason: "EDD above $10k", signup: false, login: false, deposits: false, attempts30d: 0, addedBy: PEOPLE[12]!.name, added: "2026-05-19" },
];

export const SET_ALL_COUNTRIES: [string, string][] = [
  ["af", "Afghanistan"], ["by", "Belarus"], ["cn", "China"], ["fr", "France"], ["de", "Germany"], ["in", "India"], ["iq", "Iraq"], ["jp", "Japan"],
  ["lb", "Lebanon"], ["ly", "Libya"], ["ml", "Mali"], ["ni", "Nicaragua"], ["sd", "Sudan"], ["so", "Somalia"], ["ss", "South Sudan"], ["ve", "Venezuela"],
  ["ye", "Yemen"], ["zw", "Zimbabwe"], ["tr", "Türkiye"], ["ae", "United Arab Emirates"], ["gb", "United Kingdom"], ["es", "Spain"], ["it", "Italy"], ["au", "Australia"],
];

/* ------------------------------------------------------------------ */
/* Webhooks                                                            */
/* ------------------------------------------------------------------ */

export const SET_WEBHOOK_EVENTS: { group: string; events: string[] }[] = [
  { group: "Clients", events: ["client.created", "client.updated", "kyc.approved", "kyc.rejected"] },
  { group: "Money", events: ["deposit.completed", "withdrawal.requested", "withdrawal.approved", "withdrawal.rejected", "bonus.credited"] },
  { group: "Trading", events: ["account.created", "trade.opened", "trade.closed", "margin.call", "stop.out"] },
  { group: "Partners", events: ["ib.client.linked", "ib.commission.paid"] },
];

export interface SetWebhook {
  id: string;
  name: string;
  url: string;
  events: string[];
  status: "active" | "paused" | "failing";
  secret: string;
  successRate: number;
  deliveries24h: number;
  avgMs: number;
  created: string;
}

export const SET_WEBHOOKS: SetWebhook[] = [
  { id: "wh_01", name: "CRM sync (HubSpot)", url: "https://hooks.kalks-crm.io/hubspot/ingest", events: ["client.created", "client.updated", "kyc.approved", "deposit.completed"], status: "active", secret: "whsec_EXAMPLE000000001", successRate: 99.8, deliveries24h: 1842, avgMs: 184, created: "2026-02-11" },
  { id: "wh_02", name: "Data warehouse", url: "https://ingest.kalks-dwh.net/events", events: ["trade.opened", "trade.closed", "deposit.completed", "withdrawal.approved", "account.created"], status: "active", secret: "whsec_EXAMPLE000000002", successRate: 99.97, deliveries24h: 64210, avgMs: 62, created: "2026-01-04" },
  { id: "wh_03", name: "IB portal · Aurum partners", url: "https://partners.aurumfx.com/api/kalks-webhook", events: ["ib.client.linked", "ib.commission.paid"], status: "failing", secret: "whsec_EXAMPLE000000003", successRate: 71.4, deliveries24h: 318, avgMs: 2410, created: "2026-06-22" },
  { id: "wh_04", name: "Risk alerts → Slack", url: "https://hooks.slack.com/services/T04K…/B07Q…", events: ["margin.call", "stop.out", "withdrawal.requested"], status: "active", secret: "whsec_EXAMPLE000000004", successRate: 100, deliveries24h: 207, avgMs: 141, created: "2026-03-30" },
  { id: "wh_05", name: "Marketing automation (legacy)", url: "https://api.old-mailer.com/v1/kalks", events: ["client.created"], status: "paused", secret: "whsec_EXAMPLE000000005", successRate: 96.2, deliveries24h: 0, avgMs: 420, created: "2025-12-01" },
];

export interface SetDelivery {
  id: string;
  webhookId: string;
  event: string;
  code: number;
  ms: number;
  at: string;
  attempt: number;
}

export const SET_DELIVERIES: SetDelivery[] = (() => {
  const r = seeded(4411);
  const out: SetDelivery[] = [];
  const base = Date.parse("2026-09-24T15:44:00Z");
  for (let i = 0; i < 40; i++) {
    const wh = r.pick(SET_WEBHOOKS.filter((w) => w.status !== "paused"));
    const failing = wh.status === "failing" && r.bool(0.35);
    const code = failing ? r.pick([500, 502, 504, 408]) : r.bool(0.02) ? 429 : 200;
    out.push({
      id: `dlv_${(9_800_000 + i * 137).toString(36)}`,
      webhookId: wh.id,
      event: r.pick(wh.events),
      code,
      ms: failing ? r.int(1800, 10000) : Math.round(wh.avgMs * r.range(0.6, 1.6)),
      at: new Date(base - i * r.int(20, 160) * 1000).toISOString(),
      attempt: failing ? r.int(1, 5) : 1,
    });
  }
  return out;
})();

/* ------------------------------------------------------------------ */
/* Maintenance                                                         */
/* ------------------------------------------------------------------ */

export interface SetMaintenanceWindow {
  id: string;
  title: string;
  scope: string[];
  start: string; // ISO
  durationMin: number;
  recurring?: string;
  status: "scheduled" | "completed" | "running";
  notify: boolean;
  owner: string;
  ownerPhoto: string;
}

export const SET_MAINTENANCE_WINDOWS: SetMaintenanceWindow[] = [
  { id: "mw_1", title: "Weekly server maintenance", scope: ["Trading terminal", "API"], start: "2026-09-25T23:00:00Z", durationMin: 120, recurring: "Every Sat 02:00 GMT+3", status: "scheduled", notify: true, owner: PEOPLE[9]!.name, ownerPhoto: PEOPLE[9]!.photo },
  { id: "mw_2", title: "Postgres 17 minor upgrade", scope: ["Client Area", "Back Office", "API"], start: "2026-10-02T23:30:00Z", durationMin: 45, status: "scheduled", notify: true, owner: PEOPLE[19]!.name, ownerPhoto: PEOPLE[19]!.photo },
  { id: "mw_3", title: "TRON node resync", scope: ["Wallet deposits"], start: "2026-10-09T22:00:00Z", durationMin: 30, status: "scheduled", notify: false, owner: PEOPLE[21]!.name, ownerPhoto: PEOPLE[21]!.photo },
  { id: "mw_4", title: "Weekly server maintenance", scope: ["Trading terminal", "API"], start: "2026-09-18T23:00:00Z", durationMin: 120, recurring: "Every Sat 02:00 GMT+3", status: "completed", notify: true, owner: PEOPLE[9]!.name, ownerPhoto: PEOPLE[9]!.photo },
  { id: "mw_5", title: "Pricing engine v2.8 deploy", scope: ["Trading terminal"], start: "2026-09-12T23:15:00Z", durationMin: 25, status: "completed", notify: true, owner: PEOPLE[19]!.name, ownerPhoto: PEOPLE[19]!.photo },
];

export const SET_BYPASS_IPS = ["185.199.110.0/24", "94.206.41.18", "2a02:6b8::/32"];
