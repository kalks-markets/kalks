"use client";

import * as React from "react";
import Link from "next/link";
import { Check, HandCoins, RefreshCw, RotateCw, ShieldCheck, Wallet, X } from "lucide-react";
import { Button, Card, Chip, DataTable, Dialog, Money, PageHeader, Reveal, Segmented, Skeleton, cn, type Column } from "@kalks/ui";
import { MiniStat, Section } from "@/components/config/kit";
import { Pager, TableSkeleton, ago, day, useApi, useNow, when } from "@/components/live/kit";
import { useStaff } from "@/components/staff-session";
import { P, ibSend, type Batch, type BatchDetail, type BatchStatus, type BatchesDoc, type Payout } from "./api";
import { BATCH_STATUS, EmptyNote, MemberLink, PAYOUT_STATUS, PartnersError, ReadOnlyNote, StatusPill, WEEKDAYS, int, usd, usePerms, useReasonAction, useUrlParam } from "./kit";

type Tab = "all" | BatchStatus;
const PER = 25;

function period(b: Batch) {
  return `${b.periodStart ? day(b.periodStart) : "Start"} → ${day(b.periodEnd)}`;
}

const SELF_APPROVE_NOTE = "You gave the first approval. Another staff member must give the second approval (four-eyes).";

/** Four-eyes view of a batch for the signed-in staff member. */
function fourEyesOf(b: Batch, meId: string) {
  const awaitingSecond = b.status === "awaiting_second_approval";
  return {
    awaitingSecond,
    /** waiting for its first approval, and at / above the threshold: the first approval pays nothing */
    needsTwo: b.status === "pending_approval" && !!b.fourEyes?.required,
    /** the signed-in staff member gave the first approval: they can't give the second */
    own: awaitingSecond && b.fourEyes?.firstApprovedById === meId,
    open: b.status === "pending_approval" || awaitingSecond,
  };
}

/** Transfer state of one payee line, stated plainly. */
function TransferState({ p, now }: { p: Payout; now: number }) {
  if (p.status === "paid")
    return (
      <span className="block text-[11px] text-fg-3">
        Credited {p.paidAt ? ago(p.paidAt, now) : ""}
        {p.walletTxn && <span className="block truncate font-mono">txn {p.walletTxn}</span>}
      </span>
    );
  if (p.status === "transfer_pending")
    return (
      <span className="block text-[11px] text-fg-3">
        {p.attempts > 0 ? (
          <>
            Wallet unavailable, retrying · attempt {p.attempts}
            {p.nextAttemptAt ? `, next ${ago(p.nextAttemptAt, now)}` : ""}
            {p.lastError && <span className="block max-w-56 truncate text-warn" title={p.lastError}>{p.lastError}</span>}
          </>
        ) : (
          "Queued for the wallet"
        )}
      </span>
    );
  if (p.status === "failed")
    return (
      <span className="block max-w-56 text-[11px] text-down" title={p.lastError ?? undefined}>
        Wallet refused after {p.attempts} attempt{p.attempts === 1 ? "" : "s"}
        {p.lastError ? `: ${p.lastError}` : ""}
      </span>
    );
  return null;
}

