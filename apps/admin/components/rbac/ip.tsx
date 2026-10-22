"use client";

import * as React from "react";
import { Globe2, Plus, ShieldAlert, ShieldCheck, Trash2, Wifi } from "lucide-react";
import { Button, Card, CardHeader, Chip, DataTable, EmptyState, Field, IconButton, Input, KpiCard, PageHeader, Toggle, type Column } from "@kalks/ui";
import { useCan, useStaff } from "@/components/staff-session";
import { ErrorState, TableSkeleton, ago, useApi, useNow, when } from "@/components/live/kit";
import { validateCidr, cidrSize } from "@/components/security/cidr";
import { useConfirm } from "@/components/confirm";
import { act, call } from "./kit";

type Entry = { id: number; cidr: string; label: string; created_at: string; created_by: string | null; last_sign_in_at: string | null };
type Resp = { enabled: boolean; owner_bypass: boolean; items: Entry[]; your_ip: string; your_ip_allowed: boolean; blocked_24h: number; can_edit: boolean };

export function LiveIpAllowlist() {
  const now = useNow();
  const me = useStaff();
  const canRead = useCan("security.read");
  const { data, error, reload } = useApi<Resp>(canRead ? "/api/admin/security/ip" : null);
  const [cidr, setCidr] = React.useState("");
  const [label, setLabel] = React.useState("");
  const [err, setErr] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [ask, confirmDialog] = useConfirm();

  if (!canRead) return <EmptyState title="Not available for your role" text="Your role doesn't include Back Office security settings." illustration="locked" />;
  const canEdit = !!data?.can_edit;

  async function add(value = cidr, name = label) {
    const v = value.trim();
    const local = v.includes(":") ? null : validateCidr(v);
    if (local) return setErr(local);
    setBusy(true);
    const r = await call("POST", "/api/admin/security/ip", { cidr: v, label: name });
    setBusy(false);
    if (!r.ok) return setErr(r.error.message);
    setErr(null);
    setCidr("");
    setLabel("");
    reload();
  }

  const columns: Column<Entry>[] = [
    { key: "cidr", header: "Address / range", cell: (e) => <span className="font-mono text-[13px]">{e.cidr}</span>, sort: (e) => e.cidr },
    { key: "size", header: "Covers", hideOn: "md", cell: (e) => <span className="text-fg-3">{cidrSize(e.cidr)}</span> },
    { key: "label", header: "Label", cell: (e) => e.label || <span className="text-fg-3">—</span> },
    { key: "last", header: "Last staff sign-in", hideOn: "lg", cell: (e) => <span className="text-fg-2" title={when(e.last_sign_in_at)}>{e.last_sign_in_at ? ago(e.last_sign_in_at, now) : "none in 30 days"}</span> },
    { key: "by", header: "Added", hideOn: "xl", cell: (e) => <span className="text-fg-3">{e.created_by ?? "—"} · {ago(e.created_at, now)}</span> },
    {
      key: "x",
      header: "",
      align: "right",
      cell: (e) =>
        canEdit ? (
          <IconButton size="sm" aria-label={`Remove ${e.cidr}`} onClick={async () => {
              const ok = await ask({
                title: `Remove ${e.cidr}?`,
                text: (
                  <>
                    <span className="font-mono">{e.cidr}</span>
                    {e.label ? ` (${e.label})` : ""} comes off the allow-list straight away.
                    {data?.enabled ? " While the allow-list is enforced, staff connecting from this address can no longer sign in, and their signed-in sessions stop working on their next request." : " The allow-list is off, so nobody is blocked until it's switched on."}
                  </>
                ),
                confirm: "Remove address",
                tone: "danger",
              });
              if (ok && (await act("DELETE", `/api/admin/security/ip/${e.id}`, {}, "Removed", e.cidr))) reload();
            }}>
            <Trash2 />
          </IconButton>
        ) : null,
    },
  ];

  return (
    <div className="pb-10">
      {confirmDialog}
      <PageHeader title="IP allow-list" subtitle="Limit Back Office sign-in and every staff request to your office and VPN addresses" />
      {error ? (
        <ErrorState error={error} onRetry={reload} />
      ) : !data ? (
        <TableSkeleton rows={4} />
      ) : (
        <>
          <div className="mb-4 grid grid-cols-2 gap-4 lg:grid-cols-4">
            <KpiCard label="Allow-list" value={data.enabled ? "On" : "Off"} icon={data.enabled ? <ShieldCheck /> : <ShieldAlert />} chip={data.enabled ? "enforced" : "not enforced"} chipTone={data.enabled ? "up" : "warn"} />
            <KpiCard label="Entries" value={data.items.length} icon={<Globe2 />} />
            <KpiCard label="Your IP" value={<span className="font-mono text-[18px]" data-testid="your-ip">{data.your_ip}</span>} chip={data.your_ip_allowed ? "on the list" : "not listed"} chipTone={data.your_ip_allowed ? "up" : "warn"} icon={<Wifi />} />
            <KpiCard label="Blocked, 24h" value={data.blocked_24h} chipTone={data.blocked_24h ? "down" : "neutral"} />
          </div>
          <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
            <Card className="xl:col-span-8">
              <CardHeader title="Allowed addresses" subtitle="IPv4 or IPv6, single address or CIDR range" />
              <div className="px-4 pb-5 pt-4 sm:px-6">
                {canEdit && (
                  <form
                    className="mb-4 grid gap-3 sm:grid-cols-[1fr_1fr_auto]"
                    onSubmit={(e) => {
                      e.preventDefault();
                      void add();
                    }}
                  >
                    <Field label="IP or range" error={err ?? undefined}>
                      <Input value={cidr} onChange={(e) => setCidr(e.target.value)} placeholder="203.0.113.0/24" name="cidr" className="font-mono" />
                    </Field>
                    <Field label="Label">
                      <Input value={label} onChange={(e) => setLabel(e.target.value)} placeholder="Dubai office" name="cidr-label" />
                    </Field>
                    <div className="flex items-end">
                      <Button type="submit" variant="ember" disabled={busy || !cidr.trim()} className="h-11">
                        <Plus /> Add
                      </Button>
                    </div>
                  </form>
                )}
                {canEdit && !data.your_ip_allowed && data.your_ip !== "unknown" && (
                  <div className="mb-4 flex flex-wrap items-center justify-between gap-3 rounded-[14px] border border-warn/25 bg-warn-soft px-4 py-3 text-[13px] text-fg">
                    Your current IP {data.your_ip} isn&apos;t on the list.
                    <Button size="xs" variant="surface" onClick={() => void add(data.your_ip, "Added from my connection")}>
                      Add my IP
                    </Button>
                  </div>
                )}
                <DataTable rows={data.items} columns={columns} rowKey={(e) => String(e.id)} pageSize={50} empty={<EmptyState title="No addresses yet" text="Add your office or VPN range before switching the allow-list on." illustration="package" />} />
              </div>
            </Card>
            <Card className="xl:col-span-4">
              <CardHeader title="Enforcement" subtitle="Checked at sign-in and on every Back Office request" />
              <div className="space-y-5 px-4 pb-5 pt-4 sm:px-6">
                <div className="flex items-start justify-between gap-4">
                  <div>
                    <div className="font-medium">Enforce the allow-list</div>
                    <div className="text-[12.5px] text-fg-3">Staff outside these addresses can&apos;t sign in, and signed-in sessions stop working when they move off-network.</div>
                  </div>
                  <span data-testid="ip-enforce">
                    <Toggle checked={data.enabled} onChange={async (v) => canEdit && (await act("PUT", "/api/admin/security/ip/settings", { enabled: v }, v ? "Allow-list on" : "Allow-list off")) && reload()} />
                  </span>
                </div>
                <div className="flex items-start justify-between gap-4">
                  <div>
                    <div className="font-medium">Platform Owner bypass</div>
                    <div className="text-[12.5px] text-fg-3">The platform owner can reach this Back Office from anywhere, for support. Only the owner can change this.</div>
                  </div>
                  <Toggle checked={data.owner_bypass} onChange={async (v) => me.is_owner && (await act("PUT", "/api/admin/security/ip/settings", { owner_bypass: v }, "Saved")) && reload()} />
                </div>
                <div className="flex flex-wrap gap-2 text-[12px] text-fg-3">
                  <Chip size="sm">Blocked attempts are audited as security.ip_blocked</Chip>
                  <Chip size="sm">You can&apos;t remove the entry you&apos;re connected through</Chip>
                </div>
              </div>
            </Card>
          </div>
        </>
      )}
    </div>
  );
}
