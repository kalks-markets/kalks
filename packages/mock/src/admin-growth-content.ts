/* Mock data — Back Office · Content (news, legal, emails, notifications, academy, translations). Prefix: CNT_ */
import { PEOPLE, person, type Person } from "./people";
import { seeded } from "./rng";

/* ------------------------------------------------------------------ */
/* Tenants                                                             */
/* ------------------------------------------------------------------ */

export const CNT_TENANTS = [
  { id: "kalks", name: "Kalks Markets", color: "#ff5a1f", domain: "kalkstrade.com", languages: 22 },
  { id: "aurum", name: "Aurum FX", color: "#e9b949", domain: "aurumfx.com", languages: 14 },
  { id: "nova", name: "NovaTrade Asia", color: "#38bdf8", domain: "novatrade.asia", languages: 11 },
  { id: "dunes", name: "Dunes Capital", color: "#22c55e", domain: "dunescapital.ae", languages: 6 },
] as const;
export type CntTenantId = (typeof CNT_TENANTS)[number]["id"];

/* ------------------------------------------------------------------ */
/* News curation                                                       */
/* ------------------------------------------------------------------ */

export interface CntNews {
  id: string;
  title: string;
  summary: string;
  source: string;
  image: string;
  symbols: string[];
  category: "Forex" | "Metals" | "Indices" | "Crypto" | "Stocks" | "Energies" | "Macro";
  time: string; // HH:mm GMT+3
  ago: string;
  pinned: boolean;
  hidden: boolean;
  tags: string[];
  impact: "high" | "medium" | "low";
  views: number;
  ctr: number;
  aiTagged: boolean;
}

export const CNT_NEWS: CntNews[] = [
  { id: "n1", title: "Gold holds near record as traders price deeper Fed cuts after soft PCE", summary: "XAUUSD trades at 2,654 after core PCE printed 0.1% m/m, below the 0.2% consensus; dollar index slips for a third session.", source: "Reuters", image: "/assets/photos/gold.jpg", symbols: ["XAUUSD", "XAGUSD"], category: "Metals", time: "14:21", ago: "6m", pinned: true, hidden: false, tags: ["fed", "inflation", "safe-haven"], impact: "high", views: 18420, ctr: 7.8, aiTagged: true },
  { id: "n2", title: "Nasdaq futures extend gains as chipmakers rally on AI capex guidance", summary: "NAS100 up 1.2% pre-market; NVDA +2.3% after hyperscalers raise 2027 data-centre budgets.", source: "Bloomberg", image: "/assets/photos/stock-market.jpg", symbols: ["NAS100", "NVDA"], category: "Indices", time: "14:02", ago: "25m", pinned: true, hidden: false, tags: ["tech", "earnings"], impact: "medium", views: 12108, ctr: 6.1, aiTagged: true },
  { id: "n3", title: "EUR/USD tests 1.0850 ahead of ECB's Lagarde testimony", summary: "Euro firms as German Ifo beats; options expiries cluster at 1.0850 and 1.0900.", source: "FXStreet", image: "/assets/photos/london.jpg", symbols: ["EURUSD", "GER40"], category: "Forex", time: "13:47", ago: "40m", pinned: false, hidden: false, tags: ["ecb", "euro"], impact: "medium", views: 9204, ctr: 5.4, aiTagged: true },
  { id: "n4", title: "Bitcoin climbs above $63,000 as ETF inflows hit two-week high", summary: "Spot ETFs saw $412M in net inflows on Monday; SOL outperforms with a 4.6% gain.", source: "CoinDesk", image: "/assets/photos/bitcoin.jpg", symbols: ["BTCUSD", "SOLUSD", "ETHUSD"], category: "Crypto", time: "13:30", ago: "57m", pinned: false, hidden: false, tags: ["etf", "flows"], impact: "medium", views: 14330, ctr: 8.2, aiTagged: true },
  { id: "n5", title: "Oil slides 1.4% as OPEC+ signals October output increase", summary: "WTI drops below $72 after delegates say the group will proceed with a 180k bpd hike.", source: "Dow Jones Newswires", image: "/assets/photos/dubai.jpg", symbols: ["USOIL", "UKOIL"], category: "Energies", time: "12:58", ago: "1h", pinned: false, hidden: false, tags: ["opec", "supply"], impact: "high", views: 7012, ctr: 4.9, aiTagged: false },
  { id: "n6", title: "Yen weakens past 149 as BoJ minutes show no rush to hike", summary: "USDJPY up 0.4%; traders scale back bets on a December move.", source: "Reuters", image: "/assets/photos/skyline.jpg", symbols: ["USDJPY", "JP225"], category: "Forex", time: "12:31", ago: "1h", pinned: false, hidden: false, tags: ["boj", "yen"], impact: "medium", views: 5840, ctr: 4.2, aiTagged: true },
  { id: "n7", title: "Tesla drops 3% after delivery estimates are cut by two brokers", summary: "Analysts lower Q3 deliveries forecast to 452k on weaker China demand.", source: "Bloomberg", image: "/assets/photos/nyc.jpg", symbols: ["TSLA"], category: "Stocks", time: "11:55", ago: "2h", pinned: false, hidden: false, tags: ["ev", "earnings"], impact: "low", views: 4112, ctr: 3.8, aiTagged: true },
  { id: "n8", title: "Guaranteed 300% returns: new XRP signal group goes viral", summary: "Promotional piece from an unverified Telegram channel.", source: "CryptoWire PR", image: "/assets/photos/crypto-coins.jpg", symbols: ["XRPUSD"], category: "Crypto", time: "11:40", ago: "2h", pinned: false, hidden: true, tags: ["promo", "unverified"], impact: "low", views: 0, ctr: 0, aiTagged: true },
  { id: "n9", title: "Kalks Research: three levels to watch on XAUUSD this week", summary: "Resistance at 2,670, support at 2,628 and 2,604; bias remains bullish above the 20-day MA.", source: "Kalks Research", image: "/assets/photos/charts.jpg", symbols: ["XAUUSD"], category: "Metals", time: "10:00", ago: "4h", pinned: true, hidden: false, tags: ["analysis", "levels"], impact: "low", views: 22804, ctr: 11.4, aiTagged: false },
  { id: "n10", title: "Singapore dollar steady as MAS keeps policy band unchanged", summary: "Central bank maintains slope and width, citing easing core inflation.", source: "Trading Central", image: "/assets/photos/singapore.jpg", symbols: ["USDJPY", "AUDUSD"], category: "Macro", time: "09:18", ago: "5h", pinned: false, hidden: false, tags: ["mas", "asia"], impact: "low", views: 2104, ctr: 2.9, aiTagged: true },
  { id: "n11", title: "FTSE 100 slips as miners lag on weaker iron ore prices", summary: "UK100 down 0.1%; Rio Tinto and Anglo American lead losses.", source: "FXStreet", image: "/assets/photos/finance.jpg", symbols: ["UK100"], category: "Indices", time: "08:44", ago: "6h", pinned: false, hidden: false, tags: ["uk", "miners"], impact: "low", views: 1880, ctr: 2.4, aiTagged: true },
  { id: "n12", title: "Ethereum developers set date for next network upgrade", summary: "Upgrade targets lower L2 fees; ETHUSD up 1.9% on the day.", source: "CoinDesk", image: "/assets/photos/crypto.jpg", symbols: ["ETHUSD"], category: "Crypto", time: "08:10", ago: "6h", pinned: false, hidden: false, tags: ["upgrade"], impact: "low", views: 3302, ctr: 3.6, aiTagged: true },
];

