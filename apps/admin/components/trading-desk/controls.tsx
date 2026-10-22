"use client";

import * as React from "react";
import { AlertTriangle, Ban, CirclePause, CirclePlay, SlidersHorizontal } from "lucide-react";
import { Button, Card, CardHeader, Chip, DataTable, Field, Input, Menu, Segmented, SymbolCell, Toggle, Tooltip, cn, formatNumber, type Column } from "@kalks/ui";
import { INSTRUMENTS } from "@kalks/mock";
import { IS_DEMO } from "@kalks/mock/mode";
import type { TradingGroup } from "@kalks/mock/admin-trading";
import { MiniClient } from "@/components/trading/shared";
import { ago, clientName, groupLabel, groupOptions, useDesk, useLiveDirectory, type AccountControl, type ControlMode } from "@/lib/trading-desk";
import { AccountPicker, DeskDialog, MetaTile } from "./kit";
import { useCan } from "@/components/staff-session";
import { NeedsPerm } from "@/components/needs-perm";

/* ------------------------------------------------------------------ */
/* Account controls (D115)                                             */
/* ------------------------------------------------------------------ */

type Patch = Partial<Pick<AccountControl, "tradingDisabled" | "closeOnly" | "maxLot" | "execDelayMs" | "markupPips">>;

function ComplianceChip() {
  return (
    <Tooltip content="Execution delay is a form of last-look. It must be disclosed in the client agreement and applied symmetrically. Reviewed monthly by Compliance.">
      <span>
        <Chip size="sm" tone="warn">
          <AlertTriangle className="size-3" /> Compliance
        </Chip>
      </span>
    </Tooltip>
  );
}

