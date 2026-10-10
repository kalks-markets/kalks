"use client";

import * as React from "react";
import Link from "next/link";
import { ArrowRight, Ban, CalendarDays, Check, Clock, Gauge as GaugeIcon, Layers, Loader2, Minus, Percent, ShieldCheck, Target, TrendingDown, Trophy, Wallet, Zap } from "lucide-react";
import { Button, Card, CardHeader, Chip, Dialog, EmptyState, PageHeader, Reveal, Segmented, Skeleton, cn } from "@/components/kit";
import type { T } from "@kalks/i18n";
import { useT } from "@kalks/i18n/react";
import {
  bannedLabel,
  payoutFreqLabel,
  propApi,
  sizeLabel,
  typeLabel,
  usd,
  usePropPoll,
  type Challenge,
  type Plan,
  type PlanSize,
  type PlanType,
  type PurchaseResult,
} from "./api";
import { CredentialField, ErrorNote, LoadError, PropTradeButton } from "./ui";
import { fmt as fmtUsdt, usdtAvailable, useWallet, type Overview } from "@/components/wallet-live/api";
import { PageHero } from "@/components/page-hero";

const TYPE_ORDER: PlanType[] = ["1-step", "2-step", "instant"];
const TYPE_ICON: Record<PlanType, React.ReactNode> = {
  "1-step": <Target className="size-3.5" />,
  "2-step": <Layers className="size-3.5" />,
  instant: <Zap className="size-3.5" />,
};
const TYPE_TEXT = {
  "1-step": "prop.store.typeText.oneStep",
  "2-step": "prop.store.typeText.twoStep",
  instant: "prop.store.typeText.instant",
} as const satisfies Record<PlanType, string>;

/* ------------------------------------------------------------------ */
/* Rule text                                                           */
/* ------------------------------------------------------------------ */

const days = (t: T, n: number) => t("prop.days", { count: n });
/** "balance" / "equity" (daily loss basis) and "static" / "trailing" (drawdown type) from the plan. */
const basisLabel = (t: T, b: string) => t.dyn(`prop.basis.${b}`, b);
const ddTypeLabel = (t: T, d: string) => t.dyn(`prop.ddType.${d}`, d);

export function targetsText(t: T, p: Plan) {
  return p.phases.length ? p.phases.map((x) => `${x.target}%`).join(" / ") : t("prop.none");
}

export function ddText(t: T, p: Plan) {
  const base = `${p.maxDD}% ${ddTypeLabel(t, p.ddType)}`;
  return p.ddType === "trailing" && p.trailingLock ? t("prop.rules.ddLocks", { dd: base }) : base;
}

/** Every rule of a plan at one size, as label / value rows. */
export function planRules(t: T, p: Plan, s: PlanSize): [string, React.ReactNode][] {
  const rows: [string, React.ReactNode][] = [];
  for (const ph of p.phases) {
    rows.push([t("prop.rules.phaseTarget", { phase: ph.name }), `${ph.target}% · ${usd((s.size * ph.target) / 100, 0)}`]);
    rows.push([t("prop.rules.phaseMinDays", { phase: ph.name }), days(t, ph.minDays)]);
    rows.push([t("prop.rules.phaseTimeLimit", { phase: ph.name }), ph.timeLimit ? days(t, ph.timeLimit) : t("prop.noTimeLimit")]);
  }
  if (!p.phases.length) rows.push([t("prop.rules.evaluation"), t("prop.rules.evaluationNone")]);
  rows.push([t("prop.dailyLossLimit"), t(p.dailyBasis === "equity" ? "prop.rules.dailyLossEquity" : "prop.rules.dailyLossBalance", { pct: p.dailyLoss, amount: usd((s.size * p.dailyLoss) / 100, 0) })]);
  rows.push([t("prop.rule.maxDrawdown"), `${ddText(t, p)} · ${usd((s.size * p.maxDD) / 100, 0)}`]);
  rows.push([t("prop.rule.consistency"), p.consistency > 0 ? t("prop.rules.consistencyValue", { pct: p.consistency }) : t("prop.rules.noConsistency")]);
  rows.push([t("prop.newsTrading"), p.newsTrading ? t("prop.allowed") : t(p.newsBreachFails ? "prop.rules.newsBlockedFails" : "prop.rules.newsBlocked", { min: p.newsWindow })]);
  rows.push([t("prop.rule.weekendHolding"), p.weekendHolding ? t("prop.allowed") : t("prop.rules.weekendClosed")]);
  rows.push([t("prop.expertAdvisors"), p.eaAllowed ? t("prop.allowed") : t("prop.notAllowed")]);
  rows.push([t("prop.bannedStrategies"), p.banned.length ? p.banned.map(bannedLabel).join(", ") : t("prop.none")]);
  rows.push([t("prop.profitSplit"), p.splitMax > p.split ? t("prop.rules.splitScaling", { split: p.split, max: p.splitMax }) : `${p.split}%`]);
  rows.push([t("prop.firstPayout"), t("prop.rules.firstPayoutValue", { days: days(t, p.firstPayoutDays), freq: payoutFreqLabel(p.payoutFreq), min: usd(p.minPayout, 0) })]);
  rows.push([t("prop.feeRefund"), p.refundFee ? t("prop.rules.refunded") : t("prop.nonRefundable")]);
  rows.push([t("prop.leverage"), `1:${s.leverage}`]);
  return rows;
}

