"use client";

import * as React from "react";
import Link from "next/link";
import { motion } from "motion/react";
import { AlertTriangle, ArrowRight, Cable, Info, GripVertical, Pencil, Plug, Plus, Route as RouteIcon, Server, Split, Trash2, X } from "lucide-react";
import { toast } from "sonner";
import {
  Button,
  Card,
  CardHeader,
  Chip,
  Dialog,
  DialogClose,
  Field,
  Input,
  Menu,
  PageHeader,
  Reveal,
  Segmented,
  SymbolAvatar,
  Toggle,
  cn,
  formatMoney,
  formatNumber,
} from "@kalks/ui";
import { LP_CONNECTIONS, ROUTING_DEFAULT, type RoutingCondition, type RoutingRule } from "@kalks/mock/admin-trading";
import { IS_DEMO } from "@kalks/mock/mode";
import { clientName, currentPriceOf, getAccount, groupOptions, notionalUsd as deskNotional, positionPnl, useDesk, useLiveDirectory, type Book } from "@/lib/trading-desk";
import { DeskStatusChip } from "@/components/trading-desk/status";
import { NeedsPerm } from "@/components/needs-perm";
import { useCan } from "@/components/staff-session";
import { AccountPicker, BookChip, DeskDialog } from "@/components/trading-desk/kit";
import { EXPOSURE, notionalUsd } from "@kalks/mock/admin-ops";
import { getInstrument } from "@kalks/mock";
import { ShareBar, usdCompact } from "@/components/command/kit";

const FIELDS: RoutingCondition["field"][] = ["Risk score", "Risk category", "Avg hold time", "Lot size", "Symbol", "Group", "Login", "Win rate (30d)", "Equity", "Country", "News window"];
const OPS: RoutingCondition["op"][] = ["≥", "≤", ">", "<", "=", "in", "is"];
const DEF: Record<RoutingCondition["field"], [RoutingCondition["op"], string]> = {
  "Risk score": ["≥", "70"],
  "Risk category": ["in", "toxic, likely_profitable"],
  "Avg hold time": ["<", "60s"],
  "Lot size": ["≥", "20"],
  Symbol: ["in", "XAUUSD"],
  Group: ["is", "VIP"],
  Login: ["=", "80412337"],
  "Win rate (30d)": ["≥", "70%"],
  Equity: ["≥", "$50,000"],
  Country: ["in", "AE, SA"],
  "News window": ["is", "High impact ±2m"],
};

function CondChip({ c }: { c: RoutingCondition }) {
  return (
    <span className="inline-flex items-center gap-1.5 rounded-full border border-line bg-surface-3/70 px-2.5 py-1 text-[12px]">
      <span className="text-fg">{c.field}</span>
      <span className="font-mono text-ember">{c.op}</span>
      <span className="font-mono text-fg">{c.value}</span>
    </span>
  );
}

