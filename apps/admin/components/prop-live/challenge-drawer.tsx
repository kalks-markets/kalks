"use client";

import * as React from "react";
import { AlertTriangle, BadgeCheck, CheckCircle2, CircleDashed, TrendingUp, XCircle } from "lucide-react";
import { Button, Chip, CopyButton, Dialog, EquityChart, KeyValue, Skeleton, Tabs, cn, type SeriesPoint } from "@kalks/ui";
import { MiniStat, NumInput } from "@/components/config/kit";
import { ago, useApi, useNow, when } from "@/components/live/kit";
import {
  BLOCKER_LABEL,
  FirstApproval,
  JsonView,
  KycStatus,
  PlanTypeChip,
  PropError,
  PropStatus,
  SeverityChip,
  TraderCell,
  int,
  pct,
  propWrite,
  reasonText,
  ruleLabel,
  signedUsd,
  usd,
  usdK,
  useAction,
  useClientNames,
  usePropCan,
  type ChallengeDetail,
  type LiveRules,
  type PhaseAccount,
} from "./kit";

const PASS_REASONS = ["Target reached; evaluator lag", "Rounding dispute resolved", "Platform / feed incident", "Management override"];
const FAIL_REASONS = ["Banned strategy confirmed", "Account sharing", "Copy trading across accounts", "Terms of service breach", "Fraud / chargeback"];
const SCALE_REASONS = ["Scaling review passed", "Management decision", "Retention"];

/** Live rule meter: used / limit with the headroom. */
function Meter({ label, used, limit, detail, state }: { label: string; used: number; limit: number; detail: React.ReactNode; state?: "ok" | "warn" | "breach" | "pending" }) {
  const p = limit > 0 ? Math.max(0, Math.min(100, (used / limit) * 100)) : 0;
  const s = state ?? (p >= 100 ? "breach" : p >= 50 ? "warn" : "ok");
  const Icon = s === "ok" ? CheckCircle2 : s === "warn" ? AlertTriangle : s === "breach" ? XCircle : CircleDashed;
  const tone = { ok: "text-up", warn: "text-warn", breach: "text-down", pending: "text-fg-3" }[s];
  const bg = { ok: "bg-up", warn: "bg-warn", breach: "bg-down", pending: "bg-fg-3/40" }[s];
  return (
    <div className="k-row px-4 py-3">
      <div className="flex items-center justify-between gap-3">
        <span className="flex items-center gap-2 text-[13px] font-medium">
          <Icon className={cn("size-4", tone)} />
          {label}
        </span>
        <span className={cn("k-num text-[12px]", tone)}>{p.toFixed(0)}%</span>
      </div>
      <div className="mt-2.5 h-2 overflow-hidden rounded-full bg-surface-3">
        <div className={cn("h-full rounded-full", bg)} style={{ width: `${p}%` }} />
      </div>
      <div className="mt-1.5 text-[11.5px] text-fg-3">{detail}</div>
    </div>
  );
}

export function RuleMeters({ a }: { a: PhaseAccount }) {
  const r = (a.rules ?? {}) as Partial<LiveRules>;
  if (r.dailyLimit === undefined) return <div className="rounded-[12px] border border-dashed border-line px-3 py-4 text-center text-[12.5px] text-fg-3">No evaluation yet.</div>;
  return (
    <div className="space-y-2">
      <Meter label="Daily loss" used={r.dailyUsed ?? 0} limit={r.dailyLimit ?? 0} detail={<>Used {usd(Math.max(0, r.dailyUsed ?? 0))} of {usd(r.dailyLimit)} · floor {usd(r.dailyFloor)} · resets {r.nextReset ? when(r.nextReset) : "—"}</>} />
      <Meter label="Max drawdown" used={r.ddUsed ?? 0} limit={r.ddLimit ?? 0} detail={<>Used {usd(Math.max(0, r.ddUsed ?? 0))} of {usd(r.ddLimit)} · floor {usd(r.ddFloor)} · high-water {usd(r.hwm)}</>} />
      {!a.funded && r.targetAmount ? (
        <Meter label="Profit target" used={Math.max(0, r.profit ?? 0)} limit={r.targetAmount} state={r.targetReached ? "ok" : "pending"} detail={<>{signedUsd(r.profit ?? 0)} of {usd(r.targetAmount)}{r.targetReached ? " · reached" : ""}</>} />
      ) : null}
      {!a.funded && (
        <Meter label="Minimum trading days" used={r.tradingDays ?? 0} limit={Math.max(1, r.minDays ?? 0)} state={r.daysOk ? "ok" : "pending"} detail={<>{int(r.tradingDays)} of {int(r.minDays)} days</>} />
      )}
      {r.consistencyLimit !== null && r.consistencyLimit !== undefined && (
        <Meter label="Consistency" used={Math.max(0, r.bestDay ?? 0)} limit={r.consistencyLimit} state={r.consistencyOk ? "ok" : "warn"} detail={<>Best day {usd(r.bestDay)} · limit {usd(r.consistencyLimit)}</>} />
      )}
    </div>
  );
}