export const CNT_NEWS_SOURCES = [
  { id: "s1", name: "Reuters", type: "API", enabled: true, latency: "0.8s", today: 214, autoPublish: true, languages: 12 },
  { id: "s2", name: "Bloomberg", type: "API", enabled: true, latency: "1.1s", today: 168, autoPublish: true, languages: 4 },
  { id: "s3", name: "FXStreet", type: "RSS", enabled: true, latency: "42s", today: 96, autoPublish: true, languages: 18 },
  { id: "s4", name: "Dow Jones Newswires", type: "API", enabled: true, latency: "1.4s", today: 121, autoPublish: false, languages: 3 },
  { id: "s5", name: "CoinDesk", type: "RSS", enabled: true, latency: "58s", today: 64, autoPublish: true, languages: 5 },
  { id: "s6", name: "Trading Central", type: "API", enabled: true, latency: "2.0s", today: 48, autoPublish: true, languages: 22 },
  { id: "s7", name: "Kalks Research", type: "Internal", enabled: true, latency: "—", today: 3, autoPublish: false, languages: 22 },
  { id: "s8", name: "CryptoWire PR", type: "RSS", enabled: false, latency: "3m", today: 0, autoPublish: false, languages: 1 },
];

/* ------------------------------------------------------------------ */
/* Legal documents                                                     */
/* ------------------------------------------------------------------ */

export interface CntLegalVersion {
  version: string;
  publishedAt: string;
  publishedBy: Person;
  summary: string;
  reaccept: boolean;
  acceptance: number;
}

export interface CntLegalDoc {
  id: string;
  tenant: CntTenantId;
  name: string;
  slug: string;
  version: string;
  lastPublished: string;
  acceptance: number; // %
  pending: number; // clients to re-accept
  required: "signup" | "product" | "deposit";
  languages: number;
  history: CntLegalVersion[];
}

const LEGAL_BASE: { name: string; slug: string; required: CntLegalDoc["required"]; major: number; minor: number }[] = [
  { name: "Client Agreement", slug: "client-agreement", required: "signup", major: 3, minor: 2 },
  { name: "Risk Disclosure", slug: "risk-disclosure", required: "signup", major: 2, minor: 4 },
  { name: "Privacy Policy", slug: "privacy-policy", required: "signup", major: 4, minor: 1 },
  { name: "AML Policy", slug: "aml-policy", required: "signup", major: 2, minor: 0 },
  { name: "Cookies Policy", slug: "cookies-policy", required: "signup", major: 1, minor: 3 },
  { name: "Bonus T&C", slug: "bonus-terms", required: "product", major: 1, minor: 6 },
  { name: "Prop T&C", slug: "prop-terms", required: "product", major: 2, minor: 1 },
];

const LEGAL_SUMMARIES = [
  "Updated leverage caps for EU-referred clients and clarified stop-out levels.",
  "Added USDT TRC20 as a withdrawal method; revised processing times.",
  "New section on AI-assisted support and data retention for chat transcripts.",
  "Clarified negative balance protection scope for Cent accounts.",
  "Aligned with updated FATF travel rule guidance.",
  "Editorial fixes and translated into Bengali and Swahili.",
  "Revised prop daily-loss definition to equity-based midnight snapshot.",
];

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep"];

export const CNT_LEGAL_DOCS: CntLegalDoc[] = CNT_TENANTS.flatMap((t, ti) => {
  const r = seeded(900 + ti * 31);
  return LEGAL_BASE.map((b, bi) => {
    const minor = Math.max(0, b.minor - (ti === 0 ? 0 : r.int(0, 2)));
    const n = r.int(3, 6);
    const history: CntLegalVersion[] = Array.from({ length: n }, (_, k) => {
      const mi = minor - k;
      const ver = mi >= 0 ? `v${b.major}.${mi}` : `v${b.major - 1}.${9 + mi}`;
      const m = Math.max(0, 8 - k * r.int(1, 2));
      return {
        version: ver,
        publishedAt: `${String(r.int(2, 23)).padStart(2, "0")} ${MONTHS[m]} ${k > 3 ? 2025 : 2026}`,
        publishedBy: person([4, 12, 9, 18][(bi + k) % 4]!),
        summary: LEGAL_SUMMARIES[(bi + k + ti) % LEGAL_SUMMARIES.length]!,
        reaccept: k === 0 ? bi !== 4 : r.bool(0.4),
        acceptance: k === 0 ? 0 : 100,
      };
    });
    const acceptance = bi === 0 && ti === 0 ? 72.4 : +(r.range(84, 99.6)).toFixed(1);
    history[0]!.acceptance = acceptance;
    const base = [48210, 12840, 6420, 3180][ti]!;
    return {
      id: `${t.id}-${b.slug}`,
      tenant: t.id,
      name: b.name,
      slug: b.slug,
      version: history[0]!.version,
      lastPublished: bi === 0 && ti === 0 ? "18 Sep 2026" : history[0]!.publishedAt,
      acceptance,
      pending: Math.round((base * (100 - acceptance)) / 100),
      required: b.required,
      languages: t.languages,
      history,
    };
  });
});

