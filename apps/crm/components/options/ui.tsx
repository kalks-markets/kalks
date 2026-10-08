"use client";

// Kalks FX Options page (Client Area /options): what the product is, a friendly three-card intro (Buy a Call, Buy a
// Put, limited risk when you buy) and one step before the first trade: tick "I understand how options work" and press
// "Start trading options", which records the acceptance of the options terms and opens Kalks Trader in options mode.
// Verified identity and the knowledge quiz aren't needed (gateway suitability.rs); the Academy course stays as an
// optional "Test yourself". Rendered by the live page (gateway suitability via /api/suitability) and the demo page
// (local state) through the same OptionsController.

import * as React from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import {
  ArrowRight,
  ArrowUpRight,
  BookOpen,
  CalendarClock,
  CandlestickChart,
  ChevronDown,
  CircleCheck,
  Coins,
  GraduationCap,
  Layers2,
  Lightbulb,
  Loader2,
  Lock,
  RotateCcw,
  Scale,
  ScrollText,
  ShieldAlert,
  ShieldCheck,
  Sparkles,
  Timer,
  TrendingDown,
  TrendingUp,
  TriangleAlert,
  Wallet,
} from "lucide-react";
import { Button, Card, CardHeader, Chip, Dialog, Menu, PageHeader, Reveal, Skeleton, SymbolAvatar, cn } from "@/components/kit";
import { INSTRUMENT_MAP } from "@kalks/mock";
import { IS_DEMO } from "@kalks/mock/mode";
import type { MessageKey } from "@kalks/i18n";
import { useFormat, useT } from "@kalks/i18n/react";
import { Markdown } from "@/components/academy/live/markdown";
import type { Suitability } from "./api";

/** The Academy course on options (content/academy/en/phase-9, slug `phase-9`, with a quiz per chapter); demo builds
 *  have the mock Academy. */
export const OPTIONS_COURSE_HREF = IS_DEMO ? "/academy" : "/academy/phase/phase-9";

export type TradeAccount = { login: number; type: "live" | "demo"; name: string };

/** The open-account wizard on its Options path (CFD / Options account split). */
export const OPEN_OPTIONS_ACCOUNT = "/accounts/new?product=options";

/** What a page variant (live / demo) provides. Write methods resolve to false after showing their own toast. */
export type OptionsController = {
  data: Suitability | null;
  error: string | null;
  reload: () => void;
  /** Records the acceptance of the options terms (`version` = the current one). */
  accept: (version: number) => Promise<boolean>;
  /** The client's Options accounts (the only ones options trade on); null while loading. */
  accounts: TradeAccount[] | null;
  /** Plain Kalks Trader link (options mode) when the accounts can't be listed: the terminal signs in by itself. */
  traderHref?: string | null;
  /** Opens Kalks Trader in options mode on `a`. The tab opens inside the click; `before` runs next (false = cancel). */
  openTrader: (a: TradeAccount, before?: () => Promise<boolean>) => void | Promise<void>;
  /** View-only login or staff session: can read, never accept for the client. */
  readOnly: boolean;
  demo?: boolean;
};

/* ------------------------------------------------------------------ */
/* Underlyings                                                         */
/* ------------------------------------------------------------------ */

const UNDERLYINGS: { cls: "forex" | "metals" | "energies"; symbols: string[] }[] = [
  { cls: "forex", symbols: ["EURUSD", "GBPUSD", "USDJPY", "AUDUSD", "USDCAD", "USDCHF", "NZDUSD", "EURJPY", "GBPJPY"] },
  { cls: "metals", symbols: ["XAUUSD", "XAGUSD"] },
  { cls: "energies", symbols: ["USOIL", "UKOIL"] },
];

const CCY_FLAG: Record<string, string> = { EUR: "eu", GBP: "gb", USD: "us", JPY: "jp", AUD: "au", CAD: "ca", CHF: "ch", NZD: "nz" };

