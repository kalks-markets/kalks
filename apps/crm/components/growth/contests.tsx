"use client";

import * as React from "react";
import Link from "next/link";
import { AlertTriangle, ArrowUpRight, CalendarDays, Check, ChevronRight, Gift, Loader2, Medal, ShieldAlert, Timer, Trophy, Users } from "lucide-react";
import { toast } from "sonner";
import { Avatar, Button, Card, CardHeader, Chip, CopyButton, DataTable, Dialog, DialogClose, Flag, Icon3D, Illustration, KeyValue, KpiCard, Money, PageHeader, Reveal, cn, type Column } from "@/components/kit";
import { TERMINAL_URL } from "@/lib/live";
import { Countdown } from "@/components/rewards/countdown";
import { tr, useT } from "@kalks/i18n/react";
import { BannerSlot } from "./banner-slot";
import {
  GrowthApiError,
  bandLabel,
  errorToast,
  fmtContracts,
  fmtCount,
  fmtDate,
  fmtLots,
  fmtPct,
  fmtPoints,
  fmtUsd,
  growthApi,
  isOptionsContest,
  optionsSystemGroup,
  prizeZone,
  projectedPrize,
  scoringLabel,
  useGrowth,
  type CashbackMe,
  type Contest,
  type ContestCard,
  type ContestDetail,
  type ContestsResp,
  type JoinResult,
  type Rewards,
  type Standing,
} from "./api";
import { CardEmpty, GrowthStatus, LiveAccountPicker, PageFallback, RankBadge, SectionTitle } from "./ui";
import { PageHero } from "@/components/page-hero";

/* ------------------------------------------------------------------ */
/* Helpers                                                             */
/* ------------------------------------------------------------------ */

export const isRunning = (c: Pick<Contest, "status">) => c.status === "running";
export const isUpcoming = (c: Pick<Contest, "status">) => c.status === "scheduled";
export const isPast = (c: Pick<Contest, "status">) => ["ended", "finalized", "paid", "cancelled"].includes(c.status);
export const canJoin = (c: Pick<Contest, "status" | "maxEntrants" | "entrants">) => (isRunning(c) || isUpcoming(c)) && (c.maxEntrants === null || c.entrants < c.maxEntrants);

/** Main score of a standing in the contest's scoring unit (volume: lots in a CFD contest, contracts in an options one). */
export function scoreText(c: Pick<Contest, "scoring">, s: Pick<Standing, "returnPct" | "profit" | "lots" | "score" | "contracts">) {
  if (c.scoring === "profit") return fmtUsd(s.profit);
  if (c.scoring === "lots") return tr("rewards.value.lots", { lots: fmtLots(s.lots) });
  if (c.scoring === "contracts") return tr("rewards.value.contracts", { contracts: fmtContracts(s.contracts ?? s.score) });
  return fmtPct(+s.returnPct.toFixed(2), true);
}

/** The volume column of a standing: contracts in an options contest, lots otherwise. */
export const volumeText = (c: Pick<Contest, "instrument">, s: Pick<Standing, "lots" | "contracts">) => (isOptionsContest(c) ? fmtContracts(s.contracts ?? 0) : fmtLots(s.lots));

const scoreTone = (c: Pick<Contest, "scoring">, s: Pick<Standing, "returnPct" | "profit">) => {
  if (c.scoring === "lots" || c.scoring === "contracts") return "text-fg";
  const v = c.scoring === "profit" ? s.profit : s.returnPct;
  return v > 0 ? "text-up" : v < 0 ? "text-down" : "text-fg";
};

export function kindChip(c: Pick<Contest, "kind" | "instrument">) {
  const kind =
    c.kind === "live" ? (
      <Chip tone="ember" size="sm" className="font-semibold tracking-wider">
        {tr("rewards.contest.badge.live")}
      </Chip>
    ) : (
      <Chip tone="gold" size="sm" className="font-semibold tracking-wider">
        {tr("rewards.contest.badge.demo")}
      </Chip>
    );
  if (!isOptionsContest(c)) return kind;
  return (
    <>
      {kind}
      <Chip tone="info" size="sm" className="font-semibold tracking-wider">
        {tr("rewards.contest.badge.options")}
      </Chip>
    </>
  );
}

/** "Needs 3 more trades to rank" when an entry is below the contest's minimum. */
export function tradesHint(c: Pick<Contest, "minTrades">, s: Pick<Standing, "trades" | "qualified">) {
  if (s.qualified || c.minTrades <= 0) return null;
  const n = Math.max(0, c.minTrades - s.trades);
  return n > 0 ? tr("rewards.contest.needsTrades", { count: n }) : tr("rewards.contest.qualifiesNext");
}

/* ------------------------------------------------------------------ */
/* Join flow                                                           */
/* ------------------------------------------------------------------ */

function Credentials({ c, creds }: { c: Contest; creds: NonNullable<JoinResult["credentials"]> }) {
  const t = useT();
  return (
    <div className="space-y-3" data-testid="contest-credentials">
      <div className="flex items-start gap-3 rounded-[14px] border border-warn/25 bg-warn-soft px-4 py-3 text-[12.5px] text-warn">
        <AlertTriangle className="mt-0.5 size-4 shrink-0" />
        <span>{t("rewards.join.saveNow")}</span>
      </div>
      <div className="divide-y divide-line rounded-[14px] border border-line bg-surface-2">
        {(
          [
            ["Login", t("rewards.join.login"), String(creds.login)],
            ["Password", t("rewards.join.password"), creds.password],
            ["Investor password", t("rewards.join.investorPassword"), creds.investorPassword],
            ["Server", t("rewards.join.server"), "Kalks-Demo"],
          ] as const
        ).map(([k, label, v]) => (
          <div key={k} className="flex items-center gap-3 px-4 py-2.5">
            <span className="w-36 shrink-0 text-[12.5px] text-fg-3">{label}</span>
            <span dir="ltr" className="min-w-0 flex-1 truncate text-start font-mono text-[13.5px]" data-testid={`credential-${k.toLowerCase().replace(/\s+/g, "-")}`}>
              {v}
            </span>
            <CopyButton value={v} label={label} />
          </div>
        ))}
      </div>
      <p className="text-[12px] text-fg-3">
        {t("rewards.join.accountFor", { name: c.name })}
        {c.startingBalance ? t("rewards.join.startingBalanceSuffix", { amount: fmtUsd(c.startingBalance, 0) }) : ""}
        {t("rewards.join.onlyThisAccount")}
      </p>
    </div>
  );
}

