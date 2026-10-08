"use client";

import * as React from "react";
import { Copy, Layers, Pencil, Plus, RefreshCw, Users } from "lucide-react";
import { Button, Card, Chip, Dialog, DialogClose, EmptyState, Field, Input, KpiCard, PageHeader, Reveal, Segmented, Toggle, cn, formatNumber } from "@kalks/ui";
import { ErrorState, useApi } from "@/components/live/kit";
import { useCan } from "@/components/staff-session";
import { AuditNotice, BookChip, ErrorBanner, ReasonFields, reportResult, useReason } from "@/components/trading-desk/kit";
import { loadGroups, productOf, type LiveGroup, type Product } from "@/lib/trading-desk";
import { GRP_REASONS, tradingWrite } from "./kit";

const LEVERAGES = [10, 20, 30, 50, 100, 200, 300, 400, 500, 1000, 2000];
const SPREAD_GROUPS = ["standard", "pro", "ecn", "cent"];

/** The spread groups this broker prices from: those its groups use (a broker other than the platform broker has
 *  its own, `<slug>-standard` …; the engine refuses another broker's), else the platform's four. */
function spreadGroupsOf(groups: LiveGroup[]): string[] {
  const used = Array.from(new Set(groups.map((g) => g.spreadGroup).filter(Boolean)));
  return used.length ? used.sort() : SPREAD_GROUPS;
}

function blank(): LiveGroup {
  return {
    code: "",
    name: "",
    mode: "hedging",
    cent: false,
    accountTypes: "both",
    leverages: [50, 100, 200, 500],
    defaultLeverage: 100,
    marginCallPct: 100,
    stopOutPct: 50,
    hedgedMarginPct: 50,
    minDeposit: 100,
    swapFree: false,
    commissionPerLot: 0,
    route: "B",
    spreadGroup: "standard",
    maxAccountsPerUser: 5,
    demoInitialBalance: 10000,
    demoRefillsPerDay: 3,
    demoExpiryDays: 30,
    enabled: true,
    product: "cfd",
  };
}

/** CFD / Options chip of a group or account (CFD / Options account split). */
export function ProductChip({ product, size = "sm" }: { product?: string | null; size?: "sm" | "md" }) {
  return productOf({ product }) === "options" ? (
    <Chip size={size} tone="ember">
      OPTIONS
    </Chip>
  ) : (
    <Chip size={size} tone="neutral">
      CFD
    </Chip>
  );
}