/* ------------------------------------------------------------------ */
/* Phase track                                                         */
/* ------------------------------------------------------------------ */

function PhaseTrack({ p }: { p: Plan }) {
  const t = useT();
  const steps = [
    ...p.phases.map((x) => ({ name: x.name, title: t("prop.track.target", { pct: x.target }), sub: x.timeLimit ? t("prop.track.minMax", { min: days(t, x.minDays), max: days(t, x.timeLimit) }) : t("prop.track.min", { min: days(t, x.minDays) }), funded: false })),
    { name: t("prop.status.funded"), title: t("prop.track.split", { pct: p.split }), sub: t("prop.track.firstPayout", { days: days(t, p.firstPayoutDays) }), funded: true },
  ];
  return (
    <div className="flex flex-col items-stretch gap-2 sm:flex-row">
      {steps.map((s, i) => (
        <React.Fragment key={s.name}>
          <div className={cn("k-row flex-1 px-3.5 py-3", s.funded && "border-gold/30 bg-gold-soft")}>
            <div className="flex items-center gap-2 text-[11px] text-fg-3">
              <span className={cn("grid size-5 place-items-center rounded-full text-[10px] font-semibold", s.funded ? "bg-gold text-[#1a1204]" : "bg-surface-3 text-fg-2")}>{i + 1}</span>
              {s.name}
            </div>
            <div className="k-num mt-1.5 text-[15px] font-semibold">{s.title}</div>
            <div className="mt-0.5 text-[11px] text-fg-3">{s.sub}</div>
          </div>
          {i < steps.length - 1 && (
            <div className="hidden items-center sm:flex">
              <ArrowRight className="size-4 text-fg-3 rtl:-scale-x-100" />
            </div>
          )}
        </React.Fragment>
      ))}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Configurator + plan card                                            */
/* ------------------------------------------------------------------ */

function Configurator({ plans, plan, setPlan, size, setSize }: { plans: Plan[]; plan: Plan; setPlan: (id: string) => void; size: PlanSize; setSize: (n: number) => void }) {
  const t = useT();
  const types = TYPE_ORDER.filter((x) => plans.some((p) => p.type === x));
  const sameType = plans.filter((p) => p.type === plan.type);
  return (
    <Card className="h-full">
      <CardHeader title={t("prop.store.chooseTitle")} subtitle={t("prop.store.chooseSubtitle")} icon={<Trophy />} />
      <div className="space-y-5 px-4 pb-6 pt-5 sm:px-6">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <Segmented
            size="md"
            value={plan.type}
            onChange={(x) => setPlan(plans.find((p) => p.type === x)!.id)}
            options={types.map((x) => ({ value: x, label: <>{TYPE_ICON[x]}{typeLabel(x)}</> }))}
          />
          {sameType.length > 1 && (
            // plan names stay on one line; on narrow screens the row scrolls instead of wrapping inside the pills
            <div className="-mx-1 max-w-full overflow-x-auto px-1 [scrollbar-width:none]">
              <Segmented size="xs" className="whitespace-nowrap" value={plan.id} onChange={setPlan} options={sameType.map((p) => ({ value: p.id, label: p.name }))} />
            </div>
          )}
        </div>
        <p className="text-[13.5px] text-fg-2">{TYPE_TEXT[plan.type] ? t(TYPE_TEXT[plan.type]) : null}</p>

        <div>
          <div className="k-label mb-2.5">{t("prop.accountSize")}</div>
          <div className="grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-6">
            {plan.sizes.map((s) => {
              const on = s.size === size.size;
              return (
                <button
                  key={s.size}
                  type="button"
                  onClick={() => setSize(s.size)}
                  aria-pressed={on}
                  className={cn("rounded-[14px] border px-3 py-3 text-start transition-colors", on ? "border-ember/50 bg-ember-soft" : "border-line bg-surface-2 hover:border-fg-3/40 hover:bg-surface-3")}
                >
                  <div className="k-num text-[17px] font-semibold tracking-tight">{sizeLabel(s.size)}</div>
                  <div className={cn("k-num mt-0.5 text-[11.5px]", on ? "text-ember" : "text-fg-3")}>{usd(s.fee, s.fee % 1 ? 2 : 0)}</div>
                </button>
              );
            })}
          </div>
        </div>

        <div>
          <div className="k-label mb-2.5">{t("prop.store.yourPath")}</div>
          <PhaseTrack p={plan} />
        </div>

        <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
          {[
            { i: <TrendingDown />, l: t("prop.rule.dailyLoss"), v: `${plan.dailyLoss}%`, s: `${usd((size.size * plan.dailyLoss) / 100, 0)} · ${basisLabel(t, plan.dailyBasis)}` },
            { i: <GaugeIcon />, l: t("prop.rule.maxDrawdown"), v: `${plan.maxDD}%`, s: `${ddTypeLabel(t, plan.ddType)} · ${usd((size.size * plan.maxDD) / 100, 0)}` },
            { i: <CalendarDays />, l: t("prop.minDays"), v: plan.phases.length ? String(Math.max(...plan.phases.map((p) => p.minDays))) : "—", s: plan.phases.some((p) => p.timeLimit) ? t("prop.store.timeLimitApplies") : t("prop.noTimeLimit") },
            { i: <Percent />, l: t("prop.profitSplit"), v: `${plan.split}%`, s: plan.splitMax > plan.split ? t("prop.store.scalesTo", { pct: plan.splitMax }) : t("prop.store.fixed") },
          ].map((x) => (
            <div key={x.l} className="k-row px-3.5 py-3">
              <div className="flex items-center gap-1.5 text-[11px] text-fg-3 [&_svg]:size-3.5">
                {x.i}
                {x.l}
              </div>
              <div className="k-num mt-1 text-[18px] font-semibold">{x.v}</div>
              <div className="truncate text-[11px] text-fg-3">{x.s}</div>
            </div>
          ))}
        </div>
      </div>
    </Card>
  );
}

function PlanCard({ plan, size, onBuy }: { plan: Plan; size: PlanSize; onBuy: () => void }) {
  const t = useT();
  const rows: [string, React.ReactNode][] = [
    ...plan.phases.map((p): [string, React.ReactNode] => [t("prop.rules.phaseTarget", { phase: p.name }), `${p.target}% · ${usd((size.size * p.target) / 100, 0)}`]),
    [t("prop.dailyLossLimit"), `${plan.dailyLoss}% · ${basisLabel(t, plan.dailyBasis)}`],
    [t("prop.rule.maxDrawdown"), `${plan.maxDD}% ${ddTypeLabel(t, plan.ddType)}`],
    [t("prop.leverage"), `1:${size.leverage}`],
    [t("prop.profitSplit"), plan.splitMax > plan.split ? `${plan.split}% → ${plan.splitMax}%` : `${plan.split}%`],
    [t("prop.feeRefund"), plan.refundFee ? <span className="text-up">{t("prop.store.withFirstPayout")}</span> : <span className="text-fg-3">{t("prop.nonRefundable")}</span>],
  ];
  return (
    <Card className="flex h-full flex-col">
      <div className="flex flex-1 flex-col p-6">
        <div className="k-label">{plan.name}</div>
        <div className="mt-1 text-[13px] text-fg-2">{t("prop.store.simulatedAccount", { size: sizeLabel(size.size) })}</div>
        <div className="mt-3 flex items-baseline gap-2">
          <span className="k-num text-[44px] font-semibold leading-none tracking-[-0.03em]">{usd(size.fee, size.fee % 1 ? 2 : 0)}</span>
          <span className="text-[13px] text-fg-3">{t("prop.store.oneTimeUsdt")}</span>
        </div>
        <div className="mt-2 flex flex-wrap gap-1.5">
          {plan.refundFee && (
            <Chip size="sm" tone="gold">
              {t("prop.store.refundable")}
            </Chip>
          )}
          <Chip size="sm">{t("prop.store.leverageChip", { leverage: size.leverage })}</Chip>
          <Chip size="sm">{plan.phases.some((p) => p.timeLimit) ? t("prop.rule.timeLimit") : t("prop.noTimeLimit")}</Chip>
        </div>
        <dl className="mt-5 divide-y divide-line text-[13px]">
          {rows.map(([k, v]) => (
            <div key={k} className="flex items-center justify-between gap-3 py-2.5">
              <dt className="text-fg-2">{k}</dt>
              <dd className="k-num text-end font-medium">{v}</dd>
            </div>
          ))}
        </dl>
        <div className="mt-auto pt-5">
          <Button variant="ember" size="xl" className="w-full" onClick={onBuy}>
            {t("prop.store.buyChallenge", { fee: usd(size.fee, size.fee % 1 ? 2 : 0) })} <ArrowRight className="rtl:-scale-x-100" />
          </Button>
          <div className="mt-2.5 flex items-center justify-center gap-1.5 text-[11.5px] text-fg-3">
            <Wallet className="size-3.5" /> {t("prop.store.paidFromWallet")}
          </div>
        </div>
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Compare table                                                       */
/* ------------------------------------------------------------------ */

function Yes({ children }: { children?: React.ReactNode }) {
  return (
    <span className="inline-flex items-center gap-1.5 text-fg">
      <Check className="size-3.5 text-up" />
      {children}
    </span>
  );
}

function No({ children }: { children?: React.ReactNode }) {
  return (
    <span className="inline-flex items-center gap-1.5 text-fg-3">
      <Minus className="size-3.5" />
      {children}
    </span>
  );
}

function CompareTable({ plans, current, size, onPick }: { plans: Plan[]; current: string; size: number; onPick: (id: string) => void }) {
  const t = useT();
  const feeAt = (p: Plan) => p.sizes.find((s) => s.size === size) ?? null;
  const rows: { label: string; icon: React.ReactNode; cell: (p: Plan) => React.ReactNode }[] = [
    { label: t("prop.compare.fee", { size: sizeLabel(size) }), icon: <Wallet />, cell: (p) => (feeAt(p) ? <span className="k-num font-semibold">{usd(feeAt(p)!.fee, 0)}</span> : <span className="text-fg-3">{t("prop.compare.notOffered")}</span>) },
    { label: t("prop.rule.profitTarget"), icon: <Target />, cell: (p) => targetsText(t, p) },
    { label: t("prop.dailyLossLimit"), icon: <TrendingDown />, cell: (p) => <span>{p.dailyLoss}% <span className="text-fg-3">· {basisLabel(t, p.dailyBasis)}</span></span> },
    { label: t("prop.rule.maxDrawdown"), icon: <GaugeIcon />, cell: (p) => <span>{p.maxDD}% <span className="text-fg-3">· {ddTypeLabel(t, p.ddType)}</span></span> },
    { label: t("prop.compare.minTradingDays"), icon: <CalendarDays />, cell: (p) => (p.phases.length ? p.phases.map((x) => x.minDays).join(" / ") : "—") },
    { label: t("prop.rule.timeLimit"), icon: <Clock />, cell: (p) => (p.phases.some((x) => x.timeLimit) ? p.phases.map((x) => (x.timeLimit ? days(t, x.timeLimit) : t("prop.compare.noneLower"))).join(" / ") : t("prop.none")) },
    { label: t("prop.leverage"), icon: <Layers />, cell: (p) => (feeAt(p) ? `1:${feeAt(p)!.leverage}` : `1:${p.sizes[0]?.leverage ?? "—"}`) },
    { label: t("prop.profitSplit"), icon: <Percent />, cell: (p) => (p.splitMax > p.split ? `${p.split}% → ${p.splitMax}%` : `${p.split}%`) },
    { label: t("prop.feeRefund"), icon: <Check />, cell: (p) => (p.refundFee ? <Yes>{t("prop.compare.firstPayout")}</Yes> : <No>{t("prop.no")}</No>) },
    { label: t("prop.compare.consistencyRule"), icon: <ShieldCheck />, cell: (p) => (p.consistency > 0 ? t("prop.compare.bestDay", { pct: p.consistency }) : <No>{t("prop.none")}</No>) },
    { label: t("prop.newsTrading"), icon: <Zap />, cell: (p) => (p.newsTrading ? <Yes>{t("prop.allowed")}</Yes> : <No>{t("prop.compare.newsBlocked", { min: p.newsWindow })}</No>) },
    { label: t("prop.rule.weekendHolding"), icon: <CalendarDays />, cell: (p) => (p.weekendHolding ? <Yes>{t("prop.allowed")}</Yes> : <No>{t("prop.compare.closedFriday")}</No>) },
    { label: t("prop.expertAdvisors"), icon: <Layers />, cell: (p) => (p.eaAllowed ? <Yes>{t("prop.allowed")}</Yes> : <No>{t("prop.notAllowed")}</No>) },
    { label: t("prop.bannedStrategies"), icon: <Ban />, cell: (p) => <span className="text-[12px] text-fg-2">{p.banned.length ? p.banned.map(bannedLabel).join(" · ") : t("prop.none")}</span> },
    { label: t("prop.firstPayout"), icon: <Clock />, cell: (p) => t("prop.compare.firstPayoutValue", { days: days(t, p.firstPayoutDays), freq: payoutFreqLabel(p.payoutFreq) }) },
  ];
  return (
    <Card>
      <CardHeader title={t("prop.compare.title")} subtitle={t("prop.compare.subtitle")} icon={<Layers />} />
      <div className="overflow-x-auto px-4 pb-6 pt-4 sm:px-6">
        <table className="w-full border-separate border-spacing-0 text-[13px]" style={{ minWidth: 220 + plans.length * 220 }}>
          <thead>
            <tr>
              <th className="w-[22%] pb-3 text-start text-[11.5px] font-medium uppercase tracking-wider text-fg-3">{t("prop.compare.rule")}</th>
              {plans.map((p) => {
                const on = p.id === current;
                const from = Math.min(...p.sizes.map((s) => s.fee));
                return (
                  <th key={p.id} className="px-1.5 pb-3 align-bottom">
                    <button
                      type="button"
                      onClick={() => onPick(p.id)}
                      className={cn("flex w-full items-center justify-between gap-2 rounded-t-[16px] border border-b-0 px-4 py-3 text-start transition-colors", on ? "border-ember/40 bg-ember-soft" : "border-line bg-surface-2 hover:bg-surface-3")}
                    >
                      <span className="min-w-0">
                        <span className="flex items-center gap-1.5 text-[14px] font-medium text-fg">
                          {TYPE_ICON[p.type]} <span className="truncate">{p.name}</span>
                        </span>
                        <span className="mt-0.5 block text-[11px] font-normal text-fg-3">{t("prop.compare.from", { fee: usd(from, 0) })}</span>
                      </span>
                      {on && (
                        <Chip size="sm" tone="ember">
                          {t("prop.compare.selected")}
                        </Chip>
                      )}
                    </button>
                  </th>
                );
              })}
            </tr>
          </thead>
          <tbody>
            {rows.map((r, ri) => (
              <tr key={r.label}>
                <td className="border-t border-line py-3 pe-3 text-fg-2">
                  <span className="flex items-center gap-2 [&_svg]:size-3.5 [&_svg]:text-fg-3">
                    {r.icon}
                    {r.label}
                  </span>
                </td>
                {plans.map((p) => {
                  const on = p.id === current;
                  return (
                    // h-px on the cell lets the inner box fill the row, so columns stay continuous when one cell wraps
                    <td key={p.id} className="h-px px-1.5 py-0 align-top">
                      <div
                        className={cn(
                          "h-full border-x border-t px-4 py-3",
                          on ? "border-x-ember/40 border-t-ember/15 bg-ember-soft/60" : "border-x-line border-t-line",
                          ri === rows.length - 1 && "rounded-b-[16px] border-b",
                          ri === rows.length - 1 && (on ? "border-b-ember/40" : "border-b-line"),
                        )}
                      >
                        {r.cell(p)}
                      </div>
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Checkout                                                            */
/* ------------------------------------------------------------------ */

function newKey() {
  const c = globalThis.crypto;
  return c && "randomUUID" in c ? c.randomUUID() : `k${Date.now().toString(36)}${Math.random().toString(36).slice(2, 12)}`;
}

export function CheckoutDialog({ plan, size, open, onOpenChange, onBought }: { plan: Plan; size: PlanSize; open: boolean; onOpenChange: (o: boolean) => void; onBought?: () => void }) {
  const t = useT();
  const [agree, setAgree] = React.useState(false);
  // the fee is charged from the USDT wallet: say so before the client ticks the rules, not after a failed payment
  const wallet = useWallet<Overview>(open ? "overview" : null);
  const available = wallet.data ? Number(usdtAvailable(wallet.data).available) : null;
  const short = available !== null && available < size.fee;
  const [busy, setBusy] = React.useState(false);
  const [err, setErr] = React.useState<unknown>(null);
  const [done, setDone] = React.useState<PurchaseResult | null>(null);
  // one key per dialog session: a retry after a network error or a pending payment reuses it, so the fee is
  // never charged twice
  const key = React.useRef<string>("");

  React.useEffect(() => {
    if (open) {
      key.current = newKey();
      setAgree(false);
      setErr(null);
      setDone(null);
      setBusy(false);
    }
  }, [open, plan.id, size.size]);

  const pay = async () => {
    setBusy(true);
    setErr(null);
    try {
      const r = await propApi<PurchaseResult>("challenges", { body: { planId: plan.id, size: size.size, idempotencyKey: key.current } });
      setDone(r);
      onBought?.();
    } catch (e) {
      setErr(e);
    } finally {
      setBusy(false);
    }
  };

  const fee = usd(size.fee, size.fee % 1 ? 2 : 0);
  const creds = done?.credentials ?? null;
  const login = creds?.login ?? done?.challenge.current?.login ?? null;

  if (done) {
    return (
      <Dialog
        open={open}
        onOpenChange={onOpenChange}
        title={t("prop.checkout.readyTitle")}
        description={`${plan.name} · ${sizeLabel(size.size)} · ${done.challenge.current?.phase ?? (plan.phases[0]?.name ?? t("prop.status.funded"))}`}
        width={560}
        footer={
          <>
            <Link href="/prop/mine">
              <Button variant="surface">
                {t("prop.myChallenges")} <ArrowRight className="rtl:-scale-x-100" />
              </Button>
            </Link>
            <PropTradeButton login={login} size="md" label={t("prop.tradeNow")} />
          </>
        }
      >
        <div className="space-y-4">
          <div className="flex items-start gap-3 rounded-[14px] border border-up/25 bg-up-soft px-4 py-3">
            <Check className="mt-0.5 size-4 shrink-0 text-up" />
            <p className="text-[13px] text-fg">
              {t("prop.checkout.paidText", { fee, size: sizeLabel(size.size) })}
            </p>
          </div>
          {creds ? (
            <>
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                <CredentialField label={t("prop.cred.login")} value={String(creds.login)} />
                <CredentialField label={t("prop.cred.server")} value="Kalks-Live" mono={false} />
                <CredentialField label={t("prop.cred.password")} value={creds.password} secret />
                <CredentialField label={t("prop.cred.investorPassword")} value={creds.investorPassword} secret />
              </div>
              <p className="text-[12.5px] text-fg-3">
                {t("prop.checkout.savePasswords")}
              </p>
            </>
          ) : (
            <p className="text-[13px] text-fg-2">
              {t("prop.checkout.passwordsShown")}
            </p>
          )}
        </div>
      </Dialog>
    );
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => !busy && onOpenChange(o)}
      title={t("prop.checkout.title", { plan: plan.name })}
      description={t("prop.checkout.description", { size: sizeLabel(size.size), fee })}
      width={600}
      footer={
        <>
          <Button variant="surface" disabled={busy} onClick={() => onOpenChange(false)}>
            {t("common.cancel")}
          </Button>
          <Button variant="ember" disabled={!agree || busy || short} onClick={pay}>
            {busy ? <Loader2 className="animate-spin" /> : <Wallet />} {err ? t("prop.checkout.retry", { fee }) : t("prop.checkout.pay", { fee })}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <div className="flex items-center justify-between gap-3 rounded-[14px] border border-line bg-surface-2 px-4 py-3">
          <div className="flex items-center gap-3">
            <span className="grid size-9 place-items-center rounded-full border border-line bg-surface-3 text-fg-2">
              <Wallet className="size-4" />
            </span>
            <div>
              <div className="text-[13.5px] font-medium">{t("prop.store.paidFromWallet")}</div>
              <div className="text-[12px] text-fg-3">{plan.refundFee ? t("prop.checkout.chargedOnceRefund") : t("prop.checkout.chargedOnceNoRefund")}</div>
            </div>
          </div>
          <span className="k-num text-[18px] font-semibold">{fee}</span>
        </div>
        {available !== null && (
          short ? (
            <div className="flex flex-wrap items-center justify-between gap-3 rounded-[14px] border border-warn/30 bg-warn-soft px-4 py-3 text-[12.5px] text-fg" data-testid="prop-checkout-short">
              <span>{t("prop.checkout.short", { balance: fmtUsdt(available), missing: fmtUsdt(size.fee - available) })}</span>
              <Link href="/wallet/deposit">
                <Button size="sm" variant="surface">
                  {t("prop.checkout.deposit")} <ArrowRight className="rtl:-scale-x-100" />
                </Button>
              </Link>
            </div>
          ) : (
            <div className="-mt-2 px-1 text-[12px] text-fg-3">{t("prop.checkout.walletBalance", { balance: fmtUsdt(available) })}</div>
          )
        )}

        <div>
          <div className="k-label mb-2">{t("prop.checkout.rulesTitle")}</div>
          <dl className="divide-y divide-line rounded-[14px] border border-line px-4">
            {planRules(t, plan, size).map(([k, v]) => (
              <div key={k} className="flex items-start justify-between gap-4 py-2.5 text-[12.5px]">
                <dt className="shrink-0 text-fg-3">{k}</dt>
                <dd className="text-end font-medium text-fg">{v}</dd>
              </div>
            ))}
          </dl>
          <p className="mt-2 text-[11.5px] text-fg-3">{t("prop.checkout.limitsNote")}</p>
        </div>

        <label className="flex cursor-pointer items-start gap-2.5 text-[12.5px] leading-snug text-fg-2">
          <input type="checkbox" checked={agree} onChange={(e) => setAgree(e.target.checked)} className="mt-0.5 size-4 shrink-0 accent-[var(--k-ember)]" />
          <span>{t("prop.checkout.agree")}</span>
        </label>

        <ErrorNote error={err} />
        {err !== null && (err as { code?: string }).code === "provisioning" && (
          <Link href="/prop/mine" className="inline-flex items-center gap-1 text-[12.5px] text-ember hover:underline">
            {t("prop.checkout.goToMine")} <ArrowRight className="size-3.5 rtl:-scale-x-100" />
          </Link>
        )}
      </div>
    </Dialog>
  );
}

/* ------------------------------------------------------------------ */
/* Page                                                                */
/* ------------------------------------------------------------------ */

export function LivePropStore() {
  const t = useT();
  const { data, error, loading, reload } = usePropPoll<{ plans: Plan[] }>("plans", 0);
  const mine = usePropPoll<{ challenges: Challenge[] }>("challenges", 0);
  const plans = React.useMemo(() => {
    const list = (data?.plans ?? []).map((p) => ({ ...p, sizes: p.sizes.filter((s) => s.enabled !== false).sort((a, b) => a.size - b.size) })).filter((p) => p.sizes.length > 0);
    return list.sort((a, b) => TYPE_ORDER.indexOf(a.type) - TYPE_ORDER.indexOf(b.type));
  }, [data]);
  const [planId, setPlanId] = React.useState<string | null>(null);
  const [sizeN, setSizeN] = React.useState<number | null>(null);
  const [buying, setBuying] = React.useState(false);

  const plan = plans.find((p) => p.id === planId) ?? plans.find((p) => p.type === "2-step") ?? plans[0] ?? null;
  const size = plan ? plan.sizes.find((s) => s.size === sizeN) ?? plan.sizes.find((s) => s.size === 50000) ?? plan.sizes[Math.floor((plan.sizes.length - 1) / 2)]! : null;
  const count = mine.data?.challenges.filter((c) => c.status === "active" || c.status === "funded").length ?? 0;

  return (
    <div className="pb-24">
      <PageHero
        page="prop"
        overlap
        title={t("prop.store.title")}
        lead={t("prop.store.subtitle")}
        actions={
          <Link href="/prop/mine">
            <Button variant="surface" size="lg">
              <Trophy /> {t("prop.myChallenges")}
              {count > 0 && (
                <Chip size="sm" tone="ember">
                  {count}
                </Chip>
              )}
            </Button>
          </Link>
        }
      />

      {error && !data ? (
        <LoadError error={error} onRetry={reload} />
      ) : loading ? (
        <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
          <Skeleton className="h-[520px] rounded-[20px] xl:col-span-8" />
          <Skeleton className="h-[520px] rounded-[20px] xl:col-span-4" />
        </div>
      ) : !plan || !size ? (
        <Card>
          <EmptyState art="propChallenge" title={t("prop.store.emptyTitle")} text={t("prop.store.emptyText")} />
        </Card>
      ) : (
        <>
          <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
            <Reveal className="xl:col-span-8">
              <Configurator
                plans={plans}
                plan={plan}
                setPlan={setPlanId}
                size={size}
                setSize={setSizeN}
              />
            </Reveal>
            <Reveal delay={0.05} className="xl:col-span-4">
              <PlanCard plan={plan} size={size} onBuy={() => setBuying(true)} />
            </Reveal>
          </div>

          <Reveal delay={0.05} className="mt-4 block">
            <CompareTable plans={plans} current={plan.id} size={size.size} onPick={setPlanId} />
          </Reveal>

          <Reveal delay={0.05} className="mt-4 block">
            <Card>
              <CardHeader title={t("prop.enforce.title")} subtitle={t("prop.enforce.subtitle")} icon={<ShieldCheck />} />
              <div className="grid grid-cols-1 gap-2 px-4 pb-6 pt-4 sm:grid-cols-2 sm:px-6 xl:grid-cols-4">
                {[
                  { t: t("prop.enforce.monitoring"), d: t("prop.enforce.monitoringText"), c: "bg-info" },
                  { t: t("prop.enforce.warnings"), d: t("prop.enforce.warningsText"), c: "bg-warn" },
                  { t: t("prop.enforce.breach"), d: t("prop.enforce.breachText"), c: "bg-down" },
                  { t: t("prop.enforce.pass"), d: t("prop.enforce.passText"), c: "bg-up" },
                ].map((x) => (
                  <div key={x.t} className="k-row relative overflow-hidden py-3 ps-5 pe-4">
                    <span className={cn("absolute inset-y-2 start-0 w-[3px] rounded-e-full", x.c)} />
                    <div className="text-[13.5px] font-medium">{x.t}</div>
                    <div className="mt-0.5 text-[12.5px] text-fg-3">{x.d}</div>
                  </div>
                ))}
              </div>
            </Card>
          </Reveal>

          <CheckoutDialog plan={plan} size={size} open={buying} onOpenChange={setBuying} onBought={mine.reload} />
        </>
      )}
    </div>
  );
}