export interface CntAcceptance {
  id: string;
  client: Person;
  login: string;
  doc: string;
  version: string;
  tenant: CntTenantId;
  at: string; // ISO
  ip: string;
  device: string;
  method: "checkbox" | "modal" | "signup";
}

const DEVICES = ["iPhone 15 · iOS 19.2", "Pixel 9 · Android 16", "Chrome 141 · Windows 11", "Safari 19 · macOS", "Kalks app · Android 16", "Kalks app · iOS 19", "Edge 140 · Windows 11", "Firefox 142 · Ubuntu"];
const accR = seeded(4242);
export const CNT_ACCEPTANCE_LOG: CntAcceptance[] = Array.from({ length: 64 }, (_, i) => {
  const p = person(accR.int(0, 23));
  const doc = accR.pick(LEGAL_BASE);
  const tenant = accR.pick(CNT_TENANTS).id;
  const d = CNT_LEGAL_DOCS.find((x) => x.tenant === tenant && x.slug === doc.slug)!;
  const mins = i * accR.int(4, 19);
  const at = new Date(Date.UTC(2026, 8, 24, 11, 30) - mins * 60000).toISOString();
  return {
    id: `ACC-${(882410 - i).toString()}`,
    client: p,
    login: String(80400000 + accR.int(1000, 49999)),
    doc: d.name,
    version: d.version,
    tenant,
    at,
    ip: `${accR.int(31, 213)}.${accR.int(1, 254)}.${accR.int(1, 254)}.${accR.int(1, 254)}`,
    device: accR.pick(DEVICES),
    method: accR.pick(["modal", "modal", "checkbox", "signup"] as const),
  };
});

/* ------------------------------------------------------------------ */
/* Email templates                                                     */
/* ------------------------------------------------------------------ */

export interface CntEmailTemplate {
  id: string;
  name: string;
  category: "Onboarding" | "Security" | "Funding" | "Trading" | "Compliance" | "Partners";
  trigger: string;
  subject: string;
  preheader: string;
  body: string;
  cta?: string;
  variables: string[];
  languages: number;
  sent30d: number;
  openRate: number;
  updated: string;
  status: "live" | "draft";
}

export const CNT_EMAIL_VARIABLES: { key: string; label: string; sample: string }[] = [
  { key: "first_name", label: "First name", sample: "Arjun" },
  { key: "last_name", label: "Last name", sample: "Mehta" },
  { key: "tenant_name", label: "Broker name", sample: "Kalks Markets" },
  { key: "login", label: "Account login", sample: "80412337" },
  { key: "amount", label: "Amount", sample: "2,500.00 USDT" },
  { key: "otp", label: "One-time code", sample: "482 915" },
  { key: "method", label: "Payment method", sample: "USDT · TRC20" },
  { key: "tx_id", label: "Transaction ID", sample: "TX904412" },
  { key: "margin_level", label: "Margin level", sample: "84.2%" },
  { key: "reason", label: "Rejection reason", sample: "Proof of address older than 90 days" },
  { key: "reset_link", label: "Reset link", sample: "https://kalkstrade.com/reset/4f1c…" },
  { key: "support_email", label: "Support email", sample: "support@kalkstrade.com" },
  { key: "date", label: "Date", sample: "24 Sep 2026, 14:32 GMT+3" },
  { key: "commission", label: "IB commission", sample: "$4,812.40" },
  { key: "server", label: "Trading server", sample: "Kalks-Live01" },
];

