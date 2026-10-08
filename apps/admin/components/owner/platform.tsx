"use client";

import * as React from "react";
import Link from "next/link";
import { Activity, Flag, Layers, Plus, RefreshCw, RotateCcw, Server, Trash2 } from "lucide-react";
import { Button, Card, CardHeader, Chip, Dialog, EmptyState, Field, IconButton, Input, KpiCard, PageHeader, Toggle, cn } from "@kalks/ui";
import { ErrorState, TableSkeleton, ago, useApi, useNow, when } from "@/components/live/kit";
import { STATUS_TONE, act, call, cap } from "@/components/rbac/kit";
import type { FeatureCatalogue, Probe } from "./types";
import { MODULE_LOSES, useModuleSwitch } from "./module-switch";

/** Tenant × feature grid (every tenant, the platform's own Kalks tenant included); a click sets an override for that
 *  tenant, the reset icon returns to the default. A module switch asks to confirm with a reason first. */
function Grid({ data, kind, reload }: { data: FeatureCatalogue; kind: "module" | "flag"; reload: () => void }) {
  const features = data.features.filter((f) => f.kind === kind);
  const sw = useModuleSwitch(reload);
  const set = async (tenant: number, key: string, v: boolean | null) => {
    if (kind === "module") {
      const t = data.tenants.find((x) => x.id === tenant);
      const f = features.find((x) => x.key === key);
      sw.ask({ tenantId: tenant, tenantName: t?.name ?? `#${tenant}`, key, name: f?.name ?? key, value: v, effective: v ?? !!f?.default });
      return;
    }
    if (await act("PUT", `/api/owner/tenants/${tenant}/features`, { [key]: v }, v === null ? "Back to default" : `${key}: ${v ? "on" : "off"}`)) reload();
  };
  return (
    <div className="overflow-x-auto">
      <table className="w-full min-w-[720px] border-separate border-spacing-0 text-[13px]" data-testid={`${kind}-grid`}>
        <thead>
          <tr className="text-left text-[11.5px] uppercase tracking-[0.08em] text-fg-3">
            <th className="sticky left-0 z-10 border-b border-line bg-surface px-3 py-2.5 font-medium">{kind === "module" ? "Module" : "Flag"}</th>
            {data.tenants.map((t) => (
              <th key={t.id} className="border-b border-line px-3 py-2.5 font-medium">
                <Link href={`/brokers/${t.id}`} className="normal-case tracking-normal hover:text-ember">
                  {t.name}
                </Link>
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {features.map((f) => (
            <tr key={f.key}>
              <td className="sticky left-0 z-10 max-w-[300px] border-b border-line bg-surface px-3 py-3">
                <div className="font-medium">{f.name}</div>
                <div className="text-[11.5px] text-fg-3">
                  {f.key} · default {f.default ? "on" : "off"}
                </div>
                {kind === "module" && MODULE_LOSES[f.key] && <div className="mt-1 text-[11.5px] leading-snug text-fg-2">Off: {MODULE_LOSES[f.key]}</div>}
              </td>
              {data.tenants.map((t) => {
                const c = data.matrix[String(t.id)]?.[f.key];
                return (
                  <td key={t.id} className="border-b border-line px-3 py-3">
                    <span className="flex items-center gap-2" data-testid={`grid-${t.id}-${f.key}`}>
                      <Toggle checked={!!c?.enabled} onChange={(v) => void set(t.id, f.key, v)} />
                      {c?.overridden && (
                        <button type="button" className="text-fg-3 hover:text-ember" title="Back to the platform default" onClick={() => void set(t.id, f.key, null)}>
                          <RotateCcw className="size-3.5" />
                        </button>
                      )}
                    </span>
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
      {sw.dialog}
    </div>
  );
}

export function LiveModules() {
  const { data, error, reload } = useApi<FeatureCatalogue>("/api/owner/features");
  return (
    <div className="pb-10">
      <PageHeader title="Modules per tenant" subtitle="Switch PAMM, copy trading, prop, IB, algo, API and more for each brokerage (D112). Clients' navigation and APIs follow within seconds." actions={<Button variant="surface" onClick={reload}><RefreshCw /> Refresh</Button>} />
      {error ? <ErrorState error={error} onRetry={reload} /> : !data ? <TableSkeleton rows={6} /> : (
        <Card>
          <CardHeader title="Modules" icon={<Layers />} />
          <div className="px-4 pb-5 pt-4 sm:px-6">
            <Grid data={data} kind="module" reload={reload} />
          </div>
        </Card>
      )}
    </div>
  );
}

export function LiveFlags() {
  const { data, error, reload } = useApi<FeatureCatalogue>("/api/owner/features");
  const [open, setOpen] = React.useState(false);
  const [f, setF] = React.useState({ key: "", name: "", description: "", default_enabled: false });
  const [err, setErr] = React.useState<{ field?: string; message: string } | null>(null);
  return (
    <div className="pb-10">
      <PageHeader
        title="Feature flags"
        subtitle="Platform catalogue of flags with per-tenant overrides (D146). Tenants can also switch flags in their own Settings."
        actions={
          <>
            <Button variant="surface" onClick={reload}>
              <RefreshCw /> Refresh
            </Button>
            <Button variant="ember" onClick={() => (setF({ key: "", name: "", description: "", default_enabled: false }), setErr(null), setOpen(true))}>
              <Plus /> New flag
            </Button>
          </>
        }
      />
      {error ? <ErrorState error={error} onRetry={reload} /> : !data ? <TableSkeleton rows={6} /> : (
        <div className="space-y-4">
          <Card>
            <CardHeader title="Catalogue" icon={<Flag />} />
            <div className="divide-y divide-line px-4 pb-3 pt-2 sm:px-6">
              {data.features.filter((x) => x.kind === "flag").map((x) => (
                <div key={x.key} className="flex items-center justify-between gap-4 py-3">
                  <div className="min-w-0">
                    <div className="flex items-center gap-2 font-medium">
                      {x.name}
                      <code className="text-[11px] font-normal text-fg-3">{x.key}</code>
                      {!x.builtin && <Chip size="sm" tone="info">Custom</Chip>}
                    </div>
                    <div className="text-[12.5px] text-fg-3">{x.description || "No description"} · on for {x.tenants_on}/{data.tenants.length} tenants</div>
                  </div>
                  <div className="flex shrink-0 items-center gap-3">
                    {!x.builtin && (
                      <>
                        <span className="text-[12px] text-fg-3">Default</span>
                        <Toggle checked={x.default} onChange={async (v) => (await act("PATCH", `/api/owner/features/${x.key}`, { default_enabled: v }, "Default changed")) && reload()} />
                        <IconButton size="sm" aria-label={`Delete ${x.key}`} onClick={async () => (await act("DELETE", `/api/owner/features/${x.key}`, {}, "Flag deleted")) && reload()}>
                          <Trash2 />
                        </IconButton>
                      </>
                    )}
                    {x.builtin && <Chip size="sm">Default {x.default ? "on" : "off"}</Chip>}
                  </div>
                </div>
              ))}
            </div>
          </Card>
          <Card>
            <CardHeader title="Per tenant" />
            <div className="px-4 pb-5 pt-4 sm:px-6">
              <Grid data={data} kind="flag" reload={reload} />
            </div>
          </Card>
        </div>
      )}
      <Dialog open={open} onOpenChange={setOpen} title="New feature flag" description="Available to every tenant's apps in the tenant config; off by default unless you say otherwise.">
        <div className="space-y-4">
          {err && !err.field && <div className="rounded-[14px] border border-down/25 bg-down-soft px-4 py-3 text-[13px] text-down">{err.message}</div>}
          <Field label="Key" hint="lowercase_with_underscores" error={err?.field === "key" ? err.message : undefined}>
            <Input value={f.key} onChange={(e) => setF({ ...f, key: e.target.value.toLowerCase() })} placeholder="new_chart" className="font-mono" />
          </Field>
          <Field label="Name" error={err?.field === "name" ? err.message : undefined}>
            <Input value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} placeholder="New chart engine" />
          </Field>
          <Field label="Description">
            <Input value={f.description} onChange={(e) => setF({ ...f, description: e.target.value })} />
          </Field>
          <div className="flex items-center justify-between rounded-[14px] border border-line px-4 py-3">
            <span className="text-[13px]">On by default</span>
            <Toggle checked={f.default_enabled} onChange={(v) => setF({ ...f, default_enabled: v })} />
          </div>
          <div className="flex justify-end gap-2">
            <Button variant="ghost" onClick={() => setOpen(false)}>Cancel</Button>
            <Button
              variant="ember"
              onClick={async () => {
                const r = await call("POST", "/api/owner/features", f);
                if (!r.ok) return setErr(r.error);
                setOpen(false);
                reload();
              }}
            >
              Create flag
            </Button>
          </div>
        </div>
      </Dialog>
    </div>
  );
}

export function LiveSystem() {
  const now = useNow(5000);
  const { data, error, reload, loading } = useApi<{ checked_at: string; services: Probe[] }>("/api/owner/system", { refreshMs: 30_000 });
  const up = data?.services.filter((s) => s.status === "up").length ?? 0;
  const required = data?.services.filter((s) => !s.optional) ?? [];
  const down = required.filter((s) => s.status !== "up").length;
  return (
    <div className="pb-10">
      <PageHeader
        title="System status"
        subtitle="Health of every Kalks service, probed from the Back Office server every 30 seconds"
        actions={
          <>
            <Link href={(process.env.NEXT_PUBLIC_CRM_URL ?? "http://localhost:3000") + "/status"} target="_blank">
              <Button variant="surface">Public status page</Button>
            </Link>
            <Button variant="surface" onClick={reload} disabled={loading}>
              <RefreshCw /> Check now
            </Button>
          </>
        }
      />
      {error ? <ErrorState error={error} onRetry={reload} /> : !data ? <TableSkeleton rows={6} /> : (
        <div className="space-y-4">
          <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
            <KpiCard label="Services up" value={`${up} / ${data.services.length}`} icon={<Server />} chip={down ? `${down} need attention` : "all core services up"} chipTone={down ? "down" : "up"} />
            <KpiCard label="Slowest" value={`${Math.max(...data.services.filter((s) => s.status !== "down").map((s) => s.latency_ms), 0)} ms`} icon={<Activity />} />
            <KpiCard label="Checked" value={ago(data.checked_at, now)} />
          </div>
          <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3" data-testid="service-grid">
            {data.services.map((s) => (
              <Card key={s.key} className={cn(s.status === "down" && !s.optional && "border-down/40")}>
                <CardHeader
                  title={s.name}
                  subtitle={s.detail}
                  action={
                    <Chip size="sm" tone={s.status === "down" && s.optional ? "neutral" : STATUS_TONE[s.status] ?? "neutral"} dot>
                      {s.status === "down" && s.optional ? "Not running" : cap(s.status)}
                    </Chip>
                  }
                />
                <div className="px-4 pb-4 pt-3 sm:px-6">
                  <div className="mb-2 text-[12px] text-fg-3">
                    HTTP {s.http ?? "—"} · {s.latency_ms} ms
                  </div>
                  {Object.keys(s.facts).length ? (
                    <dl className="grid grid-cols-2 gap-x-3 gap-y-1 text-[12px]">
                      {Object.entries(s.facts).map(([k, v]) => (
                        <React.Fragment key={k}>
                          <dt className="truncate text-fg-3">{k}</dt>
                          <dd className="truncate font-mono text-fg-2" title={v}>{v}</dd>
                        </React.Fragment>
                      ))}
                    </dl>
                  ) : (
                    <EmptyState title="No response" illustration="satellite_antenna" className="py-2" />
                  )}
                </div>
              </Card>
            ))}
          </div>
          <p className="text-[12px] text-fg-3">Checked at {when(data.checked_at, true)} (server time). The public status page at /status on the Client Area shows the same checks without internal details.</p>
        </div>
      )}
    </div>
  );
}