/** Symbol avatar, with a two-flag fallback for pairs the instrument list doesn't carry (e.g. NZDUSD). */
function UnderlyingAvatar({ symbol, size = 22 }: { symbol: string; size?: number }) {
  if (INSTRUMENT_MAP[symbol]) return <SymbolAvatar symbol={symbol} size={size} />;
  const base = CCY_FLAG[symbol.slice(0, 3)];
  const quote = CCY_FLAG[symbol.slice(3, 6)];
  if (!base || !quote) return <span className="inline-block shrink-0 rounded-full bg-surface-3" style={{ width: size, height: size }} />;
  return (
    <span className="relative inline-block shrink-0" style={{ width: size * 1.45, height: size }}>
      <span className={cn("fi fis absolute left-0 top-0 rounded-full ring-2 ring-surface", `fi-${base}`)} style={{ width: size, height: size }} />
      <span className={cn("fi fis absolute right-0 top-0 rounded-full ring-2 ring-surface", `fi-${quote}`)} style={{ width: size, height: size }} />
    </span>
  );
}

/* ------------------------------------------------------------------ */
/* Payoff sketches and the three ideas                                 */
/* ------------------------------------------------------------------ */

type Idea = "call" | "put" | "limited";

/** Result at expiry against the price at expiry: a bought call (gains as the price rises), a bought put (gains as it
 *  falls), or a bought call next to a sold one (dashed) to show that buying has a floor and selling doesn't. The
 *  shaded band is the price paid. Charts read left to right in every language. */
function PayoffSketch({ kind }: { kind: Idea }) {
  // zero line at y=34; price paid = 10 units; strike at x=70 (call) or x=90 (put)
  const put = kind === "put";
  const strike = put ? 90 : 70;
  const bought = put ? "4,4 90,44 156,44" : "4,44 70,44 156,4";
  return (
    <svg viewBox="0 0 160 68" className="h-[60px] w-full" aria-hidden>
      <line x1="4" y1="34" x2="156" y2="34" className="stroke-fg-3/50" strokeWidth="1" strokeDasharray="3 4" />
      <line x1={strike} y1="6" x2={strike} y2="62" className="stroke-fg-3/30" strokeWidth="1" />
      <rect x={put ? 90 : 4} y="34" width="66" height="10" className="fill-down/15" />
      {kind === "limited" && <polyline points="4,24 70,24 156,64" fill="none" strokeWidth="1.75" strokeDasharray="4 4" strokeLinejoin="round" strokeLinecap="round" className="stroke-down/60" />}
      <polyline points={bought} fill="none" strokeWidth="2.25" strokeLinejoin="round" strokeLinecap="round" className="stroke-up" />
    </svg>
  );
}

const IDEAS: { kind: Idea; tone: string; icon: React.ReactNode; title: MessageKey; text: MessageKey }[] = [
  { kind: "call", tone: "border-up/30 bg-up-soft text-up", icon: <TrendingUp />, title: "options.intro.call.title", text: "options.intro.call.text" },
  { kind: "put", tone: "border-down/30 bg-down-soft text-down", icon: <TrendingDown />, title: "options.intro.put.title", text: "options.intro.put.text" },
  { kind: "limited", tone: "border-ember/30 bg-ember-soft text-ember", icon: <ShieldCheck />, title: "options.intro.risk.title", text: "options.intro.risk.text" },
];

/** The three ideas (Buy a Call, Buy a Put, limited risk when you buy): also the Options step of the open-account wizard. */
export function IdeaCards() {
  const t = useT();
  return (
    <>
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
        {IDEAS.map((x) => (
          <div key={x.kind} className="flex flex-col rounded-[16px] border border-line bg-surface/60 px-4 pb-3 pt-3.5" data-testid={`options-idea-${x.kind}`}>
            <div className="flex items-center gap-2.5">
              <span className={cn("grid size-8 shrink-0 place-items-center rounded-full border [&_svg]:size-4", x.tone)}>{x.icon}</span>
              <div className="text-[15px] font-medium leading-tight tracking-tight text-fg">{t(x.title)}</div>
            </div>
            <p className="mt-2 text-[13px] leading-snug text-fg-2">{t(x.text)}</p>
            <div className="mt-auto pt-3">
              <PayoffSketch kind={x.kind} />
            </div>
          </div>
        ))}
      </div>
      <div className="mt-3 flex flex-wrap items-center gap-x-5 gap-y-1.5 text-[12px] text-fg-3">
        <span className="inline-flex items-center gap-2">
          <span className="h-[2.5px] w-4 rounded-full bg-up" aria-hidden />
          {t("options.intro.legend.result")}
        </span>
        <span className="inline-flex items-center gap-2">
          <span className="h-2.5 w-4 rounded-[3px] bg-down/20" aria-hidden />
          {t("options.intro.legend.cost")}
        </span>
      </div>
    </>
  );
}

