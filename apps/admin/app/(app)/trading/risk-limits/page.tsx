"use client";

import * as React from "react";
import Link from "next/link";
import { AlertTriangle, BellRing, CalendarClock, History, Layers, Newspaper, Plus, RotateCcw, Route as RouteIcon, Save, Scale, ShieldAlert, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, CardHeader, Chip, Dialog, DialogClose, EmptyState, IconButton, PageHeader, Reveal, Segmented, Skeleton, SymbolAvatar, Toggle, buttonVariants, cn, formatNumber } from "@kalks/ui";
import { IS_DEMO } from "@kalks/mock/mode";
import { useApi, useNow, ago } from "@/components/live/kit";
import { useCan } from "@/components/staff-session";
import { DeskDialog, SymbolPicker } from "@/components/trading-desk/kit";
import { DeskStatusChip } from "@/components/trading-desk/status";
import { PnlText, usdCompact } from "@/components/command/kit";
import { tradingWrite } from "@/components/trading-live/kit";
import { ModeChip, RiskError, UsageBar } from "@/components/trading-live/risk-kit";
import {
  DEFAULT_LIMITS,
  DEFAULT_SETTINGS,
  groupOptions,
  serverStamp,
  useExposure,
  useLiveDirectory,
  useRestDesk,
  type AuditEntry,
  type DailyLossLimit,
  type GroupLimitRow,
  type NewsDoc,
  type NewsLimit,
  type RiskLimits,
  type RiskLimitsDoc,
  type RiskMode,
  type RoutingMode,
} from "@/lib/trading-desk";

const RISK_CODE = "DLR-03 · Risk management";
const DAILY0: DailyLossLimit = { thresholdUsd: 100_000, warnPct: 80, mode: "alert", enabled: false };
const NEWS0: NewsLimit = { minutesBefore: 2, minutesAfter: 2, minImpact: 3, marginMultiplier: 1, blockPending: false, mode: "alert", enabled: false };
const clone = <T,>(v: T): T => JSON.parse(JSON.stringify(v)) as T;
const pct = (v: number) => `${formatNumber(v, 0)}%`;

/* ------------------------------------------------------------------ */
/* Small inputs                                                        */
/* ------------------------------------------------------------------ */

/** Number cell: free typing, parsed on change; empty = null when `nullable`. */
function NumInput({ value, onChange, prefix, suffix, w = "w-28", disabled, nullable, digits = 0, label, placeholder }: { value: number | null; onChange: (v: number | null) => void; prefix?: string; suffix?: string; w?: string; disabled?: boolean; nullable?: boolean; digits?: number; label: string; placeholder?: string }) {
  const fmt = React.useCallback((v: number | null) => (v === null ? "" : digits ? String(v) : formatNumber(v, 0)), [digits]);
  const [text, setText] = React.useState(fmt(value));
  const focused = React.useRef(false);
  React.useEffect(() => {
    if (!focused.current) setText(fmt(value));
  }, [value, fmt]);
  return (
    <span className={cn("inline-flex h-8 items-center gap-1 rounded-[10px] border border-line bg-surface-2 px-2 focus-within:border-ember/50", disabled && "opacity-60", w)}>
      {prefix && <span className="text-[11px] text-fg-3">{prefix}</span>}
      <input
        value={text}
        disabled={disabled}
        aria-label={label}
        inputMode="decimal"
        placeholder={placeholder}
        onFocus={() => (focused.current = true)}
        onBlur={() => {
          focused.current = false;
          setText(fmt(value));
        }}
        onChange={(e) => {
          const raw = e.target.value.replace(/[^0-9.]/g, "");
          setText(raw);
          if (!raw) return onChange(nullable ? null : 0);
          const n = Number(raw);
          if (Number.isFinite(n)) onChange(n);
        }}
        className="k-num min-w-0 flex-1 bg-transparent text-right font-mono text-[12.5px] outline-none placeholder:text-fg-3"
      />
      {suffix && <span className="text-[11px] text-fg-3">{suffix}</span>}
    </span>
  );
}

function ModePick({ value, onChange, disabled }: { value: RiskMode; onChange: (m: RiskMode) => void; disabled?: boolean }) {
  return (
    <span className={cn("inline-flex", disabled && "pointer-events-none opacity-60")}>
      <Segmented size="xs" value={value} onChange={onChange} options={[{ value: "alert", label: "Alert" }, { value: "enforce", label: "Enforce" }]} />
    </span>
  );
}

function Th({ children, className }: { children?: React.ReactNode; className?: string }) {
  return <th className={cn("border-y border-line bg-surface-2 px-3 py-2.5 text-right font-medium first:rounded-l-[14px] first:border-l first:pl-4 first:text-left last:rounded-r-[14px] last:border-r", className)}>{children}</th>;
}
const td = "border-b border-line px-3 py-2";

/* ------------------------------------------------------------------ */

