"use client";

/** Risk desk building blocks shared by the exposure, risk limits, client scores, routing suggestion pages, the dealer
 *  risk monitor, the account drawer and the client profile (trading engine, live builds). */
import * as React from "react";
import Link from "next/link";
import { ExternalLink, Gauge as GaugeIcon, RefreshCw, ShieldAlert } from "lucide-react";
import { toast } from "sonner";
import { Button, Chip, Dialog, EmptyState, Flag, Skeleton, Sparkline, Tooltip, cn, formatNumber } from "@kalks/ui";
import { IS_DEMO } from "@kalks/mock/mode";
import { ErrorState, ago, useApi, useNow, type ApiErr } from "@/components/live/kit";
import { useCan } from "@/components/staff-session";
import { MetaTile, signedMoney } from "@/components/trading-desk/kit";
import {
  CATEGORY_LABEL,
  categoryLabel,
  categoryTone,
  ensureClientNames,
  flagLabel,
  holdText,
  isPending,
  liveClientName,
  riskCall,
  useLiveDirectory,
  type ClientScore,
  type LimitLevel,
  type RiskMode,
  type ScoreDetail,
  type ScoreList,
} from "@/lib/trading-desk";

/* ---------------- chips, bars, states ---------------- */

export function CategoryChip({ category, size = "sm" }: { category: string | null | undefined; size?: "sm" | "md" }) {
  if (!category) return <span className="text-fg-3">—</span>;
  return (
    <Chip size={size === "sm" ? "sm" : undefined} tone={categoryTone(category)} dot>
      {categoryLabel(category)}
    </Chip>
  );
}

const SCORE_FILL: Record<string, string> = { down: "bg-down", warn: "bg-warn", neutral: "bg-fg-3", up: "bg-up" };
const SCORE_TEXT: Record<string, string> = { down: "text-down", warn: "text-warn", neutral: "text-fg-2", up: "text-up" };

/** Category bands of the engine's client score (services/trading risk/score.rs): ≥ 65 likely profitable, ≤ 35 likely losing. */
export const SCORE_PROFITABLE_MIN = 65;
export const SCORE_LOSING_MAX = 35;

/** 0-100 client score as a small bar (high = likely profitable for the client = risk for the B-book). */
export function ScoreBar({ score, category, className, wide }: { score: number; category?: string | null; className?: string; wide?: boolean }) {
  const tone = categoryTone(category ?? (score >= SCORE_PROFITABLE_MIN ? "likely_profitable" : score <= SCORE_LOSING_MAX ? "likely_losing" : "neutral"));
  return (
    <span className={cn("inline-flex items-center gap-2", className)}>
      <span className={cn("relative h-1.5 overflow-hidden rounded-full bg-surface-3", wide ? "w-full min-w-24" : "w-16")}>
        <span className={cn("absolute inset-y-0 left-0 rounded-full", SCORE_FILL[tone])} style={{ width: `${Math.max(3, Math.min(100, score))}%` }} />
        <span className="absolute inset-y-0 w-px bg-fg-3/60" style={{ left: `${SCORE_PROFITABLE_MIN}%` }} aria-hidden />
      </span>
      <span className={cn("k-num w-7 text-right font-mono text-[12px] font-medium", SCORE_TEXT[tone])}>{Math.round(score)}</span>
    </span>
  );
}

export function ModeChip({ mode, enabled = true }: { mode: RiskMode; enabled?: boolean }) {
  if (!enabled) return <Chip size="sm" tone="neutral">Off</Chip>;
  return mode === "enforce" ? (
    <Chip size="sm" tone="down" dot>
      Enforcing
    </Chip>
  ) : (
    <Chip size="sm" tone="warn" dot>
      Alert only
    </Chip>
  );
}

export function LevelChip({ level, usedPct }: { level: LimitLevel; usedPct: number }) {
  return (
    <Chip size="sm" tone={level === "red" ? "down" : level === "amber" ? "warn" : "neutral"}>
      <span className="k-num">{formatNumber(usedPct, 0)}%</span>
      {level === "red" ? " · over cap" : level === "amber" ? " · warn" : ""}
    </Chip>
  );
}

