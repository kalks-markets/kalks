"use client";

/**
 * Options › Spreads, fees & limits (O15, O28, O33, O42, O49): per account group and underlying, this broker's vol
 * spread (bid at σ − x, ask at σ + x), minimum USD spread, commission per contract capped at a % of premium, the order
 * book's maker / taker fees (docs/OPTIONS-EXCHANGE.md §7: negative maker = rebate; fee = sign × min(|rate| × qty,
 * cap % × premium), the cap being commissionCapPct; min(taker) over the broker's rows must cover max(|maker rebate|)),
 * max contracts per client and the Friday weekend margin add-on. The most specific row wins: (group, underlying) >
 * (group, all) > (all, underlying) > (all, all) > the Kalks default. Below: the order-book parameters per underlying
 * (platform-wide, Kalks staff): price tick, market / limit bands, liquidation band and fee, RFQ quote TTL, mark rules.
 *
 *   GET /api/options/groups        PUT|DELETE /api/options/groups/{group}/{symbol} {…, makerFeePerContract?,
 *                                  takerFeePerContract?, reason}
 *   GET /api/options/underlyings   PUT /api/options/underlyings/{symbol} {premiumTick?, marketBandPct?, limitBandPct?,
 *                                  bandMinTicks?, liqBandPct?, liqFeePct?, rfqQuoteTtlSecs?, markMinQty?,
 *                                  markMaxSpreadMult?, reason}   (bands / liquidation in percent: 10 = 10 %)
 */
import * as React from "react";
import { BadgePercent, BookOpen, Calculator, Coins, Pencil, Plus, RefreshCw, Scale, Trash2 } from "lucide-react";
import { Button, Card, CardHeader, Chip, DataTable, Field, KpiCard, PageHeader, Reveal, Toggle, cn, formatNumber, type Column } from "@kalks/ui";
import { IS_DEMO } from "@kalks/mock/mode";
import { ErrorState, TableSkeleton, ago, useApi, useNow, when } from "@/components/live/kit";
import type { GroupSettings, Underlying } from "./types";
import { NumInput, ReadOnlyHint, REASONS, ReasonDialog, Select, UnderlyingCell, optSend, parseNum, platformBlock, useOpt, useOptPerms, usd, volPts } from "./kit";

type Spec = { key: "volSpread" | "minSpreadUsd" | "commissionPerContract" | "commissionCapPct" | "maxContractsPerClient" | "weekendMarginPct"; label: string; scale?: number; suffix: string; hint: string; max: number; int?: boolean };
const SPECS: Spec[] = [
  { key: "volSpread", label: "Vol spread", scale: 100, suffix: "vol pts", hint: "each side of the mid vol", max: 20 },
  { key: "minSpreadUsd", label: "Min spread", suffix: "USD", hint: "per contract, bid to ask", max: 1000 },
  { key: "commissionPerContract", label: "Commission", suffix: "USD", hint: "per contract, open and close", max: 1000 },
  { key: "commissionCapPct", label: "Commission cap", suffix: "% of premium", hint: "the lower of the two applies", max: 100 },
  { key: "maxContractsPerClient", label: "Max contracts", suffix: "per client", hint: "open, across all series", max: 1_000_000, int: true },
  { key: "weekendMarginPct", label: "Weekend margin", suffix: "% add-on", hint: "on short margin, Fridays", max: 500 },
];

/* ---------------- order-book fees (§7) ---------------- */

type FeeKey = "makerFeePerContract" | "takerFeePerContract";
const FEE_MAX = 1000;
const hasFee = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v);

/**
 * §7: min(taker) ≥ max(|maker rebate|) over ALL rows of this broker (only negative makers are rebates), with the
 * edited row's new values in place of its old ones. Null when the rule holds (or there's nothing to compare).
 */
export function feeRuleViolation(rows: GroupSettings[], edited: { groupCode: string; symbol: string; maker?: number; taker?: number } | null): string | null {
  const all = rows.map((g) => (edited && g.groupCode === edited.groupCode && g.symbol === edited.symbol ? { ...g, makerFeePerContract: edited.maker, takerFeePerContract: edited.taker } : g));
  if (edited && !rows.some((g) => g.groupCode === edited.groupCode && g.symbol === edited.symbol)) all.push({ groupCode: edited.groupCode, symbol: edited.symbol, makerFeePerContract: edited.maker, takerFeePerContract: edited.taker } as GroupSettings);
  const name = (g: GroupSettings) => `${g.groupCode === "*" ? "all groups" : g.groupCode} · ${g.symbol === "*" ? "all" : g.symbol}`;
  const takers = all.filter((g) => hasFee(g.takerFeePerContract));
  const rebates = all.filter((g) => hasFee(g.makerFeePerContract) && g.makerFeePerContract! < 0);
  if (!takers.length || !rebates.length) return null;
  const minT = takers.reduce((a, b) => (b.takerFeePerContract! < a.takerFeePerContract! ? b : a));
  const maxR = rebates.reduce((a, b) => (b.makerFeePerContract! < a.makerFeePerContract! ? b : a));
  if (minT.takerFeePerContract! + 1e-9 >= -maxR.makerFeePerContract!) return null;
  return `The lowest taker fee (${usd(minT.takerFeePerContract, 2)}, ${name(minT)}) must be at least the largest maker rebate (${usd(-maxR.makerFeePerContract!, 2)}, ${name(maxR)}) across all your rows: min(taker) ≥ max(|maker rebate|).`;
}

