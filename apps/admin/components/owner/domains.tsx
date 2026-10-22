"use client";

import * as React from "react";
import { Globe, Plus } from "lucide-react";
import { toast } from "sonner";
import { Button, Chip, Input, Menu } from "@kalks/ui";
import { ago, useNow } from "@/components/live/kit";
import { STATUS_TONE, Select, act, call, cap } from "@/components/rbac/kit";
import { useConfirm } from "@/components/confirm";
import type { DomainKind, DomainRecord } from "./types";

export const DOMAIN_KINDS: { value: DomainKind; label: string; hint: string }[] = [
  { value: "website", label: "Website", hint: "Marketing site" },
  { value: "app", label: "Client Area", hint: "Sign-up, funding, accounts" },
  { value: "trade", label: "Trader", hint: "Web trading terminal" },
  { value: "admin", label: "Back Office", hint: "Staff console" },
];
const KIND_LABEL = Object.fromEntries(DOMAIN_KINDS.map((k) => [k.value, k.label])) as Record<DomainKind, string>;

/** Guess the app from the first label, like the gateway does for plain lists. */
function guessKind(domain: string): DomainKind {
  const first = domain.trim().toLowerCase().split(".")[0] ?? "";
  if (["app", "my", "client", "portal"].includes(first)) return "app";
  if (["trade", "trader", "webtrader"].includes(first)) return "trade";
  if (["admin", "backoffice", "bo"].includes(first)) return "admin";
  return "website";
}

/**
 * A broker's domains (gateway tenant_domains): one row per host with the app it serves. The gateway resolves
 * the broker from the visitor's host; Caddy issues a certificate on first visit for active domains only.
 */