export function LiveGroupsPage() {
  const { data, error, reload } = useApi<{ groups: LiveGroup[] }>("/api/trading/admin/groups", { refreshMs: 30_000 });
  const canEdit = useCan("groups.write");
  const [edit, setEdit] = React.useState<{ g: LiveGroup; isNew: boolean } | null>(null);
  const [product, setProduct] = React.useState<"all" | Product>("all");
  const all = data?.groups ?? [];
  const groups = product === "all" ? all : all.filter((g) => productOf(g) === product);
  const accounts = groups.reduce((s, g) => s + (g.accounts ?? 0), 0);

  return (
    <div className="pb-10">
      <PageHeader
        title="Account groups"
        subtitle="Trading conditions per group on the trading engine — product (CFD or Options), execution mode, leverage, margin call / stop-out, commission, swaps, routing and spread group."
        actions={
          <>
            <Button variant="surface" size="lg" onClick={reload}>
              <RefreshCw /> Refresh
            </Button>
            {canEdit && (
              <Button variant="ember" size="lg" onClick={() => setEdit({ g: { ...blank(), product: product === "options" ? "options" : "cfd", spreadGroup: spreadGroupsOf(all).find((s) => s === "standard" || s.endsWith("-standard")) ?? spreadGroupsOf(all)[0]! }, isNew: true })}>
                <Plus /> New group
              </Button>
            )}
          </>
        }
      />
      <div className="grid grid-cols-2 gap-4 xl:grid-cols-4">
        <KpiCard label="Groups" icon={<Layers />} value={<span className="k-num">{groups.length}</span>} chip={`${groups.filter((g) => g.enabled).length} enabled · ${all.filter((g) => productOf(g) === "options").length} Options`} chipTone="up" />
        <KpiCard label="Accounts" icon={<Users />} value={<span className="k-num">{accounts}</span>} chip="across all groups" delay={0.04} />
        <KpiCard label="Netting groups" icon={<Layers />} value={<span className="k-num">{groups.filter((g) => g.mode === "netting").length}</span>} chip={`${groups.filter((g) => g.cent).length} cent`} delay={0.08} />
        <KpiCard label="A-book by default" icon={<Layers />} value={<span className="k-num">{groups.filter((g) => g.route === "A").length}</span>} chip="new trades, before routing rules" chipTone="info" delay={0.12} />
      </div>
      <div className="mt-4 flex flex-wrap items-center gap-3">
        <Segmented
          size="sm"
          value={product}
          onChange={(v) => setProduct(v)}
          options={[
            { value: "all", label: `All · ${all.length}` },
            { value: "cfd", label: `CFD · ${all.filter((g) => productOf(g) === "cfd").length}` },
            { value: "options", label: `Options · ${all.filter((g) => productOf(g) === "options").length}` },
          ]}
        />
        <span className="text-[12px] text-fg-3">An account trades its group's product only: CFD groups CFDs, Options groups Kalks FX Options.</span>
      </div>
      {error ? (
        <Card className="mt-4">
          <ErrorState error={error} onRetry={reload} />
        </Card>
      ) : !data ? (
        <div className="mt-4 grid grid-cols-1 gap-4 md:grid-cols-2 2xl:grid-cols-3">
          {Array.from({ length: 6 }).map((_, i) => (
            <div key={i} className="h-[260px] animate-pulse rounded-[22px] bg-surface" />
          ))}
        </div>
      ) : groups.length === 0 ? (
        <Card className="mt-4">
          <EmptyState title={product === "all" ? "No groups yet" : `No ${product === "options" ? "Options" : "CFD"} groups`} text="Create the first account group." illustration="bank" />
        </Card>
      ) : (
        <div className="mt-4 grid grid-cols-1 gap-4 md:grid-cols-2 2xl:grid-cols-3">
          {groups.map((g, i) => (
            <Reveal key={g.code} delay={Math.min(0.2, i * 0.03)}>
              <GroupCard g={g} canEdit={canEdit} onEdit={() => setEdit({ g, isNew: false })} onDuplicate={() => setEdit({ g: { ...g, code: `${g.code}-2`, name: `${g.name} copy`, accounts: 0 }, isNew: true })} />
            </Reveal>
          ))}
        </div>
      )}
      <GroupEditor
        edit={edit}
        spreadGroups={spreadGroupsOf(all)}
        onClose={() => setEdit(null)}
        onSaved={() => {
          reload();
          void loadGroups();
        }}
      />
    </div>
  );
}

