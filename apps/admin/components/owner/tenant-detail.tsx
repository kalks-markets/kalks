"use client";

import * as React from "react";
import Link from "next/link";
import { ArrowLeft, Ban, Building2, Globe, Mail, Play, Save } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, CardHeader, Chip, Dialog, EmptyState, Field, Input, KeyValue, KpiCard, PageHeader, Toggle, formatNumber } from "@kalks/ui";
import { ErrorState, TableSkeleton, ago, day, useApi, useNow } from "@/components/live/kit";
import { InviteLink, STATUS_TONE, act, call, cap, money, pct } from "@/components/rbac/kit";
import { TenantMark } from "./tenants";
import { BillingForm, InvoiceTable } from "./billing";
import { TenantDomains } from "./domains";
import type { TenantDetail } from "./types";
import { MODULE_LOSES, useModuleSwitch } from "./module-switch";

export function LiveTenantDetail({ id }: { id: string }) {
  const now = useNow();
  const { data, error, reload } = useApi<TenantDetail>(`/api/owner/tenants/${id}`);
  const [edit, setEdit] = React.useState({ name: "", logo_url: "", contact_email: "", primary: "", accent: "", max_clients: "", max_staff: "" });
  const [suspend, setSuspend] = React.useState(false);
  const [reason, setReason] = React.useState("");
  const [invite, setInvite] = React.useState({ email: "", name: "" });
  const [link, setLink] = React.useState<string | undefined>();
  // a module switch asks to confirm with a reason (audited) and says what the clients lose
  const sw = useModuleSwitch(reload);
  React.useEffect(() => {
    if (data) {
      const t = data.tenant;
      setEdit({ name: t.name, logo_url: t.brand.logo_url ?? "", contact_email: t.contact_email ?? "", primary: t.brand.primary ?? "", accent: t.brand.accent ?? "", max_clients: String(t.limits.max_clients ?? 0), max_staff: String(t.limits.max_staff ?? 0) });
    }
  }, [data]);
  if (error) return <ErrorState error={error} onRetry={reload} />;
  if (!data) return <TableSkeleton rows={6} />;
  const t = data.tenant;
  const toggleModule = (key: string, v: boolean) => {
    const m = data.modules.find((x) => x.key === key);
    sw.ask({ tenantId: t.id, tenantName: t.name, key, name: m?.name ?? key, value: v, effective: v });
  };

  return (
    <div className="pb-10">
      <Link href="/brokers" className="mb-3 inline-flex items-center gap-1.5 text-[13px] text-fg-3 hover:text-fg">
        <ArrowLeft className="size-4" /> Tenants
      </Link>
      <PageHeader
        title={
          <span className="flex items-center gap-3">
            <TenantMark name={t.name} color={t.brand.primary} /> {t.name}
          </span>
        }
        subtitle={`${t.legal_name ?? t.slug} · ${t.slug} · since ${day(t.created_at)}`}
        actions={
          <>
            <Chip tone={STATUS_TONE[t.status] ?? "neutral"} dot>
              <span data-testid="tenant-status">{cap(t.status)}</span>
            </Chip>
            {!t.is_owner_tenant &&
              (t.status === "active" ? (
                <Button variant="surface" onClick={() => setSuspend(true)}>
                  <Ban /> Suspend
                </Button>
              ) : (
                <Button variant="ember" onClick={async () => (await act("POST", `/api/owner/tenants/${t.id}/activate`, {}, "Tenant re-activated")) && reload()}>
                  <Play /> Re-activate
                </Button>
              ))}
          </>
        }
      />
      {t.status === "suspended" && (
        <div className="mb-4 rounded-[14px] border border-down/25 bg-down-soft px-4 py-3 text-[13px] text-fg">
          Suspended {ago(t.suspended_at, now)}: {t.suspended_reason}. Clients see an unavailable page and nobody at this broker can sign in.
        </div>
      )}
      <div className="mb-4 grid grid-cols-2 gap-4 lg:grid-cols-4">
        <KpiCard label="Clients" value={formatNumber(t.clients, 0)} chip={`+${t.clients_30d} in 30d`} chipTone="up" icon={<Building2 />} />
        <KpiCard label="KYC verified" value={t.kyc_verified} />
        <KpiCard label="Staff" value={t.staff} chip={t.limits.max_staff ? `limit ${t.limits.max_staff}` : "no limit"} />
        <KpiCard label="Licence" value={money(data.billing.monthly_licence_cents, data.billing.currency)} chip={`+ ${pct(data.billing.revenue_share_bps)} revenue share`} />
      </div>
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Card className="xl:col-span-7">
          <CardHeader title="Profile & branding" subtitle="Shown on this broker's sign-in pages and app shells" />
          <div className="grid gap-4 px-4 pb-5 pt-4 sm:grid-cols-2 sm:px-6">
            <Field label="Brand name">
              <Input value={edit.name} onChange={(e) => setEdit({ ...edit, name: e.target.value })} />
            </Field>
            <Field label="Support email">
              <Input value={edit.contact_email} onChange={(e) => setEdit({ ...edit, contact_email: e.target.value })} placeholder="support@broker.com" />
            </Field>
            <Field label="Logo URL (https, SVG or PNG)" className="sm:col-span-2">
              <Input
                value={edit.logo_url}
                onChange={(e) => setEdit({ ...edit, logo_url: e.target.value })}
                placeholder="https://cdn.broker.com/logo.svg"
                className="font-mono"
                // eslint-disable-next-line @next/next/no-img-element
                leading={edit.logo_url.startsWith("https://") ? <img src={edit.logo_url} alt="" className="h-4 w-auto max-w-10 object-contain" /> : undefined}
              />
            </Field>
            <Field label="Primary colour">
              <Input value={edit.primary} onChange={(e) => setEdit({ ...edit, primary: e.target.value })} className="font-mono" leading={<span className="size-4 rounded" style={{ background: edit.primary }} />} />
            </Field>
            <Field label="Accent colour">
              <Input value={edit.accent} onChange={(e) => setEdit({ ...edit, accent: e.target.value })} className="font-mono" leading={<span className="size-4 rounded" style={{ background: edit.accent }} />} />
            </Field>
            <Field label="Max clients (0 = unlimited)">
              <Input value={edit.max_clients} onChange={(e) => setEdit({ ...edit, max_clients: e.target.value })} />
            </Field>
            <Field label="Max staff (0 = unlimited)">
              <Input value={edit.max_staff} onChange={(e) => setEdit({ ...edit, max_staff: e.target.value })} />
            </Field>
            <div className="sm:col-span-2">
              <Button
                variant="ember"
                onClick={async () =>
                  (await act(
                    "PATCH",
                    `/api/owner/tenants/${t.id}`,
                    {
                      name: edit.name,
                      contact_email: edit.contact_email.trim(),
                      brand: { primary: edit.primary, accent: edit.accent, logo_url: edit.logo_url.trim() },
                      limits: { ...t.limits, max_clients: Number(edit.max_clients) || 0, max_staff: Number(edit.max_staff) || 0 },
                    },
                    "Tenant saved",
                  )) && reload()
                }
              >
                <Save /> Save changes
              </Button>
            </div>
          </div>
        </Card>
        <Card className="xl:col-span-5">
          <CardHeader title="Modules" subtitle="What this broker's clients can use" />
          <div className="px-4 pb-4 pt-2 sm:px-6">
            {data.modules.map((m) => (
              <div key={m.key} className="flex items-center justify-between gap-3 border-b border-line py-2.5 last:border-0">
                <div className="min-w-0">
                  <div className="text-[13.5px] font-medium">{m.name}</div>
                  <div className="text-[12px] leading-snug text-fg-3">{m.enabled ? m.description : `Off: ${MODULE_LOSES[m.key] ?? m.description}`}</div>
                </div>
                <span data-testid={`module-${m.key}`}>
                  <Toggle checked={m.enabled} onChange={(v) => void toggleModule(m.key, v)} />
                </span>
              </div>
            ))}
          </div>
        </Card>
        <Card className="xl:col-span-12">
          <CardHeader title="Domains" subtitle="Each host serves one app; the broker is recognised from the host its clients and staff open" icon={<Globe />} />
          <div className="px-4 pb-5 pt-3 sm:px-6">
            <TenantDomains tenantId={t.id} records={t.domain_records ?? []} onChanged={reload} />
          </div>
        </Card>
        <Card className="xl:col-span-7">
          <CardHeader title="Billing plan" subtitle="Setup fee + monthly licence + revenue share (tracking only)" />
          <div className="px-4 pb-5 pt-4 sm:px-6">
            <BillingForm tenantId={t.id} billing={data.billing} onSaved={reload} />
          </div>
        </Card>
        <Card className="xl:col-span-5">
          <CardHeader title="Super Admins" subtitle="Full access to this broker's Back Office" icon={<Mail />} />
          <div className="px-4 pb-5 pt-3 sm:px-6">
            {data.admins.length === 0 ? (
              <EmptyState title="No Super Admin yet" illustration="busts_in_silhouette" />
            ) : (
              <KeyValue rows={data.admins.map((a) => [`${a.name} · ${a.email}`, a.status === "invited" ? "Invite pending" : a.last_login_at ? `signed in ${ago(a.last_login_at, now)}` : cap(a.status)])} />
            )}
            {t.status === "active" && (
              <form
                className="mt-4 grid gap-2 sm:grid-cols-[1fr_1fr_auto]"
                onSubmit={async (e) => {
                  e.preventDefault();
                  const r = await call<{ invite: { dev_invite_url?: string } }>("POST", `/api/owner/tenants/${t.id}/invite-admin`, invite);
                  if (!r.ok) return void toast.error(r.error.message);
                  setLink(r.data.invite.dev_invite_url);
                  setInvite({ email: "", name: "" });
                  reload();
                }}
              >
                <Input placeholder="email" value={invite.email} onChange={(e) => setInvite({ ...invite, email: e.target.value })} />
                <Input placeholder="name" value={invite.name} onChange={(e) => setInvite({ ...invite, name: e.target.value })} />
                <Button type="submit" variant="surface" disabled={!invite.email || !invite.name}>
                  Invite
                </Button>
              </form>
            )}
            <InviteLink url={link} />
          </div>
        </Card>
        <Card className="xl:col-span-12">
          <CardHeader title="Invoices" />
          <div className="px-4 pb-5 pt-4 sm:px-6">
            <InvoiceTable items={data.invoices} onChanged={reload} hideTenant />
          </div>
        </Card>
      </div>
      {sw.dialog}
      <Dialog open={suspend} onOpenChange={setSuspend} title={`Suspend ${t.name}?`} description="Every session at this broker ends now. Clients see an unavailable page and nobody can sign in until you re-activate it.">
        <Field label="Reason (audit log)">
          <Input value={reason} onChange={(e) => setReason(e.target.value)} placeholder="e.g. unpaid invoices" name="suspend-reason" />
        </Field>
        <div className="mt-5 flex justify-end gap-2">
          <Button variant="ghost" onClick={() => setSuspend(false)}>
            Cancel
          </Button>
          <Button
            variant="ember"
            disabled={reason.trim().length < 3}
            onClick={async () => {
              if (await act("POST", `/api/owner/tenants/${t.id}/suspend`, { reason }, "Tenant suspended")) {
                setSuspend(false);
                reload();
              }
            }}
          >
            Suspend tenant
          </Button>
        </div>
      </Dialog>
    </div>
  );
}
