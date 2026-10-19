"use client";

// Challenges (phase 4) on the growth BFF: Rewards hub, missions, XP / levels / streaks, nickname and leaderboards.
// Shapes mirror services/growth/README.md "Challenges". Live builds only.

import * as React from "react";
import type { MessageKey } from "@kalks/i18n";
import { tr } from "@kalks/i18n/react";
import { realtime, type Frame } from "@/lib/realtime";
import type { ContestCard } from "./api";

export interface LevelBadge {
  level: number;
  name: string;
  /** bronze | silver | gold | platinum | diamond | crown | rocket | gem */
  badge: string;
}

export interface LevelInfo extends LevelBadge {
  xp: number;
  minXp: number;
  next: (LevelBadge & { minXp: number; xpToGo: number }) | null;
  progressPct: number;
  levels: (LevelBadge & { minXp: number })[];
}

export interface StreakInfo {
  current: number;
  best: number;
  lastDay: string | null;
  graceDays: number;
}

export type MissionKind = "kyc" | "first_deposit" | "deposit" | "lots" | "trading_days" | "hold" | "join_contest" | "referral";

export interface MissionView {
  id: number;
  name: string;
  description: string;
  kind: MissionKind | string;
  schedule: "daily" | "weekly" | "once" | string;
  target: number;
  progress: number;
  xp: number;
  rewardAmount: number | null;
  rewardPayout: "wallet" | "credit" | null;
  period: string;
  resetsAt: string | null;
  completedAt: string | null;
  done: boolean;
  /** none | kyc | approval | pending | paid | failed | rejected */
  rewardStatus: string;
}

export interface RecentReward {
  kind: "xp" | "prize" | "mission";
  title: string;
  xp: number | null;
  amount: number | null;
  payout?: string | null;
  status: string;
  at: string;
}

export interface Nickname {
  nickname: string;
  custom: boolean;
  public: boolean;
  publicChoice: boolean | null;
  publicDefault: boolean;
  suggestion: string;
}

export interface Hub {
  level: LevelInfo;
  streaks: { trading: StreakInfo; login: StreakInfo; weekXp: number };
  missions: MissionView[];
  contests: ContestCard[];
  recent: RecentReward[];
  nickname: Nickname;
}

export interface BoardRow {
  rank: number | null;
  nickname: string;
  score: number;
  level: LevelBadge;
  me?: boolean;
}

export interface Board {
  board: "contest" | "xp";
  period?: string;
  scoring?: string;
  contest?: { id: number; slug: string; name: string; status: string; kind: string; scoring: string; startsAt: string; endsAt: string; prizePool: number; entrants: number };
  items: BoardRow[];
  me: BoardRow | null;
  updatedAt: string | null;
}

/** Notification types the growth service sends (support stream): a rewards view reloads when one arrives. */
export const isRewardsFrame = (f: Frame) => {
  if (f.type === "reconnected") return true;
  if (f.type !== "notification") return false;
  const type = String((f.item as { type?: string } | undefined)?.type ?? "");
  return type.startsWith("rewards.") || type.startsWith("contest.") || type.startsWith("reward.");
};

/**
 * Live updates without polling faster: the support stream (one connection per tab) pushes mission completions,
 * level-ups and prizes; `reload` runs on each (and after a reconnect). Polling in `useGrowth` stays the fallback.
 */
export function useRewardsLive(reload: () => void, enabled = true) {
  const ref = React.useRef(reload);
  ref.current = reload;
  React.useEffect(() => {
    if (!enabled || typeof window === "undefined") return;
    return realtime().subscribe((f) => {
      if (isRewardsFrame(f)) ref.current();
    });
  }, [enabled]);
}

const KIND_ICON: Record<string, string> = {
  kyc: "identification_card",
  first_deposit: "money_bag",
  deposit: "bank",
  lots: "rocket",
  trading_days: "calendar",
  hold: "hourglass_not_done",
  join_contest: "trophy",
  referral: "handshake",
};
export const missionIcon = (kind: string) => KIND_ICON[kind] ?? "sparkles";

const UNIT: Record<string, MessageKey> = { lots: "challenges.unit.lots", trading_days: "challenges.unit.days", hold: "challenges.unit.hours", deposit: "challenges.unit.usd", referral: "challenges.unit.friends", join_contest: "challenges.unit.contests" };

/** "0.6 / 1 lots", "3 / 5 days"; one-step missions (KYC, first deposit) show nothing. */
export function progressText(m: Pick<MissionView, "kind" | "progress" | "target">) {
  if (m.kind === "kyc" || m.kind === "first_deposit") return "";
  const unit = UNIT[m.kind] ? tr(UNIT[m.kind]!) : "";
  const f = (v: number) => (Number.isInteger(v) ? String(v) : String(+v.toFixed(2)));
  return `${f(Math.min(m.progress, m.target))} / ${f(m.target)}${unit ? ` ${unit}` : ""}`;
}

export const missionPct = (m: Pick<MissionView, "progress" | "target" | "done">) => (m.done ? 100 : m.target > 0 ? Math.max(0, Math.min(100, (m.progress / m.target) * 100)) : 0);

/** "5h 12m" / "3d 4h" until a reset. */
export function untilText(iso: string | null, now = Date.now()) {
  if (!iso) return "";
  const ms = Date.parse(iso) - now;
  if (!Number.isFinite(ms) || ms <= 0) return "";
  const m = Math.floor(ms / 60_000);
  const d = Math.floor(m / 1440);
  const h = Math.floor((m % 1440) / 60);
  return d > 0 ? `${d}d ${h}h` : h > 0 ? `${h}h ${m % 60}m` : `${Math.max(1, m)}m`;
}