/** fee = sign × min(|rate| × qty, cap % × premium); a negative maker rate is a rebate (paid to the client). */
export function bookFee(rate: number, contracts: number, premiumTotal: number, capPct: number) {
  return Math.sign(rate) * Math.min(Math.abs(rate) * contracts, (capPct / 100) * premiumTotal);
}

function FeeCell({ v, maker }: { v: number | undefined; maker?: boolean }) {
  if (!hasFee(v)) return <span className="text-[12px] text-fg-3">—</span>;
  if (maker && v < 0)
    return (
      <span className="k-num whitespace-nowrap font-mono text-[12.5px] text-up">
        {usd(-v, 2)} <span className="text-[10.5px] text-fg-3">rebate</span>
      </span>
    );
  return <span className="k-num font-mono text-[12.5px]">{usd(v, 2)}</span>;
}

const rank = (g: GroupSettings, group: string, symbol: string) => (g.groupCode === group && g.symbol === symbol ? 0 : g.groupCode === group && g.symbol === "*" ? 1 : g.groupCode === "*" && g.symbol === symbol ? 2 : g.groupCode === "*" && g.symbol === "*" ? 3 : 9);

export function PricingPage() {
  const perms = useOptPerms();
  const block = perms.config ? null : "Read-only for your role";
  const now = useNow();
  const { data, error, reload } = useOpt<{ groups: GroupSettings[]; default: GroupSettings }>("/api/options/groups", { refreshMs: 60_000 });
  const unders = useOpt<{ underlyings: Underlying[] }>("/api/options/underlyings");
  // this broker's Options account groups (trading engine; CFD / Options account split: only an Options account trades
  // options, so CFD groups are not offered — rows that already exist for one stay listed); demo builds use the seeded set
  const tg = useApi<{ groups: { code: string; name: string; product?: string }[] }>(IS_DEMO ? null : "/api/trading/admin/groups");
  const groupCodes = React.useMemo(() => {
    const engine = tg.data?.groups ?? [];
    const split = engine.some((g) => g.product);
    const fromEngine = engine.filter((g) => !split || g.product === "options").map((g) => g.code);
    const fromRows = (data?.groups ?? []).map((g) => g.groupCode).filter((c) => c !== "*");
    return Array.from(new Set([...(IS_DEMO ? ["options-standard", "options-pro"] : []), ...fromEngine, ...fromRows])).sort();
  }, [tg.data, data]);
  const symbols = (unders.data?.underlyings ?? []).filter((u) => u.enabled).map((u) => u.symbol);
  const [edit, setEdit] = React.useState<{ g: GroupSettings | null; isNew: boolean } | null>(null);
  const [del, setDel] = React.useState<GroupSettings | null>(null);
  const rows = React.useMemo(() => [...(data?.groups ?? [])].sort((a, b) => (a.groupCode === "*" ? -1 : b.groupCode === "*" ? 1 : a.groupCode.localeCompare(b.groupCode)) || (a.symbol === "*" ? -1 : b.symbol === "*" ? 1 : a.symbol.localeCompare(b.symbol))), [data]);
  const def = data?.default ?? rows.find((g) => g.groupCode === "*" && g.symbol === "*") ?? null;
  const feeRule = React.useMemo(() => feeRuleViolation(rows, null), [rows]);

  const cols: Column<GroupSettings>[] = [
    {
      key: "g",
      header: "Group · underlying",
      cell: (g) => (
        <span className="flex flex-col">
          <span className="flex items-center gap-1.5">
            <span className={cn("font-mono text-[12.5px] font-medium", g.groupCode === "*" && "text-ember")}>{g.groupCode === "*" ? "All groups" : g.groupCode}</span>
            <span className="text-fg-3">·</span>
            <span className="font-mono text-[12.5px]">{g.symbol === "*" ? "all underlyings" : g.symbol}</span>
          </span>
          {g.groupCode === "*" && g.symbol === "*" && <span className="text-[10.5px] text-fg-3">broker default</span>}
        </span>
      ),
      sort: (g) => `${g.groupCode}/${g.symbol}`,
      csv: (g) => `${g.groupCode}/${g.symbol}`,
    },
    { key: "vs", header: "Vol spread", align: "right", cell: (g) => <span className="k-num font-mono text-[12.5px]">±{volPts(g.volSpread)}</span>, sort: (g) => g.volSpread, csv: (g) => g.volSpread },
    { key: "ms", header: "Min spread", align: "right", cell: (g) => <span className="k-num font-mono text-[12.5px]">{usd(g.minSpreadUsd, 2)}</span>, sort: (g) => g.minSpreadUsd, csv: (g) => g.minSpreadUsd },
    {
      key: "c",
      header: "Commission",
      align: "right",
      cell: (g) => (
        <span className="k-num whitespace-nowrap font-mono text-[12.5px]">
          {usd(g.commissionPerContract, 2)} <span className="text-fg-3">≤ {formatNumber(g.commissionCapPct, 1)}%</span>
        </span>
      ),
      sort: (g) => g.commissionPerContract,
      csv: (g) => `${g.commissionPerContract} cap ${g.commissionCapPct}%`,
    },
    { key: "mk", header: "Maker", align: "right", cell: (g) => <FeeCell v={g.makerFeePerContract} maker />, sort: (g) => g.makerFeePerContract ?? 0, csv: (g) => g.makerFeePerContract ?? "" },
    { key: "tk", header: "Taker", align: "right", cell: (g) => <FeeCell v={g.takerFeePerContract} />, sort: (g) => g.takerFeePerContract ?? 0, csv: (g) => g.takerFeePerContract ?? "" },
    { key: "mx", header: "Max / client", align: "right", cell: (g) => <span className="k-num font-mono text-[12.5px]">{formatNumber(g.maxContractsPerClient, 0)}</span>, sort: (g) => g.maxContractsPerClient, csv: (g) => g.maxContractsPerClient, hideOn: "md" },
    { key: "wm", header: "Weekend", align: "right", cell: (g) => <span className="k-num font-mono text-[12.5px]">+{formatNumber(g.weekendMarginPct, 0)}%</span>, sort: (g) => g.weekendMarginPct, csv: (g) => g.weekendMarginPct, hideOn: "md" },
    {
      key: "e",
      header: "Options",
      cell: (g) => (
        <Chip size="sm" tone={g.enabled ? "up" : "down"} dot>
          {g.enabled ? "On" : "Off"}
        </Chip>
      ),
      sort: (g) => (g.enabled ? 1 : 0),
      csv: (g) => (g.enabled ? "on" : "off"),
    },
    { key: "u", header: "Updated", cell: (g) => <span className="whitespace-nowrap text-[11.5px] text-fg-3" title={`${when(g.updatedAt)} · ${g.updatedBy}`}>{ago(g.updatedAt, now)}</span>, sort: (g) => g.updatedAt, hideOn: "lg" },
    {
      key: "x",
      header: "",
      align: "right",
      cell: (g) =>
        block ? null : (
          <span className="inline-flex gap-1">
            <Button size="xs" variant="surface" onClick={(e) => (e.stopPropagation(), setEdit({ g, isNew: false }))}>
              <Pencil /> Edit
            </Button>
            {!(g.groupCode === "*" && g.symbol === "*") && (
              <Button size="xs" variant="ghost" onClick={(e) => (e.stopPropagation(), setDel(g))} aria-label={`Delete ${g.groupCode}/${g.symbol}`}>
                <Trash2 />
              </Button>
            )}
          </span>
        ),
    },
  ];

  return (
    <div className="pb-10">
      <PageHeader
        title="Spreads, fees & limits"
        subtitle="Your pricing per Options account group and underlying. The most specific row wins: group + underlying, then group, then underlying, then your default."
        actions={
          <>
            <Button variant="surface" size="lg" onClick={reload}>
              <RefreshCw /> Refresh
            </Button>
            {!block && (
              <Button variant="ember" size="lg" onClick={() => setEdit({ g: null, isNew: true })}>
                <Plus /> Add override
              </Button>
            )}
          </>
        }
      />
      <div className="grid grid-cols-2 gap-4 xl:grid-cols-4">
        <KpiCard label="Default vol spread" icon={<Scale />} value={<span className="k-num">{def ? `±${volPts(def.volSpread)}` : "—"}</span>} chip="vol points each side" />
        <KpiCard label="Default commission" icon={<Coins />} value={<span className="k-num">{def ? usd(def.commissionPerContract, 2) : "—"}</span>} chip={def ? `capped at ${formatNumber(def.commissionCapPct, 1)}% of premium` : "per contract"} delay={0.04} />
        <KpiCard label="Overrides" icon={<BadgePercent />} value={<span className="k-num">{Math.max(0, rows.length - 1)}</span>} chip={`${rows.filter((g) => !g.enabled).length} with options off`} delay={0.08} />
        <KpiCard label="Weekend add-on" icon={<Scale />} value={<span className="k-num">{def ? `+${formatNumber(def.weekendMarginPct, 0)}%` : "—"}</span>} chip="short margin on Fridays" delay={0.12} />
      </div>
      <div className="mt-4 grid grid-cols-1 gap-4 2xl:grid-cols-12">
        <Reveal delay={0.05} className="2xl:col-span-8">
          <Card className="px-4 py-5 sm:px-6">
            <div className="mb-3 flex flex-wrap items-center justify-between gap-2 text-[12.5px] text-fg-3">
              <span>Shared from Kalks: the vol surface and settlement prices. Yours: everything on this page. Maker / taker apply to order-book fills (negative maker = rebate).</span>
              <ReadOnlyHint text={block} />
            </div>
            {feeRule && <div className="mb-3 rounded-[12px] border border-down/30 bg-down-soft px-3 py-2 text-[12.5px]">{feeRule}</div>}
            {error ? <ErrorState error={error} onRetry={reload} /> : !data ? <TableSkeleton /> : <DataTable columns={cols} rows={rows} dense pageSize={30} rowKey={(g) => `${g.groupCode}/${g.symbol}`} onRowClick={block ? undefined : (g) => setEdit({ g, isNew: false })} exportName="options-group-pricing" />}
          </Card>
        </Reveal>
        <Reveal delay={0.08} className="2xl:col-span-4">
          <EffectiveCard rows={rows} groups={groupCodes} symbols={symbols} />
        </Reveal>
      </div>
      <Reveal delay={0.1} className="mt-4">
        <BookParamsCard list={unders.data?.underlyings ?? null} error={unders.error} onRetry={unders.reload} />
      </Reveal>
      <GroupEditor edit={edit} base={def} groups={groupCodes} symbols={symbols} existing={rows} onClose={() => setEdit(null)} onSaved={reload} />
      <ReasonDialog
        open={!!del}
        onOpenChange={(o) => !o && setDel(null)}
        title={`Delete ${del?.groupCode === "*" ? "All groups" : del?.groupCode} · ${del?.symbol === "*" ? "all underlyings" : del?.symbol}`}
        description="Clients in this group fall back to the next matching row at once."
        codes={REASONS.fees}
        confirmLabel="Delete override"
        confirmVariant="down-outline"
        onConfirm={async (reason) => {
          const r = await optSend("DELETE", `/api/options/groups/${encodeURIComponent(del!.groupCode)}/${encodeURIComponent(del!.symbol)}`, { reason });
          if (r.ok) reload();
          return r;
        }}
        success="Override deleted"
      />
    </div>
  );
}