export const CNT_EMAIL_TEMPLATES: CntEmailTemplate[] = [
  { id: "welcome", name: "Welcome", category: "Onboarding", trigger: "user.registered", subject: "Welcome to {{tenant_name}}, {{first_name}}", preheader: "Your account is ready — here's how to get started.", body: "Hi {{first_name}},\n\nWelcome to {{tenant_name}}. Your client area is ready and a demo account with $100,000 virtual funds has been opened for you.\n\nTo start trading live, verify your identity and make your first deposit — it takes less than five minutes.", cta: "Complete verification", variables: ["first_name", "tenant_name"], languages: 22, sent30d: 14820, openRate: 68.4, updated: "12 Sep 2026", status: "live" },
  { id: "otp", name: "OTP code", category: "Security", trigger: "auth.otp_requested", subject: "{{otp}} is your {{tenant_name}} verification code", preheader: "This code expires in 10 minutes.", body: "Hi {{first_name}},\n\nUse the code below to confirm your action. It expires in 10 minutes.\n\n{{otp}}\n\nIf you didn't request this code, change your password immediately and contact {{support_email}}.", variables: ["first_name", "otp", "tenant_name", "support_email"], languages: 22, sent30d: 96410, openRate: 91.2, updated: "02 Sep 2026", status: "live" },
  { id: "deposit", name: "Deposit confirmed", category: "Funding", trigger: "wallet.deposit_completed", subject: "Deposit of {{amount}} received", preheader: "Funds are available in your wallet.", body: "Hi {{first_name}},\n\nWe've received your deposit of {{amount}} via {{method}}. The funds are now available in your wallet.\n\nTransaction ID: {{tx_id}}\nDate: {{date}}", cta: "Transfer to trading account", variables: ["first_name", "amount", "method", "tx_id", "date"], languages: 22, sent30d: 21340, openRate: 74.9, updated: "18 Sep 2026", status: "live" },
  { id: "withdrawal", name: "Withdrawal approved", category: "Funding", trigger: "wallet.withdrawal_approved", subject: "Your withdrawal of {{amount}} is on its way", preheader: "Approved and sent to the network.", body: "Hi {{first_name}},\n\nYour withdrawal of {{amount}} via {{method}} has been approved and sent. Network confirmations usually take a few minutes.\n\nTransaction ID: {{tx_id}}", cta: "View transaction", variables: ["first_name", "amount", "method", "tx_id"], languages: 22, sent30d: 9820, openRate: 81.3, updated: "18 Sep 2026", status: "live" },
  { id: "kyc_ok", name: "KYC approved", category: "Compliance", trigger: "kyc.approved", subject: "You're verified, {{first_name}}", preheader: "Deposits and withdrawals are now unlocked.", body: "Hi {{first_name}},\n\nGood news — your identity has been verified. Deposits, withdrawals and live trading are now fully unlocked on {{tenant_name}}.", cta: "Make your first deposit", variables: ["first_name", "tenant_name"], languages: 22, sent30d: 6210, openRate: 79.1, updated: "05 Sep 2026", status: "live" },
  { id: "kyc_rej", name: "KYC rejected", category: "Compliance", trigger: "kyc.rejected", subject: "Action needed: we couldn't verify your document", preheader: "Please upload a new document.", body: "Hi {{first_name}},\n\nWe couldn't verify the document you uploaded.\n\nReason: {{reason}}\n\nPlease upload a new document from your client area. Most re-submissions are reviewed within 15 minutes.", cta: "Upload new document", variables: ["first_name", "reason"], languages: 22, sent30d: 1840, openRate: 83.6, updated: "05 Sep 2026", status: "live" },
  { id: "margin_call", name: "Margin call", category: "Trading", trigger: "account.margin_call", subject: "Margin call on account {{login}}", preheader: "Your margin level is {{margin_level}}.", body: "Hi {{first_name}},\n\nThe margin level on account {{login}} ({{server}}) has fallen to {{margin_level}}. If it reaches the stop out level, positions will be closed automatically, starting with the largest loss.\n\nConsider depositing funds or reducing your exposure.", cta: "Deposit now", variables: ["first_name", "login", "server", "margin_level"], languages: 22, sent30d: 3120, openRate: 88.7, updated: "20 Aug 2026", status: "live" },
  { id: "stop_out", name: "Stop out", category: "Trading", trigger: "account.stop_out", subject: "Positions closed on account {{login}}", preheader: "Stop out was triggered.", body: "Hi {{first_name}},\n\nStop out was triggered on account {{login}} at {{date}} and some positions were closed to protect your balance. Negative balance protection applies, so you will never lose more than your deposit.", cta: "Review trade history", variables: ["first_name", "login", "date"], languages: 22, sent30d: 842, openRate: 90.4, updated: "20 Aug 2026", status: "live" },
  { id: "pwd_reset", name: "Password reset", category: "Security", trigger: "auth.password_reset", subject: "Reset your {{tenant_name}} password", preheader: "The link expires in 30 minutes.", body: "Hi {{first_name}},\n\nWe received a request to reset your password. Use the button below — the link expires in 30 minutes.\n\nIf this wasn't you, you can safely ignore this email.", cta: "Reset password", variables: ["first_name", "tenant_name", "reset_link"], languages: 22, sent30d: 4410, openRate: 86.2, updated: "02 Sep 2026", status: "live" },
  { id: "ib_paid", name: "IB commission paid", category: "Partners", trigger: "partner.commission_paid", subject: "Your commission of {{commission}} has been paid", preheader: "Monthly partner payout.", body: "Hi {{first_name}},\n\nYour partner commission of {{commission}} for August has been credited to your wallet.\n\nThank you for growing with {{tenant_name}}.", cta: "Open partner dashboard", variables: ["first_name", "commission", "tenant_name"], languages: 16, sent30d: 1204, openRate: 72.8, updated: "01 Sep 2026", status: "live" },
  { id: "account_opened", name: "Trading account opened", category: "Onboarding", trigger: "account.created", subject: "Your new account {{login}} is ready", preheader: "Server {{server}}.", body: "Hi {{first_name}},\n\nYour trading account is ready.\n\nLogin: {{login}}\nServer: {{server}}\n\nYou can trade in the web terminal or connect with the mobile app.", cta: "Open web terminal", variables: ["first_name", "login", "server"], languages: 22, sent30d: 8930, openRate: 77.5, updated: "10 Sep 2026", status: "live" },
  { id: "dormant", name: "Dormant account reminder", category: "Onboarding", trigger: "account.dormant_60d", subject: "We miss you, {{first_name}}", preheader: "Markets moved while you were away.", body: "Hi {{first_name}},\n\nIt's been a while. Gold is up 6% this month and spreads on EURUSD start from 0.0 pips on Pro accounts.", cta: "Log in", variables: ["first_name"], languages: 8, sent30d: 0, openRate: 0, updated: "24 Sep 2026", status: "draft" },
];

export const CNT_LANG_OPTIONS = [
  { code: "en", name: "English", flag: "gb" },
  { code: "ar", name: "العربية", flag: "sa" },
  { code: "es", name: "Español", flag: "es" },
  { code: "pt", name: "Português", flag: "br" },
  { code: "hi", name: "हिन्दी", flag: "in" },
  { code: "vi", name: "Tiếng Việt", flag: "vn" },
  { code: "id", name: "Bahasa Indonesia", flag: "id" },
  { code: "tr", name: "Türkçe", flag: "tr" },
];

/* ------------------------------------------------------------------ */
/* In-app notification templates                                       */
/* ------------------------------------------------------------------ */

export interface CntNotifTemplate {
  id: string;
  event: string;
  name: string;
  category: "Funding" | "Trading" | "Account" | "Promo" | "Social" | "Security";
  channels: ("in-app" | "push")[];
  title: string;
  body: string;
  icon: string; // Icon3D name
  variables: string[];
  enabled: boolean;
  sent30d: number;
  ctr: number;
  priority: "high" | "normal";
}

