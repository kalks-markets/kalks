"use client";

import * as React from "react";
import Link from "next/link";
import {
  ArrowUpRight,
  Banknote,
  ChevronRight,
  Coins,
  Copy,
  Layers,
  Link2,
  QrCode,
  Target,
  Timer,
  UserPlus,
  Users,
  Hourglass,
} from "lucide-react";
import { toast } from "sonner";
import {
  Button,
  CapsuleBars,
  Card,
  CardHeader,
  Chip,
  CopyButton,
  EquityChart,
  Icon3D,
  IconGlyph,
  KpiCard,
  ListRow,
  Money,
  PageHeader,
  Progress,
  Reveal,
  Segmented,
  Tooltip,
  cn,
  formatMoney,
} from "@/components/kit";
import type { SeriesPoint } from "@/components/kit";
import { tr, useFormat, useT } from "@kalks/i18n/react";
import { intlTag } from "@kalks/i18n/locales";
import { ShareButtons } from "@/components/partner/share-buttons";
// readable names for engine symbols, including Kalks FX Options series codes
import { symbolLabel } from "@/components/trading/instrument";
import {
  fmtDate,
  fmtDay,
  fmtLots,
  fmtMonth,
  fmtPct,
  isOptionLine,
  kindLabel,
  monthName,
  optionsLabel,
  referralLink,
  relTime,
  scheduleLabel,
  shortUrl,
  usePartner,
  utcDay,
  type CommissionRow,
  type Dashboard,
  type Level,
} from "./api";
import {
  CardEmpty,
  CommissionStatusChip,
  PageFallback,
  PersonCell,
  SkeletonGrid,
} from "./ui";
import { QrDialog } from "./qr";
import { PageHero } from "@/components/page-hero";
import { useSession } from "@/components/session";
import { VisitingCard } from "@/components/visiting-card";

const DAY = 86400;

/* ------------------------------------------------------------------ */
/* Level                                                               */
/* ------------------------------------------------------------------ */

