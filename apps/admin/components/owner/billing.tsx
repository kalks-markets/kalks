"use client";

import * as React from "react";
import Link from "next/link";
import { Coins, FilePlus2, Receipt, RefreshCw } from "lucide-react";
import { Button, Card, CardHeader, Chip, DataTable, Dialog, EmptyState, Field, Input, KpiCard, Menu, PageHeader, Segmented, Toggle, type Column } from "@kalks/ui";
import { ErrorState, TableSkeleton, day, useApi } from "@/components/live/kit";
import { STATUS_TONE, Select, act, cap, money, pct } from "@/components/rbac/kit";
import { useConfirmReason } from "@/components/confirm-reason";
import type { Billing, Invoice } from "./types";

const NEXT: Record<string, { to: string; label: string }[]> = {
  draft: [
    { to: "issued", label: "Issue" },
    { to: "void", label: "Void" },
  ],
  issued: [
    { to: "paid", label: "Mark paid" },
    { to: "overdue", label: "Mark overdue" },
    { to: "void", label: "Void" },
  ],
  overdue: [
    { to: "paid", label: "Mark paid" },
    { to: "void", label: "Void" },
  ],
};

export function BillingForm({ tenantId, billing, onSaved }: { tenantId: number; billing: Billing; onSaved: () => void }) {
  const [f, setF] = React.useState({
    currency: billing.currency,
    setup_fee: String(billing.setup_fee_cents / 100),
    monthly_licence: String(billing.monthly_licence_cents / 100),
    revenue_share_pct: String(billing.revenue_share_bps / 100),
    billing_email: billing.billing_email ?? "",
    payment_terms_days: String(billing.payment_terms_days),
    notes: billing.notes,
  });
  const [busy, setBusy] = React.useState(false);
  return (
    <form
      className="grid gap-4 sm:grid-cols-3"
      onSubmit={async (e) => {
        e.preventDefault();
        setBusy(true);
        const ok = await act(
          "PUT",
          `/api/owner/tenants/${tenantId}/billing`,
          {
            currency: f.currency,
            setup_fee: Number(f.setup_fee) || 0,
            monthly_licence: Number(f.monthly_licence) || 0,
            revenue_share_pct: Number(f.revenue_share_pct) || 0,
            billing_email: f.billing_email || undefined,
            payment_terms_days: Number(f.payment_terms_days) || 0,
            notes: f.notes,
          },
          "Billing plan saved",
        );
        setBusy(false);
        if (ok) onSaved();
      }}
    >
      <Field label="Currency">
        <Select value={f.currency} onChange={(v) => setF({ ...f, currency: v })}>
          {["USD", "EUR", "GBP", "USDT", "AED", "INR"].map((c) => (
            <option key={c}>{c}</option>
          ))}
        </Select>
      </Field>
      <Field label="Setup fee">
        <Input inputMode="decimal" value={f.setup_fee} onChange={(e) => setF({ ...f, setup_fee: e.target.value })} />
      </Field>
      <Field label="Monthly licence">
        <Input inputMode="decimal" value={f.monthly_licence} onChange={(e) => setF({ ...f, monthly_licence: e.target.value })} />
      </Field>
      <Field label="Revenue share %">
        <Input inputMode="decimal" value={f.revenue_share_pct} onChange={(e) => setF({ ...f, revenue_share_pct: e.target.value })} />
      </Field>
      <Field label="Billing email">
        <Input type="email" value={f.billing_email} onChange={(e) => setF({ ...f, billing_email: e.target.value })} />
      </Field>
      <Field label="Payment terms (days)">
        <Input inputMode="numeric" value={f.payment_terms_days} onChange={(e) => setF({ ...f, payment_terms_days: e.target.value })} />
      </Field>
      <Field label="Notes" className="sm:col-span-3">
        <Input value={f.notes} onChange={(e) => setF({ ...f, notes: e.target.value })} placeholder="Contract reference, discounts…" />
      </Field>
      <div className="sm:col-span-3">
        <Button type="submit" variant="ember" disabled={busy}>
          {billing.configured ? "Save plan" : "Set up billing"}
        </Button>
      </div>
    </form>
  );
}

