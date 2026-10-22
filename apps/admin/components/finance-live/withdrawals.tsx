"use client";

import * as React from "react";
import { ArrowUpRight, CircleDollarSign, Clock, RefreshCw, Search, Send } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, DataTable, Dialog, EmptyState, Field, Input, KpiCard, PageHeader, Tabs, type Column } from "@kalks/ui";
import { ErrorState, FilterSelect, Pager, TableSkeleton, ago, qs, useApi, useDebounced, useNow, when } from "@/components/live/kit";
import { useCan } from "@/components/staff-session";
import { Addr, ChainTag, Check, ClientCell, Row, Status, TxLink, WD_STATUS, usd, usd2, walletWrite, type AuditItem, type Paged, type Summary, type Withdrawal } from "./kit";

const PER = 50;
type Tab = "requested" | "approved" | "paid" | "completed" | "all";

type Risk = {
  kyc: { status: string | null; account_status?: string; ok: boolean; error?: string };
  recent_deposit: { last_credited_at: string | null; last_amount: string | null; cooldown_hours: number; ok: boolean };
  flows: { deposited: string; withdrawn: string; to_trading: string; from_trading: string; trading_pnl: string; ok: boolean };
  ip: { address: string | null; seen_before: boolean; other_clients: number[]; ok: boolean };
  destination: { address: string; paid_before: number; used_for_deposits: boolean; other_clients: number[]; ok: boolean };
  balances: { currency: string; available: string; locked: string }[];
};

type Detail = { withdrawal: Withdrawal; risk: Risk; history: AuditItem[] };