export default function RiskLimitsPage() {
  const canWrite = useCan("dealing.write");
  const canPolicy = useCan("dealing.policy");
  const rest = useRestDesk();
  const now = useNow();
  const dir = useLiveDirectory();
  const { exposure } = useExposure();
  const doc = useApi<RiskLimitsDoc>(IS_DEMO ? null : "/api/trading/dealing/risk/limits");
  const news = useApi<NewsDoc>(IS_DEMO ? null : "/api/trading/dealing/risk/news", { refreshMs: 60_000 });
  const auditLimits = useApi<AuditEntry[]>(IS_DEMO ? null : "/api/trading/dealing/audit?action=risk.limits&limit=50");
  const auditSettings = useApi<AuditEntry[]>(IS_DEMO ? null : "/api/trading/dealing/audit?action=risk.settings&limit=20");

  const [demoBase, setDemoBase] = React.useState<RiskLimitsDoc>({ limits: DEFAULT_LIMITS, settings: DEFAULT_SETTINGS, updatedAt: null, updatedBy: null });
  const base: RiskLimitsDoc | null = IS_DEMO ? demoBase : doc.data;
  const [draft, setDraft] = React.useState<RiskLimits | null>(null);
  const baseKey = base ? JSON.stringify(base.limits) : "";
  React.useEffect(() => {
    if (base) setDraft(clone(base.limits));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [baseKey]);
  const dirty = !!draft && JSON.stringify(draft) !== baseKey;
  const [saving, setSaving] = React.useState(false);
  const [adding, setAdding] = React.useState(false);
  const [modeTo, setModeTo] = React.useState<RoutingMode | null>(null);
  const ro = !canWrite;

  const upd = (fn: (d: RiskLimits) => void) =>
    setDraft((d) => {
      if (!d) return d;
      const n = clone(d);
      fn(n);
      return n;
    });

  // every enabled limit in alert mode = the launch plan (alert only for the first two weeks)
  const enforcing = draft
    ? [
        ...draft.exposure.filter((r) => r.enabled && r.mode === "enforce").map((r) => (r.symbol === "*" ? "default exposure cap" : `${r.symbol} exposure`)),
        ...(draft.dailyLoss?.enabled && draft.dailyLoss.mode === "enforce" ? ["daily loss (behaves as alert)"] : []),
        ...draft.groups.filter((g) => g.enabled && g.mode === "enforce").map((g) => `group ${g.group}`),
        ...(draft.news?.enabled && draft.news.mode === "enforce" ? ["news window"] : []),
      ]
    : [];

  const usage = (symbol: string) => exposure?.symbols.find((s) => s.symbol === symbol)?.limit ?? null;
  const daily = draft?.dailyLoss ?? DAILY0;
  const nw = draft?.news ?? NEWS0;
  const houseDaily = exposure?.totals.houseDailyPnlUsd ?? null;
  const dailyUsed = houseDaily !== null && daily.thresholdUsd > 0 ? Math.max(0, (-houseDaily / daily.thresholdUsd) * 100) : 0;

  const groupRows: (GroupLimitRow & { label: string; known: boolean })[] = React.useMemo(() => {
    const opts = groupOptions();
    const rows = opts.map((o) => {
      const r = draft?.groups.find((g) => g.group === o.value);
      return { group: o.value, maxLot: r?.maxLot ?? null, maxPositions: r?.maxPositions ?? null, mode: r?.mode ?? "alert", enabled: r?.enabled ?? false, label: o.label, known: !!r };
    });
    for (const g of draft?.groups ?? []) if (!opts.some((o) => o.value === g.group)) rows.push({ ...g, label: g.group, known: true });
    return rows;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [draft, dir.groups]);
  const setGroup = (code: string, p: Partial<GroupLimitRow>) =>
    upd((d) => {
      const i = d.groups.findIndex((g) => g.group === code);
      if (i < 0) d.groups.push({ group: code, maxLot: null, maxPositions: null, mode: "alert", enabled: true, ...p });
      else d.groups[i] = { ...d.groups[i]!, ...p };
    });

  const audit = React.useMemo(() => [...(auditLimits.data ?? []), ...(auditSettings.data ?? [])].sort((a, b) => Date.parse(b.at) - Date.parse(a.at)), [auditLimits.data, auditSettings.data]);
  const routingMode = base?.settings.routingMode ?? "recommend";

  const reloadAll = () => {
    doc.reload();
    auditLimits.reload();
    auditSettings.reload();
  };

  if (!IS_DEMO && doc.error && !doc.data)
    return (
      <div className="pb-10">
        <PageHeader title="Risk limits" subtitle="Exposure caps, daily loss, group limits and news windows for the dealing desk." />
        <Card>
          <RiskError error={doc.error} onRetry={doc.reload} />
        </Card>
      </div>
    );

  return (
    <div className="pb-24">
      <PageHeader
        title="Risk limits"
        subtitle={
          <span className="inline-flex flex-wrap items-center gap-2">
            Exposure caps per symbol, house daily loss, per-group lot and position caps and news windows. Alert mode raises an alert; enforce mode also rejects new orders. <DeskStatusChip />
          </span>
        }
        actions={
          <div className="flex flex-wrap gap-2">
            <Link href="/trading/exposure" className={buttonVariants({ variant: "surface", size: "md" })}>
              <Scale /> Exposure
            </Link>
            {canWrite && (
              <>
                <Button
                  variant="ghost"
                  size="md"
                  disabled={!dirty}
                  onClick={() => {
                    if (base) setDraft(clone(base.limits));
                    toast("Changes discarded");
                  }}
                >
                  <RotateCcw /> Discard
                </Button>
                <Button variant="ember" size="md" disabled={!dirty} onClick={() => setSaving(true)}>
                  <Save /> Save limits
                </Button>
              </>
            )}
          </div>
        }
      />

      {!draft ? (
        <div className="space-y-4">
          <Skeleton className="h-16 w-full" />
          <Skeleton className="h-72 w-full" />
        </div>
      ) : (
        <>
          <Reveal>
            {enforcing.length === 0 ? (
              <div className="mb-4 flex flex-wrap items-center gap-3 rounded-[16px] border border-warn/30 bg-warn-soft px-5 py-3.5">
                <BellRing className="size-4 text-warn" />
                <span className="flex-1 text-[13px]">
                  <span className="font-medium">Alert only (plan: first 2 weeks).</span> <span className="text-fg-2">Every limit raises alerts on the dealer desk and in Live risk; no order is rejected. Switch a limit to Enforce once its thresholds are calibrated.</span>
                </span>
                <Chip tone="warn">Alert only</Chip>
              </div>
            ) : (
              <div className="mb-4 flex flex-wrap items-center gap-3 rounded-[16px] border border-down/30 bg-down-soft px-5 py-3.5">
                <ShieldAlert className="size-4 text-down" />
                <span className="flex-1 text-[13px]">
                  <span className="font-medium">Enforcing {enforcing.length} limit{enforcing.length > 1 ? "s" : ""}.</span> <span className="text-fg-2">New orders breaching {enforcing.join(", ")} are rejected (exposure_limit, group_max_lot, group_max_positions, news_window).</span>
                </span>
                <Chip tone="down">Enforcing</Chip>
              </div>
            )}
          </Reveal>
          {dirty && (
            <div className="mb-4 flex flex-wrap items-center gap-2 rounded-[14px] border border-ember/30 bg-ember-soft px-4 py-2.5 text-[12.5px]">
              <AlertTriangle className="size-3.5 text-ember" /> Unsaved changes. Save with a reason code to apply them on the engine.
            </div>
          )}

          <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
            {/* exposure caps */}
            <Reveal delay={0.04} className="xl:col-span-8">
              <Card className="h-full">
                <CardHeader
                  title="Exposure caps"
                  subtitle="B-book net USD per symbol. “*” applies to every symbol without its own row."
                  icon={<Scale />}
                  action={
                    canWrite && (
                      <Button size="sm" variant="surface" onClick={() => setAdding(true)}>
                        <Plus /> Add symbol
                      </Button>
                    )
                  }
                />
                <div className="mt-4 overflow-x-auto px-4 pb-5 sm:px-6">
                  <table className="w-full min-w-[760px] border-separate border-spacing-0 text-[13px]">
                    <thead>
                      <tr className="text-[11px] uppercase tracking-[0.05em] text-fg-3">
                        <Th>Symbol</Th>
                        <Th>Cap</Th>
                        <Th>Warn at</Th>
                        <Th className="text-center">Mode</Th>
                        <Th className="text-left">Live usage</Th>
                        <Th className="text-center">On</Th>
                        <Th />
                      </tr>
                    </thead>
                    <tbody>
                      {[...draft.exposure]
                        .sort((a, b) => (a.symbol === "*" ? -1 : b.symbol === "*" ? 1 : a.symbol.localeCompare(b.symbol)))
                        .map((l) => {
                          const u = l.symbol === "*" ? null : usage(l.symbol);
                          const set = (p: Partial<typeof l>) => upd((d) => void (d.exposure = d.exposure.map((x) => (x.symbol === l.symbol ? { ...x, ...p } : x))));
                          return (
                            <tr key={l.symbol} className={cn(!l.enabled && "opacity-55")}>
                              <td className={cn(td, "pl-4")}>
                                {l.symbol === "*" ? (
                                  <span className="flex items-center gap-2.5">
                                    <span className="grid size-[22px] place-items-center rounded-full bg-surface-3 font-mono text-[12px] text-fg-2">*</span>
                                    <span>
                                      <span className="block font-medium">Default</span>
                                      <span className="block text-[11px] text-fg-3">Every other symbol</span>
                                    </span>
                                  </span>
                                ) : (
                                  <span className="flex items-center gap-2.5">
                                    <SymbolAvatar symbol={l.symbol} size={22} />
                                    <span className="font-medium">{l.symbol}</span>
                                  </span>
                                )}
                              </td>
                              <td className={cn(td, "text-right")}>
                                <NumInput label={`Cap ${l.symbol}`} value={l.capUsd} onChange={(v) => set({ capUsd: v ?? 0 })} prefix="$" w="w-36" disabled={ro} />
                              </td>
                              <td className={cn(td, "text-right")}>
                                <NumInput label={`Warn ${l.symbol}`} value={l.warnPct} onChange={(v) => set({ warnPct: Math.min(100, v ?? 0) })} suffix="%" w="w-20" disabled={ro} />
                              </td>
                              <td className={cn(td, "text-center")}>
                                <ModePick value={l.mode} onChange={(m) => set({ mode: m })} disabled={ro} />
                              </td>
                              <td className={td}>
                                {u ? (
                                  <span className="flex items-center gap-2 whitespace-nowrap">
                                    <UsageBar usedPct={u.usedPct} level={u.level} />
                                    {u.level !== "ok" && <Chip size="sm" tone={u.level === "red" ? "down" : "warn"}>{u.level === "red" ? "Over cap" : "Warn"}</Chip>}
                                  </span>
                                ) : (
                                  <span className="text-[11.5px] text-fg-3">{l.symbol === "*" ? "Per symbol" : "No exposure"}</span>
                                )}
                              </td>
                              <td className={cn(td, "text-center")}>
                                <Toggle checked={l.enabled} onChange={(v) => set({ enabled: v })} label={`Limit ${l.symbol}`} disabled={ro} />
                              </td>
                              <td className={cn(td, "pr-2 text-right")}>
                                {canWrite && l.symbol !== "*" && (
                                  <IconButton
                                    aria-label={`Remove ${l.symbol}`}
                                    variant="ghost"
                                    size="sm"
                                    onClick={() => {
                                      upd((d) => void (d.exposure = d.exposure.filter((x) => x.symbol !== l.symbol)));
                                      toast(`${l.symbol} cap removed`, { description: "The default cap applies after you save" });
                                    }}
                                  >
                                    <Trash2 />
                                  </IconButton>
                                )}
                              </td>
                            </tr>
                          );
                        })}
                    </tbody>
                  </table>
                  {!draft.exposure.length && <EmptyState title="No exposure caps" text="Add a default “*” row or a cap per symbol." illustration="shield" />}
                </div>
              </Card>
            </Reveal>

            {/* daily loss + routing mode */}
            <div className="space-y-4 xl:col-span-4">
              <Reveal delay={0.06}>
                <Card>
                  <CardHeader title="House daily loss" subtitle="B-book P&L since server day start" icon={<CalendarClock />} action={<ModeChip mode={daily.mode} enabled={daily.enabled} />} />
                  <div className="space-y-3 px-4 pb-5 pt-4 sm:px-6">
                    <div className="flex items-center justify-between gap-3 text-[12.5px]">
                      <span className="text-fg-2">Enabled</span>
                      <Toggle checked={daily.enabled} onChange={(v) => upd((d) => void (d.dailyLoss = { ...(d.dailyLoss ?? DAILY0), enabled: v }))} label="Daily loss limit" disabled={ro} />
                    </div>
                    <div className="flex items-center justify-between gap-3 text-[12.5px]">
                      <span className="text-fg-2">Loss threshold</span>
                      <NumInput label="Daily loss threshold" value={daily.thresholdUsd} onChange={(v) => upd((d) => void (d.dailyLoss = { ...(d.dailyLoss ?? DAILY0), thresholdUsd: v ?? 0 }))} prefix="$" w="w-36" disabled={ro} />
                    </div>
                    <div className="flex items-center justify-between gap-3 text-[12.5px]">
                      <span className="text-fg-2">Warn at</span>
                      <NumInput label="Daily loss warn" value={daily.warnPct} onChange={(v) => upd((d) => void (d.dailyLoss = { ...(d.dailyLoss ?? DAILY0), warnPct: Math.min(100, v ?? 0) }))} suffix="%" w="w-20" disabled={ro} />
                    </div>
                    <div className="flex items-center justify-between gap-3 text-[12.5px]">
                      <span className="text-fg-2">Mode</span>
                      <ModePick value={daily.mode} onChange={(m) => upd((d) => void (d.dailyLoss = { ...(d.dailyLoss ?? DAILY0), mode: m }))} disabled={ro} />
                    </div>
                    <div className="k-row space-y-1.5 px-3.5 py-2.5">
                      <div className="flex items-center justify-between text-[12px]">
                        <span className="text-fg-3">House daily P&L now</span>
                        {houseDaily === null ? <span className="text-fg-3">—</span> : <PnlText value={houseDaily} className="font-mono text-[12.5px]" />}
                      </div>
                      <UsageBar usedPct={dailyUsed} level={dailyUsed >= 100 ? "red" : dailyUsed >= daily.warnPct ? "amber" : "ok"} className="w-full [&>span:first-child]:flex-1" />
                    </div>
                    <p className="text-[11px] text-fg-3">The daily loss limit only alerts: enforce mode behaves as alert (the house cannot reject client closes).</p>
                  </div>
                </Card>
              </Reveal>
              <Reveal delay={0.08}>
                <Card>
                  <CardHeader title="Routing mode" subtitle="How routing rules act on new trades" icon={<RouteIcon />} action={<Chip size="sm" tone={routingMode === "execute" ? "down" : "warn"} dot>{routingMode === "execute" ? "Execute" : "Recommend"}</Chip>} />
                  <div className="space-y-3 px-4 pb-5 pt-4 sm:px-6">
                    <span className={cn("inline-flex", !canPolicy && "pointer-events-none opacity-60")}>
                      <Segmented size="sm" value={routingMode} onChange={(v) => v !== routingMode && setModeTo(v)} options={[{ value: "recommend", label: "Recommend" }, { value: "execute", label: "Execute" }]} />
                    </span>
                    <p className="text-[12px] leading-relaxed text-fg-2">
                      {routingMode === "recommend"
                        ? "Recommend (launch default, 100% B-book): the engine keeps every trade on the default book. Rules only label trades and queue routing suggestions for a dealer to approve."
                        : "Execute: matching rules route new trades to their book straight away (A-book is held on B while no LP is connected)."}
                    </p>
                    {!canPolicy && <p className="text-[11px] text-fg-3">Changing the routing mode needs the dealing policy permission (Admin or Super Admin).</p>}
                    <Link href="/trading/routing-suggestions" className="inline-flex items-center gap-1.5 text-[12.5px] text-fg-2 hover:text-ember">
                      <RouteIcon className="size-3.5" /> Routing suggestions
                    </Link>
                  </div>
                </Card>
              </Reveal>
            </div>

            {/* group limits */}
            <Reveal delay={0.08} className="xl:col-span-7">
              <Card className="h-full">
                <CardHeader title="Group limits" subtitle="Default max lot per order and max open positions per client (across accounts), per account group" icon={<Layers />} />
                <div className="mt-4 overflow-x-auto px-4 pb-5 sm:px-6">
                  <table className="w-full min-w-[620px] border-separate border-spacing-0 text-[13px]">
                    <thead>
                      <tr className="text-[11px] uppercase tracking-[0.05em] text-fg-3">
                        <Th>Group</Th>
                        <Th>Max lot</Th>
                        <Th>Max positions</Th>
                        <Th className="text-center">Mode</Th>
                        <Th className="text-center">On</Th>
                      </tr>
                    </thead>
                    <tbody>
                      {groupRows.map((g) => (
                        <tr key={g.group} className={cn(!g.enabled && "opacity-55")}>
                          <td className={cn(td, "pl-4")}>
                            <span className="block font-medium">{g.label}</span>
                            <span className="block font-mono text-[11px] text-fg-3">{g.group}</span>
                          </td>
                          <td className={cn(td, "text-right")}>
                            <NumInput label={`Max lot ${g.group}`} value={g.maxLot} nullable digits={2} placeholder="none" onChange={(v) => setGroup(g.group, { maxLot: v })} suffix="lots" w="w-28" disabled={ro} />
                          </td>
                          <td className={cn(td, "text-right")}>
                            <NumInput label={`Max positions ${g.group}`} value={g.maxPositions} nullable placeholder="none" onChange={(v) => setGroup(g.group, { maxPositions: v === null ? null : Math.round(v) })} w="w-24" disabled={ro} />
                          </td>
                          <td className={cn(td, "text-center")}>
                            <ModePick value={g.mode} onChange={(m) => setGroup(g.group, { mode: m })} disabled={ro} />
                          </td>
                          <td className={cn(td, "text-center")}>
                            <Toggle checked={g.enabled} onChange={(v) => setGroup(g.group, { enabled: v })} label={`Group limit ${g.group}`} disabled={ro} />
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                  {!groupRows.length && <EmptyState title="No account groups" text="Groups appear here once the trading engine lists them." illustration="busts_in_silhouette" />}
                  <p className="mt-3 text-[11px] text-fg-3">Empty = no cap. Per-account dealer controls (Dealer desk) still apply on top.</p>
                </div>
              </Card>
            </Reveal>

            {/* news window */}
            <Reveal delay={0.1} className="xl:col-span-5">
              <Card className="h-full">
                <CardHeader title="News window" subtitle="Around high-impact calendar events" icon={<Newspaper />} action={<ModeChip mode={nw.mode} enabled={nw.enabled} />} />
                <div className="space-y-3 px-4 pb-5 pt-4 sm:px-6">
                  <div className="flex items-center justify-between gap-3 text-[12.5px]">
                    <span className="text-fg-2">Enabled</span>
                    <Toggle checked={nw.enabled} onChange={(v) => upd((d) => void (d.news = { ...(d.news ?? NEWS0), enabled: v }))} label="News window" disabled={ro} />
                  </div>
                  <div className="grid grid-cols-2 gap-3 text-[12.5px]">
                    <label className="space-y-1">
                      <span className="block text-fg-2">Minutes before</span>
                      <NumInput label="Minutes before" value={nw.minutesBefore} onChange={(v) => upd((d) => void (d.news = { ...(d.news ?? NEWS0), minutesBefore: Math.round(v ?? 0) }))} suffix="min" w="w-full" disabled={ro} />
                    </label>
                    <label className="space-y-1">
                      <span className="block text-fg-2">Minutes after</span>
                      <NumInput label="Minutes after" value={nw.minutesAfter} onChange={(v) => upd((d) => void (d.news = { ...(d.news ?? NEWS0), minutesAfter: Math.round(v ?? 0) }))} suffix="min" w="w-full" disabled={ro} />
                    </label>
                  </div>
                  <div className="flex flex-wrap items-center justify-between gap-3 text-[12.5px]">
                    <span className="text-fg-2">Minimum impact</span>
                    <span className={cn("inline-flex", ro && "pointer-events-none opacity-60")}>
                      <Segmented size="xs" value={String(nw.minImpact) as "1" | "2" | "3"} onChange={(v) => upd((d) => void (d.news = { ...(d.news ?? NEWS0), minImpact: Number(v) }))} options={[{ value: "1", label: "Low" }, { value: "2", label: "Medium" }, { value: "3", label: "High" }]} />
                    </span>
                  </div>
                  <div className="flex items-center justify-between gap-3 text-[12.5px]">
                    <span className="text-fg-2">
                      Margin multiplier <span className="text-[11px] text-fg-3">(1 = off)</span>
                    </span>
                    <NumInput label="Margin multiplier" value={nw.marginMultiplier} digits={2} onChange={(v) => upd((d) => void (d.news = { ...(d.news ?? NEWS0), marginMultiplier: Math.max(1, v ?? 1) }))} suffix="×" w="w-24" disabled={ro} />
                  </div>
                  <div className="flex items-center justify-between gap-3 text-[12.5px]">
                    <span className="text-fg-2">Block new pending orders</span>
                    <Toggle checked={nw.blockPending} onChange={(v) => upd((d) => void (d.news = { ...(d.news ?? NEWS0), blockPending: v }))} label="Block new pending orders" disabled={ro} />
                  </div>
                  <div className="flex items-center justify-between gap-3 text-[12.5px]">
                    <span className="text-fg-2">Mode</span>
                    <ModePick value={nw.mode} onChange={(m) => upd((d) => void (d.news = { ...(d.news ?? NEWS0), mode: m }))} disabled={ro} />
                  </div>
                  <NewsList doc={news.data} error={news.error} now={now} />
                </div>
              </Card>
            </Reveal>

            {/* audit */}
            <Reveal delay={0.1} className="xl:col-span-12">
              <Card>
                <CardHeader title="Change log" subtitle="Audit entries for risk limits and the routing mode" icon={<History />} action={base?.updatedAt ? <Chip size="sm" tone="neutral">Last saved {ago(base.updatedAt, now)}{base.updatedBy ? ` by ${base.updatedBy}` : ""}</Chip> : undefined} />
                <div className="space-y-1.5 px-4 pb-5 pt-4 sm:px-6">
                  {IS_DEMO && <div className="text-[12.5px] text-fg-3">The change log is kept by the trading engine; saves in this demo stay in the browser.</div>}
                  {!IS_DEMO && auditLimits.error && <RiskError error={auditLimits.error} compact />}
                  {!IS_DEMO && !auditLimits.error && !audit.length && <div className="text-[12.5px] text-fg-3">{auditLimits.data ? "No changes yet." : "Loading…"}</div>}
                  {audit.slice(0, 20).map((a) => (
                    <div key={a.id} className="k-row flex flex-col gap-1.5 px-3.5 py-2.5 sm:flex-row sm:items-start sm:gap-4">
                      <span className="w-36 shrink-0 font-mono text-[11.5px] text-fg-2">{serverStamp(a.at)}</span>
                      <span className="w-40 shrink-0 text-[12.5px]">
                        {a.staff.name}
                        <span className="block text-[10.5px] text-fg-3">{a.staff.role}</span>
                      </span>
                      <span className="min-w-0 flex-1">
                        <Chip size="sm" tone={a.action === "risk.settings" ? "info" : "warn"}>{a.action === "risk.settings" ? "Routing mode" : "Risk limits"}</Chip>
                        <span className="mt-1 flex flex-wrap gap-1">
                          {diffLines(a).map((l) => (
                            <span key={l} className="rounded-md border border-line bg-surface-2 px-1.5 py-0.5 font-mono text-[10.5px] text-fg-3">
                              {l}
                            </span>
                          ))}
                        </span>
                      </span>
                      <span className="text-[12px] sm:w-56 sm:shrink-0">
                        <span className="font-mono text-[10.5px] text-ember">{a.reasonCode.split(" · ")[0]}</span> {a.reasonCode.split(" · ")[1]}
                        {a.note && <span className="block truncate text-[11px] text-fg-3" title={a.note}>“{a.note}”</span>}
                      </span>
                    </div>
                  ))}
                </div>
              </Card>
            </Reveal>
          </div>

          {canWrite && dirty && (
            <div className="fixed inset-x-0 bottom-0 z-30 flex justify-center px-4 pb-[calc(16px+env(safe-area-inset-bottom))]">
              <div className="flex w-full max-w-xl items-center gap-3 rounded-[18px] border border-line bg-surface px-4 py-3 shadow-e2">
                <span className="flex-1 text-[12.5px] text-fg-2">Unsaved risk limit changes</span>
                <Button size="sm" variant="ghost" onClick={() => base && setDraft(clone(base.limits))}>
                  Discard
                </Button>
                <Button size="sm" variant="ember" onClick={() => setSaving(true)}>
                  <Save /> Save
                </Button>
              </div>
            </div>
          )}
        </>
      )}

      <DeskDialog
        open={saving}
        onOpenChange={setSaving}
        title="Save risk limits"
        description={enforcing.length ? `${enforcing.length} limit${enforcing.length > 1 ? "s" : ""} in enforce mode: breaching orders are rejected.` : "Every limit stays in alert mode: alerts only, no order is rejected."}
        defaultCode={RISK_CODE}
        confirmLabel="Save limits"
        onConfirm={async (reason) => {
          if (IS_DEMO) {
            setDemoBase((b) => ({ ...b, limits: clone(draft!), updatedAt: new Date().toISOString(), updatedBy: "Demo" }));
            return { ok: true as const, data: null as unknown, audit: [] };
          }
          const r = await tradingWrite<unknown>("dealing/risk/limits", { limits: draft }, reason, rest, "PUT");
          if (r.ok) reloadAll();
          return r;
        }}
        success="Risk limits saved"
      >
        {enforcing.length > 0 && (
          <div className="rounded-[14px] border border-down/30 bg-down-soft px-3.5 py-2.5 text-[12.5px]">
            Enforcing: {enforcing.join(", ")}.
          </div>
        )}
      </DeskDialog>

      <DeskDialog
        open={!!modeTo}
        onOpenChange={(o) => !o && setModeTo(null)}
        title={modeTo === "execute" ? "Switch routing to Execute" : "Switch routing to Recommend"}
        description={
          modeTo === "execute"
            ? "Matching routing rules will move new trades to their book at once (A-book is held on B while no LP is connected). Pending suggestions stay in the queue."
            : "The engine keeps every trade on the default book. Rules only label trades and queue suggestions for a dealer to approve."
        }
        defaultCode={RISK_CODE}
        requireNote={modeTo === "execute"}
        confirmLabel={modeTo === "execute" ? "Switch to Execute" : "Switch to Recommend"}
        confirmVariant={modeTo === "execute" ? "sell" : "ember"}
        disabled={!canPolicy ? "Needs the dealing policy permission" : undefined}
        onConfirm={async (reason) => {
          if (IS_DEMO) {
            setDemoBase((b) => ({ ...b, settings: { ...b.settings, routingMode: modeTo! } }));
            return { ok: true, data: null, audit: [] };
          }
          const r = await tradingWrite("dealing/risk/settings", { routingMode: modeTo }, reason, rest, "PUT");
          if (r.ok) reloadAll();
          return r;
        }}
        success={modeTo === "execute" ? "Routing mode: Execute" : "Routing mode: Recommend"}
      />

      <AddSymbolDialog
        open={adding}
        onOpenChange={setAdding}
        taken={draft?.exposure.map((x) => x.symbol) ?? []}
        onAdd={(symbol) => {
          const def = draft?.exposure.find((x) => x.symbol === "*");
          upd((d) => void d.exposure.push({ symbol, capUsd: def?.capUsd ?? 1_000_000, warnPct: def?.warnPct ?? 80, mode: "alert", enabled: true }));
          toast.success(`${symbol} cap added`, { description: "Save to apply it on the engine" });
        }}
      />
    </div>
  );
}

function AddSymbolDialog({ open, onOpenChange, taken, onAdd }: { open: boolean; onOpenChange: (o: boolean) => void; taken: string[]; onAdd: (s: string) => void }) {
  const [symbol, setSymbol] = React.useState("XAUUSD");
  const dup = taken.includes(symbol);
  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title="Add exposure cap"
      description="A symbol row overrides the default “*” cap. It starts in alert mode with the default cap."
      footer={
        <>
          <span className="mr-auto text-[11.5px] text-fg-3">{dup ? `${symbol} already has a cap` : null}</span>
          <DialogClose asChild>
            <Button variant="ghost" size="sm">
              Cancel
            </Button>
          </DialogClose>
          <Button
            variant="ember"
            size="sm"
            disabled={dup}
            onClick={() => {
              onAdd(symbol);
              onOpenChange(false);
            }}
          >
            <Plus /> Add {symbol}
          </Button>
        </>
      }
    >
      <SymbolPicker value={symbol} onChange={setSymbol} />
    </Dialog>
  );
}

function NewsList({ doc, error, now }: { doc: NewsDoc | null; error: { code: string; message: string } | null; now: number }) {
  if (IS_DEMO) return <div className="text-[11.5px] text-fg-3">Upcoming windows come from the economic calendar on the trading engine.</div>;
  if (error) return <RiskError error={error} compact />;
  if (!doc) return <Skeleton className="h-12 w-full" />;
  const list = doc.windows.slice(0, 5);
  return (
    <div className="space-y-1.5 border-t border-line pt-3">
      <div className="flex items-center justify-between text-[11px] uppercase tracking-wider text-fg-3">
        Next windows
        {!doc.configured ? <Chip size="sm" tone="warn">Calendar not configured</Chip> : doc.lastOkAt ? <span className="normal-case tracking-normal">calendar {ago(doc.lastOkAt, now)}</span> : null}
      </div>
      {!list.length && <div className="text-[12px] text-fg-3">No upcoming events at this impact.</div>}
      {list.map((w) => (
        <div key={w.id} className={cn("k-row flex items-center gap-3 px-3 py-2", w.active && "border-warn/40 bg-warn-soft")}>
          <span className="min-w-0 flex-1">
            <span className="block truncate text-[12.5px]">{w.title}</span>
            <span className="block text-[11px] text-fg-3">
              <span className="font-mono">{w.currency}</span> · {w.symbols.slice(0, 4).join(", ")}
              {w.symbols.length > 4 ? ` +${w.symbols.length - 4}` : ""}
            </span>
          </span>
          <span className="text-right text-[11px]">
            {w.active ? <Chip size="sm" tone="warn" dot>Active</Chip> : <span className="font-mono text-fg-2">{serverStamp(w.startsAt).slice(7, 12)}</span>}
            <span className="block text-fg-3">{"●".repeat(Math.max(1, Math.min(3, w.impact)))}</span>
          </span>
        </div>
      ))}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Audit diff                                                          */
/* ------------------------------------------------------------------ */

type AnyRec = Record<string, unknown> | null | undefined;
const limitsOf = (x: AnyRec): RiskLimits | null => {
  if (!x) return null;
  const l = (x.limits ?? x) as Partial<RiskLimits>;
  return Array.isArray(l.exposure) ? (l as RiskLimits) : null;
};
const short = (v: unknown) => (typeof v === "number" ? (Math.abs(v) >= 1000 ? usdCompact(v, 1) : formatNumber(v, 2).replace(/\.00$/, "")) : v === null || v === undefined ? "—" : String(v));

/** Short "what changed" lines for a risk.limits / risk.settings audit entry. */
function diffLines(a: AuditEntry): string[] {
  if (a.action === "risk.settings") {
    const b = a.before as AnyRec;
    const f = a.after as AnyRec;
    const pick = (x: AnyRec) => ((x?.settings as AnyRec) ?? x)?.routingMode;
    return [`routing ${short(pick(b))} → ${short(pick(f))}`];
  }
  const b = limitsOf(a.before as AnyRec);
  const f = limitsOf(a.after as AnyRec);
  if (!f) return [];
  const out: string[] = [];
  const cmp = (label: string, x: Record<string, unknown> | null | undefined, y: Record<string, unknown> | null | undefined) => {
    if (!x && y) return void out.push(`${label} added`);
    if (x && !y) return void out.push(`${label} removed`);
    for (const k of Object.keys(y ?? {})) if (JSON.stringify(x?.[k]) !== JSON.stringify(y?.[k])) out.push(`${label} ${k}: ${short(x?.[k])} → ${short(y?.[k])}`);
  };
  const syms = new Set([...(b?.exposure ?? []).map((r) => r.symbol), ...f.exposure.map((r) => r.symbol)]);
  for (const s of syms) cmp(s === "*" ? "default cap" : s, b?.exposure.find((r) => r.symbol === s) as unknown as Record<string, unknown>, f.exposure.find((r) => r.symbol === s) as unknown as Record<string, unknown>);
  cmp("daily loss", b?.dailyLoss as unknown as Record<string, unknown>, f.dailyLoss as unknown as Record<string, unknown>);
  const groups = new Set([...(b?.groups ?? []).map((r) => r.group), ...f.groups.map((r) => r.group)]);
  for (const g of groups) cmp(`group ${g}`, b?.groups.find((r) => r.group === g) as unknown as Record<string, unknown>, f.groups.find((r) => r.group === g) as unknown as Record<string, unknown>);
  cmp("news", b?.news as unknown as Record<string, unknown>, f.news as unknown as Record<string, unknown>);
  return out.length > 8 ? [...out.slice(0, 8), `+${out.length - 8} more`] : out;
}