export function JoinContestButton({ c, onJoined, size = "lg", className }: { c: Contest; onJoined: () => void; size?: "sm" | "lg"; className?: string }) {
  const [open, setOpen] = React.useState(false);
  const [login, setLogin] = React.useState<number | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [creds, setCreds] = React.useState<JoinResult["credentials"] | null>(null);
  const live = c.kind === "live";
  const options = isOptionsContest(c);
  const t = useT();

  const join = async () => {
    if (live && !login) return;
    setBusy(true);
    try {
      const r = await growthApi<JoinResult>(`contests/${c.id}/join`, { body: live ? { login } : {} });
      onJoined();
      if (r.credentials) setCreds(r.credentials);
      else {
        setOpen(false);
        toast.success(t("rewards.join.toastJoined", { name: c.name }), { description: live ? t("rewards.join.toastLive", { login, date: fmtDate(c.startsAt) }) : t("rewards.join.toastDemo") });
      }
    } catch (e) {
      if (e instanceof GrowthApiError && e.code === "options_intro_required") {
        // the options intro is a 1-minute step in the Client Area (/options)
        toast.error(t("rewards.join.error"), { description: e.message, action: { label: t("rewards.options.openIntro"), onClick: () => window.location.assign("/options") } });
      } else errorToast(t("rewards.join.error"), e);
    } finally {
      setBusy(false);
    }
  };

  // options contests: copy-trading, PAMM, MAM and prop accounts never trade options
  const groupsOk = (g: string) => (c.accountGroups.length === 0 || c.accountGroups.includes(g)) && !(options && optionsSystemGroup(g));

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o);
        if (!o) {
          setCreds(null);
          setLogin(null);
        }
      }}
      width={500}
      trigger={
        <Button variant="ember" size={size} className={className} data-testid="contest-join">
          {isUpcoming(c) ? t("rewards.join.register") : t("rewards.join.join")} <ArrowUpRight />
        </Button>
      }
      title={creds ? t("rewards.join.credsTitle") : t("rewards.join.title", { name: c.name })}
      description={creds ? t("rewards.join.credsDesc") : live ? t("rewards.join.descLive") : t("rewards.join.descDemo")}
      footer={
        creds ? (
          <>
            <DialogClose asChild>
              <Button variant="ghost">{t("common.done")}</Button>
            </DialogClose>
            <a href={TERMINAL_URL} target="_blank" rel="noopener" data-testid="contest-open-terminal">
              <Button variant="ember">
                {t("rewards.join.openTerminal")} <ArrowUpRight />
              </Button>
            </a>
          </>
        ) : (
          <>
            <DialogClose asChild>
              <Button variant="ghost">{t("common.cancel")}</Button>
            </DialogClose>
            <Button variant="ember" disabled={busy || (live && !login)} onClick={join} data-testid="contest-join-confirm">
              {busy && <Loader2 className="animate-spin" />} {t("rewards.join.confirm")}
            </Button>
          </>
        )
      }
    >
      {creds ? (
        <Credentials c={c} creds={creds} />
      ) : (
        <div className="space-y-4">
          <KeyValue
            rows={[
              [t("rewards.join.runs"), `${fmtDate(c.startsAt)} – ${fmtDate(c.endsAt)}`],
              [t("rewards.join.rankedBy"), scoringLabel(c.scoring)],
              ...(options ? ([[t("rewards.detail.instrument"), t("rewards.detail.instrumentOptions")]] as [string, string][]) : []),
              ...(options && c.minPremium ? ([[t("rewards.detail.minPremium"), fmtUsd(c.minPremium)]] as [string, string][]) : []),
              [t("rewards.join.prizePool"), fmtUsd(c.prizePool, 0)],
              ...(c.minTrades > 0 ? ([[t("rewards.join.minTrades"), String(c.minTrades)]] as [string, string][]) : []),
              ...(!live && c.startingBalance ? ([[t("rewards.join.startingBalance"), fmtUsd(c.startingBalance, 0)]] as [string, string][]) : []),
              ...(live && c.minEquity ? ([[t("rewards.join.minEquity"), fmtUsd(c.minEquity, 0)]] as [string, string][]) : []),
            ]}
          />
          {live && (
            <div>
              <div className="k-label mb-2">{t("rewards.picker.label")}</div>
              <LiveAccountPicker
                value={login}
                onChange={setLogin}
                filter={(a) => groupsOk(a.group) && (c.minEquity === null || a.equity >= c.minEquity)}
                hint={c.minEquity ? (c.accountGroups.length ? t("rewards.join.equityHintGroups", { amount: fmtUsd(c.minEquity, 0), groups: c.accountGroups.join(", ") }) : t("rewards.join.equityHint", { amount: fmtUsd(c.minEquity, 0) })) : undefined}
              />
              {c.antiCheat.disqualifyOnBalanceChange && <p className="mt-2 text-[12px] text-fg-3">{t("rewards.join.balanceRule")}</p>}
            </div>
          )}
          {c.kycRequired && <p className="text-[12px] text-fg-3">{t("rewards.join.kycOnly")}</p>}
          {options && (
            <div className="rounded-[12px] border border-info/25 bg-info-soft px-3.5 py-2.5 text-[12.5px] leading-relaxed text-fg-2" data-testid="contest-options-note">
              <p>{t("rewards.options.joinNote")}</p>
              <p className="mt-1 text-fg-3">
                {t("rewards.options.eligibility")}{" "}
                <Link href="/options" className="font-medium text-info underline-offset-2 hover:underline">
                  {t("rewards.options.openIntro")}
                </Link>
              </p>
            </div>
          )}
        </div>
      )}
    </Dialog>
  );
}

