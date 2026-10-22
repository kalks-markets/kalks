"use client";

import * as React from "react";
import Link from "next/link";
import { useRouter, useSearchParams } from "next/navigation";
import { Download, RefreshCw, Search, ShieldCheck, X } from "lucide-react";
import { toast } from "sonner";
import { Avatar, Button, Card, CardHeader, Chip, CopyButton, DataTable, Dialog, EmptyState, KeyValue, PageHeader, Reveal, Segmented, type Column } from "@kalks/ui";
import { ErrorState, FilterSelect, Mono, Pager, TableSkeleton, actionLabel, actionTone, ago, device, downloadCsv, qs, useApi, useDebounced, useNow, when } from "./kit";
import type { AuditEvent, AuditPage, Stats } from "./types";
import { useCan } from "@/components/staff-session";

type ActorKind = "all" | "staff" | "user" | "system" | "anonymous";
const PER = 50;

function actorName(e: AuditEvent) {
  if (e.actor.name) return e.actor.name;
  if (e.actor.kind === "anonymous") return typeof e.meta.email === "string" ? e.meta.email : typeof e.meta.login === "string" ? `Trader login ${e.meta.login}` : "Unknown";
  if (e.actor.kind === "system") return "System";
  return `${e.actor.kind} #${e.actor.id ?? "?"}`;
}

function targetText(e: AuditEvent) {
  if (!e.target.kind) return null;
  return e.target.name ?? e.target.email ?? `${e.target.kind} #${e.target.id}`;
}

/** One-line summary of the meta JSON. */
function metaSummary(meta: Record<string, unknown>) {
  const parts: string[] = [];
  for (const [k, v] of Object.entries(meta)) {
    if (v === null || v === undefined || k === "email") continue;
    if (typeof v === "object") {
      parts.push(`${k}: ${JSON.stringify(v)}`);
      continue;
    }
    parts.push(`${k}: ${String(v)}`);
  }
  return parts.join(" · ");
}

/** The CSV export reads at most EXPORT_PAGES × EXPORT_PER_PAGE rows (the gateway allows 1000 a page for exports). */
const EXPORT_PAGES = 20;
const EXPORT_PER_PAGE = 1000;