function RiskChecklist({ r }: { r: Risk }) {
  return (
    <div className="grid grid-cols-1 gap-2">
      <Check ok={r.kyc.ok} title={`KYC · ${r.kyc.status ?? "unknown"}`}>
        {r.kyc.error ?? (r.kyc.ok ? "Identity verified and account active." : `Account ${r.kyc.account_status ?? "?"}; withdrawals need a verified identity.`)}
      </Check>
      <Check ok={r.recent_deposit.ok} title="Recent deposit">
        {r.recent_deposit.last_credited_at ? `Last deposit ${usd(r.recent_deposit.last_amount)} USDT on ${when(r.recent_deposit.last_credited_at)}.` : "No deposits yet."} {r.recent_deposit.ok ? "" : "Deposited shortly before this request."}
      </Check>
      <Check ok={r.flows.ok} title="Money in and out">
        Deposited {usd(r.flows.deposited)} · withdrawn {usd(r.flows.withdrawn)} · trading P&amp;L {Number(r.flows.trading_pnl) >= 0 ? "+" : ""}
        {usd(r.flows.trading_pnl)} (into accounts {usd(r.flows.to_trading)}, back {usd(r.flows.from_trading)}).
      </Check>
      <Check ok={r.ip.ok} title={`Request IP · ${r.ip.address ?? "unknown"}`}>
        {r.ip.seen_before ? "Seen before for this client." : "First time for this client."} {r.ip.other_clients.length ? `Also used by clients ${r.ip.other_clients.map((x) => `#${x}`).join(", ")}.` : "Not used by other clients."}
      </Check>
      <Check ok={r.destination.ok} title="Destination address">
        {r.destination.paid_before ? `Paid to ${r.destination.paid_before} time${r.destination.paid_before === 1 ? "" : "s"} before.` : "First withdrawal to this address."} {r.destination.used_for_deposits ? "The client deposited from it." : ""}{" "}
        {r.destination.other_clients.length ? `Linked to other clients ${r.destination.other_clients.map((x) => `#${x}`).join(", ")}.` : ""}
      </Check>
    </div>
  );
}

function WithdrawalDrawer({ id, onClose, onChanged }: { id: number | null; onClose: () => void; onChanged: () => void }) {
  const { data, error, reload } = useApi<Detail>(id ? `/api/wallet/withdrawals/${id}` : null, { refreshMs: 10_000 });
  const canApprove = useCan("finance.approve");
  const canWrite = useCan("finance.write");
  const [mode, setMode] = React.useState<"approve" | "reject" | "paid" | null>(null);
  const [text, setText] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [err, setErr] = React.useState<string | null>(null);
  React.useEffect(() => {
    setMode(null);
    setText("");
    setErr(null);
  }, [id]);
  const w = data?.withdrawal;

  const act = async (path: string, body: unknown, ok: string) => {
    setBusy(true);
    setErr(null);
    const r = await walletWrite<{ withdrawal: Withdrawal }>(`withdrawals/${id}/${path}`, body);
    setBusy(false);
    if (!r.ok) return setErr(r.error.message);
    const st = r.data.withdrawal;
    if (path === "paid" && st.status === "approved" && st.payout_error) {
      setErr(st.payout_error);
    } else {
      toast.success(ok, { description: `Withdrawal #${id} · ${WD_STATUS[st.status].label}` });
      setMode(null);
      setText("");
    }
    reload();
    onChanged();
  };

  return (
    <Dialog
      side="right"
      open={id !== null}
      onOpenChange={(o) => !o && onClose()}
      title={w ? `Withdrawal #${w.id}` : "Withdrawal"}
      description={w ? `${w.network} · requested ${when(w.created_at)}` : undefined}
      footer={
        w ? (
          <>
            {/* reject only before approval: once approved the payout may already have been sent by hand */}
            {w.status === "requested" && canApprove && (
              <Button variant="down-outline" disabled={busy} onClick={() => (setMode("reject"), setText(""))}>
                Reject
              </Button>
            )}
            {w.status === "requested" && canApprove && (
              <Button variant="ember" disabled={busy} onClick={() => (setMode("approve"), setText(""))}>
                Approve
              </Button>
            )}
            {w.status === "approved" && canWrite && (
              <Button variant="ember" disabled={busy} onClick={() => (setMode("paid"), setText(""))}>
                <Send /> Mark as paid
              </Button>
            )}
          </>
        ) : undefined
      }
    >
      {error ? (
        <ErrorState error={error} onRetry={reload} />
      ) : !w || !data ? (
        <TableSkeleton rows={4} />
      ) : (
        <div className="space-y-5">
          <div className="k-row flex items-center justify-between px-4 py-4">
            <div>
              <div className="k-num text-[26px] font-semibold">{usd(w.amount)} USDT</div>
              <div className="text-[12px] text-fg-3">
                Send {usd(w.net_amount)} USDT · fee {usd(w.fee)}
              </div>
            </div>
            <Status map={WD_STATUS} s={w.status} />
          </div>
          {w.payout_error && w.status === "approved" && <div className="rounded-[14px] border border-down/30 bg-down-soft px-3.5 py-2.5 text-[12.5px]">Payout check failed: {w.payout_error}</div>}
          <div>
            <Row k="Client" v={<ClientCell id={w.user_id} />} />
            <Row k="Network" v={<ChainTag chain={w.chain} />} />
            <Row k="Send to" v={<Addr a={w.to_address} full />} />
            <Row k="Amount to send" v={<span className="k-num font-semibold">{usd(w.net_amount)} USDT</span>} />
            {w.reviewed_by && <Row k={w.status === "rejected" ? "Rejected by" : "Approved by"} v={`${w.reviewed_by}${w.reviewed_at ? ` · ${when(w.reviewed_at)}` : ""}`} />}
            {w.review_note && <Row k="Note" v={w.review_note} />}
            {w.reason && <Row k="Reason" v={w.reason} />}
            {w.payout_tx_hash && <Row k="Payout" v={<TxLink hash={w.payout_tx_hash} url={w.explorer_url} head={10} tail={8} />} />}
            {w.status === "paid" && <Row k="Confirmations" v={<span className="k-num">{w.payout_confirmations}</span>} />}
            {w.paid_by && <Row k="Paid by" v={`${w.paid_by}${w.paid_at ? ` · ${when(w.paid_at)}` : ""}`} />}
            {w.completed_at && <Row k="Completed" v={when(w.completed_at, true)} />}
          </div>
          <div>
            <div className="k-label mb-2">Risk checklist</div>
            <RiskChecklist r={data.risk} />
          </div>
          {mode && (
            <div className="space-y-3 rounded-[16px] border border-line bg-surface-2 p-4">
              <div className="text-[13.5px] font-medium">{mode === "approve" ? "Approve this withdrawal" : mode === "reject" ? "Reject this withdrawal" : "Record the payout"}</div>
              {mode === "paid" && (
                <p className="text-[12.5px] text-fg-3">
                  Send exactly {usd(w.net_amount)} USDT on {w.network} from a company address to {w.to_address}, then paste the transaction hash. The wallet verifies it on chain and completes the withdrawal after the confirmations.
                </p>
              )}
              <Field label={mode === "approve" ? "Note (optional)" : mode === "reject" ? "Reason (shown to the client)" : "Payout transaction hash"}>
                <Input value={text} onChange={(e) => setText(e.target.value)} inputClassName={mode === "paid" ? "font-mono text-[12.5px]" : undefined} placeholder={mode === "paid" ? (w.chain === "bsc" ? "0x…" : "64 hex characters") : undefined} aria-label={mode === "paid" ? "Payout transaction hash" : "Reason"} />
              </Field>
              {err && <div className="text-[12.5px] text-down">{err}</div>}
              <div className="flex gap-2">
                <Button variant="ghost" onClick={() => setMode(null)}>
                  Cancel
                </Button>
                {mode === "approve" && (
                  <Button variant="ember" disabled={busy} onClick={() => act("approve", { note: text }, "Withdrawal approved")}>
                    Confirm approval
                  </Button>
                )}
                {mode === "reject" && (
                  <Button variant="down-outline" disabled={busy || w.status !== "requested" || text.trim().length < 3} onClick={() => act("reject", { reason: text }, "Withdrawal rejected")}>
                    Reject and unlock funds
                  </Button>
                )}
                {mode === "paid" && (
                  <Button variant="ember" disabled={busy || !/^(0x)?[0-9a-fA-F]{64}$/.test(text.trim())} onClick={() => act("paid", { tx_hash: text.trim() }, "Payout recorded")}>
                    Verify payout
                  </Button>
                )}
              </div>
            </div>
          )}
          {data.history.length > 0 && (
            <div>
              <div className="k-label mb-2">History</div>
              <div className="space-y-1">
                {data.history.map((h) => (
                  <div key={h.id} className="flex justify-between gap-3 text-[12px]">
                    <span>
                      {h.action.replace("wallet.withdrawal.", "")} · {h.actor_name ?? h.actor_kind}
                      {h.reason ? ` · ${h.reason}` : ""}
                    </span>
                    <span className="shrink-0 text-fg-3">{when(h.at)}</span>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      )}
    </Dialog>
  );
}

export function LiveWithdrawalsPage() {
  const now = useNow();
  const [tab, setTab] = React.useState<Tab>("requested");
  const [chain, setChain] = React.useState("all");
  const [q, setQ] = React.useState("");
  const [page, setPage] = React.useState(1);
  const [open, setOpen] = React.useState<number | null>(null);
  const dq = useDebounced(q.trim(), 300);
  React.useEffect(() => setPage(1), [tab, chain, dq]);
  const { data, error, loading, reload } = useApi<Paged<Withdrawal>>(`/api/wallet/withdrawals${qs({ status: tab === "all" ? undefined : tab, chain, q: dq, page, limit: PER })}`, { refreshMs: 10_000 });
  const sum = useApi<Summary>("/api/wallet/summary", { refreshMs: 15_000 });
  const s = sum.data;

  const cols: Column<Withdrawal>[] = [
    { key: "id", header: "ID", cell: (r) => <span className="font-mono text-[12px] text-fg-3">#{r.id}</span> },
    { key: "c", header: "Client", cell: (r) => <ClientCell id={r.user_id} /> },
    { key: "a", header: "Amount", align: "right", cell: (r) => <span className="k-num font-mono text-[12.5px] font-medium">{usd(r.amount)}</span> },
    { key: "n", header: "Send", align: "right", hideOn: "lg", cell: (r) => <span className="k-num font-mono text-[12px] text-fg-2">{usd(r.net_amount)}</span> },
    { key: "ch", header: "Network", cell: (r) => <ChainTag chain={r.chain} /> },
    { key: "to", header: "To", cell: (r) => <Addr a={r.to_address} /> },
    { key: "k", header: "KYC", hideOn: "xl", cell: (r) => <span className="text-[12px] text-fg-3">{r.kyc_status ?? "—"}</span> },
    { key: "s", header: "Status", cell: (r) => <Status map={WD_STATUS} s={r.status} /> },
    { key: "t", header: "Requested", align: "right", cell: (r) => <span className="whitespace-nowrap text-[11.5px] text-fg-3" title={when(r.created_at)}>{ago(r.created_at, now)}</span> },
  ];

  return (
    <div className="pb-10">
      <PageHeader
        title="Withdrawals"
        subtitle="Review each request with the risk checklist, approve or reject it, then record the payout hash. The payout is verified on chain before the withdrawal completes."
        actions={
          <Button variant="surface" size="lg" onClick={() => (reload(), sum.reload())}>
            <RefreshCw /> Refresh
          </Button>
        }
      />
      <div className="grid grid-cols-2 gap-4 xl:grid-cols-4">
        <KpiCard label="To review" icon={<Clock />} value={<span className="k-num">{s?.withdrawals.requested ?? "—"}</span>} chip="Requested" chipTone="warn" />
        <KpiCard label="To pay" icon={<Send />} value={<span className="k-num">{s?.withdrawals.approved ?? "—"}</span>} chip={s ? `${s.withdrawals.paid} verifying on chain` : "—"} delay={0.04} />
        <KpiCard label="Open amount" icon={<CircleDollarSign />} value={<span className="k-num">{s ? `$${usd2(s.withdrawals.open_amount)}` : "—"}</span>} chip="Locked in client wallets" delay={0.08} />
        <KpiCard label="Paid today" icon={<ArrowUpRight />} value={<span className="k-num">{s ? `$${usd2(s.withdrawals.completed_today)}` : "—"}</span>} chip="Completed · GMT+3 day" delay={0.12} />
      </div>
      <Card className="mt-4 px-4 py-5 sm:px-6">
        <div className="mb-4 flex flex-wrap items-center gap-3">
          <Tabs
            value={tab}
            onChange={setTab}
            tabs={[
              { value: "requested", label: "To review", count: s?.withdrawals.requested },
              { value: "approved", label: "To pay", count: s?.withdrawals.approved },
              { value: "paid", label: "Verifying", count: s?.withdrawals.paid },
              { value: "completed", label: "Completed" },
              { value: "all", label: "All" },
            ]}
          />
          <div className="ml-auto flex flex-wrap items-center gap-2">
            <FilterSelect label="Network" value={chain} onChange={setChain} options={[{ value: "all", label: "All" }, { value: "bsc", label: "BNB Chain" }, { value: "tron", label: "TRON" }]} />
            <div className="flex h-9 items-center gap-2 rounded-full border border-line bg-surface-2 px-3.5">
              <Search className="size-3.5 text-fg-3" />
              <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="ID, address or payout hash" className="w-48 bg-transparent text-[13px] outline-none placeholder:text-fg-3" aria-label="Search withdrawals" />
            </div>
          </div>
        </div>
        {error ? (
          <ErrorState error={error} onRetry={reload} />
        ) : !data ? (
          <TableSkeleton />
        ) : (
          <div className={loading ? "opacity-90" : undefined}>
            <DataTable
              columns={cols}
              rows={data.items}
              dense
              pageSize={PER}
              rowKey={(r) => String(r.id)}
              onRowClick={(r) => setOpen(r.id)}
              exportName="withdrawals"
              empty={<EmptyState title="Nothing here" text="Withdrawal requests from the Client Area appear here." illustration="inbox_tray" />}
            />
            <Pager page={data.page} perPage={data.limit} total={data.total} onPage={setPage} />
          </div>
        )}
      </Card>
      <WithdrawalDrawer id={open} onClose={() => setOpen(null)} onChanged={() => (reload(), sum.reload())} />
    </div>
  );
}
