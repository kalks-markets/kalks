"use client";

import * as React from "react";
import Link from "next/link";
import { ArrowLeft, ArrowUpRight, Check, ListChecks, ShieldAlert, Timer, Trophy, Users } from "lucide-react";
import { Button, Card, CardHeader, Chip, KeyValue, Money, PageHeader, Reveal, cn } from "@/components/kit";
import { TERMINAL_URL } from "@/lib/live";
import { Countdown } from "@/components/rewards/countdown";
import { useT } from "@kalks/i18n/react";
import { bandLabel, fmtCount, fmtDate, fmtDateTime, fmtPct, fmtUsd, isOptionsContest, projectedPrize, scoringLabel, useGrowth, type ContestDetail } from "./api";
import { JoinContestButton, Leaderboard, canJoin, isPast, isRunning, isUpcoming, kindChip, scoreText, tradesHint, volumeText } from "./contests";
import { CardEmpty, GrowthStatus, PageFallback, RankBadge } from "./ui";
import { useRewardsLive } from "./challenges-api";

function Stat({ label, value, className }: { label: string; value: React.ReactNode; className?: string }) {
  return (
    <div className="k-row px-3.5 py-3">
      <div className="text-[11.5px] text-fg-3">{label}</div>
      <div className={cn("k-num mt-1 text-[18px] font-semibold leading-none", className)}>{value}</div>
    </div>
  );
}

function MyEntry({ d }: { d: ContestDetail }) {
  const t = useT();
  const c = d.contest;
  const me = d.myEntry;
  if (!me)
    return (
      <Card className="h-full">
        <CardHeader title={t("rewards.detail.entryTitle")} subtitle={t("rewards.detail.notJoined")} />
        <div className="px-4 pb-6 pt-4 sm:px-6">
          <CardEmpty title={canJoin(c) ? t("rewards.detail.joinToRank") : isPast(c) ? t("rewards.detail.ended") : t("rewards.detail.entriesClosed")} text={canJoin(c) ? t("rewards.detail.joinText") : undefined} />
        </div>
      </Card>
    );
  const dq = me.status === "disqualified";
  const hint = tradesHint(c, me);
  const options = isOptionsContest(c);
  const good = (v: number) => (v > 0 ? "text-up" : v < 0 ? "text-down" : "");
  return (
    <Card className="h-full" data-testid="contest-my-entry">
      <CardHeader
        title={t("rewards.detail.entryTitle")}
        subtitle={me.login ? t("rewards.detail.accountUpdated", { login: me.login, date: fmtDateTime(me.updatedAt) }) : t("rewards.detail.updated", { date: fmtDateTime(me.updatedAt) })}
        action={dq ? <GrowthStatus status="disqualified" /> : projectedPrize(c, me) !== null ? <Chip tone="gold">{t("rewards.hero.prizeZone")}</Chip> : null}
      />
      <div className="space-y-3 px-4 pb-6 pt-4 sm:px-6">
        {dq && (
          <div className="flex items-start gap-3 rounded-[14px] border border-down/30 bg-down-soft px-4 py-3 text-[12.5px] text-down" data-testid="contest-disqualified">
            <ShieldAlert className="mt-0.5 size-4 shrink-0" />
            <span>{t("rewards.detail.dqText")}</span>
          </div>
        )}
        <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
          <Stat label={t("rewards.hero.rank")} value={dq ? "—" : me.rank ? `#${me.rank}` : "—"} />
          <Stat label={scoringLabel(c.scoring)} value={scoreText(c, me)} className={c.scoring === "lots" ? "" : good(c.scoring === "profit" ? me.profit : me.returnPct)} />
          <Stat label={t("rewards.detail.return")} value={fmtPct(+me.returnPct.toFixed(2), true)} className={good(me.returnPct)} />
          <Stat label={t("rewards.detail.profit")} value={fmtUsd(me.profit)} className={good(me.profit)} />
          <Stat label={t("rewards.hero.trades")} value={me.trades} />
          <Stat label={options ? t("rewards.detail.contracts") : t("rewards.detail.lots")} value={volumeText(c, me)} />
        </div>
        {options && !!me.selfTrades && (
          <div className="rounded-[12px] border border-warn/25 bg-warn-soft px-3 py-2 text-[12.5px] text-warn" data-testid="contest-self-trades">
            {t("rewards.options.excludedSelf", { count: me.selfTrades })}
          </div>
        )}
        {options && !!me.smallTrades && (
          <div className="rounded-[12px] border border-line bg-surface-2 px-3 py-2 text-[12.5px] text-fg-2" data-testid="contest-small-trades">
            {t("rewards.options.excludedSmall", { count: me.smallTrades, amount: fmtUsd(c.minPremium ?? 0) })}
          </div>
        )}
        {hint && !dq && (
          <div className="flex items-center justify-between rounded-[12px] border border-warn/25 bg-warn-soft px-3 py-2 text-[12.5px] text-warn" data-testid="contest-trades-hint">
            <span>{t("rewards.detail.belowMin", { hint })}</span>
          </div>
        )}
        {me.prize ? (
          <div className="flex items-center justify-between rounded-[12px] border border-gold/25 bg-gold-soft px-3 py-2 text-[12.5px] text-gold">
            <span>
              {t("rewards.detail.prize", { amount: fmtUsd(me.prize, 0) })}
              {me.prizeStatus ? ` · ${t.dyn(`rewards.status.${me.prizeStatus}`, me.prizeStatus.replace(/_/g, " ")).toLowerCase()}` : ""}
            </span>
            <Trophy className="size-3.5 shrink-0" />
          </div>
        ) : null}
        {isRunning(c) && !dq && (
          <a href={TERMINAL_URL} target="_blank" rel="noopener" className="block">
            <Button variant="surface" className="w-full">
              {t("rewards.hero.trade")} <ArrowUpRight />
            </Button>
          </a>
        )}
      </div>
    </Card>
  );
}