/* ------------------------------------------------------------------ */
/* Kalks Trader button (options mode)                                  */
/* ------------------------------------------------------------------ */

/** Opens `href` in a new tab after `before` succeeds; the tab opens inside the click (popup blockers). */
async function openHrefAfter(href: string, before: () => Promise<boolean>) {
  const w = window.open("about:blank", "_blank");
  if (!(await before())) {
    w?.close();
    return;
  }
  if (w && !w.closed) {
    w.opener = null;
    w.location.replace(href);
  } else window.location.assign(href);
}

/** Opens Kalks Trader in options mode: one account directly, several through a menu, none -> open an account.
 *  With `before` (the first start) it runs first and must succeed. */
function TraderButton({
  ctl,
  size = "md",
  label,
  icon,
  before,
  disabled,
  className,
  testId = "options-trade",
}: {
  ctl: OptionsController;
  size?: "md" | "lg" | "xl";
  label?: string;
  icon?: React.ReactNode;
  before?: () => Promise<boolean>;
  disabled?: boolean;
  className?: string;
  testId?: string;
}) {
  const t = useT();
  const router = useRouter();
  const [busy, setBusy] = React.useState(false);
  const accounts = ctl.accounts;
  const text = label ?? t("options.trade.cta");
  const ico = busy ? <Loader2 className="animate-spin" /> : (icon ?? <CandlestickChart />);
  const run = async (fn: () => void | Promise<void>) => {
    setBusy(true);
    try {
      await fn();
    } finally {
      setBusy(false);
    }
  };
  const open = (a: TradeAccount) => run(() => ctl.openTrader(a, before));
  const off = disabled || busy;

  if (!accounts && ctl.traderHref) {
    const href = ctl.traderHref;
    if (before) {
      return (
        <Button variant="ember" size={size} disabled={off} onClick={() => run(() => openHrefAfter(href, before))} className={className} data-testid={testId}>
          {ico} {text} <ArrowUpRight className="rtl:-scale-x-100" />
        </Button>
      );
    }
    return (
      <a href={href} target="_blank" rel="noopener" className={className}>
        <Button variant="ember" size={size} className="w-full" data-testid={testId}>
          <CandlestickChart /> {text} <ArrowUpRight className="rtl:-scale-x-100" />
        </Button>
      </a>
    );
  }
  if (!accounts) {
    return (
      <Button variant="ember" size={size} disabled className={className}>
        <Loader2 className="animate-spin" /> {text}
      </Button>
    );
  }
  // CFD / Options account split: options trade in an Options account only; without one, open one
  if (accounts.length === 0) {
    if (before) {
      return (
        <Button variant="ember" size={size} disabled={off} onClick={() => run(async () => void ((await before()) && router.push(OPEN_OPTIONS_ACCOUNT)))} className={className} data-testid={testId}>
          {ico} {text} <ArrowRight className="rtl:-scale-x-100" />
        </Button>
      );
    }
    return (
      <Link href={OPEN_OPTIONS_ACCOUNT} className={className}>
        <Button variant="ember" size={size} className="w-full" data-testid="options-open-account">
          {t("options.account.open")} <ArrowRight className="rtl:-scale-x-100" />
        </Button>
      </Link>
    );
  }
  if (accounts.length === 1) {
    return (
      <Button variant="ember" size={size} disabled={off} onClick={() => open(accounts[0]!)} className={className} data-testid={testId}>
        {ico} {text}
      </Button>
    );
  }
  return (
    <Menu
      align="start"
      width={300}
      header={<div className="k-label">{t("options.trade.chooseAccount")}</div>}
      items={accounts.map((a) => ({
        label: (
          <span className="flex min-w-0 items-center gap-2">
            <span className="font-mono text-fg">#{a.login}</span>
            <span className="truncate text-fg-3">{a.name}</span>
          </span>
        ),
        icon: <CandlestickChart />,
        hint: (
          <Chip size="sm" tone={a.type === "live" ? "ember" : "gold"}>
            {t(a.type === "live" ? "options.trade.live" : "options.trade.demo")}
          </Chip>
        ),
        onSelect: () => void open(a),
      }))}
      trigger={
        <Button variant="ember" size={size} disabled={off} className={className} data-testid={testId}>
          {ico} {text} <ChevronDown />
        </Button>
      }
    />
  );
}