function BatchDrawer({ id, perms, onClose, onChanged }: { id: number | null; perms: { write: boolean; approve: boolean }; onClose: () => void; onChanged: () => void }) {
  const now = useNow();
  const me = useStaff();
  const act = useReasonAction();
  const { data, error, reload } = useApi<BatchDetail>(id !== null ? P(`batches/${id}`) : null, { refreshMs: 30_000 });
  const d = data && data.batch.id === id ? data : null;
  const b = d?.batch;
  const fe = b ? fourEyesOf(b, String(me.id)) : null;
  const flagged = d?.payouts.filter((p) => p.openFlags > 0) ?? [];
  const failed = d?.payouts.filter((p) => p.status === "failed").length ?? 0;
  const retrying = d?.payouts.filter((p) => p.status === "transfer_pending").length ?? 0;
  const done = (x: BatchDetail) => {
    void x;
    reload();
    onChanged();
  };

  const approve = () =>
    b &&
    fe &&
    act.ask({
      title: fe.needsTwo ? `Approve batch #${b.id} (1 of 2)` : fe.awaitingSecond ? `Second approval for batch #${b.id}` : `Approve batch #${b.id}`,
      description: fe.needsTwo
        ? `This batch is at or above the four-eyes threshold (${usd(b.fourEyes?.threshold ?? 0)}). Your approval is recorded, but nothing is paid until another staff member with approval rights gives the second approval.`
        : fe.awaitingSecond
          ? `${b.fourEyes?.firstApprovedBy ?? "Another staff member"} gave the first approval. Your approval releases ${usd(b.total)} to ${int(b.payees)} payee${b.payees === 1 ? "" : "s"}: each amount is credited to the member's client wallet right away; if the wallet service is down, the transfer waits and retries automatically.`
          : `Pays ${usd(b.total)} to ${int(b.payees)} payee${b.payees === 1 ? "" : "s"}. Each amount is credited to the member's client wallet right away; if the wallet service is down, the transfer waits and retries automatically.`,
      body: (
        <div className="space-y-2">
          <div className="grid grid-cols-3 gap-2">
            <MiniStat label="Total" value={usd(b.total)} tone="gold" />
            <MiniStat label="Payees" value={int(b.payees)} />
            <MiniStat label="Lines" value={int(b.lines)} />
          </div>
          {flagged.length > 0 && (
            <div className="rounded-[12px] border border-warn/30 bg-warn-soft px-3.5 py-2.5 text-[12.5px] text-fg">
              {flagged.length} payee{flagged.length === 1 ? " has" : "s have"} open fraud flags ({flagged.map((p) => p.name || `#${p.userId}`).join(", ")}). Review them first, or reject the batch.
            </div>
          )}
        </div>
      ),
      reason: "optional",
      placeholder: "Anything worth noting for the audit log",
      confirmLabel: fe.needsTwo ? "Approve (1 of 2)" : `Approve and pay ${usd(b.total)}`,
      confirmVariant: "buy",
      run: (note) => ibSend<BatchDetail>(`batches/${b.id}/approve`, note ? { note } : {}),
      success: (r) => (r.batch.status === "awaiting_second_approval" ? "First approval recorded · waiting for a second approver" : `Batch #${b.id} approved`),
      successDetail: (r) => {
        if (r.batch.status === "awaiting_second_approval") return `Nothing is paid until another staff member approves batch #${b.id}.`;
        const paid = r.payouts.filter((p) => p.status === "paid").length;
        const pend = r.payouts.filter((p) => p.status === "transfer_pending").length;
        return pend ? `${paid} credited · ${pend} waiting for the wallet, retrying` : `${paid} credited to client wallets`;
      },
      onDone: done,
    });

  const reject = () =>
    b &&
    act.ask({
      title: `Reject batch #${b.id}`,
      description: `Nothing is paid${fe?.awaitingSecond ? ` (the first approval by ${b.fourEyes?.firstApprovedBy ?? "another staff member"} is discarded)` : ""}. The batch's commission lines go back to pending and will be picked up by the next batch (reject individual lines in Commissions first if they should never be paid).`,
      confirmLabel: "Reject batch",
      confirmVariant: "sell",
      run: (reason) => ibSend<BatchDetail>(`batches/${b.id}/reject`, { reason }),
      success: `Batch #${b.id} rejected`,
      onDone: done,
    });

  const retry = () =>
    b &&
    act.ask({
      title: `Retry transfers for batch #${b.id}`,
      description: `${failed ? `${failed} failed` : ""}${failed && retrying ? " and " : ""}${retrying ? `${retrying} waiting` : ""} transfer${failed + retrying === 1 ? "" : "s"} are sent to the wallet again now. Credits are idempotent, so nobody is paid twice.`,
      reason: "none",
      confirmLabel: "Retry now",
      run: () => ibSend<BatchDetail>(`batches/${b.id}/retry`, {}),
      success: "Transfers retried",
      successDetail: (r) => {
        const pend = r.payouts.filter((p) => p.status === "transfer_pending").length;
        const f = r.payouts.filter((p) => p.status === "failed").length;
        return pend || f ? `${pend} still waiting · ${f} failed` : "All credited";
      },
      onDone: done,
    });

  const cols: Column<Payout>[] = [
    { key: "p", header: "Payee", cell: (p) => <MemberLink id={p.userId} name={p.name} sub={p.email ?? undefined} className="max-w-48" />, csv: (p) => `${p.name} #${p.userId}` },
    { key: "l", header: "Per lot", align: "right", hideOn: "md", cell: (p) => <span className="k-num text-[12.5px] text-fg-2">{usd(p.breakdown.lots)}</span>, csv: (p) => p.breakdown.lots },
    { key: "c", header: "CPA", align: "right", hideOn: "lg", cell: (p) => <span className="k-num text-[12.5px] text-fg-2">{usd(p.breakdown.cpa)}</span>, csv: (p) => p.breakdown.cpa },
    { key: "r", header: "Rebates", align: "right", hideOn: "lg", cell: (p) => <span className="k-num text-[12.5px] text-fg-2">{usd(p.breakdown.rebates)}</span>, csv: (p) => p.breakdown.rebates },
    { key: "adj", header: "Adj.", align: "right", hideOn: "lg", cell: (p) => <span className={cn("k-num text-[12.5px]", p.breakdown.adjustments < 0 ? "text-down" : "text-fg-2")}>{usd(p.breakdown.adjustments)}</span>, csv: (p) => p.breakdown.adjustments },
    { key: "a", header: "Amount", align: "right", cell: (p) => <span className="block"><Money value={p.amount} countUp={false} className="font-medium" /><span className="block text-[10.5px] text-fg-3">{int(p.lines)} line{p.lines === 1 ? "" : "s"}</span></span>, csv: (p) => p.amount },
    {
      key: "s",
      header: "Transfer",
      cell: (p) => (
        <span className="flex flex-col items-start gap-0.5">
          <span className="flex items-center gap-1">
            <StatusPill map={PAYOUT_STATUS} status={p.status} />
            {p.openFlags > 0 && <Chip size="sm" tone="warn">{p.openFlags} flag{p.openFlags > 1 ? "s" : ""}</Chip>}
          </span>
          <TransferState p={p} now={now} />
        </span>
      ),
      csv: (p) => p.status,
    },
  ];

  return (
    <>
      <Dialog
        open={id !== null}
        onOpenChange={(o) => !o && onClose()}
        width={980}
        title={b ? `Batch #${b.id}` : id !== null ? `Batch #${id}` : ""}
        description={b ? `${b.schedule} · ${period(b)} · created by ${b.createdBy} ${when(b.createdAt)}` : undefined}
        footer={
          b && fe && perms.approve && (fe.open || failed + retrying > 0) ? (
            <div className="flex w-full flex-wrap items-center justify-end gap-2">
              {fe.open ? (
                <>
                  {fe.own && <span className="mr-auto max-w-md text-[12px] text-fg-3">{SELF_APPROVE_NOTE}</span>}
                  <Button variant="down-outline" size="sm" onClick={reject}>
                    <X /> Reject
                  </Button>
                  <Button variant="buy" size="sm" onClick={approve} disabled={fe.own} title={fe.own ? SELF_APPROVE_NOTE : undefined} data-testid="batch-approve">
                    <Check /> {fe.needsTwo ? "Approve (1 of 2)" : fe.awaitingSecond ? "Second approval and pay" : "Approve and pay"}
                  </Button>
                </>
              ) : (
                <Button variant="surface" size="sm" onClick={retry}>
                  <RotateCw /> Retry transfers
                </Button>
              )}
            </div>
          ) : undefined
        }
      >
        {error && !d ? (
          <PartnersError error={error} onRetry={reload} />
        ) : !d || !b ? (
          <div className="space-y-3">
            <div className="grid grid-cols-2 gap-2.5 sm:grid-cols-4">
              {[0, 1, 2, 3].map((i) => (
                <Skeleton key={i} className="h-16 w-full" />
              ))}
            </div>
            <Skeleton className="h-48 w-full" />
          </div>
        ) : (
          <div>
            <Section title="Summary" action={<StatusPill map={BATCH_STATUS} status={b.status} />}>
              <div className="grid grid-cols-2 gap-2.5 sm:grid-cols-4">
                <MiniStat label="Total" value={usd(b.total)} tone="gold" />
                <MiniStat label="Payees" value={int(b.payees)} sub={`${int(b.lines)} commission line${b.lines === 1 ? "" : "s"}`} />
                <MiniStat label="Credited" value={int(d.payouts.filter((p) => p.status === "paid").length)} sub={`of ${int(d.payouts.length)}`} tone={b.status === "paid" ? "up" : undefined} />
                <MiniStat label="Open flags" value={int(flagged.length)} sub="Payees with open flags" tone={flagged.length ? "warn" : undefined} />
              </div>
              <div className="mt-3 flex items-start gap-2.5 rounded-[14px] border border-line bg-surface-2/60 px-3.5 py-2.5 text-[12px] text-fg-3">
                <Wallet className="mt-0.5 size-3.5 shrink-0 text-fg-2" />
                <span>
                  Approved payouts are credited to each member&apos;s client wallet. If the wallet service is unavailable the line stays <b className="font-medium text-fg-2">pending transfer</b> and is retried automatically with backoff; nothing is paid twice.
                </span>
              </div>
              {(fe?.needsTwo || b.fourEyes?.firstApprovedBy) && (
                <div className={cn("mt-3 flex items-start gap-2.5 rounded-[14px] border px-3.5 py-2.5 text-[12.5px]", fe?.awaitingSecond ? "border-warn/30 bg-warn-soft text-fg" : "border-line bg-surface-2/60 text-fg-2")} data-testid="batch-four-eyes">
                  <ShieldCheck className="mt-0.5 size-3.5 shrink-0" />
                  <span>
                    {fe?.needsTwo ? (
                      <>Four-eyes: this batch is at or above {usd(b.fourEyes?.threshold ?? 0)}, so two different staff members must approve it before anything is paid.</>
                    ) : (
                      <>
                        First approval by <span className="font-medium">{b.fourEyes?.firstApprovedBy}</span> {when(b.fourEyes?.firstApprovedAt ?? null)}
                        {fe?.awaitingSecond && <> · waiting for a second approver (four-eyes, {usd(b.fourEyes?.threshold ?? 0)} and above)</>}
                        {b.fourEyes?.firstApprovalNote && <span className="mt-1 block text-fg-3">“{b.fourEyes.firstApprovalNote}”</span>}
                        {fe?.own && <span className="mt-1 block text-fg-3">{SELF_APPROVE_NOTE}</span>}
                      </>
                    )}
                  </span>
                </div>
              )}
              {(b.decidedBy || b.decisionNote) && (
                <div className="mt-3 text-[12.5px] text-fg-2">
                  {b.status === "rejected" ? "Rejected" : "Approved"} by <span className="font-medium">{b.decidedBy ?? "—"}</span> {when(b.decidedAt)}
                  {b.decisionNote && <div className="mt-1 text-fg-3">“{b.decisionNote}”</div>}
                </div>
              )}
              {fe?.open && !perms.approve && <div className="mt-3"><ReadOnlyNote what="approve payouts" /></div>}
            </Section>
            <Section title="Payees" action={<Link href={`/partners/commissions?batch=${b.id}`} className="text-[12px] text-fg-3 hover:text-fg">View lines</Link>}>
              <DataTable columns={cols} rows={d.payouts} pageSize={25} dense rowKey={(p) => String(p.id)} exportName={`ib-batch-${b.id}`} empty={<EmptyNote title="No payees" />} />
            </Section>
          </div>
        )}
      </Dialog>
      {act.node}
    </>
  );
}

