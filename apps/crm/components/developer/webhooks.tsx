"use client";

import * as React from "react";
import { AnimatePresence, motion } from "motion/react";
import { toast } from "sonner";
import { Eye, EyeOff, MoreHorizontal, Pencil, Plus, RefreshCw, Send, Trash2, Webhook, X, Zap } from "lucide-react";
import {
  Button,
  Card,
  CardHeader,
  Chip,
  CopyButton,
  DataTable,
  Dialog,
  Field,
  IconButton,
  Input,
  Menu,
  Segmented,
  SymbolAvatar,
  Toggle,
  Tooltip,
  cn,
  formatDateTime,
  type Column,
} from "@/components/kit";
import { ACCOUNTS, getInstrument } from "@kalks/mock";
import { type FanoutTarget, type SignalWebhook, type SizingMode, type WebhookDelivery } from "@kalks/mock/developer";
import { CodeBlock, toJson } from "./code-block";
import { ago } from "./api-keys";

export type Delivery = Omit<WebhookDelivery, "status"> & { status: WebhookDelivery["status"] | 423 };

const SIZING: { value: SizingMode; label: string; suffix: string; step: number; hint: string }[] = [
  { value: "fixed", label: "Fixed lot", suffix: "lot", step: 0.01, hint: "Always trade this volume" },
  { value: "multiplier", label: "Multiplier", suffix: "×", step: 0.1, hint: "Signal volume × value" },
  { value: "risk", label: "Risk %", suffix: "%", step: 0.1, hint: "% of equity at risk to SL" },
];

function acct(login: string) {
  return ACCOUNTS.find((a) => a.login === login)!;
}

/** Resolved lot size on an account for a sample signal (1.00 lot, SL distance given in price units). */
export function resolveLots(t: FanoutTarget, symbol: string, signalVol = 1, slDist?: number) {
  if (t.mode === "fixed") return t.value;
  if (t.mode === "multiplier") return +(signalVol * t.value).toFixed(2);
  const a = acct(t.login);
  const inst = getInstrument(symbol);
  const eq = a.cent ? a.equity / 100 : a.equity;
  const dist = slDist ?? inst.price * 0.006;
  const riskUsd = (eq * t.value) / 100;
  return Math.max(0.01, +(riskUsd / (dist * inst.contractSize)).toFixed(2));
}

/* ------------------------------------------------------------------ */
/* Webhook list item                                                   */
/* ------------------------------------------------------------------ */

export function WebhookListItem({ w, active, onSelect, onToggle }: { w: SignalWebhook; active: boolean; onSelect: () => void; onToggle: (v: boolean) => void }) {
  return (
    <motion.div
      layout
      onClick={onSelect}
      className={cn(
        "k-row relative cursor-pointer overflow-hidden px-4 py-3.5 transition-colors",
        active ? "border-ember/45 bg-ember-soft/60 shadow-[0_0_0_1px_color-mix(in_oklab,var(--k-ember)_15%,transparent),0_12px_30px_-18px_color-mix(in_oklab,var(--k-ember)_60%,transparent)]" : "hover:bg-surface-3/60",
      )}
    >
      {active && <motion.span layoutId="wh-active" className="absolute inset-y-3 left-0 w-[3px] rounded-r-full bg-ember" />}
      <div className="flex items-start gap-3">
        <div className="flex -space-x-2 pt-0.5">
          {w.symbols.slice(0, 2).map((s) => (
            <span key={s} className="rounded-full ring-2 ring-surface-2">
              <SymbolAvatar symbol={s} size={28} />
            </span>
          ))}
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="truncate text-[14px] font-medium">{w.name}</span>
          </div>
          <div className="mt-0.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-[11.5px] text-fg-3">
            <Chip size="sm" tone={w.source === "TradingView" ? "info" : "neutral"}>
              {w.source}
            </Chip>
            <span className="font-mono">{w.symbols.join(" · ")}</span>
          </div>
        </div>
        <div onClick={(e) => e.stopPropagation()}>
          <Toggle checked={w.enabled} onChange={onToggle} label={`Enable ${w.name}`} />
        </div>
      </div>
      <div className="mt-3 grid grid-cols-4 gap-2 border-t border-line pt-3 text-[11px]">
        <div>
          <div className="text-fg-3">Signals 24h</div>
          <div className="k-num mt-0.5 text-[13px] font-medium">{w.signals24h}</div>
        </div>
        <div>
          <div className="text-fg-3">Success</div>
          <div className={cn("k-num mt-0.5 text-[13px] font-medium", w.successRate >= 97 ? "text-up" : "text-warn")}>{w.successRate}%</div>
        </div>
        <div>
          <div className="text-fg-3">Accounts</div>
          <div className="k-num mt-0.5 text-[13px] font-medium">
            {w.targets.filter((t) => t.enabled).length}
            <span className="text-fg-3">/{w.targets.length}</span>
          </div>
        </div>
        <div className="text-right">
          <div className="text-fg-3">Last</div>
          <div className="k-num mt-0.5 text-[13px] font-medium text-fg-2">{ago(w.lastSignal)}</div>
        </div>
      </div>
    </motion.div>
  );
}