/* ------------------------------------------------------------------ */
/* Hero                                                                */
/* ------------------------------------------------------------------ */

function Feature({ icon, title, text }: { icon: React.ReactNode; title: string; text: string }) {
  return (
    <div className="rounded-[16px] border border-line bg-surface/60 px-4 py-3.5 backdrop-blur-sm">
      <div className="flex items-center gap-2.5">
        <span className="grid size-8 shrink-0 place-items-center rounded-full border border-ember/30 bg-ember-soft text-ember [&_svg]:size-4">{icon}</span>
        <div className="text-[14px] font-medium text-fg">{title}</div>
      </div>
      <p className="mt-2 text-[12.5px] leading-snug text-fg-2">{text}</p>
    </div>
  );
}

function Hero({ ctl, onStart, onHowItWorks }: { ctl: OptionsController; onStart: () => void; onHowItWorks: () => void }) {
  const t = useT();
  const data = ctl.data;
  return (
    <Card hot className="overflow-hidden">
      <div className="grid grid-cols-1 gap-6 px-5 py-6 sm:px-7 sm:py-7 lg:grid-cols-[1.25fr_1fr]">
        <div className="min-w-0">
          <Chip tone="ember" size="sm" className="font-semibold uppercase tracking-wider">
            {t("options.hero.eyebrow")}
          </Chip>
          <h2 className="mt-3 max-w-[560px] text-[24px] font-medium leading-[1.15] tracking-[-0.02em] text-fg sm:text-[30px]">{t("options.hero.title")}</h2>
          <p className="mt-3 max-w-[560px] text-[14.5px] leading-relaxed text-fg-2">{t("options.hero.text")}</p>
          {data?.eligible ? (
            <>
              <div className="mt-6 flex flex-wrap items-center gap-x-4 gap-y-3">
                <TraderButton ctl={ctl} size="lg" />
                <button type="button" onClick={onHowItWorks} className="inline-flex items-center gap-1.5 text-[13.5px] font-medium text-fg-2 underline-offset-4 hover:text-fg hover:underline" data-testid="options-how">
                  <Lightbulb className="size-4" /> {t("options.hero.howItWorks")}
                </button>
              </div>
              <p className="mt-3 flex max-w-[560px] items-start gap-2 text-[13px] leading-snug text-fg-2">
                <CircleCheck className="mt-px size-4 shrink-0 text-up" />
                {t(ctl.accounts && ctl.accounts.length === 0 ? "options.trade.noAccount" : "options.trade.ready")}
              </p>
            </>
          ) : (
            <div className="mt-6 flex flex-wrap items-center gap-2">
              {data ? (
                <Button variant="ember" size="lg" onClick={onStart} data-testid="options-get-started">
                  {t("options.hero.start")} <ArrowRight className="rtl:-scale-x-100" />
                </Button>
              ) : (
                !ctl.error && <Skeleton className="h-11 w-44 rounded-full" />
              )}
              <Link href={OPTIONS_COURSE_HREF}>
                <Button variant="surface" size="lg">
                  <GraduationCap /> {t("options.page.learnCourse")}
                </Button>
              </Link>
            </div>
          )}
        </div>

        <div className="min-w-0 space-y-3">
          {UNDERLYINGS.map((g) => (
            <div key={g.cls} className="rounded-[16px] border border-line bg-surface/60 px-4 py-3">
              <div className="k-label mb-2.5">{t(`options.hero.class.${g.cls}` as MessageKey)}</div>
              <div className="flex flex-wrap gap-2">
                {g.symbols.map((s) => (
                  <span key={s} className="inline-flex items-center gap-2 rounded-full border border-line bg-surface-2 py-1 pe-3 ps-1.5 text-[12.5px] font-medium text-fg">
                    <UnderlyingAvatar symbol={s} size={20} />
                    <span className="font-mono">{s}</span>
                  </span>
                ))}
              </div>
            </div>
          ))}
        </div>
      </div>
      <div className="grid grid-cols-1 gap-3 border-t border-line/70 px-5 py-5 sm:grid-cols-2 sm:px-7 xl:grid-cols-4">
        <Feature icon={<Layers2 />} title={t("options.hero.feature.underlyings.title")} text={t("options.hero.feature.underlyings.text")} />
        <Feature icon={<CalendarClock />} title={t("options.hero.feature.expiries.title")} text={t("options.hero.feature.expiries.text")} />
        <Feature icon={<Coins />} title={t("options.hero.feature.settlement.title")} text={t("options.hero.feature.settlement.text")} />
        <Feature icon={<Scale />} title={t("options.hero.feature.sides.title")} text={t("options.hero.feature.sides.text")} />
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Friendly intro: three ideas, one checkbox, Start trading options    */
/* ------------------------------------------------------------------ */

function IntroCard({ ctl, data, onTerms }: { ctl: OptionsController; data: Suitability; onTerms: () => void }) {
  const t = useT();
  const [understood, setUnderstood] = React.useState(false);
  const d = data.disclosure;
  const start = () => (d ? ctl.accept(d.version) : Promise.resolve(false));

  return (
    <Card id="get-started" className="scroll-mt-24" data-testid="options-intro">
      <CardHeader
        title={t("options.intro.title")}
        subtitle={t("options.intro.subtitle")}
        icon={<Sparkles />}
        action={
          <Link href={OPTIONS_COURSE_HREF} data-testid="options-quiz-link">
            <Button size="sm" variant="ghost">
              <GraduationCap /> {t("options.intro.quiz")}
            </Button>
          </Link>
        }
      />
      <div className="px-4 pb-6 pt-4 sm:px-6">
        <IdeaCards />
        <div className="mt-5 rounded-[18px] border border-line bg-surface-2/50 px-4 py-4 sm:px-5">
          {ctl.readOnly && (
            <div className="mb-4 flex items-start gap-2.5 rounded-[14px] border border-warn/30 bg-warn-soft px-4 py-3 text-[13px] text-fg-2">
              <Lock className="mt-0.5 size-4 shrink-0 text-warn" />
              {t("options.readOnly")}
            </div>
          )}
          <div className="flex flex-col gap-4 md:flex-row md:items-center md:justify-between">
            <div className="min-w-0">
              <label className={cn("flex items-center gap-3 text-[15px] font-medium text-fg", ctl.readOnly || !d ? "cursor-not-allowed opacity-60" : "cursor-pointer")}>
                <input
                  type="checkbox"
                  className="size-5 shrink-0 accent-[var(--k-ember)]"
                  checked={understood}
                  disabled={ctl.readOnly || !d}
                  onChange={(e) => setUnderstood(e.target.checked)}
                  data-testid="options-understand"
                />
                {t("options.intro.confirm")}
              </label>
              <p className="mt-1.5 ps-8 text-[12.5px] leading-snug text-fg-3">
                {d ? (
                  <>
                    {t("options.intro.consent")}{" "}
                    <button type="button" onClick={onTerms} className="inline-flex items-center gap-1 font-medium text-ember underline-offset-4 hover:underline" data-testid="options-terms-open">
                      <ScrollText className="size-3.5" /> {t("options.intro.terms")}
                    </button>
                  </>
                ) : (
                  t("options.terms.unavailable")
                )}
              </p>
            </div>
            <TraderButton
              ctl={ctl}
              size="xl"
              label={t("options.intro.start")}
              icon={<CandlestickChart />}
              before={start}
              disabled={!understood || !d || ctl.readOnly}
              className="w-full md:w-auto md:shrink-0"
              testId="options-start"
            />
          </div>
        </div>
      </div>
    </Card>
  );
}

/** For clients who already trade options: the three ideas again, the full terms and the optional quiz. */
function HowItWorksDialog({ data, open, onOpenChange, onTerms }: { data: Suitability; open: boolean; onOpenChange: (o: boolean) => void; onTerms: () => void }) {
  const t = useT();
  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      width={820}
      title={t("options.hero.howItWorks")}
      footer={
        <div className="flex w-full flex-wrap items-center justify-between gap-3">
          <div className="flex flex-wrap items-center gap-x-5 gap-y-2 text-[13px]">
            {data.disclosure && (
              <button type="button" onClick={onTerms} className="inline-flex items-center gap-1.5 font-medium text-fg-2 underline-offset-4 hover:text-fg hover:underline">
                <ScrollText className="size-4" /> {t("options.intro.terms")}
              </button>
            )}
            <Link href={OPTIONS_COURSE_HREF} className="inline-flex items-center gap-1.5 font-medium text-fg-2 underline-offset-4 hover:text-fg hover:underline">
              <GraduationCap className="size-4" /> {t("options.intro.quiz")}
            </Link>
          </div>
          <Button variant="ember" onClick={() => onOpenChange(false)}>
            {t("options.intro.gotIt")}
          </Button>
        </div>
      }
    >
      <IdeaCards />
    </Dialog>
  );
}

/* ------------------------------------------------------------------ */
/* Full terms                                                          */
/* ------------------------------------------------------------------ */

const KEY_POINTS: { key: MessageKey; icon: React.ReactNode }[] = [
  { key: "options.terms.point.buy", icon: <ShieldCheck /> },
  { key: "options.terms.point.sell", icon: <Scale /> },
  { key: "options.terms.point.prices", icon: <BookOpen /> },
  { key: "options.terms.point.settle", icon: <Coins /> },
];

/** The options terms as published (English, binding). Other languages get the key points translated on top. */
export function TermsDialog({ data, open, onOpenChange }: { data: Suitability; open: boolean; onOpenChange: (o: boolean) => void }) {
  const t = useT();
  const f = useFormat();
  const d = data.disclosure!;
  const en = t.locale === "en";
  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      width={760}
      title={t("options.terms.title")}
      description={t("options.terms.version", { version: d.version, date: f.date(d.publishedAt) })}
      footer={
        <div className="flex w-full flex-wrap items-center justify-between gap-3">
          {data.disclosureAccepted && data.acceptedVersion && data.acceptedAt ? (
            <span className="flex items-center gap-2 text-[13px] text-up">
              <CircleCheck className="size-4" /> {t("options.terms.acceptedOn", { version: data.acceptedVersion, date: f.date(data.acceptedAt) })}
            </span>
          ) : (
            <span />
          )}
          <Button variant="surface" onClick={() => onOpenChange(false)}>
            {t("options.terms.close")}
          </Button>
        </div>
      }
    >
      {!en && (
        <div className="mb-6 rounded-[16px] border border-line bg-surface-2/60 px-4 py-3.5">
          <div className="k-label mb-2.5">{t("options.terms.inShort")}</div>
          <ul className="space-y-2">
            {KEY_POINTS.map((p) => (
              <li key={p.key} className="flex items-start gap-2.5 text-[13.5px] leading-snug text-fg">
                <span className="mt-0.5 shrink-0 text-ember [&_svg]:size-4">{p.icon}</span>
                {t(p.key)}
              </li>
            ))}
          </ul>
        </div>
      )}
      <div className="mb-3 flex flex-wrap items-baseline justify-between gap-2 border-b border-line pb-3">
        <h3 className="text-[16px] font-medium tracking-tight text-fg">{d.title}</h3>
        {!en && <span className="text-[12px] text-fg-3">{t("options.terms.englishNote")}</span>}
      </div>
      <div dir="ltr" lang="en">
        <Markdown src={d.bodyMd} className="text-[14px] leading-[1.7] [&_h2]:mt-7 [&_h2]:text-[17px] [&_h3]:mt-5 [&_h3]:text-[15px]" />
      </div>
    </Dialog>
  );
}