export function AccountControls({ addOpen, onAddOpenChange }: { addOpen: boolean; onAddOpenChange: (o: boolean) => void }) {
  const { state, api } = useDesk();
  const canDeal = useCan("dealing.write");
  const [filter, setFilter] = React.useState<"all" | "delay" | "disabled">("all");
  const [pending, setPending] = React.useState<{ login: string; patch: Patch; label: string } | null>(null);
  const [addLogin, setAddLogin] = React.useState<string | null>(null);
  const [add, setAdd] = React.useState({ maxLot: "10", delay: "0", markup: "0.0", closeOnly: false, disabled: false });
  const rows = state.accountControls.filter((r) => filter === "all" || (filter === "delay" ? r.execDelayMs > 0 : r.tradingDisabled || r.closeOnly));
  const delayOn = state.tenant.execDelayEnabled;

  const cols: Column<AccountControl>[] = [
    { key: "c", header: "Client", cell: (r) => <MiniClient clientId={r.clientId} login={r.login} />, csv: (r) => `${clientName(r.clientId, r.login)} (${r.login})` },
    { key: "g", header: "Group", cell: (r) => <Chip size="sm" tone={/^vip$/i.test(r.group) ? "gold" : "neutral"}>{groupLabel(r.group)}</Chip>, csv: (r) => r.group },
    {
      key: "m",
      header: "Markup (pips)",
      align: "right",
      sort: (r) => r.markupPips,
      cell: (r) => (
        <span className="inline-flex items-center gap-1" onClick={(e) => e.stopPropagation()}>
          <NeedsPerm ok={canDeal} perm="dealing.write">
            <span className="inline-flex items-center gap-1">
              {[-0.1, 0.1].map((d) => (
                <button key={d} onClick={() => setPending({ login: r.login, patch: { markupPips: +(r.markupPips + d).toFixed(1) }, label: `Markup → ${(r.markupPips + d).toFixed(1)} pips` })} aria-label={d < 0 ? "Decrease markup" : "Increase markup"} className="grid size-6 place-items-center rounded-full border border-line text-[12px] text-fg-3 hover:text-fg">
                  {d < 0 ? "−" : "+"}
                </button>
              ))}
            </span>
          </NeedsPerm>
          <span className={cn("k-num w-12 text-right font-mono text-[12.5px]", r.markupPips < 0 ? "text-up" : r.markupPips > 0.6 ? "text-warn" : "text-fg")}>{r.markupPips > 0 ? "+" : ""}{r.markupPips.toFixed(1)}</span>
        </span>
      ),
    },
    {
      key: "ml",
      header: "Max lot",
      align: "right",
      sort: (r) => r.maxLot,
      cell: (r) => (
        <span onClick={(e) => e.stopPropagation()}>
          <NeedsPerm ok={canDeal} perm="dealing.write">
            <Menu
              width={140}
              items={[1, 2, 5, 10, 20, 50, 100].map((v) => ({ label: <span className="font-mono">{v} lots</span>, onSelect: () => setPending({ login: r.login, patch: { maxLot: v }, label: `Max lot → ${v}` }) }))}
              trigger={<button className="k-num rounded-full border border-line px-2.5 py-0.5 font-mono text-[12.5px] hover:border-fg-3">{r.maxLot || "—"}</button>}
            />
          </NeedsPerm>
        </span>
      ),
    },
    {
      key: "d",
      header: "Exec. delay",
      align: "right",
      sort: (r) => r.execDelayMs,
      cell: (r) => (
        <span className="inline-flex items-center justify-end gap-2" onClick={(e) => e.stopPropagation()}>
          {r.execDelayMs > 0 && <ComplianceChip />}
          <NeedsPerm ok={canDeal} perm="dealing.write">
            <Menu
              width={140}
              items={[0, 50, 120, 250, 500].map((v) => ({ label: <span className="font-mono">{v} ms</span>, onSelect: () => setPending({ login: r.login, patch: { execDelayMs: v }, label: `Execution delay → ${v} ms` }) }))}
              trigger={
                <button className={cn("k-num whitespace-nowrap rounded-full border border-line px-2.5 py-0.5 font-mono text-[12.5px] hover:border-fg-3", r.execDelayMs > 0 ? (delayOn ? "text-warn" : "text-fg-3 line-through") : "text-fg-3")} title={!delayOn && r.execDelayMs > 0 ? "Ignored — tenant policy blocks delays" : undefined}>
                  {r.execDelayMs} ms
                </button>
              }
            />
          </NeedsPerm>
        </span>
      ),
    },
    {
      key: "td",
      header: "Trading",
      align: "center",
      cell: (r) => (
        <span className="inline-flex items-center gap-2" onClick={(e) => e.stopPropagation()}>
          <NeedsPerm ok={canDeal} perm="dealing.write">
            <Toggle checked={!r.tradingDisabled} label={`Trading enabled for ${r.login}`} onChange={(v) => setPending({ login: r.login, patch: { tradingDisabled: !v }, label: v ? "Enable trading" : "Disable trading" })} />
          </NeedsPerm>
        </span>
      ),
      csv: (r) => (r.tradingDisabled ? "disabled" : "enabled"),
    },
    {
      key: "co",
      header: "Close-only",
      align: "center",
      cell: (r) => (
        <span onClick={(e) => e.stopPropagation()}>
          <NeedsPerm ok={canDeal} perm="dealing.write">
            <Toggle checked={r.closeOnly} label={`Close-only for ${r.login}`} onChange={(v) => setPending({ login: r.login, patch: { closeOnly: v }, label: v ? "Set close-only" : "Lift close-only" })} />
          </NeedsPerm>
        </span>
      ),
      csv: (r) => (r.closeOnly ? "yes" : "no"),
    },
    { key: "r", header: "Reason", hideOn: "xl", cell: (r) => <span className="whitespace-nowrap text-[12px] text-fg-2">{r.reason}</span>, csv: (r) => r.reason },
    { key: "by", header: "Set by", cell: (r) => <span className="whitespace-nowrap text-[12px] text-fg-3">{r.setBy} · {ago(r.updated)}</span>, sort: (r) => r.updated },
  ];

  return (
    <>
      <DataTable
        columns={cols}
        rows={rows}
        dense
        pageSize={12}
        rowKey={(r) => r.login}
        exportName="dealer-account-controls"
        search={(r) => `${r.login} ${clientName(r.clientId, r.login)} ${r.reason}`}
        toolbar={<Segmented size="sm" value={filter} onChange={setFilter} options={[{ value: "all", label: "All controls" }, { value: "delay", label: "With delay" }, { value: "disabled", label: "Disabled / close-only" }]} />}
      />
      <DeskDialog
        open={!!pending}
        onOpenChange={(o) => !o && setPending(null)}
        title={pending?.label ?? ""}
        description={pending ? `${clientName(state.accountControls.find((r) => r.login === pending.login)?.clientId ?? "", pending.login)} · ${pending.login}` : ""}
        confirmLabel="Apply control"
        onConfirm={(r) => api.setAccountControl(pending!.login, pending!.patch, r)}
        success="Account control applied"
      />
      <DeskDialog
        open={addOpen}
        onOpenChange={onAddOpenChange}
        title="New account control"
        description="Per-account dealer controls, enforced on every new trade (dealer or client)."
        confirmLabel="Create control"
        disabled={!addLogin && "Select an account"}
        onConfirm={(r) => api.setAccountControl(addLogin!, { maxLot: Number(add.maxLot) || 10, execDelayMs: Number(add.delay) || 0, markupPips: Number(add.markup) || 0, closeOnly: add.closeOnly, tradingDisabled: add.disabled }, r)}
        success="Account control created"
      >
        <AccountPicker value={addLogin} onChange={setAddLogin} />
        <div className="grid grid-cols-3 gap-3">
          <Field label="Max lot">
            <Input value={add.maxLot} onChange={(e) => setAdd({ ...add, maxLot: e.target.value })} aria-label="Max lot" className="font-mono" />
          </Field>
          <Field label="Markup (pips)">
            <Input value={add.markup} onChange={(e) => setAdd({ ...add, markup: e.target.value })} aria-label="Markup" className="font-mono" />
          </Field>
          <Field label="Delay (ms)" hint={delayOn ? `≤ ${state.tenant.execDelayCapMs}` : "blocked"}>
            <Input value={add.delay} disabled={!delayOn} onChange={(e) => setAdd({ ...add, delay: e.target.value })} aria-label="Execution delay" className="font-mono" />
          </Field>
        </div>
        <div className="flex flex-wrap gap-4 text-[12.5px]">
          <label className="flex items-center gap-2">
            <Toggle checked={add.closeOnly} onChange={(v) => setAdd({ ...add, closeOnly: v })} label="Close-only" /> Close-only
          </label>
          <label className="flex items-center gap-2">
            <Toggle checked={add.disabled} onChange={(v) => setAdd({ ...add, disabled: v })} label="Disable trading" /> Disable trading
          </label>
        </div>
      </DeskDialog>
    </>
  );
}