/** Limit usage meter: amber at the warn %, red at the cap. */
export function UsageBar({ usedPct, level, className }: { usedPct: number; level: LimitLevel; className?: string }) {
  return (
    <span className={cn("inline-flex items-center gap-2", className)}>
      <span className="relative h-1.5 w-20 overflow-hidden rounded-full bg-surface-3">
        <span className={cn("absolute inset-y-0 left-0 rounded-full", level === "red" ? "bg-down" : level === "amber" ? "bg-warn" : "bg-up")} style={{ width: `${Math.max(2, Math.min(100, usedPct))}%` }} />
      </span>
      <span className={cn("k-num w-9 text-right font-mono text-[11.5px]", level === "red" ? "text-down" : level === "amber" ? "text-warn" : "text-fg-2")}>{formatNumber(usedPct, 0)}%</span>
    </span>
  );
}

/** Error state for risk desk calls: an engine without the risk routes reads as "after the engine update". */
export function RiskError({ error, onRetry, className, compact }: { error: ApiErr | { code: string; message: string }; onRetry?: () => void; className?: string; compact?: boolean }) {
  if (isPending(error)) {
    if (compact) return <div className={cn("rounded-[14px] border border-line bg-surface-2/60 px-3.5 py-3 text-[12.5px] text-fg-3", className)}>Risk desk available after the engine update.</div>;
    return <EmptyState className={className} illustration="hourglass_not_done" title="Available after the engine update" text="The trading engine on this environment does not serve the risk desk yet. This page fills in as soon as it is deployed." />;
  }
  if (compact) return <div className={cn("rounded-[14px] border border-down/30 bg-down-soft px-3.5 py-3 text-[12.5px] text-fg", className)}>{error.message}</div>;
  return <ErrorState className={className} error={error as ApiErr} onRetry={onRetry} />;
}

/** Demo builds: the risk desk runs on the trading engine only. */
export function ConnectEngineState({ what, className }: { what: string; className?: string }) {
  return <EmptyState className={className} illustration="electric_plug" title="Connect the trading engine" text={`${what} come from the trading engine's risk desk. This demo build has no engine connected, so the list stays empty.`} />;
}

/** Client name + mono login, linking to the trading account. */
export function LoginCell({ login, userId, sub }: { login: string; userId: string; sub?: React.ReactNode }) {
  useLiveDirectory();
  React.useEffect(() => ensureClientNames([userId]), [userId]);
  return (
    <span className="block min-w-0">
      <span className="block truncate text-[13px]">{IS_DEMO ? `Client ${userId}` : liveClientName(userId, login)}</span>
      <span className="block font-mono text-[11px] text-fg-3">
        {login}
        {sub ? <span className="font-sans"> · {sub}</span> : null}
      </span>
    </span>
  );
}

/* ---------------- recompute ---------------- */

export async function recomputeScores(login?: string) {
  const r = await riskCall<{ queued: boolean }>("POST", "dealing/risk/scores/run", login ? { login } : {});
  if (!r.ok) {
    toast.error("Recompute not started", { description: isPending(r.error) ? "Available after the engine update." : r.error.message });
    return false;
  }
  toast.success(login ? `Recompute queued for ${login}` : "Recompute queued for every client", { description: "Scores refresh in a few seconds" });
  return true;
}

/* ---------------- score drawer ---------------- */