function RuleEditor({ rule, open, onOpenChange, onSave }: { rule: RoutingRule; open: boolean; onOpenChange: (o: boolean) => void; onSave: (r: RoutingRule) => void }) {
  const [draft, setDraft] = React.useState(rule);
  React.useEffect(() => {
    if (open) setDraft(rule);
  }, [open, rule]);
  const setCond = (i: number, p: Partial<RoutingCondition>) => setDraft((d) => ({ ...d, conditions: d.conditions.map((c, k) => (k === i ? { ...c, ...p } : c)) }));
  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      width={640}
      title={rule.id === "new" ? "New routing rule" : `Edit ${rule.id}`}
      description="Rules are evaluated top-down on every new order; the first match wins."
      footer={
        <>
          <DialogClose asChild>
            <Button size="sm" variant="ghost">Cancel</Button>
          </DialogClose>
          <Button
            size="sm"
            variant="ember"
            disabled={!draft.conditions.length || !draft.name.trim()}
            onClick={() => {
              onSave(draft);
              onOpenChange(false);
            }}
          >
            Save rule
          </Button>
        </>
      }
    >
      <div className="space-y-5">
        <Field label="Rule name">
          <Input value={draft.name} onChange={(e) => setDraft({ ...draft, name: e.target.value })} />
        </Field>
        <div>
          <div className="mb-2 flex items-center justify-between">
            <span className="text-[12.5px] font-medium text-fg-2">IF</span>
            <Segmented size="xs" value={draft.join} onChange={(j) => setDraft({ ...draft, join: j })} options={["AND", "OR"] as const} />
          </div>
          <div className="space-y-2">
            {draft.conditions.map((c, i) => (
              <div key={i} className="flex items-center gap-2">
                <span className="w-9 text-center font-mono text-[10.5px] text-fg-3">{i === 0 ? "IF" : draft.join}</span>
                <Menu
                  align="start"
                  width={200}
                  items={FIELDS.map((f) => ({ label: f, onSelect: () => setCond(i, { field: f, op: DEF[f][0], value: DEF[f][1] }) }))}
                  trigger={<button className="h-10 flex-1 rounded-[12px] border border-line bg-surface-2 px-3 text-left text-[13px] hover:border-fg-3">{c.field}</button>}
                />
                <Menu
                  width={100}
                  items={OPS.map((o) => ({ label: <span className="font-mono">{o}</span>, onSelect: () => setCond(i, { op: o }) }))}
                  trigger={<button className="h-10 w-14 rounded-[12px] border border-line bg-surface-2 font-mono text-[13px] text-ember hover:border-fg-3">{c.op}</button>}
                />
                <Input value={c.value} onChange={(e) => setCond(i, { value: e.target.value })} className="h-10 w-40 font-mono text-[13px]" />
                <button onClick={() => setDraft((d) => ({ ...d, conditions: d.conditions.filter((_, k) => k !== i) }))} className="grid size-8 place-items-center rounded-full text-fg-3 hover:bg-surface-3 hover:text-down" aria-label="Remove">
                  <X className="size-4" />
                </button>
              </div>
            ))}
            <button onClick={() => setDraft((d) => ({ ...d, conditions: [...d.conditions, { field: "Symbol", op: "in", value: "XAUUSD" }] }))} className="ml-11 flex h-8 items-center gap-1 rounded-full border border-dashed border-fg-3/60 px-3 text-[12px] text-fg-2 hover:border-ember hover:text-ember">
              <Plus className="size-3.5" /> Add condition
            </button>
          </div>
        </div>
        <div className="rounded-[14px] border border-line bg-surface-2 p-4">
          <div className="mb-3 text-[12.5px] font-medium text-fg-2">THEN route</div>
          <div className="flex flex-wrap items-center gap-3">
            <Segmented size="sm" value={draft.action.book} onChange={(b) => setDraft({ ...draft, action: { ...draft.action, book: b } })} options={[{ value: "A", label: "A-book" }, { value: "B", label: "B-book" }]} />
            <Segmented size="sm" value={String(draft.action.pct)} onChange={(p) => setDraft({ ...draft, action: { ...draft.action, pct: Number(p) } })} options={["25", "50", "75", "100"] as const} />
            <span className="text-[12px] text-fg-3">% of volume{draft.action.book === "A" ? " → Primary LP" : ""}</span>
          </div>
        </div>
      </div>
    </Dialog>
  );
}

