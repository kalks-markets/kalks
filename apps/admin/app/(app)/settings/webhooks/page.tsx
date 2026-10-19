"use client";

import * as React from "react";
import { toast } from "sonner";
import { Activity, BookOpen, CheckCircle2, MoreHorizontal, Pause, Play, Plus, RefreshCw, RotateCw, Send, Trash2, Webhook, XCircle } from "lucide-react";
import { Button, Card, CardHeader, Chip, Dialog, Field, IconButton, Input, KpiCard, Menu, PageHeader, Progress, Reveal, Segmented, Sparkline, cn, formatDateTime, formatNumber } from "@kalks/ui";
import { SET_DELIVERIES, SET_WEBHOOK_EVENTS, SET_WEBHOOKS, type SetDelivery, type SetWebhook } from "@kalks/mock/admin-platform-settings";
import { SecretInput } from "@/components/settings/kit";

const STATUS_TONE = { active: "up", paused: "neutral", failing: "down" } as const;

function codeTone(c: number) {
  return c < 300 ? "up" : c === 429 ? "warn" : "down";
}

function payloadFor(d: SetDelivery) {
  const base: Record<string, unknown> = { id: `evt_${d.id.slice(4)}`, type: d.event, created_at: d.at, tenant: "kalks" };
  const data: Record<string, unknown> = d.event.startsWith("deposit")
    ? { client_id: "u_1004", login: "80412337", amount: "2500.00", currency: "USDT", network: "TRC20", tx_hash: "9f2c41e7…b18d" }
    : d.event.startsWith("withdrawal")
      ? { client_id: "u_1011", amount: "1200.00", currency: "USDT", address: "TQ7xH2m9…A59KfE", status: d.event.split(".")[1] }
      : d.event.startsWith("trade")
        ? { login: "80412337", ticket: 51837724, symbol: "XAUUSD", side: "buy", volume: 0.5, price: 2654.3 }
        : d.event.startsWith("ib")
          ? { partner_id: "ib_2204", client_id: "u_1016", amount: "184.20" }
          : { client_id: "u_1002", email: "lucas.ferreira@mail.com", country: "br" };
  return JSON.stringify({ ...base, data }, null, 2);
}

function spark(seed: number, failing: boolean) {
  return Array.from({ length: 24 }, (_, i) => {
    const v = 60 + Math.sin(i * 0.7 + seed) * 18 + ((i * seed) % 7) * 3;
    return failing && i > 16 ? v * 0.3 : v;
  });
}

