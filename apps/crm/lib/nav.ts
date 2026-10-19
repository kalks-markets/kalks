import {
  LayoutGrid,
  Layers,
  Wallet,
  PieChart,
  Handshake,
  Copy,
  Trophy,
  Gift,
  Code2,
  GraduationCap,
  UserRound,
  LifeBuoy,
  Newspaper,
  CalendarDays,
  Globe2,
  PlusCircle,
  ArrowDownToLine,
  ArrowUpFromLine,
  ArrowLeftRight,
  History,
  BarChart3,
  BookText,
  FileText,
  Users,
  Network,
  Coins,
  Link2,
  Banknote,
  Compass,
  Repeat,
  Landmark,
  LineChart,
  Crown,
  Briefcase,
  UsersRound,
  Target,
  Medal,
  Percent,
  Ticket,
  ListOrdered,
  KeyRound,
  Webhook,
  Workflow,
  FlaskConical,
  Store,
  Award,
  BookOpen,
  Bot,
  ShieldCheck,
  BadgeCheck,
  Eye,
  SlidersHorizontal,
  Bell,
  ChartSpline,
} from "lucide-react";
import type { NavModule } from "@kalks/ui";
import { IS_DEMO } from "@kalks/mock/mode";
import type { MessageKey, T } from "@kalks/i18n";
import { LIVE_GATED } from "@/lib/live";

export const CRM_NAV: NavModule[] = [
  {
    key: "dashboard",
    label: "Dashboard",
    icon: LayoutGrid,
    href: "/",
    match: ["/", "/markets", "/news", "/calendar"],
    section: "main",
    sub: [
      { href: "/", label: "Overview", icon: LayoutGrid },
      { href: "/markets", label: "Markets", icon: Globe2 },
      { href: "/news", label: "News", icon: Newspaper },
      { href: "/calendar", label: "Calendar", icon: CalendarDays },
    ],
  },
  {
    key: "accounts",
    label: "Accounts",
    icon: Layers,
    href: "/accounts",
    section: "main",
    sub: [
      { href: "/accounts", label: "My accounts", icon: Layers },
      { href: "/accounts/new", label: "Open account", icon: PlusCircle },
    ],
  },
  {
    key: "wallet",
    label: "Wallet",
    icon: Wallet,
    href: "/wallet",
    section: "main",
    sub: [
      { href: "/wallet", label: "Overview", icon: Wallet },
      { href: "/wallet/deposit", label: "Deposit", icon: ArrowDownToLine },
      { href: "/wallet/withdraw", label: "Withdraw", icon: ArrowUpFromLine },
      { href: "/wallet/transfer", label: "Transfer", icon: ArrowLeftRight },
      { href: "/wallet/history", label: "History", icon: History },
    ],
  },
  {
    key: "portfolio",
    label: "Portfolio",
    icon: PieChart,
    href: "/portfolio",
    section: "main",
    sub: [
      { href: "/portfolio", label: "Overview", icon: PieChart },
      { href: "/portfolio/analytics", label: "Analytics", icon: BarChart3 },
      { href: "/portfolio/history", label: "Trade history", icon: History },
      { href: "/portfolio/ledger", label: "Ledger", icon: BookText },
      { href: "/portfolio/statements", label: "Statements", icon: FileText },
    ],
  },
  { key: "options", label: "Options", icon: ChartSpline, href: "/options", section: "main" },
  {
    key: "partner",
    label: "Partner (IB)",
    icon: Handshake,
    href: "/partner",
    section: "grow",
    sub: [
      { href: "/partner", label: "Dashboard", icon: LayoutGrid },
      { href: "/partner/clients", label: "Clients", icon: Users },
      { href: "/partner/network", label: "Network", icon: Network },
      { href: "/partner/commissions", label: "Commissions", icon: Coins },
      { href: "/partner/links", label: "Links & materials", icon: Link2 },
      { href: "/partner/payouts", label: "Payouts", icon: Banknote },
    ],
  },
  {
    key: "social",
    label: "Copy & PAMM",
    icon: Copy,
    href: "/social",
    section: "grow",
    sub: [
      { href: "/social", label: "Discover", icon: Compass },
      { href: "/social/copy", label: "Copy trading", icon: Repeat },
      { href: "/social/pamm", label: "PAMM funds", icon: Landmark },
      { href: "/social/investments", label: "My investments", icon: LineChart },
      { href: "/social/managed", label: "Managed accounts", icon: UsersRound },
      { href: "/social/master", label: "Become a master", icon: Crown },
      { href: "/social/mam", label: "MAM manager", icon: Briefcase },
    ],
  },
  {
    key: "prop",
    label: "Prop Challenges",
    icon: Trophy,
    href: "/prop",
    section: "grow",
    sub: [
      { href: "/prop", label: "Challenges", icon: Target },
      { href: "/prop/mine", label: "My challenges", icon: Trophy },
      { href: "/prop/payouts", label: "Payouts", icon: Banknote },
      { href: "/prop/certificates", label: "Certificates", icon: Award },
    ],
  },
  {
    key: "rewards",
    label: "Contests & Rewards",
    icon: Gift,
    href: "/rewards",
    section: "grow",
    sub: [
      { href: "/rewards", label: "Contests", icon: Medal },
      { href: "/rewards/leaderboard", label: "Leaderboard", icon: ListOrdered },
      { href: "/rewards/loyalty", label: "Loyalty", icon: Gift },
      { href: "/rewards/cashback", label: "Cashback", icon: Percent },
      { href: "/rewards/promotions", label: "Promotions", icon: Ticket },
    ],
  },
  {
    key: "developer",
    label: "API & Algo",
    icon: Code2,
    href: "/developer",
    section: "build",
    sub: [
      { href: "/developer", label: "API keys", icon: KeyRound },
      { href: "/developer/webhooks", label: "Webhooks", icon: Webhook },
      { href: "/developer/strategies", label: "Strategy builder", icon: Workflow },
      { href: "/developer/deployments", label: "Running strategies", icon: Bot },
      { href: "/developer/backtests", label: "Backtests", icon: FlaskConical },
      { href: "/developer/marketplace", label: "Marketplace", icon: Store },
      { href: "/developer/docs", label: "Docs", icon: BookOpen },
    ],
  },
  {
    key: "academy",
    label: "Academy & AI Coach",
    icon: GraduationCap,
    href: "/academy",
    section: "learn",
    sub: [
      { href: "/academy", label: "Courses", icon: GraduationCap },
      { href: "/academy/glossary", label: "Glossary", icon: BookOpen },
      { href: "/academy/progress", label: "My progress", icon: Award },
      { href: "/academy/coach", label: "AI Coach", icon: Bot },
    ],
  },
  {
    key: "profile",
    label: "Profile & Security",
    icon: UserRound,
    href: "/profile",
    section: "account",
    sub: [
      { href: "/profile", label: "Profile", icon: UserRound },
      { href: "/profile/security", label: "Security", icon: ShieldCheck },
      { href: "/profile/verification", label: "Verification", icon: BadgeCheck },
      { href: "/profile/viewers", label: "View-only access", icon: Eye },
      { href: "/profile/notifications", label: "Notifications", icon: Bell },
      { href: "/profile/preferences", label: "Preferences", icon: SlidersHorizontal },
    ],
  },
  { key: "support", label: "Support", icon: LifeBuoy, href: "/support", section: "account" },
];