function EffectiveCard({ rows, groups, symbols }: { rows: GroupSettings[]; groups: string[]; symbols: string[] }) {
  const [group, setGroup] = React.useState("");
  const [sym, setSym] = React.useState("");
  const [n, setN] = React.useState("10");
  const [prem, setPrem] = React.useState("45");
  React.useEffect(() => {
    if (!group && groups.length) setGroup(groups[0]!);
    if (!sym && symbols.length) setSym(symbols[0]!);
  }, [groups, symbols, group, sym]);
  const hit = rows.map((g) => ({ g, r: rank(g, group, sym) })).filter((x) => x.r < 9).sort((a, b) => a.r - b.r)[0]?.g ?? null;
  const contracts = parseNum(n) ?? 0;
  const premium = parseNum(prem) ?? 0;
  const commission = hit ? Math.min(hit.commissionPerContract * contracts, (hit.commissionCapPct / 100) * premium * contracts) : null;
  const maker = hit && hasFee(hit.makerFeePerContract) ? bookFee(hit.makerFeePerContract, contracts, premium * contracts, hit.commissionCapPct) : null;
  const taker = hit && hasFee(hit.takerFeePerContract) ? bookFee(hit.takerFeePerContract, contracts, premium * contracts, hit.commissionCapPct) : null;
  return (
    <Card className="pb-5">
      <CardHeader title="Effective for a client" subtitle="Which row prices a client in a group, and the commission and book fees on an example order." icon={<Calculator />} />
      <div className="mt-4 space-y-3 px-6">
        <div className="grid grid-cols-2 gap-2">
          <Field label="Group">
            <Select value={group} onChange={setGroup} label="Group" options={groups.map((g) => ({ value: g, label: g }))} />
          </Field>
          <Field label="Underlying">
            <Select value={sym} onChange={setSym} label="Underlying" options={symbols.map((s) => ({ value: s, label: s }))} />
          </Field>
          <Field label="Contracts">
            <NumInput value={n} onChange={setN} label="Contracts" />
          </Field>
          <Field label="Premium" hint="per contract">
            <NumInput value={prem} onChange={setPrem} label="Premium per contract" suffix="USD" />
          </Field>
        </div>
        {!hit ? (
          <div className="text-[12.5px] text-fg-3">No row matches: the Kalks default applies.</div>
        ) : (
          <div className="space-y-1.5 rounded-[14px] border border-line bg-surface-2/60 px-3.5 py-3 text-[12.5px]">
            <div className="flex justify-between">
              <span className="text-fg-3">Row used</span>
              <span className="font-mono">
                {hit.groupCode === "*" ? "All groups" : hit.groupCode} · {hit.symbol === "*" ? "all" : hit.symbol}
              </span>
            </div>
            <div className="flex justify-between">
              <span className="text-fg-3">Options</span>
              <span className={hit.enabled ? "text-up" : "text-down"}>{hit.enabled ? "Tradable" : "Off for this group"}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-fg-3">Vol spread</span>
              <span className="k-num font-mono">±{volPts(hit.volSpread)} vol</span>
            </div>
            <div className="flex justify-between">
              <span className="text-fg-3">Commission</span>
              <span className="k-num font-mono">
                min({usd(hit.commissionPerContract, 2)} × {formatNumber(contracts, 0)}, {formatNumber(hit.commissionCapPct, 1)}% × {usd(premium * contracts, 2)}) = <span className="text-fg">{usd(commission, 2)}</span>
              </span>
            </div>
            <div className="flex justify-between gap-3">
              <span className="text-fg-3">Book maker</span>
              <span className="k-num text-right font-mono">
                {maker === null ? (
                  <span className="text-fg-3">not set</span>
                ) : (
                  <>
                    {hit.makerFeePerContract! < 0 ? "−" : ""}min({usd(Math.abs(hit.makerFeePerContract!), 2)} × {formatNumber(contracts, 0)}, {formatNumber(hit.commissionCapPct, 1)}% × {usd(premium * contracts, 2)}) ={" "}
                    <span className={maker < 0 ? "text-up" : "text-fg"}>{maker < 0 ? `${usd(-maker, 2)} rebate` : usd(maker, 2)}</span>
                  </>
                )}
              </span>
            </div>
            <div className="flex justify-between gap-3">
              <span className="text-fg-3">Book taker</span>
              <span className="k-num text-right font-mono">
                {taker === null ? (
                  <span className="text-fg-3">not set</span>
                ) : (
                  <>
                    min({usd(hit.takerFeePerContract, 2)} × {formatNumber(contracts, 0)}, {formatNumber(hit.commissionCapPct, 1)}% × {usd(premium * contracts, 2)}) = <span className="text-fg">{usd(taker, 2)}</span>
                  </>
                )}
              </span>
            </div>
            <div className="flex justify-between">
              <span className="text-fg-3">Max contracts</span>
              <span className="k-num font-mono">{formatNumber(hit.maxContractsPerClient, 0)}</span>
            </div>
          </div>
        )}
      </div>
    </Card>
  );
}

