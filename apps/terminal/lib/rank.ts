// Kalks Trader's contest rank widget (phase 4): pure helpers (no imports), tested in tests/rank.test.mjs.
// Data: GET /api/growth/rank (app/api/growth/rank/route.ts) -> services/growth GET /v1/growth/me/rank?login=.

export type RankContest = {
  id: number;
  name: string;
  scoring: string;
  endsAt: string;
  rank: number | null;
  ranked: number;
  score: number;
  trades: number;
  minTrades: number;
  running: boolean;
};

export type RankLevel = { level: number; name: string; badge: string; xp: number; progressPct: number };

export type RankData = { contests: RankContest[]; level: RankLevel | null };

/** The contest the widget shows: running ones first (best rank, then the one ending soonest); null = none. */
export function pickContest(list: readonly RankContest[] | null | undefined): RankContest | null {
  const running = (list ?? []).filter((c) => c.running);
  if (!running.length) return null;
  return [...running].sort((a, b) => (a.rank ?? 1e9) - (b.rank ?? 1e9) || Date.parse(a.endsAt) - Date.parse(b.endsAt))[0]!;
}

/** Trades still needed to rank (0 = ranks already). */
export const tradesToGo = (c: Pick<RankContest, "trades" | "minTrades">) => Math.max(0, c.minTrades - c.trades);

/** "2d 4h" / "5h 12m" / "12m" until an end time; "" once past. */
export function endsIn(iso: string, now = Date.now()): string {
  const ms = Date.parse(iso) - now;
  if (!Number.isFinite(ms) || ms <= 0) return "";
  const m = Math.floor(ms / 60_000);
  const d = Math.floor(m / 1440);
  const h = Math.floor((m % 1440) / 60);
  return d > 0 ? `${d}d ${h}h` : h > 0 ? `${h}h ${m % 60}m` : `${Math.max(1, m)}m`;
}

/** Only the fields the widget needs, from the service's answer (anything else, like logins, is dropped). */
export function rankView(raw: unknown): RankData {
  const o = (raw ?? {}) as { contests?: unknown[]; level?: Record<string, unknown> | null };
  const num = (v: unknown, d = 0) => (typeof v === "number" && Number.isFinite(v) ? v : d);
  const contests: RankContest[] = Array.isArray(o.contests)
    ? o.contests.slice(0, 5).map((x) => {
        const c = (x ?? {}) as Record<string, unknown>;
        return {
          id: num(c.id),
          name: String(c.name ?? "").slice(0, 80),
          scoring: String(c.scoring ?? ""),
          endsAt: String(c.endsAt ?? ""),
          rank: typeof c.rank === "number" ? c.rank : null,
          ranked: num(c.ranked),
          score: num(c.score),
          trades: num(c.trades),
          minTrades: num(c.minTrades),
          running: c.running === true,
        };
      })
    : [];
  const l = o.level;
  const level = l && typeof l.level === "number" ? { level: l.level, name: String(l.name ?? ""), badge: String(l.badge ?? "bronze"), xp: num(l.xp), progressPct: num(l.progressPct) } : null;
  return { contests, level };
}