/* ------------------------------------------------------------------ */
/* Leaderboard                                                         */
/* ------------------------------------------------------------------ */

const ROW_GRID = "grid grid-cols-[34px_minmax(0,1fr)_auto] items-center gap-3 sm:grid-cols-[40px_minmax(0,1.6fr)_70px_110px_100px_90px]";

function StandingRow({ c, s }: { c: Contest; s: Standing }) {
  const t = useT();
  const dq = s.status === "disqualified";
  const top = s.rank !== null && s.rank <= 3 && s.qualified && !dq;
  // only a qualified entry wins its rank's prize
  const prize = projectedPrize(c, s);
  const hint = tradesHint(c, s);
  return (
    <div
      className={cn(
        "k-row px-3 py-2.5 sm:px-4",
        ROW_GRID,
        top && !s.me && "border-gold/20 bg-gold-soft/40",
        s.me && "border-ember/40 bg-ember-soft",
        (dq || !s.qualified) && !s.me && "opacity-60",
      )}
      data-testid={s.me ? "leaderboard-row-me" : "leaderboard-row"}
    >
      <div className="flex justify-center">
        <RankBadge rank={dq ? null : s.rank} />
      </div>
      <div className="flex min-w-0 items-center gap-3">
        <div className="relative shrink-0">
          <Avatar name={s.name.replace(/[^\p{L}\s]/gu, "").trim() || "?"} size={34} />
          {s.country && <Flag country={s.country.toLowerCase()} className="absolute -bottom-1 -end-1 size-3.5 ring-2 ring-surface-2" />}
        </div>
        <div className="min-w-0">
          <div className="flex items-center gap-1.5 truncate text-[13.5px] font-medium">
            <span className="truncate">{s.me ? t("rewards.board.youName", { name: s.name }) : s.name}</span>
            {s.me && (
              <Chip size="sm" tone="ember">
                {t("rewards.you")}
              </Chip>
            )}
            {dq && <GrowthStatus status="disqualified" dot={false} />}
          </div>
          <div className="truncate text-[11.5px] text-fg-3">
            <span className="k-num">{t("rewards.value.trades", { count: s.trades })}</span>
            {hint && <span className="text-warn"> · {hint}</span>}
            {s.me && s.login && <span className="font-mono"> · #{s.login}</span>}
          </div>
        </div>
      </div>
      <div className="k-num hidden text-end text-[12.5px] text-fg-2 sm:block">{volumeText(c, s)}</div>
      <div className="text-end">
        <span className={cn("k-num text-[14px] font-semibold", scoreTone(c, s))}>{scoreText(c, s)}</span>
        <span className="k-num block text-[11px] text-fg-3 sm:hidden">{prize ? fmtUsd(prize, 0) : ""}</span>
      </div>
      <div className={cn("k-num hidden text-end text-[13px] sm:block", s.profit > 0 ? "text-up" : s.profit < 0 ? "text-down" : "text-fg-2")}>{fmtUsd(s.profit)}</div>
      <div className="hidden text-end sm:block">{prize && !dq ? <span className={cn("k-num text-[13.5px] font-semibold", top ? "text-gold" : "text-fg")}>{fmtUsd(prize, 0)}</span> : <span className="text-[12px] text-fg-3">—</span>}</div>
    </div>
  );
}

function Podium({ c, rows }: { c: Contest; rows: Standing[] }) {
  const t = useT();
  const ranked = rows.filter((r) => r.rank !== null && r.status !== "disqualified");
  if (ranked.length < 3) return null;
  const [first, second, third] = ranked;
  return (
    <div className="mt-5 grid grid-cols-3 items-end gap-2 px-4 sm:gap-3 sm:px-6">
      {[second!, first!, third!].map((r) => {
        const one = r === first;
        const prize = projectedPrize(c, r);
        return (
          <div key={String(r.entryId)} className={cn("flex flex-col items-center rounded-[18px] border px-2 pb-4 text-center", one ? "border-gold/30 bg-gold-soft/60 pt-5" : "border-line bg-surface-2 pt-4")}>
            <div className="relative">
              <Avatar name={r.name.replace(/[^\p{L}\s]/gu, "").trim() || "?"} size={one ? 56 : 46} />
              <span className="absolute -bottom-2 left-1/2 -translate-x-1/2">
                <RankBadge rank={r.rank} size={22} />
              </span>
            </div>
            <div className="mt-4 flex max-w-full items-center gap-1.5 text-[12.5px] font-medium">
              {r.country && <Flag country={r.country.toLowerCase()} className="size-3.5" />}
              <span className="truncate">{r.me ? t("rewards.you") : r.name}</span>
            </div>
            <div className={cn("k-num mt-1 font-semibold", one ? "text-[17px]" : "text-[14px]", scoreTone(c, r))}>{scoreText(c, r)}</div>
            {prize ? <div className={cn("k-num mt-0.5 text-[12px] font-semibold", one ? "text-gold" : "text-fg-2")}>{fmtUsd(prize, 0)}</div> : null}
          </div>
        );
      })}
    </div>
  );
}

export function Leaderboard({ d, limit, title, podium = true }: { d: ContestDetail; limit?: number; title?: string; podium?: boolean }) {
  const t = useT();
  const c = d.contest;
  const all = d.leaderboard;
  const shown = limit ? all.slice(0, limit) : all;
  const me = d.myEntry;
  const meShown = !!me && shown.some((s) => s.me);
  const zone = prizeZone(c);
  const lastRankShown = shown.reduce((m, s) => Math.max(m, s.rank ?? 0), 0);
  return (
    <Card className="flex h-full flex-col" data-testid="contest-leaderboard">
      <CardHeader
        title={title ?? t("rewards.board.title")}
        subtitle={
          <span>
            {isRunning(c) ? t("rewards.board.updates") : c.status === "scheduled" ? t("rewards.board.starts", { date: fmtDate(c.startsAt) }) : t("rewards.board.final")} · {t("rewards.board.traders", { count: d.entrants, n: fmtCount(d.entrants) })} · {t("rewards.board.rankedBy", { scoring: scoringLabel(c.scoring).toLowerCase() })}
          </span>
        }
        action={
          limit ? (
            <Link href={`/rewards/contests/${c.id}`}>
              <Button size="sm" variant="surface">
                {t("rewards.board.full")} <ChevronRight className="rtl:-scale-x-100" />
              </Button>
            </Link>
          ) : undefined
        }
      />
      {all.length === 0 ? (
        <div className="px-4 pb-6 pt-4 sm:px-6">
          <CardEmpty title={t("rewards.board.emptyTitle")} text={isUpcoming(c) ? t("rewards.board.emptyUpcoming") : t("rewards.board.emptyOpen")} />
        </div>
      ) : (
        <>
          {podium && <Podium c={c} rows={all} />}
          {/* the header is for the wide grid: ROW_GRID first, so tailwind-merge keeps it hidden on phones */}
          <div className={cn(ROW_GRID, "mt-5 hidden gap-3 px-8 text-[10.5px] font-medium uppercase tracking-[0.06em] text-fg-3 sm:grid")}>
            <span className="text-center">#</span>
            <span>{t("rewards.board.colTrader")}</span>
            <span className="text-end">{isOptionsContest(c) ? t("rewards.board.colContracts") : t("rewards.board.colLots")}</span>
            <span className="text-end">{scoringLabel(c.scoring)}</span>
            <span className="text-end">{t("rewards.board.colProfit")}</span>
            <span className="text-end">{t("rewards.board.colPrize")}</span>
          </div>
          <div className="mt-2 flex-1 space-y-1.5 px-4 pb-2 sm:px-6">
            {shown.map((s, i) => (
              <React.Fragment key={String(s.entryId)}>
                <StandingRow c={c} s={s} />
                {zone > 0 && s.rank === zone && i < shown.length - 1 && (
                  <div className="flex items-center gap-2 py-1.5 text-[11px] text-fg-3">
                    <span className="h-px flex-1 bg-line" /> {t("rewards.board.zoneEnds", { rank: zone })} <span className="h-px flex-1 bg-line" />
                  </div>
                )}
              </React.Fragment>
            ))}
          </div>
          {me && !meShown && (
            <div className="space-y-1.5 px-4 pb-2 sm:px-6">
              <div className="flex items-center gap-2 py-1.5 text-[11px] text-fg-3">
                <span className="h-px flex-1 bg-line" /> {zone > lastRankShown ? t("rewards.board.zoneTo", { rank: zone }) : t("rewards.board.yourPosition")} <span className="h-px flex-1 bg-line" />
              </div>
              <StandingRow c={c} s={me} />
            </div>
          )}
          <div className="pb-4" />
        </>
      )}
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Prize card                                                          */
/* ------------------------------------------------------------------ */

export function PrizeCard({ c }: { c: Contest }) {
  const max = Math.max(1, ...c.prizes.map((p) => p.amount));
  const wallet = c.prizes.every((p) => p.payout === "wallet");
  const t = useT();
  return (
    <Card className="flex h-full flex-col">
      <CardHeader title={t("rewards.prize.title")} subtitle={wallet ? t("rewards.prize.wallet") : t("rewards.prize.mixed")} action={<Icon3D name="money_bag" size={36} />} />
      <div className="mt-4 flex-1 space-y-1.5 px-4 pb-5 sm:px-6">
        {c.prizes.length === 0 && <CardEmpty title={t("rewards.prize.noneTitle")} text={t("rewards.prize.noneText")} />}
        {c.prizes.map((p, i) => (
          <div key={`${p.rankFrom}-${p.rankTo}`} className="flex items-center gap-3">
            <span className="k-num w-14 shrink-0 text-[12px] font-medium text-fg-2">{bandLabel(p)}</span>
            <div className="relative h-7 flex-1 overflow-hidden rounded-full bg-surface-2">
              <div className={cn("absolute inset-y-0 start-0 rounded-full", i === 0 ? "bg-gold" : i < 3 ? "bg-gold/35" : "bg-surface-3")} style={{ width: `${Math.max(14, (p.amount / max) * 100)}%` }} />
              <span className={cn("k-num relative flex h-full items-center px-3 text-[12px] font-semibold", i === 0 ? "text-[#1a1204]" : "text-fg")}>
                {fmtUsd(p.amount, 0)}
                {p.rankTo > p.rankFrom ? ` ${t("rewards.value.each")}` : ""}
              </span>
            </div>
            {!wallet && <span className="w-12 text-end text-[10.5px] uppercase tracking-wider text-fg-3">{p.payout === "credit" ? t("rewards.prize.payout.credit") : p.payout === "wallet" ? t("rewards.prize.payout.wallet") : p.payout}</span>}
          </div>
        ))}
      </div>
      <div className="grid grid-cols-2 gap-2 border-t border-line px-4 py-4 sm:px-6">
        <div className="k-row px-3 py-2.5">
          <div className="text-[11.5px] text-fg-3">{t("rewards.prize.pool")}</div>
          <div className="k-num mt-0.5 text-[13px] font-medium text-gold">{fmtUsd(c.prizePool, 0)}</div>
        </div>
        <div className="k-row px-3 py-2.5">
          <div className="text-[11.5px] text-fg-3">{t("rewards.prize.minTrades")}</div>
          <div className="k-num mt-0.5 text-[13px] font-medium">{c.minTrades || t("rewards.prize.none")}</div>
        </div>
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Hero (featured running / next contest)                              */
/* ------------------------------------------------------------------ */

function ContestHero({ c, d, onJoined }: { c: ContestCard; d: ContestDetail | null; onJoined: () => void }) {
  const t = useT();
  const running = isRunning(c);
  const me = d?.myEntry ?? c.myEntry;
  const start = Date.parse(c.startsAt);
  const end = Date.parse(c.endsAt);
  const [now, setNow] = React.useState<number | null>(null);
  React.useEffect(() => {
    setNow(Date.now());
    const t = setInterval(() => setNow(Date.now()), 60_000);
    return () => clearInterval(t);
  }, []);
  const pctTime = now === null ? 0 : Math.min(100, Math.max(0, ((now - start) / Math.max(1, end - start)) * 100));
  const dayN = now === null ? 0 : Math.max(1, Math.ceil((now - start) / 86400_000));
  const days = Math.max(1, Math.round((end - start) / 86400_000));
  const zone = prizeZone(c);
  const entrants = d?.entrants ?? c.entrants;
  const hint = me ? tradesHint(c, me) : null;
  return (
    <Card hot className="relative overflow-hidden" data-testid="contest-hero">
      <div className="relative grid grid-cols-1 gap-6 p-5 sm:p-7 xl:grid-cols-12">
        <div className="xl:col-span-7">
          <div className="flex flex-wrap items-center gap-2">
            <GrowthStatus status={c.status} />
            {kindChip(c)}
            <Chip>
              {fmtDate(c.startsAt, false)} – {fmtDate(c.endsAt)}
            </Chip>
          </div>
          <h2 className="mt-4 text-[28px] font-medium leading-tight tracking-[-0.02em] sm:text-[34px]">{c.name}</h2>
          {c.description && <p className="mt-1.5 max-w-lg text-[14px] text-fg-2">{c.description}</p>}

          <div className="mt-6 flex flex-wrap items-end gap-x-8 gap-y-5">
            <div>
              <div className="k-label">{t("rewards.prize.pool")}</div>
              <Money value={c.prizePool} decimals={0} countUp={false} className="mt-1.5 block text-[40px] font-semibold leading-none tracking-tight text-gold sm:text-[44px]" />
            </div>
            <div>
              <div className="k-label mb-2 flex items-center gap-1.5">
                <Timer className="size-3.5" /> {running ? t("rewards.hero.endsIn") : t("rewards.hero.startsIn")}
              </div>
              <Countdown to={running ? c.endsAt : c.startsAt} />
            </div>
          </div>

          <div className="mt-6 flex flex-wrap items-center gap-3">
            {me ? (
              <Button variant="up-outline" size="lg" disabled className="disabled:opacity-100">
                <Check /> {me.login ? t("rewards.hero.joinedAccount", { login: me.login }) : t("rewards.hero.joined")}
              </Button>
            ) : canJoin(c) ? (
              <JoinContestButton c={c} onJoined={onJoined} />
            ) : (
              <Button size="lg" variant="surface" disabled>
                {t("rewards.hero.entriesClosed")}
              </Button>
            )}
            {me && (
              <a href={TERMINAL_URL} target="_blank" rel="noopener">
                <Button variant="surface" size="lg">
                  {t("rewards.hero.trade")}
                </Button>
              </a>
            )}
            <Link href={`/rewards/contests/${c.id}`}>
              <Button variant="ghost" size="lg">
                {t("rewards.hero.rules")}
              </Button>
            </Link>
          </div>
        </div>

        <div className="xl:col-span-5">
          <div className="rounded-[18px] border border-line bg-surface-2/70 p-5">
            {me ? (
              <>
                <div className="flex items-center justify-between">
                  <div>
                    <div className="text-[14px] font-medium">{t("rewards.hero.standing")}</div>
                    <div className="text-[12px] text-fg-3">{me.rank ? t("rewards.hero.rankOf", { rank: me.rank, count: fmtCount(entrants) }) : me.status === "disqualified" ? t("rewards.hero.dq") : t("rewards.hero.notRanked")}</div>
                  </div>
                  {me.status === "disqualified" ? <GrowthStatus status="disqualified" /> : projectedPrize(c, me) !== null ? <Chip tone="gold">{t("rewards.hero.prizeZone")}</Chip> : null}
                </div>
                <div className="mt-5 grid grid-cols-3 gap-2">
                  <div className="k-row px-3 py-3">
                    <div className="text-[11.5px] text-fg-3">{t("rewards.hero.rank")}</div>
                    <div className="k-num mt-1 text-[20px] font-semibold leading-none">{me.rank ? `#${me.rank}` : "—"}</div>
                  </div>
                  <div className="k-row px-3 py-3">
                    <div className="text-[11.5px] text-fg-3">{scoringLabel(c.scoring)}</div>
                    <div className={cn("k-num mt-1 text-[17px] font-semibold leading-none", scoreTone(c, me))}>{scoreText(c, me)}</div>
                  </div>
                  <div className="k-row px-3 py-3">
                    <div className="text-[11.5px] text-fg-3">{t("rewards.hero.trades")}</div>
                    <div className="k-num mt-1 text-[20px] font-semibold leading-none">{me.trades}</div>
                  </div>
                </div>
              </>
            ) : (
              <>
                <div className="text-[14px] font-medium">{running ? t("rewards.hero.joinRunning") : t("rewards.hero.registerEarly")}</div>
                <div className="mt-1 text-[12.5px] text-fg-3">
                  {c.kind === "demo" ? (c.startingBalance ? t("rewards.hero.demoTextBalance", { amount: fmtUsd(c.startingBalance, 0) }) : t("rewards.hero.demoText")) : t("rewards.hero.liveText")}
                </div>
                <div className="mt-5 grid grid-cols-2 gap-2">
                  <div className="k-row flex items-center gap-2 px-3 py-2.5 text-[12.5px]">
                    <Users className="size-3.5 text-fg-3" />
                    <span className="k-num">{t("rewards.value.joined", { count: `${fmtCount(entrants)}${c.maxEntrants ? ` / ${fmtCount(c.maxEntrants)}` : ""}` })}</span>
                  </div>
                  <div className="k-row flex items-center gap-2 px-3 py-2.5 text-[12.5px]">
                    <Trophy className="size-3.5 text-fg-3" />
                    <span>{zone ? t("rewards.hero.topPaid", { count: zone }) : t("rewards.hero.rankingOnly")}</span>
                  </div>
                </div>
              </>
            )}
            <div className="mt-4 space-y-3 text-[12px]">
              <div>
                <div className="mb-1.5 flex justify-between text-fg-3">
                  <span>{t("rewards.hero.progress")}</span>
                  <span className="k-num text-fg-2">{running ? t("rewards.hero.dayOf", { day: Math.min(dayN, days), days }) : t("rewards.value.days", { count: days })}</span>
                </div>
                <div className="h-1.5 overflow-hidden rounded-full bg-surface-3">
                  <div className="h-full rounded-full bg-ember" style={{ width: `${running ? pctTime : 0}%` }} />
                </div>
              </div>
              {hint && (
                <div className="flex items-center justify-between rounded-[12px] border border-warn/25 bg-warn-soft px-3 py-2 text-warn">
                  <span>{hint}</span>
                  <ShieldAlert className="size-3.5 shrink-0" />
                </div>
              )}
              {me && me.status !== "disqualified" && !hint && me.rank && zone > 0 && me.rank > zone && (
                <div className="flex items-center justify-between rounded-[12px] border border-gold/25 bg-gold-soft px-3 py-2 text-gold">
                  <span>{t("rewards.hero.placesToZone", { count: me.rank - zone, rank: zone })}</span>
                  <Trophy className="size-3.5 shrink-0" />
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Cards                                                               */
/* ------------------------------------------------------------------ */

function ContestTile({ c, onJoined }: { c: ContestCard; onJoined: () => void }) {
  const fill = c.maxEntrants ? Math.min(100, (c.entrants / c.maxEntrants) * 100) : null;
  const past = isPast(c);
  const me = c.myEntry;
  const t = useT();
  return (
    <Card className="flex flex-col overflow-hidden" data-testid="contest-card">
      <div className="border-b border-line bg-surface-2/60 px-5 pb-4 pt-4">
        <div className="flex items-center gap-1.5">
          {kindChip(c)}
          <GrowthStatus status={c.status} />
        </div>
        <div className="mt-3 flex items-end justify-between gap-3">
          <div>
            <div className="text-[11.5px] text-fg-3">{t("rewards.tile.prizePool")}</div>
            <div className="k-num text-[24px] font-semibold leading-tight text-gold">{fmtUsd(c.prizePool, 0)}</div>
          </div>
          {!past && (
            <Chip>
              <Timer className="size-3" /> <Countdown to={isRunning(c) ? c.endsAt : c.startsAt} compact className="text-[11px]" />
            </Chip>
          )}
        </div>
      </div>
      <div className="flex flex-1 flex-col px-5 pb-5 pt-3">
        <Link href={`/rewards/contests/${c.id}`} className="text-[16px] font-medium tracking-tight hover:underline hover:underline-offset-2">
          {c.name}
        </Link>
        {c.description && <div className="mt-0.5 line-clamp-2 text-[12.5px] text-fg-3">{c.description}</div>}
        <div className="mt-4 grid grid-cols-[minmax(0,1fr)_auto] gap-2 text-[12px]">
          <div className="k-row flex min-w-0 items-center gap-2 px-3 py-2">
            <CalendarDays className="size-3.5 shrink-0 text-fg-3" />
            <span className="k-num truncate whitespace-nowrap">
              {fmtDate(c.startsAt, false)} – {fmtDate(c.endsAt, false)}
            </span>
          </div>
          <div className="k-row flex items-center gap-2 px-3 py-2">
            <Users className="size-3.5 shrink-0 text-fg-3" />
            <span className="k-num">{t("rewards.value.joined", { count: fmtCount(c.entrants) })}</span>
          </div>
        </div>
        <div className="mt-2 line-clamp-2 text-[11.5px] leading-relaxed text-fg-3">
          {t("rewards.tile.rankedBy", { scoring: scoringLabel(c.scoring).toLowerCase() })}
          {c.minTrades ? t("rewards.tile.minTrades", { count: c.minTrades }) : ""}
          {c.kind === "demo" && c.startingBalance ? t("rewards.tile.demoBalance", { amount: fmtUsd(c.startingBalance, 0) }) : ""}
          {c.kind === "live" && c.minEquity ? t("rewards.tile.minEquity", { amount: fmtUsd(c.minEquity, 0) }) : ""}
        </div>
        {fill !== null && !past && (
          <div className="mt-3">
            <div className="mb-1 flex justify-between text-[11px] text-fg-3">
              <span>{t("rewards.tile.seats")}</span>
              <span className="k-num">
                {fmtCount(c.entrants)} / {fmtCount(c.maxEntrants!)}
              </span>
            </div>
            <div className="h-1.5 overflow-hidden rounded-full bg-surface-3">
              <div className="h-full rounded-full bg-ember" style={{ width: `${fill}%` }} />
            </div>
          </div>
        )}
        {me && (
          <div className="k-row mt-3 flex items-center gap-3 px-3 py-2.5">
            <RankBadge rank={me.status === "disqualified" ? null : me.rank} size={26} />
            <div className="min-w-0 flex-1 text-[12.5px]">
              <div className="font-medium">{past ? t("rewards.tile.yourResult") : t("rewards.tile.youreIn")}</div>
              <div className="truncate text-fg-3">{me.status === "disqualified" ? t("rewards.status.disqualified") : `${scoreText(c, me)} · ${t("rewards.value.trades", { count: me.trades })}`}</div>
            </div>
            {me.prize ? <span className="k-num text-[13px] font-semibold text-gold">{fmtUsd(me.prize, 0)}</span> : null}
          </div>
        )}
        <div className="mt-auto flex items-center gap-2 pt-4">
          {!me && canJoin(c) ? <JoinContestButton c={c} onJoined={onJoined} size="sm" className="flex-1" /> : null}
          <Link href={`/rewards/contests/${c.id}`} className={cn(!me && canJoin(c) ? "" : "flex-1")}>
            <Button size="sm" variant="surface" className="w-full">
              {past ? t("rewards.tile.results") : t("rewards.tile.details")}
            </Button>
          </Link>
        </div>
      </div>
    </Card>
  );
}

function RewardsShortcuts() {
  const rewards = useGrowth<Rewards>("rewards");
  const cash = useGrowth<CashbackMe>("cashback");
  const r = rewards.data;
  const t = useT();
  const items = [
    {
      href: "/rewards/loyalty",
      icon: "gem_stone",
      title: r ? t("rewards.shortcuts.points", { points: fmtPoints(r.points.balance) }) : t("rewards.shortcuts.loyalty"),
      sub: r ? t("rewards.shortcuts.tierValue", { tier: r.tier.name, value: fmtUsd(r.points.balance * r.pointValue) }) : t("rewards.shortcuts.loyaltySub"),
      chip: t("rewards.shortcuts.redeem"),
    },
    {
      href: "/rewards/cashback",
      icon: "money_with_wings",
      title: cash.data ? t("rewards.shortcuts.cashbackValue", { amount: fmtUsd(cash.data.totals.lifetime) }) : t("rewards.shortcuts.cashback"),
      sub: cash.data ? t("rewards.shortcuts.cashbackPending", { amount: fmtUsd(cash.data.totals.accrued) }) : t("rewards.shortcuts.cashbackSub"),
      chip: t("rewards.shortcuts.view"),
    },
    { href: "/rewards/promotions", icon: "wrapped_gift", title: t("rewards.shortcuts.promotions"), sub: t("rewards.shortcuts.promotionsSub"), chip: t("rewards.shortcuts.open") },
  ];
  return (
    <Card className="flex h-full flex-col">
      <CardHeader title={t("rewards.shortcuts.title")} subtitle={t("rewards.shortcuts.subtitle")} />
      <div className="mt-4 flex-1 space-y-2 px-4 pb-5 sm:px-6">
        {items.map((it) => (
          <Link key={it.href} href={it.href} className="k-row flex items-center gap-3 px-3.5 py-3 transition-colors hover:bg-surface-3/60">
            <Icon3D name={it.icon} size={36} />
            <div className="min-w-0 flex-1">
              <div className="k-num text-[14px] font-medium">{it.title}</div>
              <div className="truncate text-[11.5px] text-fg-3">{it.sub}</div>
            </div>
            <Chip size="sm" tone="ember">
              {it.chip}
            </Chip>
          </Link>
        ))}
      </div>
    </Card>
  );
}

function MyResults({ items }: { items: ContestCard[] }) {
  const rows = items.filter((c) => c.myEntry);
  const t = useT();
  const columns: Column<ContestCard>[] = [
    {
      key: "name",
      header: t("rewards.results.colContest"),
      cell: (c) => (
        <Link href={`/rewards/contests/${c.id}`} className="flex items-center gap-2 font-medium hover:underline hover:underline-offset-2">
          {kindChip(c)} {c.name}
        </Link>
      ),
    },
    { key: "dates", header: t("rewards.results.colDates"), hideOn: "md", cell: (c) => <span className="k-num text-fg-2">{`${fmtDate(c.startsAt, false)} – ${fmtDate(c.endsAt)}`}</span>, sort: (c) => c.startsAt },
    { key: "status", header: t("common.status"), cell: (c) => <GrowthStatus status={c.myEntry?.status === "disqualified" ? "disqualified" : c.status} /> },
    { key: "rank", header: t("rewards.results.colRank"), align: "right", cell: (c) => <span className="k-num font-medium">{c.myEntry?.rank ? `#${c.myEntry.rank}` : "—"}</span>, sort: (c) => c.myEntry?.rank ?? 99999 },
    { key: "score", header: t("rewards.results.colScore"), align: "right", cell: (c) => <span className={cn("k-num", scoreTone(c, c.myEntry!))}>{scoreText(c, c.myEntry!)}</span> },
    { key: "trades", header: t("rewards.results.colTrades"), align: "right", hideOn: "sm", cell: (c) => <span className="k-num text-fg-2">{c.myEntry!.trades}</span> },
    {
      key: "prize",
      header: t("rewards.results.colPrize"),
      align: "right",
      cell: (c) =>
        c.myEntry?.prize ? (
          <span className="flex items-center justify-end gap-2">
            <span className="k-num font-semibold text-gold">{fmtUsd(c.myEntry.prize, 0)}</span>
            {c.myEntry.prizeStatus && <GrowthStatus status={c.myEntry.prizeStatus} dot={false} />}
          </span>
        ) : (
          <span className="text-fg-3">—</span>
        ),
    },
  ];
  return (
    <Card id="my-results" className="scroll-mt-24">
      <CardHeader title={t("rewards.results.title")} subtitle={t("rewards.results.subtitle")} icon={<Medal />} />
      <div className="px-4 pb-6 pt-4 sm:px-6">
        {rows.length === 0 ? <CardEmpty title={t("rewards.results.emptyTitle")} text={t("rewards.results.emptyText")} /> : <DataTable columns={columns} rows={rows} pageSize={8} rowKey={(c) => String(c.id)} />}
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Page                                                                */
/* ------------------------------------------------------------------ */

function pickFeatured(items: ContestCard[]) {
  const running = items.filter(isRunning).sort((a, b) => Number(!!b.myEntry) - Number(!!a.myEntry) || b.prizePool - a.prizePool);
  if (running[0]) return running[0];
  return items.filter(isUpcoming).sort((a, b) => Date.parse(a.startsAt) - Date.parse(b.startsAt))[0] ?? null;
}

export function LiveContestsPage() {
  const t = useT();
  const list = useGrowth<ContestsResp>("contests", 60_000);
  const featured = list.data ? pickFeatured(list.data.items) : null;
  const detail = useGrowth<ContestDetail>(featured ? `contests/${featured.id}` : null, 15_000);
  const reload = () => {
    list.reload();
    detail.reload();
  };

  const title = t("rewards.contests.title");
  const subtitle = t("rewards.contests.subtitle");
  if (!list.data)
    return <PageFallback title={title} subtitle={subtitle} error={list.error} onRetry={list.reload} top={<BannerSlot placement="rewards" />} rows={[{ cols: "", h: "h-[320px]", n: 1 }, { cols: "sm:grid-cols-2 xl:grid-cols-4", h: "h-[150px]", n: 4 }]} />;

  const { items, stats } = list.data;
  const upcoming = items.filter((c) => c !== featured && (isUpcoming(c) || isRunning(c)));
  const past = items.filter(isPast).filter((c) => c.status !== "cancelled" || c.myEntry);
  const d = detail.data && featured && detail.data.contest.id === featured.id ? detail.data : null;

  return (
    <div className="pb-16">
      <PageHero page="rewards" overlap
        title={title}
        lead={subtitle}
        actions={
          <a href="#my-results">
            <Button variant="surface">
              <Medal /> {t("rewards.results.title")}
            </Button>
          </a>
        }
      />
      <BannerSlot placement="rewards" />

      {featured ? (
        <Reveal>
          <ContestHero c={featured} d={d} onJoined={reload} />
        </Reveal>
      ) : (
        <Card>
          <div className="flex flex-col items-center px-6 py-12 text-center">
            <Illustration name="rewards" width={208} maxHeight={156} />
            <h3 className="mt-6 text-[17px] font-medium">{t("rewards.contests.noneTitle")}</h3>
            <p className="mt-1 max-w-md text-[13.5px] text-fg-3">{t("rewards.contests.noneText")}</p>
          </div>
        </Card>
      )}

      <div className="mt-4 grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <KpiCard label={t("rewards.contests.kpiEntered")} icon={<Medal />} value={<span className="k-num">{stats.entered}</span>} chip={t("rewards.contests.kpiPrizeFinishes", { count: stats.prizeFinishes })} chipTone="gold" delay={0.05} />
        <KpiCard label={t("rewards.contests.kpiPrizesWon")} icon={<Gift />} value={<Money value={stats.prizesWon} countUp={false} />} chip={t("rewards.contests.kpiPaidToWallet")} chipTone="up" delay={0.1} />
        <KpiCard label={t("rewards.contests.kpiBest")} icon={<Trophy />} value={<span className="k-num">{stats.bestRank ? `#${stats.bestRank}` : "—"}</span>} chip={stats.bestRank ? t("rewards.contests.kpiAcross") : t("rewards.contests.kpiNotRanked")} delay={0.15} />
        <KpiCard label={t("rewards.contests.kpiActive")} value={<span className="k-num">{stats.active}</span>} hot illustration="trophy" chip={t("rewards.contests.kpiUpcoming", { count: items.filter(isUpcoming).length })} chipTone="ember" delay={0.2} />
      </div>

      {featured && (
        <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
          <Reveal delay={0.1} className="xl:col-span-8">
            {d ? <Leaderboard d={d} limit={10} /> : <Card className="h-full min-h-[360px]" />}
          </Reveal>
          <div className="flex flex-col gap-4 xl:col-span-4">
            <Reveal delay={0.15}>
              <PrizeCard c={featured} />
            </Reveal>
            <Reveal delay={0.2} className="flex-1">
              <RewardsShortcuts />
            </Reveal>
          </div>
        </div>
      )}

      {upcoming.length > 0 && (
        <Reveal delay={0.1} className="mt-8">
          <SectionTitle title={t("rewards.contests.openTitle")} text={t("rewards.contests.openText")} />
          <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
            {upcoming.map((c) => (
              <ContestTile key={String(c.id)} c={c} onJoined={reload} />
            ))}
          </div>
        </Reveal>
      )}

      {past.length > 0 && (
        <Reveal delay={0.1} className="mt-8">
          <SectionTitle title={t("rewards.contests.pastTitle")} text={t("rewards.contests.pastText")} />
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
            {past.slice(0, 8).map((c) => (
              <ContestTile key={String(c.id)} c={c} onJoined={reload} />
            ))}
          </div>
        </Reveal>
      )}

      <Reveal delay={0.1} className="mt-8">
        <MyResults items={items} />
      </Reveal>
    </div>
  );
}