function GroupCard({ g, canEdit, onEdit, onDuplicate }: { g: LiveGroup; canEdit: boolean; onEdit: () => void; onDuplicate: () => void }) {
  const options = productOf(g) === "options";
  const rows: [string, React.ReactNode][] = options
    ? [
        ["Margin call / stop-out", `${g.marginCallPct}% / ${g.stopOutPct}%`],
        ["Min deposit", `$${formatNumber(g.minDeposit, 0)}`],
        ["Fees", "Per contract · Options › Pricing"],
      ]
    : [
        ["Leverage", `1:${Math.min(...g.leverages)} – 1:${Math.max(...g.leverages)} · default 1:${g.defaultLeverage}`],
        ["Margin call / stop-out", `${g.marginCallPct}% / ${g.stopOutPct}%`],
        ["Hedged margin", `${g.hedgedMarginPct}%`],
        ["Min deposit", `$${formatNumber(g.minDeposit, 0)}`],
        ["Commission", g.commissionPerLot ? `$${formatNumber(g.commissionPerLot, 2)} per lot RT` : "None"],
        ["Swaps", g.swapFree ? "Swap-free" : "Charged at rollover"],
      ];
  rows.push(
    ["Spread group", <span key="s" className="font-mono">{g.spreadGroup}</span>],
    ["Accounts per client", `${g.maxAccountsPerUser} ${options ? "Options" : "CFD"} (per live / demo)`],
    ["Demo", `$${formatNumber(g.demoInitialBalance, 0)} · ${g.demoRefillsPerDay} refills/day · ${g.demoExpiryDays} d`],
  );
  return (
    <Card className="flex h-full flex-col">
      <div className="flex items-start gap-3 px-5 pt-5">
        <span className="grid size-11 shrink-0 place-items-center rounded-[14px] border border-line bg-surface-2 font-mono text-[13px] font-semibold text-fg-2">{g.name.slice(0, 2).toUpperCase()}</span>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-1.5">
            <h3 className="text-[16px] font-medium tracking-tight">{g.name}</h3>
            <span className="font-mono text-[11px] text-fg-3">{g.code}</span>
            {!g.enabled && <Chip size="sm" tone="neutral">Disabled</Chip>}
          </div>
          <div className="mt-1.5 flex flex-wrap items-center gap-1.5">
            <ProductChip product={g.product} />
            <Chip size="sm" tone={g.mode === "netting" ? "gold" : "neutral"}>{g.mode}</Chip>
            {g.cent && <Chip size="sm" tone="info">Cent · USC</Chip>}
            <Chip size="sm" tone="neutral">{g.accountTypes === "both" ? "Live + demo" : g.accountTypes === "live" ? "Live only" : "Demo only"}</Chip>
            <span className="inline-flex items-center gap-1 text-[11.5px] text-fg-3">
              route <BookChip book={g.route} />
            </span>
          </div>
        </div>
        <div className="text-right">
          <div className="k-num text-[18px] font-medium">{g.accounts ?? 0}</div>
          <div className="text-[10.5px] text-fg-3">accounts</div>
        </div>
      </div>
      <div className="mt-4 flex-1 space-y-1.5 px-5">
        {rows.map(([k, v]) => (
          <div key={k} className="flex items-center justify-between gap-3 text-[12.5px]">
            <span className="text-fg-3">{k}</span>
            <span className="k-num truncate text-right text-fg-2">{v}</span>
          </div>
        ))}
      </div>
      <div className="mt-4 flex gap-2 border-t border-line px-5 py-3">
        {canEdit ? (
          <>
            <Button size="sm" variant="surface" onClick={onEdit}>
              <Pencil /> Edit
            </Button>
            <Button size="sm" variant="ghost" onClick={onDuplicate}>
              <Copy /> Duplicate
            </Button>
          </>
        ) : (
          <span className="text-[12px] text-fg-3">Read-only for your role</span>
        )}
      </div>
    </Card>
  );
}

function Num({ label, value, onChange, suffix, hint }: { label: string; value: number; onChange: (v: number) => void; suffix?: string; hint?: string }) {
  const [txt, setTxt] = React.useState(String(value));
  React.useEffect(() => setTxt(String(value)), [value]);
  return (
    <Field label={label} hint={hint}>
      <Input
        value={txt}
        inputMode="decimal"
        onChange={(e) => {
          const t = e.target.value.replace(/[^0-9.]/g, "");
          setTxt(t);
          const n = Number(t);
          if (t !== "" && Number.isFinite(n)) onChange(n);
        }}
        aria-label={label}
        className="font-mono"
        trailing={suffix}
      />
    </Field>
  );
}