export function TenantDelayCard() {
  const { state, api } = useDesk();
  const canPolicy = useCan("dealing.policy");
  const [confirm, setConfirm] = React.useState(false);
  const on = state.tenant.execDelayEnabled;
  return (
    <Card className={cn("h-full", on && "border-warn/30")}>
      <CardHeader title="Execution delay (tenant policy · D115)" subtitle="Per-tenant switch for all per-account delays" icon={<SlidersHorizontal />} action={canPolicy ? <Toggle checked={on} onChange={() => setConfirm(true)} label="Allow execution delay" /> : <Chip size="sm" tone={on ? "warn" : "up"}>{on ? "Allowed" : "Blocked"}</Chip>} />
      <div className="px-6 pb-6 pt-4">
        <div className="flex items-start gap-3 rounded-[14px] border border-warn/30 bg-warn-soft px-4 py-3">
          <AlertTriangle className="mt-0.5 size-4 shrink-0 text-warn" />
          <div className="text-[12.5px] leading-relaxed text-fg-2">
            <span className="font-medium text-fg">Compliance warning.</span> Artificial execution delays can breach best-execution rules (e.g. CySEC, FCA). When on, the delay is applied symmetrically to every market fill on the account — profitable or not — and is flagged in the audit log.
          </div>
        </div>
        <div className="mt-3 grid grid-cols-3 gap-2 text-[12px]">
          <MetaTile label="Status" value={on ? "Allowed" : "Blocked"} tone={on ? "warn" : "up"} />
          <MetaTile label="Cap" value={`${state.tenant.execDelayCapMs} ms`} />
          <MetaTile label="Accounts w/ delay" value={state.accountControls.filter((c) => c.execDelayMs > 0).length} />
        </div>
      </div>
      <DeskDialog
        open={confirm}
        onOpenChange={setConfirm}
        title={on ? "Block execution delay for this tenant" : "Allow execution delay for this tenant"}
        description={on ? "All per-account delays are ignored immediately." : `Per-account delays (max ${state.tenant.execDelayCapMs} ms) become active. Compliance is notified.`}
        confirmLabel={on ? "Block delays" : "Allow delays"}
        confirmVariant={on ? "buy" : "sell"}
        onConfirm={(r) => api.setTenantPolicy({ execDelayEnabled: !on }, r)}
        success={on ? "Execution delay blocked tenant-wide" : "Execution delay allowed"}
      />
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Symbol controls (D140): halt / close-only / resume                  */
/* ------------------------------------------------------------------ */

export function SymbolControls() {
  const { state, api } = useDesk();
  const canDeal = useCan("dealing.write");
  const dir = useLiveDirectory();
  const [scope, setScope] = React.useState<TradingGroup | "all">("all");
  const [pending, setPending] = React.useState<{ symbol: string; mode: ControlMode | null } | null>(null);
  const [q, setQ] = React.useState("");
  const counts = React.useMemo(() => {
    const m = new Map<string, { pos: number; lots: number; orders: number }>();
    for (const p of state.positions) {
      const x = m.get(p.symbol) ?? { pos: 0, lots: 0, orders: 0 };
      x.pos++;
      x.lots += p.volume;
      m.set(p.symbol, x);
    }
    for (const o of state.orders) {
      const x = m.get(o.symbol) ?? { pos: 0, lots: 0, orders: 0 };
      x.orders++;
      m.set(o.symbol, x);
    }
    return m;
  }, [state.positions, state.orders]);
  const universe = IS_DEMO || !dir.symbols.size ? INSTRUMENTS.map((i) => i.symbol) : [...dir.symbols.keys()];
  const symbols = universe
    .filter((s) => !q || s.includes(q.toUpperCase()))
    .sort((a, b) => (counts.get(b)?.pos ?? 0) - (counts.get(a)?.pos ?? 0));
  const ctl = (s: string) => state.symbolControls.find((c) => c.symbol === s && c.group === scope);
  const inherited = (s: string) => (scope !== "all" ? state.symbolControls.find((c) => c.symbol === s && c.group === "all") : undefined);

  return (
    <div className="space-y-4">
      {state.symbolControls.length > 0 && (
        <div className="flex flex-wrap gap-2">
          {state.symbolControls.map((c) => (
            <span key={c.id} className={cn("inline-flex items-center gap-2 rounded-full border px-3 py-1 text-[12px]", c.mode === "halt" ? "border-down/30 bg-down-soft" : "border-warn/30 bg-warn-soft")}>
              <span className="font-mono font-medium">{c.symbol}</span>
              <span className={c.mode === "halt" ? "text-down" : "text-warn"}>{c.mode === "halt" ? "Halted" : "Close-only"}</span>
              <span className="text-fg-3">· {c.group === "all" ? "all groups" : groupLabel(c.group)} · {c.staff} · {ago(c.at)}</span>
            </span>
          ))}
        </div>
      )}
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-[12.5px] font-medium text-fg-2">Scope</span>
        <Segmented size="sm" value={scope} onChange={setScope} options={[{ value: "all", label: "All groups" }, ...groupOptions().map((g) => ({ value: g.value as TradingGroup, label: g.label }))]} />
        <Input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Symbol" aria-label="Filter symbols" className="ml-auto h-8 w-32 rounded-full font-mono text-[12.5px]" />
      </div>
      <div className="grid grid-cols-1 gap-2 md:grid-cols-2 2xl:grid-cols-3">
        {symbols.map((s) => {
          const c = ctl(s);
          const inh = inherited(s);
          const n = counts.get(s);
          const mode = c?.mode ?? null;
          return (
            <div key={s} className={cn("k-row flex items-center gap-3 px-3.5 py-2.5", mode === "halt" && "border-down/30", mode === "close-only" && "border-warn/30")}>
              <SymbolCell symbol={s} size={24} sub={<span className="k-num text-[11px] text-fg-3">{n ? `${n.pos} pos · ${formatNumber(n.lots, 2)} lots · ${n.orders} orders` : "no open risk"}</span>} />
              <div className="ml-auto flex items-center gap-1.5">
                {mode ? (
                  <Chip size="sm" tone={mode === "halt" ? "down" : "warn"} dot>
                    {mode === "halt" ? "Halted" : "Close-only"}
                  </Chip>
                ) : inh ? (
                  <Chip size="sm" tone="neutral">{inh.mode === "halt" ? "Halted (all)" : "Close-only (all)"}</Chip>
                ) : (
                  <Chip size="sm" tone="up" dot>
                    Trading
                  </Chip>
                )}
                <NeedsPerm ok={canDeal} perm="dealing.write">
                  <span className="flex items-center gap-1.5">
                    {mode !== "halt" && (
                      <Button size="xs" variant="surface" onClick={() => setPending({ symbol: s, mode: "halt" })} aria-label={`Halt ${s}`}>
                        <Ban /> Halt
                      </Button>
                    )}
                    {mode !== "close-only" && (
                      <Button size="xs" variant="surface" onClick={() => setPending({ symbol: s, mode: "close-only" })} aria-label={`Close-only ${s}`}>
                        <CirclePause /> Close-only
                      </Button>
                    )}
                    {mode && (
                      <Button size="xs" variant="up-outline" onClick={() => setPending({ symbol: s, mode: null })} aria-label={`Resume ${s}`}>
                        <CirclePlay /> Resume
                      </Button>
                    )}
                  </span>
                </NeedsPerm>
              </div>
            </div>
          );
        })}
      </div>
      <DeskDialog
        open={!!pending}
        onOpenChange={(o) => !o && setPending(null)}
        title={pending ? `${pending.mode === "halt" ? "Halt" : pending.mode === "close-only" ? "Set close-only on" : "Resume"} ${pending.symbol}${scope === "all" ? "" : ` for ${groupLabel(scope)}`}` : ""}
        description={
          pending?.mode === "halt"
            ? "No new trades, pending orders or closes (dealer force-close still works). Takes effect on the next order."
            : pending?.mode === "close-only"
              ? "Existing positions can be closed; new positions and pending orders are rejected."
              : "Normal trading resumes for this scope."
        }
        confirmLabel={pending?.mode === "halt" ? "Halt symbol" : pending?.mode === "close-only" ? "Set close-only" : "Resume trading"}
        confirmVariant={pending?.mode ? "sell" : "buy"}
        onConfirm={(r) => api.setSymbolControl(pending!.symbol, scope, pending!.mode, r)}
        success={pending ? `${pending.symbol} ${pending.mode === "halt" ? "halted" : pending.mode === "close-only" ? "is close-only" : "resumed"}` : ""}
      />
    </div>
  );
}