function ConnectLp() {
  const [testing, setTesting] = React.useState(false);
  return (
    <Dialog
      title="Connect liquidity provider"
      description="FIX 4.4 session for A-book hedging. Credentials are stored in the HSM-backed vault."
      width={560}
      trigger={
        <Button variant="ember" size="sm" className="w-full">
          <Plug /> Connect LP (FIX 4.4)
        </Button>
      }
      footer={
        <>
          <Button
            size="sm"
            variant="surface"
            disabled={testing}
            onClick={() => {
              setTesting(true);
              setTimeout(() => {
                setTesting(false);
                toast.error("Logon rejected (35=5)", { description: "TargetCompID not recognised — check with LP onboarding" });
              }, 1400);
            }}
          >
            {testing ? "Testing…" : "Test connection"}
          </Button>
          <DialogClose asChild>
            <Button size="sm" variant="ember" onClick={() => toast.success("LP session saved", { description: "Will connect at next session start (00:05 GMT+3)" })}>
              Save session
            </Button>
          </DialogClose>
        </>
      }
    >
      <div className="grid grid-cols-2 gap-3">
        <Field label="SenderCompID"><Input defaultValue="KALKS_PROD" className="font-mono" /></Field>
        <Field label="TargetCompID"><Input placeholder="LP_TRADE" className="font-mono" /></Field>
        <Field label="Host"><Input placeholder="fix.lp-example.net" className="font-mono" /></Field>
        <Field label="Port"><Input placeholder="9876" className="font-mono" /></Field>
        <Field label="Password"><Input type="password" placeholder="••••••••" /></Field>
        <Field label="Heartbeat (s)"><Input defaultValue="30" className="font-mono" /></Field>
      </div>
      <div className="mt-4 flex items-center justify-between rounded-[14px] border border-line bg-surface-2 px-4 py-3 text-[12.5px]">
        <span className="text-fg-2">TLS · stunnel sidecar · IP allow-list 185.44.12.0/28</span>
        <Chip size="sm" tone="up">Required</Chip>
      </div>
    </Dialog>
  );
}