export function TenantDomains({ tenantId, records, onChanged }: { tenantId: number; records: DomainRecord[]; onChanged: () => void }) {
  const now = useNow();
  const [domain, setDomain] = React.useState("");
  const [kind, setKind] = React.useState<DomainKind>("website");
  const [touched, setTouched] = React.useState(false);
  const [busy, setBusy] = React.useState(false);
  const [ask, confirmDialog] = useConfirm();
  const base = `/api/owner/tenants/${tenantId}/domains`;

  const add = async (e: React.FormEvent) => {
    e.preventDefault();
    const host = domain.trim().toLowerCase();
    const ok = await ask({
      title: `Add ${host}?`,
      text: `${host} becomes a live address for this broker's ${KIND_LABEL[kind]} straight away: visitors on it are routed to this broker, and a certificate is issued on the first visit once its DNS points at the platform server.`,
      confirm: "Add domain",
    });
    if (!ok) return;
    setBusy(true);
    const r = await call<{ domain: DomainRecord }>("POST", base, { domain, kind });
    setBusy(false);
    if (!r.ok) return void toast.error(r.error.message);
    toast.success(`${r.data.domain.domain} added`, { description: `Serves the ${KIND_LABEL[r.data.domain.kind]}. Point its DNS at the platform server.` });
    setDomain("");
    setTouched(false);
    onChanged();
  };

  const check = async (d: DomainRecord) => {
    const r = await call<{ resolves: boolean; verified: boolean; domain: DomainRecord }>("POST", `${base}/${d.id}/check`, {});
    if (!r.ok) return void toast.error(r.error.message);
    if (r.data.verified) toast.success(`${d.domain} points here`, { description: r.data.domain.dns_addresses.join(", ") });
    else if (r.data.resolves) toast.error(`${d.domain} resolves elsewhere`, { description: r.data.domain.dns_addresses.join(", ") });
    else toast.error(`${d.domain} has no DNS record yet`);
    onChanged();
  };

  return (
    <div>
      {confirmDialog}
      {records.length === 0 ? (
        <p className="rounded-[12px] border border-dashed border-line px-4 py-5 text-[13px] text-fg-3">No domains yet. Add the broker&apos;s website, Client Area, Trader and Back Office hosts.</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full min-w-[560px] text-[13px]" data-testid="tenant-domains">
            <thead>
              <tr className="border-b border-line text-left text-[11.5px] font-medium uppercase tracking-[0.06em] text-fg-3">
                <th className="py-2 pr-3 font-medium">Domain</th>
                <th className="py-2 pr-3 font-medium">App</th>
                <th className="py-2 pr-3 font-medium">Status</th>
                <th className="py-2 pr-3 font-medium">DNS</th>
                <th className="py-2 font-medium" />
              </tr>
            </thead>
            <tbody>
              {records.map((d) => (
                <tr key={d.id} className="border-b border-line last:border-0" data-domain={d.domain}>
                  <td className="py-2.5 pr-3">
                    <a href={`https://${d.domain}`} target="_blank" rel="noreferrer" className="font-mono text-[12.5px] text-fg hover:text-ember">
                      {d.domain}
                    </a>
                  </td>
                  <td className="py-2.5 pr-3">
                    <Select
                      value={d.kind}
                      aria-label={`App served on ${d.domain}`}
                      className="h-8 min-w-[132px] text-[12.5px]"
                      onChange={async (v) => {
                        if (v === d.kind) return;
                        const ok = await ask({
                          title: `Serve the ${KIND_LABEL[v as DomainKind]} on ${d.domain}?`,
                          text: `${d.domain} stops serving the ${KIND_LABEL[d.kind]} immediately and serves the ${KIND_LABEL[v as DomainKind]} instead. Anyone using ${d.domain} for the ${KIND_LABEL[d.kind]} (bookmarks, links, signed-in sessions) lands on the new app.`,
                          confirm: "Change app",
                          tone: "danger",
                        });
                        if (ok && (await act("PATCH", `${base}/${d.id}`, { kind: v }, `${d.domain}: ${KIND_LABEL[v as DomainKind]}`))) onChanged();
                      }}
                    >
                      {DOMAIN_KINDS.map((k) => (
                        <option key={k.value} value={k.value}>
                          {k.label}
                        </option>
                      ))}
                    </Select>
                  </td>
                  <td className="py-2.5 pr-3">
                    <Chip size="sm" tone={STATUS_TONE[d.status] ?? "neutral"} dot>
                      {cap(d.status)}
                    </Chip>
                  </td>
                  <td className="py-2.5 pr-3 text-[12px] text-fg-3">
                    {d.verified_at ? (
                      <span className="text-up">Points here</span>
                    ) : d.dns_checked_at ? (
                      <span className="text-warn">{d.dns_addresses.length ? `Resolves to ${d.dns_addresses.slice(0, 2).join(", ")}` : "No record"}</span>
                    ) : (
                      "Not checked"
                    )}
                    {d.dns_checked_at && <span className="ml-1.5 text-fg-3">· {ago(d.dns_checked_at, now)}</span>}
                  </td>
                  <td className="py-2.5 text-right">
                    <Menu
                      align="end"
                      items={[
                        { label: "Check DNS", onSelect: () => void check(d) },
                        d.status === "active"
                          ? {
                              label: "Disable",
                              onSelect: async () => {
                                const ok = await ask({
                                  title: `Disable ${d.domain}?`,
                                  text: `${d.domain} stops serving the ${KIND_LABEL[d.kind]} immediately: visitors can no longer reach this broker on it and no new certificate is issued. You can enable it again later.`,
                                  confirm: "Disable domain",
                                  tone: "danger",
                                });
                                if (ok && (await act("PATCH", `${base}/${d.id}`, { status: "disabled" }, `${d.domain} disabled`))) onChanged();
                              },
                            }
                          : { label: "Enable", onSelect: async () => (await act("PATCH", `${base}/${d.id}`, { status: "active" }, `${d.domain} enabled`)) && onChanged() },
                        "sep",
                        {
                          label: "Remove",
                          danger: true,
                          onSelect: async () => {
                            const ok = await ask({
                              title: `Remove ${d.domain}?`,
                              text: `${d.domain} is deleted from this broker and stops serving the ${KIND_LABEL[d.kind]} immediately. Its DNS check history is lost; to use it again you'll have to add it back.`,
                              confirm: "Remove domain",
                              tone: "danger",
                            });
                            if (ok && (await act("DELETE", `${base}/${d.id}`, {}, `${d.domain} removed`))) onChanged();
                          },
                        },
                      ]}
                      trigger={
                        <Button size="xs" variant="surface" aria-label={`Actions for ${d.domain}`}>
                          Manage
                        </Button>
                      }
                    />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <form onSubmit={add} className="mt-4 grid gap-2 sm:grid-cols-[1fr_170px_auto]">
        <Input
          name="new-domain"
          placeholder="app.broker.com"
          value={domain}
          className="font-mono"
          leading={<Globe className="size-4 text-fg-3" />}
          onChange={(e) => {
            setDomain(e.target.value);
            if (!touched) setKind(guessKind(e.target.value));
          }}
        />
        <Select
          name="new-domain-kind"
          value={kind}
          onChange={(v) => {
            setKind(v as DomainKind);
            setTouched(true);
          }}
        >
          {DOMAIN_KINDS.map((k) => (
            <option key={k.value} value={k.value}>
              {k.label}
            </option>
          ))}
        </Select>
        <Button type="submit" variant="surface" disabled={busy || domain.trim().length < 4}>
          <Plus /> Add domain
        </Button>
      </form>
      <p className="mt-2 text-[12px] text-fg-3">Point each domain&apos;s DNS (A record) at the platform server. The certificate is issued on the first visit once the domain is active.</p>
    </div>
  );
}