export const CNT_NOTIF_TEMPLATES: CntNotifTemplate[] = [
  { id: "nt1", event: "wallet.deposit_completed", name: "Deposit credited", category: "Funding", channels: ["in-app", "push"], title: "Deposit received", body: "{{amount}} via {{method}} is now in your wallet.", icon: "money_with_wings", variables: ["amount", "method"], enabled: true, sent30d: 21340, ctr: 38.2, priority: "normal" },
  { id: "nt2", event: "wallet.withdrawal_approved", name: "Withdrawal sent", category: "Funding", channels: ["in-app", "push"], title: "Withdrawal approved", body: "{{amount}} is on its way to {{address}}.", icon: "dollar_banknote", variables: ["amount", "address"], enabled: true, sent30d: 9820, ctr: 22.4, priority: "normal" },
  { id: "nt3", event: "account.margin_call", name: "Margin call", category: "Trading", channels: ["in-app", "push"], title: "Margin level {{margin_level}}", body: "Account {{login}} is close to stop out. Add funds or reduce exposure.", icon: "warning", variables: ["margin_level", "login"], enabled: true, sent30d: 3120, ctr: 61.8, priority: "high" },
  { id: "nt4", event: "order.tp_hit", name: "Take profit hit", category: "Trading", channels: ["push"], title: "{{symbol}} take profit hit", body: "Closed {{volume}} lots at {{price}} for {{profit}}.", icon: "chart_increasing", variables: ["symbol", "volume", "price", "profit"], enabled: true, sent30d: 48210, ctr: 18.9, priority: "normal" },
  { id: "nt5", event: "order.sl_hit", name: "Stop loss hit", category: "Trading", channels: ["push"], title: "{{symbol}} stop loss hit", body: "Closed {{volume}} lots at {{price}} ({{profit}}).", icon: "chart_decreasing", variables: ["symbol", "volume", "price", "profit"], enabled: true, sent30d: 39120, ctr: 21.3, priority: "normal" },
  { id: "nt6", event: "price.alert", name: "Price alert", category: "Trading", channels: ["in-app", "push"], title: "{{symbol}} reached {{price}}", body: "Your alert was triggered. Tap to open the chart.", icon: "bell", variables: ["symbol", "price"], enabled: true, sent30d: 66480, ctr: 34.1, priority: "normal" },
  { id: "nt7", event: "kyc.approved", name: "KYC approved", category: "Account", channels: ["in-app", "push"], title: "You're verified", body: "Deposits and withdrawals are unlocked, {{first_name}}.", icon: "check_mark_button", variables: ["first_name"], enabled: true, sent30d: 6210, ctr: 52.7, priority: "normal" },
  { id: "nt8", event: "security.new_login", name: "New device login", category: "Security", channels: ["in-app", "push"], title: "New login from {{device}}", body: "{{city}} · {{ip}}. Not you? Secure your account now.", icon: "locked", variables: ["device", "city", "ip"], enabled: true, sent30d: 18840, ctr: 9.6, priority: "high" },
  { id: "nt9", event: "copy.master_trade", name: "Copied trade opened", category: "Social", channels: ["in-app"], title: "{{master}} opened {{symbol}}", body: "Copied {{volume}} lots into your account.", icon: "busts_in_silhouette", variables: ["master", "symbol", "volume"], enabled: true, sent30d: 12402, ctr: 14.2, priority: "normal" },
  { id: "nt10", event: "promo.bonus_available", name: "Deposit bonus", category: "Promo", channels: ["in-app", "push"], title: "{{bonus}}% bonus unlocked", body: "Deposit before {{expiry}} to claim up to {{max}}.", icon: "wrapped_gift", variables: ["bonus", "expiry", "max"], enabled: false, sent30d: 0, ctr: 0, priority: "normal" },
  { id: "nt11", event: "contest.rank_changed", name: "Contest rank", category: "Promo", channels: ["in-app"], title: "You're #{{rank}} in {{contest}}", body: "{{gap}} behind the next place. Keep going!", icon: "trophy", variables: ["rank", "contest", "gap"], enabled: true, sent30d: 5402, ctr: 27.5, priority: "normal" },
  { id: "nt12", event: "prop.phase_passed", name: "Prop phase passed", category: "Trading", channels: ["in-app", "push"], title: "Phase {{phase}} passed", body: "Your {{size}} challenge moves to the next phase.", icon: "1st_place_medal", variables: ["phase", "size"], enabled: true, sent30d: 612, ctr: 71.4, priority: "high" },
];

export const CNT_NOTIF_SAMPLES: Record<string, string> = {
  amount: "2,500.00 USDT",
  method: "USDT · TRC20",
  address: "TQ7x…9KfE",
  margin_level: "84.2%",
  login: "80412337",
  symbol: "XAUUSD",
  volume: "0.50",
  price: "2,654.30",
  profit: "+$612.40",
  first_name: "Arjun",
  device: "iPhone 15",
  city: "Dubai",
  ip: "94.206.18.41",
  master: "Lucas Ferreira",
  bonus: "30",
  expiry: "30 Sep",
  max: "$3,000",
  rank: "4",
  contest: "September Gold Rush",
  gap: "$218",
  phase: "1",
  size: "$50K",
};

/* ------------------------------------------------------------------ */
/* Academy                                                             */
/* ------------------------------------------------------------------ */

export interface CntLesson {
  id: string;
  title: string;
  type: "video" | "article" | "quiz";
  duration: string;
}

export interface CntCourse {
  id: string;
  title: string;
  subtitle: string;
  cover: string;
  level: "Beginner" | "Intermediate" | "Advanced";
  category: string;
  lessons: CntLesson[];
  quizzes: number;
  enrolments: number;
  completion: number;
  rating: number;
  status: "published" | "draft" | "scheduled";
  author: Person;
  languages: number;
  updated: string;
  tenants: string;
}

const L = (id: string, title: string, type: CntLesson["type"], duration: string): CntLesson => ({ id, title, type, duration });