/* ------------------------------------------------------------------ */
/* Side cards                                                          */
/* ------------------------------------------------------------------ */

const FACTS: { key: MessageKey; icon: React.ReactNode }[] = [
  { key: "options.facts.style", icon: <CalendarClock /> },
  { key: "options.facts.premium", icon: <Coins /> },
  { key: "options.facts.contracts", icon: <Layers2 /> },
  { key: "options.facts.close", icon: <ArrowUpRight /> },
  { key: "options.facts.cutoff", icon: <Timer /> },
  { key: "options.facts.margin", icon: <ShieldAlert /> },
  { key: "options.trade.cashOnly", icon: <Wallet /> },
];

function FactsCard() {
  const t = useT();
  return (
    <Card>
      <CardHeader title={t("options.facts.title")} icon={<Scale />} />
      <ul className="space-y-2.5 px-4 pb-5 pt-4 sm:px-6">
        {FACTS.map((x) => (
          <li key={x.key} className="flex items-start gap-3 text-[13px] leading-snug text-fg-2">
            <span className="mt-px grid size-6 shrink-0 place-items-center rounded-full border border-line bg-surface-2 text-fg-3 [&_svg]:size-3.5">{x.icon}</span>
            <span>{t(x.key)}</span>
          </li>
        ))}
      </ul>
    </Card>
  );
}