function LevelHero({ d }: { d: Dashboard }) {
  const t = useT();
  const f = useFormat();
  const levels = [...d.progress.levels].sort((a, b) => a.rank - b.rank);
  const cur = d.member.level ?? levels[0] ?? null;
  const curIdx = Math.max(
    0,
    levels.findIndex((l) => l.key === cur?.key),
  );
  const next = d.progress.next;
  const { activeClients, monthlyLots, monthEnds } = d.progress;
  const clientsPct = next
    ? next.minActiveClients > 0
      ? (activeClients / next.minActiveClients) * 100
      : 100
    : 100;
  const lotsPct = next
    ? next.minMonthlyLots > 0
      ? (monthlyLots / next.minMonthlyLots) * 100
      : 100
    : 100;
  const ladderPct =
    levels.length > 1
      ? ((curIdx + (next ? Math.min(clientsPct, lotsPct, 100) / 100 : 0)) /
          (levels.length - 1)) *
        100
      : 100;
  const resets = fmtDate(monthEnds, false);
  const suspended = d.member.status !== "active";

  return (
    <Card hot className="h-full overflow-hidden">
      <div className="relative flex h-full flex-col gap-6 p-5 sm:p-7">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div className="flex min-w-0 items-center gap-4">
            <Icon3D name={cur?.icon ?? "coin"} size={60} className="shrink-0" />
            <div className="min-w-0">
              <div className="k-label text-ember">
                {t("partner.dash.levelOf", {
                  n: curIdx + 1,
                  total: levels.length || 1,
                })}
              </div>
              <h2 className="mt-1 text-[28px] font-semibold leading-none tracking-tight sm:text-[30px]">
                {cur?.name ?? t("partner.dash.partnerFallback")}
              </h2>
              <div className="mt-2 flex flex-wrap items-center gap-x-2 gap-y-1 text-[12.5px] text-fg-2">
                <span>{t("partner.dash.since", { date: fmtMonth(d.member.joinedAt) })}</span>
                <span className="text-fg-3">·</span>
                <span>
                  {t("partner.dash.code")}{" "}
                  <span className="font-mono text-fg">{d.member.code}</span>
                </span>
              </div>
            </div>
          </div>
          <div className="flex items-center gap-2">
            {suspended ? (
              <Chip tone="down" dot>
                {t("partner.dash.suspended")}
              </Chip>
            ) : (
              <Chip tone="gold">
                {t("partner.schedulePayouts", {
                  schedule: scheduleLabel(d.programme.payout.schedule),
                })}
              </Chip>
            )}
            <Link href="/partner/commissions">
              <Button size="sm" variant="surface">
                {t("partner.dash.rateCard")}{" "}
                <ChevronRight className="rtl:-scale-x-100" />
              </Button>
            </Link>
          </div>
        </div>

        {suspended && (
          <div className="rounded-[14px] border border-down/30 bg-down/10 px-4 py-3 text-[12.5px] text-fg-2">
            {t("partner.dash.suspendedNote")}
          </div>
        )}

        <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
          <ProgressBlock
            icon={<Users className="size-4" />}
            label={t("partner.dash.activeClients")}
            value={activeClients}
            target={next?.minActiveClients ?? null}
            pct={clientsPct}
            nextName={next?.name}
            hint={
              next
                ? next.minActiveClients > activeClients
                  ? t("partner.dash.moreClients", {
                      count: next.minActiveClients - activeClients,
                    })
                  : t("partner.dash.targetMetMonth")
                : t("partner.dash.clientsTraded")
            }
            fmt={(v) => String(v)}
          />
          <ProgressBlock
            icon={<Layers className="size-4" />}
            label={t("partner.dash.monthlyLots")}
            value={monthlyLots}
            target={next?.minMonthlyLots ?? null}
            pct={lotsPct}
            nextName={next?.name}
            hint={
              next
                ? next.minMonthlyLots > monthlyLots
                  ? t("partner.dash.lotsToGo", {
                      lots: fmtLots(next.minMonthlyLots - monthlyLots, 1),
                      date: resets,
                    })
                  : t("partner.dash.targetMetResets", { date: resets })
                : t("partner.dash.allTiersResets", { date: resets })
            }
            fmt={(v) =>
              v.toLocaleString(intlTag(f.locale), { maximumFractionDigits: 1 })
            }
          />
        </div>

        {levels.length > 1 && (
          <div className="mt-auto">
            <div
              className="relative grid gap-1"
              style={{
                gridTemplateColumns: `repeat(${levels.length}, minmax(0, 1fr))`,
              }}
            >
              <div
                className="absolute top-[19px] h-px bg-white/10"
                style={{
                  insetInlineStart: `${50 / levels.length}%`,
                  insetInlineEnd: `${50 / levels.length}%`,
                }}
              />
              <div
                className="absolute top-[19px] h-px bg-gradient-to-r from-ember to-gold rtl:bg-gradient-to-l"
                style={{
                  insetInlineStart: `${50 / levels.length}%`,
                  width: `${(Math.min(100, ladderPct) / 100) * (100 - 100 / levels.length)}%`,
                }}
              />
              {levels.map((l, i) => {
                const done = i < curIdx;
                const on = i === curIdx;
                return (
                  <Tooltip
                    key={l.key}
                    content={
                      l.rank === 1
                        ? t("partner.dash.levelStarting", { name: l.name })
                        : t("partner.dash.levelNeeds", {
                            name: l.name,
                            clients: l.minActiveClients,
                            lots: l.minMonthlyLots.toLocaleString(intlTag(f.locale)),
                          })
                    }
                  >
                    <div className="relative flex flex-col items-center gap-1.5 text-center">
                      <span
                        className={cn(
                          "grid size-10 place-items-center rounded-full border",
                          on
                            ? "border-ember/60 bg-surface text-ember"
                            : done
                              ? "border-gold/40 bg-surface text-gold"
                              : "border-line bg-surface text-fg-3",
                        )}
                      >
                        <IconGlyph name={l.icon} className="size-[18px]" />
                      </span>
                      <span
                        className={cn(
                          "max-w-full truncate text-[11.5px] font-medium",
                          on ? "text-fg" : done ? "text-gold" : "text-fg-3",
                        )}
                      >
                        {l.name}
                      </span>
                    </div>
                  </Tooltip>
                );
              })}
            </div>
            <div className="mt-4 flex flex-wrap items-center gap-2 rounded-[14px] border border-line bg-surface-2/60 px-4 py-2.5 text-[12.5px]">
              {next ? (
                <>
                  <span className="text-fg-2">
                    {t("partner.dash.atLevel", { name: next.name })}
                  </span>
                  {next.perks.length ? (
                    next.perks.map((p) => (
                      <Chip key={p} size="sm" tone="gold">
                        {p}
                      </Chip>
                    ))
                  ) : (
                    <span className="text-fg-3">
                      {t("partner.dash.higherRates")}
                    </span>
                  )}
                </>
              ) : (
                <>
                  <span className="text-fg-2">{t("partner.dash.topReached")}</span>
                  {cur?.perks.map((p) => (
                    <Chip key={p} size="sm" tone="gold">
                      {p}
                    </Chip>
                  ))}
                </>
              )}
            </div>
          </div>
        )}
      </div>
    </Card>
  );
}