export default function RoutingPage() {
  const { state } = useDesk();
  const canDeal = useCan("dealing.write");
  const rules = state.routingRules.filter((r) => IS_DEMO || !r.id.startsWith("RQ-"));
  const [pending, setPending] = React.useState<{ rules: RoutingRule[]; summary: string } | null>(null);
  // live: quick routes (RQ-…) are edited in their own card and kept ahead of the rule set on every publish
  const quick = IS_DEMO ? [] : state.routingRules.filter((r) => r.id.startsWith("RQ-"));
  const propose = (next: RoutingRule[], summary: string) => setPending({ rules: [...quick, ...next], summary });
  const [edit, setEdit] = React.useState<RoutingRule | null>(null);
  const [dragId, setDragId] = React.useState<string | null>(null);
  const [overId, setOverId] = React.useState<string | null>(null);

  const allLots = ROUTING_DEFAULT.lots24h + rules.reduce((s, r) => s + r.lots24h, 0);
  const aLots = rules.filter((r) => r.enabled && r.action.book === "A").reduce((s, r) => s + (r.lots24h * r.action.pct) / 100, 0);
  const aPct = (aLots / allLots) * 100;
  const lpConnected = LP_CONNECTIONS.some((l) => l.status === "connected");
  const hedge = EXPOSURE.slice(0, 6).map((e, i) => {
    const inst = getInstrument(e.symbol);
    const net = notionalUsd(e.symbol, e.buyLots - e.sellLots, inst.price);
    const share = Math.min(0.9, aPct / 100 + [0.22, 0.08, 0.3, 0.12, 0.05, 0.1][i]!);
    return { symbol: e.symbol, net, hedged: net * share, share };
  });

  const move = (from: string, to: string) => {
    if (from === to) return;
    const a = [...rules];
    const fi = a.findIndex((x) => x.id === from);
    const ti = a.findIndex((x) => x.id === to);
    const [m] = a.splice(fi, 1);
    a.splice(ti, 0, m!);
    propose(a, `Priority: ${from} moved to position ${ti + 1}`);
  };

  return (
    <div className="pb-10">
      <PageHeader
        title="Book & routing"
        subtitle={<span className="inline-flex flex-wrap items-center gap-2">Hybrid A/B book. Orders are matched against these rules top-down; the first match decides the route. <DeskStatusChip /></span>}
        actions={
          canDeal && (
            <Button variant="ember" size="lg" onClick={() => setEdit({ id: "new", name: "New rule", conditions: IS_DEMO ? [{ field: "Risk score", op: "≥", value: "7" }] : [{ field: "Lot size", op: "≥", value: "10" }], join: "AND", action: { book: "A", pct: 100, ...(IS_DEMO ? { lp: "Primary LP" } : {}) }, enabled: true, hits24h: 0, lots24h: 0 })}>
              <Plus /> New rule
            </Button>
          )
        }
      />

      {!lpConnected && (
        <Reveal>
          <div className="mb-4 flex flex-wrap items-center gap-3 rounded-[16px] border border-warn/30 bg-warn-soft px-5 py-3.5">
            <AlertTriangle className="size-4 text-warn" />
            <span className="flex-1 text-[13px]">
              <span className="font-medium">No LP connected.</span> <span className="text-fg-2">A-book matches are held on the B-book and flagged for manual hedging until a FIX 4.4 session is live.</span>
            </span>
            <Chip tone="warn">Fallback: B-book</Chip>
          </div>
        </Reveal>
      )}

      {!IS_DEMO && (
        <Reveal>
          <div className="mb-4 flex flex-wrap items-start gap-3 rounded-[16px] border border-line bg-surface-2/60 px-5 py-3.5 text-[12.5px]">
            <Info className="mt-0.5 size-4 shrink-0 text-fg-3" />
            <span className="min-w-0 flex-1 text-fg-2">
              Conditions on <span className="text-fg">risk score</span> (0-100), <span className="text-fg">risk category</span>, <span className="text-fg">avg hold time</span>, <span className="text-fg">win rate</span>, <span className="text-fg">country</span>, <span className="text-fg">equity</span> and <span className="text-fg">news window</span> are now evaluated by the engine. A rule with a share under 100% splits deterministically by ticket (that share of tickets goes to the rule&apos;s book). In recommend mode rules only label trades and queue{" "}
              <Link href="/trading/routing-suggestions" className="text-fg underline decoration-line underline-offset-2 hover:text-ember">
                routing suggestions
              </Link>
              .
            </span>
          </div>
        </Reveal>
      )}

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal delay={0.05} className="xl:col-span-8">
          <Card>
            <CardHeader title="Routing rules" subtitle={`${rules.filter((r) => r.enabled).length} active · drag to change priority`} icon={<Split />} />
            <div className="mt-4 space-y-2 px-4 pb-5 sm:px-6">
              {rules.map((r, i) => (
                <motion.div layout key={r.id} transition={{ type: "spring", bounce: 0.15, duration: 0.4 }}>
                <div
                  draggable={canDeal}
                  onDragStart={() => canDeal && setDragId(r.id)}
                  onDragOver={(e) => {
                    e.preventDefault();
                    setOverId(r.id);
                  }}
                  onDragEnd={() => {
                    setDragId(null);
                    setOverId(null);
                  }}
                  onDrop={() => dragId && move(dragId, r.id)}
                  className={cn("k-row flex items-start gap-3 px-3 py-3.5 transition-colors", !r.enabled && "opacity-55", overId === r.id && dragId !== r.id && "border-ember/50", dragId === r.id && "opacity-40")}
                >
                  <span className={cn("mt-1 text-fg-3", canDeal ? "cursor-grab hover:text-fg active:cursor-grabbing" : "cursor-not-allowed opacity-50")} aria-label={canDeal ? "Drag to reorder" : "Reordering needs the dealing.write permission"} title={canDeal ? undefined : "Needs the dealing.write permission"}>
                    <GripVertical className="size-4" />
                  </span>
                  <span className="k-num mt-0.5 grid size-6 shrink-0 place-items-center rounded-full bg-surface-3 font-mono text-[11px] text-fg-2">{i + 1}</span>
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="text-[13.5px] font-medium">{r.name}</span>
                      <span className="font-mono text-[10.5px] text-fg-3">{r.id}</span>
                    </div>
                    <div className="mt-2 flex flex-wrap items-center gap-1.5">
                      <span className="font-mono text-[10.5px] font-semibold text-fg-3">IF</span>
                      {r.conditions.map((c, k) => (
                        <React.Fragment key={k}>
                          {k > 0 && <span className="font-mono text-[10.5px] font-semibold text-fg-3">{r.join}</span>}
                          <CondChip c={c} />
                        </React.Fragment>
                      ))}
                      <ArrowRight className="mx-1 size-3.5 text-fg-3" />
                      <Chip tone={r.action.book === "A" ? "info" : "neutral"}>
                        {r.action.book}-book {r.action.pct}%{r.action.lp ? ` → ${r.action.lp}` : ""}
                      </Chip>
                    </div>
                  </div>
                  <div className="hidden text-right sm:block">
                    <div className="k-num font-mono text-[13px]">{formatNumber(r.hits24h, 0)}</div>
                    <div className="text-[10.5px] text-fg-3">{formatNumber(r.lots24h, 1)} lots · 24h</div>
                  </div>
                  <NeedsPerm ok={canDeal} perm="dealing.write">
                  <div className="flex items-center gap-1">
                    <button onClick={() => setEdit(r)} className="grid size-8 place-items-center rounded-full text-fg-3 hover:bg-surface-3 hover:text-fg" aria-label="Edit rule">
                      <Pencil className="size-3.5" />
                    </button>
                    <button
                      onClick={() => propose(rules.filter((x) => x.id !== r.id), `Deleted ${r.id} · ${r.name}`)}
                      className="grid size-8 place-items-center rounded-full text-fg-3 hover:bg-down-soft hover:text-down"
                      aria-label="Delete rule"
                    >
                      <Trash2 className="size-3.5" />
                    </button>
                    <Toggle
                      checked={r.enabled}
                      label={`Toggle ${r.name}`}
                      onChange={(v) => propose(rules.map((x) => (x.id === r.id ? { ...x, enabled: v } : x)), `${v ? "Enabled" : "Disabled"} ${r.id} · ${r.name}`)}
                    />
                  </div>
                  </NeedsPerm>
                </div>
                </motion.div>
              ))}
              <div className="flex items-center gap-3 rounded-[14px] border border-dashed border-line px-3 py-3.5">
                <span className="w-4" />
                <span className="grid size-6 place-items-center rounded-full bg-surface-3 font-mono text-[10px] text-fg-3">∞</span>
                <span className="flex-1 text-[13px]">
                  <span className="font-mono text-[10.5px] font-semibold text-fg-3">DEFAULT</span> <ArrowRight className="mx-1 inline size-3.5 text-fg-3" />
                  {IS_DEMO ? <Chip>B-book {ROUTING_DEFAULT.pct}%</Chip> : <Chip>Account route → group default route</Chip>}
                </span>
                {IS_DEMO && (
                  <div className="text-right">
                    <div className="k-num font-mono text-[13px]">{formatNumber(ROUTING_DEFAULT.hits24h, 0)}</div>
                    <div className="text-[10.5px] text-fg-3">{formatNumber(ROUTING_DEFAULT.lots24h, 1)} lots · 24h</div>
                  </div>
                )}
              </div>
            </div>
          </Card>
        </Reveal>

        <div className="space-y-4 xl:col-span-4">
          <Reveal delay={0.08}>
            <QuickRoutes />
          </Reveal>
          {!IS_DEMO && (
            <Reveal delay={0.1}>
              <LiveBookSplit />
            </Reveal>
          )}
          {IS_DEMO && (
          <Reveal delay={0.1}>
            <Card>
              <CardHeader title="Simulated impact" subtitle="Last 24h flow replayed through current rules" />
              <div className="px-6 pb-6 pt-4">
                <div className="flex items-end justify-between">
                  <div>
                    <div className="text-[11px] uppercase tracking-wider text-info">A-book</div>
                    <div className="k-num text-[26px] font-semibold">{aPct.toFixed(1)}%</div>
                  </div>
                  <div className="text-right">
                    <div className="text-[11px] uppercase tracking-wider text-fg-2">B-book</div>
                    <div className="k-num text-[26px] font-semibold">{(100 - aPct).toFixed(1)}%</div>
                  </div>
                </div>
                <ShareBar
                  className="mt-3"
                  height={10}
                  parts={[
                    { value: aPct, className: "bg-info" },
                    { value: 100 - aPct, className: "bg-gradient-to-r from-ember to-[#ff8a3d]" },
                  ]}
                />
                <div className="k-num mt-2 flex justify-between font-mono text-[11.5px] text-fg-3">
                  <span>{formatNumber(aLots, 1)} lots</span>
                  <span>{formatNumber(allLots - aLots, 1)} lots</span>
                </div>
                <div className="mt-4 grid grid-cols-2 gap-2">
                  <div className="k-row px-3 py-2.5">
                    <div className="text-[10.5px] uppercase tracking-wider text-fg-3">Toxic P&L avoided</div>
                    <div className="k-num font-mono text-[14px] text-up">+{formatMoney(aLots * 41.8, "USD", 0)}</div>
                  </div>
                  <div className="k-row px-3 py-2.5">
                    <div className="text-[10.5px] uppercase tracking-wider text-fg-3">B-book revenue ceded</div>
                    <div className="k-num font-mono text-[14px] text-down">-{formatMoney(aLots * 6.2, "USD", 0)}</div>
                  </div>
                </div>
                <div className="mt-4 text-[11px] uppercase tracking-wider text-fg-3">Hedged exposure by symbol</div>
                <div className="mt-2 space-y-2">
                  {hedge.map((h) => (
                    <div key={h.symbol} className="flex items-center gap-2.5 text-[12px]">
                      <SymbolAvatar symbol={h.symbol} size={18} />
                      <span className="w-14">{h.symbol}</span>
                      <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-surface-3">
                        <motion.div className="h-full rounded-full bg-info" animate={{ width: `${h.share * 100}%` }} transition={{ duration: 0.6 }} />
                      </div>
                      <span className="k-num w-16 text-right font-mono text-fg-2">{usdCompact(h.hedged, 1)}</span>
                    </div>
                  ))}
                </div>
              </div>
            </Card>
          </Reveal>
          )}
          <Reveal delay={0.15}>
            <Card>
              <CardHeader title="LP connections" subtitle="A-book execution venues" icon={<Cable />} />
              <div className="space-y-2 px-6 pb-6 pt-4">
                {LP_CONNECTIONS.map((l) => (
                  <div key={l.name} className="k-row flex items-center gap-3 px-3.5 py-2.5">
                    <Server className="size-4 text-fg-3" />
                    <div className="flex-1">
                      <div className="text-[13px] font-medium">{l.name}</div>
                      <div className="text-[11.5px] text-fg-3">
                        {l.protocol} · {l.note}
                      </div>
                    </div>
                    <Chip size="sm" tone="neutral" dot>
                      Not connected
                    </Chip>
                  </div>
                ))}
                {IS_DEMO ? (
                  <div className="pt-2">
                    <ConnectLp />
                  </div>
                ) : (
                  <div className="rounded-[14px] border border-line bg-surface-2/60 px-3.5 py-2.5 text-[12px] leading-relaxed text-fg-3">
                    The engine records the A/B decision on every ticket and calls the LP adapter for A-book fills. No LP adapter is connected yet, so A-book trades are executed internally and flagged for manual hedging.
                  </div>
                )}
              </div>
            </Card>
          </Reveal>
        </div>
      </div>

      {edit && (
        <RuleEditor
          rule={edit}
          open={!!edit}
          onOpenChange={(o) => !o && setEdit(null)}
          onSave={(r) => {
            const id = r.id === "new" ? `RR-${Math.max(4, ...rules.filter((x) => x.id.startsWith("RR-")).map((x) => Number(x.id.slice(3)) || 0)) + 1}` : r.id;
            propose(r.id === "new" ? [...rules, { ...r, id }] : rules.map((x) => (x.id === r.id ? r : x)), `${r.id === "new" ? "Created" : "Edited"} ${id} · ${r.name}`);
          }}
        />
      )}
      <PublishRules pending={pending} onClose={() => setPending(null)} />
    </div>
  );
}

