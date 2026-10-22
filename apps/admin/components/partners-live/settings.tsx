"use client";

import * as React from "react";
import Link from "next/link";
import { Plus, RefreshCw, RotateCcw, Save, Trash2 } from "lucide-react";
import { Button, Card, CardHeader, Chip, PageHeader, Reveal, Segmented, Skeleton, Toggle, cn } from "@kalks/ui";
import { ChipList, MiniField, NumInput, Select, SettingRow, TextInput } from "@/components/config/kit";
import { day, useApi, when } from "@/components/live/kit";
import { OPTIONS_RATE_MAX, P, ibSend, levelBody, toSettings, type Level, type LevelsDoc, type SelfRefAction, type Settings, type SettingsDoc, type SymbolGroup, type WriteResult } from "./api";
import { PartnersError, ReadOnlyNote, WEEKDAYS, levelStyle, usd, usePerms, useReasonAction } from "./kit";

type Rates = Record<string, Record<string, number>>;
/** Options rate per level key (USD per contract); only levels the service sent `optionsRate` for. */
type OptRates = Record<string, number>;
const ASSET_CLASSES = ["forex", "metals", "indices", "energies", "crypto", "stocks"];

const ratesOf = (levels: Level[]): Rates => Object.fromEntries(levels.map((l) => [l.key, { ...l.rates }]));
const optRatesOf = (levels: Level[]): OptRates => Object.fromEntries(levels.filter((l) => l.optionsRate !== undefined).map((l) => [l.key, l.optionsRate!]));
const optionsRateOk = (v: number) => Number.isFinite(v) && v >= 0 && v <= OPTIONS_RATE_MAX;
const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
const slug = (s: string) =>
  s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 32);

/* ------------------------------------------------------------------ */
/* Cards                                                                */
/* ------------------------------------------------------------------ */

