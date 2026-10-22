"use client";

import * as React from "react";
import Link from "next/link";
import { AlertTriangle, Eye, Gauge as GaugeIcon, History, Layers, Lock, ShieldAlert, ShieldCheck, Sliders, Users } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, CardHeader, Chip, DataTable, EmptyState, Input, KpiCard, PageHeader, Reveal, Segmented, Skeleton, buttonVariants, cn, formatNumber, type Column } from "@kalks/ui";
import { IS_DEMO } from "@kalks/mock/mode";
import { FilterSelect, ago, sendJson, useApi, useNow, type ApiErr } from "@/components/live/kit";
import { useCan } from "@/components/staff-session";
import { NeedsPerm } from "@/components/needs-perm";
import { DeskDialog } from "@/components/trading-desk/kit";
import { DeskStatusChip } from "@/components/trading-desk/status";
import { usdCompact } from "@/components/command/kit";
import { tradingWrite } from "@/components/trading-live/kit";
import { ConnectEngineState, RiskError } from "@/components/trading-live/risk-kit";
import { serverStamp, useRestDesk, type AuditEntry } from "@/lib/trading-desk";

/* ------------------------------------------------------------------ */
/* Engine shapes (services/trading/src/risk/leverage.rs)               */
/* ------------------------------------------------------------------ */

type Scope = "all" | "group" | "asset_class" | "account";
type AppliesTo = "new_positions" | "all_positions";
type Cap = { id: number; scope: Scope; target: string; maxLeverage: number; appliesTo: AppliesTo; enabled: boolean; reasonCode: string; note: string; createdBy: string; createdAt: string; updatedBy: string; updatedAt: string };
type CapsDoc = { caps: Cap[]; assetClasses: string[]; groups: { code: string; name: string; leverages: number[]; defaultLeverage: number }[]; maxCap: number };
type Row = {
  login: string;
  userId: number;
  type: "live" | "demo";
  group: string;
  currency: string;
  leverage: number;
  positions: number;
  equity: number;
  marginBefore: number;
  marginAfter: number;
  freeMarginAfter: number;
  marginLevelBefore: number | null;
  marginLevelAfter: number | null;
  marginCallLevel: number;
  stopOutLevel: number;
  status: "ok" | "margin_call" | "stop_out";
  changed: boolean;
};
type Totals = { accountsInScope: number; accountsAffected: number; positionsAffected: number; marginBeforeUsd: number; marginAfterUsd: number; marginIncreaseUsd: number; marginCall: number; stopOut: number };
type Preview = { scope: Scope; target: string; label: string; maxLeverage: number; appliesTo: AppliesTo; totals: Totals; live: Totals; marginCallAccounts: Row[]; stopOutAccounts: Row[]; accounts: Row[]; truncated: boolean; generatedAt: string; current: Cap | null };
type ApplyResult = { cap: Cap; recheck: { accounts: number; marginCalls: number; stopOuts: number } };

const RISK_CODE = "DLR-03 · Risk management";
const QUICK = [1, 2, 5, 10, 20, 50];
const CONFIRM_WORD = "APPLY";
const SCOPE_LABEL: Record<Scope, string> = { all: "All accounts", group: "Account group", asset_class: "Asset class", account: "One account" };
const CLASS_LABEL: Record<string, string> = { forex: "Forex", metals: "Metals", indices: "Indices", energies: "Energies", crypto: "Crypto", stocks: "Stocks" };
const lev = (n: number) => `1:${formatNumber(n, 0)}`;
const pct = (v: number | null) => (v === null ? "—" : `${formatNumber(v, 0)}%`);
const money = (v: number) => formatNumber(v, 2);

function scopeText(scope: Scope, target: string, doc?: CapsDoc | null) {
  if (scope === "all") return "All accounts";
  if (scope === "group") return `Group ${doc?.groups.find((g) => g.code === target)?.name ?? target}`;
  if (scope === "asset_class") return CLASS_LABEL[target] ?? target;
  return `Account ${target}`;
}

function AppliesChip({ a }: { a: AppliesTo }) {
  return a === "all_positions" ? (
    <Chip size="sm" tone="down">
      All open positions
    </Chip>
  ) : (
    <Chip size="sm" tone="info">
      New positions
    </Chip>
  );
}