export function LivePayoutBatches() {
  const now = useNow();
  const meId = String(useStaff().id);
  const perms = usePerms();
  const act = useReasonAction();
  const [tab, setTab] = React.useState<Tab>("all");
  const [page, setPage] = React.useState(1);
  const [sel, setSel] = useUrlParam("batch");
  React.useEffect(() => setPage(1), [tab]);
  const { data, error, loading, reload } = useApi<BatchesDoc>(`${P("batches")}?page=${page}&limit=${PER}${tab !== "all" ? `&status=${tab}` : ""}`, { refreshMs: 30_000 });
  const { data: settings } = useApi<{ settings: { payout: { weekday: number; monthDay: number; autoCreate: boolean } } }>(P("settings"));
  const selId = sel && /^\d+$/.test(sel) ? Number(sel) : null;
  const payout = settings?.settings.payout;

  const create = () =>
    data &&
    act.ask({
      title: "Create a batch now",
      description: `Collects every payable pending line up to now into one batch for approval. Payees below the ${usd(data.minAmount)} minimum are carried over. Nothing is paid until the batch is approved.`,
      body: (
        <div className="grid grid-cols-2 gap-2">
          <MiniStat label="Payable now" value={usd(data.unbatched.amount)} tone="gold" />
          <MiniStat label="Payees" value={int(data.unbatched.payees)} sub="before the minimum" />
        </div>
      ),
      reason: "none",
      confirmLabel: "Create batch",
      run: () => ibSend<BatchDetail>("batches", {}),
      success: (r) => `Batch #${r.batch.id} created`,
      successDetail: (r) => `${usd(r.batch.total)} to ${int(r.batch.payees)} payees · awaiting approval`,
      onDone: (r) => {
        reload();
        setSel(String(r.batch.id));
      },
    });

  const cols: Column<Batch>[] = [
    { key: "id", header: "Batch", cell: (b) => <span><span className="block font-mono text-[12.5px]">#{b.id}</span><span className="text-[11px] capitalize text-fg-3">{b.schedule}</span></span>, csv: (b) => b.id },
    { key: "p", header: "Period", hideOn: "md", cell: (b) => <span className="whitespace-nowrap text-[12.5px] text-fg-2">{period(b)}</span>, csv: (b) => `${b.periodStart ?? ""} ${b.periodEnd}` },
    { key: "n", header: "Payees", align: "right", cell: (b) => <span className="k-num">{int(b.payees)}<span className="block text-[10.5px] text-fg-3">{int(b.lines)} line{b.lines === 1 ? "" : "s"}</span></span>, csv: (b) => b.payees },
    { key: "t", header: "Total", align: "right", cell: (b) => <Money value={b.total} countUp={false} className="font-medium" />, csv: (b) => b.total },
    {
      key: "tr",
      header: "Transfers",
      hideOn: "lg",
      cell: (b) =>
        b.transfers && b.status !== "pending_approval" && b.status !== "awaiting_second_approval" && b.status !== "rejected" ? (
          <span className="k-num text-[12px] text-fg-2">
            <span className="text-up">{b.transfers.paid} credited</span>
            {b.transfers.pending > 0 && <span className="text-info"> · {b.transfers.pending} retrying</span>}
            {b.transfers.failed > 0 && <span className="text-down"> · {b.transfers.failed} failed</span>}
          </span>
        ) : (
          <span className="text-[12px] text-fg-3">—</span>
        ),
    },
    {
      key: "s",
      header: "Status",
      cell: (b) => {
        const fe = fourEyesOf(b, meId);
        return (
          <span className="flex flex-col items-start gap-0.5">
            <StatusPill map={BATCH_STATUS} status={b.status} />
            {fe.awaitingSecond ? (
              <span className="max-w-40 truncate text-[10.5px] text-fg-3" title={fe.own ? SELF_APPROVE_NOTE : undefined}>
                1st: {fe.own ? "you" : b.fourEyes?.firstApprovedBy}
              </span>
            ) : fe.needsTwo ? (
              <span className="text-[10.5px] text-fg-3">Needs 2 approvers</span>
            ) : (
              b.decidedBy && <span className="max-w-36 truncate text-[10.5px] text-fg-3">{b.decidedBy}</span>
            )}
          </span>
        );
      },
      csv: (b) => b.status,
    },
    { key: "c", header: "Created", align: "right", hideOn: "md", cell: (b) => <span className="whitespace-nowrap text-[11.5px] text-fg-3" title={when(b.createdAt)}>{ago(b.createdAt, now)}<span className="block max-w-32 truncate">{b.createdBy}</span></span>, csv: (b) => b.createdAt },
  ];

  const scheduleText = data ? (data.schedule === "weekly" && payout ? `Weekly, closes ${WEEKDAYS[payout.weekday - 1]}` : data.schedule === "monthly" && payout ? `Monthly, on day ${payout.monthDay}` : data.schedule[0]!.toUpperCase() + data.schedule.slice(1)) : "—";

  return (
    <div className="pb-16">
      <PageHeader
        title="Payout batches"
        subtitle={`Commission is paid in batches. Approved payouts are credited to each IB's client wallet.${
          data?.fourEyesThreshold !== undefined ? (data.fourEyesThreshold === 0 ? " Every batch needs two approvers (four-eyes)." : ` Batches of ${usd(data.fourEyesThreshold)} or more need two approvers (four-eyes).`) : ""
        }`}
        actions={
          <>
            {perms.loaded && !perms.approve && <ReadOnlyNote what="approve payouts" />}
            <Button variant="surface" onClick={reload}>
              <RefreshCw /> Refresh
            </Button>
            {perms.write && (
              <Button variant="ember" onClick={create} disabled={!data}>
                <HandCoins /> Create batch now
              </Button>
            )}
          </>
        }
      />

      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <MiniStat label="Not yet batched" value={data ? usd(data.unbatched.amount) : "—"} sub={data ? `${int(data.unbatched.payees)} payee${data.unbatched.payees === 1 ? "" : "s"}, payable now` : undefined} tone="gold" />
        <MiniStat label="Schedule" value={scheduleText} sub={payout ? (payout.autoCreate ? "Batches created automatically" : "Batches created by hand") : undefined} />
        <MiniStat label="Next close" value={data ? day(data.nextClose) : "—"} sub={data ? ago(data.nextClose, now) : undefined} />
        <MiniStat label="Minimum payout" value={data ? usd(data.minAmount) : "—"} sub="Smaller amounts carry over" />
      </div>

      <Reveal delay={0.05}>
        <Card className="mt-4 p-4 sm:p-6">
          <div className="mb-3 overflow-x-auto">
            <Segmented
              size="xs"
              value={tab}
              onChange={setTab}
              options={[
                { value: "all", label: "All" },
                { value: "pending_approval", label: "Awaiting approval" },
                { value: "awaiting_second_approval", label: "Awaiting 2nd approval" },
                { value: "approved", label: "Transferring" },
                { value: "paid", label: "Paid" },
                { value: "partially_paid", label: "Partly paid" },
                { value: "rejected", label: "Rejected" },
              ]}
            />
          </div>
          {error && !data ? (
            <PartnersError error={error} onRetry={reload} />
          ) : !data ? (
            <TableSkeleton />
          ) : (
            <div className={loading ? "opacity-60 transition-opacity" : undefined}>
              <DataTable
                columns={cols}
                rows={data.items}
                pageSize={PER}
                dense
                rowKey={(b) => String(b.id)}
                onRowClick={(b) => setSel(String(b.id))}
                empty={
                  <EmptyNote
                    className="mt-3"
                    title={tab === "all" ? "No payout batches yet" : `No ${BATCH_STATUS[tab as BatchStatus]?.label.toLowerCase() ?? ""} batches`}
                    text={tab === "all" ? `A batch is created ${payout?.autoCreate ? "automatically when a period closes" : "with “Create batch now”"} once there are payable commissions above the minimum.` : undefined}
                  />
                }
              />
              <Pager page={page} perPage={PER} total={data.total} onPage={setPage} />
            </div>
          )}
        </Card>
      </Reveal>

      <BatchDrawer id={selId} perms={perms} onClose={() => setSel(null)} onChanged={reload} />
      {act.node}
    </div>
  );
}