function Rules({ d }: { d: ContestDetail }) {
  const t = useT();
  const c = d.contest;
  const lines = c.rules
    .split(/\n+/)
    .map((l) => l.replace(/^\s*[-*•]\s*/, "").trim())
    .filter(Boolean);
  const options = isOptionsContest(c);
  const ranked = options
    ? t(c.scoring === "return_pct" ? "rewards.detail.ruleRankedReturnOptions" : c.scoring === "profit" ? "rewards.detail.ruleRankedProfitOptions" : "rewards.detail.ruleRankedContracts")
    : t(c.scoring === "return_pct" ? "rewards.detail.ruleRankedReturn" : c.scoring === "profit" ? "rewards.detail.ruleRankedProfit" : "rewards.detail.ruleRankedLots", { scoring: scoringLabel(c.scoring).toLowerCase() });
  // options contests (O36): what counts, the minimum premium, self-trades, who may join, no loyalty / cashback
  const optionRules = options
    ? [
        t("rewards.detail.ruleOptionsOnly"),
        t("rewards.detail.ruleOptionsRealised"),
        c.minPremium ? t("rewards.detail.ruleMinPremium", { amount: fmtUsd(c.minPremium) }) : null,
        t("rewards.detail.ruleSelfTrade"),
        t("rewards.detail.ruleOptionsEligibility"),
        t("rewards.detail.ruleOptionsNoRewards"),
      ]
    : [];
  const auto = [
    ranked,
    ...optionRules,
    t("rewards.detail.ruleWindow"),
    c.minTrades > 0 ? t("rewards.detail.ruleMinTrades", { count: c.minTrades }) : null,
    t("rewards.detail.ruleTies"),
    c.antiCheat.disqualifyOnBalanceChange ? t("rewards.detail.ruleBalanceDq") : t("rewards.detail.ruleBalanceReview"),
    c.antiCheat.minHoldSeconds > 0 ? t("rewards.detail.ruleHold", { seconds: c.antiCheat.minHoldSeconds }) : null,
    c.antiCheat.maxSingleTradePct > 0 ? t("rewards.detail.ruleMaxTrade", { pct: c.antiCheat.maxSingleTradePct }) : null,
  ].filter((x): x is string => !!x);
  return (
    <Card className="h-full">
      <CardHeader title={t("rewards.detail.rulesTitle")} subtitle={t("rewards.detail.rulesSubtitle")} icon={<ListChecks />} />
      <ul className="space-y-2 px-4 pb-4 pt-4 sm:px-6">
        {[...lines, ...auto].map((r, i) => (
          <li key={i} className="k-row flex items-start gap-2.5 px-3.5 py-2.5 text-[13px] text-fg-2">
            <Check className="mt-0.5 size-3.5 shrink-0 text-up" />
            {r}
          </li>
        ))}
      </ul>
      <div className="px-4 pb-5 sm:px-6">
        <KeyValue
          rows={[
            [t("rewards.detail.type"), c.kind === "demo" ? (c.startingBalance ? t("rewards.detail.typeDemoBalance", { amount: fmtUsd(c.startingBalance, 0) }) : t("rewards.detail.typeDemo")) : t("rewards.detail.typeLive")],
            [t("rewards.detail.instrument"), options ? t("rewards.detail.instrumentOptions") : t("rewards.detail.instrumentCfd")],
            ...(options && c.minPremium ? ([[t("rewards.detail.minPremium"), fmtUsd(c.minPremium)]] as [string, string][]) : []),
            [t("rewards.detail.window"), `${fmtDateTime(c.startsAt)} – ${fmtDateTime(c.endsAt)}`],
            ...(c.accountGroups.length ? ([[t("rewards.detail.accountTypes"), c.accountGroups.join(", ")]] as [string, string][]) : []),
            ...(c.minEquity ? ([[t("rewards.detail.minEquity"), fmtUsd(c.minEquity, 0)]] as [string, string][]) : []),
            [t("rewards.detail.verification"), c.kycRequired ? t("rewards.detail.verifiedOnly") : t("rewards.detail.notRequired")],
            [t("rewards.detail.seats"), c.maxEntrants ? `${fmtCount(d.entrants)} / ${fmtCount(c.maxEntrants)}` : t("rewards.detail.seatsUnlimited", { count: fmtCount(d.entrants) })],
          ]}
        />
      </div>
    </Card>
  );
}

