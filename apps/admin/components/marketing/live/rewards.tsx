"use client";

import * as React from "react";
import { Check, Coins, Gift, Pencil, Plus, RefreshCw, RotateCcw, Trash2, Users, Wallet } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, CardHeader, Chip, DataTable, Input, KpiCard, PageHeader, Reveal, Toggle, cn, type Column } from "@kalks/ui";
import { FilterSelect, Pager, TableSkeleton, day, qs, useApi, useDebounced, when } from "@/components/live/kit";
import { ErrorBanner } from "@/components/trading-desk/kit";
import { AWAITING, FirstApproval, FourEyesDecide, fourEyesRule } from "./four-eyes";
import { M, errText, mkSend, type AppliesTo, type CatalogueItem, type CatalogueKind, type EarnRule, type Member, type Overview, type Paged, type Redemption, type Settings, type Tier } from "./api";
import {
  AreaF,
  ClientCell,
  EmptyNote,
  FormDialog,
  MkError,
  NumF,
  PAY_STATUS,
  ReadOnlyNote,
  SelectF,
  StatusPill,
  TextF,
  ToggleRow,
  int,
  num,
  numOrNull,
  numStr,
  splitList,
  usd,
  usdK,
  useAction,
  usePerms,
} from "./kit";

const ASSET_CLASSES = ["forex", "metals", "indices", "energies", "crypto", "stocks"];
const KIND_LABEL: Record<CatalogueKind, string> = { cashback: "Wallet cashback", bonus_credit: "Bonus credit", fee_discount: "Fee discount" };
const PER = 25;

export function LiveRewards() {
  const perms = usePerms();
  const act = useAction();
  const ov = useApi<Overview>(M("overview"));
  const tiers = useApi<{ tiers: Tier[] }>(M("tiers"));
  const [tick, setTick] = React.useState(0);
  const l = ov.data?.loyalty;
  const tierList = React.useMemo(() => [...(tiers.data?.tiers ?? [])].sort((a, b) => a.rank - b.rank), [tiers.data]);

  return (
    <div className="pb-16">
      <PageHeader
        title="Rewards & loyalty"
        subtitle="Points per lot with tier multipliers, the redemption catalogue, and every client's balance."
        actions={
          <>
            {perms.loaded && !perms.write && <ReadOnlyNote what="edit the programme" />}
            <Button variant="surface" onClick={() => (ov.reload(), tiers.reload(), setTick((n) => n + 1))}>
              <RefreshCw /> Refresh
            </Button>
          </>
        }
      />

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <KpiCard label="Loyalty members" icon={<Users />} value={<span className="k-num">{l ? int(l.members) : "—"}</span>} chip="Clients with points" />
        <KpiCard label="Points issued · 30d" icon={<Coins />} value={<span className="k-num">{l ? int(l.pointsIssued30d) : "—"}</span>} chip="Trading, bonus and promo" delay={0.05} />
        <KpiCard label="Points redeemed · 30d" icon={<Gift />} value={<span className="k-num">{l ? int(l.pointsRedeemed30d) : "—"}</span>} chip={l && l.pointsIssued30d ? `${((l.pointsRedeemed30d / l.pointsIssued30d) * 100).toFixed(1)}% of issued` : "Catalogue redemptions"} chipTone="gold" delay={0.1} />
        <KpiCard label="Points liability" icon={<Wallet />} value={<span className="k-num">{l ? usdK(l.liabilityUsd) : "—"}</span>} chip={l ? `${int(l.liabilityPoints)} points outstanding` : "At the point value"} chipTone="warn" delay={0.15} />
      </div>
      {ov.error && !ov.data && <MkError className="mt-4" error={ov.error} onRetry={ov.reload} />}

      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal delay={0.05} className="xl:col-span-4">
          <SettingsCard canWrite={perms.write} />
        </Reveal>
        <Reveal delay={0.1} className="xl:col-span-8">
          <TiersCard tiers={tierList} loading={!tiers.data} error={tiers.error} reload={tiers.reload} canWrite={perms.write} />
        </Reveal>
      </div>

      <Reveal delay={0.1} className="mt-4">
        <RulesCard canWrite={perms.write} ask={act.ask} />
      </Reveal>

      <Reveal delay={0.1} className="mt-4">
        <CatalogueCard canWrite={perms.write} tiers={tierList} ask={act.ask} />
      </Reveal>

      <Reveal delay={0.1} className="mt-4">
        <RedemptionsCard canApprove={perms.approve} tick={tick} ask={act.ask} />
      </Reveal>

      <Reveal delay={0.1} className="mt-4">
        <MembersCard canApprove={perms.approve} tick={tick} onChanged={() => ov.reload()} />
      </Reveal>
      {act.node}
    </div>
  );
}

type Ask = ReturnType<typeof useAction>["ask"];

/* ------------------------------------------------------------------ */
/* Settings                                                             */
/* ------------------------------------------------------------------ */

