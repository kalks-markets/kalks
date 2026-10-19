"use client";

// /rewards/leaderboard (phase 4): XP this week / month / all time, or one contest. Everyone appears by nickname
// with a level badge (never a name, login or country); the client's own row is marked and pinned below the table.
// Live: the support stream reloads it on prizes and level-ups, and it polls every 30 s.

import * as React from "react";
import Link from "next/link";
import { ArrowLeft, Medal, Settings2 } from "lucide-react";
import { Button, Card, CardHeader, Chip, PageHeader, Reveal, Segmented, cn } from "@/components/kit";
import { useT } from "@kalks/i18n/react";
import { fmtCount, fmtLots, fmtPct, fmtUsd, scoringLabel, useGrowth } from "./api";
import { useRewardsLive, type Board, type BoardRow, type Hub } from "./challenges-api";
import { LevelBadge } from "./hub";
import { CardEmpty, PageFallback, RankBadge } from "./ui";

type Period = "week" | "month" | "all";

function scoreText(scoring: string, v: number) {
  if (scoring === "xp") return `${fmtCount(v)} XP`;
  if (scoring === "return_pct") return fmtPct(+v.toFixed(2), true);
  if (scoring === "profit") return fmtUsd(v);
  if (scoring === "lots") return fmtLots(v);
  return fmtCount(v);
}

function Row({ r, scoring }: { r: BoardRow; scoring: string }) {
  const t = useT();
  return (
    <div className={cn("k-row grid grid-cols-[36px_minmax(0,1fr)_auto] items-center gap-3 px-3.5 py-2.5", r.me && "border-ember/40 bg-ember-soft/40")} data-testid={r.me ? "board-me" : undefined}>
      <RankBadge rank={r.rank} />
      <span className="flex min-w-0 items-center gap-2.5">
        <LevelBadge level={r.level} size={28} />
        <span className="min-w-0">
          <span className="flex items-center gap-1.5">
            <span className="truncate text-[13.5px] font-medium" dir="auto">
              {r.nickname}
            </span>
            {r.me && (
              <Chip size="sm" tone="ember">
                {t("rewards.you")}
              </Chip>
            )}
          </span>
          <span className="block truncate text-[11px] text-fg-3">{t("challenges.level.short", { level: r.level.level, name: r.level.name })}</span>
        </span>
      </span>
      <span className="k-num text-[13.5px] font-semibold">{scoreText(scoring, r.score)}</span>
    </div>
  );
}

export function LiveLeaderboardPage() {
  const t = useT();
  const hub = useGrowth<Hub>("hub", 120_000);
  const [sel, setSel] = React.useState<string>("week");
  const contests = (hub.data?.contests ?? []).filter((c) => c.status === "running" || c.status === "scheduled");
  const isPeriod = sel === "week" || sel === "month" || sel === "all";
  const path = isPeriod ? `leaderboard?period=${sel}` : `leaderboard?contest=${encodeURIComponent(sel)}`;
  const board = useGrowth<Board>(path, 30_000);
  useRewardsLive(board.reload);
  const title = t("challenges.board.title");
  const subtitle = t("challenges.board.subtitle");
  const back = (
    <Link href="/rewards" className="mb-3 inline-flex items-center gap-1.5 text-[12.5px] text-fg-3 hover:text-fg">
      <ArrowLeft className="size-3.5 rtl:-scale-x-100" /> {t("challenges.board.back")}
    </Link>
  );
  const b = board.data && (isPeriod ? board.data.board === "xp" && board.data.period === sel : board.data.board === "contest") ? board.data : null;
  const scoring = b?.board === "contest" ? (b.contest?.scoring ?? "profit") : "xp";
  const meShown = b?.items.some((r) => r.me);

  const options: { value: string; label: React.ReactNode }[] = [
    { value: "week" satisfies Period, label: t("challenges.board.week") },
    { value: "month" satisfies Period, label: t("challenges.board.month") },
    { value: "all" satisfies Period, label: t("challenges.board.all") },
    ...contests.slice(0, 4).map((c) => ({ value: String(c.id), label: <span className="max-w-[160px] truncate">{c.name}</span> })),
  ];

  if (!board.data && board.error)
    return (
      <div>
        {back}
        <PageFallback title={title} subtitle={subtitle} error={board.error} onRetry={board.reload} rows={[{ cols: "", h: "h-[420px]", n: 1 }]} />
      </div>
    );

  return (
    <div className="pb-16">
      {back}
      <PageHeader
        title={title}
        subtitle={subtitle}
        actions={
          <Link href="/profile">
            <Button variant="surface">
              <Settings2 /> {t("challenges.board.nickname")}
            </Button>
          </Link>
        }
      />
      <div className="mb-4 overflow-x-auto">
        <Segmented value={sel} onChange={setSel} options={options} aria-label={t("challenges.board.pick")} />
      </div>
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal className="xl:col-span-8">
          <Card>
            <CardHeader
              title={b?.board === "contest" ? (b.contest?.name ?? title) : t(sel === "month" ? "challenges.board.xpMonth" : sel === "all" ? "challenges.board.xpAll" : "challenges.board.xpWeek")}
              subtitle={b?.board === "contest" ? t("challenges.board.rankedBy", { scoring: scoringLabel(scoring).toLowerCase() }) : t("challenges.board.xpHint")}
              icon={<Medal />}
            />
            <div className="space-y-1.5 px-4 pb-6 pt-3 sm:px-6" data-testid="leaderboard">
              {!b ? (
                Array.from({ length: 6 }, (_, i) => <div key={i} className="k-row h-[52px] animate-pulse" />)
              ) : b.items.length === 0 ? (
                <CardEmpty title={t("challenges.board.emptyTitle")} text={t("challenges.board.emptyText")} />
              ) : (
                b.items.map((r, i) => <Row key={`${r.rank}-${r.nickname}-${i}`} r={r} scoring={scoring} />)
              )}
            </div>
          </Card>
        </Reveal>
        <div className="flex flex-col gap-4 xl:col-span-4">
          <Reveal delay={0.05}>
            <Card>
              <CardHeader title={t("challenges.board.you")} />
              <div className="px-4 pb-5 pt-3 sm:px-6">
                {b?.me ? <Row r={{ ...b.me, me: true }} scoring={scoring} /> : <CardEmpty title={t("challenges.board.notRanked")} text={t("challenges.board.notRankedText")} />}
                {b?.me && !meShown && b.me.rank && <p className="mt-2 text-[11.5px] text-fg-3">{t("challenges.board.belowTop")}</p>}
              </div>
            </Card>
          </Reveal>
          <Reveal delay={0.1}>
            <Card>
              <CardHeader title={t("challenges.board.privacyTitle")} />
              <p className="px-6 pb-6 pt-2 text-[12.5px] leading-relaxed text-fg-2">{t("challenges.board.privacyText")}</p>
            </Card>
          </Reveal>
        </div>
      </div>
    </div>
  );
}
