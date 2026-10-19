"use client";

import { IS_DEMO } from "@kalks/mock/mode";
import { LiveSystem } from "@/components/owner/platform";

import * as React from "react";
import { AnimatePresence, motion } from "motion/react";
import { toast } from "sonner";
import { AlertOctagon, CheckCircle2, Clock, Construction, ExternalLink, Layers, Megaphone, Play, RotateCw, Trash2, Wrench } from "lucide-react";
import {
  Avatar,
  Button,
  Card,
  CardHeader,
  Chip,
  Dialog,
  DialogClose,
  Field,
  IconButton,
  Input,
  PageHeader,
  Progress,
  Reveal,
  Segmented,
  Toggle,
  Tooltip,
  cn,
  formatDateTime,
  formatNumber,
} from "@kalks/ui";
import { BRK_CRONS, BRK_INCIDENTS, BRK_QUEUES, BRK_SERVICES, BRK_STATUS_COMPONENTS, BRK_TENANTS, type BrkIncident } from "@kalks/mock/admin-platform-brokers";
import { PEOPLE } from "@kalks/mock/people";
import { ConfirmDialog, SectionLabel, Select, TenantLogo, Textarea, timeAgo } from "@/components/brokers/kit";
import { IncidentItem, SVC_LABEL, SVC_TONE, ServiceCard, UptimeBars } from "@/components/brokers/system-widgets";

function SystemOpsPage() {
  const [incidents, setIncidents] = React.useState<BrkIncident[]>(BRK_INCIDENTS);
  const [post, setPost] = React.useState(false);
  const degraded = BRK_SERVICES.filter((s) => s.status !== "operational").length;

  return (
    <div className="pb-16">
      <PageHeader
        title="System ops"
        subtitle="Cross-tenant infrastructure: services, queues, scheduled jobs, maintenance and the public status page"
        actions={
          <>
            <Button variant="surface" onClick={() => toast.success("Runbook opened", { description: "Incident response · v3.2" })}>
              Runbooks
            </Button>
            <Button variant="ember" onClick={() => setPost(true)}>
              <Megaphone /> Post incident
            </Button>
          </>
        }
      />

      <Reveal>
        <SystemSummary degraded={degraded} />
      </Reveal>

      <Reveal delay={0.05} className="mt-4">
        <MaintenanceCard />
      </Reveal>

      <Reveal delay={0.1} className="mt-4">
        <div className="mb-3 flex items-end justify-between px-1">
          <div>
            <h2 className="text-[17px] font-medium tracking-tight">Services</h2>
            <p className="text-[13px] text-fg-3">Shared by all {BRK_TENANTS.length} tenants · refreshed every 10s</p>
          </div>
          <Button size="sm" variant="ghost" onClick={() => toast.success("Health checks re-run", { description: `${BRK_SERVICES.length} services probed · ${degraded} degraded` })}>
            <RotateCw /> Re-check all
          </Button>
        </div>
        <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
          {BRK_SERVICES.map((s) => (
            <ServiceCard key={s.key} s={s} />
          ))}
        </div>
      </Reveal>

      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-2">
        <Reveal delay={0.05}>
          <QueuesCard />
        </Reveal>
        <Reveal delay={0.1}>
          <CronCard />
        </Reveal>
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal delay={0.05} className="xl:col-span-7">
          <StatusPagePreview incidents={incidents} />
        </Reveal>
        <Reveal delay={0.1} className="xl:col-span-5">
          <Card className="flex h-full flex-col">
            <CardHeader title="Incidents" subtitle="Open, scheduled and recent" action={<Button size="sm" variant="surface" onClick={() => setPost(true)}><Megaphone /> Post</Button>} />
            <div className="mt-4 flex-1 space-y-3 px-4 pb-6 sm:px-6">
              {incidents.map((inc, i) => (
                <div key={inc.id} className="relative">
                  <IncidentItem inc={inc} compact={i > 0} />
                  {inc.status !== "resolved" && inc.status !== "scheduled" && (
                    <div className="absolute bottom-4 right-4 flex gap-2">
                      <Button
                        size="xs"
                        variant="up-outline"
                        onClick={() => {
                          setIncidents((xs) => xs.map((x) => (x.id === inc.id ? { ...x, status: "resolved", resolvedAt: "2026-09-24T09:00:00Z", updates: [{ at: "2026-09-24T09:00:00Z", status: "Resolved", text: "All deposits credited. Monitoring complete." }, ...x.updates] } : x)));
                          toast.success(`${inc.id} resolved`, { description: "Status page and subscribed tenants updated" });
                        }}
                      >
                        <CheckCircle2 /> Resolve
                      </Button>
                    </div>
                  )}
                </div>
              ))}
            </div>
          </Card>
        </Reveal>
      </div>

      <PostIncidentDialog
        open={post}
        onOpenChange={setPost}
        onPost={(inc) => {
          setIncidents((xs) => [inc, ...xs]);
          toast.success("Incident posted", { description: `${inc.title} · status.kalkstrade.com + ${BRK_TENANTS.length} tenant status pages updated` });
        }}
      />
    </div>
  );
}

