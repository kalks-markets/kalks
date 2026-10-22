"use client";

import * as React from "react";
import { Check, HandCoins, RefreshCw, X } from "lucide-react";
import { Button, Card, Chip, DataTable, EmptyState, KpiCard, PageHeader, Reveal, Segmented, type Column } from "@kalks/ui";
import { MiniStat } from "@/components/config/kit";
import { TableSkeleton, ago, day, useApi, useNow, when } from "@/components/live/kit";
import { ReadOnlyNote, SocialError, SocialStatus, amountOf, socialWrite, useNoteAction, useSocialCan, usd, type FeeView } from "./kit";

type Tab = "pending" | "approved" | "paid" | "rejected" | "failed";

export function LivePayoutsPage() {
  const now = useNow();
  const canApprove = useSocialCan("social.approve");
  const [tab, setTab] = React.useState<Tab>("pending");
  const { data, error, reload } = useApi<{ items: FeeView[]; totals?: Record<string, unknown> }>(`/api/social/admin/fees?status=${tab}`, { refreshMs: 30_000 });
  const act = useNoteAction();
  const rows = data?.items ?? [];
  const totals = data?.totals ?? {};
  const t = (k: string) => amountOf(totals[k]);

  // Where the fee was charged. A rejected fee is refunded in full (platform cut included) to the payer's wallet.
  const chargedOn = (f: FeeView) => (f.source === "pamm" ? `PAMM fund ${f.login}, as units` : f.source === "mam" ? `MAM-linked account ${f.login}` : `copy account ${f.login}`);
  const review = (f: FeeView, decision: "approve" | "reject") =>
    act.ask({
      title: decision === "approve" ? `Approve and pay fee #${f.id}` : `Reject and refund fee #${f.id}`,
      description:
        decision === "approve"
          ? `Pays ${usd(f.masterAmount)} to ${f.master}'s wallet now (fee ${usd(f.amount)} less the platform cut of ${usd(f.platformCut)}). This moves real money and can't be undone from here.`
          : `Refunds the full fee of ${usd(f.amount)} to the payer's wallet (client #${f.payerUserId}; charged on ${chargedOn(f)}). Nothing is paid to ${f.master} and the platform keeps no cut. This moves real money and can't be undone from here.`,
      confirmLabel: decision === "approve" ? `Pay ${usd(f.masterAmount)}` : `Refund ${usd(f.amount)}`,
      confirmVariant: decision === "approve" ? "buy" : "sell",
      body:
        decision === "approve" ? (
          <div className="grid grid-cols-3 gap-2">
            <MiniStat label="Fee charged" value={usd(f.amount)} />
            <MiniStat label="Platform cut" value={usd(f.platformCut)} />
            <MiniStat label="To master" value={usd(f.masterAmount)} tone="gold" />
          </div>
        ) : (
          <div className="grid grid-cols-3 gap-2">
            <MiniStat label="Fee charged" value={usd(f.amount)} />
            <MiniStat label="To master" value={usd(0)} />
            <MiniStat label={`Refund to client #${f.payerUserId}`} value={usd(f.amount)} tone="gold" />
          </div>
        ),
      run: (note) => socialWrite(`admin/fees/${f.id}/review`, { decision, note }),
      success: decision === "approve" ? `Fee #${f.id} approved` : `Fee #${f.id} rejected · ${usd(f.amount)} refunded to client #${f.payerUserId}'s wallet`,
      onDone: reload,
    });

  const cols: Column<FeeView>[] = [
    { key: "id", header: "Fee", sort: (f) => f.id, csv: (f) => f.id, cell: (f) => <span className="font-mono text-[12px]">#{f.id}<span className="block text-[11px] text-fg-3">{f.source === "pamm" ? "PAMM" : f.source === "mam" ? "MAM" : "Copy"}</span></span> },
    { key: "m", header: "Master", sort: (f) => f.master, csv: (f) => f.master, cell: (f) => <span className="text-[13px] font-medium">{f.master}<span className="block text-[11px] font-normal text-fg-3">#{f.masterId}{f.fundId ? ` · fund #${f.fundId}` : f.subscriptionId ? ` · sub #${f.subscriptionId}` : f.linkId ? ` · MAM link #${f.linkId}` : ""}</span></span> },
    { key: "p", header: "Payer", csv: (f) => `${f.payerUserId} ${f.login}`, cell: (f) => <span className="font-mono text-[12px]">#{f.payerUserId}<span className="block text-[11px] text-fg-3">{f.login}</span></span> },
    { key: "per", header: "Period", hideOn: "lg", sort: (f) => Date.parse(f.periodEnd ?? f.createdAt), cell: (f) => <span className="whitespace-nowrap text-[12px] text-fg-2">{f.periodStart ? day(f.periodStart) : "—"} → {f.periodEnd ? day(f.periodEnd) : "—"}</span> },
    { key: "hwm", header: "HWM", align: "right", hideOn: "xl", cell: (f) => <span className="k-num whitespace-nowrap text-[12px] text-fg-3">{f.hwmBefore !== null ? usd(f.hwmBefore) : "—"} → {f.hwmAfter !== null ? usd(f.hwmAfter) : "—"}</span> },
    { key: "a", header: "Fee", align: "right", sort: (f) => f.amount, csv: (f) => f.amount, cell: (f) => <span className="k-num">{usd(f.amount)}</span> },
    { key: "c", header: "Platform cut", align: "right", hideOn: "md", sort: (f) => f.platformCut, csv: (f) => f.platformCut, cell: (f) => <span className="k-num text-fg-2">{usd(f.platformCut)}</span> },
    { key: "ma", header: "To master", align: "right", sort: (f) => f.masterAmount, csv: (f) => f.masterAmount, cell: (f) => <span className="k-num font-medium">{usd(f.masterAmount)}</span> },
    { key: "s", header: "Status", csv: (f) => f.status, cell: (f) => <span className="flex flex-col items-start gap-0.5"><SocialStatus status={f.status} />{f.reviewedBy && <span className="text-[10.5px] text-fg-3">{f.reviewedBy}</span>}</span> },
    { key: "t", header: "Created", align: "right", hideOn: "md", sort: (f) => Date.parse(f.paidAt ?? f.createdAt), csv: (f) => f.createdAt, cell: (f) => <span className="whitespace-nowrap text-[11.5px] text-fg-3" title={when(f.paidAt ?? f.createdAt)}>{f.paidAt ? `Paid ${ago(f.paidAt, now)}` : ago(f.createdAt, now)}</span> },
    ...(tab === "pending" && canApprove
      ? [
          {
            key: "x",
            header: "",
            align: "right" as const,
            cell: (f: FeeView) => (
              <span className="flex justify-end gap-1" onClick={(e) => e.stopPropagation()}>
                <button type="button" onClick={() => review(f, "reject")} className="grid size-7 place-items-center rounded-full border border-line text-fg-3 hover:border-down/40 hover:bg-down-soft hover:text-down" aria-label="Reject">
                  <X className="size-3.5" />
                </button>
                <button type="button" onClick={() => review(f, "approve")} className="grid size-7 place-items-center rounded-full border border-up/40 bg-up-soft text-up hover:bg-up/20" aria-label="Approve and pay">
                  <Check className="size-3.5" />
                </button>
              </span>
            ),
          },
        ]
      : []),
    ...(tab !== "pending"
      ? [{ key: "n", header: "Note", hideOn: "lg" as const, cell: (f: FeeView) => <span className="line-clamp-2 max-w-56 text-[12px] text-fg-3">{f.note || "—"}</span> }]
      : []),
  ];

  const tabTotal = rows.reduce((s, f) => s + f.masterAmount, 0);

  return (
    <div className="pb-16">
      <PageHeader
        title="Fee payouts"
        subtitle="Performance fees charged to followers, PAMM investors and MAM accounts. Approving pays the master's wallet; the platform cut stays with the house. Rejecting refunds the whole fee to the payer's wallet."
        actions={
          <>
            {!canApprove && <ReadOnlyNote what="approve payouts" />}
            <Button variant="surface" onClick={reload}>
              <RefreshCw /> Refresh
            </Button>
          </>
        }
      />
      {error && !data ? (
        <SocialError error={error} onRetry={reload} />
      ) : (
        <>
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
            <KpiCard label="Pending approval" icon={<HandCoins />} value={<span className="k-num">{data ? usd(t("pending").amount) : "—"}</span>} chip={t("pending").count !== null ? `${t("pending").count} fees` : "Fees charged, not yet paid"} chipTone="warn" />
            <KpiCard label="Approved, paying" value={<span className="k-num">{data ? usd(t("approved").amount) : "—"}</span>} chip={t("approved").count !== null ? `${t("approved").count} fees` : "Payment to the wallet in progress"} delay={0.05} />
            <KpiCard label="Paid to masters" value={<span className="k-num">{data ? usd(t("paid").amount) : "—"}</span>} chip={t("paid").count !== null ? `${t("paid").count} fees` : "Credited to master wallets"} chipTone="up" delay={0.1} />
          </div>
          <Reveal delay={0.1}>
            <Card className="mt-4 p-4 sm:p-6">
              {!data ? (
                <TableSkeleton />
              ) : (
                <DataTable
                  columns={cols}
                  rows={rows}
                  dense
                  pageSize={20}
                  rowKey={(f) => String(f.id)}
                  search={(f) => `${f.id} ${f.master} ${f.masterId} ${f.login} ${f.payerUserId}`}
                  searchPlaceholder="Fee, master, login, payer…"
                  exportName={`social-fees-${tab}`}
                  empty={<EmptyState illustration="calendar" title={`No ${tab} fees`} text={tab === "pending" ? "Fees appear here after each settlement period ends (copy) or at a PAMM rollover." : undefined} />}
                  toolbar={
                    <div className="flex flex-wrap items-center gap-2">
                      <Segmented
                        size="xs"
                        value={tab}
                        onChange={setTab}
                        options={[
                          { value: "pending", label: "Pending" },
                          { value: "approved", label: "Approved" },
                          { value: "paid", label: "Paid" },
                          { value: "rejected", label: "Rejected" },
                          { value: "failed", label: "Failed" },
                        ]}
                      />
                      {rows.length > 0 && <Chip size="sm">{rows.length} fees · {usd(tabTotal)} to masters</Chip>}
                    </div>
                  }
                />
              )}
            </Card>
          </Reveal>
        </>
      )}
      {act.node}
    </div>
  );
}
