"use client";

// Rewards hub (phase 4): level + XP progress, trading and login streaks, missions with progress, recent rewards.
// Shown at the top of /rewards; LevelStrip on the dashboard, NicknameCard on the profile. Live updates come from
// the support stream (mission completed, level up, prize paid) with a slow poll as the fallback.

import * as React from "react";
import Link from "next/link";
import { Check, ChevronRight, Clock, Flame, ListOrdered, Loader2, LogIn, Sparkles } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, CardHeader, Chip, Field, Icon3D, Input, Progress, Toggle, cn } from "@/components/kit";
import type { MessageKey } from "@kalks/i18n";
import { useT } from "@kalks/i18n/react";
import { errorToast, fmtDate, fmtUsd, growthApi, useGrowth } from "./api";
import { missionIcon, missionPct, progressText, untilText, useRewardsLive, type Hub, type LevelBadge as Badge, type MissionView, type Nickname, type RecentReward } from "./challenges-api";
import { CardEmpty } from "./ui";

/* ------------------------------------------------------------------ */
/* Level badge                                                          */
/* ------------------------------------------------------------------ */

const BADGE_STYLE: Record<string, string> = {
  bronze: "bg-[radial-gradient(circle_at_30%_25%,#ffd9b8,#d98b4a_45%,#8a4b1f)]",
  silver: "bg-[radial-gradient(circle_at_30%_25%,#ffffff,#c7ccd4_45%,#7a818c)]",
  gold: "bg-[radial-gradient(circle_at_30%_25%,#fff3c4,#e9b949_45%,#9c6f14)]",
  platinum: "bg-[radial-gradient(circle_at_30%_25%,#ffffff,#b9c6cf_40%,#55636e)]",
  diamond: "bg-[radial-gradient(circle_at_30%_25%,#ffffff,#9fe3f5_40%,#2b7f97)]",
  crown: "bg-[radial-gradient(circle_at_30%_25%,#fff3c4,var(--k-ember-2)_45%,color-mix(in_oklab,var(--k-ember)_70%,#000))]",
  rocket: "bg-[radial-gradient(circle_at_30%_25%,#ffffff,var(--k-ember-2)_45%,color-mix(in_oklab,var(--k-ember)_78%,#000))]",
  gem: "bg-[radial-gradient(circle_at_30%_25%,#ffffff,#9fe3f5_40%,#2b7f97)]",
};

/** Round level badge with the level number (the badge colour is the broker's choice per level). */
export function LevelBadge({ level, size = 40, className }: { level: Pick<Badge, "level" | "badge" | "name">; size?: number; className?: string }) {
  return (
    <span
      title={`${level.name} · ${level.level}`}
      className={cn("k-num grid shrink-0 place-items-center rounded-full font-bold text-black/70 shadow-[inset_0_-2px_6px_rgba(0,0,0,.25)]", BADGE_STYLE[level.badge] ?? BADGE_STYLE.bronze, className)}
      style={{ width: size, height: size, fontSize: Math.max(10, size * 0.38) }}
    >
      {level.level}
    </span>
  );
}

/* ------------------------------------------------------------------ */
/* Level + streaks                                                      */
/* ------------------------------------------------------------------ */

function StreakTile({ icon, label, value, best, hint }: { icon: React.ReactNode; label: string; value: number; best: number; hint: string }) {
  const t = useT();
  return (
    <div className="k-row flex items-center gap-3 px-3.5 py-3">
      <span className={cn("grid size-9 shrink-0 place-items-center rounded-full border", value > 0 ? "border-ember/40 bg-ember-soft text-ember" : "border-line bg-surface-3 text-fg-3")}>{icon}</span>
      <div className="min-w-0 flex-1">
        <div className="text-[12px] text-fg-3">{label}</div>
        <div className="k-num text-[16px] font-semibold">{t("challenges.streak.days", { count: value })}</div>
      </div>
      <div className="text-end text-[11px] text-fg-3">
        <div>{t("challenges.streak.best", { count: best })}</div>
        <div className="truncate">{hint}</div>
      </div>
    </div>
  );
}