const gated = (href: string) => LIVE_GATED.some((g) => href === g || href.startsWith(g + "/"));

/** Live builds list every module, like the demo, without the demo's mock count badges and without
 * sub-pages that are still gated (LIVE_GATED) — navigation never leads to a placeholder. */
export const LIVE_NAV: NavModule[] = CRM_NAV.map((m) => ({
  ...m,
  label: m.key === "academy" ? "Academy" : m.label,
  badge: undefined, // demo badges are mock counts
  sub: m.sub?.filter((s) => !gated(s.href)).map((s) => ({ ...s, badge: undefined })),
}));

/** Navigation for this build. */
export const NAV: NavModule[] = IS_DEMO ? CRM_NAV : LIVE_NAV;

/** Flattened entries for the ⌘K palette ("Soon" modules are grouped as Coming soon in live builds). */
export const CRM_COMMANDS = NAV.flatMap((m) =>
  (m.sub ?? [{ href: m.href, label: m.label, icon: m.icon }]).map((s) => ({
    group: m.label,
    label: s.label,
    href: s.href,
    Icon: s.icon ?? m.icon,
  })),
);

/* ------------------------------------------------------------------ */
/* Localisation: nav labels above are the English source; the shell   */
/* shows them in the reader's language through these keys.            */
/* ------------------------------------------------------------------ */