function GroupEditor({ edit, spreadGroups, onClose, onSaved }: { edit: { g: LiveGroup; isNew: boolean } | null; spreadGroups: string[]; onClose: () => void; onSaved: () => void }) {
  const [g, setG] = React.useState<LiveGroup>(blank());
  const [error, setError] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);
  const r = useReason();
  const { reset } = r;
  React.useEffect(() => {
    if (edit) {
      setG(edit.g);
      setError(null);
      reset();
    }
  }, [edit, reset]);
  if (!edit) return null;
  const set = (p: Partial<LiveGroup>) => setG((x) => ({ ...x, ...p }));
  const locked = !edit.isNew && (edit.g.accounts ?? 0) > 0;
  const toggleLev = (l: number) => {
    const has = g.leverages.includes(l);
    const next = has ? g.leverages.filter((x) => x !== l) : [...g.leverages, l].sort((a, b) => a - b);
    set({ leverages: next, defaultLeverage: next.includes(g.defaultLeverage) ? g.defaultLeverage : (next[0] ?? g.defaultLeverage) });
  };
  const invalid = !/^[a-z0-9-]{1,40}$/.test(g.code)
    ? "Code: lowercase letters, digits or dashes"
    : !g.name.trim()
      ? "Enter a name"
      : !g.leverages.length
        ? "Choose at least one leverage"
        : g.stopOutPct >= g.marginCallPct
          ? "Stop-out must be below the margin call level"
          : g.hedgedMarginPct > 100
            ? "Hedged margin is 0–100 %"
            : null;

  const save = async () => {
    setBusy(true);
    setError(null);
    const { accounts: _a, ...body } = g;
    void _a;
    const res = await tradingWrite<LiveGroup>(edit.isNew ? "admin/groups" : `admin/groups/${edit.g.code}`, { ...body, name: g.name.trim() }, r.reason, null, edit.isNew ? "POST" : "PUT");
    setBusy(false);
    if (!res.ok) {
      setError(res.error);
      reportResult(res, "");
      return;
    }
    reportResult(res, `${g.name} ${edit.isNew ? "created" : "saved"}`, "applies to the group's accounts at once");
    onSaved();
    onClose();
  };

  return (
    <Dialog
      open
      onOpenChange={(o) => !o && onClose()}
      side="right"
      width={640}
      title={edit.isNew ? "New account group" : `Edit ${edit.g.name}`}
      description={edit.isNew ? "Groups set the trading conditions of every account in them." : `${edit.g.accounts ?? 0} accounts. Changes apply to them at once.`}
      footer={
        <>
          <span className="mr-auto max-w-[260px] truncate text-[11.5px] text-fg-3">{invalid ?? r.error}</span>
          <DialogClose asChild>
            <Button size="sm" variant="ghost">
              Cancel
            </Button>
          </DialogClose>
          <Button size="sm" variant="ember" disabled={!!invalid || !!r.error || busy} onClick={save}>
            {busy ? "Saving…" : edit.isNew ? "Create group" : "Save group"}
          </Button>
        </>
      }
    >
      <div className="space-y-5">
        <div className="grid grid-cols-2 gap-3">
          <Field label="Name">
            <Input value={g.name} onChange={(e) => set({ name: e.target.value })} aria-label="Group name" maxLength={40} />
          </Field>
          <Field label="Code" hint={edit.isNew ? "lowercase, fixed after creation" : "fixed"}>
            <Input value={g.code} disabled={!edit.isNew} onChange={(e) => set({ code: e.target.value.toLowerCase().replace(/[^a-z0-9-]/g, "") })} aria-label="Group code" className="font-mono" />
          </Field>
        </div>
        <Field label="Product" hint={locked ? "fixed: has accounts" : "what the group's accounts trade; fixed once it has accounts"}>
          <Segmented
            size="sm"
            value={productOf(g)}
            onChange={(v) => !locked && set({ product: v })}
            options={[
              { value: "cfd", label: "CFD" },
              { value: "options", label: "Options" },
            ]}
          />
        </Field>
        {productOf(g) === "options" && (
          <p className="-mt-2 text-[12px] text-fg-3">Options accounts trade Kalks FX Options only (house prices and the order book), never CFDs. Leverage, hedged margin and commission per lot don't apply to options: per-contract fees, spreads and limits per group are set in Options › Pricing.</p>
        )}
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          <Field label="Execution mode" hint={locked ? "fixed: has accounts" : undefined}>
            <Segmented size="sm" value={g.mode} onChange={(v) => !locked && set({ mode: v })} options={[{ value: "hedging", label: "Hedging" }, { value: "netting", label: "Netting" }]} />
          </Field>
          <Field label="Account currency" hint={locked ? "fixed: has accounts" : undefined}>
            <Segmented size="sm" value={g.cent ? "cent" : "usd"} onChange={(v) => !locked && set({ cent: v === "cent" })} options={[{ value: "usd", label: "USD" }, { value: "cent", label: "Cent (USC)" }]} />
          </Field>
          <Field label="Account types">
            <Segmented size="sm" value={g.accountTypes} onChange={(v) => set({ accountTypes: v })} options={[{ value: "both", label: "Both" }, { value: "live", label: "Live" }, { value: "demo", label: "Demo" }]} />
          </Field>
        </div>
        <div>
          <div className="mb-1.5 flex items-center justify-between text-[12.5px] font-medium text-fg-2">
            Leverage list <span className="font-normal text-fg-3">clients choose from these</span>
          </div>
          <div className="flex flex-wrap gap-1.5">
            {LEVERAGES.map((l) => {
              const on = g.leverages.includes(l);
              const def = g.defaultLeverage === l;
              return (
                <button
                  key={l}
                  type="button"
                  onClick={() => toggleLev(l)}
                  aria-pressed={on}
                  className={cn("rounded-full border px-2.5 py-1 font-mono text-[12px]", on ? (def ? "border-ember bg-ember text-white" : "border-ember/50 bg-ember-soft text-fg") : "border-line bg-surface-2 text-fg-3 hover:text-fg")}
                >
                  1:{l}
                </button>
              );
            })}
          </div>
          <div className="mt-2 flex flex-wrap items-center gap-2 text-[12px] text-fg-3">
            Default
            <Segmented size="xs" value={String(g.defaultLeverage)} onChange={(v) => set({ defaultLeverage: Number(v) })} options={g.leverages.map((l) => ({ value: String(l), label: `1:${l}` }))} />
          </div>
        </div>
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
          <Num label="Margin call" value={g.marginCallPct} onChange={(v) => set({ marginCallPct: v })} suffix="%" />
          <Num label="Stop-out" value={g.stopOutPct} onChange={(v) => set({ stopOutPct: v })} suffix="%" />
          <Num label="Hedged margin" value={g.hedgedMarginPct} onChange={(v) => set({ hedgedMarginPct: v })} suffix="%" hint="per leg" />
          <Num label="Min deposit" value={g.minDeposit} onChange={(v) => set({ minDeposit: v })} suffix="USD" />
          <Num label="Commission" value={g.commissionPerLot} onChange={(v) => set({ commissionPerLot: v })} suffix="$/lot" hint="round turn" />
          <Num label="Accounts per client" value={g.maxAccountsPerUser} onChange={(v) => set({ maxAccountsPerUser: Math.round(v) })} />
        </div>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <Field label="Default route" hint="before routing rules">
            <Segmented size="sm" value={g.route} onChange={(v) => set({ route: v })} options={[{ value: "B", label: "B-book" }, { value: "A", label: "A-book" }]} />
          </Field>
          <Field label="Spread group" hint="market-data markups">
            <Segmented size="sm" value={g.spreadGroup} onChange={(v) => set({ spreadGroup: v })} options={Array.from(new Set([...spreadGroups, g.spreadGroup])).map((s) => ({ value: s, label: s }))} />
          </Field>
        </div>
        <div className="grid grid-cols-3 gap-3">
          <Num label="Demo balance" value={g.demoInitialBalance} onChange={(v) => set({ demoInitialBalance: v })} suffix="USD" />
          <Num label="Demo refills / day" value={g.demoRefillsPerDay} onChange={(v) => set({ demoRefillsPerDay: Math.round(v) })} />
          <Num label="Demo expiry" value={g.demoExpiryDays} onChange={(v) => set({ demoExpiryDays: Math.round(v) })} suffix="days" />
        </div>
        <div className="flex flex-wrap gap-5 text-[12.5px]">
          <label className="flex items-center gap-2">
            <Toggle checked={g.swapFree} onChange={(v) => set({ swapFree: v })} label="Swap-free" /> Swap-free (no rollover charges)
          </label>
          <label className="flex items-center gap-2">
            <Toggle checked={g.enabled} onChange={(v) => set({ enabled: v })} label="Enabled" /> Enabled for new accounts
          </label>
        </div>
        <ReasonFields r={r} codes={GRP_REASONS} />
        <ErrorBanner error={error} />
        <AuditNotice />
      </div>
    </Dialog>
  );
}