function SettingsCard({ canWrite }: { canWrite: boolean }) {
  const { data, error, reload } = useApi<Settings>(M("settings"));
  const [d, setD] = React.useState({ pointValue: "", minHoldSeconds: "", pointsExpiryMonths: "", cashbackHoldHours: "" });
  const [busy, setBusy] = React.useState(false);
  const [err, setErr] = React.useState<string | null>(null);
  React.useEffect(() => {
    if (data) setD({ pointValue: numStr(data.pointValue), minHoldSeconds: numStr(data.minHoldSeconds), pointsExpiryMonths: numStr(data.pointsExpiryMonths), cashbackHoldHours: numStr(data.cashbackHoldHours) });
  }, [data]);
  const dirty = !!data && (Number(d.pointValue) !== data.pointValue || Number(d.minHoldSeconds) !== data.minHoldSeconds || Number(d.pointsExpiryMonths) !== data.pointsExpiryMonths || Number(d.cashbackHoldHours) !== data.cashbackHoldHours);
  const save = async () => {
    if (!data) return;
    setErr(null);
    setBusy(true);
    const r = await mkSend<Settings>(
      "settings",
      { ...data, pointValue: Number(d.pointValue), minHoldSeconds: Math.round(Number(d.minHoldSeconds)), pointsExpiryMonths: Math.round(Number(d.pointsExpiryMonths)), cashbackHoldHours: Math.round(Number(d.cashbackHoldHours)) },
      "PUT",
    );
    setBusy(false);
    if (!r.ok) {
      setErr(errText(r.error));
      return;
    }
    toast.success("Loyalty settings saved", { description: "Recorded in the marketing audit log" });
    reload();
  };
  return (
    <Card className="h-full">
      <CardHeader
        title="Programme settings"
        subtitle="Point value, holds and expiry"
        action={
          canWrite && (
            <Button size="sm" variant={dirty ? "ember" : "surface"} disabled={!dirty || busy} onClick={save}>
              <Check /> {busy ? "Saving…" : "Save"}
            </Button>
          )
        }
      />
      <div className="mt-4 space-y-3 px-4 pb-6 sm:px-6">
        {error && !data ? (
          <MkError error={error} onRetry={reload} />
        ) : !data ? (
          <TableSkeleton rows={3} />
        ) : (
          <fieldset disabled={!canWrite} className="grid grid-cols-2 gap-3">
            <NumF label="Point value" value={d.pointValue} onChange={(v) => setD((x) => ({ ...x, pointValue: v }))} prefix="$" suffix="/ pt" hint="display + cost" />
            <NumF label="Minimum hold" value={d.minHoldSeconds} onChange={(v) => setD((x) => ({ ...x, minHoldSeconds: v }))} suffix="sec" step={1} />
            <NumF label="Points expiry" value={d.pointsExpiryMonths} onChange={(v) => setD((x) => ({ ...x, pointsExpiryMonths: v }))} suffix="months" step={1} hint="without earning" />
            <NumF label="Cashback hold" value={d.cashbackHoldHours} onChange={(v) => setD((x) => ({ ...x, cashbackHoldHours: v }))} suffix="hours" step={1} />
          </fieldset>
        )}
        <ErrorBanner error={err} />
        {data && <div className="text-[11.5px] text-fg-3">100 points are worth {usd(100 * (Number(d.pointValue) || 0))}.</div>}
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Tiers (full replace)                                                 */
/* ------------------------------------------------------------------ */

type TierRow = { key: string; name: string; minPoints: string; multiplier: string; perks: string };

function TiersCard({ tiers, loading, error, reload, canWrite }: { tiers: Tier[]; loading: boolean; error: { code: string; message: string } | null; reload: () => void; canWrite: boolean }) {
  const [rows, setRows] = React.useState<TierRow[]>([]);
  const [busy, setBusy] = React.useState(false);
  const [err, setErr] = React.useState<string | null>(null);
  const reset = React.useCallback(() => setRows(tiers.map((t) => ({ key: t.key, name: t.name, minPoints: numStr(t.minPoints), multiplier: numStr(t.multiplier), perks: (t.perks ?? []).join("; ") }))), [tiers]);
  React.useEffect(reset, [reset]);
  const orig = JSON.stringify(tiers.map((t) => ({ key: t.key, name: t.name, minPoints: numStr(t.minPoints), multiplier: numStr(t.multiplier), perks: (t.perks ?? []).join("; ") })));
  const dirty = JSON.stringify(rows) !== orig;
  const upd = (i: number, p: Partial<TierRow>) => setRows((x) => x.map((r, j) => (j === i ? { ...r, ...p } : r)));

  const save = async () => {
    setErr(null);
    const sorted = [...rows].sort((a, b) => Number(a.minPoints) - Number(b.minPoints));
    if (sorted.some((r) => !r.key.trim() || !r.name.trim())) return setErr("Every tier needs a key and a name.");
    if (new Set(sorted.map((r) => r.key.trim())).size !== sorted.length) return setErr("Tier keys must be unique.");
    if (sorted.some((r) => !(Number(r.multiplier) > 0))) return setErr("Multipliers must be above 0.");
    const body = {
      tiers: sorted.map((r, i) => ({ key: r.key.trim(), name: r.name.trim(), rank: i + 1, minPoints: Math.round(Number(r.minPoints) || 0), multiplier: Number(r.multiplier), perks: r.perks.split(";").map((p) => p.trim()).filter(Boolean) })),
    };
    setBusy(true);
    const r = await mkSend("tiers", body, "PUT");
    setBusy(false);
    if (!r.ok) return setErr(errText(r.error));
    toast.success("Tiers saved", { description: "Recorded in the marketing audit log" });
    reload();
  };

  return (
    <Card className="h-full">
      <CardHeader
        title="Tiers"
        subtitle="By points earned in the last 12 months · the multiplier applies to trading points"
        action={
          canWrite && (
            <>
              {dirty && (
                <Button size="sm" variant="ghost" onClick={reset}>
                  <RotateCcw /> Reset
                </Button>
              )}
              <Button size="sm" variant="surface" onClick={() => setRows((x) => [...x, { key: `tier${x.length + 1}`, name: "", minPoints: String((Number(x[x.length - 1]?.minPoints) || 0) + 1000), multiplier: "1", perks: "" }])}>
                <Plus /> Add tier
              </Button>
              <Button size="sm" variant={dirty ? "ember" : "surface"} disabled={!dirty || busy} onClick={save}>
                <Check /> {busy ? "Saving…" : "Save tiers"}
              </Button>
            </>
          )
        }
      />
      <div className="mt-4 px-4 pb-6 sm:px-6">
        {error && loading ? (
          <MkError error={error} onRetry={reload} />
        ) : loading ? (
          <TableSkeleton rows={4} />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full min-w-[640px] text-[13px]">
              <thead>
                <tr className="text-left text-[11px] uppercase tracking-[0.05em] text-fg-3">
                  <th className="pb-2 font-medium">#</th>
                  <th className="pb-2 font-medium">Key</th>
                  <th className="pb-2 font-medium">Name</th>
                  <th className="pb-2 font-medium">Min points</th>
                  <th className="pb-2 font-medium">Multiplier</th>
                  <th className="pb-2 font-medium">Perks (; separated)</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {rows.map((r, i) => (
                  <tr key={i} className="border-t border-line">
                    <td className="k-num py-1.5 pr-2 text-fg-3">{i + 1}</td>
                    <td className="py-1.5 pr-2">
                      <Input aria-label={`Tier ${i + 1} key`} value={r.key} disabled={!canWrite} onChange={(e) => upd(i, { key: e.target.value.toLowerCase().replace(/[^a-z0-9_-]/g, "") })} className="h-9 w-24 rounded-[10px] px-2.5" inputClassName="font-mono text-[12.5px]" />
                    </td>
                    <td className="py-1.5 pr-2">
                      <Input aria-label={`Tier ${i + 1} label`} value={r.name} disabled={!canWrite} onChange={(e) => upd(i, { name: e.target.value })} className="h-9 w-28 rounded-[10px] px-2.5" />
                    </td>
                    <td className="py-1.5 pr-2">
                      <Input aria-label={`Tier ${i + 1} min points`} type="number" value={r.minPoints} disabled={!canWrite} onChange={(e) => upd(i, { minPoints: e.target.value })} className="h-9 w-28 rounded-[10px] px-2.5" inputClassName="k-num" />
                    </td>
                    <td className="py-1.5 pr-2">
                      <Input aria-label={`Tier ${i + 1} multiplier`} type="number" step="0.05" value={r.multiplier} disabled={!canWrite} onChange={(e) => upd(i, { multiplier: e.target.value })} className="h-9 w-20 rounded-[10px] px-2.5" inputClassName="k-num" />
                    </td>
                    <td className="py-1.5 pr-2">
                      <Input aria-label={`Tier ${i + 1} perks`} value={r.perks} disabled={!canWrite} onChange={(e) => upd(i, { perks: e.target.value })} className="h-9 min-w-48 rounded-[10px] px-2.5 text-[12.5px]" />
                    </td>
                    <td className="py-1.5 text-right">
                      {canWrite && rows.length > 1 && (
                        <button type="button" onClick={() => setRows((x) => x.filter((_, j) => j !== i))} className="grid size-8 place-items-center rounded-full border border-line text-fg-3 hover:text-down" aria-label={`Remove tier ${i + 1}`}>
                          <Trash2 className="size-3.5" />
                        </button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {rows.length === 0 && <EmptyNote title="No tiers" text="Add at least one tier (the entry tier with 0 points)." />}
          </div>
        )}
        <div className="mt-3">
          <ErrorBanner error={err} />
        </div>
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Earning rules                                                        */
/* ------------------------------------------------------------------ */

function matchText(r: EarnRule) {
  if (r.symbols?.length) return r.symbols.join(", ");
  if (r.assetClass) return r.assetClass;
  return "Any symbol";
}

function RulesCard({ canWrite, ask }: { canWrite: boolean; ask: Ask }) {
  const { data, error, reload } = useApi<{ items: EarnRule[] }>(M("rules"));
  const [editing, setEditing] = React.useState<EarnRule | null>(null);
  const [open, setOpen] = React.useState(false);
  const rows = [...(data?.items ?? [])].sort((a, b) => a.priority - b.priority || a.id - b.id);
  const toggle = (r: EarnRule) =>
    ask({
      title: r.active ? `Turn off "${r.name}"` : `Turn on "${r.name}"`,
      description: r.active ? "Deals that matched this rule fall through to the next matching rule." : "Matching deals earn with this rule again.",
      confirmLabel: r.active ? "Turn off" : "Turn on",
      note: "none",
      run: () => mkSend(`rules/${r.id}`, { active: !r.active }, "PATCH"),
      success: r.active ? "Rule turned off" : "Rule turned on",
      onDone: reload,
    });
  const cols: Column<EarnRule>[] = [
    { key: "p", header: "Priority", cell: (r) => <span className="k-num text-fg-2">{r.priority}</span>, sort: (r) => r.priority, csv: (r) => r.priority },
    { key: "n", header: "Rule", cell: (r) => <span className="block max-w-56"><span className="block truncate font-medium">{r.name}</span><span className="block truncate text-[11.5px] text-fg-3">{matchText(r)}</span></span>, csv: (r) => r.name },
    { key: "g", header: "Groups", hideOn: "md", cell: (r) => <span className="text-[12.5px] text-fg-2">{r.accountGroups?.length ? r.accountGroups.join(", ") : "All"}</span>, csv: (r) => r.accountGroups.join(" ") },
    { key: "t", header: "Accounts", cell: (r) => <Chip size="sm">{r.accountType === "any" ? "Live + demo" : r.accountType}</Chip>, csv: (r) => r.accountType },
    { key: "pts", header: "Points / lot", align: "right", cell: (r) => <span className="k-num font-medium text-gold">{num(r.pointsPerLot)}</span>, sort: (r) => r.pointsPerLot, csv: (r) => r.pointsPerLot },
    {
      key: "a",
      header: "Active",
      align: "right",
      cell: (r) => (
        <span className="flex items-center justify-end gap-2" onClick={(e) => e.stopPropagation()}>
          {canWrite ? <Toggle checked={r.active} onChange={() => toggle(r)} label={`${r.name} active`} /> : <StatusPill map={{ true: { label: "On", tone: "up" }, false: { label: "Off", tone: "neutral" } }} status={String(r.active)} />}
          {canWrite && (
            <Button size="xs" variant="surface" onClick={() => (setEditing(r), setOpen(true))} aria-label={`Edit ${r.name}`}>
              <Pencil />
            </Button>
          )}
        </span>
      ),
      csv: (r) => (r.active ? "on" : "off"),
    },
  ];
  return (
    <Card>
      <CardHeader
        title="Earning rules"
        subtitle="Points = floor(lots × points per lot × tier multiplier). The first active matching rule wins (priority ascending)."
        action={
          canWrite && (
            <Button size="sm" variant="ember" onClick={() => (setEditing(null), setOpen(true))} data-testid="new-rule">
              <Plus /> New rule
            </Button>
          )
        }
      />
      <div className="mt-4 px-4 pb-5 sm:px-6">
        {error && !data ? <MkError error={error} onRetry={reload} /> : !data ? <TableSkeleton rows={4} /> : <DataTable columns={cols} rows={rows} pageSize={20} dense rowKey={(r) => String(r.id)} exportName="earning-rules" empty={<EmptyNote className="mt-3" title="No earning rules" text="Without a matching rule, closed deals earn no points." />} />}
      </div>
      <RuleDialog open={open} onOpenChange={setOpen} rule={editing} onSaved={reload} />
    </Card>
  );
}

function RuleDialog({ open, onOpenChange, rule, onSaved }: { open: boolean; onOpenChange: (o: boolean) => void; rule: EarnRule | null; onSaved: () => void }) {
  const [name, setName] = React.useState("");
  const [match, setMatch] = React.useState<"any" | "class" | "symbols">("class");
  const [assetClass, setAssetClass] = React.useState("forex");
  const [symbols, setSymbols] = React.useState("");
  const [groups, setGroups] = React.useState("");
  const [accountType, setAccountType] = React.useState<EarnRule["accountType"]>("live");
  const [ppl, setPpl] = React.useState("10");
  const [priority, setPriority] = React.useState("100");
  const [active, setActive] = React.useState(true);
  React.useEffect(() => {
    if (!open) return;
    setName(rule?.name ?? "");
    setMatch(rule ? (rule.symbols?.length ? "symbols" : rule.assetClass ? "class" : "any") : "class");
    setAssetClass(rule?.assetClass ?? "forex");
    setSymbols((rule?.symbols ?? []).join(", "));
    setGroups((rule?.accountGroups ?? []).join(", "));
    setAccountType(rule?.accountType ?? "live");
    setPpl(numStr(rule?.pointsPerLot ?? 10));
    setPriority(numStr(rule?.priority ?? 100));
    setActive(rule?.active ?? true);
  }, [open, rule]);
  return (
    <FormDialog
      open={open}
      onOpenChange={onOpenChange}
      width={600}
      title={rule ? `Edit rule · ${rule.name}` : "New earning rule"}
      submitLabel={rule ? "Save rule" : "Create rule"}
      submitTestId="rule-form-submit"
      submit={() => {
        if (!name.trim()) return "Give the rule a name.";
        if (!(Number(ppl) >= 0)) return "Points per lot can't be negative.";
        if (match === "symbols" && splitList(symbols).length === 0) return "List at least one symbol.";
        const body = {
          name: name.trim(),
          assetClass: match === "class" ? assetClass : null,
          symbols: match === "symbols" ? splitList(symbols, true) : [],
          accountGroups: splitList(groups),
          accountType,
          pointsPerLot: Number(ppl),
          priority: Math.round(Number(priority) || 0),
          active,
        };
        return rule ? mkSend(`rules/${rule.id}`, body, "PATCH") : mkSend("rules", body);
      }}
      success={rule ? "Rule saved" : "Rule created"}
      onDone={onSaved}
    >
      <TextF label="Name" value={name} onChange={setName} placeholder="e.g. Forex majors" />
      <div className="grid grid-cols-2 gap-3">
        <SelectF label="Matches" value={match} onChange={setMatch} options={[{ value: "class", label: "Asset class" }, { value: "symbols", label: "Symbol list" }, { value: "any", label: "Any symbol" }]} />
        {match === "class" && <SelectF label="Asset class" value={assetClass} onChange={setAssetClass} options={ASSET_CLASSES.map((c) => ({ value: c, label: c[0]!.toUpperCase() + c.slice(1) }))} />}
        {match === "symbols" && <TextF label="Symbols" value={symbols} onChange={setSymbols} mono placeholder="EURUSD, XAUUSD" />}
        {match === "any" && <div />}
        <NumF label="Points per lot" value={ppl} onChange={setPpl} suffix="pts" />
        <NumF label="Priority" value={priority} onChange={setPriority} step={1} hint="lower first" />
        <SelectF label="Account type" value={accountType} onChange={setAccountType} options={[{ value: "live", label: "Live only" }, { value: "demo", label: "Demo only" }, { value: "any", label: "Live and demo" }]} />
        <TextF label="Account groups" value={groups} onChange={setGroups} placeholder="All groups" hint="comma separated" />
      </div>
      <div className="k-row px-4">
        <ToggleRow label="Active" checked={active} onChange={setActive} />
      </div>
    </FormDialog>
  );
}

/* ------------------------------------------------------------------ */
/* Catalogue                                                            */
/* ------------------------------------------------------------------ */

function valueText(it: Pick<CatalogueItem, "kind" | "value" | "params">) {
  if (it.kind === "fee_discount") return `${num(it.value)}% off ${it.params?.appliesTo ?? "any"} fees`;
  if (it.kind === "bonus_credit") return `${usd(it.value, 0)} bonus`;
  return `${usd(it.value)} to wallet`;
}

function CatalogueCard({ canWrite, tiers, ask }: { canWrite: boolean; tiers: Tier[]; ask: Ask }) {
  const { data, error, reload } = useApi<{ items: CatalogueItem[] }>(M("catalogue"));
  const [editing, setEditing] = React.useState<CatalogueItem | null>(null);
  const [open, setOpen] = React.useState(false);
  const items = data?.items ?? [];
  const toggle = (it: CatalogueItem) =>
    ask({
      title: it.active ? `Hide "${it.name}"` : `Show "${it.name}"`,
      description: it.active ? "Clients can no longer redeem it. Past redemptions are unaffected." : "Clients can redeem it again.",
      confirmLabel: it.active ? "Hide reward" : "Show reward",
      note: "none",
      run: () => mkSend(`catalogue/${it.id}`, { active: !it.active }, "PATCH"),
      success: it.active ? "Reward hidden" : "Reward visible",
      onDone: reload,
    });
  return (
    <Card>
      <CardHeader
        title="Redemption catalogue"
        subtitle="What clients can spend points on"
        action={
          canWrite && (
            <Button size="sm" variant="ember" onClick={() => (setEditing(null), setOpen(true))} data-testid="new-reward">
              <Plus /> Add reward
            </Button>
          )
        }
      />
      <div className="mt-4 px-4 pb-6 sm:px-6">
        {error && !data ? (
          <MkError error={error} onRetry={reload} />
        ) : !data ? (
          <TableSkeleton rows={3} />
        ) : items.length === 0 ? (
          <EmptyNote title="No rewards in the catalogue" />
        ) : (
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-4">
            {items.map((it) => (
              <div key={it.id} className={cn("k-row flex flex-col px-4 py-3.5", !it.active && "opacity-60")}>
                <div className="flex items-start justify-between gap-2">
                  <div className="min-w-0">
                    <div className="truncate text-[14px] font-medium">{it.name}</div>
                    <div className="text-[11.5px] text-fg-3">{KIND_LABEL[it.kind] ?? it.kind}</div>
                  </div>
                  <Chip size="sm" tone={it.active ? "up" : "neutral"} dot>
                    {it.active ? "Live" : "Hidden"}
                  </Chip>
                </div>
                <div className="mt-2 line-clamp-2 min-h-[2rem] text-[12px] text-fg-2">{it.description}</div>
                <div className="mt-2 flex items-baseline justify-between">
                  <span className="k-num text-[18px] font-semibold text-gold">{int(it.costPoints)} pts</span>
                  <span className="k-num text-[12.5px] text-fg-2">{valueText(it)}</span>
                </div>
                <div className="mt-2 flex flex-wrap gap-1 text-[11px] text-fg-3">
                  {it.minTier && <Chip size="sm">{tiers.find((t) => t.key === it.minTier)?.name ?? it.minTier}+</Chip>}
                  <Chip size="sm">{it.stock === null ? "Unlimited" : `${int(it.stock)} left`}</Chip>
                  {it.kind === "bonus_credit" && <Chip size="sm">{usd(it.params?.releasePerLot)}/lot · {int(it.params?.expiryDays)}d</Chip>}
                  {it.kind === "fee_discount" && <Chip size="sm">valid {int(it.params?.validDays)}d</Chip>}
                </div>
                {canWrite && (
                  <div className="mt-3 flex gap-1.5 border-t border-line pt-3">
                    <Button size="xs" variant="surface" onClick={() => (setEditing(it), setOpen(true))}>
                      <Pencil /> Edit
                    </Button>
                    <Button size="xs" variant="ghost" onClick={() => toggle(it)}>
                      {it.active ? "Hide" : "Show"}
                    </Button>
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>
      <CatalogueDialog open={open} onOpenChange={setOpen} item={editing} tiers={tiers} onSaved={reload} />
    </Card>
  );
}

function CatalogueDialog({ open, onOpenChange, item, tiers, onSaved }: { open: boolean; onOpenChange: (o: boolean) => void; item: CatalogueItem | null; tiers: Tier[]; onSaved: () => void }) {
  const [name, setName] = React.useState("");
  const [description, setDescription] = React.useState("");
  const [kind, setKind] = React.useState<CatalogueKind>("cashback");
  const [cost, setCost] = React.useState("1000");
  const [value, setValue] = React.useState("10");
  const [minTier, setMinTier] = React.useState("");
  const [stock, setStock] = React.useState("");
  const [active, setActive] = React.useState(true);
  const [releasePerLot, setReleasePerLot] = React.useState("2");
  const [expiryDays, setExpiryDays] = React.useState("30");
  const [appliesTo, setAppliesTo] = React.useState<AppliesTo>("any");
  const [validDays, setValidDays] = React.useState("30");
  React.useEffect(() => {
    if (!open) return;
    setName(item?.name ?? "");
    setDescription(item?.description ?? "");
    setKind(item?.kind ?? "cashback");
    setCost(numStr(item?.costPoints ?? 1000));
    setValue(numStr(item?.value ?? 10));
    setMinTier(item?.minTier ?? "");
    setStock(numStr(item?.stock));
    setActive(item?.active ?? true);
    setReleasePerLot(numStr(item?.params?.releasePerLot ?? 2));
    setExpiryDays(numStr(item?.params?.expiryDays ?? 30));
    setAppliesTo((item?.params?.appliesTo as AppliesTo) ?? "any");
    setValidDays(numStr(item?.params?.validDays ?? 30));
  }, [open, item]);
  return (
    <FormDialog
      open={open}
      onOpenChange={onOpenChange}
      width={620}
      title={item ? `Edit · ${item.name}` : "New reward"}
      submitLabel={item ? "Save reward" : "Add reward"}
      submitTestId="reward-form-submit"
      submit={() => {
        if (!name.trim()) return "Give the reward a name.";
        if (!(Number(cost) > 0)) return "Cost must be above 0 points.";
        if (!(Number(value) > 0)) return "Value must be above 0.";
        const params = kind === "bonus_credit" ? { releasePerLot: Number(releasePerLot), expiryDays: Math.round(Number(expiryDays)) } : kind === "fee_discount" ? { appliesTo, validDays: Math.round(Number(validDays)) } : {};
        if (kind === "bonus_credit" && !(params.releasePerLot! > 0 && params.expiryDays! > 0)) return "Set the release per lot and expiry days.";
        const body = { name: name.trim(), description: description.trim(), kind, costPoints: Math.round(Number(cost)), value: Number(value), minTier: minTier || null, stock: numOrNull(stock), active, params };
        return item ? mkSend(`catalogue/${item.id}`, body, "PATCH") : mkSend("catalogue", body);
      }}
      success={item ? "Reward saved" : "Reward added"}
      onDone={onSaved}
    >
      <TextF label="Name" value={name} onChange={setName} />
      <AreaF label="Description" value={description} onChange={setDescription} rows={2} />
      <div className="grid grid-cols-2 gap-3">
        <SelectF label="Kind" value={kind} onChange={setKind} options={(Object.keys(KIND_LABEL) as CatalogueKind[]).map((k) => ({ value: k, label: KIND_LABEL[k] }))} />
        <NumF label="Cost" value={cost} onChange={setCost} suffix="points" step={1} />
        <NumF label="Value" value={value} onChange={setValue} prefix={kind === "fee_discount" ? undefined : "$"} suffix={kind === "fee_discount" ? "%" : "USD"} />
        <SelectF label="Minimum tier" value={minTier} onChange={setMinTier} options={[{ value: "", label: "Any tier" }, ...tiers.map((t) => ({ value: t.key, label: t.name }))]} />
        <NumF label="Stock" value={stock} onChange={setStock} placeholder="Unlimited" step={1} />
        {kind === "bonus_credit" && (
          <>
            <NumF label="Release per lot" value={releasePerLot} onChange={setReleasePerLot} prefix="$" suffix="/ lot" />
            <NumF label="Expiry days" value={expiryDays} onChange={setExpiryDays} suffix="days" step={1} />
          </>
        )}
        {kind === "fee_discount" && (
          <>
            <SelectF label="Applies to" value={appliesTo} onChange={setAppliesTo} options={[{ value: "any", label: "Any fee" }, { value: "prop", label: "Prop challenges" }, { value: "commission", label: "Commission" }]} />
            <NumF label="Valid days" value={validDays} onChange={setValidDays} suffix="days" step={1} />
          </>
        )}
      </div>
      <div className="k-row px-4">
        <ToggleRow label="Active" hint="Visible in the client catalogue" checked={active} onChange={setActive} />
      </div>
    </FormDialog>
  );
}

/* ------------------------------------------------------------------ */
/* Redemptions                                                          */
/* ------------------------------------------------------------------ */

function RedemptionsCard({ canApprove, tick, ask }: { canApprove: boolean; tick: number; ask: Ask }) {
  const [status, setStatus] = React.useState("all");
  const [page, setPage] = React.useState(1);
  React.useEffect(() => setPage(1), [status]);
  const { data, error, loading, reload } = useApi<Paged<Redemption> & { fourEyesThreshold?: number }>(`${M("redemptions")}${qs({ status, page, limit: PER })}`);
  React.useEffect(() => {
    if (tick) reload();
  }, [tick, reload]);
  const threshold = data?.fourEyesThreshold;
  const retry = (r: Redemption) =>
    ask({
      title: `Retry redemption #${r.id}`,
      description: `${r.itemName} · ${r.name || `client #${r.userId}`} · ${usd(r.value)}`,
      body: r.error ? <div className="rounded-[12px] border border-down/30 bg-down-soft px-3.5 py-2.5 text-[12.5px] text-fg">Last error: {r.error}</div> : undefined,
      confirmLabel: "Retry now",
      note: "none",
      run: () => mkSend(`redemptions/${r.id}/retry`, {}),
      success: `Redemption #${r.id} retried`,
      onDone: reload,
    });
  const cols: Column<Redemption>[] = [
    { key: "at", header: "Time", cell: (r) => <span className="k-num whitespace-nowrap text-[12.5px] text-fg-2">{when(r.createdAt)}</span>, sort: (r) => r.createdAt, csv: (r) => r.createdAt },
    { key: "c", header: "Client", cell: (r) => <ClientCell id={r.userId} name={r.name} />, csv: (r) => r.userId },
    { key: "i", header: "Reward", cell: (r) => <span className="block max-w-52"><span className="block truncate font-medium">{r.itemName}</span><span className="block text-[11.5px] text-fg-3">{KIND_LABEL[r.kind] ?? r.kind}{r.voucherCode ? ` · ${r.voucherCode}` : ""}{r.login ? ` · acc ${r.login}` : ""}</span></span>, csv: (r) => r.itemName },
    { key: "p", header: "Points", align: "right", cell: (r) => <span className="k-num">{int(r.points)}</span>, sort: (r) => r.points, csv: (r) => r.points },
    { key: "v", header: "Value", align: "right", cell: (r) => <span className="k-num">{r.kind === "fee_discount" ? `${num(r.value)}%` : usd(r.value)}</span>, csv: (r) => r.value },
    {
      key: "s",
      header: "Status",
      cell: (r) => (
        <span className="flex flex-col items-start gap-0.5">
          <StatusPill map={PAY_STATUS} status={r.status} />
          {r.status === AWAITING && <FirstApproval fe={r.fourEyes} className="flex max-w-56 items-center gap-1 text-[10.5px] text-warn" />}
          {r.error && <span className={`max-w-40 truncate text-[10.5px] ${r.status === "rejected" ? "text-fg-3" : "text-down"}`} title={r.error}>{r.error}</span>}
        </span>
      ),
      csv: (r) => r.status,
    },
    ...(canApprove
      ? [
          {
            key: "x",
            header: "",
            align: "right" as const,
            cell: (r: Redemption) =>
              r.status === "approval" || r.status === AWAITING ? (
                <FourEyesDecide<{ redemption?: Redemption }>
                  act={{ ask }}
                  what="redemption"
                  status={r.status}
                  amount={r.value}
                  fe={r.fourEyes}
                  threshold={threshold}
                  approvePath={`redemptions/${r.id}/approve`}
                  rejectPath={`redemptions/${r.id}/reject`}
                  statusOf={(d) => d.redemption?.status}
                  paidText="Redemption approved · cash credited to the wallet"
                  describe={`${r.itemName}: ${usd(r.value)} cash to the client's wallet. Rejecting gives the ${int(r.points)} points back.`}
                  onDone={reload}
                  testId={`redemption-${r.id}`}
                />
              ) : r.status === "failed" ? (
                <Button size="xs" variant="surface" onClick={() => retry(r)}>
                  Retry
                </Button>
              ) : null,
          },
        ]
      : []),
  ];
  return (
    <Card>
      <CardHeader title="Redemptions" subtitle={`A failed wallet credit is retried, never refunded silently.${threshold != null ? ` Cash redemptions: ${fourEyesRule(threshold)}` : ""}`} />
      <div className="mt-4 px-4 pb-5 sm:px-6">
        {error && !data ? (
          <MkError error={error} onRetry={reload} />
        ) : !data ? (
          <TableSkeleton />
        ) : (
          <div className={loading ? "opacity-60 transition-opacity" : undefined}>
            <DataTable
              columns={cols}
              rows={data.items}
              pageSize={PER}
              dense
              rowKey={(r) => String(r.id)}
              exportName="loyalty-redemptions"
              toolbar={<FilterSelect label="Status" value={status} onChange={setStatus} options={[
                    { value: "all", label: "All" },
                    { value: "open", label: "Needs approval" },
                    { value: "awaiting_second_approval", label: "Awaiting 2nd approval" },
                    { value: "pending", label: "Pending" },
                    { value: "completed", label: "Completed" },
                    { value: "failed", label: "Failed" },
                    { value: "rejected", label: "Rejected" },
                  ]} />}
              empty={<EmptyNote className="mt-3" title="No redemptions" />}
            />
            <Pager page={page} perPage={PER} total={data.total} onPage={setPage} />
          </div>
        )}
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Members                                                              */
/* ------------------------------------------------------------------ */

const tierName = (t: Member["tier"]) => (t && typeof t === "object" ? t.name : t ?? "—");

function MembersCard({ canApprove, tick, onChanged }: { canApprove: boolean; tick: number; onChanged: () => void }) {
  const [q, setQ] = React.useState("");
  const dq = useDebounced(q.trim(), 350);
  const [page, setPage] = React.useState(1);
  React.useEffect(() => setPage(1), [dq]);
  const { data, error, loading, reload } = useApi<Paged<Member>>(`${M("members")}${qs({ q: dq, page, limit: PER })}`);
  React.useEffect(() => {
    if (tick) reload();
  }, [tick, reload]);
  const [adjust, setAdjust] = React.useState<Member | null>(null);
  const [adjustOpen, setAdjustOpen] = React.useState(false);
  const cols: Column<Member>[] = [
    { key: "c", header: "Client", cell: (m) => <ClientCell id={m.userId} name={m.name} sub={m.email} />, csv: (m) => m.userId },
    { key: "co", header: "Country", hideOn: "md", cell: (m) => <span className="text-[12.5px] text-fg-2">{m.country || "—"}</span>, csv: (m) => m.country },
    { key: "t", header: "Tier", cell: (m) => <Chip size="sm" tone="gold">{tierName(m.tier)}</Chip>, csv: (m) => tierName(m.tier) },
    { key: "b", header: "Balance", align: "right", cell: (m) => <span className="k-num font-medium">{int(m.balance)}</span>, sort: (m) => m.balance, csv: (m) => m.balance },
    { key: "12", header: "Earned 12m", align: "right", hideOn: "md", cell: (m) => <span className="k-num text-fg-2">{int(m.earned12m)}</span>, sort: (m) => m.earned12m, csv: (m) => m.earned12m },
    { key: "l", header: "Lifetime", align: "right", hideOn: "lg", cell: (m) => <span className="k-num text-fg-2">{int(m.lifetime)}</span>, sort: (m) => m.lifetime, csv: (m) => m.lifetime },
    { key: "le", header: "Last earn", align: "right", hideOn: "lg", cell: (m) => <span className="k-num text-[12.5px] text-fg-3">{day(m.lastEarnAt)}</span>, csv: (m) => m.lastEarnAt ?? "" },
    ...(canApprove
      ? [
          {
            key: "x",
            header: "",
            align: "right" as const,
            cell: (m: Member) => (
              <Button size="xs" variant="surface" onClick={() => (setAdjust(m), setAdjustOpen(true))}>
                Adjust
              </Button>
            ),
          },
        ]
      : []),
  ];
  return (
    <Card>
      <CardHeader
        title="Members"
        subtitle="Points balance per client"
        action={
          <>
            <div className="flex h-9 items-center gap-2 rounded-full border border-line bg-surface-2 px-3.5">
              <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Name, email or client ID" aria-label="Search members" className="w-52 bg-transparent text-[13px] outline-none placeholder:text-fg-3" />
            </div>
            {canApprove && (
              <Button size="sm" variant="surface" onClick={() => (setAdjust(null), setAdjustOpen(true))}>
                Adjust points
              </Button>
            )}
          </>
        }
      />
      <div className="mt-4 px-4 pb-5 sm:px-6">
        {error && !data ? (
          <MkError error={error} onRetry={reload} />
        ) : !data ? (
          <TableSkeleton />
        ) : (
          <div className={loading ? "opacity-60 transition-opacity" : undefined}>
            <DataTable columns={cols} rows={data.items} pageSize={PER} dense rowKey={(m) => String(m.userId)} exportName="loyalty-members" empty={<EmptyNote className="mt-3" title={dq ? "No members match" : "No members yet"} text={dq ? undefined : "Clients appear after their first points."} />} />
            <Pager page={page} perPage={PER} total={data.total} onPage={setPage} />
          </div>
        )}
      </div>
      <AdjustDialog open={adjustOpen} onOpenChange={setAdjustOpen} member={adjust} onSaved={() => (reload(), onChanged())} />
    </Card>
  );
}

function AdjustDialog({ open, onOpenChange, member, onSaved }: { open: boolean; onOpenChange: (o: boolean) => void; member: Member | null; onSaved: () => void }) {
  const [userId, setUserId] = React.useState("");
  const [points, setPoints] = React.useState("");
  const [note, setNote] = React.useState("");
  React.useEffect(() => {
    if (!open) return;
    setUserId(member ? String(member.userId) : "");
    setPoints("");
    setNote("");
  }, [open, member]);
  const p = Math.round(Number(points) || 0);
  return (
    <FormDialog
      open={open}
      onOpenChange={onOpenChange}
      width={520}
      title="Adjust points"
      description={member ? `${member.name || `Client #${member.userId}`} · balance ${int(member.balance)} pts` : "Credit or debit a client's points balance."}
      submitLabel={p < 0 ? `Debit ${int(-p)} points` : `Credit ${int(p)} points`}
      submitTestId="adjust-form-submit"
      submit={() => {
        if (!/^\d+$/.test(userId.trim())) return "Enter the client ID.";
        if (!p) return "Enter a non-zero number of points (negative to debit).";
        if (note.trim().length < 3) return "Add a note (at least 3 characters).";
        return mkSend<{ balance: number }>("points/adjust", { userId: Number(userId), points: p, note: note.trim() });
      }}
      success={(r: { balance: number }) => `Points adjusted · new balance ${int(r.balance)}`}
      onDone={onSaved}
    >
      <div className="grid grid-cols-2 gap-3">
        <TextF label="Client ID" value={userId} onChange={setUserId} mono disabled={!!member} />
        <NumF label="Points" value={points} onChange={setPoints} step={1} hint="negative to debit" />
      </div>
      <AreaF label="Note" value={note} onChange={setNote} rows={2} placeholder="Why? Kept in the audit log and shown in the client's history." />
    </FormDialog>
  );
}