export const CNT_COURSES: CntCourse[] = [
  { id: "c1", title: "Forex trading foundations", subtitle: "Pips, lots, leverage and your first trade", cover: "/assets/photos/trading-screen.jpg", level: "Beginner", category: "Forex", lessons: [L("l1", "What moves currency prices", "video", "8:42"), L("l2", "Pips, lots and contract size", "video", "11:05"), L("l3", "Leverage and margin explained", "article", "6 min"), L("l4", "Placing your first order in the terminal", "video", "9:30"), L("l5", "Stop loss and take profit", "video", "7:18"), L("l6", "Check your understanding", "quiz", "10 Q")], quizzes: 2, enrolments: 18420, completion: 64, rating: 4.8, status: "published", author: PEOPLE[9]!, languages: 22, updated: "12 Sep 2026", tenants: "All tenants" },
  { id: "c2", title: "Trading gold like a pro", subtitle: "XAUUSD drivers, sessions and risk", cover: "/assets/photos/gold.jpg", level: "Intermediate", category: "Metals", lessons: [L("l1", "Why gold reacts to real yields", "video", "12:14"), L("l2", "London and New York sessions", "video", "9:02"), L("l3", "Trading around US data releases", "article", "8 min"), L("l4", "Position sizing on volatile instruments", "video", "10:40"), L("l5", "Case study: PCE day", "video", "14:22"), L("l6", "Final quiz", "quiz", "12 Q")], quizzes: 2, enrolments: 9840, completion: 52, rating: 4.9, status: "published", author: PEOPLE[4]!, languages: 18, updated: "20 Sep 2026", tenants: "All tenants" },
  { id: "c3", title: "Crypto CFDs: 24/7 markets", subtitle: "BTC, ETH and weekend risk", cover: "/assets/photos/bitcoin.jpg", level: "Intermediate", category: "Crypto", lessons: [L("l1", "How crypto CFDs differ from spot", "video", "7:55"), L("l2", "Weekend gaps and funding", "article", "5 min"), L("l3", "Volatility-adjusted stops", "video", "9:48"), L("l4", "Quiz", "quiz", "8 Q")], quizzes: 1, enrolments: 7210, completion: 47, rating: 4.6, status: "published", author: PEOPLE[8]!, languages: 14, updated: "02 Sep 2026", tenants: "Kalks Markets, NovaTrade Asia" },
  { id: "c4", title: "Risk management masterclass", subtitle: "Drawdown, correlation and position sizing", cover: "/assets/photos/analytics.jpg", level: "Advanced", category: "Risk", lessons: [L("l1", "The maths of drawdown", "video", "13:10"), L("l2", "Correlated exposure across pairs", "video", "11:44"), L("l3", "Kelly, fixed-fractional and volatility sizing", "article", "12 min"), L("l4", "Building a trading journal", "video", "8:20"), L("l5", "Stress-testing a portfolio", "video", "10:05"), L("l6", "Scenario quiz", "quiz", "15 Q"), L("l7", "Capstone assessment", "quiz", "20 Q")], quizzes: 3, enrolments: 4380, completion: 38, rating: 4.9, status: "published", author: PEOPLE[4]!, languages: 12, updated: "15 Sep 2026", tenants: "All tenants" },
  { id: "c5", title: "Passing your prop challenge", subtitle: "Rules, daily loss and consistency", cover: "/assets/photos/trader.jpg", level: "Intermediate", category: "Prop", lessons: [L("l1", "How evaluation rules work", "video", "9:12"), L("l2", "Daily loss vs max loss", "article", "7 min"), L("l3", "Consistency and news rules", "video", "8:35"), L("l4", "Rule check", "quiz", "10 Q")], quizzes: 1, enrolments: 6120, completion: 58, rating: 4.7, status: "published", author: PEOPLE[8]!, languages: 16, updated: "21 Sep 2026", tenants: "Kalks Markets" },
  { id: "c6", title: "Indices and stock CFDs", subtitle: "NAS100, US30 and earnings season", cover: "/assets/photos/stock-market.jpg", level: "Beginner", category: "Indices", lessons: [L("l1", "What an index CFD is", "video", "6:40"), L("l2", "Trading earnings announcements", "video", "10:18"), L("l3", "Dividends and adjustments", "article", "5 min")], quizzes: 1, enrolments: 3210, completion: 44, rating: 4.5, status: "scheduled", author: PEOPLE[9]!, languages: 10, updated: "23 Sep 2026", tenants: "All tenants" },
  { id: "c7", title: "Copy trading & PAMM for investors", subtitle: "Choosing masters and managing risk", cover: "/assets/photos/finance.jpg", level: "Beginner", category: "Social", lessons: [L("l1", "How copy trading works", "video", "7:30"), L("l2", "Reading a master's statistics", "video", "9:15"), L("l3", "Setting copy limits", "article", "4 min")], quizzes: 1, enrolments: 0, completion: 0, rating: 0, status: "draft", author: PEOPLE[12]!, languages: 1, updated: "24 Sep 2026", tenants: "Aurum FX" },
  { id: "c8", title: "Algo trading with the Kalks API", subtitle: "From REST orders to WebSocket quotes", cover: "/assets/photos/dashboard.jpg", level: "Advanced", category: "Algo", lessons: [L("l1", "API keys and permissions", "video", "6:20"), L("l2", "Placing orders over REST", "video", "12:48"), L("l3", "Streaming quotes over WebSocket", "video", "11:02"), L("l4", "Backtesting in the strategy builder", "article", "9 min"), L("l5", "Code review quiz", "quiz", "10 Q")], quizzes: 1, enrolments: 1840, completion: 31, rating: 4.8, status: "published", author: PEOPLE[21]!, languages: 4, updated: "08 Sep 2026", tenants: "All tenants" },
];

/* ------------------------------------------------------------------ */
/* Translations                                                        */
/* ------------------------------------------------------------------ */

export const CNT_LANGS = [
  { code: "en", name: "English", flag: "gb", rtl: false, keysMissing: 0 },
  { code: "ar", name: "Arabic", flag: "sa", rtl: true, keysMissing: 12 },
  { code: "ur", name: "Urdu", flag: "pk", rtl: true, keysMissing: 148 },
  { code: "hi", name: "Hindi", flag: "in", rtl: false, keysMissing: 36 },
  { code: "es", name: "Spanish", flag: "es", rtl: false, keysMissing: 4 },
  { code: "pt", name: "Portuguese", flag: "br", rtl: false, keysMissing: 9 },
  { code: "fr", name: "French", flag: "fr", rtl: false, keysMissing: 22 },
  { code: "de", name: "German", flag: "de", rtl: false, keysMissing: 31 },
  { code: "tr", name: "Turkish", flag: "tr", rtl: false, keysMissing: 18 },
  { code: "ru", name: "Russian", flag: "ru", rtl: false, keysMissing: 27 },
  { code: "zh", name: "Chinese", flag: "cn", rtl: false, keysMissing: 14 },
  { code: "ja", name: "Japanese", flag: "jp", rtl: false, keysMissing: 64 },
  { code: "ko", name: "Korean", flag: "kr", rtl: false, keysMissing: 71 },
  { code: "id", name: "Indonesian", flag: "id", rtl: false, keysMissing: 19 },
  { code: "vi", name: "Vietnamese", flag: "vn", rtl: false, keysMissing: 11 },
  { code: "th", name: "Thai", flag: "th", rtl: false, keysMissing: 96 },
  { code: "ms", name: "Malay", flag: "my", rtl: false, keysMissing: 42 },
  { code: "fa", name: "Persian", flag: "ir", rtl: true, keysMissing: 212 },
  { code: "it", name: "Italian", flag: "it", rtl: false, keysMissing: 38 },
  { code: "pl", name: "Polish", flag: "pl", rtl: false, keysMissing: 124 },
  { code: "bn", name: "Bengali", flag: "bd", rtl: false, keysMissing: 286 },
  { code: "sw", name: "Swahili", flag: "ke", rtl: false, keysMissing: 341 },
] as const;
export type CntLang = (typeof CNT_LANGS)[number]["code"];
export const CNT_TOTAL_KEYS = 2418;