function LevelCard({ hub }: { hub: Hub }) {
  const t = useT();
  const l = hub.level;
  return (
    <Card className="flex h-full flex-col">
      <CardHeader title={t("challenges.level.title")} subtitle={t("challenges.level.subtitle")} />
      <div className="flex-1 space-y-4 px-4 pb-5 pt-3 sm:px-6">
        <div className="flex items-center gap-4">
          <LevelBadge level={l} size={60} />
          <div className="min-w-0 flex-1">
            <div className="text-[12px] text-fg-3">{t("challenges.level.levelN", { level: l.level })}</div>
            <div className="truncate text-[19px] font-medium tracking-tight">{l.name}</div>
            <div className="k-num text-[12.5px] text-fg-2">{t("challenges.level.xp", { xp: l.xp.toLocaleString() })}</div>
          </div>
        </div>
        <div>
          <Progress value={l.progressPct} size="md" label={t("challenges.level.progress")} />
          <div className="mt-1.5 flex justify-between text-[11.5px] text-fg-3">
            <span>{l.name}</span>
            <span className="k-num">{l.next ? t("challenges.level.toNext", { xp: l.next.xpToGo.toLocaleString(), name: l.next.name }) : t("challenges.level.top")}</span>
          </div>
        </div>
        <div className="grid grid-cols-1 gap-2">
          <StreakTile icon={<Flame className="size-4" />} label={t("challenges.streak.trading")} value={hub.streaks.trading.current} best={hub.streaks.trading.best} hint={t("challenges.streak.tradingHint", { xp: hub.streaks.weekXp })} />
          <StreakTile icon={<LogIn className="size-4" />} label={t("challenges.streak.login")} value={hub.streaks.login.current} best={hub.streaks.login.best} hint={t("challenges.streak.loginHint")} />
        </div>
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Missions                                                             */
/* ------------------------------------------------------------------ */

function RewardChip({ m }: { m: MissionView }) {
  const t = useT();
  if (!m.rewardAmount) return null;
  const label = m.rewardPayout === "credit" ? t("challenges.mission.credit", { amount: fmtUsd(m.rewardAmount, 0) }) : t("challenges.mission.cash", { amount: fmtUsd(m.rewardAmount, 0) });
  return (
    <Chip size="sm" tone="gold">
      {label}
    </Chip>
  );
}

const REWARD_STATUS: Record<string, MessageKey> = { kyc: "challenges.reward.kyc", approval: "challenges.reward.approval", pending: "challenges.reward.pending", paid: "challenges.reward.paid", failed: "challenges.reward.failed", rejected: "challenges.reward.rejected" };

function MissionRow({ m, now }: { m: MissionView; now: number }) {
  const t = useT();
  const pct = missionPct(m);
  const left = m.schedule === "once" ? "" : untilText(m.resetsAt, now);
  const status = m.done && m.rewardAmount && REWARD_STATUS[m.rewardStatus] ? t(REWARD_STATUS[m.rewardStatus]!) : null;
  return (
    <div className={cn("k-row flex items-center gap-3 px-3.5 py-3", m.done && "border-up/30")} data-testid={`mission-${m.id}`}>
      <span className="relative shrink-0">
        <Icon3D name={missionIcon(m.kind)} size={36} />
        {m.done && (
          <span className="absolute -bottom-1 -end-1 grid size-4 place-items-center rounded-full bg-up text-white ring-2 ring-surface">
            <Check className="size-3" />
          </span>
        )}
      </span>
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-1.5">
          <span className="truncate text-[13.5px] font-medium">{m.name}</span>
          <Chip size="sm">{t(m.schedule === "daily" ? "challenges.mission.daily" : m.schedule === "weekly" ? "challenges.mission.weekly" : "challenges.mission.once")}</Chip>
        </div>
        <div className="truncate text-[11.5px] text-fg-3">{m.description}</div>
        <div className="mt-2 flex items-center gap-2">
          <Progress value={pct} tone={m.done ? "up" : "accent"} className="flex-1" label={m.name} />
          <span className="k-num w-24 shrink-0 text-end text-[11px] text-fg-2">{m.done ? t("challenges.mission.done") : progressText(m)}</span>
        </div>
      </div>
      <div className="flex shrink-0 flex-col items-end gap-1">
        <Chip size="sm" tone="ember">
          +{m.xp} XP
        </Chip>
        <RewardChip m={m} />
        {status ? (
          <span className="text-[10.5px] text-fg-3">{status}</span>
        ) : left ? (
          <span className="flex items-center gap-1 text-[10.5px] text-fg-3">
            <Clock className="size-3" /> {t("challenges.mission.resetsIn", { time: left })}
          </span>
        ) : null}
      </div>
    </div>
  );
}

function MissionsCard({ hub }: { hub: Hub }) {
  const t = useT();
  const [now, setNow] = React.useState(() => Date.now());
  React.useEffect(() => {
    const i = setInterval(() => setNow(Date.now()), 60_000);
    return () => clearInterval(i);
  }, []);
  // open ones first (closest to done), then the done ones
  const items = [...hub.missions].sort((a, b) => Number(a.done) - Number(b.done) || missionPct(b) - missionPct(a));
  const done = hub.missions.filter((m) => m.done).length;
  return (
    <Card className="flex h-full flex-col">
      <CardHeader
        title={t("challenges.missions.title")}
        subtitle={t("challenges.missions.subtitle")}
        action={
          <Chip tone={done ? "up" : "neutral"}>
            {done} / {hub.missions.length}
          </Chip>
        }
      />
      <div className="flex-1 space-y-2 px-4 pb-5 pt-3 sm:px-6">
        {items.length === 0 ? <CardEmpty title={t("challenges.missions.emptyTitle")} text={t("challenges.missions.emptyText")} /> : items.map((m) => <MissionRow key={m.id} m={m} now={now} />)}
        <p className="pt-1 text-[11px] leading-relaxed text-fg-3">{t("challenges.missions.rules")}</p>
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Recent rewards                                                       */
/* ------------------------------------------------------------------ */

function RecentRow({ r }: { r: RecentReward }) {
  const t = useT();
  return (
    <div className="flex items-center gap-3 border-b border-line py-2.5 last:border-0">
      <Icon3D name={r.kind === "prize" ? "trophy" : r.kind === "mission" ? "wrapped_gift" : "sparkles"} size={28} />
      <div className="min-w-0 flex-1">
        <div className="truncate text-[13px]">{r.title}</div>
        <div className="text-[11px] text-fg-3">{fmtDate(r.at)}</div>
      </div>
      <span className="k-num shrink-0 text-[13px] font-semibold text-gold">{r.xp !== null ? `+${r.xp} XP` : r.amount !== null ? fmtUsd(r.amount, 0) : ""}</span>
      {r.kind !== "xp" && r.status !== "paid" && REWARD_STATUS[r.status] && <span className="hidden text-[10.5px] text-fg-3 sm:inline">{t(REWARD_STATUS[r.status]!)}</span>}
    </div>
  );
}

function RecentCard({ hub }: { hub: Hub }) {
  const t = useT();
  return (
    <Card className="flex h-full flex-col">
      <CardHeader
        title={t("challenges.recent.title")}
        subtitle={t("challenges.recent.subtitle")}
        action={
          <Link href="/rewards/leaderboard">
            <Button size="sm" variant="surface">
              <ListOrdered /> {t("challenges.board.open")}
            </Button>
          </Link>
        }
      />
      <div className="flex-1 px-4 pb-5 pt-2 sm:px-6">{hub.recent.length === 0 ? <CardEmpty title={t("challenges.recent.emptyTitle")} text={t("challenges.recent.emptyText")} /> : hub.recent.slice(0, 8).map((r, i) => <RecentRow key={`${r.kind}-${r.at}-${i}`} r={r} />)}</div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Hub                                                                  */
/* ------------------------------------------------------------------ */

/** The hub row on /rewards. Renders nothing until loaded (the contests below have their own fallback). */
export function RewardsHub() {
  const hub = useGrowth<Hub>("hub", 60_000);
  useRewardsLive(hub.reload);
  if (!hub.data) return hub.error ? null : <div className="mb-4 grid grid-cols-1 gap-4 xl:grid-cols-12">{[4, 5, 3].map((n) => <Card key={n} className={cn("h-[340px] animate-pulse", n === 4 ? "xl:col-span-4" : n === 5 ? "xl:col-span-5" : "xl:col-span-3")} />)}</div>;
  return (
    <div className="mb-4 grid grid-cols-1 gap-4 xl:grid-cols-12" data-testid="rewards-hub">
      <div className="xl:col-span-4">
        <LevelCard hub={hub.data} />
      </div>
      <div className="xl:col-span-5">
        <MissionsCard hub={hub.data} />
      </div>
      <div className="xl:col-span-3">
        <RecentCard hub={hub.data} />
      </div>
    </div>
  );
}

/** Dashboard: level, XP progress and streak in one slim card linking to the Rewards hub. */
export function LevelStrip({ className = "mt-4" }: { className?: string }) {
  const t = useT();
  const hub = useGrowth<Hub>("hub", 120_000);
  useRewardsLive(hub.reload);
  const h = hub.data;
  if (!h) return null;
  const open = h.missions.filter((m) => !m.done).length;
  return (
    <Link href="/rewards" className={cn("block", className)}>
      <Card className="flex flex-wrap items-center gap-4 px-5 py-4 transition-colors hover:border-[var(--k-border-top)]">
        <LevelBadge level={h.level} size={44} />
        <div className="min-w-[160px] flex-1">
          <div className="flex items-baseline gap-2">
            <span className="text-[14px] font-medium">{h.level.name}</span>
            <span className="k-num text-[12px] text-fg-3">{t("challenges.level.xp", { xp: h.level.xp.toLocaleString() })}</span>
          </div>
          <Progress value={h.level.progressPct} className="mt-1.5" label={t("challenges.level.progress")} />
        </div>
        <Chip tone={h.streaks.trading.current > 0 ? "ember" : "neutral"}>
          <Flame className="size-3" /> {t("challenges.streak.days", { count: h.streaks.trading.current })}
        </Chip>
        {open > 0 && (
          <Chip tone="gold">
            <Sparkles className="size-3" /> {t("challenges.missions.open", { count: open })}
          </Chip>
        )}
        <ChevronRight className="size-4 text-fg-3 rtl:-scale-x-100" />
      </Card>
    </Link>
  );
}

/* ------------------------------------------------------------------ */
/* Nickname (profile)                                                   */
/* ------------------------------------------------------------------ */

/** Profile: the leaderboard nickname (what other people see instead of the name) and the public-website choice. */
export function NicknameCard() {
  const t = useT();
  const nick = useGrowth<Nickname>("nickname");
  const [value, setValue] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const loaded = nick.data?.nickname;
  React.useEffect(() => {
    if (loaded) setValue(loaded);
  }, [loaded]);
  if (nick.error && !nick.data) return null;
  const save = async (body: { nickname?: string; public?: boolean }) => {
    setBusy(true);
    setError(null);
    try {
      const r = await growthApi<Nickname>("nickname", { method: "POST", body });
      nick.setData(r);
      toast.success(body.nickname ? t("challenges.nick.saved") : r.public ? t("challenges.nick.publicOn") : t("challenges.nick.publicOff"));
    } catch (e) {
      setError(e instanceof Error ? e.message : null);
      errorToast(t("challenges.nick.error"), e);
    } finally {
      setBusy(false);
    }
  };
  const n = nick.data;
  return (
    <Card data-testid="nickname-card">
      <CardHeader title={t("challenges.nick.title")} subtitle={t("challenges.nick.subtitle")} icon={<ListOrdered />} />
      <div className="space-y-4 px-6 pb-6 pt-3">
        <Field label={t("challenges.nick.label")} hint={t("challenges.nick.hint")} error={error ?? undefined}>
          <Input value={value} onChange={(e) => setValue(e.target.value)} maxLength={20} disabled={!n || busy} dir="auto" />
        </Field>
        <div className="flex flex-wrap gap-2">
          <Button size="sm" variant="ember" disabled={!n || busy || !value.trim() || value.trim() === n?.nickname} onClick={() => save({ nickname: value.trim() })}>
            {busy && <Loader2 className="animate-spin" />} {t("challenges.nick.save")}
          </Button>
          {n && (
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => setValue(n.suggestion)}>
              <Sparkles /> {t("challenges.nick.suggest")}
            </Button>
          )}
        </div>
        <div className="k-row flex items-center gap-3 px-4 py-3">
          <div className="min-w-0 flex-1">
            <div className="text-[13px] font-medium">{t("challenges.nick.publicLabel")}</div>
            <div className="text-[11.5px] text-fg-3">{t("challenges.nick.publicHint")}</div>
          </div>
          <Toggle checked={!!n?.public} disabled={!n || busy} onChange={(v) => save({ public: v })} label={t("challenges.nick.publicLabel")} />
        </div>
      </div>
    </Card>
  );
}