export function ScoreDrawer({ login, onClose }: { login: string | null; onClose: () => void }) {
  const now = useNow();
  const canWrite = useCan("dealing.write");
  const { data, error, loading, reload } = useApi<ScoreDetail>(login ? `/api/trading/dealing/risk/scores/${login}` : null);
  const [busy, setBusy] = React.useState(false);
  const c = data?.current && data.current.login === login ? data.current : null;
  const hist = React.useMemo(() => [...(data?.history ?? [])].sort((a, b) => Date.parse(a.computedAt) - Date.parse(b.computedAt)), [data]);
  return (
    <Dialog
      side="right"
      open={!!login}
      onOpenChange={(o) => !o && onClose()}
      title={
        <span className="flex flex-wrap items-center gap-2">
          Client score <span className="font-mono">{login}</span>
          {c && <CategoryChip category={c.category} />}
        </span>
      }
      description={c ? `${IS_DEMO ? `Client ${c.userId}` : liveClientName(c.userId, c.login)} · ${c.group} · computed ${ago(c.computedAt, now)}` : loading ? "Loading…" : "No score yet"}
      footer={
        <>
          {login && (
            <Link href={`/trading/accounts?login=${login}`} className="mr-auto inline-flex items-center gap-1 text-[12.5px] text-fg-2 hover:text-fg">
              <ExternalLink className="size-3.5" /> Trading account
            </Link>
          )}
          {canWrite && (
            <Button
              size="sm"
              variant="surface"
              disabled={busy || !login}
              onClick={async () => {
                setBusy(true);
                if (await recomputeScores(login!)) setTimeout(reload, 3000);
                setBusy(false);
              }}
            >
              <RefreshCw /> {busy ? "Queuing…" : "Recompute"}
            </Button>
          )}
        </>
      }
    >
      {error ? (
        <RiskError error={error} onRetry={reload} />
      ) : !data ? (
        <div className="space-y-2">
          <Skeleton className="h-20 w-full" />
          <Skeleton className="h-40 w-full" />
        </div>
      ) : !c ? (
        <EmptyState illustration="chart_increasing" title="Not scored yet" text="The engine scores an account once it has closed trades. Recompute to score it now." />
      ) : (
        <div className="space-y-5">
          <ScoreSummary c={c} />
          <div>
            <div className="mb-2 text-[12.5px] font-medium text-fg-2">Why this score</div>
            <div className="space-y-1.5">
              {c.why.length === 0 && <div className="text-[12.5px] text-fg-3">No factors reported.</div>}
              {c.why.map((w) => (
                <div key={w.factor} className="k-row flex items-start gap-3 px-3.5 py-2.5">
                  <span className={cn("mt-1.5 size-1.5 shrink-0 rounded-full", { up: "bg-up", down: "bg-down", warn: "bg-warn", neutral: "bg-fg-3" }[w.tone])} />
                  <span className="min-w-0 flex-1">
                    <span className="flex flex-wrap items-baseline justify-between gap-x-3">
                      <span className="text-[13px]">{w.label}</span>
                      <span className="k-num font-mono text-[12.5px] text-fg">{w.value}</span>
                    </span>
                    {w.detail && <span className="mt-0.5 block text-[11.5px] leading-snug text-fg-3">{w.detail}</span>}
                  </span>
                  <span className={cn("k-num w-11 shrink-0 text-right font-mono text-[12px] font-medium", w.points > 0 ? "text-down" : w.points < 0 ? "text-up" : "text-fg-3")}>
                    {w.points > 0 ? "+" : ""}
                    {formatNumber(w.points, 0)}
                  </span>
                </div>
              ))}
            </div>
            <div className="mt-1.5 text-[11px] text-fg-3">Points push the score up (client likely profitable, risk for the B-book) or down.</div>
          </div>
          <div>
            <div className="mb-2 flex items-center justify-between text-[12.5px] font-medium text-fg-2">
              Score history <span className="text-[11px] font-normal text-fg-3">{hist.length} runs</span>
            </div>
            {hist.length >= 2 ? (
              <div className="k-row px-3.5 py-3">
                <Sparkline data={hist.map((h) => h.score)} width={480} height={56} tone="ember" className="h-14 w-full" />
              </div>
            ) : (
              <div className="text-[12.5px] text-fg-3">History builds up with every scoring run.</div>
            )}
            {hist.length > 0 && (
              <div className="mt-2 space-y-1">
                {[...hist].reverse().slice(0, 6).map((h) => (
                  <div key={h.computedAt} className="flex items-center justify-between gap-3 text-[12px]">
                    <span className="text-fg-3">{ago(h.computedAt, now)}</span>
                    <span className="flex items-center gap-2">
                      <CategoryChip category={h.category} />
                      <span className="k-num w-7 text-right font-mono">{Math.round(h.score)}</span>
                    </span>
                  </div>
                ))}
              </div>
            )}
          </div>
          <Facts c={c} />
        </div>
      )}
    </Dialog>
  );
}