function GroupEditor({ edit, base, groups, symbols, existing, onClose, onSaved }: { edit: { g: GroupSettings | null; isNew: boolean } | null; base: GroupSettings | null; groups: string[]; symbols: string[]; existing: GroupSettings[]; onClose: () => void; onSaved: () => void }) {
  const [group, setGroup] = React.useState("*");
  const [sym, setSym] = React.useState("*");
  const [vals, setVals] = React.useState<Record<Spec["key"], string>>({} as Record<Spec["key"], string>);
  const [fees, setFees] = React.useState<Record<FeeKey, string>>({ makerFeePerContract: "", takerFeePerContract: "" });
  const [enabled, setEnabled] = React.useState(true);
  React.useEffect(() => {
    if (!edit) return;
    const src = edit.g ?? base;
    setGroup(edit.g?.groupCode ?? groups[0] ?? "*");
    setSym(edit.g?.symbol ?? "*");
    const v = {} as Record<Spec["key"], string>;
    for (const s of SPECS) v[s.key] = src ? String(+((src[s.key] as number) * (s.scale ?? 1)).toPrecision(10)) : "";
    setVals(v);
    setFees({ makerFeePerContract: hasFee(src?.makerFeePerContract) ? String(src.makerFeePerContract) : "", takerFeePerContract: hasFee(src?.takerFeePerContract) ? String(src.takerFeePerContract) : "" });
    setEnabled(src?.enabled ?? true);
  }, [edit, base, groups]);
  if (!edit) return null;
  const parsed: Partial<Record<Spec["key"], number>> = {};
  let invalid: string | null = null;
  for (const s of SPECS) {
    const n = parseNum(vals[s.key] ?? "");
    if (n === null || Number.isNaN(n) || n < 0 || n > s.max || (s.int && !Number.isInteger(n))) invalid ??= `${s.label}: 0–${formatNumber(s.max, 0)}${s.int ? ", whole number" : ""}`;
    else parsed[s.key] = s.scale ? n / s.scale : n;
  }
  // order-book fees: taker ≥ 0, maker may be negative (a rebate), |value| ≤ 1000; empty = leave as the service has it
  const fee: Partial<Record<FeeKey, number>> = {};
  for (const [k, label] of [["makerFeePerContract", "Maker fee"], ["takerFeePerContract", "Taker fee"]] as [FeeKey, string][]) {
    const n = parseNum(fees[k]);
    if (n === null) continue;
    if (Number.isNaN(n)) invalid ??= `${label}: enter a number`;
    else if (Math.abs(n) > FEE_MAX) invalid ??= `${label}: at most ${formatNumber(FEE_MAX, 0)} USD per contract`;
    else if (k === "takerFeePerContract" && n < 0) invalid ??= "Taker fee: 0 or more (only the maker can get a rebate)";
    else fee[k] = n;
  }
  const feeRule = feeRuleViolation(existing, { groupCode: group, symbol: sym, maker: fee.makerFeePerContract ?? (edit.g ? edit.g.makerFeePerContract : base?.makerFeePerContract), taker: fee.takerFeePerContract ?? (edit.g ? edit.g.takerFeePerContract : base?.takerFeePerContract) });
  if (feeRule) invalid ??= "Fees break the maker-rebate rule (see below)";
  if (edit.isNew && existing.some((g) => g.groupCode === group && g.symbol === sym)) invalid ??= "That group / underlying already has a row: edit it instead";
  const feeChanged = (Object.keys(fee) as FeeKey[]).filter((k) => !edit.g || !hasFee(edit.g[k]) || Math.abs(fee[k]! - edit.g[k]!) > 1e-12).length;
  const changed = edit.g ? SPECS.filter((s) => parsed[s.key] !== undefined && Math.abs((parsed[s.key] as number) - (edit.g![s.key] as number)) > 1e-12).length + (enabled !== edit.g.enabled ? 1 : 0) + feeChanged : 1;
  if (!edit.isNew && !changed) invalid ??= "Nothing changed yet";
  const isDefault = group === "*" && sym === "*";
  return (
    <ReasonDialog
      open
      onOpenChange={(o) => !o && onClose()}
      title={edit.isNew ? "Add a pricing override" : `Edit ${group === "*" ? "All groups" : group} · ${sym === "*" ? "all underlyings" : sym}`}
      description={edit.isNew ? "A new row starts from the settings that apply today." : isDefault ? "Your default: every group and underlying without its own row." : "Applies to new quotes and orders at once."}
      codes={REASONS.fees}
      confirmLabel={edit.isNew ? "Add override" : "Save"}
      disabled={invalid}
      onConfirm={async (reason) => {
        const r = await optSend("PUT", `/api/options/groups/${encodeURIComponent(group)}/${encodeURIComponent(sym)}`, { ...parsed, ...fee, enabled, reason });
        if (r.ok) onSaved();
        return r;
      }}
      success="Pricing saved"
    >
      <div className="grid grid-cols-2 gap-3">
        <Field label="Account group">
          <Select value={group} onChange={setGroup} label="Account group" disabled={!edit.isNew} options={[{ value: "*", label: "All groups" }, ...groups.map((g) => ({ value: g, label: g }))]} />
        </Field>
        <Field label="Underlying">
          <Select value={sym} onChange={setSym} label="Underlying" disabled={!edit.isNew} options={[{ value: "*", label: "All underlyings" }, ...symbols.map((s) => ({ value: s, label: s }))]} />
        </Field>
        {SPECS.map((s) => (
          <Field key={s.key} label={s.label} hint={s.hint}>
            <NumInput value={vals[s.key] ?? ""} onChange={(v) => setVals((x) => ({ ...x, [s.key]: v }))} label={s.label} suffix={s.suffix} />
          </Field>
        ))}
        <div className="col-span-2 mt-1 text-[12px] text-fg-3">Order book (§7): fee = sign × min(|rate| × contracts, commission cap × premium). The Kalks market maker pays 0 / 0.</div>
        <Field label="Maker fee" hint="per contract; negative = rebate">
          <NumInput value={fees.makerFeePerContract} onChange={(v) => setFees((x) => ({ ...x, makerFeePerContract: v }))} label="Maker fee" suffix="USD" placeholder="not set" invalid={Number.isNaN(parseNum(fees.makerFeePerContract) ?? 0)} />
        </Field>
        <Field label="Taker fee" hint="per contract, ≥ 0">
          <NumInput value={fees.takerFeePerContract} onChange={(v) => setFees((x) => ({ ...x, takerFeePerContract: v }))} label="Taker fee" suffix="USD" placeholder="not set" invalid={Number.isNaN(parseNum(fees.takerFeePerContract) ?? 0) || (parseNum(fees.takerFeePerContract) ?? 0) < 0} />
        </Field>
        {feeRule && <div className="col-span-2 rounded-[12px] border border-down/30 bg-down-soft px-3 py-2 text-[12.5px]">{feeRule}</div>}
        <label className="col-span-2 flex items-center gap-2 text-[12.5px]">
          <Toggle checked={enabled} onChange={setEnabled} label="Options enabled" /> Options enabled for {group === "*" ? "all groups" : `group ${group}`}
          {sym !== "*" ? ` on ${sym}` : ""}
        </label>
      </div>
    </ReasonDialog>
  );
}