function ProgressBlock({
  icon,
  label,
  value,
  target,
  pct,
  nextName,
  hint,
  fmt,
}: {
  icon: React.ReactNode;
  label: string;
  value: number;
  target: number | null;
  pct: number;
  nextName?: string;
  hint: string;
  fmt: (v: number) => string;
}) {
  const t = useT();
  return (
    <div className="rounded-[16px] border border-line bg-surface-2/60 px-4 py-3.5">
      <div className="flex items-center justify-between gap-2 text-[12.5px]">
        <span className="flex items-center gap-2 text-fg-2">
          {icon}
          {label}
        </span>
        {target !== null && nextName && (
          <span className="k-num text-fg-3">
            {t("partner.dash.pctTo", {
              pct: Math.min(100, Math.floor(pct)),
              name: nextName,
            })}
          </span>
        )}
      </div>
      <div className="mt-2 flex items-baseline gap-1.5">
        <span className="k-num text-[26px] font-semibold leading-none">
          {fmt(value)}
        </span>
        {target !== null && (
          <span className="k-num text-[14px] text-fg-3">/ {fmt(target)}</span>
        )}
      </div>
      {target !== null && (
        <Progress
          value={pct}
          className="mt-3 h-2"
          tone={pct >= 100 ? "up" : pct > 70 ? "gold" : "ember"}
        />
      )}
      <div className="mt-2 text-[11.5px] text-fg-3">{hint}</div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Referral link                                                       */
/* ------------------------------------------------------------------ */

function ReferralCard({ d }: { d: Dashboard }) {
  const t = useT();
  const f = useFormat();
  const [qr, setQr] = React.useState(false);
  const me = useSession();
  const link = referralLink(d.linkBase, d.member.code);
  return (
    <Card className="flex h-full flex-col">
      <CardHeader
        title={t("partner.dash.referralTitle")}
        subtitle={t("partner.dash.referralSubtitle")}
        icon={<Link2 />}
      />
      <div className="flex flex-1 flex-col gap-4 px-4 pb-5 pt-4 sm:px-6">
        {/* the partner's visiting card (founder 2026-10-10): name, level, code and link */}
        <VisitingCard
          finish="orange"
          kicker={t.dyn("partner.card.kicker", "Kalks Partner")}
          name={me.name}
          title={`${d.member.level?.name ?? t("partner.dash.partnerFallback")} · ${t("partner.dash.since", { date: fmtMonth(d.member.joinedAt) })}`}
          footer={
            <div className="flex items-end justify-between gap-3">
              <div className="min-w-0">
                <div dir="ltr" className="truncate text-start font-mono text-[clamp(10.5px,3.4cqw,13.5px)] font-semibold">
                  {shortUrl(link)}
                </div>
                <div className="mt-[0.6cqw] text-[clamp(7.5px,2.3cqw,9.5px)] font-semibold uppercase tracking-[0.14em] text-white/75">{t("partner.referralLink")}</div>
              </div>
              <div className="shrink-0 text-end">
                <div className="font-mono text-[clamp(12px,4.4cqw,18px)] font-bold tracking-wider">{d.member.code}</div>
                <div className="mt-[0.6cqw] text-[clamp(7.5px,2.3cqw,9.5px)] font-semibold uppercase tracking-[0.14em] text-white/75">{t("partner.referralCode")}</div>
              </div>
            </div>
          }
        />
        <div className="flex items-center gap-2 rounded-[14px] border border-ember/30 bg-ember-soft/60 py-1.5 ps-4 pe-1.5">
          <span
            dir="ltr"
            className="min-w-0 flex-1 truncate text-start font-mono text-[13px] text-fg"
          >
            {shortUrl(link)}
          </span>
          <CopyButton
            value={link}
            label={t("partner.referralLink")}
            className="size-8 shrink-0 rounded-full"
          />
          <Button
            size="xs"
            variant="ember"
            onClick={() => setQr(true)}
            className="shrink-0"
          >
            <QrCode /> QR
          </Button>
        </div>
        <div>
          <div className="mb-2 text-[12px] text-fg-3">{t("partner.dash.share")}</div>
          <ShareButtons
            url={link}
            text={t("partner.dash.shareText")}
          />
        </div>
        <div className="mt-auto grid grid-cols-3 gap-2">
          {[
            [t("partner.clicks"), d.funnel.clicks],
            [t("partner.signups"), d.funnel.signups],
            [t("partner.firstDeposits"), d.funnel.ftds],
          ].map(([k, v]) => (
            <div key={k as string} className="k-row min-w-0 px-3 py-2.5">
              <div className="truncate text-[11px] text-fg-3">{k}</div>
              <div className="k-num text-[15px] font-medium">
                {f.number(v as number, 0)}
              </div>
            </div>
          ))}
        </div>
        <Link
          href="/partner/links"
          className="flex items-center justify-between text-[12.5px] text-fg-2 hover:text-fg"
        >
          {t("partner.dash.campaignLinks")}{" "}
          <ArrowUpRight className="size-4 rtl:-scale-x-100" />
        </Link>
      </div>
      <QrDialog
        open={qr}
        onOpenChange={setQr}
        value={link}
        title={t("partner.dash.referralQr")}
        fileBase={`kalks-${d.member.code}-qr`}
      />
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Earnings                                                            */
/* ------------------------------------------------------------------ */

const RANGES = { "1M": 30, "3M": 90, "6M": 180 } as const;
type Range = keyof typeof RANGES;

/** Daily points for the last 180 days (days without accruals carry the running total). */
function dailySeries(d: Dashboard): SeriesPoint[] {
  const today = Math.floor(Date.now() / 1000 / DAY) * DAY;
  const byDay = new Map<number, { amount: number; cumulative: number }>();
  for (const s of d.series)
    byDay.set(Math.floor(Date.parse(s.date) / 1000 / DAY) * DAY, s);
  const first = d.series[0];
  let acc = first ? first.cumulative - first.amount : d.earnings.lifetime;
  const out: SeriesPoint[] = [];
  for (let i = 179; i >= 0; i--) {
    const t = today - i * DAY;
    const hit = byDay.get(t);
    if (hit) acc = hit.cumulative;
    out.push({ time: t, value: +acc.toFixed(2), volume: hit ? hit.amount : 0 });
  }
  return out;
}

function EarningsCard({ d }: { d: Dashboard }) {
  const t = useT();
  const all = React.useMemo(() => dailySeries(d), [d]);
  const [range, setRange] = React.useState<Range>("3M");
  const data = React.useMemo(() => all.slice(-RANGES[range]), [all, range]);
  const [hover, setHover] = React.useState<SeriesPoint | null>(null);
  const onHover = React.useCallback((p: SeriesPoint | null) => setHover(p), []);
  const earned = data.reduce((s, p) => s + (p.volume ?? 0), 0);
  const shown = hover ?? data[data.length - 1]!;
  const empty = d.earnings.lifetime === 0 && d.series.length === 0;
  return (
    <Card className="h-full">
      <div className="flex flex-col gap-4 px-5 pt-6 sm:flex-row sm:items-start sm:justify-between sm:px-6">
        <div>
          <div className="k-label">{t("partner.dash.lifetime")}</div>
          <div className="mt-2 flex flex-wrap items-baseline gap-3">
            <Money
              value={shown.value}
              countUp={false}
              className="text-[34px] font-semibold tracking-tight"
            />
            {!empty && (
              <Chip tone={earned > 0 ? "up" : "neutral"}>
                {t("partner.dash.earnedIn", {
                  amount: `${earned >= 0 ? "+" : ""}${formatMoney(earned)}`,
                  range,
                })}
              </Chip>
            )}
          </div>
          <div className="mt-1 text-xs text-fg-3">
            {hover ? (
              <>
                {t("partner.dash.hoverEarned", {
                  date: fmtDate(new Date(hover.time * 1000).toISOString()),
                  amount: formatMoney(hover.volume ?? 0),
                })}
              </>
            ) : (
              t("partner.dash.lifetimeHint")
            )}
          </div>
        </div>
        {!empty && (
          <Segmented
            size="xs"
            value={range}
            onChange={setRange}
            options={["1M", "3M", "6M"] as const}
          />
        )}
      </div>
      <div className="px-3 pb-4 pt-2">
        {empty ? (
          <div className="px-2 pb-2 pt-4 sm:px-3">
            <CardEmpty
              className="h-[248px]"
              title={t("partner.noCommission")}
              text={t("partner.dash.earningsEmpty")}
            />
          </div>
        ) : (
          <EquityChart data={data} height={280} onHover={onHover} />
        )}
      </div>
    </Card>
  );
}

/** The last `n` weeks (Monday UTC starts), zero-filled. */
function weeks(d: Dashboard, n: number) {
  const today = Math.floor(Date.now() / 1000 / DAY) * DAY;
  const dow = (new Date(today * 1000).getUTCDay() + 6) % 7;
  const monday = today - dow * DAY;
  const byWeek = new Map<number, number>();
  for (const w of d.weekly)
    byWeek.set(Math.floor(Date.parse(w.week) / 1000 / DAY) * DAY, w.amount);
  return Array.from({ length: n }, (_, i) => {
    const t = monday - (n - 1 - i) * 7 * DAY;
    return {
      t,
      label: utcDay(t * 1000),
      value: byWeek.get(t) ?? 0,
    };
  });
}

function WeeklyCard({ d }: { d: Dashboard }) {
  const t = useT();
  const data = weeks(d, 8);
  const last = data[data.length - 1]!;
  const prev = data[data.length - 2]!;
  const ch =
    prev.value > 0 ? ((last.value - prev.value) / prev.value) * 100 : null;
  const any = data.some((w) => w.value !== 0);
  return (
    <Card className="flex h-full flex-col">
      <CardHeader
        title={t("partner.dash.byWeek")}
        subtitle={t("partner.dash.byWeekSubtitle")}
        action={
          ch !== null ? (
            <Chip tone={ch >= 0 ? "up" : "down"}>
              {t("partner.dash.wow", {
                pct: `${ch >= 0 ? "+" : ""}${ch.toFixed(1)}`,
              })}
            </Chip>
          ) : undefined
        }
      />
      <div className="px-5 pt-4 sm:px-6">
        <Money
          value={last.value}
          countUp={false}
          className="text-[26px] font-semibold"
        />
        <div className="text-[12px] text-fg-3">
          {t("partner.dash.weekOf", {
            week: last.label,
            date: fmtDay(d.programme.payout.nextClose),
          })}
        </div>
      </div>
      <div className="flex-1 px-4 pb-5 pt-6 sm:px-6">
        {any ? (
          <CapsuleBars
            data={data.map((w) => ({
              label: w.label,
              value: Math.max(0, w.value),
            }))}
            height={200}
            format={(v) => formatMoney(v, "USD", 0)}
            className="gap-1.5 sm:gap-3 [&>div>span]:whitespace-nowrap [&>div>span]:text-[10px]"
          />
        ) : (
          <CardEmpty
            className="h-[200px]"
            title={t("partner.dash.weeklyEmpty")}
            text={t("partner.dash.weeklyEmptyText")}
          />
        )}
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Clients & feed                                                      */
/* ------------------------------------------------------------------ */

function TopClientsCard({ d }: { d: Dashboard }) {
  const t = useT();
  const top = d.topClients;
  const max = Math.max(...top.map((c) => c.lotsMonth), 0.0001);
  return (
    <Card className="flex h-full flex-col">
      <CardHeader
        title={t("partner.dash.topClients")}
        subtitle={t("partner.dash.topClientsSubtitle", {
          month: monthName(d.progress.month),
        })}
        action={
          <Link href="/partner/clients">
            <Button size="sm" variant="surface">
              {t("partner.dash.allClients")}
            </Button>
          </Link>
        }
      />
      <div className="mt-4 flex-1 space-y-2 px-4 pb-5 sm:px-6">
        {top.length === 0 ? (
          <CardEmpty
            title={t("partner.dash.topEmpty")}
            text={t("partner.dash.topEmptyText")}
          />
        ) : (
          top.map((c, i) => (
            <ListRow key={c.id} href="/partner/clients" className="py-2.5">
              <span className="w-4 text-center font-mono text-[11px] text-fg-3">
                {i + 1}
              </span>
              <div className="min-w-0 flex-1">
                <PersonCell name={c.name} country={c.country} size={32} />
              </div>
              <div className="w-24 shrink-0 text-end">
                <div className="k-num text-[13px] font-medium">
                  {t("partner.lotsN", { lots: fmtLots(c.lotsMonth) })}
                </div>
                <div className="mt-1 h-1 overflow-hidden rounded-full bg-surface-3">
                  <div
                    className="h-full rounded-full bg-gold"
                    style={{ width: `${(c.lotsMonth / max) * 100}%` }}
                  />
                </div>
              </div>
            </ListRow>
          ))
        )}
      </div>
    </Card>
  );
}

export function commissionLine(e: CommissionRow) {
  if (e.kind === "cpa") return tr("partner.line.cpa");
  // option lines are paid per contract: "Options · 3 contracts" instead of lots
  const option = isOptionLine(e);
  const lot = (v: number) => tr("partner.line.lot", { lots: fmtLots(v) });
  const sym = e.symbol ? symbolLabel(tr, e.symbol) : null;
  if (e.kind === "lot")
    return `${sym ?? tr("partner.line.trade")} · ${option ? optionsLabel(e.contracts) : lot(e.lots)} · L${e.tier}`;
  if (e.kind === "split")
    return `${kindLabel("split")}${sym ? ` · ${sym}` : ""} · L${e.tier}`;
  if (e.kind === "rebate")
    return `${kindLabel("rebate")}${sym ? ` · ${sym}` : ""}${option ? ` · ${optionsLabel(e.contracts)}` : e.lots ? ` · ${lot(e.lots)}` : ""}`;
  return e.note ? `${kindLabel(e.kind)} · ${e.note}` : kindLabel(e.kind);
}

function RecentCard({ d }: { d: Dashboard }) {
  const t = useT();
  const events = d.recent.slice(0, 7);
  return (
    <Card className="flex h-full flex-col">
      <CardHeader
        title={t("partner.dash.recent")}
        subtitle={t("partner.dash.recentSubtitle")}
        action={
          <Link href="/partner/commissions">
            <Button size="sm" variant="surface">
              {t("partner.dash.ledger")}
            </Button>
          </Link>
        }
      />
      <div className="mt-4 flex-1 space-y-2 px-4 pb-5 sm:px-6">
        {events.length === 0 ? (
          <CardEmpty
            title={t("partner.noCommission")}
            text={t("partner.dash.recentEmptyText")}
          />
        ) : (
          events.map((e) => (
            <div
              key={e.id}
              className="k-row flex items-center gap-3 px-3.5 py-2.5"
            >
              <div className="min-w-0 flex-1">
                <PersonCell
                  name={e.client.name}
                  country={e.client.country}
                  size={32}
                  sub={`${commissionLine(e)} · ${relTime(e.createdAt)}`}
                />
              </div>
              <div className="flex shrink-0 flex-col items-end gap-1">
                <span
                  className={cn(
                    "k-num text-[13.5px] font-semibold",
                    e.amount >= 0 ? "text-up" : "text-down",
                  )}
                >
                  {e.amount >= 0 ? "+" : "-"}
                  {formatMoney(Math.abs(e.amount))}
                </span>
                <CommissionStatusChip status={e.status} />
              </div>
            </div>
          ))
        )}
      </div>
    </Card>
  );
}

function CpaCard({ d }: { d: Dashboard }) {
  const t = useT();
  const { cpa } = d.programme;
  const cur = d.member.level;
  const better = [...d.progress.levels]
    .filter((l) => cur && l.rank > cur.rank && l.cpaAmount > cur.cpaAmount)
    .sort((a, b) => a.rank - b.rank)[0] as Level | undefined;
  const mins = Math.round(d.programme.minTradeSeconds / 60);
  const rules: [React.ReactNode, string][] = [
    [
      <Target key="t" className="size-3.5" />,
      t("partner.cpa.ruleDeposit", {
        amount: formatMoney(cpa.minFirstDeposit, "USD", 0),
      }),
    ],
    ...(cpa.requireFirstTrade
      ? ([
          [
            <Timer key="u" className="size-3.5" />,
            t("partner.cpa.ruleTrade", {
              duration:
                mins >= 1
                  ? t("partner.unit.min", { n: mins })
                  : t("partner.unit.sec", { n: d.programme.minTradeSeconds }),
            }),
          ],
        ] as [React.ReactNode, string][])
      : []),
    [
      <Hourglass key="h" className="size-3.5" />,
      t("partner.cpa.ruleHold", { count: cpa.holdDays }),
    ],
  ];
  return (
    <Card className="flex h-full flex-col overflow-hidden">
      <CardHeader
        title={t("partner.cpa.title")}
        subtitle={t("partner.cpa.subtitleDash")}
        action={
          cpa.enabled ? (
            <Chip tone="gold">
              {t("partner.cpa.earnedCount", { n: d.earnings.cpaCount })}
            </Chip>
          ) : (
            <Chip>{t("partner.cpa.notOffered")}</Chip>
          )
        }
      />
      <div className="px-5 pt-4 sm:px-6">
        <Money
          value={d.earnings.cpaEarned}
          countUp={false}
          className="text-[30px] font-semibold"
        />
        <div className="text-[12px] text-fg-3">
          {d.earnings.cpaWaiting > 0
            ? t("partner.cpa.waiting", { count: d.earnings.cpaWaiting })
            : t("partner.cpa.earnedFrom")}
        </div>
      </div>
      {cpa.enabled ? (
        <div className="mt-4 space-y-2 px-4 sm:px-6">
          <div className="k-row flex items-center justify-between px-3.5 py-2.5 text-[12.5px]">
            <span className="text-fg-2">
              {t("partner.cpa.perClientAt", {
                name: cur?.name ?? t("partner.yourLevel"),
              })}
            </span>
            <span className="k-num font-semibold">
              {formatMoney(cur?.cpaAmount ?? 0, "USD", 0)}
              {better && (
                <span className="font-normal text-fg-3">
                  {" "}
                  ·{" "}
                  {t("partner.cpa.amountAt", {
                    amount: formatMoney(better.cpaAmount, "USD", 0),
                    name: better.name,
                  })}
                </span>
              )}
            </span>
          </div>
          {rules.map(([icon, text], i) => (
            <div
              key={i}
              className="flex items-center gap-2.5 text-[12.5px] text-fg-2"
            >
              <span className="grid size-6 shrink-0 place-items-center rounded-full bg-surface-3 text-fg-2">
                {icon}
              </span>
              {text}
            </div>
          ))}
        </div>
      ) : (
        <div className="mt-4 px-4 text-[12.5px] text-fg-3 sm:px-6">
          {t("partner.cpa.offText")}
        </div>
      )}
      <div className="mt-auto px-4 pb-5 pt-5 sm:px-6">
        <div className="k-row flex items-center justify-between px-4 py-3">
          <div>
            <div className="text-[12px] text-fg-3">
              {t("partner.dash.rebateSplit")}
            </div>
            <div className="k-num mt-0.5 text-[14px] font-medium">
              {fmtPct(d.member.rebatePct)} <span className="text-fg-3">·</span>{" "}
              {fmtPct(d.member.splitPct)}
            </div>
          </div>
          <Link href="/partner/commissions#rebates">
            <Button size="xs" variant="surface">
              {t("partner.dash.adjust")}
            </Button>
          </Link>
        </div>
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */

export function LivePartnerDashboard() {
  const t = useT();
  const f = useFormat();
  const TITLE = t("partner.dash.title");
  const SUBTITLE = t("partner.dash.subtitle");
  const { data: d, error, reload } = usePartner<Dashboard>("", 60_000);

  if (!d)
    return (
      <PageFallback
        title={TITLE}
        subtitle={SUBTITLE}
        error={error}
        onRetry={reload}
        skeleton={
          <SkeletonGrid
            rows={[
              { cols: "xl:grid-cols-[2fr_1fr]", h: "h-[380px]", n: 2 },
              { cols: "sm:grid-cols-2 xl:grid-cols-5", h: "h-[150px]", n: 5 },
              { cols: "xl:grid-cols-[2fr_1fr]", h: "h-[360px]", n: 2 },
            ]}
          />
        }
      />
    );

  const link = referralLink(d.linkBase, d.member.code);
  const next = d.progress.next;
  const mon = monthName(d.progress.month);
  const prevMon = monthName(d.progress.month, -1);
  const lotsCh =
    d.progress.prevMonthLots > 0
      ? ((d.progress.monthlyLots - d.progress.prevMonthLots) /
          d.progress.prevMonthLots) *
        100
      : null;
  const due = d.earnings.pending + d.earnings.approved;

  return (
    <div className="pb-24">
      <PageHero page="partner" overlap
        title={TITLE}
        lead={SUBTITLE}
        actions={
          <>
            <Link href="/partner/payouts">
              <Button variant="surface" size="lg">
                <Banknote /> {t("partner.payouts.title")}
              </Button>
            </Link>
            <Button
              variant="ember"
              size="lg"
              onClick={() => {
                navigator.clipboard?.writeText(link).catch(() => {});
                toast.success(t("partner.toast.linkCopied"), {
                  description: shortUrl(link),
                });
              }}
            >
              <Copy /> {t("partner.copyReferralLink")}
            </Button>
          </>
        }
      />

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal className="min-w-0 xl:col-span-8">
          <LevelHero d={d} />
        </Reveal>
        <Reveal delay={0.05} className="min-w-0 xl:col-span-4">
          <ReferralCard d={d} />
        </Reveal>
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-5">
        <KpiCard
          label={t("partner.dash.totalReferrals")}
          icon={<UserPlus />}
          value={
            <span className="k-num">{f.number(d.counts.referrals, 0)}</span>
          }
          chip={t("partner.dash.plusThisMonth", {
            n: d.counts.referralsThisMonth,
          })}
          chipTone={d.counts.referralsThisMonth > 0 ? "up" : "neutral"}
          href="/partner/clients"
        />
        <KpiCard
          label={t("partner.dash.activeClients")}
          icon={<Users />}
          value={<span className="k-num">{d.progress.activeClients}</span>}
          chip={
            next
              ? t("partner.dash.neededFor", {
                  n: next.minActiveClients,
                  name: next.name,
                })
              : t("partner.dash.topLevel")
          }
          chipTone="gold"
          href="/partner/network"
          delay={0.04}
        />
        <KpiCard
          label={t("partner.dash.networkLots", { month: mon })}
          icon={<Layers />}
          value={
            <span className="k-num">
              {f.number(d.progress.monthlyLots, 1)}
            </span>
          }
          chip={
            lotsCh !== null
              ? t("partner.dash.vsMonth", {
                  pct: `${lotsCh >= 0 ? "+" : ""}${lotsCh.toFixed(1)}`,
                  month: prevMon,
                })
              : t("partner.dash.lotsInMonth", {
                  lots: fmtLots(d.progress.prevMonthLots, 1),
                  month: prevMon,
                })
          }
          chipTone={lotsCh === null ? "neutral" : lotsCh >= 0 ? "up" : "down"}
          href="/partner/network"
          delay={0.08}
        />
        <KpiCard
          label={t("partner.pendingCommission")}
          icon={<Coins />}
          value={<Money value={due} countUp={false} />}
          hot
          chip={t("partner.batchCloses", {
            date: fmtDay(d.programme.payout.nextClose),
          })}
          chipTone="ember"
          href="/partner/payouts"
          delay={0.12}
        />
        <KpiCard
          label={t("partner.paidAllTime")}
          icon={<Banknote />}
          value={<Money value={d.earnings.paid} countUp={false} />}
          chip={t("partner.dash.intoWallet")}
          href="/partner/payouts"
          delay={0.16}
          className="sm:col-span-2 lg:col-span-1"
        />
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal delay={0.05} className="min-w-0 xl:col-span-8">
          <EarningsCard d={d} />
        </Reveal>
        <Reveal delay={0.1} className="min-w-0 xl:col-span-4">
          <WeeklyCard d={d} />
        </Reveal>
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4 lg:grid-cols-2 xl:grid-cols-3">
        <Reveal delay={0.05} className="min-w-0">
          <TopClientsCard d={d} />
        </Reveal>
        <Reveal delay={0.1} className="min-w-0">
          <RecentCard d={d} />
        </Reveal>
        <Reveal delay={0.15} className="min-w-0 lg:col-span-2 xl:col-span-1">
          <CpaCard d={d} />
        </Reveal>
      </div>
    </div>
  );
}
