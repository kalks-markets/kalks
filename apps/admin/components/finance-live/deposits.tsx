"use client";

import * as React from "react";
import { ArrowDownLeft, Inbox, Landmark, RefreshCw, Search, TriangleAlert } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, DataTable, Dialog, EmptyState, Field, Input, KpiCard, PageHeader, Segmented, Tabs, type Column } from "@kalks/ui";
import { NowPaymentsDeposits } from "./nowpayments";
import { ErrorState, FilterSelect, Pager, TableSkeleton, ago, qs, useApi, useDebounced, useNow, when } from "@/components/live/kit";
import { useCan } from "@/components/staff-session";
import { Addr, ChainTag, ClientCell, DEP_STATUS, Row, Status, TxLink, usd, usd2, walletWrite, type Deposit, type Paged, type Summary } from "./kit";

const PER = 50;
type Tab = "queue" | "all" | "confirming" | "credited" | "failed";

function DepositDrawer({ id, onClose, onChanged }: { id: number | null; onClose: () => void; onChanged: () => void }) {
  const { data, error, reload } = useApi<{ deposit: Deposit }>(id ? `/api/wallet/deposits/${id}` : null, { refreshMs: 10_000 });
  const canWrite = useCan("finance.write");
  const [mode, setMode] = React.useState<"assign" | "reject" | null>(null);
  const [user, setUser] = React.useState("");
  const [reason, setReason] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [err, setErr] = React.useState<string | null>(null);
  const d = data?.deposit;
  React.useEffect(() => {
    setMode(null);
    setErr(null);
    setReason("");
    setUser("");
  }, [id]);
  React.useEffect(() => {
    if (mode === "assign" && d) setUser(String(d.user_id ?? d.suggested_user_id ?? ""));
  }, [mode, d]);

  const act = async (path: string, body: unknown, ok: string) => {
    setBusy(true);
    setErr(null);
    const r = await walletWrite<{ deposit: Deposit }>(`deposits/${id}/${path}`, body);
    setBusy(false);
    if (!r.ok) return setErr(r.error.message);
    toast.success(ok, { description: `Deposit #${id} · ${DEP_STATUS[r.data.deposit.status]?.label ?? r.data.deposit.status}` });
    setMode(null);
    reload();
    onChanged();
  };

  const queue = d && (d.status === "unmatched" || d.status === "review");
  return (
    <Dialog
      side="right"
      open={id !== null}
      onOpenChange={(o) => !o && onClose()}
      title={d ? `Deposit #${d.id}` : "Deposit"}
      description={d ? `${d.network} · ${d.source === "scanner" ? "found by the scanner" : "submitted by the client"}` : undefined}
      footer={
        d && canWrite ? (
          <>
            {(d.status === "failed" || d.status === "pending") && d.user_id && (
              <Button variant="surface" disabled={busy} onClick={() => act("recheck", {}, "Sent back to verification")}>
                <RefreshCw /> Re-check on chain
              </Button>
            )}
            {(queue || d.status === "pending" || d.status === "failed") && (
              <Button variant="down-outline" disabled={busy} onClick={() => setMode("reject")}>
                Reject
              </Button>
            )}
            {queue && (
              <Button variant="ember" disabled={busy} onClick={() => setMode("assign")}>
                {d.status === "unmatched" ? "Assign to client" : "Approve & credit"}
              </Button>
            )}
          </>
        ) : undefined
      }
    >
      {error ? (
        <ErrorState error={error} onRetry={reload} />
      ) : !d ? (
        <TableSkeleton rows={4} />
      ) : (
        <div className="space-y-5">
          <div className="k-row flex items-center justify-between px-4 py-4">
            <div>
              <div className="k-num text-[26px] font-semibold">{usd(d.amount ?? d.expected_amount)} USDT</div>
              {d.expected_amount && d.amount && Number(d.expected_amount) !== Number(d.amount) && <div className="text-[12px] text-warn">Requested {usd(d.expected_amount)} USDT</div>}
            </div>
            <Status map={DEP_STATUS} s={d.status} />
          </div>
          {d.review_reason && (
            <div className="flex gap-2 rounded-[14px] border border-warn/30 bg-warn-soft px-3.5 py-2.5 text-[12.5px]">
              <TriangleAlert className="mt-0.5 size-3.5 shrink-0 text-warn" /> {d.review_reason}
            </div>
          )}
          {d.failure_reason && <div className="rounded-[14px] border border-down/30 bg-down-soft px-3.5 py-2.5 text-[12.5px]">{d.failure_reason}</div>}
          <div>
            <Row k="Client" v={<ClientCell id={d.user_id} />} />
            {d.suggested_user_id && !d.user_id && <Row k="Suggested" v={<ClientCell id={d.suggested_user_id} />} />}
            <Row k="Network" v={<ChainTag chain={d.chain} />} />
            <Row k="Transaction" v={<TxLink hash={d.tx_hash} url={d.explorer_url} head={10} tail={8} />} />
            <Row k="From" v={<Addr a={d.from_address} full />} />
            <Row k="To (company)" v={<Addr a={d.to_address} full />} />
            <Row k="Block" v={d.block_number ? <span className="font-mono text-[12px]">{d.block_number.toLocaleString("en-US")}{d.block_time ? ` · ${when(d.block_time, true)}` : ""}</span> : "—"} />
            <Row k="Confirmations" v={<span className="k-num">{d.confirmations} / {d.required_confirmations}</span>} />
            <Row k="Deposit request" v={d.intent ? <span className="text-[12px]">{d.intent.id} · {usd(d.intent.amount)} USDT · {when(d.intent.created_at)}</span> : "—"} />
            <Row k="Credited" v={d.credited_at ? `${when(d.credited_at, true)}${d.ledger_txn_id ? ` · txn ${d.ledger_txn_id}` : ""}` : "—"} />
            {d.assigned_by && <Row k="Assigned by" v={d.assigned_by} />}
            <Row k="Created" v={when(d.created_at, true)} />
          </div>
          {!!d.sender_history?.length && (
            <div>
              <div className="k-label mb-2">Earlier deposits from this sender</div>
              <div className="space-y-1.5">
                {d.sender_history.map((h) => (
                  <div key={h.user_id} className="k-row flex items-center justify-between px-3 py-2">
                    <ClientCell id={h.user_id} />
                    <span className="text-[12px] text-fg-3">
                      {h.deposits} deposit{h.deposits === 1 ? "" : "s"} · last {when(h.last)}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          )}
          {mode && (
            <div className="space-y-3 rounded-[16px] border border-line bg-surface-2 p-4">
              <div className="text-[13.5px] font-medium">{mode === "assign" ? (d.status === "unmatched" ? "Assign this deposit to a client" : "Credit this deposit to the client") : "Reject this deposit"}</div>
              {mode === "assign" && d.status === "unmatched" && (
                <Field label="Client ID" hint="The client's number from Clients (e.g. 42).">
                  <Input value={user} onChange={(e) => setUser(e.target.value.replace(/\D/g, ""))} placeholder="42" aria-label="Client ID" />
                </Field>
              )}
              <Field label="Reason" hint="Kept in the audit log.">
                <Input value={reason} onChange={(e) => setReason(e.target.value)} placeholder={mode === "assign" ? "Matched with ticket #…" : "Not a client payment"} aria-label="Reason" />
              </Field>
              {err && <div className="text-[12.5px] text-down">{err}</div>}
              <div className="flex gap-2">
                <Button variant="ghost" onClick={() => setMode(null)}>
                  Cancel
                </Button>
                {mode === "assign" ? (
                  <Button variant="ember" disabled={busy || reason.trim().length < 3 || !Number(user || d.user_id)} onClick={() => act("assign", { user_id: Number(user || d.user_id), reason }, "Deposit assigned")}>
                    Confirm
                  </Button>
                ) : (
                  <Button variant="down-outline" disabled={busy || reason.trim().length < 3} onClick={() => act("reject", { reason }, "Deposit rejected")}>
                    Reject deposit
                  </Button>
                )}
              </div>
            </div>
          )}
        </div>
      )}
    </Dialog>
  );
}

export function LiveDepositsPage() {
  const now = useNow();
  // on-chain USDT deposits, or crypto deposits through NOWPayments (?source=nowpayments)
  const [source, setSourceState] = React.useState<"chain" | "nowpayments">(() => (typeof window !== "undefined" && new URLSearchParams(window.location.search).get("source") === "nowpayments" ? "nowpayments" : "chain"));
  const setSource = (s: "chain" | "nowpayments") => {
    setSourceState(s);
    window.history.replaceState(window.history.state, "", s === "nowpayments" ? "?source=nowpayments" : window.location.pathname);
  };
  const [tab, setTab] = React.useState<Tab>("queue");
  const [chain, setChain] = React.useState("all");
  const [q, setQ] = React.useState("");
  const [page, setPage] = React.useState(1);
  const [open, setOpen] = React.useState<number | null>(null);
  const dq = useDebounced(q.trim(), 300);
  React.useEffect(() => setPage(1), [tab, chain, dq]);
  const status = tab === "all" ? undefined : tab;
  const { data, error, loading, reload } = useApi<Paged<Deposit>>(`/api/wallet/deposits${qs({ status, chain, q: dq, page, limit: PER })}`, { refreshMs: 10_000 });
  const sum = useApi<Summary>("/api/wallet/summary", { refreshMs: 15_000 });
  const s = sum.data;

  const cols: Column<Deposit>[] = [
    { key: "id", header: "ID", cell: (r) => <span className="font-mono text-[12px] text-fg-3">#{r.id}</span> },
    { key: "c", header: "Client", cell: (r) => <ClientCell id={r.user_id ?? null} /> },
    { key: "a", header: "Amount", align: "right", cell: (r) => <span className="k-num font-mono text-[12.5px] font-medium">{usd(r.amount ?? r.expected_amount)}</span> },
    { key: "n", header: "Network", cell: (r) => <ChainTag chain={r.chain} /> },
    { key: "h", header: "Transaction", cell: (r) => <TxLink hash={r.tx_hash} url={r.explorer_url} /> },
    { key: "f", header: "From", hideOn: "xl", cell: (r) => <Addr a={r.from_address} /> },
    { key: "cf", header: "Conf.", align: "right", cell: (r) => <span className="k-num text-[12px]">{r.status === "credited" ? "done" : `${r.confirmations}/${r.required_confirmations}`}</span> },
    { key: "s", header: "Status", cell: (r) => <Status map={DEP_STATUS} s={r.status} /> },
    { key: "t", header: "Received", align: "right", cell: (r) => <span className="whitespace-nowrap text-[11.5px] text-fg-3" title={when(r.created_at)}>{ago(r.created_at, now)}</span> },
  ];

  return (
    <div className="pb-10">
      <PageHeader
        title="Deposits"
        subtitle="USDT deposits on BNB Chain and TRON, verified on chain and credited after the confirmations, and crypto deposits through NOWPayments. Unclaimed transfers and held deposits wait in the queue."
        actions={
          <Button variant="surface" size="lg" onClick={() => (reload(), sum.reload())}>
            <RefreshCw /> Refresh
          </Button>
        }
      />
      <div className="grid grid-cols-2 gap-4 xl:grid-cols-4">
        <KpiCard label="Queue" icon={<Inbox />} value={<span className="k-num">{s ? s.deposits.unmatched + s.deposits.review : "—"}</span>} chip={s ? `${s.deposits.unmatched} unmatched · ${s.deposits.review} review` : "—"} chipTone="warn" />
        <KpiCard label="Confirming" icon={<ArrowDownLeft />} value={<span className="k-num">{s?.deposits.open ?? "—"}</span>} chip="On chain, not credited yet" delay={0.04} />
        <KpiCard label="Credited today" icon={<Landmark />} value={<span className="k-num">{s ? `$${usd2(s.deposits.credited_today)}` : "—"}</span>} chip={s ? `${s.deposits.credited_today_count} deposits · GMT+3 day` : "—"} delay={0.08} />
        <KpiCard label="Client wallets" icon={<Landmark />} value={<span className="k-num">{s ? `$${usd2(s.wallets.liabilities)}` : "—"}</span>} chip={s ? `${s.wallets.funded} funded wallets` : "—"} delay={0.12} />
      </div>
      <Segmented
        className="mt-4"
        aria-label="Deposit source"
        value={source}
        onChange={setSource}
        options={[
          { value: "chain", label: "USDT on chain" },
          { value: "nowpayments", label: "NOWPayments" },
        ]}
      />
      {source === "nowpayments" ? (
        <div className="mt-4">
          <NowPaymentsDeposits />
        </div>
      ) : (
      <Card className="mt-4 px-4 py-5 sm:px-6">
        <div className="mb-4 flex flex-wrap items-center gap-3">
          <Tabs
            value={tab}
            onChange={setTab}
            tabs={[
              { value: "queue", label: "Queue", count: s ? s.deposits.unmatched + s.deposits.review : undefined },
              { value: "confirming", label: "Confirming" },
              { value: "credited", label: "Credited" },
              { value: "failed", label: "Failed" },
              { value: "all", label: "All" },
            ]}
          />
          <div className="ml-auto flex flex-wrap items-center gap-2">
            <FilterSelect label="Network" value={chain} onChange={setChain} options={[{ value: "all", label: "All" }, { value: "bsc", label: "BNB Chain" }, { value: "tron", label: "TRON" }]} />
            <div className="flex h-9 items-center gap-2 rounded-full border border-line bg-surface-2 px-3.5">
              <Search className="size-3.5 text-fg-3" />
              <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Tx hash or sender" className="w-44 bg-transparent text-[13px] outline-none placeholder:text-fg-3" aria-label="Search deposits" />
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
              exportName="deposits"
              empty={<EmptyState title={tab === "queue" ? "The queue is empty" : "No deposits"} text={tab === "queue" ? "Unclaimed transfers and deposits held by the auto-credit rules appear here." : "Deposits appear here as clients pay."} illustration="inbox_tray" />}
            />
            <Pager page={data.page} perPage={data.limit} total={data.total} onPage={setPage} />
          </div>
        )}
      </Card>
      )}
      <DepositDrawer id={open} onClose={() => setOpen(null)} onChanged={() => (reload(), sum.reload())} />
    </div>
  );
}