/* ------------------------------------------------------------------ */
/* Order-book parameters per underlying (platform-wide, §2 §5 §6 §8)   */
/* ------------------------------------------------------------------ */

type BookKey = "premiumTick" | "marketBandPct" | "limitBandPct" | "bandMinTicks" | "liqBandPct" | "liqFeePct" | "rfqQuoteTtlSecs" | "markMinQty" | "markMaxSpreadMult";
type BookSpec = { key: BookKey; label: string; hint: string; suffix?: string; min: number; max?: number; int?: boolean; gt?: boolean };
const BOOK_SPECS: { title: string; text: string; specs: BookSpec[] }[] = [
  {
    title: "Price tick & bands",
    text: "Limits must rest within mark × (1 ± limit band) + min ticks; market orders become IOC limits at the market band.",
    specs: [
      { key: "premiumTick", label: "Price tick", hint: "quote ccy per unit", min: 0, gt: true },
      { key: "bandMinTicks", label: "Band min", hint: "added to both bands", suffix: "ticks", min: 0, int: true },
      { key: "marketBandPct", label: "Market band", hint: "of mark", suffix: "%", min: 0, max: 100 },
      { key: "limitBandPct", label: "Limit band", hint: "of mark", suffix: "%", min: 0, max: 100 },
    ],
  },
  {
    title: "Liquidation",
    text: "Stop-outs close options as reduce-only IOCs inside the liquidation band; the backstop takes the rest at mark ∓ the fee.",
    specs: [
      { key: "liqBandPct", label: "Liquidation band", hint: "of mark", suffix: "%", min: 0, max: 50 },
      { key: "liqFeePct", label: "Backstop fee", hint: "of mark, ≥ 1 tick", suffix: "%", min: 0, max: 50 },
    ],
  },
  {
    title: "RFQ & mark",
    text: "How long an MM combo quote stays firm, and when the mark is clamped inside the book (else the model mid).",
    specs: [
      { key: "rfqQuoteTtlSecs", label: "RFQ quote TTL", hint: "firm for", suffix: "s", min: 1, max: 60, int: true },
      { key: "markMinQty", label: "Mark min size", hint: "each side of the book", suffix: "contracts", min: 0 },
      { key: "markMaxSpreadMult", label: "Mark max spread", hint: "× the model spread", suffix: "×", min: 1 },
    ],
  },
];
const ALL_BOOK = BOOK_SPECS.flatMap((g) => g.specs);