const LANG_ORDER = CNT_LANGS.map((l) => l.code);

// [key, namespace, ...22 translations in CNT_LANGS order]
const T: [string, string, ...string[]][] = [
  ["nav.dashboard", "navigation", "Dashboard", "لوحة التحكم", "ڈیش بورڈ", "डैशबोर्ड", "Panel", "Painel", "Tableau de bord", "Übersicht", "Kontrol paneli", "Панель", "仪表板", "ダッシュボード", "대시보드", "Dasbor", "Bảng điều khiển", "แดชบอร์ด", "Papan pemuka", "داشبورد", "Pannello", "Pulpit", "ড্যাশবোর্ড", "Dashibodi"],
  ["nav.wallet", "navigation", "Wallet", "المحفظة", "والیٹ", "वॉलेट", "Billetera", "Carteira", "Portefeuille", "Wallet", "Cüzdan", "Кошелёк", "钱包", "ウォレット", "지갑", "Dompet", "Ví", "กระเป๋าเงิน", "Dompet", "کیف پول", "Portafoglio", "Portfel", "ওয়ালেট", "Pochi"],
  ["nav.support", "navigation", "Support", "الدعم", "سپورٹ", "सहायता", "Soporte", "Suporte", "Assistance", "Support", "Destek", "Поддержка", "客服支持", "サポート", "고객지원", "Dukungan", "Hỗ trợ", "ฝ่ายสนับสนุน", "Sokongan", "پشتیبانی", "Assistenza", "Pomoc", "সহায়তা", "Msaada"],
  ["nav.settings", "navigation", "Settings", "الإعدادات", "ترتیبات", "सेटिंग्स", "Ajustes", "Configurações", "Paramètres", "Einstellungen", "Ayarlar", "Настройки", "设置", "設定", "설정", "Pengaturan", "Cài đặt", "การตั้งค่า", "Tetapan", "تنظیمات", "Impostazioni", "Ustawienia", "সেটিংস", "Mipangilio"],
  ["wallet.deposit", "wallet", "Deposit", "إيداع", "جمع کروائیں", "जमा करें", "Depositar", "Depositar", "Déposer", "Einzahlen", "Para yatır", "Пополнить", "入金", "入金", "입금", "Setor", "Nạp tiền", "ฝากเงิน", "Deposit", "واریز", "Deposita", "Wpłać", "জমা দিন", "Weka pesa"],
  ["wallet.withdraw", "wallet", "Withdraw", "سحب", "رقم نکلوائیں", "निकासी", "Retirar", "Sacar", "Retirer", "Auszahlen", "Para çek", "Вывести", "出金", "出金", "출금", "Tarik", "Rút tiền", "ถอนเงิน", "Keluarkan", "برداشت", "Preleva", "Wypłać", "উত্তোলন", "Toa pesa"],
  ["wallet.pending", "wallet", "Pending", "قيد الانتظار", "زیر التواء", "लंबित", "Pendiente", "Pendente", "En attente", "Ausstehend", "Beklemede", "В обработке", "待处理", "保留中", "대기 중", "Tertunda", "Đang chờ", "รอดำเนินการ", "Belum selesai", "در انتظار", "In sospeso", "Oczekujące", "অপেক্ষমাণ", "Inasubiri"],
  ["account.balance", "accounts", "Balance", "الرصيد", "بیلنس", "शेष राशि", "Saldo", "Saldo", "Solde", "Kontostand", "Bakiye", "Баланс", "余额", "残高", "잔고", "Saldo", "Số dư", "ยอดคงเหลือ", "Baki", "موجودی", "Saldo", "Saldo", "ব্যালেন্স", "Salio"],
  ["account.equity", "accounts", "Equity", "حقوق الملكية", "ایکویٹی", "इक्विटी", "Patrimonio", "Patrimônio", "Fonds propres", "Eigenkapital", "Varlık", "Средства", "净值", "有効証拠金", "자산", "Ekuitas", "Vốn chủ sở hữu", "มูลค่าสุทธิ", "Ekuiti", "ارزش خالص", "Patrimonio", "Kapitał", "ইক্যুইটি", "Thamani halisi"],
  ["account.open_new", "accounts", "Open new account", "فتح حساب جديد", "نیا اکاؤنٹ کھولیں", "नया खाता खोलें", "Abrir cuenta nueva", "Abrir nova conta", "Ouvrir un compte", "Neues Konto eröffnen", "Yeni hesap aç", "Открыть счёт", "开立新账户", "新規口座開設", "새 계좌 개설", "Buka akun baru", "Mở tài khoản mới", "เปิดบัญชีใหม่", "Buka akaun baharu", "افتتاح حساب جدید", "Apri nuovo conto", "Otwórz nowe konto", "নতুন অ্যাকাউন্ট খুলুন", "Fungua akaunti mpya"],
  ["trade.buy", "trading", "Buy", "شراء", "خریدیں", "खरीदें", "Comprar", "Comprar", "Acheter", "Kaufen", "Al", "Купить", "买入", "買い", "매수", "Beli", "Mua", "ซื้อ", "Beli", "خرید", "Compra", "Kup", "কিনুন", "Nunua"],
  ["trade.sell", "trading", "Sell", "بيع", "فروخت کریں", "बेचें", "Vender", "Vender", "Vendre", "Verkaufen", "Sat", "Продать", "卖出", "売り", "매도", "Jual", "Bán", "ขาย", "Jual", "فروش", "Vendi", "Sprzedaj", "বিক্রি করুন", "Uza"],
  ["trade.margin_call", "trading", "Margin level below 100%", "مستوى الهامش أقل من 100%", "مارجن لیول 100% سے کم", "मार्जिन स्तर 100% से कम", "Nivel de margen por debajo del 100%", "Nível de margem abaixo de 100%", "Niveau de marge inférieur à 100 %", "Margin-Level unter 100 %", "Marjin seviyesi %100'ün altında", "Уровень маржи ниже 100%", "保证金水平低于100%", "証拠金維持率が100%を下回りました", "증거금 수준 100% 미만", "Level margin di bawah 100%", "Mức ký quỹ dưới 100%", "ระดับมาร์จิ้นต่ำกว่า 100%", "Tahap margin di bawah 100%", "سطح مارجین زیر ۱۰۰٪", "Livello di margine sotto il 100%", "Poziom depozytu poniżej 100%", "মার্জিন লেভেল ১০০%-এর নিচে", "Kiwango cha margin chini ya 100%"],
  ["kyc.verify_identity", "kyc", "Verify your identity", "تحقق من هويتك", "اپنی شناخت کی تصدیق کریں", "अपनी पहचान सत्यापित करें", "Verifica tu identidad", "Verifique sua identidade", "Vérifiez votre identité", "Bestätigen Sie Ihre Identität", "Kimliğinizi doğrulayın", "Подтвердите личность", "验证您的身份", "本人確認を行ってください", "신원을 인증하세요", "Verifikasi identitas Anda", "Xác minh danh tính", "ยืนยันตัวตนของคุณ", "Sahkan identiti anda", "هویت خود را تأیید کنید", "Verifica la tua identità", "Zweryfikuj tożsamość", "আপনার পরিচয় যাচাই করুন", "Thibitisha utambulisho wako"],
  ["kyc.upload_document", "kyc", "Upload document", "تحميل المستند", "دستاویز اپ لوڈ کریں", "दस्तावेज़ अपलोड करें", "Subir documento", "Enviar documento", "Téléverser le document", "Dokument hochladen", "Belge yükle", "Загрузить документ", "上传文件", "書類をアップロード", "서류 업로드", "Unggah dokumen", "Tải lên tài liệu", "อัปโหลดเอกสาร", "Muat naik dokumen", "بارگذاری مدرک", "Carica documento", "Prześlij dokument", "নথি আপলোড করুন", "Pakia hati"],
  ["auth.sign_in", "auth", "Sign in", "تسجيل الدخول", "سائن ان کریں", "साइन इन करें", "Iniciar sesión", "Entrar", "Se connecter", "Anmelden", "Giriş yap", "Войти", "登录", "ログイン", "로그인", "Masuk", "Đăng nhập", "เข้าสู่ระบบ", "Log masuk", "ورود", "Accedi", "Zaloguj się", "সাইন ইন", "Ingia"],
  ["auth.forgot_password", "auth", "Forgot password?", "نسيت كلمة المرور؟", "پاس ورڈ بھول گئے؟", "पासवर्ड भूल गए?", "¿Olvidaste tu contraseña?", "Esqueceu a senha?", "Mot de passe oublié ?", "Passwort vergessen?", "Şifrenizi mi unuttunuz?", "Забыли пароль?", "忘记密码？", "パスワードをお忘れですか？", "비밀번호를 잊으셨나요?", "Lupa kata sandi?", "Quên mật khẩu?", "ลืมรหัสผ่าน?", "Lupa kata laluan?", "رمز عبور را فراموش کرده‌اید؟", "Password dimenticata?", "Nie pamiętasz hasła?", "পাসওয়ার্ড ভুলে গেছেন?", "Umesahau nenosiri?"],
  ["email.welcome_subject", "emails", "Welcome to {{tenant_name}}", "مرحباً بك في {{tenant_name}}", "{{tenant_name}} میں خوش آمدید", "{{tenant_name}} में आपका स्वागत है", "Bienvenido a {{tenant_name}}", "Bem-vindo à {{tenant_name}}", "Bienvenue chez {{tenant_name}}", "Willkommen bei {{tenant_name}}", "{{tenant_name}}'a hoş geldiniz", "Добро пожаловать в {{tenant_name}}", "欢迎来到 {{tenant_name}}", "{{tenant_name}}へようこそ", "{{tenant_name}}에 오신 것을 환영합니다", "Selamat datang di {{tenant_name}}", "Chào mừng đến với {{tenant_name}}", "ยินดีต้อนรับสู่ {{tenant_name}}", "Selamat datang ke {{tenant_name}}", "به {{tenant_name}} خوش آمدید", "Benvenuto in {{tenant_name}}", "Witamy w {{tenant_name}}", "{{tenant_name}}-এ স্বাগতম", "Karibu {{tenant_name}}"],
];