function ScoreSummary({ c }: { c: ClientScore }) {
  return (
    <div className="space-y-3">
      <div className="flex items-center gap-4 rounded-[16px] border border-line bg-surface-2/60 px-4 py-3.5">
        <div className="text-center">
          <div className={cn("k-num font-mono text-[34px] font-semibold leading-none", SCORE_TEXT[categoryTone(c.category)])}>{Math.round(c.score)}</div>
          <div className="mt-1 text-[10px] uppercase tracking-wider text-fg-3">of 100</div>
        </div>
        <div className="min-w-0 flex-1 space-y-2">
          <ScoreBar score={c.score} category={c.category} wide />
          <div className="flex flex-wrap gap-1">
            {c.toxicFlags.map((f) => (
              <Chip key={f} size="sm" tone="down">
                <ShieldAlert className="size-3" /> {flagLabel(f)}
              </Chip>
            ))}
            {!c.toxicFlags.length && <span className="text-[11.5px] text-fg-3">No toxic-flow flags</span>}
          </div>
        </div>
      </div>
      <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
        <MetaTile label="Trades" value={formatNumber(c.trades, 0)} />
        <MetaTile label="Win rate" value={c.winRate === null ? "—" : `${formatNumber(c.winRate * (c.winRate <= 1 ? 100 : 1), 0)}%`} />
        <MetaTile label="Profit factor" value={c.profitFactor === null ? "—" : formatNumber(c.profitFactor, 2)} />
        <MetaTile label="Net P&L" value={signedMoney(c.netPnlUsd, 0)} tone={c.netPnlUsd >= 0 ? "up" : "down"} />
        <MetaTile label="Avg hold" value={holdText(c.avgHoldSecs)} tone={c.avgHoldSecs !== null && c.avgHoldSecs < 60 ? "warn" : undefined} />
        <MetaTile label="Scalping" value={c.scalpingShare === null ? "—" : `${formatNumber(c.scalpingShare * (c.scalpingShare <= 1 ? 100 : 1), 0)}%`} />
        <MetaTile label="Group" value={c.group} />
        <MetaTile label="Kind" value={c.kind} />
      </div>
    </div>
  );
}