/* ------------------------------------------------------------------ */

function SystemSummary({ degraded }: { degraded: number }) {
  const stats: [string, React.ReactNode, string][] = [
    ["Services", <span key="s" className="k-num">{BRK_SERVICES.length - degraded}<span className="text-fg-3">/{BRK_SERVICES.length}</span></span>, "operational"],
    ["API p95", <span key="a" className="k-num">42<span className="text-sm font-normal text-fg-3">ms</span></span>, "4.8k req/s"],
    ["WS connections", <span key="w" className="k-num">61.2k</span>, "across 3 regions"],
    ["Queue backlog", <span key="q" className="k-num">{formatNumber(BRK_QUEUES.reduce((s, q) => s + q.depth, 0), 0)}</span>, "jobs pending"],
    ["Error rate", <span key="e" className="k-num">0.04<span className="text-sm font-normal text-fg-3">%</span></span>, "5xx last 15m"],
    ["Uptime 30d", <span key="u" className="k-num text-up">99.97<span className="text-sm font-normal text-fg-3">%</span></span>, "SLA 99.95%"],
  ];
  return (
    <Card hot className="relative overflow-hidden">
      <div className="relative flex flex-col gap-5 p-5 lg:flex-row lg:items-center">
        <div className="flex items-center gap-4 lg:w-[300px]">
          <span className="relative grid size-12 place-items-center rounded-full border border-warn/30 bg-warn-soft">
            <span className="absolute inset-0 animate-ping rounded-full bg-warn/20" />
            <AlertOctagon className="relative size-5 text-warn" />
          </span>
          <div>
            <div className="text-[16px] font-medium">Partially degraded</div>
            <div className="text-[12.5px] text-fg-2">{degraded} service degraded · TRON watcher block lag</div>
          </div>
        </div>
        <div className="grid flex-1 grid-cols-2 gap-3 sm:grid-cols-3 xl:grid-cols-6">
          {stats.map(([l, v, s]) => (
            <div key={l} className="rounded-[14px] border border-white/10 bg-black/20 px-4 py-3">
              <div className="k-label">{l}</div>
              <div className="mt-1 text-[22px] font-semibold leading-tight tracking-tight">{v}</div>
              <div className="text-[11.5px] text-fg-3">{s}</div>
            </div>
          ))}
        </div>
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */

function MaintenanceCard() {
  const [on, setOn] = React.useState(false);
  const [confirm, setConfirm] = React.useState(false);
  const [scope, setScope] = React.useState<"all" | "selected">("all");
  const [mode, setMode] = React.useState<"readonly" | "full">("readonly");
  const [msg, setMsg] = React.useState("We're upgrading our systems. Trading continues normally; account changes are paused for up to 30 minutes.");
  const [picked, setPicked] = React.useState<string[]>(["tnt_003"]);
  return (
    <Card className={cn("transition-colors", on && "border-warn/40")}>
      <div className="flex flex-col gap-5 p-5 lg:flex-row">
        <div className="flex items-start gap-4 lg:w-[340px]">
          <span className={cn("grid size-11 shrink-0 place-items-center rounded-full border", on ? "border-warn/40 bg-warn-soft text-warn" : "border-line bg-surface-2 text-fg-2")}>
            <Construction className="size-5" />
          </span>
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-3">
              <span className="text-[16px] font-medium">Maintenance mode</span>
              <Toggle checked={on} onChange={(v) => (v ? setConfirm(true) : (setOn(false), toast.success("Maintenance mode off", { description: "All tenants back online" })))} label="Maintenance mode" />
            </div>
            <div className="mt-1 text-[12.5px] text-fg-3">Shows a branded banner or blocks logins across tenants. Positions and stop-outs keep running.</div>
            <AnimatePresence>
              {on && (
                <motion.div initial={{ opacity: 0, y: -4 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0 }} className="mt-3">
                  <Chip tone="warn" dot>
                    Live since {formatDateTime("2026-09-24T09:00:00Z", { hour: "2-digit", minute: "2-digit" })} GMT+3
                  </Chip>
                </motion.div>
              )}
            </AnimatePresence>
          </div>
        </div>
        <div className="grid flex-1 grid-cols-1 gap-4 md:grid-cols-[auto_auto_1fr]">
          <div>
            <SectionLabel>Scope</SectionLabel>
            <Segmented value={scope} onChange={setScope} options={[{ value: "all", label: "All tenants" }, { value: "selected", label: "Selected" }]} />
            {scope === "selected" && (
              <div className="mt-2.5 flex max-w-[260px] flex-wrap gap-1.5">
                {BRK_TENANTS.map((t) => {
                  const p = picked.includes(t.id);
                  return (
                    <Tooltip key={t.id} content={t.name}>
                      <button type="button" onClick={() => setPicked((xs) => (p ? xs.filter((x) => x !== t.id) : [...xs, t.id]))} className={cn("rounded-[10px] p-0.5 transition-opacity", p ? "ring-2 ring-ember" : "opacity-40 hover:opacity-80")}>
                        <TenantLogo color={t.color} mark={t.mark} size={24} />
                      </button>
                    </Tooltip>
                  );
                })}
              </div>
            )}
          </div>
          <div>
            <SectionLabel>Mode</SectionLabel>
            <Segmented value={mode} onChange={setMode} options={[{ value: "readonly", label: "Read-only banner" }, { value: "full", label: "Block logins" }]} />
            <div className="mt-2.5 text-[12px] text-fg-3">Window: 28 Sep 02:00–02:30 GMT+3</div>
          </div>
          <div>
            <SectionLabel>Client-facing message</SectionLabel>
            <Textarea value={msg} onChange={(e) => setMsg(e.target.value)} className="min-h-[76px]" />
          </div>
        </div>
      </div>
      <ConfirmDialog
        open={confirm}
        onOpenChange={setConfirm}
        danger
        title="Enable maintenance mode?"
        description={`${mode === "full" ? "Client Area logins will be blocked" : "A read-only banner will be shown"} for ${scope === "all" ? `all ${BRK_TENANTS.length} tenants` : `${picked.length} selected tenants`}. Trading and stop-outs are unaffected.`}
        confirmLabel="Enable maintenance"
        onConfirm={() => {
          setOn(true);
          toast.warning("Maintenance mode enabled", { description: scope === "all" ? "All tenants notified" : `${picked.length} tenants notified` });
        }}
      />
    </Card>
  );
}

/* ------------------------------------------------------------------ */

function QueuesCard() {
  const [purge, setPurge] = React.useState<string | null>(null);
  return (
    <Card className="flex h-full flex-col">
      <CardHeader title="Queues" subtitle="BullMQ · Redis-backed" icon={<Layers />} />
      <div className="mt-4 flex-1 space-y-2 px-4 pb-6 sm:px-6">
        {BRK_QUEUES.map((q) => {
          const pct = (q.depth / q.capacity) * 100;
          return (
            <div key={q.name} className="k-row group flex items-center gap-4 px-4 py-3">
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="truncate font-mono text-[12.5px]">{q.name}</span>
                  {q.status !== "healthy" && <Chip size="sm" tone="warn">{q.status}</Chip>}
                </div>
                <div className="mt-2 flex items-center gap-3">
                  <Progress value={pct} tone={pct > 50 ? "warn" : "up"} className="h-1" />
                  <span className="k-num w-12 shrink-0 text-right text-[11px] text-fg-3">{pct.toFixed(0)}%</span>
                </div>
              </div>
              <div className="k-num hidden w-16 text-right sm:block">
                <div className="text-[14px] font-medium">{formatNumber(q.depth, 0)}</div>
                <div className="text-[10.5px] text-fg-3">depth</div>
              </div>
              <div className="k-num hidden w-14 text-right sm:block">
                <div className="text-[13px]">{q.rate}/s</div>
                <div className="text-[10.5px] text-fg-3">{q.consumers} cons.</div>
              </div>
              <div className="k-num w-14 text-right text-[12px] text-fg-3">{q.oldest}</div>
              <div className="flex gap-1">
                <Tooltip content="Retry failed jobs">
                  <IconButton size="sm" aria-label="Retry" onClick={() => toast.success(`Retrying failed jobs · ${q.name}`, { description: "0 failed in last hour" })}>
                    <RotateCw />
                  </IconButton>
                </Tooltip>
                <Tooltip content="Purge queue">
                  <IconButton size="sm" aria-label="Purge" onClick={() => setPurge(q.name)}>
                    <Trash2 />
                  </IconButton>
                </Tooltip>
              </div>
            </div>
          );
        })}
      </div>
      <ConfirmDialog open={!!purge} onOpenChange={(o) => !o && setPurge(null)} danger title={`Purge ${purge}?`} description="Waiting jobs are permanently deleted. Active jobs finish normally." confirmLabel="Purge queue" onConfirm={() => toast.warning(`${purge} purged`)} />
    </Card>
  );
}

function CronCard() {
  return (
    <Card className="flex h-full flex-col">
      <CardHeader title="Scheduled jobs" subtitle="Cron · server time GMT+3" icon={<Clock />} />
      <div className="mt-4 flex-1 space-y-2 px-4 pb-6 sm:px-6">
        {BRK_CRONS.map((c) => (
          <div key={c.name} className="k-row flex items-center gap-4 px-4 py-3">
            <span className={cn("size-2 shrink-0 rounded-full", c.status === "success" ? "bg-up" : c.status === "failed" ? "bg-down" : "animate-pulse-dot bg-ember")} />
            <div className="min-w-0 flex-1">
              <div className="truncate font-mono text-[12.5px]">{c.name}</div>
              <div className="mt-0.5 flex items-center gap-2 text-[11.5px] text-fg-3">
                <span className="rounded bg-surface-3 px-1.5 font-mono">{c.schedule}</span>
                <span className="hidden truncate sm:inline">{c.human}</span>
              </div>
            </div>
            <div className="hidden text-right text-[11.5px] md:block">
              <div className="text-fg-2">{c.status === "running" ? "running now" : timeAgo(c.lastRun)}</div>
              <div className="k-num text-fg-3">{c.duration}</div>
            </div>
            <div className="w-16 text-right">
              <Chip size="sm" tone={c.status === "success" ? "up" : c.status === "failed" ? "down" : "ember"}>
                {c.status}
              </Chip>
            </div>
            <Tooltip content="Run now">
              <IconButton size="sm" aria-label="Run now" onClick={() => toast.success(`${c.name} triggered`, { description: "Manual run · logged to audit trail" })}>
                <Play />
              </IconButton>
            </Tooltip>
          </div>
        ))}
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */

function StatusPagePreview({ incidents }: { incidents: BrkIncident[] }) {
  const open = incidents.filter((i) => i.status !== "resolved" && i.status !== "scheduled");
  const scheduled = incidents.filter((i) => i.status === "scheduled");
  return (
    <Card className="flex h-full flex-col">
      <CardHeader
        title="Public status page"
        subtitle="Preview of status.kalkstrade.com (mirrored on each tenant's status subdomain)"
        action={
          <Button size="sm" variant="surface" onClick={() => toast.message("Opening status.kalkstrade.com")}>
            <ExternalLink /> Open
          </Button>
        }
      />
      <div className="mt-4 flex-1 px-4 pb-6 sm:px-6">
        <div className="overflow-hidden rounded-[18px] border border-line bg-bg">
          <div className="flex items-center gap-2 border-b border-line bg-surface-2 px-4 py-2">
            <span className="size-2.5 rounded-full bg-fg-3/40" />
            <span className="size-2.5 rounded-full bg-fg-3/40" />
            <span className="size-2.5 rounded-full bg-fg-3/40" />
            <span className="ml-3 flex-1 rounded-full bg-surface-3 px-3 py-0.5 text-center font-mono text-[11px] text-fg-3">status.kalkstrade.com</span>
          </div>
          <div className="p-5">
            <div className="flex items-center justify-between">
              <span role="img" aria-label="Kalks" className="block h-5 w-[72px] bg-fg" style={{ WebkitMask: "url(/assets/brand/kalks-logo.svg) left center / contain no-repeat", mask: "url(/assets/brand/kalks-logo.svg) left center / contain no-repeat" }} />
              <Button size="xs" variant="surface" onClick={() => toast.success("Subscribed", { description: "Status updates to ops@kalkstrade.com" })}>
                Subscribe
              </Button>
            </div>
            <div className={cn("mt-4 flex items-center gap-3 rounded-[14px] px-4 py-3", open.length ? "bg-warn-soft text-warn" : "bg-up-soft text-up")}>
              {open.length ? <Wrench className="size-5" /> : <CheckCircle2 className="size-5" />}
              <span className="text-[14px] font-medium">{open.length ? "Some systems are experiencing issues" : "All systems operational"}</span>
            </div>
            {open.map((i) => (
              <div key={i.id} className="mt-3 rounded-[14px] border border-warn/25 px-4 py-3">
                <div className="text-[13.5px] font-medium">{i.title}</div>
                <div className="mt-1 text-[12.5px] text-fg-2">
                  <span className="font-medium capitalize text-info">{i.status}</span> — {i.updates[0]!.text}
                </div>
              </div>
            ))}
            <div className="mt-5 divide-y divide-line rounded-[14px] border border-line">
              {BRK_STATUS_COMPONENTS.map((c) => (
                <div key={c.name} className="grid grid-cols-1 gap-2 px-4 py-3 sm:grid-cols-[200px_1fr_auto] sm:items-center sm:gap-4">
                  <span className="text-[13px]">{c.name}</span>
                  <UptimeBars bars={c.bars} className="h-4" />
                  <span className={cn("text-[12px] sm:w-24 sm:text-right", SVC_TONE[c.status] === "up" ? "text-up" : "text-warn")}>{SVC_LABEL[c.status]}</span>
                </div>
              ))}
            </div>
            {scheduled.map((i) => (
              <div key={i.id} className="mt-3 flex items-start gap-3 rounded-[14px] border border-info/25 bg-info-soft px-4 py-3 text-[12.5px]">
                <Clock className="mt-0.5 size-4 text-info" />
                <div>
                  <div className="font-medium text-fg">{i.title}</div>
                  <div className="text-fg-2">{i.updates[0]!.text}</div>
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */

const COMPONENTS = BRK_STATUS_COMPONENTS.map((c) => c.name);

function PostIncidentDialog({ open, onOpenChange, onPost }: { open: boolean; onOpenChange: (o: boolean) => void; onPost: (i: BrkIncident) => void }) {
  const [title, setTitle] = React.useState("Elevated latency on withdrawals processing");
  const [severity, setSeverity] = React.useState<BrkIncident["severity"]>("minor");
  const [status, setStatus] = React.useState<BrkIncident["status"]>("investigating");
  const [comps, setComps] = React.useState<string[]>(["Withdrawals"]);
  const [msg, setMsg] = React.useState("We're investigating delays in withdrawal processing. Funds are safe; requests will complete once resolved.");
  const [notify, setNotify] = React.useState(true);
  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      width={640}
      title="Post incident"
      description="Published to status.kalkstrade.com and every tenant status page."
      footer={
        <>
          <DialogClose asChild>
            <Button size="sm" variant="ghost">
              Cancel
            </Button>
          </DialogClose>
          <Button
            size="sm"
            variant="ember"
            disabled={!title.trim() || !comps.length}
            onClick={() => {
              onPost({ id: "inc_2092", title, severity, status, components: comps, startedAt: "2026-09-24T09:00:00Z", author: PEOPLE[4]!, updates: [{ at: "2026-09-24T09:00:00Z", status: status[0]!.toUpperCase() + status.slice(1), text: msg }] });
              onOpenChange(false);
            }}
          >
            <Megaphone /> Publish
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <Field label="Title">
          <Input value={title} onChange={(e) => setTitle(e.target.value)} />
        </Field>
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <Field label="Severity">
            <Select value={severity} onChange={(v) => setSeverity(v as BrkIncident["severity"])} options={["minor", "major", "critical", "maintenance"].map((v) => ({ value: v, label: v[0]!.toUpperCase() + v.slice(1) }))} />
          </Field>
          <Field label="Status">
            <Select value={status} onChange={(v) => setStatus(v as BrkIncident["status"])} options={["investigating", "identified", "monitoring", "scheduled"].map((v) => ({ value: v, label: v[0]!.toUpperCase() + v.slice(1) }))} />
          </Field>
        </div>
        <div>
          <SectionLabel>Affected components</SectionLabel>
          <div className="flex flex-wrap gap-1.5">
            {COMPONENTS.map((c) => {
              const on = comps.includes(c);
              return (
                <button key={c} type="button" onClick={() => setComps((xs) => (on ? xs.filter((x) => x !== c) : [...xs, c]))} className={cn("rounded-full border px-3 py-1 text-[12.5px] transition-colors", on ? "border-ember/40 bg-ember-soft text-ember" : "border-line bg-surface-2 text-fg-2 hover:text-fg")}>
                  {c}
                </button>
              );
            })}
          </div>
        </div>
        <Field label="Message">
          <Textarea value={msg} onChange={(e) => setMsg(e.target.value)} />
        </Field>
        <div className="k-row flex items-center justify-between gap-4 p-4">
          <div className="flex items-center gap-3">
            <Avatar src={PEOPLE[4]!.photo} name={PEOPLE[4]!.name} size={32} />
            <div>
              <div className="text-[13px] font-medium">Notify subscribers & tenant admins</div>
              <div className="text-[12px] text-fg-3">Email + in-app to 2,418 subscribers</div>
            </div>
          </div>
          <Toggle checked={notify} onChange={setNotify} label="Notify" />
        </div>
      </div>
    </Dialog>
  );
}

export default function Page() {
  return IS_DEMO ? <SystemOpsPage /> : <LiveSystem />;
}