export function InvoiceTable({ items, onChanged, hideTenant }: { items: Invoice[]; onChanged: () => void; hideTenant?: boolean }) {
  const [askReason, reasonDialog] = useConfirmReason();
  async function move(i: Invoice, to: string) {
    let note: string | undefined;
    if (to === "paid" || to === "void") {
      const total = money(i.total_cents, i.currency);
      const why = await askReason(
        to === "paid"
          ? {
              title: `Mark ${i.number} as paid?`,
              text: `${i.tenant.name}'s invoice for ${total} is recorded as paid today and drops out of outstanding. Paid invoices can't be changed again, so check the money has actually arrived.`,
              label: "Payment reference",
              placeholder: "Bank transfer ref, tx hash or receipt number",
              confirm: "Mark paid",
            }
          : {
              title: `Void ${i.number}?`,
              text: `${i.tenant.name}'s invoice for ${total} is cancelled for good: it no longer counts as owed and can't be reissued, paid or edited. Create a new invoice if the amount was wrong.`,
              placeholder: "Why is this invoice being voided?",
              confirm: "Void invoice",
              tone: "danger",
            },
      );
      if (why === null) return;
      note = why;
    }
    if (await act("POST", `/api/owner/invoices/${i.id}/status`, { status: to, ...(note ? { note } : {}) }, `${i.number}: ${to}`)) onChanged();
  }
  const columns: Column<Invoice>[] = [
    { key: "n", header: "Invoice", cell: (i) => <span className="font-mono text-[12.5px]">{i.number}</span>, sort: (i) => i.number, csv: (i) => i.number },
    ...(hideTenant ? [] : [{ key: "t", header: "Tenant", cell: (i: Invoice) => <Link href={`/brokers/${i.tenant.id}`} className="hover:text-ember">{i.tenant.name}</Link>, sort: (i: Invoice) => i.tenant.name, csv: (i: Invoice) => i.tenant.name }]),
    { key: "p", header: "Period", hideOn: "md", cell: (i) => <span className="whitespace-nowrap text-fg-2">{day(i.period_start)} – {day(i.period_end)}</span>, csv: (i) => `${i.period_start}..${i.period_end}` },
    {
      key: "b",
      header: "Breakdown",
      hideOn: "lg",
      cell: (i) => (
        <span className="text-[12px] text-fg-3">
          {i.setup_fee_cents ? `setup ${money(i.setup_fee_cents, i.currency)} · ` : ""}licence {money(i.licence_cents, i.currency)} · {pct(i.revenue_share_bps)} of {money(i.revenue_base_cents, i.currency)} = {money(i.revenue_share_cents, i.currency)}
          {i.adjustment_cents ? ` · adj ${money(i.adjustment_cents, i.currency)}` : ""}
        </span>
      ),
    },
    { key: "a", header: "Total", align: "right", cell: (i) => <span className="k-num font-medium">{money(i.total_cents, i.currency)}</span>, sort: (i) => i.total_cents, csv: (i) => (i.total_cents / 100).toFixed(2) },
    { key: "d", header: "Due", align: "right", hideOn: "md", cell: (i) => <span className="text-fg-3">{i.status === "paid" ? `paid ${day(i.paid_at)}` : day(i.due_at)}</span> },
    { key: "s", header: "Status", cell: (i) => <Chip size="sm" tone={STATUS_TONE[i.status] ?? "neutral"} dot>{cap(i.status)}</Chip>, csv: (i) => i.status },
    {
      key: "x",
      header: "",
      align: "right",
      cell: (i) =>
        NEXT[i.status] ? (
          <Menu
            align="end"
            items={NEXT[i.status]!.map((n) => ({ label: n.label, danger: n.to === "void", onSelect: () => void move(i, n.to) }))}
            trigger={
              <Button size="xs" variant="surface">
                Update
              </Button>
            }
          />
        ) : null,
    },
  ];
  return (
    <>
      {reasonDialog}
      <DataTable rows={items} columns={columns} rowKey={(i) => String(i.id)} pageSize={20} exportName="invoices" empty={<EmptyState title="No invoices yet" illustration="receipt" />} />
    </>
  );
}

type Overview = {
  items: { tenant: { id: number; slug: string; name: string; status: string }; billing: Billing; outstanding_cents: number; paid_cents: number; last_period_end: string | null; setup_invoiced: boolean }[];
  totals: { mrr_cents: number; outstanding_cents: number; paid_cents: number };
  invoices: Invoice[];
};