type Tab = "events" | "flags" | "payouts" | "audit";

export function ChallengeDrawer({ id, open, onOpenChange, onChanged, funded }: { id: number | null; open: boolean; onOpenChange: (o: boolean) => void; onChanged?: () => void; funded?: boolean }) {
  const tick = useNow(3000);
  const canWrite = usePropCan("prop.write");
  const { data: c, error, reload } = useApi<ChallengeDetail>(open && id ? `/api/prop/challenges/${id}` : null, { refreshMs: 3000 });
  const eq = useApi<{ initialBalance: number; points: { at: string; balance: number; equity: number }[] }>(open && id ? `/api/prop/challenges/${id}/equity` : null, { refreshMs: 15_000 });
  const names = useClientNames(c ? [c.userId] : []);
  const [tab, setTab] = React.useState<Tab>("events");
  const act = useAction();
  const scaleTo = React.useRef(0);

  const loaded = c && c.id === id ? c : null;
  const a = loaded?.current ?? loaded?.phases?.[loaded.phases.length - 1] ?? null;
  const r = (a?.rules ?? {}) as Partial<LiveRules>;
  // the service clock can be a little ahead of the page's: never show "in 1m" for something that already happened
  const now = Math.max(tick, a?.lastEvalAt ? Date.parse(a.lastEvalAt) : 0);
  const done = () => {
    reload();
    onChanged?.();
  };

  const override = (action: "pass" | "fail") => {
    if (!loaded || !a) return;
    act.ask({
      title: action === "pass" ? `Manual pass · ${a.phase} · #${a.login ?? loaded.id}` : `Manual fail · challenge #${loaded.id}`,
      description:
        action === "pass"
          ? "Passes the current phase now, bypassing the remaining targets: the account is flattened and set read-only, a certificate is issued and the next account opens."
          : "Fails the challenge now: every position is closed, pending orders are cancelled and the account is disabled. The fee is not refunded. The trader is notified.",
      reasons: action === "pass" ? PASS_REASONS : FAIL_REASONS,
      note: "required",
      notePlaceholder: "What happened and who approved it",
      confirmLabel: action === "pass" ? "Pass phase" : "Fail challenge",
      confirmVariant: action === "pass" ? "buy" : "sell",
      run: (v) => propWrite(`challenges/${loaded.id}/override`, { action, reason: v.reason, note: v.note }),
      success: action === "pass" ? `Challenge #${loaded.id} passed` : `Challenge #${loaded.id} failed`,
      onDone: done,
    });
  };

  const scale = () => {
    if (!loaded || !a) return;
    const cap = loaded.plan?.scalingCap ?? 0;
    const inc = loaded.plan?.scalingIncrease ?? 25;
    scaleTo.current = Math.round((a.initialBalance * (1 + inc / 100)) / 1000) * 1000;
    act.ask({
      title: `Scale funded account #${a.login}`,
      description: `Current size ${usd(a.initialBalance, 0)}. The plan scales by ${inc}% up to ${usdK(cap)}. The balance difference is added as a PRP adjustment.`,
      reasons: SCALE_REASONS,
      note: "optional",
      confirmLabel: "Scale account",
      body: <ScaleInput valueRef={scaleTo} min={a.initialBalance} cap={cap} />,
      run: (v) => propWrite(`accounts/${a.id}/scale`, { toSize: scaleTo.current, reason: reasonText(v) }),
      success: "Account scaled",
      onDone: done,
    });
  };

  const points: SeriesPoint[] = React.useMemo(() => {
    const out: SeriesPoint[] = [];
    let last = -1;
    for (const p of eq.data?.points ?? []) {
      const t = Math.floor(Date.parse(p.at) / 1000);
      if (t <= last) continue;
      last = t;
      out.push({ time: t, value: p.equity });
    }
    return out;
  }, [eq.data]);

  const status = loaded?.status;
  const canPass = status === "active" && a?.status === "active" && !a.funded;
  const canFail = status === "active" || status === "funded";

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      side="right"
      title={
        <span className="flex items-center gap-2">
          Challenge <span className="font-mono text-fg-2">#{id}</span>
          {a?.login && (
            <>
              <span className="font-mono text-[13px] text-fg-3">· {a.login}</span>
              <CopyButton value={String(a.login)} label="Login" />
            </>
          )}
        </span>
      }
      description={loaded ? `${loaded.planName} · bought ${when(loaded.createdAt)}` : "Loading…"}
      footer={
        canWrite && loaded ? (
          <>
            {funded && a?.funded && a.status === "active" && (
              <Button size="sm" variant="surface" className="mr-auto" onClick={scale}>
                <TrendingUp /> Scale
              </Button>
            )}
            <Button size="sm" variant="up-outline" disabled={!canPass} onClick={() => override("pass")}>
              <BadgeCheck /> Manual pass
            </Button>
            <Button size="sm" variant="down-outline" disabled={!canFail} onClick={() => override("fail")}>
              <XCircle /> Manual fail
            </Button>
          </>
        ) : undefined
      }
    >
      {error && !loaded ? (
        <PropError error={error} onRetry={reload} />
      ) : !loaded || !a ? (
        <div className="space-y-3">
          <Skeleton className="h-12 w-full" />
          <Skeleton className="h-24 w-full" />
          <Skeleton className="h-48 w-full" />
        </div>
      ) : (
        <div className="space-y-5">
          <div className="flex items-center justify-between gap-3">
            <TraderCell name={loaded.traderName} userId={loaded.userId} names={names} />
            <PropStatus status={loaded.status} />
          </div>
          <div className="flex flex-wrap items-center gap-1.5">
            <PlanTypeChip type={loaded.type} />
            <Chip size="sm">{a.phase}</Chip>
            <Chip size="sm" tone="ember">
              {usd(loaded.size, 0)}
            </Chip>
            <Chip size="sm">1:{loaded.leverage}</Chip>
            <Chip size="sm">{loaded.group}</Chip>
            {names[String(loaded.userId)] && <KycStatus status={names[String(loaded.userId)]!.kyc} />}
            {r.verdict?.kind === "breach" && (
              <Chip size="sm" tone="down">
                {ruleLabel(r.verdict.rule)} breached
              </Chip>
            )}
          </div>
          {loaded.failureReason && <div className="rounded-[12px] border border-down/30 bg-down-soft px-3 py-2.5 text-[12.5px] text-fg">{loaded.failureReason}</div>}

          <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
            <MiniStat label="Balance" value={usd(a.balance)} />
            <MiniStat label="Equity" value={usd(a.equity)} />
            <MiniStat label="P/L" value={signedUsd((a.equity ?? a.initialBalance) - a.initialBalance)} tone={(a.equity ?? 0) >= a.initialBalance ? "up" : "down"} />
            <MiniStat label="Open positions" value={int(a.openPositions)} sub={a.lastEvalAt ? `evaluated ${ago(a.lastEvalAt, now)}` : "not evaluated"} />
          </div>

          <div>
            <div className="k-label mb-2">Rules · live</div>
            <RuleMeters a={a} />
          </div>

          {loaded.payout && (
            <div className="k-row px-4 py-3">
              <div className="flex items-center justify-between">
                <span className="text-[13px] font-medium">Payout quote</span>
                <Chip size="sm" tone={loaded.payout.eligible ? "up" : "neutral"}>
                  {loaded.payout.eligible ? "Eligible" : "Not eligible"}
                </Chip>
              </div>
              <div className="mt-2 grid grid-cols-3 gap-2 text-[12px]">
                <div>
                  <div className="text-fg-3">Profit</div>
                  <div className="k-num font-medium">{usd(loaded.payout.profit)}</div>
                </div>
                <div>
                  <div className="text-fg-3">Trader {pct(loaded.payout.split, 0)}</div>
                  <div className="k-num font-medium text-up">{usd(loaded.payout.traderAmount)}</div>
                </div>
                <div>
                  <div className="text-fg-3">Total</div>
                  <div className="k-num font-medium">{usd(loaded.payout.total)}</div>
                </div>
              </div>
              <div className="mt-2 flex flex-wrap gap-1.5 text-[11.5px] text-fg-3">
                {loaded.payout.eligibleFrom && <span>Eligible from {when(loaded.payout.eligibleFrom)}</span>}
                {loaded.payout.blockers.map((b) => (
                  <Chip key={b} size="sm" tone="warn">
                    {BLOCKER_LABEL[b] ?? b}
                  </Chip>
                ))}
              </div>
            </div>
          )}

          <div>
            <div className="k-label mb-2">Phases</div>
            <ol className="space-y-2">
              {(loaded.phases ?? []).map((p) => (
                <li key={p.id} className="k-row flex items-center justify-between gap-3 px-4 py-2.5 text-[12.5px]">
                  <span className="flex min-w-0 items-center gap-2.5">
                    <span className={cn("grid size-6 shrink-0 place-items-center rounded-full border font-mono text-[11px]", p.funded ? "border-gold/40 bg-gold-soft text-gold" : "border-ember/40 bg-ember-soft text-ember")}>{p.funded ? "F" : p.phaseIndex + 1}</span>
                    <span className="min-w-0">
                      <span className="block font-medium">
                        {p.phase} <span className="font-mono font-normal text-fg-3">{p.login ?? "—"}</span>
                      </span>
                      <span className="block truncate text-[11.5px] text-fg-3">
                        {p.startedAt ? when(p.startedAt) : "not started"}
                        {p.endedAt ? ` → ${when(p.endedAt)}` : ""}
                        {p.endReason ? ` · ${p.endReason}` : ""}
                      </span>
                    </span>
                  </span>
                  <span className="flex shrink-0 items-center gap-2">
                    <span className="k-num text-fg-2">{usd(p.initialBalance, 0)}</span>
                    <PropStatus status={p.status} />
                  </span>
                </li>
              ))}
            </ol>
          </div>

          <div>
            <div className="k-label mb-1">Equity · {a.phase}</div>
            {points.length > 1 ? (
              <EquityChart
                data={points}
                height={200}
                intraday
                showVolume={false}
                color={(a.equity ?? 0) >= a.initialBalance ? "gold" : "down"}
                lines={[
                  ...(r.dailyFloor ? [{ price: r.dailyFloor, label: "Daily floor", tone: "down" as const }] : []),
                  ...(r.ddFloor ? [{ price: r.ddFloor, label: "DD floor", tone: "down" as const }] : []),
                  ...(r.targetAmount && !a.funded ? [{ price: a.initialBalance + r.targetAmount, label: "Target", tone: "up" as const }] : []),
                ]}
              />
            ) : (
              <div className="rounded-[12px] border border-dashed border-line px-3 py-6 text-center text-[12.5px] text-fg-3">No equity samples yet.</div>
            )}
          </div>

          {a.stats && (
            <KeyValue
              rows={[
                ["Trades", int(a.stats.trades)],
                ["Win rate", a.stats.winRate === null || a.stats.winRate === undefined ? "—" : pct(a.stats.winRate)],
                ["Profit factor", a.stats.profitFactor === null || a.stats.profitFactor === undefined ? "—" : a.stats.profitFactor.toFixed(2)],
                ["Volume", `${(a.stats.lots ?? 0).toFixed(2)} lots`],
                ["Best day", a.stats.bestDay ? `${usd(a.stats.bestDay.profit)} · ${a.stats.bestDay.day}` : "—"],
                ["Fee", `${usd(loaded.fee)}${loaded.feeRefunded ? " · refunded" : ""}`],
                ["Split", pct(loaded.split, 0)],
              ]}
            />
          )}

          <div>
            <Tabs
              value={tab}
              onChange={setTab}
              tabs={[
                { value: "events", label: "Events", count: loaded.events.length },
                { value: "flags", label: "Flags", count: loaded.flags.length },
                { value: "payouts", label: "Payouts", count: loaded.payouts.length },
                { value: "audit", label: "Audit", count: loaded.audit.length },
              ]}
            />
            <div className="pt-3">
              {tab === "events" && (
                <List empty="No rule events.">
                  {loaded.events.map((e) => (
                    <li key={e.id} className="py-2.5 text-[12.5px]">
                      <div className="flex items-center justify-between gap-2">
                        <span className="flex items-center gap-2">
                          <SeverityChip severity={e.severity} />
                          <span className="font-medium">{ruleLabel(e.rule)}</span>
                        </span>
                        <span className="text-[11.5px] text-fg-3" title={when(e.at, true)}>
                          {ago(e.at, now)}
                        </span>
                      </div>
                      <div className="mt-1 text-fg-2">{e.message}</div>
                      <div className="k-num mt-0.5 text-[11.5px] text-fg-3">
                        {e.login} · equity {usd(e.equity)}
                        {e.threshold !== null ? ` · threshold ${usd(e.threshold)}` : ""}
                      </div>
                    </li>
                  ))}
                </List>
              )}
              {tab === "flags" && (
                <List empty="No banned-strategy flags.">
                  {loaded.flags.map((f) => (
                    <li key={f.id} className="py-2.5 text-[12.5px]">
                      <div className="flex items-center justify-between gap-2">
                        <span className="font-medium">{ruleLabel(f.kind)}</span>
                        <PropStatus status={f.status} />
                      </div>
                      <div className="mt-1 text-fg-2">{f.summary}</div>
                      <div className="mt-0.5 text-[11.5px] text-fg-3">
                        {when(f.createdAt)}
                        {f.reviewedBy ? ` · reviewed by ${f.reviewedBy}` : ""}
                      </div>
                    </li>
                  ))}
                </List>
              )}
              {tab === "payouts" && (
                <List empty="No payouts.">
                  {loaded.payouts.map((p) => (
                    <li key={p.id} className="flex items-center justify-between gap-3 py-2.5 text-[12.5px]">
                      <span>
                        <span className="font-mono">#{p.id}</span> · {when(p.requestedAt)}
                        <FirstApproval p={p} now={tick} className="block text-[11.5px] text-fg-3" />
                        {p.note && p.status !== "awaiting_second_approval" && <span className="block text-[11.5px] text-fg-3">{p.note}</span>}
                      </span>
                      <span className="flex items-center gap-2">
                        <span className="k-num font-medium">{usd(p.total)}</span>
                        <PropStatus status={p.status} />
                      </span>
                    </li>
                  ))}
                </List>
              )}
              {tab === "audit" && (
                <List empty="No audit entries.">
                  {loaded.audit.map((x) => (
                    <li key={x.id} className="py-2.5 text-[12.5px]">
                      <div className="flex items-center justify-between gap-2">
                        <span className="font-mono text-[12px]">{x.action}</span>
                        <span className="text-[11.5px] text-fg-3">{when(x.at)}</span>
                      </div>
                      <div className="mt-0.5 text-[11.5px] text-fg-3">
                        {x.actorName || x.actor} · {x.actorRole}
                        {x.reason ? ` · ${x.reason}` : ""}
                        {x.note ? ` · ${x.note}` : ""}
                      </div>
                      {x.after !== null && x.after !== undefined && (
                        <div className="mt-1.5 rounded-[10px] border border-line bg-surface-2/60 px-2.5 py-1.5">
                          <JsonView value={x.after} />
                        </div>
                      )}
                    </li>
                  ))}
                </List>
              )}
            </div>
          </div>
        </div>
      )}
      {act.node}
    </Dialog>
  );
}

function List({ children, empty }: { children: React.ReactNode[]; empty: string }) {
  if (!children.length) return <div className="rounded-[12px] border border-dashed border-line px-3 py-4 text-center text-[12.5px] text-fg-3">{empty}</div>;
  return <ul className="divide-y divide-line">{children}</ul>;
}

function ScaleInput({ valueRef, min, cap }: { valueRef: React.MutableRefObject<number>; min: number; cap: number }) {
  const [v, setV] = React.useState(valueRef.current);
  return (
    <div className="flex items-center justify-between gap-3 rounded-[12px] border border-line bg-surface-2 px-3 py-2.5">
      <span className="text-[13px] text-fg-2">New account size</span>
      <NumInput
        size="sm"
        className="w-40"
        prefix="$"
        value={v}
        min={min}
        max={cap || undefined}
        step={1000}
        onChange={(n) => {
          setV(n);
          valueRef.current = n;
        }}
      />
    </div>
  );
}
