"use client";

import * as React from "react";
import { Check, CircleDollarSign, Clock, HandCoins, ShieldCheck, X } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, CardHeader, DataTable, Dialog, Input, KpiCard, PageHeader, Reveal, Tabs, Tooltip, type Column } from "@kalks/ui";
import { MiniField, MiniStat } from "@/components/config/kit";
import { TableSkeleton, ago, useApi, useNow, when } from "@/components/live/kit";
import { useStaff } from "@/components/staff-session";
import { ChallengeDrawer } from "./challenge-drawer";
import { KycStatus, PropError, PropStatus, ReadOnlyNote, TraderCell, pct, propWrite, usd, useAction, useClientNames, usePropCan, type Overview, type Payout, type PropSettings, FirstApproval, SECOND_APPROVER_NEEDED, isFirstApprover } from "./kit";

type Tab = "queue" | "history";
const OPEN = ["pending", "awaiting_second_approval", "failed", "approved"];
const RANK: Record<string, number> = { pending: 0, awaiting_second_approval: 1, failed: 2, approved: 3 };

export function LivePayoutsPage() {
  const now = useNow();
  const me = useStaff();
  const canApprove = usePropCan("prop.approve");
  const canWrite = usePropCan("prop.write");
  const [tab, setTab] = React.useState<Tab>("queue");
  const { data, error, reload } = useApi<{ payouts: Payout[]; fourEyesThreshold?: number }>("/api/prop/payouts", { refreshMs: 10_000 });
  const ov = useApi<Overview>("/api/prop/overview", { refreshMs: 15_000 });
  const settings = useApi<{ settings: PropSettings }>("/api/prop/settings");
  const threshold = settings.data?.settings.fourEyesThreshold ?? data?.fourEyesThreshold;
  const all = data?.payouts ?? [];
  const queue = all.filter((p) => OPEN.includes(p.status)).sort((a, b) => (a.status === b.status ? Date.parse(a.requestedAt) - Date.parse(b.requestedAt) : (RANK[a.status] ?? 9) - (RANK[b.status] ?? 9)));
  const history = all.filter((p) => !OPEN.includes(p.status));
  const rows = tab === "queue" ? queue : history;
  const names = useClientNames(all.map((p) => p.userId));
  const act = useAction();
  const [sel, setSel] = React.useState<number | null>(null);
  const [open, setOpen] = React.useState(false);
  const [fourEyesOpen, setFourEyesOpen] = React.useState(false);

  const breakdown = (p: Payout) => (
    <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
      <MiniStat label="Profit" value={usd(p.profit)} />
      <MiniStat label={`Trader ${pct(p.split, 0)}`} value={usd(p.traderAmount)} tone="up" />
      <MiniStat label="Fee refund" value={usd(p.feeRefund)} />
      <MiniStat label="Total credit" value={usd(p.total)} tone="gold" />
    </div>
  );

  const decide = (p: Payout, decision: "approve" | "reject") => {
    const who = names[String(p.userId)]?.name ?? p.traderName;
    const firstOfTwo = decision === "approve" && p.status === "pending" && !!p.fourEyes?.required;
    const second = decision === "approve" && p.status === "awaiting_second_approval";
    const retry = decision === "approve" && (p.status === "failed" || p.status === "approved");
    const credits = `Credits ${usd(p.total)} to ${who}'s USDT wallet${p.feeRefund ? ` (includes the ${usd(p.feeRefund)} fee refund)` : ""} and issues a payout certificate. This moves real money.`;
    act.ask({
      title: decision === "reject" ? `Reject payout #${p.id}` : retry ? `Retry the wallet credit of payout #${p.id}` : second ? `Second approval · payout #${p.id}` : `Approve payout #${p.id}`,
      description:
        decision === "reject"
          ? `The ${usd(p.profit)} profit is put back on account ${p.login}. The trader is notified with your note.`
          : firstOfTwo
            ? `Payouts of ${usd(p.fourEyes?.threshold ?? threshold ?? 0, 0)} or more need a second approver (four-eyes). Your approval is recorded; nothing is credited until another staff member with approval rights approves.`
            : credits,
      note: decision === "approve" ? "optional" : "required",
      noteLabel: decision === "approve" ? "Note" : "Reason for the trader",
      notePlaceholder: decision === "approve" ? "Optional note for the audit log" : "Why the payout is rejected (sent to the trader)",
      confirmLabel: decision === "reject" ? "Reject payout" : firstOfTwo ? "Approve (1 of 2)" : second ? `Approve ${usd(p.total)} (2 of 2)` : retry ? "Retry credit" : `Approve ${usd(p.total)}`,
      confirmVariant: decision === "approve" ? "buy" : "sell",
      body: (
        <div className="space-y-3">
          {breakdown(p)}
          {second && p.fourEyes?.firstApprovedBy && (
            <div className="flex items-start gap-2 rounded-[12px] border border-line bg-surface-2 px-3 py-2.5 text-[12.5px] text-fg-2">
              <ShieldCheck className="mt-0.5 size-3.5 shrink-0 text-up" />
              <FirstApproval p={p} now={now} className="block text-[12.5px] text-fg-2" />
            </div>
          )}
          {decision === "approve" && !retry && (
            <div className="flex flex-wrap items-center gap-2 rounded-[12px] border border-line bg-surface-2 px-3 py-2.5 text-[12.5px] text-fg-2">
              KYC at request <KycStatus status={p.kycStatus} />
              <span className="text-fg-3">· the client&apos;s current KYC must be verified; it is checked again on {firstOfTwo || second ? "each approval" : "approval"}.</span>
            </div>
          )}
        </div>
      ),
      run: (v) => propWrite(`payouts/${p.id}/${decision}`, v.note ? { note: v.note } : {}),
      success: decision === "reject" ? `Payout #${p.id} rejected` : firstOfTwo ? "First approval recorded · waiting for a second approver" : `Payout #${p.id} approved`,
      onDone: reload,
    });
  };

  const approveButton = (p: Payout) => {
    const mine = p.status === "awaiting_second_approval" && isFirstApprover(p, me.id);
    const label = p.status === "pending" ? (p.fourEyes?.required ? "Approve (1 of 2)" : "Approve") : p.status === "awaiting_second_approval" ? "Approve (2 of 2)" : "Retry credit";
    const button = (
      <Button size="xs" variant="up-outline" disabled={mine} onClick={() => decide(p, "approve")}>
        <Check /> {label}
      </Button>
    );
    return mine ? (
      <Tooltip content={SECOND_APPROVER_NEEDED}>
        <span tabIndex={0} aria-label={SECOND_APPROVER_NEEDED}>
          {button}
        </span>
      </Tooltip>
    ) : (
      button
    );
  };

  const columns: Column<Payout>[] = [
    { key: "id", header: "Payout", sort: (p) => p.id, csv: (p) => p.id, cell: (p) => <span className="font-mono text-[12px]">#{p.id}<span className="block text-[11px] text-fg-3">challenge #{p.challengeId}</span></span> },
    { key: "trader", header: "Trader", sort: (p) => p.traderName, csv: (p) => p.traderName, cell: (p) => <TraderCell name={p.traderName} userId={p.userId} names={names} /> },
    { key: "acc", header: "Account", hideOn: "lg", csv: (p) => `${p.planName} ${p.login}`, cell: (p) => <span className="text-[12.5px]">{p.planName}<span className="block font-mono text-[11px] text-fg-3">{p.login} · {usd(p.size, 0)}</span></span> },
    { key: "profit", header: "Profit", align: "right", sort: (p) => p.profit, csv: (p) => p.profit, cell: (p) => <span className="k-num">{usd(p.profit)}</span> },
    { key: "split", header: "Split", align: "right", hideOn: "md", csv: (p) => p.split, cell: (p) => <span className="k-num text-gold">{pct(p.split, 0)}</span> },
    { key: "trader$", header: "Trader amount", align: "right", sort: (p) => p.traderAmount, csv: (p) => p.traderAmount, cell: (p) => <span className="k-num font-medium text-up">{usd(p.traderAmount)}</span> },
    { key: "refund", header: "Fee refund", align: "right", hideOn: "md", csv: (p) => p.feeRefund, cell: (p) => (p.feeRefund ? <span className="k-num text-gold">+{usd(p.feeRefund)}</span> : <span className="text-fg-3">—</span>) },
    { key: "total", header: "Total", align: "right", sort: (p) => p.total, csv: (p) => p.total, cell: (p) => <span className="k-num font-semibold">{usd(p.total)}{p.fourEyes?.required && <span className="block text-[10.5px] font-normal text-fg-3">four-eyes</span>}</span> },
    { key: "kyc", header: "KYC at request", hideOn: "lg", csv: (p) => p.kycStatus ?? "", cell: (p) => <KycStatus status={p.kycStatus} /> },
    { key: "req", header: tab === "queue" ? "Requested" : "Decided", sort: (p) => Date.parse(tab === "queue" ? p.requestedAt : (p.decidedAt ?? p.requestedAt)), csv: (p) => p.requestedAt, cell: (p) => <span className="whitespace-nowrap text-[11.5px] text-fg-3" title={when(p.requestedAt)}>{tab === "queue" ? ago(p.requestedAt, now) : p.decidedAt ? `${ago(p.decidedAt, now)}${p.decidedBy ? ` · ${p.decidedBy}` : ""}` : "—"}</span> },
    {
      key: "status",
      header: "Status",
      sort: (p) => p.status,
      csv: (p) => (p.status === "awaiting_second_approval" && p.fourEyes?.firstApprovedBy ? `${p.status} (first approval: ${p.fourEyes.firstApprovedBy})` : p.status),
      cell: (p) => (
        <span className="flex flex-col items-start gap-0.5">
          <PropStatus status={p.status} />
          <FirstApproval p={p} now={now} />
          {p.status !== "awaiting_second_approval" && (p.error || p.note) && <span className="max-w-44 truncate text-[10.5px] text-fg-3" title={p.error ?? p.note ?? ""}>{p.error ?? p.note}</span>}
          {p.status === "paid" && p.fourEyes?.firstApprovedBy && <span className="max-w-56 truncate text-[10.5px] text-fg-3">Four-eyes · 1st {p.fourEyes.firstApprovedBy}</span>}
        </span>
      ),
    },
    ...(tab === "queue" && canApprove
      ? [
          {
            key: "x",
            header: "",
            align: "right" as const,
            cell: (p: Payout) =>
              OPEN.includes(p.status) ? (
                <span className="flex justify-end gap-1.5" onClick={(e) => e.stopPropagation()}>
                  {approveButton(p)}
                  {(p.status === "pending" || p.status === "awaiting_second_approval") && (
                    <Button size="xs" variant="down-outline" onClick={() => decide(p, "reject")}>
                      <X /> Reject
                    </Button>
                  )}
                </span>
              ) : null,
          },
        ]
      : []),
  ];

  const waiting = queue.filter((p) => p.status === "pending" || p.status === "awaiting_second_approval");
  const second = queue.filter((p) => p.status === "awaiting_second_approval").length;
  const pendingAmt = waiting.reduce((s, p) => s + p.total, 0);

  return (
    <div className="pb-24">
      <PageHeader
        title="Payouts"
        subtitle={
          <span>
            Funded-account payout requests. The profit is held off the account while in review; approval credits the trader&apos;s USDT wallet.
            {threshold !== undefined && <> {threshold === 0 ? "Four-eyes: every payout needs two approvers." : `Four-eyes at or above ${usd(threshold, 0)}.`}</>}
          </span>
        }
        actions={
          <>
            {canWrite && (
              <Button variant="surface" onClick={() => setFourEyesOpen(true)} disabled={!settings.data}>
                <ShieldCheck /> Four-eyes
              </Button>
            )}
            {!canApprove && <ReadOnlyNote what="approve payouts" />}
          </>
        }
      />
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <KpiCard label="Pending" icon={<Clock />} value={<span className="k-num">{waiting.length}</span>} chip={second ? `${usd(pendingAmt)} · ${second} awaiting 2nd approval` : usd(pendingAmt)} chipTone="warn" />
        <KpiCard label="Failed credits" icon={<HandCoins />} value={<span className="k-num">{queue.filter((p) => p.status === "failed").length}</span>} chip="Wallet credit not confirmed" chipTone={queue.some((p) => p.status === "failed") ? "down" : "neutral"} delay={0.04} />
        <KpiCard label="Paid · 30d" icon={<CircleDollarSign />} value={<span className="k-num">{usd(ov.data?.paid30d, 0)}</span>} chip={`vs ${usd(ov.data?.fees30d, 0)} fees`} delay={0.08} />
        <KpiCard label="Rejected" icon={<X />} value={<span className="k-num">{history.filter((p) => p.status === "rejected").length}</span>} chip="All time" delay={0.12} />
      </div>
      <Reveal delay={0.08} className="mt-4">
        <Card>
          <CardHeader
            title={tab === "queue" ? "Approval queue" : "History"}
            subtitle={tab === "queue" ? "Pending, awaiting a second approval and failed first, oldest first" : "Paid and rejected payouts"}
            action={<Tabs value={tab} onChange={setTab} tabs={[{ value: "queue", label: "Queue", count: queue.length }, { value: "history", label: "History", count: history.length }]} />}
          />
          <div className="px-4 pb-6 pt-4 sm:px-6">
            {error && !data ? (
              <PropError error={error} onRetry={reload} />
            ) : !data ? (
              <TableSkeleton />
            ) : (
              <DataTable
                columns={columns}
                rows={rows}
                pageSize={20}
                dense
                rowKey={(p) => String(p.id)}
                exportName={`prop-payouts-${tab}`}
                search={(p) => `${p.id} ${p.traderName} ${p.login} ${p.planName} ${names[String(p.userId)]?.email ?? ""}`}
                searchPlaceholder="Trader, login, payout id"
                onRowClick={(p) => {
                  setSel(p.challengeId);
                  setOpen(true);
                }}
                empty={<div className="py-10 text-center text-[13px] text-fg-3">{tab === "queue" ? "Nothing waiting for approval." : "No decided payouts yet."}</div>}
              />
            )}
          </div>
        </Card>
      </Reveal>
      <ChallengeDrawer id={sel} open={open} onOpenChange={setOpen} onChanged={reload} funded />
      <FourEyesDialog
        open={fourEyesOpen}
        onClose={() => setFourEyesOpen(false)}
        current={settings.data?.settings ?? null}
        onDone={() => {
          settings.reload();
          reload();
        }}
      />
      {act.node}
    </div>
  );
}

/** Prop settings: the four-eyes threshold (prop.write; audited `settings.four_eyes_threshold` with the reason). */
function FourEyesDialog({ open, onClose, current, onDone }: { open: boolean; onClose: () => void; current: PropSettings | null; onDone: () => void }) {
  const [value, setValue] = React.useState("");
  const [reason, setReason] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [err, setErr] = React.useState<string | null>(null);
  React.useEffect(() => {
    if (!open) return;
    setValue(current ? String(current.fourEyesThreshold) : "1000");
    setReason("");
    setErr(null);
  }, [open, current]);
  const valid = /^\d+(\.\d{1,2})?$/.test(value) && Number(value) >= 0;
  const ok = valid && reason.trim().length > 0 && reason.length <= 200;
  async function save() {
    setBusy(true);
    setErr(null);
    const r = await propWrite<{ settings: PropSettings }>("settings", { fourEyesThreshold: Number(value), reason: reason.trim() }, "PUT");
    setBusy(false);
    if (!r.ok) {
      setErr(r.error.message);
      toast.error("Not saved", { description: r.error.message });
      return;
    }
    const t = r.data.settings.fourEyesThreshold;
    toast.success(t === 0 ? "Four-eyes on every payout" : `Four-eyes at or above ${usd(t, 0)}`, { description: "Recorded in the prop audit log" });
    onDone();
    onClose();
  }
  return (
    <Dialog
      open={open}
      onOpenChange={(o) => !o && onClose()}
      width={480}
      title="Four-eyes approval"
      description="Payout requests at or above the threshold need two different staff members with approval rights: the first approval is recorded, the second one credits the wallet."
      footer={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="ember" size="sm" disabled={!ok || busy} onClick={save}>
            {busy ? "Saving…" : "Save"}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <MiniField label="Four-eyes threshold (USD)">
          <Input value={value} onChange={(e) => setValue(e.target.value.replace(/[^0-9.]/g, ""))} inputMode="decimal" aria-label="Four-eyes threshold (USD)" className="h-10 font-mono" />
          <span className="text-[11.5px] text-fg-3">Payouts at or above this amount need a second staff member to approve. 0 = every payout.</span>
          <span className="text-[11.5px] text-fg-3">Compared with the total credited to the wallet (trader share + fee refund). Requests already approved once keep the rule they started with.</span>
        </MiniField>
        <MiniField label="Reason" hint="Required">
          <Input value={reason} onChange={(e) => setReason(e.target.value)} maxLength={200} placeholder="Why the threshold changes (audit log)" aria-label="Reason" className="h-10" />
        </MiniField>
        {current?.updatedAt && (
          <p className="text-[11.5px] text-fg-3">
            Last changed {when(current.updatedAt)}
            {current.updatedBy ? ` by ${current.updatedBy}` : ""}.
          </p>
        )}
        {err && (
          <div role="alert" className="rounded-[14px] border border-down/30 bg-down-soft px-3.5 py-2.5 text-[12.5px] text-fg">
            {err}
          </div>
        )}
      </div>
    </Dialog>
  );
}
