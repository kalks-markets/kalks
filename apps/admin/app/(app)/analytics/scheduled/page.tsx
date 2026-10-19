"use client";

import * as React from "react";
import { AlertTriangle, CalendarClock, CheckCircle2, Copy, Mail, MoreHorizontal, Pencil, Play, Plus, Send, Server, MessageSquare, Trash2, XCircle } from "lucide-react";
import { toast } from "sonner";
import {
  Avatar,
  Button,
  Card,
  CardHeader,
  Chip,
  DataTable,
  IconButton,
  KpiCard,
  Menu,
  PageHeader,
  Reveal,
  Segmented,
  StatusChip,
  Toggle,
  Tooltip,
  cn,
  formatDateTime,
  type Column,
} from "@kalks/ui";
import { PEOPLE } from "@kalks/mock";
import { ANL_DELIVERY_LOG, ANL_SCHEDULES, anlStaffEmail, type AnlDelivery, type AnlSchedule } from "@kalks/mock/admin-growth-analytics";
import { ScheduleDialog, type ScheduleDraft } from "@/components/analytics/schedule-dialog";
import { IS_DEMO } from "@kalks/mock/mode";
import { LiveScheduled } from "@/components/reports/live-ops";

const PHOTO_BY_EMAIL = Object.fromEntries(PEOPLE.map((p) => [anlStaffEmail(p), p]));
const FREQ_TONE = { daily: "ember", weekly: "gold", monthly: "info" } as const;
const DOW = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
/** Fixed "now" for the mock: 24 Sep 2026 13:30 GMT+3. */
const NOW = Date.UTC(2026, 8, 24, 10, 30);

function nextRun(d: ScheduleDraft): string {
  const [hh, mm] = d.time.split(":").map(Number);
  for (let i = 0; i < 40; i++) {
    const day = new Date(Date.UTC(2026, 8, 24 + i, hh! - 3, mm!));
    if (day.getTime() <= NOW) continue;
    const local = new Date(day.getTime() + 3 * 3600_000);
    if (d.frequency === "daily") return day.toISOString();
    if (d.frequency === "weekly" && DOW[local.getUTCDay()] === (d.day ?? "Mon")) return day.toISOString();
    if (d.frequency === "monthly") {
      const dom = local.getUTCDate();
      const want = d.day === "Last" ? new Date(Date.UTC(local.getUTCFullYear(), local.getUTCMonth() + 1, 0)).getUTCDate() : parseInt(d.day ?? "1", 10);
      if (dom === want) return day.toISOString();
    }
  }
  return new Date(NOW + 86_400_000).toISOString();
}

function untilLabel(iso: string) {
  const mins = Math.round((new Date(iso).getTime() - NOW) / 60_000);
  if (mins < 60) return `in ${mins}m`;
  if (mins < 60 * 24) return `in ${Math.floor(mins / 60)}h ${mins % 60}m`;
  return `in ${Math.round(mins / 1440)}d`;
}

function Recipients({ list }: { list: string[] }) {
  const shown = list.slice(0, 4);
  return (
    <Tooltip content={<div className="space-y-0.5">{list.map((e) => <div key={e} className="font-mono text-[11px]">{e}</div>)}</div>}>
      <div className="flex items-center">
        <div className="flex -space-x-2">
          {shown.map((e, i) => {
            const p = PHOTO_BY_EMAIL[e];
            return (
              <span key={e} className="relative inline-flex" style={{ zIndex: shown.length - i }}>
                {p ? (
                  <Avatar src={p.photo} name={p.name} size={26} className="rounded-full ring-2 ring-surface" />
                ) : (
                  <span className="grid size-[26px] place-items-center rounded-full bg-surface-3 text-[10px] font-semibold uppercase text-fg-2 ring-2 ring-surface">{e.slice(0, 2)}</span>
                )}
              </span>
            );
          })}
        </div>
        {list.length > shown.length && <span className="k-num ml-1.5 text-[11.5px] text-fg-3">+{list.length - shown.length}</span>}
        <span className="ml-2 hidden max-w-[150px] truncate text-[11.5px] text-fg-3 2xl:inline">{list[0]}</span>
      </div>
    </Tooltip>
  );
}

const CH_ICON = { Email: <Mail className="size-3.5" />, SFTP: <Server className="size-3.5" />, Slack: <MessageSquare className="size-3.5" /> };