function StatusChip({ s }: { s: Row["status"] }) {
  if (s === "stop_out") return <Chip size="sm" tone="down" dot>Stop-out</Chip>;
  if (s === "margin_call") return <Chip size="sm" tone="warn" dot>Margin call</Chip>;
  return <Chip size="sm" tone="up">OK</Chip>;
}

/* ------------------------------------------------------------------ */

export default function LeverageControlPage() {
  if (IS_DEMO)
    return (
      <div className="pb-10">
        <PageHeader title="Leverage control" subtitle="Cap the leverage clients get when the house is at risk: by account, group, asset class or for everyone." />
        <Card className="px-4 py-5 sm:px-6">
          <ConnectEngineState what="Leverage caps" />
        </Card>
      </div>
    );
  return <LiveLeverageControl />;
}

function LiveLeverageControl() {
  const canWrite = useCan("dealing.write");
  const rest = useRestDesk();
  const now = useNow();
  const doc = useApi<CapsDoc>("/api/trading/dealing/risk/leverage-caps", { refreshMs: 30_000 });
  const audit = useApi<AuditEntry[]>("/api/trading/dealing/audit?action=risk.leverage_cap&limit=30");

  // form
  const [scope, setScope] = React.useState<Scope>("all");
  const [group, setGroup] = React.useState("");
  const [klass, setKlass] = React.useState("crypto");
  const [login, setLogin] = React.useState("");
  const [max, setMax] = React.useState<number | null>(10);
  const [custom, setCustom] = React.useState("");
  const [applies, setApplies] = React.useState<AppliesTo>("new_positions");
  const target = scope === "group" ? group : scope === "asset_class" ? klass : scope === "account" ? login.trim() : "";
  const formKey = JSON.stringify({ scope, target, max, applies });
  const groups = React.useMemo(() => doc.data?.groups ?? [], [doc.data]);
  React.useEffect(() => {
    if (!group && groups[0]) setGroup(groups[0].code);
  }, [group, groups]);

  // preview
  const [preview, setPreview] = React.useState<{ key: string; data: Preview } | null>(null);
  const [previewing, setPreviewing] = React.useState(false);
  const [previewErr, setPreviewErr] = React.useState<ApiErr | null>(null);
  const fresh = !!preview && preview.key === formKey;
  const formError = !max || max < 1 || max > (doc.data?.maxCap ?? 1000) ? `Max leverage must be 1 to ${doc.data?.maxCap ?? 1000}` : scope === "account" && !/^\d{1,18}$/.test(target) ? "Enter the account login" : scope === "group" && !target ? "Pick a group" : null;

  // dialogs
  const [applying, setApplying] = React.useState(false);
  const [confirmText, setConfirmText] = React.useState("");
  const [disabling, setDisabling] = React.useState<Cap | null>(null);

  const caps = doc.data?.caps ?? [];
  const active = caps.filter((c) => c.enabled);
  const inactive = caps.filter((c) => !c.enabled);
  const tightest = active.reduce<Cap | null>((m, c) => (!m || c.maxLeverage < m.maxLeverage ? c : m), null);
  const lastChange = caps.reduce<string | null>((m, c) => (!m || Date.parse(c.updatedAt) > Date.parse(m) ? c.updatedAt : m), null);

  const runPreview = async () => {
    if (formError) return toast.error(formError);
    setPreviewing(true);
    setPreviewErr(null);
    const r = await sendJson<Preview>("/api/trading/dealing/risk/leverage-caps/preview", { scope, target, maxLeverage: max, appliesTo: applies });
    setPreviewing(false);
    if (!r.ok) {
      setPreviewErr(r.error);
      toast.error("Preview failed", { description: r.error.message });
      return;
    }
    setPreview({ key: formKey, data: r.data });
    toast.success("Impact previewed", { description: `${r.data.totals.accountsAffected} account(s) re-margined · ${r.data.totals.marginCall} margin call · ${r.data.totals.stopOut} stop-out` });
  };

  const reloadAll = () => {
    doc.reload();
    audit.reload();
  };

  if (doc.error && !doc.data)
    return (
      <div className="pb-10">
        <PageHeader title="Leverage control" subtitle="Cap the leverage clients get when the house is at risk." />
        <Card>
          <RiskError error={doc.error} onRetry={doc.reload} />
        </Card>
      </div>
    );

  const p = fresh ? preview!.data : null;
  const allPositions = applies === "all_positions";

  return (
    <div className="pb-16">
      <PageHeader
        title="Leverage control"
        subtitle={
          <span className="inline-flex flex-wrap items-center gap-2">
            Cap the leverage clients get when the house is at risk, so they trade on their own capital as margin. Effective leverage = min(account, symbol max, the tightest cap). <DeskStatusChip />
          </span>
        }
        actions={
          <Link href="/trading/risk-limits" className={buttonVariants({ variant: "surface", size: "md" })}>
            <ShieldAlert /> Risk limits
          </Link>
        }
      />

      <div className="grid grid-cols-2 gap-4 xl:grid-cols-4">
        <KpiCard label="Active caps" icon={<Lock />} value={<span className="k-num">{doc.data ? active.length : "—"}</span>} chip={inactive.length ? `${inactive.length} disabled` : "None disabled"} />
        <KpiCard label="Tightest cap" icon={<GaugeIcon />} value={<span className="k-num">{tightest ? lev(tightest.maxLeverage) : "None"}</span>} chip={tightest ? scopeText(tightest.scope, tightest.target, doc.data) : "Normal leverage"} chipTone={tightest ? "warn" : "up"} delay={0.04} />
        <KpiCard label="On open positions" icon={<Layers />} value={<span className="k-num">{active.filter((c) => c.appliesTo === "all_positions").length}</span>} chip="all_positions caps" chipTone="down" delay={0.08} />
        <KpiCard label="Last change" icon={<History />} value={<span className="k-num text-[20px]">{lastChange ? ago(lastChange, now) : "—"}</span>} chip={lastChange ? serverStamp(lastChange) : "No caps yet"} delay={0.12} />
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
        {/* form */}
        <Reveal delay={0.04} className="xl:col-span-5">
          <Card className="h-full">
            <CardHeader title="New cap" subtitle="Preview the impact, then apply it with a reason code" icon={<Sliders />} />
            <div className="space-y-5 px-4 pb-6 pt-4 sm:px-6">
              <div className="space-y-2">
                <div className="text-[11px] uppercase tracking-[0.05em] text-fg-3">Scope</div>
                <div className="overflow-x-auto">
                  <Segmented size="sm" value={scope} onChange={setScope} options={(Object.keys(SCOPE_LABEL) as Scope[]).map((s) => ({ value: s, label: SCOPE_LABEL[s] }))} aria-label="Scope" />
                </div>
                {scope === "group" && <FilterSelect label="Group" value={group} onChange={setGroup} options={groups.map((g) => ({ value: g.code, label: `${g.name} (${g.code})` }))} />}
                {scope === "asset_class" && <FilterSelect label="Asset class" value={klass} onChange={setKlass} options={(doc.data?.assetClasses ?? Object.keys(CLASS_LABEL)).map((c) => ({ value: c, label: CLASS_LABEL[c] ?? c }))} />}
                {scope === "account" && <Input value={login} onChange={(e) => setLogin(e.target.value.replace(/\D/g, "").slice(0, 18))} placeholder="Login, e.g. 10000001" inputMode="numeric" inputClassName="font-mono" aria-label="Account login" />}
                <p className="text-[11.5px] text-fg-3">
                  {scope === "asset_class" ? "Only symbols of this class are capped; the account leverage clients choose is not limited." : scope === "account" ? "A dealer override for one account." : scope === "group" ? "Every account in the group." : "Every account of the broker, live and demo."} The most restrictive matching cap wins.
                </p>
              </div>

              <div className="space-y-2">
                <div className="text-[11px] uppercase tracking-[0.05em] text-fg-3">Max leverage</div>
                <div className="flex flex-wrap gap-2">
                  {QUICK.map((q) => (
                    <button
                      key={q}
                      type="button"
                      aria-pressed={max === q && !custom}
                      onClick={() => {
                        setMax(q);
                        setCustom("");
                      }}
                      className={cn("k-num h-9 min-w-16 rounded-full border px-3.5 text-[13px] font-semibold transition-colors", max === q && !custom ? "border-ember/60 bg-ember-soft text-ember" : "border-line bg-surface-2 text-fg-2 hover:text-fg")}
                    >
                      {q}x
                    </button>
                  ))}
                  <Input
                    value={custom}
                    onChange={(e) => {
                      const v = e.target.value.replace(/\D/g, "").slice(0, 4);
                      setCustom(v);
                      setMax(v ? Number(v) : null);
                    }}
                    placeholder="Custom"
                    inputMode="numeric"
                    fieldSize="sm"
                    leading={<span className="text-[12px] text-fg-3">1:</span>}
                    className="w-28"
                    aria-label="Custom max leverage"
                  />
                </div>
              </div>

              <div className="space-y-2">
                <div className="text-[11px] uppercase tracking-[0.05em] text-fg-3">Applies to</div>
                <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
                  {(
                    [
                      ["new_positions", "New positions only", "New trades are margined at the cap. Open positions keep their margin.", ShieldCheck],
                      ["all_positions", "All open positions", "Open positions are re-margined at once. Accounts can fall into margin call or stop-out.", AlertTriangle],
                    ] as const
                  ).map(([v, title, text, Icon]) => (
                    <button
                      key={v}
                      type="button"
                      aria-pressed={applies === v}
                      onClick={() => setApplies(v)}
                      className={cn("k-row flex items-start gap-2.5 px-3.5 py-3 text-left transition-colors", applies === v && (v === "all_positions" ? "border-down/50 bg-down-soft" : "border-ember/50 bg-ember-soft"))}
                    >
                      <Icon className={cn("mt-0.5 size-4 shrink-0", v === "all_positions" ? "text-down" : "text-ember")} />
                      <span>
                        <span className="block text-[13px] font-medium">{title}</span>
                        <span className="block text-[11.5px] leading-snug text-fg-3">{text}</span>
                      </span>
                    </button>
                  ))}
                </div>
              </div>

              <div className="flex flex-wrap items-center gap-2 border-t border-line pt-4">
                <NeedsPerm ok={canWrite} perm="dealing.write" text="Previewing and applying caps need the dealing.write permission">
                  <Button variant="surface" size="md" disabled={!!formError || previewing || !canWrite} onClick={runPreview}>
                    <Eye /> {previewing ? "Previewing…" : "Preview impact"}
                  </Button>
                </NeedsPerm>
                {canWrite && (
                  <Button
                    variant={allPositions ? "sell" : "ember"}
                    size="md"
                    disabled={!fresh || !!formError}
                    onClick={() => {
                      setConfirmText("");
                      setApplying(true);
                    }}
                  >
                    <Lock /> Apply {max ? lev(max) : ""}
                  </Button>
                )}
                <span className="text-[11.5px] text-fg-3">{formError ?? (fresh ? "Previewed: ready to apply" : "Apply unlocks after a preview of these exact settings")}</span>
              </div>
              {!canWrite && <p className="text-[11.5px] text-fg-3">Previewing and applying caps need the dealing write permission (dealer, risk manager, admins).</p>}
            </div>
          </Card>
        </Reveal>

        {/* active caps */}
        <Reveal delay={0.06} className="xl:col-span-7">
          <Card className="h-full">
            <CardHeader title="Caps" subtitle="Enabled caps apply to the next margin computation; no restart" icon={<Lock />} action={active.length ? <Chip tone="warn" dot>{active.length} active</Chip> : <Chip tone="up">Normal leverage</Chip>} />
            <div className="space-y-2 px-4 pb-6 pt-4 sm:px-6">
              {!doc.data && <Skeleton className="h-24 w-full" />}
              {doc.data && !caps.length && <EmptyState title="No leverage caps" text="Clients trade at their account leverage (and each symbol's max). Add a cap when the house is at risk." illustration="shield" />}
              {[...active, ...inactive].map((c) => (
                <div key={c.id} className={cn("k-row flex flex-col gap-2 px-3.5 py-3 sm:flex-row sm:items-center sm:gap-4", !c.enabled && "opacity-55")}>
                  <span className="k-num w-20 shrink-0 font-mono text-[18px] font-semibold">{lev(c.maxLeverage)}</span>
                  <span className="min-w-0 flex-1">
                    <span className="flex flex-wrap items-center gap-1.5">
                      <span className="text-[13px] font-medium">{scopeText(c.scope, c.target, doc.data)}</span>
                      <AppliesChip a={c.appliesTo} />
                      {!c.enabled && <Chip size="sm" tone="neutral">Disabled</Chip>}
                    </span>
                    <span className="mt-0.5 block truncate text-[11.5px] text-fg-3" title={c.note}>
                      <span className="font-mono text-ember">{c.reasonCode.split(" · ")[0]}</span> {c.note ? `“${c.note}”` : ""} · {c.updatedBy} · {ago(c.updatedAt, now)}
                    </span>
                  </span>
                  {canWrite && c.enabled && (
                    <Button size="sm" variant="ghost" onClick={() => setDisabling(c)}>
                      Disable
                    </Button>
                  )}
                </div>
              ))}
            </div>
          </Card>
        </Reveal>

        {/* preview */}
        <Reveal delay={0.08} className="xl:col-span-12">
          <Card>
            <CardHeader
              title="Impact preview"
              subtitle={p ? `${scopeText(p.scope, p.target, doc.data)} at ${lev(p.maxLeverage)} · ${p.appliesTo === "all_positions" ? "all open positions" : "new positions only"} · computed ${ago(p.generatedAt, now)}` : "Run a preview: the engine recomputes every account's margin with the cap"}
              icon={<Users />}
              action={p?.current ? <Chip size="sm" tone="info">Replaces {lev(p.current.maxLeverage)} {p.current.enabled ? "" : "(disabled)"}</Chip> : undefined}
            />
            <div className="space-y-4 px-4 pb-6 pt-4 sm:px-6">
              {previewErr && <RiskError error={previewErr} compact />}
              {!p && !previewErr && <div className="text-[12.5px] text-fg-3">{preview ? "The form changed since the last preview: preview again before applying." : "No preview yet."}</div>}
              {p && <PreviewBody p={p} />}
            </div>
          </Card>
        </Reveal>

        {/* audit */}
        <Reveal delay={0.1} className="xl:col-span-12">
          <Card>
            <CardHeader title="Change log" subtitle="Audit entries for leverage caps" icon={<History />} />
            <div className="space-y-1.5 px-4 pb-5 pt-4 sm:px-6">
              {audit.error && <RiskError error={audit.error} compact />}
              {!audit.error && !audit.data?.length && <div className="text-[12.5px] text-fg-3">{audit.data ? "No changes yet." : "Loading…"}</div>}
              {(audit.data ?? []).map((a) => {
                const after = (a.after as { cap?: Cap } | null)?.cap;
                return (
                  <div key={a.id} className="k-row flex flex-col gap-1.5 px-3.5 py-2.5 sm:flex-row sm:items-start sm:gap-4">
                    <span className="w-36 shrink-0 font-mono text-[11.5px] text-fg-2">{serverStamp(a.at)}</span>
                    <span className="w-40 shrink-0 text-[12.5px]">
                      {a.staff.name}
                      <span className="block text-[10.5px] text-fg-3">{a.staff.role}</span>
                    </span>
                    <span className="min-w-0 flex-1 text-[12.5px]">
                      {after ? `${(a.flags ?? []).includes("disabled") ? "Disabled" : "Set"} ${lev(after.maxLeverage)} · ${scopeText(after.scope, after.target, doc.data)}` : "Leverage cap"}
                      <span className="mt-1 flex flex-wrap gap-1">
                        {(a.flags ?? []).map((f) => (
                          <span key={f} className="rounded-md border border-line bg-surface-2 px-1.5 py-0.5 font-mono text-[10.5px] text-fg-3">
                            {f}
                          </span>
                        ))}
                      </span>
                    </span>
                    <span className="text-[12px] sm:w-56 sm:shrink-0">
                      <span className="font-mono text-[10.5px] text-ember">{a.reasonCode.split(" · ")[0]}</span> {a.reasonCode.split(" · ")[1]}
                      {a.note && (
                        <span className="block truncate text-[11px] text-fg-3" title={a.note}>
                          “{a.note}”
                        </span>
                      )}
                    </span>
                  </div>
                );
              })}
            </div>
          </Card>
        </Reveal>
      </div>

      <DeskDialog<ApplyResult>
        open={applying}
        onOpenChange={setApplying}
        title={`Apply ${max ? lev(max) : ""} · ${scopeText(scope, target, doc.data)}`}
        description={allPositions ? "Open positions are re-margined at once and the margin check runs on every affected account." : "New positions are margined at the cap; open positions keep their margin."}
        defaultCode={RISK_CODE}
        requireNote
        confirmLabel="Apply cap"
        confirmVariant={allPositions ? "sell" : "ember"}
        disabled={!fresh ? "Preview again first" : allPositions && confirmText.trim().toUpperCase() !== CONFIRM_WORD ? `Type ${CONFIRM_WORD} to confirm` : undefined}
        onConfirm={async (reason) => {
          const r = await tradingWrite<ApplyResult>("dealing/risk/leverage-caps", { scope, target, maxLeverage: max, appliesTo: applies }, reason, rest);
          if (r.ok) {
            setPreview(null);
            reloadAll();
          }
          return r;
        }}
        success={(d) => (d.recheck.accounts ? `Leverage capped at ${lev(d.cap.maxLeverage)}: ${d.recheck.accounts} account(s) re-checked, ${d.recheck.marginCalls} margin call, ${d.recheck.stopOuts} stop-out` : `Leverage capped at ${lev(d.cap.maxLeverage)}`)}
      >
        {p && (
          <div className={cn("rounded-[14px] border px-3.5 py-3 text-[12.5px]", allPositions && (p.totals.marginCall || p.totals.stopOut) ? "border-down/30 bg-down-soft" : "border-line bg-surface-2")}>
            {p.totals.accountsAffected} account(s) re-margined, margin {usdCompact(p.totals.marginBeforeUsd)} → {usdCompact(p.totals.marginAfterUsd)}.{" "}
            <span className={p.totals.marginCall ? "text-warn" : undefined}>{p.totals.marginCall} into margin call</span>, <span className={p.totals.stopOut ? "text-down" : undefined}>{p.totals.stopOut} into stop-out</span>
            {p.live.accountsAffected !== p.totals.accountsAffected ? ` (live: ${p.live.accountsAffected} affected, ${p.live.marginCall} margin call, ${p.live.stopOut} stop-out)` : ""}.
          </div>
        )}
        {allPositions && (
          <label className="block space-y-1.5">
            <span className="block text-[12.5px] text-fg-2">
              Type <span className="font-mono font-semibold text-down">{CONFIRM_WORD}</span> to re-margin every open position in scope
            </span>
            <Input value={confirmText} onChange={(e) => setConfirmText(e.target.value)} placeholder={CONFIRM_WORD} inputClassName="font-mono" aria-label="Confirmation" />
          </label>
        )}
      </DeskDialog>

      <DeskDialog<ApplyResult>
        open={!!disabling}
        onOpenChange={(o) => !o && setDisabling(null)}
        title={disabling ? `Disable ${lev(disabling.maxLeverage)} · ${scopeText(disabling.scope, disabling.target, doc.data)}` : "Disable cap"}
        description={
          disabling?.appliesTo === "all_positions"
            ? "Open positions go back to the normal leverage at once (margin calls clear on the re-check). New positions get the normal leverage."
            : "New positions get the normal leverage again. Positions opened under this cap keep the margin they opened with."
        }
        defaultCode={RISK_CODE}
        requireNote
        confirmLabel="Disable cap"
        onConfirm={async (reason) => {
          const r = await tradingWrite<ApplyResult>(`dealing/risk/leverage-caps/${disabling!.id}`, {}, reason, rest, "DELETE");
          if (r.ok) reloadAll();
          return r;
        }}
        success={(d) => `Cap ${lev(d.cap.maxLeverage)} disabled`}
      />
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Preview                                                             */
/* ------------------------------------------------------------------ */

const ROW_COLS: Column<Row>[] = [
  {
    key: "login",
    header: "Account",
    cell: (r) => (
      <span className="block">
        <span className="block font-mono text-[12.5px]">{r.login}</span>
        <span className="block text-[11px] text-fg-3">
          {r.group} · {r.type} · {lev(r.leverage)}
        </span>
      </span>
    ),
    sort: (r) => r.login,
    csv: (r) => r.login,
  },
  { key: "positions", header: "Positions", align: "right", cell: (r) => <span className="k-num">{r.positions}</span>, sort: (r) => r.positions },
  { key: "equity", header: "Equity", align: "right", cell: (r) => <span className="k-num font-mono">{money(r.equity)}</span>, sort: (r) => r.equity, csv: (r) => r.equity },
  { key: "mb", header: "Margin before", align: "right", cell: (r) => <span className="k-num font-mono text-fg-2">{money(r.marginBefore)}</span>, sort: (r) => r.marginBefore, csv: (r) => r.marginBefore },
  { key: "ma", header: "Margin after", align: "right", cell: (r) => <span className={cn("k-num font-mono", r.changed && "text-fg")}>{money(r.marginAfter)}</span>, sort: (r) => r.marginAfter, csv: (r) => r.marginAfter },
  { key: "ccy", header: "Ccy", cell: (r) => <span className="font-mono text-[11px] text-fg-3">{r.currency}</span>, hideOn: "md" },
  {
    key: "level",
    header: "Margin level",
    align: "right",
    cell: (r) => (
      <span className="k-num font-mono">
        <span className="text-fg-3">{pct(r.marginLevelBefore)} → </span>
        <span className={r.status === "stop_out" ? "text-down" : r.status === "margin_call" ? "text-warn" : undefined}>{pct(r.marginLevelAfter)}</span>
      </span>
    ),
    sort: (r) => r.marginLevelAfter ?? Number.MAX_SAFE_INTEGER,
    csv: (r) => r.marginLevelAfter ?? "",
  },
  { key: "status", header: "After", align: "center", cell: (r) => <StatusChip s={r.status} />, sort: (r) => (r.status === "stop_out" ? 0 : r.status === "margin_call" ? 1 : 2), csv: (r) => r.status },
];

function PreviewBody({ p }: { p: Preview }) {
  const t = p.totals;
  const tile = (label: string, value: React.ReactNode, sub: React.ReactNode, tone?: "down" | "warn") => (
    <div className={cn("k-row px-3.5 py-3", tone === "down" && "border-down/30 bg-down-soft", tone === "warn" && "border-warn/30 bg-warn-soft")}>
      <div className="text-[11px] uppercase tracking-[0.05em] text-fg-3">{label}</div>
      <div className="k-num mt-1 text-[20px] font-semibold">{value}</div>
      <div className="text-[11.5px] text-fg-3">{sub}</div>
    </div>
  );
  return (
    <>
      <div className="grid grid-cols-2 gap-2 lg:grid-cols-5">
        {tile("Accounts affected", t.accountsAffected, `${t.accountsInScope} in scope · ${t.positionsAffected} positions`)}
        {tile("Margin before", usdCompact(t.marginBeforeUsd), "USD, all accounts in scope")}
        {tile("Margin after", usdCompact(t.marginAfterUsd), t.marginIncreaseUsd > 0 ? `+${usdCompact(t.marginIncreaseUsd)}` : "unchanged")}
        {tile("Margin call", t.marginCall, `live ${p.live.marginCall}`, t.marginCall ? "warn" : undefined)}
        {tile("Stop-out", t.stopOut, `live ${p.live.stopOut}`, t.stopOut ? "down" : undefined)}
      </div>
      {p.appliesTo === "new_positions" && <p className="text-[12px] text-fg-3">New positions only: open positions keep their margin, so nobody falls into margin call by applying it. New orders need more free margin.</p>}
      {p.truncated && <p className="text-[12px] text-fg-3">Showing the 500 accounts with the lowest margin level after the cap; the totals cover every account.</p>}
      <DataTable columns={ROW_COLS} rows={p.accounts} dense pageSize={15} rowKey={(r) => r.login} search={(r) => `${r.login} ${r.group}`} searchPlaceholder="Login or group" exportName={`leverage-cap-preview-${p.maxLeverage}x`} empty={<EmptyState title="No open positions affected" text="No account in scope holds a position whose margin changes." illustration="shield" />} />
    </>
  );
}