export function LiveAudit() {
  const router = useRouter();
  const params = useSearchParams();
  const now = useNow();
  const initialActor = params.get("actor") ?? "";
  const [actorKind, setActorKind] = React.useState<ActorKind>((initialActor.split(":")[0] as ActorKind) || "all");
  const [actorId, setActorId] = React.useState<string>(initialActor.includes(":") ? initialActor.split(":")[1]! : "");
  const [action, setAction] = React.useState(params.get("action") ?? "all");
  const [q, setQ] = React.useState(params.get("q") ?? "");
  const [from, setFrom] = React.useState(params.get("from") ?? "");
  const [to, setTo] = React.useState(params.get("to") ?? "");
  const [page, setPage] = React.useState(1);
  const [open, setOpen] = React.useState<AuditEvent | null>(null);
  const dq = useDebounced(q.trim(), 300);
  const actor = actorKind === "all" ? "" : actorId ? `${actorKind}:${actorId}` : actorKind;
  const filters = { actor, action, q: dq, from, to };

  React.useEffect(() => setPage(1), [actor, action, dq, from, to]);
  React.useEffect(() => {
    router.replace(`/security${qs({ actor, action, q: dq, from, to })}`, { scroll: false });
  }, [actor, action, dq, from, to, router]);

  const { data, error, loading, reload } = useApi<AuditPage>(`/api/admin/audit${qs({ ...filters, page, per_page: PER })}`, { refreshMs: 30_000 });
  const stats = useApi<Stats>("/api/admin/stats", { refreshMs: 30_000 });
  const sec = stats.data?.security;
  const canExport = useCan("audit.export");

  async function exportCsv() {
    const rows: AuditEvent[] = [];
    let total = 0;
    for (let p = 1; p <= EXPORT_PAGES; p++) {
      const r = await fetch(`/api/admin/audit${qs({ ...filters, page: p, per_page: EXPORT_PER_PAGE, export: true })}`, { cache: "no-store" });
      if (!r.ok) return toast.error("Export failed", { description: "Couldn't load the audit log. Try again." });
      const d = (await r.json()) as AuditPage;
      total = d.total;
      rows.push(...d.items);
      if (rows.length >= d.total || d.items.length === 0) break;
    }
    const truncated = rows.length < total;
    downloadCsv(
      `audit-log-${new Date().toISOString().slice(0, 10)}`,
      ["ID", "Time (UTC)", "Actor type", "Actor ID", "Actor", "Actor email", "Action", "Target type", "Target ID", "Target", "IP", "User agent", "Details"],
      rows.map((e) => [e.id, e.created_at, e.actor.kind, e.actor.id, actorName(e), e.actor.email, e.action, e.target.kind, e.target.id, targetText(e), e.ip, e.user_agent, JSON.stringify(e.meta)]),
    );
    if (truncated)
      toast.warning(`Export cut off at ${rows.length.toLocaleString("en-US")} of ${total.toLocaleString("en-US")} entries`, {
        description: `The file has the newest entries only, back to ${when(rows[rows.length - 1]?.created_at ?? null, true)}. Narrow the date range or filters and export the rest separately.`,
        duration: 15_000,
      });
  }

  const columns: Column<AuditEvent>[] = [
    { key: "time", header: "Time (GMT+3)", cell: (e) => <span className="whitespace-nowrap text-fg-2" title={ago(e.created_at, now)}>{when(e.created_at, true)}</span> },
    {
      key: "actor",
      header: "Actor",
      cell: (e) => (
        <span className="flex min-w-0 items-center gap-2.5">
          <Avatar name={actorName(e)} size={28} />
          <span className="min-w-0">
            <span className="block max-w-[200px] truncate font-medium">{actorName(e)}</span>
            <span className="block truncate text-[11.5px] capitalize text-fg-3">{e.actor.role_label ?? (e.actor.kind === "user" ? "Client" : e.actor.kind)}</span>
          </span>
        </span>
      ),
    },
    {
      key: "action",
      header: "Action",
      cell: (e) => (
        <span className="flex flex-col items-start gap-1">
          <span className="text-[13px]">{actionLabel(e.action)}</span>
          <Chip size="sm" tone={actionTone(e.action)}>
            <span className="font-mono">{e.action}</span>
          </Chip>
        </span>
      ),
    },
    { key: "target", header: "Target", cell: (e) => (targetText(e) ? (e.target.kind === "user" && e.target.id ? <Link href={`/clients/${e.target.id}`} onClick={(x) => x.stopPropagation()} className="text-ember hover:underline">{targetText(e)}</Link> : <span className="text-fg-2">{targetText(e)}</span>) : <span className="text-fg-3">—</span>), hideOn: "lg" },
    { key: "ip", header: "IP", cell: (e) => <Mono className="text-fg-2">{e.ip ?? "—"}</Mono>, hideOn: "md" },
    { key: "details", header: "Details", cell: (e) => <span className="block max-w-[280px] truncate text-[12px] text-fg-3">{metaSummary(e.meta) || "—"}</span>, hideOn: "xl" },
  ];

  const anyFilter = actorKind !== "all" || action !== "all" || !!dq || !!from || !!to;

  return (
    <div className="pb-10">
      <PageHeader
        title="Audit log"
        subtitle="Every sign-in, security event and staff action, append-only. Times in GMT+3."
        actions={
          <>
            <Button variant="surface" onClick={reload}>
              <RefreshCw /> Refresh
            </Button>
            {canExport && (
              <Button
                variant="ember"
                onClick={exportCsv}
                disabled={!data || data.total === 0}
                title={data && data.total > EXPORT_PAGES * EXPORT_PER_PAGE ? `Exports the newest ${(EXPORT_PAGES * EXPORT_PER_PAGE).toLocaleString("en-US")} of ${data.total.toLocaleString("en-US")} matching entries; narrow the filters to get the rest` : undefined}
              >
                <Download /> Export CSV
              </Button>
            )}
          </>
        }
      />

      <Reveal>
        <div className="mb-4 grid grid-cols-2 gap-3 lg:grid-cols-4">
          {[
            { label: "Events · 24h", value: sec?.audit_events_24h, sub: "All actors" },
            { label: "Client sign-ins · 24h", value: sec?.logins_24h, sub: "Successful" },
            { label: "Failed sign-ins · 24h", value: sec?.failed_logins_24h, sub: "Clients and staff", warn: (sec?.failed_logins_24h ?? 0) > 0 },
            { label: "Matching this filter", value: data?.total, sub: anyFilter ? "Filtered" : "All time" },
          ].map((x) => (
            <div key={x.label} className="k-row px-4 py-3">
              <div className="truncate text-[11px] uppercase tracking-wider text-fg-3">{x.label}</div>
              <div className={`k-num mt-1 text-[20px] font-medium ${x.warn ? "text-warn" : ""}`}>{x.value?.toLocaleString("en-US") ?? "—"}</div>
              <div className="truncate text-[11.5px] text-fg-3">{x.sub}</div>
            </div>
          ))}
        </div>
      </Reveal>

      <Reveal delay={0.05}>
        <Card>
          <CardHeader title="Events" subtitle="Click a row for the full record" icon={<ShieldCheck />} />
          <div className="px-4 pb-5 pt-4 sm:px-6">
            <div className="mb-4 flex flex-wrap items-center gap-2">
              <div className="flex h-9 w-full min-w-0 items-center gap-2 rounded-full border border-line bg-surface-2 px-3.5 sm:w-auto sm:max-w-xs sm:flex-1">
                <Search className="size-3.5 shrink-0 text-fg-3" />
                <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Email, name, IP or detail" className="min-w-0 flex-1 bg-transparent text-[13px] outline-none placeholder:text-fg-3" aria-label="Search audit log" />
              </div>
              <Segmented
                size="xs"
                value={actorKind}
                onChange={(v) => {
                  setActorKind(v);
                  setActorId("");
                }}
                options={[
                  { value: "all", label: "Everyone" },
                  { value: "staff", label: "Staff" },
                  { value: "user", label: "Clients" },
                  { value: "anonymous", label: "Unknown" },
                  { value: "system", label: "System" },
                ]}
              />
              <FilterSelect label="Action" value={action} onChange={setAction} options={[{ value: "all", label: "All actions" }, ...(data?.actions ?? (action !== "all" ? [action] : [])).map((a) => ({ value: a, label: a }))]} />
              <label className="flex h-9 items-center gap-2 rounded-full border border-line bg-surface-2 px-3.5 text-[12.5px] text-fg-3">
                From
                <input type="date" value={from} onChange={(e) => setFrom(e.target.value)} className="bg-transparent text-fg outline-none" aria-label="From date" />
              </label>
              <label className="flex h-9 items-center gap-2 rounded-full border border-line bg-surface-2 px-3.5 text-[12.5px] text-fg-3">
                To
                <input type="date" value={to} onChange={(e) => setTo(e.target.value)} className="bg-transparent text-fg outline-none" aria-label="To date" />
              </label>
              {actorId && (
                <Chip tone="ember">
                  {actorKind} #{actorId}
                  <button onClick={() => setActorId("")} aria-label="Clear actor" className="hover:text-fg">
                    <X className="size-3" />
                  </button>
                </Chip>
              )}
              {anyFilter && (
                <Button
                  size="xs"
                  variant="ghost"
                  onClick={() => {
                    setActorKind("all");
                    setActorId("");
                    setAction("all");
                    setQ("");
                    setFrom("");
                    setTo("");
                  }}
                >
                  Clear filters
                </Button>
              )}
            </div>
            {error ? (
              <ErrorState error={error} onRetry={reload} />
            ) : !data ? (
              <TableSkeleton />
            ) : (
              <div className={loading ? "opacity-60 transition-opacity" : "transition-opacity"}>
                <DataTable
                  rows={data.items}
                  pageSize={PER}
                  columns={columns}
                  rowKey={(e) => String(e.id)}
                  onRowClick={setOpen}
                  dense
                  empty={<EmptyState title="No events match" text="Try a wider date range or clear the filters." illustration="magnifying_glass_tilted_left" />}
                />
                <Pager page={data.page} perPage={data.per_page} total={data.total} onPage={setPage} />
              </div>
            )}
          </div>
        </Card>
      </Reveal>

      <Dialog side="right" open={!!open} onOpenChange={(o) => !o && setOpen(null)} title={open ? actionLabel(open.action) : "Event"} description={open ? `Audit #${open.id} · ${when(open.created_at, true)} GMT+3` : undefined}>
        {open && (
          <div className="space-y-5">
            <KeyValue
              rows={[
                ["Action", <Chip key="a" size="sm" tone={actionTone(open.action)}><span className="font-mono">{open.action}</span></Chip>],
                ["Actor", <span key="ac">{actorName(open)}{open.actor.email && open.actor.email !== actorName(open) ? <span className="text-fg-3"> · {open.actor.email}</span> : null}</span>],
                ["Actor type", <span key="at" className="capitalize">{open.actor.role_label ?? open.actor.kind}{open.actor.id ? ` #${open.actor.id}` : ""}</span>],
                ["Target", targetText(open) ?? "—"],
                ["IP address", <span key="ip" className="inline-flex items-center gap-1"><Mono>{open.ip ?? "—"}</Mono>{open.ip && <CopyButton value={open.ip} label="IP" />}</span>],
                ["Device", device(open.user_agent)],
                ["Time (UTC)", <Mono key="t">{open.created_at}</Mono>],
              ]}
            />
            <div>
              <div className="k-label mb-2">Details</div>
              <pre className="max-h-72 overflow-auto rounded-[14px] border border-line bg-surface-2 p-4 font-mono text-[12px] leading-relaxed text-fg-2">{JSON.stringify(open.meta, null, 2)}</pre>
            </div>
            {open.user_agent && (
              <div>
                <div className="k-label mb-2">User agent</div>
                <p className="break-all rounded-[14px] border border-line bg-surface-2 p-3 font-mono text-[11.5px] text-fg-3">{open.user_agent}</p>
              </div>
            )}
            <div className="flex flex-wrap gap-2">
              {open.actor.kind === "user" && open.actor.id && (
                <Link href={`/clients/${open.actor.id}`} className="text-[12.5px] text-ember hover:underline">
                  Open client profile
                </Link>
              )}
              {open.actor.kind !== "anonymous" && open.actor.kind !== "system" && open.actor.id && (
                <button
                  className="text-[12.5px] text-ember hover:underline"
                  onClick={() => {
                    setActorKind(open.actor.kind as ActorKind);
                    setActorId(String(open.actor.id));
                    setOpen(null);
                  }}
                >
                  All events by this {open.actor.kind === "user" ? "client" : "staff member"}
                </button>
              )}
            </div>
          </div>
        )}
      </Dialog>
    </div>
  );
}