function monthBounds(offset = 0) {
  const n = new Date();
  const s = new Date(Date.UTC(n.getUTCFullYear(), n.getUTCMonth() + offset, 1));
  const e = new Date(Date.UTC(n.getUTCFullYear(), n.getUTCMonth() + offset + 1, 0));
  return [s.toISOString().slice(0, 10), e.toISOString().slice(0, 10)] as const;
}

function NewInvoice({ open, onOpenChange, data, onDone }: { open: boolean; onOpenChange: (o: boolean) => void; data: Overview; onDone: () => void }) {
  const billable = data.items.filter((i) => i.billing.configured);
  const [tenant, setTenant] = React.useState("");
  const [start, setStart] = React.useState(monthBounds()[0]);
  const [end, setEnd] = React.useState(monthBounds()[1]);
  const [base, setBase] = React.useState("0");
  const [adj, setAdj] = React.useState("0");
  const [setup, setSetup] = React.useState(false);
  const [issue, setIssue] = React.useState(true);
  const row = billable.find((i) => String(i.tenant.id) === tenant);
  React.useEffect(() => {
    if (open) {
      const first = billable[0];
      setTenant(first ? String(first.tenant.id) : "");
      setSetup(first ? !first.setup_invoiced && first.billing.setup_fee_cents > 0 : false);
      setBase("0");
      setAdj("0");
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);
  const b = row?.billing;
  const months = Math.max(1, Math.round((Date.parse(end) - Date.parse(start) + 86400000) / 86400000 / 30.44));
  const total = b ? (setup ? b.setup_fee_cents : 0) + b.monthly_licence_cents * months + Math.round(((Number(base) || 0) * b.revenue_share_bps) / 100) + Math.round((Number(adj) || 0) * 100) : 0;
  return (
    <Dialog open={open} onOpenChange={onOpenChange} title="New invoice" description="Computed from the tenant's billing plan. Tracking only: send and collect it outside the platform.">
      {billable.length === 0 ? (
        <EmptyState title="Set up a billing plan first" text="Open a tenant and fill in its billing plan." illustration="receipt" />
      ) : (
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="Tenant" className="sm:col-span-2">
            <Select value={tenant} onChange={(v) => { setTenant(v); const r = billable.find((i) => String(i.tenant.id) === v); setSetup(!!r && !r.setup_invoiced && r.billing.setup_fee_cents > 0); }}>
              {billable.map((i) => (
                <option key={i.tenant.id} value={i.tenant.id}>
                  {i.tenant.name}
                </option>
              ))}
            </Select>
          </Field>
          <Field label="Period start">
            <Input type="date" value={start} onChange={(e) => setStart(e.target.value)} />
          </Field>
          <Field label="Period end">
            <Input type="date" value={end} onChange={(e) => setEnd(e.target.value)} />
          </Field>
          <Field label={`Broker revenue for the period (${b?.currency ?? "USD"})`} hint={b ? `${pct(b.revenue_share_bps)} share` : undefined}>
            <Input inputMode="decimal" value={base} onChange={(e) => setBase(e.target.value)} />
          </Field>
          <Field label="Adjustment (+/-)">
            <Input inputMode="decimal" value={adj} onChange={(e) => setAdj(e.target.value)} />
          </Field>
          <div className="flex items-center justify-between rounded-[14px] border border-line px-4 py-3">
            <span className="text-[13px]">Include setup fee {b ? `(${money(b.setup_fee_cents, b.currency)})` : ""}</span>
            <Toggle checked={setup} onChange={setSetup} />
          </div>
          <div className="flex items-center justify-between rounded-[14px] border border-line px-4 py-3">
            <span className="text-[13px]">Issue now (else draft)</span>
            <Toggle checked={issue} onChange={setIssue} />
          </div>
          <div className="flex items-center justify-between rounded-[14px] bg-surface-2 px-4 py-3 sm:col-span-2">
            <span className="text-[13px] text-fg-3">
              {months} month{months > 1 ? "s" : ""} licence · estimated total
            </span>
            <span className="k-num text-[18px] font-medium">{money(total, b?.currency)}</span>
          </div>
          <div className="flex justify-end gap-2 sm:col-span-2">
            <Button variant="ghost" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button
              variant="ember"
              disabled={!tenant}
              onClick={async () => {
                const ok = await act("POST", "/api/owner/invoices", { tenant_id: Number(tenant), period_start: start, period_end: end, include_setup_fee: setup, revenue_base: Number(base) || 0, adjustment: Number(adj) || 0, issue }, "Invoice created");
                if (ok) {
                  onDone();
                  onOpenChange(false);
                }
              }}
            >
              Create invoice
            </Button>
          </div>
        </div>
      )}
    </Dialog>
  );
}

export function LiveBilling() {
  const { data, error, reload } = useApi<Overview>("/api/owner/billing");
  const [status, setStatus] = React.useState<"all" | Invoice["status"]>("all");
  const [creating, setCreating] = React.useState(false);
  const invoices = (data?.invoices ?? []).filter((i) => status === "all" || i.status === status);
  return (
    <div className="pb-10">
      <PageHeader
        title="Billing & invoices"
        subtitle="Setup fees, monthly licences and revenue share per tenant (D110)"
        actions={
          <>
            <Button variant="surface" onClick={reload}>
              <RefreshCw /> Refresh
            </Button>
            <Button variant="ember" onClick={() => setCreating(true)} disabled={!data}>
              <FilePlus2 /> New invoice
            </Button>
          </>
        }
      />
      {error ? (
        <ErrorState error={error} onRetry={reload} />
      ) : !data ? (
        <TableSkeleton rows={5} />
      ) : (
        <div className="space-y-4">
          <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
            <KpiCard label="Licence MRR" value={money(data.totals.mrr_cents)} icon={<Coins />} />
            <KpiCard label="Outstanding" value={money(data.totals.outstanding_cents)} icon={<Receipt />} chipTone={data.totals.outstanding_cents ? "warn" : "up"} />
            <KpiCard label="Collected (all time)" value={money(data.totals.paid_cents)} />
            <KpiCard label="Tenants billed" value={`${data.items.filter((i) => i.billing.configured).length} / ${data.items.length}`} />
          </div>
          <Card>
            <CardHeader title="Plans" subtitle="Open a tenant to change its plan" />
            <div className="px-4 pb-5 pt-4 sm:px-6">
              <DataTable
                rows={data.items}
                rowKey={(i) => String(i.tenant.id)}
                pageSize={25}
                columns={[
                  { key: "t", header: "Tenant", cell: (i) => <Link href={`/brokers/${i.tenant.id}`} className="font-medium hover:text-ember">{i.tenant.name}</Link> },
                  { key: "s", header: "Setup fee", align: "right", cell: (i) => (i.billing.configured ? <span className="k-num">{money(i.billing.setup_fee_cents, i.billing.currency)}{i.setup_invoiced ? <span className="text-fg-3"> · invoiced</span> : null}</span> : <span className="text-fg-3">No plan</span>) },
                  { key: "l", header: "Licence / mo", align: "right", cell: (i) => <span className="k-num">{money(i.billing.monthly_licence_cents, i.billing.currency)}</span> },
                  { key: "r", header: "Revenue share", align: "right", cell: (i) => <span className="k-num">{pct(i.billing.revenue_share_bps)}</span> },
                  { key: "o", header: "Outstanding", align: "right", cell: (i) => <span className="k-num">{money(i.outstanding_cents, i.billing.currency)}</span> },
                  { key: "p", header: "Last period", align: "right", hideOn: "md", cell: (i) => <span className="text-fg-3">{day(i.last_period_end)}</span> },
                ]}
              />
            </div>
          </Card>
          <Card>
            <CardHeader
              title="Invoices"
              action={<Segmented size="xs" value={status} onChange={setStatus} options={(["all", "draft", "issued", "overdue", "paid", "void"] as const).map((s) => ({ value: s, label: cap(s) }))} />}
            />
            <div className="px-4 pb-5 pt-4 sm:px-6">
              <InvoiceTable items={invoices} onChanged={reload} />
            </div>
          </Card>
          <NewInvoice open={creating} onOpenChange={setCreating} data={data} onDone={reload} />
        </div>
      )}
    </div>
  );
}