function RateCard({
  s,
  levels,
  rates,
  base,
  opts,
  baseOpts,
  edit,
  onRate,
  onOptRate,
}: {
  s: Settings;
  levels: Level[];
  rates: Rates;
  base: Rates;
  opts: OptRates;
  baseOpts: OptRates;
  edit: boolean;
  onRate: (lk: string, gk: string, v: number) => void;
  onOptRate: (lk: string, v: number) => void;
}) {
  const hasOptions = levels.some((l) => l.key in baseOpts);
  const changed =
    s.symbolGroups.reduce((n, g) => n + levels.filter((l) => (rates[l.key]?.[g.key] ?? 0) !== (base[l.key]?.[g.key] ?? 0)).length, 0) +
    levels.filter((l) => l.key in baseOpts && opts[l.key] !== baseOpts[l.key]).length;
  return (
    <Card className="h-full min-w-0">
      <CardHeader title="Rate card" subtitle={`USD per standard lot${hasOptions ? " (options: per contract)" : ""} · symbol group × level. Tier shares apply on top.`} action={changed > 0 ? <Chip tone="ember" dot>{changed} edited</Chip> : undefined} />
      <div className="overflow-x-auto px-4 pb-5 pt-4 sm:px-6">
        <table className="w-full border-separate border-spacing-0 text-[13px]" style={{ minWidth: 200 + levels.length * 112 }}>
          <thead>
            <tr>
              <th className="rounded-l-[14px] border-y border-l border-line bg-surface-2 px-4 py-3 text-left text-[11.5px] font-medium uppercase tracking-[0.05em] text-fg-3">Symbol group</th>
              {levels.map((l, i) => (
                <th key={l.key} className={cn("border-y border-line bg-surface-2 px-2 py-3 text-right text-[11.5px] font-medium uppercase tracking-[0.05em] text-fg-3", i === levels.length - 1 && "rounded-r-[14px] border-r")}>
                  <span className="inline-flex items-center gap-1.5">
                    <span className="size-2 rounded-full" style={{ background: levelStyle(l.key, levels).color }} />
                    {l.name}
                  </span>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {s.symbolGroups.map((g) => (
              <tr key={g.key}>
                <td className="border-b border-line px-4 py-2">
                  <div className="font-medium text-fg">{g.name || <span className="text-fg-3">Unnamed</span>}</div>
                  <div className="max-w-56 truncate text-[11px] text-fg-3">{g.symbols.length ? g.symbols.join(", ") : g.assetClass ? `All ${g.assetClass}` : "No symbols"}</div>
                </td>
                {levels.map((l) => {
                  const v = rates[l.key]?.[g.key] ?? 0;
                  const edited = v !== (base[l.key]?.[g.key] ?? 0);
                  return (
                    <td key={l.key} className="border-b border-line px-1.5 py-2 text-right">
                      {edit ? (
                        <NumInput size="sm" align="right" prefix="$" value={v} onChange={(x) => onRate(l.key, g.key, x)} step={0.5} min={0} max={1000} decimals={2} className={cn("ml-auto w-[100px]", edited && "border-ember/50 bg-ember-soft")} />
                      ) : (
                        <span className="k-num pr-2 text-fg-2">{usd(v)}</span>
                      )}
                    </td>
                  );
                })}
              </tr>
            ))}
            {hasOptions && (
              <tr>
                <td className="border-b border-line px-4 py-2">
                  <div className="font-medium text-fg">Options (USD / contract)</div>
                  <div className="max-w-56 truncate text-[11px] text-fg-3">Every option series · per contract, round turn</div>
                </td>
                {levels.map((l) => {
                  if (!(l.key in baseOpts)) return <td key={l.key} className="border-b border-line px-3.5 py-2 text-right text-fg-3">—</td>;
                  const v = opts[l.key] ?? 0;
                  const edited = v !== baseOpts[l.key];
                  return (
                    <td key={l.key} className="border-b border-line px-1.5 py-2 text-right">
                      {edit ? (
                        <NumInput size="sm" align="right" prefix="$" value={v} onChange={(x) => onOptRate(l.key, x)} step={0.25} min={0} max={OPTIONS_RATE_MAX} decimals={2} className={cn("ml-auto w-[100px]", edited && "border-ember/50 bg-ember-soft", !optionsRateOk(v) && "border-down/60")} />
                      ) : (
                        <span className={cn("k-num pr-2", v > 0 ? "text-fg-2" : "text-fg-3")}>{usd(v)}</span>
                      )}
                    </td>
                  );
                })}
              </tr>
            )}
            <tr>
              <td className="px-4 py-2.5 text-[12px] text-fg-3">CPA per qualified client</td>
              {levels.map((l) => (
                <td key={l.key} className="k-num px-3.5 py-2.5 text-right text-[12.5px] text-fg-2">
                  {usd(l.cpaAmount)}
                </td>
              ))}
            </tr>
          </tbody>
        </table>
        <div className="mt-3 text-[11.5px] text-fg-3">
          Rates and CPA amounts live on each level. CPA amounts, targets and perks are edited on{" "}
          <Link href="/partners/levels" className="text-fg-2 underline-offset-2 hover:underline">
            Levels
          </Link>
          . New rates apply to deals closed after saving.
          {hasOptions && <> Options are paid per contract at the level&apos;s options rate (0–{OPTIONS_RATE_MAX} USD, 0 = no commission on options); per-lot rates never apply to option deals and option contracts never count as lots.</>}
        </div>
      </div>
    </Card>
  );
}

function TiersCard({ s, set, edit, levels }: { s: Settings; set: (p: Partial<Settings>) => void; edit: boolean; levels: Level[] }) {
  const entry = levels[0];
  const example = entry?.rates["fx-major"] ?? Object.values(entry?.rates ?? {})[0] ?? 0;
  const tiers = s.tiers;
  return (
    <Card className="h-full min-w-0">
      <CardHeader
        title="Tiers"
        subtitle="Share of the level rate paid at each tier up the tree"
        action={
          edit ? (
            <Button size="sm" variant="surface" disabled={tiers.length >= 10} onClick={() => set({ tiers: [...tiers, Math.max(1, Math.round((tiers[tiers.length - 1] ?? 10) / 2))] })}>
              <Plus /> Tier
            </Button>
          ) : undefined
        }
      />
      <div className="space-y-2 px-4 pb-5 pt-4 sm:px-6">
        {tiers.map((t, i) => (
          <div key={i} className="k-row flex items-center gap-3 px-3.5 py-2">
            <span className={cn("grid size-7 shrink-0 place-items-center rounded-full border font-mono text-[11px]", i === 0 ? "border-ember/40 bg-ember-soft text-ember" : "border-line bg-surface-3 text-fg-2")}>T{i + 1}</span>
            <div className="min-w-0 flex-1">
              <div className="truncate text-[12.5px] text-fg-2">{i === 0 ? "The client's own IB" : `${i} level${i > 1 ? "s" : ""} above it`}</div>
              <div className="mt-1 h-1 overflow-hidden rounded-full bg-surface-3">
                <div className="h-full rounded-full bg-ember" style={{ width: `${Math.min(100, t)}%`, opacity: 1 - i * 0.1 }} />
              </div>
            </div>
            {edit ? (
              <NumInput size="sm" align="right" value={t} onChange={(v) => set({ tiers: tiers.map((x, k) => (k === i ? v : x)) })} suffix="%" min={0} max={100} className="w-24" />
            ) : (
              <span className="k-num w-16 text-right font-medium">{t}%</span>
            )}
            <span className="k-num hidden w-14 text-right text-[12px] text-fg-3 sm:inline">{usd((example * t) / 100)}</span>
            {edit && (
              <button type="button" disabled={tiers.length <= 1} onClick={() => set({ tiers: tiers.filter((_, k) => k !== i) })} className="grid size-7 place-items-center rounded-full text-fg-3 hover:bg-surface-3 hover:text-down disabled:opacity-30" aria-label={`Remove tier ${i + 1}`}>
                <Trash2 className="size-3.5" />
              </button>
            )}
          </div>
        ))}
        {entry && (
          <div className="rounded-[12px] border border-dashed border-line px-3.5 py-2.5 text-[12px] text-fg-3">
            Example: 1 lot on a Forex major by a client of a {entry.name} IB ({usd(example)}/lot). Each tier uses its own IB&apos;s level rate.
          </div>
        )}
      </div>
    </Card>
  );
}

function GroupsCard({ s, set, edit, baseKeys }: { s: Settings; set: (p: Partial<Settings>) => void; edit: boolean; baseKeys: Set<string> }) {
  const upd = (i: number, p: Partial<SymbolGroup>) => set({ symbolGroups: s.symbolGroups.map((g, k) => (k === i ? { ...g, ...p } : g)) });
  const add = () => {
    let key = "group";
    let n = 1;
    while (s.symbolGroups.some((g) => g.key === key)) key = `group-${++n}`;
    set({ symbolGroups: [...s.symbolGroups, { key, name: "", assetClass: null, symbols: [] }] });
  };
  return (
    <Card className="min-w-0">
      <CardHeader
        title="Symbol groups"
        subtitle="Which symbols share a rate. Explicit symbols win over the asset class."
        action={
          edit ? (
            <Button size="sm" variant="surface" onClick={add}>
              <Plus /> Group
            </Button>
          ) : undefined
        }
      />
      <div className="space-y-2 px-4 pb-5 pt-4 sm:px-6">
        {s.symbolGroups.map((g, i) => {
          const isNew = !baseKeys.has(g.key);
          return (
            <div key={i} className="k-row grid grid-cols-1 gap-3 px-3.5 py-3 lg:grid-cols-[minmax(160px,1fr)_150px_minmax(200px,2fr)_32px] lg:items-center">
              <div className="min-w-0">
                {edit ? (
                  <TextInput size="sm" value={g.name} onChange={(v) => upd(i, isNew ? { name: v, key: slug(v) || g.key } : { name: v })} placeholder="Group name" />
                ) : (
                  <div className="text-[13px] font-medium">{g.name}</div>
                )}
                <div className="mt-1 font-mono text-[10.5px] text-fg-3">
                  key {g.key}
                  {isNew && " · new"}
                </div>
              </div>
              {edit ? (
                <Select size="sm" value={g.assetClass ?? ""} onChange={(v) => upd(i, { assetClass: v || null })} options={[{ value: "", label: "No asset class" }, ...ASSET_CLASSES.map((a) => ({ value: a, label: `All ${a}` }))]} />
              ) : (
                <span className="text-[12.5px] text-fg-2">{g.assetClass ? `All ${g.assetClass}` : "—"}</span>
              )}
              {edit ? (
                <ChipList values={g.symbols} onChange={(v) => upd(i, { symbols: v.map((x) => x.toUpperCase()) })} placeholder="Add symbol, Enter" tone="neutral" />
              ) : (
                <span className="text-[12px] text-fg-3">{g.symbols.length ? g.symbols.join(", ") : "No explicit symbols"}</span>
              )}
              {edit ? (
                <button type="button" disabled={s.symbolGroups.length <= 1} onClick={() => set({ symbolGroups: s.symbolGroups.filter((_, k) => k !== i) })} className="grid size-7 place-items-center justify-self-end rounded-full text-fg-3 hover:bg-surface-3 hover:text-down disabled:opacity-30" aria-label={`Remove ${g.name || g.key}`}>
                  <Trash2 className="size-3.5" />
                </button>
              ) : (
                <span />
              )}
            </div>
          );
        })}
        <div className="text-[11.5px] text-fg-3">Removing a group also removes its rates from every level. Symbols in no group earn no per-lot commission.</div>
      </div>
    </Card>
  );
}

function RowsCard({ title, subtitle, children, action }: { title: string; subtitle?: string; children: React.ReactNode; action?: React.ReactNode }) {
  return (
    <Card className="h-full min-w-0">
      <CardHeader title={title} subtitle={subtitle} action={action} />
      <div className="divide-y divide-line px-4 pb-4 pt-2 sm:px-6">{children}</div>
    </Card>
  );
}

const Val = ({ children }: { children: React.ReactNode }) => <span className="k-num text-[13px] font-medium text-fg">{children}</span>;

/* ------------------------------------------------------------------ */
/* Page                                                                 */
/* ------------------------------------------------------------------ */

export function LiveProgrammeSettings() {
  const perms = usePerms();
  const sd = useApi<SettingsDoc>(P("settings"));
  const ld = useApi<LevelsDoc>(P("levels"));
  const act = useReasonAction();
  const base = React.useMemo(() => (sd.data ? toSettings(sd.data.settings as unknown as Record<string, unknown>) : null), [sd.data]);
  const levels = React.useMemo(() => [...(ld.data?.levels ?? [])].sort((a, b) => a.rank - b.rank), [ld.data]);
  const baseRates = React.useMemo(() => ratesOf(levels), [levels]);
  const baseOpts = React.useMemo(() => optRatesOf(levels), [levels]);
  const [s, setS] = React.useState<Settings | null>(null);
  const [rates, setRates] = React.useState<Rates>({});
  const [opts, setOpts] = React.useState<OptRates>({});
  React.useEffect(() => setS(base), [base]);
  React.useEffect(() => setRates(baseRates), [baseRates]);
  React.useEffect(() => setOpts(baseOpts), [baseOpts]);

  const edit = perms.write;
  const set = (p: Partial<Settings>) => setS((x) => (x ? { ...x, ...p } : x));
  const setIn = <K extends "cpa" | "payout" | "selfReferral" | "wash">(k: K, p: Partial<Settings[K]>) => setS((x) => (x ? { ...x, [k]: { ...x[k], ...p } } : x));

  const settingsDirty = !!s && !!base && !same(s, base);
  const groupKeys = new Set(s?.symbolGroups.map((g) => g.key) ?? []);
  const baseKeys = new Set(base?.symbolGroups.map((g) => g.key) ?? []);
  const ratesDirty = levels.some((l) => s?.symbolGroups.some((g) => (rates[l.key]?.[g.key] ?? 0) !== (baseRates[l.key]?.[g.key] ?? 0)));
  const optsDirty = levels.some((l) => l.key in baseOpts && opts[l.key] !== baseOpts[l.key]);
  const optsBad = levels.find((l) => l.key in opts && !optionsRateOk(opts[l.key]!));
  const dirty = settingsDirty || ratesDirty || optsDirty;

  const reload = () => {
    sd.reload();
    ld.reload();
  };

  const save = () => {
    if (!s || !base) return;
    const removed = [...baseKeys].filter((k) => !groupKeys.has(k));
    const sections = [
      !same(s.tiers, base.tiers) && "tiers",
      ratesDirty && "rate card",
      optsDirty && "options rates",
      !same(s.symbolGroups, base.symbolGroups) && "symbol groups",
      (s.minTradeSeconds !== base.minTradeSeconds || s.centLotFactor !== base.centLotFactor || !same(s.excludedGroups, base.excludedGroups)) && "qualifying trades",
      !same(s.cpa, base.cpa) && "CPA rules",
      !same(s.payout, base.payout) && "payout schedule",
      s.fourEyesThreshold !== base.fourEyesThreshold && `four-eyes threshold (${usd(base.fourEyesThreshold)} → ${usd(s.fourEyesThreshold)})`,
      (s.maxRebatePct !== base.maxRebatePct || s.maxSplitPct !== base.maxSplitPct) && "rebate / split limits",
      (s.clientVisibility !== base.clientVisibility || s.linkBase !== base.linkBase || s.allowDemotion !== base.allowDemotion) && "visibility and links",
      (!same(s.selfReferral, base.selfReferral) || !same(s.wash, base.wash)) && "abuse rules",
    ].filter(Boolean) as string[];
    act.ask({
      title: "Save programme settings",
      description: `Changes: ${sections.join(", ")}. They apply to deals and events processed after saving; commission already accrued is not recalculated.`,
      confirmLabel: "Save changes",
      body: removed.length ? <div className="rounded-[12px] border border-warn/30 bg-warn-soft px-3.5 py-2.5 text-[12.5px] text-fg">Removing {removed.join(", ")} also deletes its rates on every level.</div> : undefined,
      disabled: optsBad ? `${optsBad.name}: the options rate must be 0–${OPTIONS_RATE_MAX} USD per contract.` : undefined,
      run: async (reason): Promise<WriteResult<unknown>> => {
        // 1) a removed group must first leave every level's rates (the service checks levels against the groups)
        if (settingsDirty && removed.length) {
          const r = await ibSend("levels", { levels: levels.map((l) => ({ ...levelBody(l), rates: Object.fromEntries(Object.entries(l.rates).filter(([k]) => groupKeys.has(k))) })), reason }, "PUT");
          if (!r.ok) return r;
        }
        // 2) settings
        if (settingsDirty) {
          const r = await ibSend("settings", { settings: s, reason }, "PUT");
          if (!r.ok) return r;
        }
        // 3) rates and options rates (after new groups exist)
        if (ratesDirty || optsDirty) {
          const body = levels.map((l) => ({
            ...levelBody(l),
            rates: Object.fromEntries(Object.entries({ ...l.rates, ...(rates[l.key] ?? {}) }).filter(([k]) => groupKeys.has(k))),
            ...(l.key in opts ? { optionsRate: opts[l.key] } : {}),
          }));
          const r = await ibSend("levels", { levels: body, reason }, "PUT");
          if (!r.ok) return r;
        }
        return { ok: true, data: null };
      },
      success: "Programme settings saved",
      onDone: reload,
    });
  };

  const loading = !sd.data || !ld.data || !s;
  const error = sd.error ?? ld.error;

  return (
    <div className="pb-24">
      <PageHeader
        title="Commission plans"
        subtitle="Rate card, tiers, CPA, payouts and abuse rules for the whole IB programme"
        actions={
          <>
            {perms.loaded && !perms.write && <ReadOnlyNote what="change the programme" />}
            <Button variant="surface" onClick={reload}>
              <RefreshCw /> Refresh
            </Button>
            {edit && (
              <>
                <Button
                  variant="ghost"
                  disabled={!dirty}
                  onClick={() => {
                    setS(base);
                    setRates(baseRates);
                    setOpts(baseOpts);
                  }}
                >
                  <RotateCcw /> Discard
                </Button>
                <Button variant="ember" disabled={!dirty} onClick={save}>
                  <Save /> Save changes
                </Button>
              </>
            )}
          </>
        }
      />

      {error && loading ? (
        <PartnersError error={error} onRetry={reload} />
      ) : loading || !s || !base ? (
        <div className="grid grid-cols-1 gap-4 2xl:grid-cols-12">
          <Skeleton className="h-96 w-full rounded-[20px] 2xl:col-span-8" />
          <Skeleton className="h-96 w-full rounded-[20px] 2xl:col-span-4" />
        </div>
      ) : (
        <>
          <div className="mb-4 flex flex-wrap items-center gap-2 text-[12.5px] text-fg-3">
            <Chip size="sm">Version {sd.data!.version}</Chip>
            <span>
              Last saved {when(sd.data!.updatedAt)} by {sd.data!.updatedBy ?? "—"}
            </span>
            {dirty && (
              <Chip size="sm" tone="warn" dot>
                Unsaved changes
              </Chip>
            )}
          </div>

          <div className="grid grid-cols-1 gap-4 2xl:grid-cols-12">
            <Reveal className="min-w-0 2xl:col-span-8">
              <RateCard
                s={s}
                levels={levels}
                rates={rates}
                base={baseRates}
                opts={opts}
                baseOpts={baseOpts}
                edit={edit}
                onRate={(lk, gk, v) => setRates((r) => ({ ...r, [lk]: { ...(r[lk] ?? {}), [gk]: v } }))}
                onOptRate={(lk, v) => setOpts((o) => ({ ...o, [lk]: v }))}
              />
            </Reveal>
            <Reveal delay={0.05} className="min-w-0 2xl:col-span-4">
              <TiersCard s={s} set={set} edit={edit} levels={levels} />
            </Reveal>
          </div>

          <Reveal delay={0.05} className="mt-4 block">
            <GroupsCard s={s} set={set} edit={edit} baseKeys={baseKeys} />
          </Reveal>

          <div className="mt-4 grid grid-cols-1 gap-4 lg:grid-cols-2">
            <Reveal className="min-w-0">
              <RowsCard title="Qualifying trades" subtitle="What counts toward per-lot commission">
                <SettingRow label="Minimum trade duration" hint="Trades held for less earn nothing">
                  {edit ? <NumInput size="sm" className="w-32" value={s.minTradeSeconds} onChange={(v) => set({ minTradeSeconds: Math.round(v) })} min={0} max={86400} step={30} suffix="sec" /> : <Val>{s.minTradeSeconds}s</Val>}
                </SettingRow>
                <SettingRow label="Cent lot factor" hint="1 cent lot counts as this many standard lots">
                  {edit ? <NumInput size="sm" className="w-32" value={s.centLotFactor} onChange={(v) => set({ centLotFactor: v })} min={0.0001} max={1} step={0.01} /> : <Val>{s.centLotFactor}</Val>}
                </SettingRow>
                <div className="py-2.5">
                  <div className="text-[13.5px] font-medium text-fg">Excluded trading groups</div>
                  <div className="mb-2 mt-0.5 text-[12px] text-fg-3">Deals in these account groups never earn commission (for example prop).</div>
                  {edit ? (
                    <ChipList values={s.excludedGroups} onChange={(v) => set({ excludedGroups: v })} placeholder="Add group, Enter" tone="neutral" />
                  ) : (
                    <div className="flex flex-wrap gap-1.5">{s.excludedGroups.length ? s.excludedGroups.map((g) => <Chip key={g} size="sm">{g}</Chip>) : <span className="text-[12.5px] text-fg-3">None</span>}</div>
                  )}
                </div>
              </RowsCard>
            </Reveal>

            <Reveal delay={0.05} className="min-w-0">
              <RowsCard title="CPA" subtitle="One-time amount per qualified referred client (amount set per level)" action={<Toggle checked={s.cpa.enabled} onChange={(v) => edit && setIn("cpa", { enabled: v })} label="CPA enabled" />}>
                <div className={cn(!s.cpa.enabled && "opacity-50")}>
                  <SettingRow label="Minimum first deposit" hint="The client's first live deposit, USD">
                    {edit ? <NumInput size="sm" className="w-32" prefix="$" value={s.cpa.minFirstDeposit} onChange={(v) => setIn("cpa", { minFirstDeposit: v })} min={0} step={50} /> : <Val>{usd(s.cpa.minFirstDeposit, 0)}</Val>}
                  </SettingRow>
                  <SettingRow label="First trade required" hint="A qualifying live trade must follow the deposit">
                    <Toggle checked={s.cpa.requireFirstTrade} onChange={(v) => edit && setIn("cpa", { requireFirstTrade: v })} label="First trade required" />
                  </SettingRow>
                  <SettingRow label="Hold period" hint="Days before an earned CPA becomes payable">
                    {edit ? <NumInput size="sm" className="w-32" value={s.cpa.holdDays} onChange={(v) => setIn("cpa", { holdDays: Math.round(v) })} min={0} max={365} suffix="days" /> : <Val>{s.cpa.holdDays} days</Val>}
                  </SettingRow>
                </div>
              </RowsCard>
            </Reveal>

            <Reveal className="min-w-0">
              <RowsCard title="Payout schedule" subtitle={`Next period closes ${day(sd.data!.nextPayoutClose)}`}>
                <SettingRow label="Schedule">
                  {edit ? (
                    <Segmented size="xs" value={s.payout.schedule} onChange={(v) => setIn("payout", { schedule: v })} options={[{ value: "daily", label: "Daily" }, { value: "weekly", label: "Weekly" }, { value: "monthly", label: "Monthly" }]} />
                  ) : (
                    <Val>{s.payout.schedule}</Val>
                  )}
                </SettingRow>
                {s.payout.schedule === "weekly" && (
                  <SettingRow label="Period closes on">
                    {edit ? <Select size="sm" className="w-36" value={String(s.payout.weekday)} onChange={(v) => setIn("payout", { weekday: Number(v) })} options={WEEKDAYS.map((w, i) => ({ value: String(i + 1), label: w }))} /> : <Val>{WEEKDAYS[s.payout.weekday - 1]}</Val>}
                  </SettingRow>
                )}
                {s.payout.schedule === "monthly" && (
                  <SettingRow label="Day of month" hint="1–28">
                    {edit ? <NumInput size="sm" className="w-28" value={s.payout.monthDay} onChange={(v) => setIn("payout", { monthDay: Math.round(v) })} min={1} max={28} /> : <Val>{s.payout.monthDay}</Val>}
                  </SettingRow>
                )}
                <SettingRow label="Minimum payout" hint="Payees below this carry over to the next batch">
                  {edit ? <NumInput size="sm" className="w-32" prefix="$" value={s.payout.minAmount} onChange={(v) => setIn("payout", { minAmount: v })} min={0} step={5} /> : <Val>{usd(s.payout.minAmount)}</Val>}
                </SettingRow>
                <SettingRow label="Create batch automatically" hint="When a period closes, a batch is created for approval">
                  <Toggle checked={s.payout.autoCreate} onChange={(v) => edit && setIn("payout", { autoCreate: v })} label="Auto-create batches" />
                </SettingRow>
                <SettingRow label="Four-eyes threshold (USD)" hint="Payouts at or above this amount need a second staff member to approve. 0 = every payout.">
                  {edit ? <NumInput size="sm" className="w-32" prefix="$" value={s.fourEyesThreshold} onChange={(v) => set({ fourEyesThreshold: v })} min={0} step={100} /> : <Val>{s.fourEyesThreshold === 0 ? "Every batch" : usd(s.fourEyesThreshold)}</Val>}
                </SettingRow>
              </RowsCard>
            </Reveal>

            <Reveal delay={0.05} className="min-w-0">
              <RowsCard title="Rebates, visibility and links">
                <SettingRow label="Max client rebate" hint="Most an IB may return to its clients, % of its tier-1 amount">
                  {edit ? <NumInput size="sm" className="w-28" value={s.maxRebatePct} onChange={(v) => set({ maxRebatePct: v })} min={0} max={100} suffix="%" /> : <Val>{s.maxRebatePct}%</Val>}
                </SettingRow>
                <SettingRow label="Max sub-IB split" hint="Most an IB may pass down to the IB below it">
                  {edit ? <NumInput size="sm" className="w-28" value={s.maxSplitPct} onChange={(v) => set({ maxSplitPct: v })} min={0} max={100} suffix="%" /> : <Val>{s.maxSplitPct}%</Val>}
                </SettingRow>
                <SettingRow label="Client details shown to IBs" hint={s.clientVisibility === "full" ? "Names, emails and trades. Clients consent to this in the sign-up terms." : "Initials and lots only."}>
                  {edit ? <Segmented size="xs" value={s.clientVisibility} onChange={(v) => set({ clientVisibility: v })} options={[{ value: "full", label: "Full" }, { value: "masked", label: "Masked" }]} /> : <Val>{s.clientVisibility}</Val>}
                </SettingRow>
                <SettingRow label="Allow demotion" hint="Monthly evaluation may also move IBs down">
                  <Toggle checked={s.allowDemotion} onChange={(v) => edit && set({ allowDemotion: v })} label="Allow demotion" />
                </SettingRow>
                <div className="py-2.5">
                  <MiniField label="Referral link base" hint="https://…">
                    {edit ? <TextInput size="sm" mono value={s.linkBase} onChange={(v) => set({ linkBase: v.trim() })} placeholder="https://app.example.com" /> : <span className="font-mono text-[12.5px] text-fg-2">{s.linkBase}</span>}
                  </MiniField>
                </div>
              </RowsCard>
            </Reveal>

            <Reveal className="min-w-0 lg:col-span-2">
              <RowsCard title="Abuse rules" subtitle="Block: no commission from that client until an admin clears the flag. Flag: record only.">
                <div className="grid grid-cols-1 gap-x-8 lg:grid-cols-2">
                  <div className="divide-y divide-line">
                    {(
                      [
                        ["ip", "Self-referral · same IP", "Client and IB seen on the same IP address"],
                        ["device", "Self-referral · same device", "Shared device fingerprint"],
                        ["identity", "Self-referral · same identity", "Matching identity details"],
                      ] as const
                    ).map(([k, label, hint]) => (
                      <SettingRow key={k} label={label} hint={hint}>
                        {edit ? (
                          <Segmented<SelfRefAction>
                            size="xs"
                            value={s.selfReferral[k]}
                            onChange={(v) => setIn("selfReferral", { [k]: v } as Partial<Settings["selfReferral"]>)}
                            options={[
                              { value: "block", label: "Block" },
                              { value: "flag", label: "Flag" },
                              { value: "off", label: "Off" },
                            ]}
                          />
                        ) : (
                          <Val>{s.selfReferral[k]}</Val>
                        )}
                      </SettingRow>
                    ))}
                  </div>
                  <div className="divide-y divide-line">
                    <SettingRow label="Wash-trading and short-trade checks">
                      <Toggle checked={s.wash.enabled} onChange={(v) => edit && setIn("wash", { enabled: v })} label="Wash checks" />
                    </SettingRow>
                    <div className={cn("grid grid-cols-2 gap-3 py-3", !s.wash.enabled && "opacity-50")}>
                      <MiniField label="Opposite-trade window">{edit ? <NumInput size="sm" value={s.wash.windowSecs} onChange={(v) => setIn("wash", { windowSecs: Math.round(v) })} min={1} suffix="sec" /> : <Val>{s.wash.windowSecs}s</Val>}</MiniField>
                      <MiniField label="Volume tolerance">{edit ? <NumInput size="sm" value={s.wash.volumeTolerancePct} onChange={(v) => setIn("wash", { volumeTolerancePct: v })} min={0} suffix="%" /> : <Val>{s.wash.volumeTolerancePct}%</Val>}</MiniField>
                      <MiniField label="Short trades in 24 h" hint="at least">{edit ? <NumInput size="sm" value={s.wash.shortTradesMin} onChange={(v) => setIn("wash", { shortTradesMin: Math.round(v) })} min={1} suffix="trades" /> : <Val>{s.wash.shortTradesMin}</Val>}</MiniField>
                      <MiniField label="Share of client's trades" hint="at least">{edit ? <NumInput size="sm" value={s.wash.shortTradesPct} onChange={(v) => setIn("wash", { shortTradesPct: v })} min={0} max={100} suffix="%" /> : <Val>{s.wash.shortTradesPct}%</Val>}</MiniField>
                    </div>
                  </div>
                </div>
              </RowsCard>
            </Reveal>
          </div>

          {edit && dirty && (
            <div className="fixed inset-x-3 bottom-3 z-40 mx-auto flex max-w-xl items-center gap-3 rounded-full border border-line bg-surface px-4 py-2.5 shadow-[0_18px_40px_-18px_rgba(0,0,0,0.6)] sm:bottom-5">
              <span className="flex-1 text-[12.5px] text-fg-2">Unsaved changes</span>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => {
                  setS(base);
                  setRates(baseRates);
                  setOpts(baseOpts);
                }}
              >
                Discard
              </Button>
              <Button size="sm" variant="ember" onClick={save}>
                <Save /> Save
              </Button>
            </div>
          )}
        </>
      )}
      {act.node}
    </div>
  );
}