function LearnCard() {
  const t = useT();
  return (
    <Card>
      <div className="flex items-start gap-3.5 px-5 py-5 sm:px-6">
        <span className="grid size-10 shrink-0 place-items-center rounded-full border border-gold/30 bg-gold-soft text-gold">
          <GraduationCap className="size-5" />
        </span>
        <div className="min-w-0">
          <div className="text-[15px] font-medium text-fg">{t("options.learn.title")}</div>
          <p className="mt-1 text-[13px] leading-snug text-fg-2">{t("options.learn.text")}</p>
          <Link href={OPTIONS_COURSE_HREF} className="mt-3 inline-block">
            <Button size="sm" variant="surface">
              {t("options.learn.cta")} <ArrowRight className="rtl:-scale-x-100" />
            </Button>
          </Link>
        </div>
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Page                                                                */
/* ------------------------------------------------------------------ */

function IntroSkeleton() {
  return (
    <Card className="px-6 py-6">
      <Skeleton className="h-5 w-56" />
      <Skeleton className="mt-2 h-4 w-72" />
      <div className="mt-6 grid grid-cols-1 gap-3 sm:grid-cols-3">
        {[0, 1, 2].map((i) => (
          <Skeleton key={i} className="h-[150px] w-full rounded-[16px]" />
        ))}
      </div>
      <Skeleton className="mt-5 h-[76px] w-full rounded-[18px]" />
    </Card>
  );
}

export function OptionsPage({ ctl }: { ctl: OptionsController }) {
  const t = useT();
  const data = ctl.data;
  const eligible = !!data?.eligible;
  const [howOpen, setHowOpen] = React.useState(false);
  const [termsOpen, setTermsOpen] = React.useState(false);
  const start = () => document.getElementById("get-started")?.scrollIntoView({ behavior: "smooth", block: "start" });

  return (
    <>
      <PageHeader
        title={t("options.page.title")}
        subtitle={t("options.page.subtitle")}
        actions={
          <>
            {ctl.demo && (
              <Chip tone="gold" size="sm">
                {t("options.demo.note")}
              </Chip>
            )}
            {eligible && (
              <Chip tone="up" dot>
                {t("options.page.statusReady")}
              </Chip>
            )}
          </>
        }
      />
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-3">
        <Reveal className="xl:col-span-3">
          <Hero ctl={ctl} onStart={start} onHowItWorks={() => setHowOpen(true)} />
        </Reveal>
        {!eligible && (
          <Reveal delay={0.06} className="min-w-0 xl:col-span-2">
            {data ? (
              <IntroCard ctl={ctl} data={data} onTerms={() => setTermsOpen(true)} />
            ) : ctl.error ? (
              <Card className="flex flex-col items-center gap-3 px-6 py-10 text-center">
                <TriangleAlert className="size-6 text-warn" />
                <div className="text-[14px] text-fg-2">{ctl.error}</div>
                <Button variant="surface" size="sm" onClick={ctl.reload}>
                  <RotateCcw /> {t("options.error.retry")}
                </Button>
              </Card>
            ) : (
              <IntroSkeleton />
            )}
          </Reveal>
        )}
        <Reveal delay={0.1} className={cn("min-w-0", eligible ? "grid grid-cols-1 items-start gap-4 lg:grid-cols-2 xl:col-span-3" : "space-y-4")}>
          <FactsCard />
          <LearnCard />
        </Reveal>
      </div>
      {data?.disclosure && <TermsDialog data={data} open={termsOpen} onOpenChange={setTermsOpen} />}
      {data && (
        <HowItWorksDialog
          data={data}
          open={howOpen}
          onOpenChange={setHowOpen}
          onTerms={() => {
            setHowOpen(false);
            setTermsOpen(true);
          }}
        />
      )}
    </>
  );
}