function Facts({ c }: { c: ClientScore }) {
  const f = c.facts;
  return (
    <div>
      <div className="mb-2 text-[12.5px] font-medium text-fg-2">Facts</div>
      <div className="grid grid-cols-2 gap-x-6 gap-y-2 rounded-[14px] border border-line bg-surface-2/60 px-4 py-3 text-[12.5px] sm:grid-cols-3">
        <Fact k="Deposits" v={<span className="k-num font-mono">{signedMoney(f.depositsUsd, 0).replace("+", "")}</span>} />
        <Fact k="Account age" v={<span className="k-num">{formatNumber(f.accountAgeDays, 0)} days</span>} />
        <Fact
          k="Country"
          v={
            f.country ? (
              <span className="inline-flex items-center gap-1.5">
                <Flag country={f.country.toLowerCase()} /> <span className="font-mono">{f.country.toUpperCase()}</span>
              </span>
            ) : (
              "—"
            )
          }
        />
        <Fact k="IB source" v={f.ibSource ?? "Direct"} />
        <Fact k="Bonus" v={<span className="k-num font-mono">{signedMoney(f.bonusUsd, 0).replace("+", "")}</span>} />
        <Fact k="User" v={<Link href={`/clients/${c.userId}`} className="font-mono hover:text-ember">#{c.userId}</Link>} />
      </div>
    </div>
  );
}

function Fact({ k, v }: { k: string; v: React.ReactNode }) {
  return (
    <div className="min-w-0">
      <div className="text-[10.5px] uppercase tracking-wider text-fg-3">{k}</div>
      <div className="truncate text-fg-2">{v}</div>
    </div>
  );
}

/* ---------------- compact block (account drawer, client profile) ---------------- */

/**
 * Compact "Client score" block: the account's (or the client's accounts') score, category, flags and the top
 * reasons, with a link to the full breakdown. Pass `login` (account drawer) or `userId` (client profile).
 */
export function ClientScoreBlock({ login, userId, className }: { login?: string; userId?: string; className?: string }) {
  const can = useCan("dealing.read");
  const now = useNow();
  const url = IS_DEMO || !can ? null : login ? `/api/trading/dealing/risk/scores/${login}` : userId ? `/api/trading/dealing/risk/scores?userId=${userId}&limit=20` : null;
  const one = useApi<ScoreDetail>(login ? url : null);
  const many = useApi<ScoreList>(!login ? url : null);
  const [open, setOpen] = React.useState<string | null>(null);
  if (IS_DEMO || !can) return null;
  const error = login ? one.error : many.error;
  const items: ClientScore[] = login ? (one.data?.current ? [one.data.current] : []) : (many.data?.items ?? []);
  const loaded = login ? !!one.data : !!many.data;
  const top = [...items].sort((a, b) => b.score - a.score)[0];
  return (
    <div className={cn("rounded-[16px] border border-line bg-surface-2/60 px-4 py-3.5", className)}>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <span className="inline-flex items-center gap-2 text-[13px] font-medium">
          <GaugeIcon className="size-4 text-fg-3" /> Client score
        </span>
        {top && (
          <span className="inline-flex items-center gap-2">
            <CategoryChip category={top.category} />
            <ScoreBar score={top.score} category={top.category} />
          </span>
        )}
      </div>
      {error ? (
        <RiskError error={error} compact className="mt-2.5" />
      ) : !loaded ? (
        <Skeleton className="mt-2.5 h-10 w-full" />
      ) : !top ? (
        <div className="mt-2 text-[12px] text-fg-3">Not scored yet: the engine scores accounts with closed trades.</div>
      ) : (
        <div className="mt-2.5 space-y-2">
          {top.toxicFlags.length > 0 && (
            <div className="flex flex-wrap gap-1">
              {top.toxicFlags.map((f) => (
                <Chip key={f} size="sm" tone="down">
                  {flagLabel(f)}
                </Chip>
              ))}
            </div>
          )}
          <div className="space-y-1">
            {[...top.why]
              .sort((a, b) => Math.abs(b.points) - Math.abs(a.points))
              .slice(0, 3)
              .map((w) => (
                <div key={w.factor} className="flex items-center justify-between gap-3 text-[12px]">
                  <span className="truncate text-fg-2">
                    {w.label} <span className="font-mono text-fg">{w.value}</span>
                  </span>
                  <span className={cn("k-num font-mono", w.points > 0 ? "text-down" : w.points < 0 ? "text-up" : "text-fg-3")}>
                    {w.points > 0 ? "+" : ""}
                    {formatNumber(w.points, 0)}
                  </span>
                </div>
              ))}
          </div>
          {items.length > 1 && (
            <div className="flex flex-wrap gap-1.5 pt-0.5">
              {items.map((s) => (
                <Tooltip key={s.login} content={`${CATEGORY_LABEL[s.category] ?? s.category} · ${s.trades} trades`}>
                  <button type="button" onClick={() => setOpen(s.login)} className="inline-flex items-center gap-1.5 rounded-full border border-line bg-surface px-2 py-0.5 font-mono text-[11px] text-fg-2 hover:text-fg">
                    {s.login} <span className="k-num">{Math.round(s.score)}</span>
                  </button>
                </Tooltip>
              ))}
            </div>
          )}
          <div className="flex items-center justify-between gap-2 pt-0.5 text-[11.5px] text-fg-3">
            <span>
              Account <span className="font-mono">{top.login}</span> · computed {ago(top.computedAt, now)}
            </span>
            <button type="button" onClick={() => setOpen(top.login)} className="text-fg-2 hover:text-ember">
              Why this score
            </button>
          </div>
        </div>
      )}
      <ScoreDrawer login={open} onClose={() => setOpen(null)} />
    </div>
  );
}