const MODULE_KEYS: Record<string, MessageKey> = {
  dashboard: "shell.nav.dashboard",
  accounts: "shell.nav.accounts",
  wallet: "shell.nav.wallet",
  portfolio: "shell.nav.portfolio",
  partner: "shell.nav.partner",
  social: "shell.nav.social",
  prop: "shell.nav.prop",
  rewards: "shell.nav.rewards",
  developer: "shell.nav.developer",
  academy: IS_DEMO ? "shell.nav.academy" : "shell.nav.academyLive",
  profile: "shell.nav.profileSecurity",
  support: "shell.nav.support",
  options: "options.nav.title",
};

const SUB_KEYS: Record<string, MessageKey> = {
  "/": "shell.nav.overview",
  "/markets": "shell.nav.markets",
  "/news": "shell.nav.news",
  "/calendar": "shell.nav.calendar",
  "/accounts": "shell.nav.myAccounts",
  "/accounts/new": "shell.nav.openAccount",
  "/wallet": "shell.nav.overview",
  "/wallet/deposit": "shell.nav.deposit",
  "/wallet/withdraw": "shell.nav.withdraw",
  "/wallet/transfer": "shell.nav.transfer",
  "/wallet/history": "shell.nav.history",
  "/portfolio": "shell.nav.overview",
  "/portfolio/analytics": "shell.nav.analytics",
  "/portfolio/history": "shell.nav.tradeHistory",
  "/portfolio/ledger": "shell.nav.ledger",
  "/portfolio/statements": "shell.nav.statements",
  "/partner": "shell.nav.dashboard",
  "/partner/clients": "shell.nav.clients",
  "/partner/network": "shell.nav.network",
  "/partner/commissions": "shell.nav.commissions",
  "/partner/links": "shell.nav.links",
  "/partner/payouts": "shell.nav.payouts",
  "/social": "shell.nav.discover",
  "/social/copy": "shell.nav.copyTrading",
  "/social/pamm": "shell.nav.pamm",
  "/social/investments": "shell.nav.investments",
  "/social/master": "shell.nav.becomeMaster",
  "/prop": "shell.nav.challenges",
  "/prop/mine": "shell.nav.myChallenges",
  "/prop/payouts": "shell.nav.payouts",
  "/prop/certificates": "shell.nav.certificates",
  "/rewards": "shell.nav.contests",
  "/rewards/leaderboard": "shell.nav.leaderboard",
  "/rewards/loyalty": "shell.nav.loyalty",
  "/rewards/cashback": "shell.nav.cashback",
  "/rewards/promotions": "shell.nav.promotions",
  "/developer": "shell.nav.apiKeys",
  "/developer/webhooks": "shell.nav.webhooks",
  "/developer/strategies": "shell.nav.strategyBuilder",
  "/developer/deployments": "shell.nav.runningStrategies",
  "/developer/backtests": "shell.nav.backtests",
  "/developer/marketplace": "shell.nav.marketplace",
  "/developer/docs": "shell.nav.docs",
  "/academy": "shell.nav.courses",
  "/academy/glossary": "shell.nav.glossary",
  "/academy/progress": "shell.nav.myProgress",
  "/academy/coach": "shell.nav.aiCoach",
  "/profile": "shell.nav.profile",
  "/profile/security": "shell.nav.security",
  "/profile/verification": "shell.nav.verification",
  "/profile/viewers": "shell.nav.viewers",
  "/profile/preferences": "shell.nav.preferences",
  "/profile/notifications": "shell.nav.notifications",
  "/social/managed": "shell.nav.managed",
  "/social/mam": "shell.nav.mamManager",
};

/** Navigation with labels in the reader's language (unknown entries keep their English label). */
export function localizeNav(modules: NavModule[], t: T): NavModule[] {
  return modules.map((m) => ({
    ...m,
    label: MODULE_KEYS[m.key] ? t(MODULE_KEYS[m.key]!) : m.label,
    sub: m.sub?.map((s) => ({ ...s, label: SUB_KEYS[s.href] ? t(SUB_KEYS[s.href]!) : s.label })),
  }));
}

/** ⌘K palette entries in the reader's language. */
export function localizeCommands(commands: typeof CRM_COMMANDS, t: T): typeof CRM_COMMANDS {
  const byHref = new Map(NAV.map((m) => [m.href, m.key]));
  return commands.map((c) => {
    const mod = NAV.find((m) => m.label === c.group);
    const group = mod && MODULE_KEYS[mod.key] ? t(MODULE_KEYS[mod.key]!) : c.group;
    const sub = SUB_KEYS[c.href] ?? (byHref.get(c.href) ? MODULE_KEYS[byHref.get(c.href)!] : undefined);
    return { ...c, group, label: sub ? t(sub) : c.label };
  });
}