/** §2 defaults: FX pip / 10, XAU 0.01, other metals and oil 0.001; bands 10 % / 50 % + 5 ticks; RFQ firm 5 s. */
function bookDefaults(u: Underlying): Record<BookKey, number> {
  const premiumTick = u.assetClass === "forex" ? +(u.pipSize / 10).toPrecision(6) : u.symbol.startsWith("XAU") ? 0.01 : 0.001;
  return { premiumTick, marketBandPct: 10, limitBandPct: 50, bandMinTicks: 5, liqBandPct: 5, liqFeePct: 2, rfqQuoteTtlSecs: 5, markMinQty: 1, markMaxSpreadMult: 3 };
}
const bookVal = (u: Underlying, k: BookKey) => (typeof u[k] === "number" ? (u[k] as number) : null);

function BookParamsCard({ list, error, onRetry }: { list: Underlying[] | null; error: { code: string; message: string } | null; onRetry: () => void }) {
  const perms = useOptPerms();
  const block = platformBlock(perms);
  const [edit, setEdit] = React.useState<Underlying | null>(null);
  const rows = React.useMemo(() => [...(list ?? [])].sort((a, b) => Number(b.enabled) - Number(a.enabled) || a.sort - b.sort || a.symbol.localeCompare(b.symbol)), [list]);
  const missing = rows.filter((u) => ALL_BOOK.some((s) => bookVal(u, s.key) === null)).length;
  const v = (u: Underlying, k: BookKey, d = 2) => {
    const x = bookVal(u, k);
    return x === null ? <span className="text-fg-3">—</span> : formatNumber(x, d);
  };
  const cols: Column<Underlying>[] = [
    { key: "s", header: "Underlying", cell: (u) => <UnderlyingCell symbol={u.symbol} size={22} sub={u.enabled ? `${u.contractSize} ${u.contractUnit} · ${u.quoteCcy}` : "disabled"} />, sort: (u) => u.symbol, csv: (u) => u.symbol },
    {
      key: "t",
      header: "Tick",
      align: "right",
      cell: (u) => {
        const t = bookVal(u, "premiumTick");
        return (
          <span className="flex flex-col items-end">
            <span className="k-num font-mono text-[12.5px]">{t === null ? <span className="text-fg-3">—</span> : String(t)}</span>
            {t !== null && u.quoteCcy === "USD" && <span className="k-num font-mono text-[10.5px] text-fg-3">{usd(t * u.contractSize, 2)}/ct</span>}
          </span>
        );
      },
      sort: (u) => bookVal(u, "premiumTick") ?? 0,
      csv: (u) => bookVal(u, "premiumTick") ?? "",
    },
    {
      key: "b",
      header: "Market · limit band",
      align: "right",
      cell: (u) => (
        <span className="k-num whitespace-nowrap font-mono text-[12px]">
          {v(u, "marketBandPct", 1)}% · {v(u, "limitBandPct", 1)}% <span className="text-fg-3">+{v(u, "bandMinTicks", 0)} t</span>
        </span>
      ),
      csv: (u) => `${bookVal(u, "marketBandPct") ?? ""}/${bookVal(u, "limitBandPct") ?? ""}/${bookVal(u, "bandMinTicks") ?? ""}`,
    },
    {
      key: "l",
      header: "Liq. band · fee",
      align: "right",
      cell: (u) => (
        <span className="k-num whitespace-nowrap font-mono text-[12px]">
          {v(u, "liqBandPct", 1)}% · {v(u, "liqFeePct", 1)}%
        </span>
      ),
      csv: (u) => `${bookVal(u, "liqBandPct") ?? ""}/${bookVal(u, "liqFeePct") ?? ""}`,
      hideOn: "md",
    },
    { key: "r", header: "RFQ TTL", align: "right", cell: (u) => <span className="k-num font-mono text-[12px]">{v(u, "rfqQuoteTtlSecs", 0)} s</span>, sort: (u) => bookVal(u, "rfqQuoteTtlSecs") ?? 0, csv: (u) => bookVal(u, "rfqQuoteTtlSecs") ?? "", hideOn: "lg" },
    {
      key: "m",
      header: "Mark: min · spread",
      align: "right",
      cell: (u) => (
        <span className="k-num whitespace-nowrap font-mono text-[12px]">
          {v(u, "markMinQty", 0)} ct · {v(u, "markMaxSpreadMult", 1)}×
        </span>
      ),
      csv: (u) => `${bookVal(u, "markMinQty") ?? ""}/${bookVal(u, "markMaxSpreadMult") ?? ""}`,
      hideOn: "lg",
    },
    {
      key: "x",
      header: "",
      align: "right",
      cell: (u) => (
        <Button size="xs" variant="ghost" onClick={(e) => (e.stopPropagation(), setEdit(u))}>
          <Pencil /> {block ? "View" : "Edit"}
        </Button>
      ),
    },
  ];
  return (
    <Card className="pb-5">
      <CardHeader title="Order book parameters" subtitle="Per underlying and platform-wide: the price tick, order bands, liquidation band and backstop fee, RFQ quote TTL and when the mark is clamped inside the book." icon={<BookOpen />} action={<ReadOnlyHint text={block} />} />
      <div className="mt-4 px-4 sm:px-6">
        {missing > 0 && list && <div className="mb-3 rounded-[12px] border border-warn/30 bg-warn-soft px-3 py-2 text-[12px]">The options service doesn&apos;t send every order-book field yet for {missing} underlying{missing === 1 ? "" : "s"} (shown as —). Editing starts from the documented defaults.</div>}
        {error ? <ErrorState error={error} onRetry={onRetry} /> : !list ? <TableSkeleton rows={6} /> : <DataTable columns={cols} rows={rows} dense pageSize={20} rowKey={(u) => u.symbol} onRowClick={setEdit} exportName="options-book-parameters" />}
      </div>
      <BookParamsDialog u={edit} block={block} onClose={() => setEdit(null)} onSaved={onRetry} />
    </Card>
  );
}