export default function WebhooksPage() {
  const [hooks, setHooks] = React.useState<SetWebhook[]>(SET_WEBHOOKS);
  const [sel, setSel] = React.useState(SET_WEBHOOKS[0]!.id);
  const [filter, setFilter] = React.useState<"all" | "failed">("all");
  const [open, setOpen] = React.useState(false);
  const [payload, setPayload] = React.useState<SetDelivery | null>(null);
  const [name, setName] = React.useState("");
  const [url, setUrl] = React.useState("");
  const [events, setEvents] = React.useState<string[]>(["deposit.completed", "withdrawal.approved"]);
  const [retried, setRetried] = React.useState<Record<string, boolean>>({});

  const current = hooks.find((h) => h.id === sel) ?? hooks[0]!;
  const deliveries = SET_DELIVERIES.filter((d) => d.webhookId === current.id && (filter === "all" || d.code >= 300));
  const total24 = hooks.reduce((s, h) => s + h.deliveries24h, 0);

  const patch = (id: string, p: Partial<SetWebhook>) => setHooks((hs) => hs.map((h) => (h.id === id ? { ...h, ...p } : h)));

  return (
    <div className="pb-16">
      <PageHeader
        title="Webhooks"
        subtitle="Signed HTTP callbacks to your CRM, data warehouse and partners. Payloads are signed with HMAC-SHA256 in the X-Kalks-Signature header."
        actions={
          <>
            <Button variant="surface" onClick={() => toast("Opening webhook reference · api.kalkstrade.com/docs/webhooks")}>
              <BookOpen /> Event reference
            </Button>
            <Button variant="ember" onClick={() => setOpen(true)}>
              <Plus /> Add endpoint
            </Button>
          </>
        }
      />

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <KpiCard label="Endpoints" icon={<Webhook />} value={<span className="k-num">{hooks.length}</span>} chip={`${hooks.filter((h) => h.status === "active").length} active`} chipTone="up" />
        <KpiCard label="Deliveries · 24h" icon={<Activity />} value={<span className="k-num">{formatNumber(total24, 0)}</span>} chip="p95 312 ms" delay={0.05} />
        <KpiCard label="Success rate" icon={<CheckCircle2 />} value={<span className="k-num">99.41%</span>} chip="auto-retry ×5, exp. backoff" chipTone="up" delay={0.1} />
        <KpiCard label="Failing endpoints" icon={<XCircle />} value={<span className="k-num text-down">{hooks.filter((h) => h.status === "failing").length}</span>} chip="Aurum partners · 5xx" chipTone="down" delay={0.15} />
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal className="xl:col-span-7">
          <Card className="h-full">
            <CardHeader title="Endpoints" subtitle="Select an endpoint to inspect its deliveries" />
            <div className="mt-4 space-y-2.5 px-4 pb-6 sm:px-6">
              {hooks.map((h) => (
                <div
                  key={h.id}
                  role="button"
                  tabIndex={0}
                  onClick={() => setSel(h.id)}
                  onKeyDown={(e) => e.key === "Enter" && setSel(h.id)}
                  className={cn("k-row cursor-pointer p-4 transition-colors hover:bg-surface-3/60", sel === h.id && "border-ember/35 bg-ember-soft/40 shadow-[0_0_0_1px_rgba(255,90,31,0.15)]")}
                >
                  <div className="flex items-start gap-3">
                    <span className={cn("mt-1.5 size-2 shrink-0 rounded-full", h.status === "active" ? "bg-up shadow-[0_0_10px_var(--k-up)]" : h.status === "failing" ? "animate-pulse bg-down" : "bg-fg-3")} />
                    <div className="min-w-0 flex-1">
                      <div className="flex flex-wrap items-center gap-2">
                        <span className="text-[14px] font-medium">{h.name}</span>
                        <Chip size="sm" tone={STATUS_TONE[h.status]}>
                          {h.status}
                        </Chip>
                      </div>
                      <div className="mt-0.5 truncate font-mono text-[12px] text-fg-3">{h.url}</div>
                      <div className="mt-2 flex flex-wrap gap-1">
                        {h.events.slice(0, 3).map((e) => (
                          <span key={e} className="rounded-md bg-surface-3 px-1.5 py-0.5 font-mono text-[10.5px] text-fg-2">
                            {e}
                          </span>
                        ))}
                        {h.events.length > 3 && <span className="rounded-md bg-surface-3 px-1.5 py-0.5 text-[10.5px] text-fg-3">+{h.events.length - 3}</span>}
                      </div>
                    </div>
                    <Sparkline data={spark(h.id.charCodeAt(4), h.status === "failing")} width={80} height={30} tone={h.status === "failing" ? "down" : h.status === "paused" ? "gold" : "up"} className="hidden sm:block" />
                    <div className="hidden w-24 text-right sm:block">
                      <div className={cn("k-num text-[14px] font-semibold", h.successRate < 95 ? "text-down" : "text-fg")}>{h.successRate}%</div>
                      <div className="k-num text-[11px] text-fg-3">{formatNumber(h.deliveries24h, 0)} · {h.avgMs} ms</div>
                    </div>
                    <div onClick={(e) => e.stopPropagation()}>
                      <Menu
                        trigger={
                          <IconButton size="sm" aria-label="Endpoint actions">
                            <MoreHorizontal />
                          </IconButton>
                        }
                        items={[
                          { label: "Send test event", icon: <Send />, onSelect: () => toast.success(`Test event sent to ${h.name}`, { description: h.status === "failing" ? "HTTP 502 Bad Gateway · 2,410 ms" : "HTTP 200 OK · 142 ms" }) },
                          h.status === "paused"
                            ? { label: "Resume", icon: <Play />, onSelect: () => { patch(h.id, { status: "active" }); toast.success(`${h.name} resumed`); } }
                            : { label: "Pause", icon: <Pause />, onSelect: () => { patch(h.id, { status: "paused" }); toast(`${h.name} paused`, { description: "Events are queued for 72h" }); } },
                          { label: "Rotate secret", icon: <RotateCw />, onSelect: () => { patch(h.id, { secret: `whsec_${Math.abs(h.secret.length * 7919).toString(36)}EXAMPLE` }); toast.success("Signing secret rotated", { description: "Old secret stays valid for 24h" }); } },
                          "sep",
                          { label: "Delete endpoint", icon: <Trash2 />, danger: true, onSelect: () => { setHooks((hs) => hs.filter((x) => x.id !== h.id)); toast.success(`${h.name} deleted`); } },
                        ]}
                      />
                    </div>
                  </div>
                  {h.status !== "paused" && (
                    <div className="mt-3 pl-5">
                      <Progress value={h.successRate} tone={h.successRate < 95 ? "down" : "up"} className="h-1" />
                    </div>
                  )}
                </div>
              ))}
            </div>
          </Card>
        </Reveal>

        <Reveal delay={0.05} className="xl:col-span-5">
          <Card className="flex h-full flex-col">
            <CardHeader
              title="Recent deliveries"
              subtitle={current.name}
              action={
                <Segmented
                  size="xs"
                  value={filter}
                  onChange={setFilter}
                  options={[
                    { value: "all", label: "All" },
                    { value: "failed", label: "Failed" },
                  ]}
                />
              }
            />
            <div className="px-4 pt-4 sm:px-6">
              <div className="text-[12px] font-medium text-fg-2">Signing secret</div>
              <div className="mt-1.5">
                <SecretInput key={current.id + current.secret} value={`whsec_${"•".repeat(14)}${current.secret.slice(-4)}`} reveal={current.secret} />
              </div>
            </div>
            <div className="k-fade-bottom mt-4 max-h-[520px] flex-1 space-y-1.5 overflow-y-auto px-4 pb-6 sm:px-6">
              {deliveries.length === 0 && <div className="py-10 text-center text-[13px] text-fg-3">No {filter === "failed" ? "failed " : ""}deliveries in the last 24h.</div>}
              {deliveries.map((d) => {
                const ok = d.code < 300 || retried[d.id];
                return (
                  <div key={d.id} className="flex items-center gap-3 rounded-[12px] px-3 py-2.5 transition-colors hover:bg-surface-2">
                    <Chip size="sm" tone={retried[d.id] ? "up" : codeTone(d.code)} className="w-11 justify-center font-mono">
                      {retried[d.id] ? 200 : d.code}
                    </Chip>
                    <button type="button" onClick={() => setPayload(d)} className="min-w-0 flex-1 text-left">
                      <div className="truncate font-mono text-[12.5px] text-fg hover:text-ember">{d.event}</div>
                      <div className="k-num text-[11px] text-fg-3">
                        {formatDateTime(d.at, { hour: "2-digit", minute: "2-digit", second: "2-digit" })} · {formatNumber(d.ms, 0)} ms{d.attempt > 1 ? ` · attempt ${d.attempt}/5` : ""}
                      </div>
                    </button>
                    {!ok ? (
                      <Button
                        size="xs"
                        variant="surface"
                        onClick={() => {
                          setRetried((r) => ({ ...r, [d.id]: true }));
                          toast.success(`Redelivered ${d.event}`, { description: "HTTP 200 OK · 318 ms" });
                        }}
                      >
                        <RefreshCw /> Retry
                      </Button>
                    ) : (
                      <span className="font-mono text-[10.5px] text-fg-3">{d.id.slice(4, 10)}</span>
                    )}
                  </div>
                );
              })}
            </div>
          </Card>
        </Reveal>
      </div>

      {/* payload */}
      <Dialog open={!!payload} onOpenChange={(o) => !o && setPayload(null)} title={payload?.event ?? ""} description={payload ? `Delivery ${payload.id} · HTTP ${payload.code} · ${payload.ms} ms` : undefined} width={620}>
        {payload && (
          <div className="space-y-3">
            <div className="k-row overflow-x-auto p-4">
              <div className="mb-2 text-[11px] uppercase tracking-wider text-fg-3">Headers</div>
              <pre className="font-mono text-[12px] leading-relaxed text-fg-2">{`POST ${current.url}
Content-Type: application/json
X-Kalks-Event: ${payload.event}
X-Kalks-Signature: t=1790264640,v1=5f1c9e0a7b…d42e
X-Kalks-Delivery: ${payload.id}`}</pre>
            </div>
            <div className="k-row overflow-x-auto p-4">
              <div className="mb-2 text-[11px] uppercase tracking-wider text-fg-3">Body</div>
              <pre className="font-mono text-[12px] leading-relaxed text-fg">{payloadFor(payload)}</pre>
            </div>
          </div>
        )}
      </Dialog>

      {/* create */}
      <Dialog
        open={open}
        onOpenChange={setOpen}
        title="Add webhook endpoint"
        description="We'll POST JSON to this URL and retry up to 5 times with exponential backoff."
        width={640}
        footer={
          <>
            <Button variant="ghost" onClick={() => setOpen(false)}>
              Cancel
            </Button>
            <Button
              variant="ember"
              onClick={() => {
                if (!/^https:\/\/.+\..+/.test(url)) {
                  toast.error("URL must use https://");
                  return;
                }
                if (!events.length) {
                  toast.error("Select at least one event");
                  return;
                }
                const id = `wh_${hooks.length + 10}`;
                setHooks((hs) => [{ id, name: name || new URL(url).host, url, events, status: "active", secret: "whsec_EXAMPLE000000006", successRate: 100, deliveries24h: 0, avgMs: 0, created: "2026-09-24" }, ...hs]);
                setSel(id);
                setOpen(false);
                setName("");
                setUrl("");
                toast.success("Endpoint created", { description: "Signing secret generated · send a test event to verify" });
              }}
            >
              Create endpoint
            </Button>
          </>
        }
      >
        <div className="space-y-4">
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <Field label="Name">
              <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="e.g. Salesforce sync" />
            </Field>
            <Field label="Endpoint URL">
              <Input value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://" inputClassName="font-mono text-[12.5px]" />
            </Field>
          </div>
          <div>
            <div className="mb-2 flex items-center justify-between text-[12.5px] font-medium text-fg-2">
              Events
              <button type="button" className="text-[12px] text-ember" onClick={() => setEvents(events.length ? [] : SET_WEBHOOK_EVENTS.flatMap((g) => g.events))}>
                {events.length ? "Clear" : "Select all"}
              </button>
            </div>
            <div className="space-y-3">
              {SET_WEBHOOK_EVENTS.map((g) => (
                <div key={g.group}>
                  <div className="mb-1.5 text-[11px] uppercase tracking-wider text-fg-3">{g.group}</div>
                  <div className="flex flex-wrap gap-1.5">
                    {g.events.map((e) => {
                      const on = events.includes(e);
                      return (
                        <button
                          key={e}
                          type="button"
                          onClick={() => setEvents((s) => (on ? s.filter((x) => x !== e) : [...s, e]))}
                          className={cn("rounded-full border px-2.5 py-1 font-mono text-[11.5px] transition-colors", on ? "border-ember/40 bg-ember-soft text-ember" : "border-line text-fg-3 hover:text-fg-2")}
                        >
                          {e}
                        </button>
                      );
                    })}
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      </Dialog>
    </div>
  );
}