function PrizesTable({ d }: { d: ContestDetail }) {
  const t = useT();
  const c = d.contest;
  return (
    <Card className="h-full">
      <CardHeader title={t("rewards.detail.prizesTitle")} subtitle={t("rewards.detail.prizesSubtitle", { amount: fmtUsd(c.prizePool, 0) })} icon={<Trophy />} />
      <div className="px-4 pb-6 pt-4 sm:px-6">
        {c.prizes.length === 0 ? (
          <CardEmpty title={t("rewards.prize.noneTitle")} text={t("rewards.prize.noneText")} />
        ) : (
          <div className="overflow-hidden rounded-[14px] border border-line">
            <table className="w-full text-[13px]">
              <thead>
                <tr className="bg-surface-2 text-[11px] uppercase tracking-[0.05em] text-fg-3">
                  <th className="px-4 py-2.5 text-start font-medium">{t("rewards.detail.colRank")}</th>
                  <th className="px-4 py-2.5 text-end font-medium">{t("rewards.detail.colPrize")}</th>
                  <th className="px-4 py-2.5 text-end font-medium">{t("rewards.detail.colPaidAs")}</th>
                </tr>
              </thead>
              <tbody>
                {c.prizes.map((p) => (
                  <tr key={`${p.rankFrom}-${p.rankTo}`} className="border-t border-line">
                    <td className="px-4 py-2.5">
                      <span className="flex items-center gap-2.5">
                        <RankBadge rank={p.rankFrom} size={22} />
                        <span className="k-num font-medium">{bandLabel(p)}</span>
                      </span>
                    </td>
                    <td className="k-num px-4 py-2.5 text-end font-semibold text-gold">
                      {fmtUsd(p.amount, 0)}
                      {p.rankTo > p.rankFrom ? <span className="font-normal text-fg-3"> {t("rewards.value.each")}</span> : null}
                    </td>
                    <td className="px-4 py-2.5 text-end text-fg-2">{p.payout === "credit" ? t("rewards.detail.paidCredit") : t("rewards.detail.paidWallet")}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </Card>
  );
}

export function LiveContestDetail({ id }: { id: string }) {
  const t = useT();
  const { data, error, reload } = useGrowth<ContestDetail>(`contests/${encodeURIComponent(id)}`, 15_000);
  useRewardsLive(reload);
  const back = (
    <Link href="/rewards" className="mb-3 inline-flex items-center gap-1.5 text-[12.5px] text-fg-3 hover:text-fg">
      <ArrowLeft className="size-3.5 rtl:-scale-x-100" /> {t("rewards.detail.allContests")}
    </Link>
  );
  if (!data)
    return (
      <div>
        {back}
        <PageFallback title={t("rewards.detail.fallbackTitle")} subtitle={t("rewards.detail.fallbackSubtitle")} error={error} onRetry={reload} rows={[{ cols: "", h: "h-[220px]", n: 1 }, { cols: "xl:grid-cols-2", h: "h-[320px]", n: 2 }]} />
      </div>
    );
  const c = data.contest;
  const running = isRunning(c);
  return (
    <div className="pb-16">
      {back}
      <PageHeader title={c.name} subtitle={c.description || t(c.kind === "demo" ? "rewards.detail.subtitleDemo" : "rewards.detail.subtitleLive", { scoring: scoringLabel(c.scoring).toLowerCase() })} />

      <Reveal>
        <Card hot className="overflow-hidden">
          <div className="grid grid-cols-1 gap-6 p-5 sm:p-7 lg:grid-cols-[1fr_auto] lg:items-end">
            <div>
              <div className="flex flex-wrap items-center gap-2">
                <GrowthStatus status={c.status} />
                {kindChip(c)}
                <Chip>
                  {fmtDate(c.startsAt, false)} – {fmtDate(c.endsAt)}
                </Chip>
                <Chip>
                  <Users className="size-3" /> {t("rewards.value.joined", { count: fmtCount(data.entrants) })}
                </Chip>
              </div>
              <div className="mt-6 flex flex-wrap items-end gap-x-8 gap-y-5">
                <div>
                  <div className="k-label">{t("rewards.detail.prizePool")}</div>
                  <Money value={c.prizePool} decimals={0} countUp={false} className="mt-1.5 block text-[40px] font-semibold leading-none tracking-tight text-gold" />
                </div>
                {(running || isUpcoming(c)) && (
                  <div>
                    <div className="k-label mb-2 flex items-center gap-1.5">
                      <Timer className="size-3.5" /> {running ? t("rewards.hero.endsIn") : t("rewards.hero.startsIn")}
                    </div>
                    <Countdown to={running ? c.endsAt : c.startsAt} />
                  </div>
                )}
              </div>
            </div>
            <div className="flex flex-wrap items-center gap-3">
              {data.myEntry ? (
                <Button variant="up-outline" size="lg" disabled className="disabled:opacity-100">
                  <Check /> {data.myEntry.login ? t("rewards.detail.joinedLogin", { login: data.myEntry.login }) : t("rewards.hero.joined")}
                </Button>
              ) : canJoin(c) ? (
                <JoinContestButton c={c} onJoined={reload} />
              ) : (
                <Button variant="surface" size="lg" disabled>
                  {isPast(c) ? t("rewards.detail.contestEnded") : t("rewards.hero.entriesClosed")}
                </Button>
              )}
            </div>
          </div>
        </Card>
      </Reveal>

      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal delay={0.05} className="xl:col-span-5">
          <MyEntry d={data} />
        </Reveal>
        <Reveal delay={0.1} className="xl:col-span-7">
          <PrizesTable d={data} />
        </Reveal>
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal delay={0.1} className="xl:col-span-8">
          <Leaderboard d={data} title={running ? t("rewards.board.title") : t("rewards.board.titleFinal")} podium={false} />
        </Reveal>
        <Reveal delay={0.15} className="xl:col-span-4">
          <Rules d={data} />
        </Reveal>
      </div>
    </div>
  );
}