/* ------------------------------------------------------------------ */
/* Detail: endpoint + secret                                           */
/* ------------------------------------------------------------------ */

export function EndpointRows({ w, onRotate }: { w: SignalWebhook; onRotate: () => void }) {
  const [reveal, setReveal] = React.useState(false);
  React.useEffect(() => setReveal(false), [w.id]);
  return (
    <div className="space-y-2">
      <div className="k-row flex items-center gap-3 px-4 py-2.5">
        <Chip size="sm" tone="up">
          POST
        </Chip>
        <span className="min-w-0 flex-1 truncate font-mono text-[12.5px]">{w.url}</span>
        <CopyButton value={w.url} label="Webhook URL" />
      </div>
      <div className="k-row flex items-center gap-3 px-4 py-2.5">
        <span className="w-[42px] shrink-0 text-[10.5px] font-medium uppercase tracking-wider text-fg-3">Secret</span>
        <span className="min-w-0 flex-1 truncate font-mono text-[12.5px]">
          {reveal ? <span className="text-ember">{w.secret}</span> : <span className="text-fg-2">{w.secret.slice(0, 6)}{"•".repeat(18)}{w.secret.slice(-3)}</span>}
        </span>
        <Tooltip content={reveal ? "Hide secret" : "Reveal secret"}>
          <button type="button" onClick={() => setReveal((r) => !r)} className="grid size-6 place-items-center rounded-md text-fg-3 hover:bg-surface-3 hover:text-fg" aria-label="Reveal secret">
            {reveal ? <EyeOff className="size-3.5" /> : <Eye className="size-3.5" />}
          </button>
        </Tooltip>
        <CopyButton value={w.secret} label="Webhook secret" />
        <Tooltip content="Rotate secret">
          <button type="button" onClick={onRotate} className="grid size-6 place-items-center rounded-md text-fg-3 hover:bg-surface-3 hover:text-fg" aria-label="Rotate secret">
            <RefreshCw className="size-3.5" />
          </button>
        </Tooltip>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Fan-out editor                                                      */
/* ------------------------------------------------------------------ */

export function FanoutEditor({ w, onChange }: { w: SignalWebhook; onChange: (targets: FanoutTarget[]) => void }) {
  const [draft, setDraft] = React.useState<FanoutTarget[]>(w.targets);
  React.useEffect(() => setDraft(w.targets), [w.id, w.targets]);
  const dirty = JSON.stringify(draft) !== JSON.stringify(w.targets);
  const sym = w.symbols[0]!;
  const available = ACCOUNTS.filter((a) => !draft.some((t) => t.login === a.login));
  const update = (i: number, patch: Partial<FanoutTarget>) => setDraft((d) => d.map((t, j) => (j === i ? { ...t, ...patch } : t)));

  return (
    <div>
      <div className="mb-2.5 flex flex-wrap items-center justify-between gap-2">
        <div>
          <div className="k-label">Fan-out targets</div>
          <div className="mt-0.5 text-[11.5px] text-fg-3">
            Preview for a <span className="font-mono text-fg-2">buy 1.00 {sym}</span> signal
          </div>
        </div>
        <div className="flex items-center gap-2">
          <AnimatePresence>
            {dirty && (
              <motion.div initial={{ opacity: 0, x: 8 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: 8 }} className="flex items-center gap-2">
                <Button size="xs" variant="ghost" onClick={() => setDraft(w.targets)}>
                  Discard
                </Button>
                <Button
                  size="xs"
                  variant="ember"
                  onClick={() => {
                    onChange(draft);
                    toast.success("Fan-out saved", { description: `${draft.filter((t) => t.enabled).length} accounts will receive ${w.name} signals.` });
                  }}
                >
                  Save changes
                </Button>
              </motion.div>
            )}
          </AnimatePresence>
          <Menu
            width={250}
            trigger={
              <Button size="xs" variant="surface" disabled={!available.length}>
                <Plus /> Add account
              </Button>
            }
            items={available.map((a) => ({
              label: (
                <span className="flex items-center gap-2">
                  <span className="font-mono">{a.login}</span>
                  <span className="text-[11px] text-fg-3">
                    {a.type === "live" ? "Live" : "Demo"} · {a.group}
                  </span>
                </span>
              ),
              onSelect: () => setDraft((d) => [...d, { login: a.login, mode: "fixed", value: 0.1, enabled: true }]),
            }))}
          />
        </div>
      </div>
      <div className="space-y-2">
        <AnimatePresence initial={false}>
          {draft.map((t, i) => {
            const a = acct(t.login);
            const s = SIZING.find((x) => x.value === t.mode)!;
            const lots = resolveLots(t, sym);
            return (
              <motion.div
                key={t.login}
                layout
                initial={{ opacity: 0, height: 0 }}
                animate={{ opacity: 1, height: "auto" }}
                exit={{ opacity: 0, height: 0 }}
                className={cn("k-row overflow-hidden", !t.enabled && "opacity-60")}
              >
                <div className="flex flex-wrap items-center gap-x-3 gap-y-2.5 px-3.5 py-3">
                  <div className="flex min-w-[130px] flex-1 items-center gap-2.5">
                    <Toggle checked={t.enabled} onChange={(v) => update(i, { enabled: v })} label={`Enable ${t.login}`} />
                    <div className="min-w-0">
                      <div className="font-mono text-[13px]">{t.login}</div>
                      <div className={cn("text-[10.5px]", a.type === "live" ? "text-ember" : "text-gold")}>
                        {a.type === "live" ? "Live" : "Demo"} · {a.group}
                        {a.cent ? " · USC" : ""}
                      </div>
                    </div>
                  </div>
                  <Segmented size="xs" value={t.mode} onChange={(v) => update(i, { mode: v, value: v === "fixed" ? 0.1 : v === "multiplier" ? 1 : 1 })} options={SIZING.map((x) => ({ value: x.value, label: x.label }))} />
                  <div className="flex h-8 w-[104px] items-center rounded-full border border-line bg-surface px-3 focus-within:border-ember/50">
                    <input
                      type="number"
                      step={s.step}
                      min={s.step}
                      value={t.value}
                      onChange={(e) => update(i, { value: Number(e.target.value) })}
                      className="k-num w-full min-w-0 bg-transparent text-[13px] outline-none [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none"
                      aria-label={`${s.label} for ${t.login}`}
                    />
                    <span className="text-[11px] text-fg-3">{s.suffix}</span>
                  </div>
                  <Tooltip content={s.hint}>
                    <div className="w-[76px] text-right">
                      <div className="text-[11px] text-fg-3">Resolved</div>
                      <div className="k-num text-[13px] font-medium text-gold">{t.enabled ? `${lots.toFixed(2)} lot` : "—"}</div>
                    </div>
                  </Tooltip>
                  <button
                    type="button"
                    onClick={() => setDraft((d) => d.filter((_, j) => j !== i))}
                    className="grid size-7 place-items-center rounded-full text-fg-3 hover:bg-down-soft hover:text-down"
                    aria-label={`Remove ${t.login}`}
                  >
                    <X className="size-3.5" />
                  </button>
                </div>
              </motion.div>
            );
          })}
        </AnimatePresence>
        {draft.length === 0 && <div className="rounded-[14px] border border-dashed border-line py-6 text-center text-[13px] text-fg-3">No target accounts — signals will be accepted but not traded.</div>}
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Payload                                                             */
/* ------------------------------------------------------------------ */

export function payloadFor(w: SignalWebhook, mode: "template" | "resolved") {
  const sym = w.symbols[0]!;
  const inst = getInstrument(sym);
  const p = inst.price;
  const d = inst.digits;
  if (mode === "template")
    return toJson({
      secret: w.secret.slice(0, 10) + "…",
      action: "{{strategy.order.action}}",
      symbol: sym,
      volume: "{{strategy.order.contracts}}",
      price: "{{close}}",
      sl: "{{plot_0}}",
      tp: "{{plot_1}}",
      comment: w.name.toLowerCase().replace(/[^a-z0-9]+/g, "-").slice(0, 24),
      time: "{{timenow}}",
    });
  return toJson({
    secret: w.secret.slice(0, 10) + "…",
    action: "buy",
    symbol: sym,
    volume: 1.0,
    price: +p.toFixed(d),
    sl: +(p * 0.994).toFixed(d),
    tp: +(p * 1.012).toFixed(d),
    comment: w.name.toLowerCase().replace(/[^a-z0-9]+/g, "-").slice(0, 24),
    time: "2026-09-24T14:52:08Z",
  });
}

export function PayloadCard({ w }: { w: SignalWebhook }) {
  const [mode, setMode] = React.useState<"template" | "resolved">("template");
  return (
    <CodeBlock
      lang="json"
      title={mode === "template" ? "TradingView alert message" : "Received payload"}
      code={payloadFor(w, mode)}
      highlight={[3, 4]}
      tabs={<Segmented size="xs" value={mode} onChange={setMode} options={[{ value: "template", label: "Template" }, { value: "resolved", label: "Example" }]} />}
    />
  );
}

/* ------------------------------------------------------------------ */
/* Delivery log                                                        */
/* ------------------------------------------------------------------ */

const STATUS_TONE: Record<Delivery["status"], "up" | "warn" | "down"> = { 200: "up", 422: "warn", 401: "down", 429: "warn", 423: "down" };

export function DeliveryLog({ rows, webhooks, selectedId }: { rows: Delivery[]; webhooks: SignalWebhook[]; selectedId: string }) {
  const [scope, setScope] = React.useState<"this" | "all">("all");
  const [status, setStatus] = React.useState<"any" | "ok" | "err">("any");
  const [open, setOpen] = React.useState<Delivery | null>(null);
  const name = (id: string) => webhooks.find((w) => w.id === id)?.name ?? id;
  const view = rows.filter((r) => (scope === "all" || r.webhookId === selectedId) && (status === "any" || (status === "ok" ? r.status === 200 : r.status !== 200)));

  const columns: Column<Delivery>[] = [
    {
      key: "time",
      header: "Time (GMT+3)",
      sort: (r) => r.time,
      cell: (r) => (
        <div className="whitespace-nowrap">
          <div className="k-num font-mono text-[12.5px]">{formatDateTime(r.time, { hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false })}</div>
          <div className="text-[10.5px] text-fg-3">{formatDateTime(r.time, { day: "2-digit", month: "short" })}</div>
        </div>
      ),
    },
    { key: "wh", header: "Webhook", hideOn: "md", cell: (r) => <span className="block max-w-[180px] truncate text-fg-2">{name(r.webhookId)}</span> },
    {
      key: "action",
      header: "Action",
      cell: (r) => (
        <Chip size="sm" tone={r.action === "buy" ? "up" : r.action === "sell" ? "down" : "neutral"}>
          {r.action.toUpperCase()}
        </Chip>
      ),
    },
    {
      key: "sym",
      header: "Symbol",
      cell: (r) => (
        <span className="inline-flex items-center gap-2">
          <SymbolAvatar symbol={r.symbol} size={20} />
          <span className="font-medium">{r.symbol}</span>
        </span>
      ),
    },
    {
      key: "status",
      header: "Status",
      sort: (r) => r.status,
      cell: (r) => (
        <Chip size="sm" tone={STATUS_TONE[r.status]} dot>
          <span className="font-mono">{r.status}</span>
        </Chip>
      ),
    },
    { key: "lat", header: "Latency", align: "right", sort: (r) => r.latency, cell: (r) => <span className={cn("k-num", r.latency > 60 ? "text-warn" : "text-fg-2")}>{r.latency} ms</span> },
    {
      key: "fill",
      header: "Accounts filled",
      align: "right",
      cell: (r) => (
        <div className="inline-flex items-center gap-2">
          <div className="flex gap-0.5">
            {Array.from({ length: r.total }, (_, i) => (
              <span key={i} className={cn("h-3 w-1.5 rounded-full", i < r.filled ? "bg-up" : "bg-surface-3")} />
            ))}
          </div>
          <span className={cn("k-num w-8 text-right", r.filled === r.total ? "text-fg" : r.filled === 0 ? "text-down" : "text-warn")}>
            {r.filled}/{r.total}
          </span>
        </div>
      ),
    },
    { key: "msg", header: "Response", hideOn: "lg", cell: (r) => <span className="block max-w-[260px] truncate text-[12.5px] text-fg-3">{r.message}</span> },
  ];

  return (
    <Card>
      <CardHeader title="Delivery log" subtitle="Every signal, its response and how many accounts filled · click a row for details" icon={<Send />} />
      <div className="px-4 pb-5 pt-4 sm:px-6">
        <DataTable
          columns={columns}
          rows={view}
          rowKey={(r) => r.id}
          pageSize={10}
          dense
          onRowClick={setOpen}
          search={(r) => `${r.symbol} ${r.action} ${r.status} ${name(r.webhookId)} ${r.message}`}
          searchPlaceholder="Search symbol, status…"
          exportName="webhook-deliveries"
          toolbar={
            <div className="flex flex-wrap items-center gap-2">
              <Segmented size="xs" value={scope} onChange={setScope} options={[{ value: "all", label: "All webhooks" }, { value: "this", label: "Selected" }]} />
              <Segmented size="xs" value={status} onChange={setStatus} options={[{ value: "any", label: "Any" }, { value: "ok", label: "2xx" }, { value: "err", label: "Errors" }]} />
            </div>
          }
        />
      </div>
      <Dialog open={!!open} onOpenChange={(o) => !o && setOpen(null)} side="right" title={open ? `Delivery ${open.id}` : ""} description={open ? `${name(open.webhookId)} · ${formatDateTime(open.time, { day: "2-digit", month: "short", hour: "2-digit", minute: "2-digit", second: "2-digit" })}` : ""}>
        {open && <DeliveryDetail d={open} w={webhooks.find((w) => w.id === open.webhookId)} />}
      </Dialog>
    </Card>
  );
}

function DeliveryDetail({ d, w }: { d: Delivery; w?: SignalWebhook }) {
  const inst = getInstrument(d.symbol);
  const targets = (w?.targets ?? []).filter((t) => t.enabled).slice(0, d.total);
  const response =
    d.status === 200
      ? { status: "accepted", signal_id: d.id, orders: targets.slice(0, d.filled).map((t, i) => ({ login: t.login, ticket: 51298844 + i * 17, volume: resolveLots(t, d.symbol), source: "webhook" })) }
      : { error: d.status === 422 ? "invalid_order" : d.status === 401 ? "unauthorized" : d.status === 423 ? "kill_switch_active" : "rate_limited", message: d.message };
  return (
    <div className="space-y-5">
      <div className="grid grid-cols-3 gap-2">
        {[
          ["Status", <span key="s" className={cn("font-mono", d.status === 200 ? "text-up" : "text-down")}>{d.status}</span>],
          ["Latency", `${d.latency} ms`],
          ["Filled", `${d.filled}/${d.total}`],
        ].map(([k, v], i) => (
          <div key={i} className="k-row px-3 py-2.5">
            <div className="text-[11.5px] text-fg-3">{k}</div>
            <div className="k-num mt-0.5 text-[15px] font-medium">{v}</div>
          </div>
        ))}
      </div>
      <div>
        <div className="k-label mb-2">Request</div>
        <CodeBlock
          lang="json"
          title="POST /v1/signal"
          code={toJson({ action: d.action, symbol: d.symbol, volume: 1.0, price: +inst.price.toFixed(inst.digits), comment: w?.name.toLowerCase().replace(/[^a-z0-9]+/g, "-").slice(0, 24) })}
        />
      </div>
      <div>
        <div className="k-label mb-2">Response</div>
        <CodeBlock lang="json" title={`HTTP ${d.status}`} code={toJson(response)} />
      </div>
      {d.status !== 200 && (
        <Button variant="surface" className="w-full" onClick={() => toast.success("Delivery replayed", { description: `${d.action.toUpperCase()} ${d.symbol} re-queued to ${d.total} accounts` })}>
          <RefreshCw /> Replay signal
        </Button>
      )}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Create webhook                                                      */
/* ------------------------------------------------------------------ */

const PICK_SYMBOLS = ["XAUUSD", "EURUSD", "GBPUSD", "USDJPY", "NAS100", "US30", "GER40", "BTCUSD", "ETHUSD", "USOIL"];

export function CreateWebhookDialog({ open, onOpenChange, onCreate }: { open: boolean; onOpenChange: (o: boolean) => void; onCreate: (w: SignalWebhook) => void }) {
  const [name, setName] = React.useState("BTC 4H trend");
  const [source, setSource] = React.useState<"TradingView" | "Custom">("TradingView");
  const [symbols, setSymbols] = React.useState<string[]>(["BTCUSD"]);
  const [targets, setTargets] = React.useState<string[]>(["80412337"]);
  const [mode, setMode] = React.useState<SizingMode>("risk");

  const create = () => {
    if (!name.trim()) return toast.error("Name your webhook");
    if (!symbols.length) return toast.error("Pick at least one symbol");
    const id = `wh_${Array.from(crypto.getRandomValues(new Uint8Array(3)), (b) => b.toString(16).padStart(2, "0")).join("")}`;
    const secret = `whsec_${Array.from(crypto.getRandomValues(new Uint8Array(12)), (b) => "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnpqrstuvwxyz23456789"[b % 56]).join("")}`;
    onCreate({
      id,
      name: name.trim(),
      source,
      symbols,
      url: `https://api.kalkstrade.com/algo/hooks/${id}${Array.from(crypto.getRandomValues(new Uint8Array(2)), (b) => b.toString(16).padStart(2, "0")).join("")}`,
      secret,
      enabled: true,
      createdAt: new Date().toISOString(),
      lastSignal: new Date().toISOString(),
      signals24h: 0,
      successRate: 100,
      avgLatency: 0,
      targets: targets.map((login) => ({ login, mode, value: mode === "fixed" ? 0.1 : mode === "multiplier" ? 1 : 1, enabled: true })),
    });
    onOpenChange(false);
    toast.success("Webhook created", { description: "Paste the URL and message template into your TradingView alert." });
  };

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      width={580}
      title="Create signal webhook"
      description="Each webhook gets a unique URL and signing secret. Orders are tagged source=webhook."
      footer={
        <>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button variant="ember" onClick={create}>
            <Webhook /> Create webhook
          </Button>
        </>
      }
    >
      <div className="space-y-5">
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-[1fr_auto]">
          <Field label="Name">
            <Input value={name} onChange={(e) => setName(e.target.value)} leading={<Pencil />} />
          </Field>
          <div>
            <div className="mb-1.5 text-[12.5px] font-medium text-fg-2">Source</div>
            <Segmented size="md" value={source} onChange={setSource} options={["TradingView", "Custom"] as const} />
          </div>
        </div>
        <div>
          <div className="mb-1.5 flex justify-between text-[12.5px] font-medium text-fg-2">
            Allowed symbols <span className="font-normal text-fg-3">Signals for other symbols return 422</span>
          </div>
          <div className="flex flex-wrap gap-1.5">
            {PICK_SYMBOLS.map((s) => {
              const on = symbols.includes(s);
              return (
                <button
                  key={s}
                  type="button"
                  onClick={() => setSymbols(on ? symbols.filter((x) => x !== s) : [...symbols, s])}
                  className={cn("inline-flex h-8 items-center gap-1.5 rounded-full border pl-1 pr-3 text-[12.5px] font-medium transition-colors", on ? "border-ember/40 bg-ember-soft text-fg" : "border-line bg-surface-2 text-fg-2 hover:bg-surface-3")}
                >
                  <SymbolAvatar symbol={s} size={22} />
                  {s}
                </button>
              );
            })}
          </div>
        </div>
        <div>
          <div className="mb-1.5 text-[12.5px] font-medium text-fg-2">Fan out to accounts</div>
          <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
            {ACCOUNTS.map((a) => {
              const on = targets.includes(a.login);
              return (
                <button
                  key={a.login}
                  type="button"
                  onClick={() => setTargets(on ? targets.filter((x) => x !== a.login) : [...targets, a.login])}
                  className={cn("k-row flex items-center gap-3 px-3.5 py-2.5 text-left transition-colors", on ? "border-ember/45 bg-ember-soft" : "hover:bg-surface-3")}
                >
                  <span className={cn("grid size-4 place-items-center rounded-[5px] border", on ? "border-ember bg-ember text-white" : "border-fg-3")}>
                    {on && (
                      <svg viewBox="0 0 12 12" className="size-2.5 fill-none stroke-current stroke-2">
                        <path d="M2.5 6.2l2.2 2.2 4.8-4.8" />
                      </svg>
                    )}
                  </span>
                  <span className="flex-1 font-mono text-[13px]">{a.login}</span>
                  <Chip size="sm" tone={a.type === "live" ? "ember" : "gold"}>
                    {a.type === "live" ? a.group : `Demo ${a.group}`}
                  </Chip>
                </button>
              );
            })}
          </div>
        </div>
        <div>
          <div className="mb-1.5 text-[12.5px] font-medium text-fg-2">Default sizing rule</div>
          <div className="grid grid-cols-3 gap-2">
            {SIZING.map((s) => (
              <button
                key={s.value}
                type="button"
                onClick={() => setMode(s.value)}
                className={cn("k-row px-3 py-2.5 text-left transition-colors", mode === s.value ? "border-ember/45 bg-ember-soft" : "hover:bg-surface-3")}
              >
                <div className="text-[13px] font-medium">{s.label}</div>
                <div className="mt-0.5 text-[11px] leading-tight text-fg-3">{s.hint}</div>
              </button>
            ))}
          </div>
          <p className="mt-2 text-[11.5px] text-fg-3">You can fine-tune the size per account after creating.</p>
        </div>
      </div>
    </Dialog>
  );
}

export function WebhookMenu({ w, onRename, onDelete }: { w: SignalWebhook; onRename: () => void; onDelete: () => void }) {
  return (
    <Menu
      width={210}
      trigger={
        <IconButton size="sm" aria-label="Webhook actions">
          <MoreHorizontal />
        </IconButton>
      }
      items={[
        { label: "Rename", icon: <Pencil />, onSelect: onRename },
        { label: "Duplicate", icon: <Plus />, onSelect: () => toast.success(`${w.name} duplicated`, { description: "New URL and secret issued." }) },
        { label: "Test with TradingView", icon: <Zap />, onSelect: () => toast.info("Create an alert in TradingView → Notifications → Webhook URL", { description: w.url }) },
        "sep",
        { label: "Delete webhook", icon: <Trash2 />, danger: true, onSelect: onDelete },
      ]}
    />
  );
}