function PublishRules({ pending, onClose }: { pending: { rules: RoutingRule[]; summary: string } | null; onClose: () => void }) {
  const { api } = useDesk();
  return (
    <DeskDialog
      open={!!pending}
      onOpenChange={(o) => !o && onClose()}
      title="Publish routing change"
      description={pending?.summary}
      confirmLabel="Publish rules"
      onConfirm={(r) => api.saveRoutingRules(pending!.rules, r, pending!.summary)}
      success="Routing rules published · applies to new trades"
    />
  );
}

/** Per-group / per-account "route new trades to A/B" overrides (rules RQ-G… / RQ-L…, evaluated first). */
function QuickRoutes() {
  const { state, api } = useDesk();
  useLiveDirectory();
  const canDeal = useCan("dealing.write");
  const [login, setLogin] = React.useState<string | null>(null);
  const [pending, setPending] = React.useState<{ scope: { login: string } | { group: string }; book: Book | null; label: string } | null>(null);
  const quick = state.routingRules.filter((r) => r.id.startsWith("RQ-"));
  const groupBook = (g: string) => quick.find((r) => r.id === `RQ-G${g}`)?.action.book ?? null;
  const loginRules = quick.filter((r) => r.id.startsWith("RQ-L"));
  return (
    <Card>
      <CardHeader title="Quick routes" subtitle="New trades only · by group or account" icon={<RouteIcon />} />
      <div className="space-y-4 px-6 pb-6 pt-4">
        <div className="space-y-1.5">
          {groupOptions().map(({ value: g, label }) => {
            const b = groupBook(g);
            return (
              <div key={g} className="flex items-center gap-2 text-[12.5px]">
                <span className="w-24 truncate">{label}</span>
                <NeedsPerm ok={canDeal} perm="dealing.write">
                  <Segmented
                    size="xs"
                    value={b ?? "rules"}
                    onChange={(v) => canDeal && setPending({ scope: { group: g }, book: v === "rules" ? null : (v as Book), label: `${label}: ${v === "rules" ? "follow rules" : `route new trades to ${v}-book`}` })}
                    options={[{ value: "rules", label: "Rules" }, { value: "A", label: "A-book" }, { value: "B", label: "B-book" }]}
                  />
                </NeedsPerm>
              </div>
            );
          })}
        </div>
        <div className="border-t border-line pt-4">
          <AccountPicker value={login} onChange={setLogin} />
          <div className="mt-2 flex gap-2">
            <NeedsPerm ok={canDeal} perm="dealing.write">
              <span className="flex gap-2">
                {(["A", "B"] as const).map((b) => (
                  <Button key={b} size="xs" variant="surface" disabled={!login || !canDeal} onClick={() => login && setPending({ scope: { login }, book: b, label: `${login}: route new trades to ${b}-book` })}>
                    Route to {b}-book
                  </Button>
                ))}
              </span>
            </NeedsPerm>
          </div>
          {loginRules.length > 0 && (
            <div className="mt-3 space-y-1.5">
              {loginRules.map((r) => {
                const l = r.conditions[0]?.value ?? "";
                const acc = getAccount(l);
                return (
                  <div key={r.id} className="k-row flex items-center gap-2 px-3 py-1.5 text-[12px]">
                    <span className="font-mono">{l}</span>
                    <span className="flex-1 truncate text-fg-3">{acc ? clientName(acc.clientId, acc.login) : ""}</span>
                    <BookChip book={r.action.book} />
                    <NeedsPerm ok={canDeal} perm="dealing.write">
                      <button type="button" onClick={() => setPending({ scope: { login: l }, book: null, label: `${l}: remove account route` })} className="grid size-6 place-items-center rounded-full text-fg-3 hover:bg-surface-3 hover:text-down" aria-label={`Remove route for ${l}`}>
                        <X className="size-3.5" />
                      </button>
                    </NeedsPerm>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </div>
      <DeskDialog
        open={!!pending}
        onOpenChange={(o) => !o && setPending(null)}
        title="Change routing"
        description={pending?.label}
        confirmLabel="Apply route"
        onConfirm={(r) => api.quickRoute(pending!.scope, pending!.book, r)}
        success={pending?.label ?? "Route updated"}
      />
    </Card>
  );
}

/** Live: where the open risk sits right now (engine positions), A vs B, with the largest net A-book symbols. */
function LiveBookSplit() {
  const { state } = useDesk();
  const q0 = { bid: 0, ask: 0 };
  const by = { A: { n: 0, lots: 0, pnl: 0 }, B: { n: 0, lots: 0, pnl: 0 } };
  const netA = new Map<string, number>();
  for (const p of state.positions) {
    const b = by[p.route];
    b.n++;
    b.lots += p.volume;
    b.pnl += positionPnl(p, q0);
    if (p.route === "A") {
      const px = currentPriceOf(p, q0) || p.openPrice;
      netA.set(p.symbol, (netA.get(p.symbol) ?? 0) + (p.side === "buy" ? 1 : -1) * deskNotional(p.symbol, p.volume, px));
    }
  }
  const total = by.A.lots + by.B.lots;
  const aPct = total ? (by.A.lots / total) * 100 : 0;
  const top = [...netA.entries()].sort((a, b) => Math.abs(b[1]) - Math.abs(a[1])).slice(0, 6);
  return (
    <Card>
      <CardHeader title="Open book split" subtitle="Open positions by book · live from the engine" />
      <div className="px-6 pb-6 pt-4">
        <div className="flex items-end justify-between">
          <div>
            <div className="text-[11px] uppercase tracking-wider text-info">A-book</div>
            <div className="k-num text-[26px] font-semibold">{aPct.toFixed(1)}%</div>
          </div>
          <div className="text-right">
            <div className="text-[11px] uppercase tracking-wider text-fg-2">B-book</div>
            <div className="k-num text-[26px] font-semibold">{total ? (100 - aPct).toFixed(1) : "0.0"}%</div>
          </div>
        </div>
        <ShareBar className="mt-3" height={10} parts={[{ value: aPct, className: "bg-info" }, { value: total ? 100 - aPct : 100, className: "bg-gradient-to-r from-ember to-[#ff8a3d]" }]} />
        <div className="k-num mt-2 flex justify-between font-mono text-[11.5px] text-fg-3">
          <span>{by.A.n} pos · {formatNumber(by.A.lots, 2)} lots</span>
          <span>{by.B.n} pos · {formatNumber(by.B.lots, 2)} lots</span>
        </div>
        <div className="mt-4 grid grid-cols-2 gap-2">
          <div className="k-row px-3 py-2.5">
            <div className="text-[10.5px] uppercase tracking-wider text-fg-3">Client floating on A</div>
            <div className={cn("k-num font-mono text-[14px]", by.A.pnl >= 0 ? "text-up" : "text-down")}>{formatMoney(by.A.pnl, "USD", 0)}</div>
          </div>
          <div className="k-row px-3 py-2.5">
            <div className="text-[10.5px] uppercase tracking-wider text-fg-3">Broker B-book floating</div>
            <div className={cn("k-num font-mono text-[14px]", -by.B.pnl >= 0 ? "text-up" : "text-down")}>{formatMoney(-by.B.pnl, "USD", 0)}</div>
          </div>
        </div>
        <div className="mt-4 text-[11px] uppercase tracking-wider text-fg-3">Net A-book exposure by symbol</div>
        <div className="mt-2 space-y-2">
          {top.length === 0 && <div className="text-[12px] text-fg-3">No A-book positions open.</div>}
          {top.map(([sym, v]) => (
            <div key={sym} className="flex items-center gap-2.5 text-[12px]">
              <SymbolAvatar symbol={sym} size={18} />
              <span className="w-16">{sym}</span>
              <span className={cn("k-num flex-1 text-right font-mono", v >= 0 ? "text-up" : "text-down")}>{usdCompact(v, 1)}</span>
            </div>
          ))}
        </div>
      </div>
    </Card>
  );
}