export const CNT_NAMESPACES = ["navigation", "wallet", "accounts", "trading", "kyc", "auth", "emails"] as const;

export interface CntTranslationRow {
  key: string;
  namespace: string;
  values: Record<string, string | null>; // null = missing
  mt: Record<string, string>; // machine translation suggestions
  source: Record<string, "human" | "mt">;
  updated: string;
}

const mR = seeded(5150);
const MISS_P: Record<string, number> = { ur: 0.3, fa: 0.4, bn: 0.5, sw: 0.55, pl: 0.3, th: 0.25, ko: 0.2, ja: 0.15, ms: 0.15, it: 0.12, de: 0.1, fr: 0.08, ru: 0.1, hi: 0.1 };

export const CNT_TRANSLATIONS: CntTranslationRow[] = T.map(([key, namespace, ...vals]) => {
  const values: Record<string, string | null> = {};
  const mt: Record<string, string> = {};
  const source: Record<string, "human" | "mt"> = {};
  LANG_ORDER.forEach((code, i) => {
    const v = vals[i]!;
    mt[code] = v;
    const miss = code !== "en" && mR.bool(MISS_P[code] ?? 0.04);
    values[code] = miss ? null : v;
    source[code] = code === "en" || mR.bool(0.6) ? "human" : "mt";
  });
  return { key, namespace, values, mt, source, updated: `${mR.int(1, 23)} Sep 2026` };
});

export interface CntOverride {
  key: string;
  tenant: CntTenantId;
  lang: string;
  value: string;
  by: string;
}

export const CNT_OVERRIDES: CntOverride[] = [
  { key: "wallet.deposit", tenant: "aurum", lang: "en", value: "Fund account", by: "Sofia Rossi" },
  { key: "wallet.deposit", tenant: "dunes", lang: "ar", value: "تمويل الحساب", by: "Omar Haddad" },
  { key: "nav.dashboard", tenant: "nova", lang: "en", value: "Home", by: "Mei Lin" },
  { key: "nav.support", tenant: "aurum", lang: "en", value: "Concierge", by: "Sofia Rossi" },
  { key: "account.open_new", tenant: "nova", lang: "vi", value: "Mở tài khoản giao dịch", by: "Mei Lin" },
  { key: "email.welcome_subject", tenant: "dunes", lang: "en", value: "Ahlan, welcome to {{tenant_name}}", by: "Omar Haddad" },
  { key: "wallet.withdraw", tenant: "aurum", lang: "es", value: "Retirar fondos", by: "Carlos Mendoza" },
];