function BookParamsDialog({ u, block, onClose, onSaved }: { u: Underlying | null; block: string | null; onClose: () => void; onSaved: () => void }) {
  const [vals, setVals] = React.useState<Record<BookKey, string>>({} as Record<BookKey, string>);
  React.useEffect(() => {
    if (!u) return;
    const d = bookDefaults(u);
    const next = {} as Record<BookKey, string>;
    for (const s of ALL_BOOK) next[s.key] = String(bookVal(u, s.key) ?? d[s.key]);
    setVals(next);
  }, [u]);
  if (!u) return null;
  const patch: Partial<Record<BookKey, number>> = {};
  let invalid: string | null = null;
  for (const s of ALL_BOOK) {
    const n = parseNum(vals[s.key] ?? "");
    if (n === null || Number.isNaN(n)) {
      invalid ??= `${s.label}: enter a number`;
      continue;
    }
    if (s.gt && !(n > s.min)) invalid ??= `${s.label}: above ${s.min}`;
    else if (n < s.min) invalid ??= `${s.label}: at least ${s.min}`;
    else if (s.max !== undefined && n > s.max) invalid ??= `${s.label}: at most ${s.max}${s.suffix === "%" ? " %" : ""}`;
    else if (s.int && !Number.isInteger(n)) invalid ??= `${s.label}: a whole number`;
    const cur = bookVal(u, s.key);
    if (cur === null || Math.abs(n - cur) > 1e-12) patch[s.key] = n;
  }
  const changed = Object.keys(patch);
  const tick = parseNum(vals.premiumTick ?? "");
  return (
    <ReasonDialog
      open
      onOpenChange={(o) => !o && onClose()}
      side="right"
      title={`${u.symbol} · order book`}
      description={`Platform-wide for every broker · ${u.contractSize} ${u.contractUnit} per contract, priced in ${u.quoteCcy}. Takes effect with the next snapshot; resting orders keep their price.`}
      codes={REASONS.config}
      confirmLabel={changed.length ? `Save ${changed.length} change${changed.length === 1 ? "" : "s"}` : "No changes"}
      disabled={block ?? invalid ?? (!changed.length ? "Nothing changed yet" : null)}
      onConfirm={async (reason) => {
        const r = await optSend("PUT", `/api/options/underlyings/${u.symbol}`, { ...patch, reason });
        if (r.ok) onSaved();
        return r;
      }}
      success={`${u.symbol} order-book parameters saved`}
    >
      <fieldset disabled={!!block} className="space-y-6">
        {BOOK_SPECS.map((g) => (
          <div key={g.title}>
            <div className="mb-2.5">
              <div className="text-[13px] font-medium text-fg">{g.title}</div>
              <div className="text-[12px] text-fg-3">{g.text}</div>
            </div>
            <div className="grid grid-cols-2 gap-3">
              {g.specs.map((s) => (
                <Field key={s.key} label={s.label} hint={s.key === "premiumTick" && tick && u.quoteCcy === "USD" ? `${usd(tick * u.contractSize, 2)} per contract` : s.hint}>
                  <NumInput value={vals[s.key] ?? ""} onChange={(x) => setVals((p) => ({ ...p, [s.key]: x }))} label={s.label} suffix={s.suffix} invalid={Number.isNaN(parseNum(vals[s.key] ?? "") ?? 0)} />
                </Field>
              ))}
            </div>
          </div>
        ))}
        {changed.length > 0 && (
          <div className="rounded-[14px] border border-line bg-surface-2/60 px-3.5 py-2.5 text-[12px] text-fg-3">
            Changing: <span className="text-fg-2">{changed.join(", ")}</span>
          </div>
        )}
      </fieldset>
    </ReasonDialog>
  );
}