function DemoScheduledReportsPage() {
  const [rows, setRows] = React.useState<AnlSchedule[]>(ANL_SCHEDULES);
  const [log, setLog] = React.useState<AnlDelivery[]>(ANL_DELIVERY_LOG);
  const [open, setOpen] = React.useState(false);
  const [editing, setEditing] = React.useState<AnlSchedule | null>(null);
  const [filter, setFilter] = React.useState<"all" | "daily" | "weekly" | "monthly">("all");
  const [logFilter, setLogFilter] = React.useState<"all" | "issues">("all");
  const seq = React.useRef(0);

  const runNow = (s: AnlSchedule) => {
    seq.current += 1;
    const id = `DLV-${99_812 + seq.current}`;
    const entry: AnlDelivery = { id, schedule: s.name, at: new Date().toISOString(), recipients: s.recipients.length, status: "running", size: "—", channel: s.format === "XML" ? "SFTP" : "Email" };
    setLog((l) => [entry, ...l]);
    toast(`Running “${s.name}”`, { description: `${s.report} · ${s.format} → ${s.recipients.length} recipient${s.recipients.length > 1 ? "s" : ""}` });
    setTimeout(() => {
      setLog((l) => l.map((x) => (x.id === id ? { ...x, status: "delivered", size: s.format === "PDF" ? "1.9 MB" : "3.2 MB" } : x)));
      setRows((r) => r.map((x) => (x.id === s.id ? { ...x, lastStatus: "delivered" } : x)));
      toast.success(`“${s.name}” delivered`, { description: `${s.recipients.length} recipient${s.recipients.length > 1 ? "s" : ""} · ${id}` });
    }, 2200);
  };

  const save = (d: ScheduleDraft) => {
    if (editing) {
      setRows((r) => r.map((x) => (x.id === editing.id ? { ...x, ...d, nextRun: nextRun(d) } : x)));
      toast.success("Schedule updated", { description: `${d.name} · next run ${formatDateTime(nextRun(d))} GMT+3` });
    } else {
      const s: AnlSchedule = { ...d, id: `SCH-${140 + rows.length}`, nextRun: nextRun(d), lastStatus: "never", owner: PEOPLE[4]! };
      setRows((r) => [s, ...r]);
      toast.success("Schedule created", { description: `${d.name} · first run ${formatDateTime(s.nextRun)} GMT+3` });
    }
  };

  const view = rows.filter((r) => filter === "all" || r.frequency === filter);
  const upcoming = [...rows].filter((r) => r.enabled).sort((a, b) => a.nextRun.localeCompare(b.nextRun)).slice(0, 6);
  const logView = log.filter((l) => logFilter === "all" || l.status === "failed" || l.status === "bounced");
  const delivered = log.filter((l) => l.status === "delivered").length;

  const cols: Column<AnlSchedule>[] = [
    {
      key: "name",
      header: "Schedule",
      cell: (s) => (
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className={cn("truncate text-[13.5px] font-medium", !s.enabled && "text-fg-3")}>{s.name}</span>
            {s.lastStatus === "failed" && (
              <Tooltip content="Last delivery failed">
                <AlertTriangle className="size-3.5 shrink-0 text-down" />
              </Tooltip>
            )}
          </div>
          <div className="text-[11.5px] text-fg-3">
            <span className="font-mono">{s.id}</span> · {s.report}
          </div>
        </div>
      ),
    },
    {
      key: "freq",
      header: "Frequency",
      cell: (s) => (
        <div className="flex items-center gap-2">
          <Chip size="sm" tone={FREQ_TONE[s.frequency]} className="capitalize">{s.frequency}</Chip>
          <span className="k-num text-[12px] text-fg-2">{s.day ? `${s.day} · ` : ""}{s.time}</span>
        </div>
      ),
    },
    {
      key: "next",
      header: "Next run (GMT+3)",
      sort: (s) => s.nextRun,
      cell: (s) =>
        s.enabled ? (
          <div>
            <div className="k-num text-[12.5px]">{formatDateTime(s.nextRun)}</div>
            <div className="text-[11px] text-fg-3">{untilLabel(s.nextRun)}</div>
          </div>
        ) : (
          <span className="text-[12px] text-fg-3">Paused</span>
        ),
    },
    { key: "rcp", header: "Recipients", cell: (s) => <Recipients list={s.recipients} /> },
    { key: "fmt", header: "Format", cell: (s) => <Chip size="sm">{s.format}</Chip>, hideOn: "md" },
    { key: "last", header: "Last run", cell: (s) => (s.lastStatus === "never" ? <Chip size="sm">Never run</Chip> : <StatusChip status={s.lastStatus === "delivered" ? "completed" : "failed"} label={s.lastStatus === "delivered" ? "Delivered" : "Failed"} />), hideOn: "lg" },
    {
      key: "on",
      header: "Enabled",
      align: "center",
      cell: (s) => (
        <div className="flex justify-center" onClick={(e) => e.stopPropagation()}>
          <Toggle
            checked={s.enabled}
            label={`Toggle ${s.name}`}
            onChange={(v) => {
              setRows((r) => r.map((x) => (x.id === s.id ? { ...x, enabled: v } : x)));
              toast.success(v ? "Schedule resumed" : "Schedule paused", { description: s.name });
            }}
          />
        </div>
      ),
    },
    {
      key: "act",
      header: "",
      align: "right",
      cell: (s) => (
        <div className="flex items-center justify-end gap-1.5" onClick={(e) => e.stopPropagation()}>
          <Tooltip content="Run now">
            <IconButton size="sm" aria-label="Run now" onClick={() => runNow(s)}>
              <Play />
            </IconButton>
          </Tooltip>
          <Menu
            trigger={
              <IconButton size="sm" aria-label="More">
                <MoreHorizontal />
              </IconButton>
            }
            items={[
              { label: "Edit", icon: <Pencil />, onSelect: () => { setEditing(s); setOpen(true); } },
              {
                label: "Duplicate",
                icon: <Copy />,
                onSelect: () => {
                  const c = { ...s, id: `SCH-${150 + rows.length}`, name: `${s.name} (copy)`, enabled: false, lastStatus: "never" as const };
                  setRows((r) => [c, ...r]);
                  toast.success("Schedule duplicated", { description: `${c.name} · paused until you enable it` });
                },
              },
              { label: "Send test to me", icon: <Send />, onSelect: () => toast.success("Test sent", { description: `${s.report} → priya.nair@kalkstrade.com` }) },
              "sep",
              {
                label: "Delete",
                icon: <Trash2 />,
                danger: true,
                onSelect: () => {
                  setRows((r) => r.filter((x) => x.id !== s.id));
                  toast.success("Schedule deleted", { description: s.name, action: { label: "Undo", onClick: () => setRows((r) => [s, ...r]) } });
                },
              },
            ]}
          />
        </div>
      ),
    },
  ];

  return (
    <div className="pb-16">
      <PageHeader
        title="Scheduled reports"
        subtitle="Automated deliveries by email, SFTP or Slack · all times server GMT+3"
        actions={
          <Button
            variant="ember"
            size="lg"
            shimmer
            onClick={() => {
              setEditing(null);
              setOpen(true);
            }}
          >
            <Plus /> New schedule
          </Button>
        }
      />

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <KpiCard label="Active schedules" icon={<CalendarClock />} value={<span className="k-num">{rows.filter((r) => r.enabled).length}<span className="text-fg-3">/{rows.length}</span></span>} chip="3 regulator feeds" chipTone="gold" />
        <KpiCard label="Delivered · 7 days" icon={<CheckCircle2 />} value={<span className="k-num">{58 + delivered}</span>} chip="98.4% success" chipTone="up" delay={0.05} />
        <KpiCard label="Failed / bounced" icon={<XCircle />} value={<span className="k-num">{log.filter((l) => l.status === "failed" || l.status === "bounced").length}</span>} chip="Needs attention" chipTone="down" delay={0.1} />
        <KpiCard label="Next delivery" icon={<Send />} value={<span className="k-num">{upcoming[0] ? upcoming[0].time : "—"}</span>} hot illustration="calendar" footer={<span className="truncate text-[11.5px] text-fg-2">{upcoming[0]?.name} · {upcoming[0] ? untilLabel(upcoming[0].nextRun) : ""}</span>} delay={0.15} />
      </div>

      <Reveal delay={0.1} className="mt-4">
        <Card>
          <CardHeader
            title="Schedules"
            subtitle={`${view.length} schedule${view.length === 1 ? "" : "s"} · click a row to edit`}
            icon={<CalendarClock />}
            action={<Segmented size="xs" value={filter} onChange={setFilter} options={[{ value: "all", label: "All" }, { value: "daily", label: "Daily" }, { value: "weekly", label: "Weekly" }, { value: "monthly", label: "Monthly" }]} />}
          />
          <div className="mt-4 px-4 pb-5 sm:px-6">
            <DataTable
              columns={cols}
              rows={view}
              pageSize={10}
              search={(s) => `${s.name} ${s.report} ${s.recipients.join(" ")} ${s.id}`}
              searchPlaceholder="Search schedules or recipients…"
              rowKey={(s) => s.id}
              onRowClick={(s) => {
                setEditing(s);
                setOpen(true);
              }}
            />
          </div>
        </Card>
      </Reveal>

      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal delay={0.1} className="xl:col-span-8">
          <Card className="h-full">
            <CardHeader title="Delivery log" subtitle="Last 7 days · retries automatic up to 3×" icon={<Send />} action={<Segmented size="xs" value={logFilter} onChange={setLogFilter} options={[{ value: "all", label: "All" }, { value: "issues", label: "Issues" }]} />} />
            <div className="mt-4 space-y-2 px-4 pb-5 sm:px-6">
              {logView.slice(0, 8).map((l) => (
                <div key={l.id} className="k-row flex items-center gap-3 px-4 py-2.5">
                  <span
                    className={cn(
                      "grid size-8 shrink-0 place-items-center rounded-full border [&_svg]:size-3.5",
                      l.status === "delivered" ? "border-up/25 bg-up-soft text-up" : l.status === "running" ? "border-ember/30 bg-ember-soft text-ember" : "border-down/25 bg-down-soft text-down",
                    )}
                  >
                    {l.status === "delivered" ? <CheckCircle2 /> : l.status === "running" ? <Play /> : <XCircle />}
                  </span>
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-[13px] font-medium">{l.schedule}</div>
                    <div className="truncate text-[11.5px] text-fg-3">
                      <span className="font-mono">{l.id}</span> · {l.recipients} recipient{l.recipients > 1 ? "s" : ""}
                      {l.note && <span className="text-down"> · {l.note}</span>}
                    </div>
                  </div>
                  <Chip size="sm" className="hidden sm:inline-flex">{CH_ICON[l.channel]}{l.channel}</Chip>
                  <span className="k-num hidden w-16 text-right text-[12px] text-fg-3 md:block">{l.size}</span>
                  <span className="k-num hidden w-28 text-right text-[12px] text-fg-2 sm:block">{formatDateTime(l.at)}</span>
                  {l.status === "failed" || l.status === "bounced" ? (
                    <Button size="xs" variant="down-outline" onClick={() => {
                      setLog((x) => x.map((y) => (y.id === l.id ? { ...y, status: "delivered", note: undefined, size: "2.0 MB" } : y)));
                      toast.success("Redelivered", { description: `${l.schedule} · ${l.id}` });
                    }}>
                      Retry
                    </Button>
                  ) : (
                    <StatusChip status={l.status === "running" ? "running" : "completed"} label={l.status === "running" ? "Sending" : "Delivered"} />
                  )}
                </div>
              ))}
            </div>
          </Card>
        </Reveal>
        <Reveal delay={0.15} className="xl:col-span-4">
          <Card className="h-full">
            <CardHeader title="Upcoming runs" subtitle="Next deliveries · GMT+3" icon={<CalendarClock />} />
            <div className="relative mt-5 px-6 pb-6">
              <span className="absolute bottom-8 left-[33px] top-2 w-px bg-gradient-to-b from-ember/60 via-line to-transparent" />
              <div className="space-y-4">
                {upcoming.map((s, i) => (
                  <div key={s.id} className="relative flex gap-4">
                    <span className={cn("relative z-10 mt-1 size-[18px] shrink-0 rounded-full border-2", i === 0 ? "border-ember bg-ember-soft shadow-[0_0_14px_rgba(255,90,31,0.6)]" : "border-line bg-surface-2")} />
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center justify-between gap-2">
                        <span className="truncate text-[13px] font-medium">{s.name}</span>
                        <span className={cn("k-num shrink-0 text-[11.5px]", i === 0 ? "text-ember" : "text-fg-3")}>{untilLabel(s.nextRun)}</span>
                      </div>
                      <div className="k-num mt-0.5 text-[11.5px] text-fg-3">
                        {formatDateTime(s.nextRun)} · {s.format} · {s.recipients.length} recipients
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </Card>
        </Reveal>
      </div>

      <ScheduleDialog open={open} onOpenChange={setOpen} initial={editing} onSave={save} />
    </div>
  );
}

/** Live builds: the reports service (/api/reports). Demo builds: mock data. */
export default function ScheduledReportsPage() {
  return IS_DEMO ? <DemoScheduledReportsPage /> : <LiveScheduled />;
}
