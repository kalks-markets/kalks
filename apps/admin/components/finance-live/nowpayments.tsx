"use client";

/**
 * NOWPayments (crypto deposits) in the Back Office: the settings card on Finance → Wallet settings and the payments
 * list / review drawer on Finance → Deposits. Wallet service routes /v1/admin/nowpayments/* (services/wallet README
 * "NOWPayments"). Keys are write-only: the page only ever sees "set", the last 4 characters and who changed them.
 */
import * as React from "react";
import { CheckCircle2, Copy, KeyRound, PlugZap, RefreshCw, Save, Search, ShieldAlert, TriangleAlert } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, CardHeader, Chip, DataTable, Dialog, EmptyState, Field, Input, Tabs, Toggle, type ChipTone, type Column } from "@kalks/ui";
import { ErrorState, Pager, TableSkeleton, ago, qs, useApi, useDebounced, useNow, when } from "@/components/live/kit";
import { useCan } from "@/components/staff-session";
import { ClientCell, Row, Status, usd, usd2, walletWrite, type AuditItem, type Paged } from "./kit";

/* ------------------------------------------------------------------ */
/* Types                                                               */
/* ------------------------------------------------------------------ */

type Secret = { set: boolean; last4: string | null; updated_at: string | null; updated_by: string | null };

type NpSettings = {
  enabled: boolean;
  sandbox: boolean;
  api_key: Secret;
  ipn_secret: Secret;
  coins: string[];
  min_deposit_usd: string;
  max_deposit_usd: string;
  fee_paid_by_user: boolean;
  ipn_url: string;
  ipn_url_custom: boolean;
  ready: boolean;
  updated_at: string | null;
  updated_by: string | null;
};

type SettingsView = { settings: NpSettings; secrets_storage: boolean; default_coins: string[]; history: AuditItem[] };

type Coin = { code: string; ticker: string; name: string; network: string; needs_extra_id: boolean };

type TestResult = { ok: boolean; status: string; sandbox: boolean; merchant_coins: Coin[]; missing: string[] };

export type NpPayment = {
  id: number;
  state: "creating" | "open" | "credited" | "review" | "approval" | "rejected" | "closed" | "error";
  status: string;
  np_status: string | null;
  np_payment_id: string | null;
  order_id: string;
  user_id: number;
  parent_id: number | null;
  sandbox: boolean;
  price_amount: string;
  pay_currency: string;
  coin: Coin;
  pay_amount: string | null;
  pay_address: string | null;
  payin_extra_id: string | null;
  actually_paid: string | null;
  actually_paid_at_fiat: string | null;
  outcome_amount: string | null;
  outcome_currency: string | null;
  payin_hash: string | null;
  expires_at: string | null;
  review_reason: string | null;
  failure_reason: string | null;
  credit_amount: string | null;
  suggested_credit: string | null;
  ledger_txn_id: number | null;
  credited_at: string | null;
  credited_by: string | null;
  approval_amount: string | null;
  approval_reason: string | null;
  approval_by_name: string | null;
  approval_at: string | null;
  decided_by: string | null;
  decision_note: string | null;
  last_ipn: unknown;
  last_ipn_at: string | null;
  np_payment: unknown;
  ipn_count: number;
  checks: number;
  last_checked_at: string | null;
  check_error: string | null;
  ip: string | null;
  created_at: string;
  updated_at: string;
  history?: AuditItem[];
  threshold_usd?: string | null;
  can?: { write: boolean; approve: boolean; own_proposal: boolean };
};

type NpList = Paged<NpPayment> & { counts: { review: number; approval: number; open: number; credited_24h: number; credited_24h_usd: string } };

export const NP_STATE: Record<NpPayment["state"], { tone: ChipTone; label: string }> = {
  creating: { tone: "neutral", label: "Creating" },
  open: { tone: "info", label: "Open" },
  credited: { tone: "up", label: "Credited" },
  review: { tone: "warn", label: "Review" },
  approval: { tone: "ember", label: "Second approval" },
  rejected: { tone: "neutral", label: "Rejected" },
  closed: { tone: "neutral", label: "Closed" },
  error: { tone: "down", label: "Create failed" },
};

const coinName = (c: Coin) => (c.network ? `${c.ticker} · ${c.network}` : c.ticker);

/* ------------------------------------------------------------------ */
/* Settings (Finance → Wallet settings)                                */
/* ------------------------------------------------------------------ */

function SecretField({ label, s, value, onChange, hint, disabled }: { label: string; s: Secret; value: string; onChange: (v: string) => void; hint: string; disabled: boolean }) {
  return (
    <Field
      label={label}
      hint={hint}
      aside={
        s.set ? (
          <Chip size="sm" tone="up">
            Set ✓ …{s.last4}
          </Chip>
        ) : (
          <Chip size="sm" tone="warn">
            Not set
          </Chip>
        )
      }
    >
      <Input
        type="password"
        autoComplete="new-password"
        spellCheck={false}
        disabled={disabled}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={s.set ? "Leave empty to keep the current one" : "Paste from the NOWPayments dashboard"}
        inputClassName="font-mono text-[12.5px]"
        aria-label={label}
      />
    </Field>
  );
}

export function NowPaymentsSettingsCard() {
  const { data, error, reload } = useApi<SettingsView>("/api/wallet/nowpayments/settings");
  const canEdit = useCan("finance.settings");
  const [f, setF] = React.useState<NpSettings | null>(null);
  const [apiKey, setApiKey] = React.useState("");
  const [ipnSecret, setIpnSecret] = React.useState("");
  const [reason, setReason] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [test, setTest] = React.useState<TestResult | null>(null);
  const [testing, setTesting] = React.useState(false);
  React.useEffect(() => {
    if (data) setF(data.settings);
  }, [data]);

  if (error) return <ErrorState error={error} onRetry={reload} />;
  if (!data || !f) return <TableSkeleton rows={4} />;
  const s = data.settings;
  const set = <K extends keyof NpSettings>(k: K, v: NpSettings[K]) => setF((x) => (x ? { ...x, [k]: v } : x));
  const coinsOffered = new Set(f.coins);
  const pickable: Coin[] = test?.merchant_coins ?? [];
  const dirty = JSON.stringify(f) !== JSON.stringify(s) || apiKey.trim() !== "" || ipnSecret.trim() !== "";

  const save = async () => {
    if (reason.trim().length < 3) {
      toast.error("Enter a reason first", { description: "Every settings change is kept in the audit log with its reason." });
      return;
    }
    setBusy(true);
    const body: Record<string, unknown> = {
      enabled: f.enabled,
      sandbox: f.sandbox,
      coins: f.coins,
      min_deposit_usd: f.min_deposit_usd,
      max_deposit_usd: f.max_deposit_usd,
      fee_paid_by_user: f.fee_paid_by_user,
      ipn_url: f.ipn_url_custom ? f.ipn_url : f.ipn_url === s.ipn_url ? undefined : f.ipn_url,
      reason: reason.trim(),
    };
    if (apiKey.trim()) body.api_key = apiKey.trim();
    if (ipnSecret.trim()) body.ipn_secret = ipnSecret.trim();
    const r = await walletWrite<SettingsView>("nowpayments/settings", body, "PUT");
    setBusy(false);
    if (!r.ok) {
      toast.error("Not saved", { description: r.error.message });
      return;
    }
    toast.success("NOWPayments settings saved");
    setApiKey("");
    setIpnSecret("");
    setReason("");
    reload();
  };

  const runTest = async () => {
    setTesting(true);
    const r = await walletWrite<TestResult>("nowpayments/test", { api_key: apiKey.trim() || undefined, sandbox: f.sandbox });
    setTesting(false);
    if (!r.ok) {
      setTest(null);
      toast.error("Connection failed", { description: r.error.message });
      return;
    }
    setTest(r.data);
    toast.success(`NOWPayments answered ${r.data.status || "OK"}`, { description: `${r.data.merchant_coins.length} coins enabled in the merchant account${r.data.sandbox ? " (sandbox)" : ""}` });
  };

  const toggleCoin = (code: string) => set("coins", coinsOffered.has(code) ? f.coins.filter((c) => c !== code) : [...f.coins, code]);

  return (
    <Card>
      <CardHeader
        title="NOWPayments · crypto deposits"
        subtitle="Clients pay in BTC, ETH, USDT and other coins; the USD amount is credited to their wallet as USDT once NOWPayments reports the payment finished. The direct USDT deposits above stay as they are."
        icon={<PlugZap />}
        action={s.ready ? <Chip tone="up" dot>Live for clients</Chip> : <Chip tone="neutral">Off</Chip>}
      />
      <div className="space-y-5 px-6 pb-6 pt-4">
        {!data.secrets_storage && (
          <div className="flex gap-2 rounded-[14px] border border-down/35 bg-down-soft px-3.5 py-2.5 text-[12.5px]">
            <ShieldAlert className="mt-0.5 size-4 shrink-0 text-down" />
            WALLET_SECRETS_KEY is not set on the server, so keys can't be stored. Ask the operator to run deploy.sh (it generates the key) and restart the wallet.
          </div>
        )}
        <div className="flex flex-wrap gap-6">
          <label className="flex items-center gap-2 text-[13px]">
            <Toggle checked={f.enabled} onChange={(v) => canEdit && set("enabled", v)} disabled={!canEdit} /> Enabled (clients see “Crypto (NOWPayments)”)
          </label>
          <label className="flex items-center gap-2 text-[13px]">
            <Toggle checked={f.sandbox} onChange={(v) => canEdit && set("sandbox", v)} disabled={!canEdit} /> Sandbox (api-sandbox.nowpayments.io, test money)
          </label>
          <label className="flex items-center gap-2 text-[13px]">
            <Toggle checked={f.fee_paid_by_user} onChange={(v) => canEdit && set("fee_paid_by_user", v)} disabled={!canEdit} /> Client pays the fees
          </label>
        </div>
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
          <SecretField label="API key" s={s.api_key} value={apiKey} onChange={setApiKey} disabled={!canEdit} hint="NOWPayments dashboard → Settings → Payments → API keys. Stored encrypted, never shown again." />
          <SecretField label="IPN secret" s={s.ipn_secret} value={ipnSecret} onChange={setIpnSecret} disabled={!canEdit} hint="NOWPayments dashboard → Settings → Payments → Instant payment notifications → IPN secret key." />
        </div>
        <Field label="IPN callback URL" hint="Sent with every payment. Also paste it in the NOWPayments dashboard (Instant payment notifications). Must reach this Client Area over HTTPS.">
          <div className="flex items-center gap-2">
            <Input value={f.ipn_url} disabled={!canEdit} onChange={(e) => setF((x) => (x ? { ...x, ipn_url: e.target.value, ipn_url_custom: true } : x))} inputClassName="font-mono text-[12.5px]" aria-label="IPN callback URL" className="flex-1" />
            <Button
              variant="surface"
              onClick={() => {
                navigator.clipboard?.writeText(f.ipn_url).catch(() => {});
                toast.success("IPN URL copied");
              }}
            >
              <Copy /> Copy
            </Button>
          </div>
        </Field>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <Field label="Minimum deposit (USD)" hint="NOWPayments' own minimum per coin applies when it is higher.">
            <Input disabled={!canEdit} value={f.min_deposit_usd} onChange={(e) => set("min_deposit_usd", e.target.value)} aria-label="Minimum deposit (USD)" />
          </Field>
          <Field label="Maximum deposit (USD)">
            <Input disabled={!canEdit} value={f.max_deposit_usd} onChange={(e) => set("max_deposit_usd", e.target.value)} aria-label="Maximum deposit (USD)" />
          </Field>
        </div>
        <div>
          <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
            <div className="text-[12.5px] font-semibold text-fg-2">Coins offered to clients ({f.coins.length})</div>
            <Button variant="surface" size="sm" disabled={testing || (!s.api_key.set && !apiKey.trim())} onClick={runTest}>
              {testing ? <RefreshCw className="animate-spin" /> : <PlugZap />} Test connection
            </Button>
          </div>
          <div className="flex flex-wrap gap-1.5">
            {f.coins.map((c) => (
              <button key={c} type="button" disabled={!canEdit} onClick={() => toggleCoin(c)} title="Remove" className="rounded-full border border-ember/40 bg-ember-soft px-2.5 py-1 font-mono text-[12px] disabled:opacity-60">
                {c} {canEdit && "×"}
              </button>
            ))}
          </div>
          {test && test.missing.length > 0 && (
            <div className="mt-2 flex gap-2 text-[12px] text-warn">
              <TriangleAlert className="mt-0.5 size-3.5 shrink-0" /> Not enabled in the merchant account: {test.missing.join(", ")}. Enable them in NOWPayments (coins settings) or remove them here.
            </div>
          )}
          {pickable.length > 0 && (
            <div className="mt-3">
              <div className="mb-1.5 text-[12px] text-fg-3">Enabled in your NOWPayments account (click to offer):</div>
              <div className="flex max-h-48 flex-wrap gap-1.5 overflow-auto">
                {pickable.map((c) => (
                  <button
                    key={c.code}
                    type="button"
                    disabled={!canEdit}
                    onClick={() => toggleCoin(c.code)}
                    title={coinName(c)}
                    className={`rounded-full border px-2.5 py-1 text-[12px] ${coinsOffered.has(c.code) ? "border-ember/40 bg-ember-soft" : "border-line bg-surface-2 hover:border-[var(--k-border-top)]"}`}
                  >
                    <span className="font-mono">{c.code}</span> <span className="text-fg-3">{c.network || c.name}</span>
                  </button>
                ))}
              </div>
            </div>
          )}
          {!test && <div className="mt-2 text-[12px] text-fg-3">Run “Test connection” to check the key and pick from the coins enabled in your merchant account. Suggested: {data.default_coins.join(", ")}.</div>}
        </div>
        {canEdit ? (
          <div className="flex flex-col gap-3 border-t border-line pt-4 sm:flex-row sm:items-end">
            <Field label="Reason for the change" hint="Kept in the audit log (keys are never logged)." className="flex-1">
              <Input value={reason} onChange={(e) => setReason(e.target.value)} placeholder="e.g. Go live with NOWPayments (ticket FIN-140)" aria-label="Reason for the NOWPayments change" />
            </Field>
            <Button variant="ember" disabled={!dirty || busy} onClick={save}>
              <Save /> Save NOWPayments
            </Button>
          </div>
        ) : (
          <div className="rounded-[14px] border border-line bg-surface-2 px-4 py-3 text-[12.5px] text-fg-3">Read-only: only admins change NOWPayments settings.</div>
        )}
        {(s.api_key.updated_at || s.ipn_secret.updated_at) && (
          <div className="flex flex-wrap gap-4 text-[12px] text-fg-3">
            {s.api_key.updated_at && (
              <span className="inline-flex items-center gap-1">
                <KeyRound className="size-3.5" /> API key changed {when(s.api_key.updated_at)}
                {s.api_key.updated_by ? ` by ${s.api_key.updated_by}` : ""}
              </span>
            )}
            {s.ipn_secret.updated_at && (
              <span className="inline-flex items-center gap-1">
                <KeyRound className="size-3.5" /> IPN secret changed {when(s.ipn_secret.updated_at)}
                {s.ipn_secret.updated_by ? ` by ${s.ipn_secret.updated_by}` : ""}
              </span>
            )}
          </div>
        )}
        {data.history.length > 0 && (
          <div>
            <div className="k-label mb-2">Change history</div>
            <div className="space-y-1.5">
              {data.history.slice(0, 8).map((h) => (
                <div key={h.id} className="k-row flex justify-between gap-2 px-3 py-2 text-[12.5px]">
                  <span>
                    {h.actor_name ?? h.actor_kind}
                    {h.reason ? ` · ${h.reason}` : ""}
                  </span>
                  <span className="shrink-0 text-fg-3">{when(h.at)}</span>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Payments (Finance → Deposits → NOWPayments)                         */
/* ------------------------------------------------------------------ */

function Json({ v }: { v: unknown }) {
  if (v === null || v === undefined) return <span className="text-fg-3">—</span>;
  return <pre className="max-h-64 overflow-auto rounded-[12px] border border-line bg-surface-2 p-3 font-mono text-[11.5px] leading-relaxed">{JSON.stringify(v, null, 2)}</pre>;
}

function PaymentDrawer({ id, onClose, onChanged }: { id: number | null; onClose: () => void; onChanged: () => void }) {
  const { data, error, reload } = useApi<{ payment: NpPayment }>(id ? `/api/wallet/nowpayments/payments/${id}` : null, { refreshMs: 10_000 });
  const canWrite = useCan("finance.write");
  const canApprove = useCan("finance.adjust_approve");
  const [mode, setMode] = React.useState<"approve" | "reject" | null>(null);
  const [amount, setAmount] = React.useState("");
  const [reason, setReason] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [err, setErr] = React.useState<string | null>(null);
  const p = data?.payment;
  React.useEffect(() => {
    setMode(null);
    setErr(null);
    setReason("");
    setAmount("");
  }, [id]);
  React.useEffect(() => {
    if (mode === "approve" && p) setAmount(p.state === "approval" ? (p.approval_amount ?? "") : (p.suggested_credit ?? p.price_amount));
  }, [mode, p]);

  const act = async (path: string, body: unknown, ok: string) => {
    setBusy(true);
    setErr(null);
    const r = await walletWrite<{ payment: NpPayment }>(`nowpayments/payments/${id}/${path}`, body);
    setBusy(false);
    if (!r.ok) return setErr(r.error.message);
    toast.success(ok, { description: `Payment #${id} · ${NP_STATE[r.data.payment.state]?.label ?? r.data.payment.state}` });
    setMode(null);
    reload();
    onChanged();
  };

  const own = !!p?.can?.own_proposal;
  return (
    <Dialog
      side="right"
      open={id !== null}
      onOpenChange={(o) => !o && onClose()}
      title={p ? `NOWPayments #${p.id}` : "NOWPayments payment"}
      description={p ? `${coinName(p.coin)}${p.sandbox ? " · sandbox" : ""}${p.parent_id ? ` · repeated deposit of #${p.parent_id}` : ""}` : undefined}
      footer={
        p ? (
          <>
            {canWrite && ["open", "closed", "review", "approval"].includes(p.state) && p.np_payment_id && (
              <Button variant="surface" disabled={busy} onClick={() => act("refresh", {}, "Re-checked with NOWPayments")}>
                <RefreshCw /> Re-check
              </Button>
            )}
            {p.state === "review" && canWrite && (
              <>
                <Button variant="down-outline" disabled={busy} onClick={() => setMode("reject")}>
                  Reject
                </Button>
                <Button variant="ember" disabled={busy} onClick={() => setMode("approve")}>
                  Approve & credit
                </Button>
              </>
            )}
            {p.state === "approval" && (own ? canWrite : canApprove) && (
              <Button variant="down-outline" disabled={busy} onClick={() => setMode("reject")}>
                {own ? "Withdraw proposal" : "Reject"}
              </Button>
            )}
            {p.state === "approval" && canApprove && !own && (
              <Button variant="ember" disabled={busy} onClick={() => setMode("approve")}>
                Second approval & credit
              </Button>
            )}
          </>
        ) : undefined
      }
    >
      {error ? (
        <ErrorState error={error} onRetry={reload} />
      ) : !p ? (
        <TableSkeleton rows={4} />
      ) : (
        <div className="space-y-5">
          <div className="k-row flex items-center justify-between px-4 py-4">
            <div>
              <div className="k-num text-[26px] font-semibold">{usd(p.credit_amount ?? p.price_amount)} USD</div>
              <div className="text-[12px] text-fg-3">
                {p.pay_amount ?? "—"} {p.coin.ticker} asked · {p.actually_paid ?? "0"} paid
              </div>
            </div>
            <Status map={NP_STATE} s={p.state} />
          </div>
          {p.review_reason && (
            <div className="flex gap-2 rounded-[14px] border border-warn/30 bg-warn-soft px-3.5 py-2.5 text-[12.5px]">
              <TriangleAlert className="mt-0.5 size-3.5 shrink-0 text-warn" /> {p.review_reason}
            </div>
          )}
          {p.state === "approval" && (
            <div className="rounded-[14px] border border-ember/35 bg-ember-soft px-3.5 py-2.5 text-[12.5px]">
              {p.approval_by_name} proposed crediting <b>{usd(p.approval_amount)} USD</b> {p.approval_at ? when(p.approval_at) : ""}
              {p.approval_reason ? ` · ${p.approval_reason}` : ""}. Above the four-eyes threshold ({usd(p.threshold_usd)} USD): another staff member with “approve adjustments” credits it.
            </div>
          )}
          {p.failure_reason && <div className="rounded-[14px] border border-down/30 bg-down-soft px-3.5 py-2.5 text-[12.5px]">{p.failure_reason}</div>}
          {p.check_error && <div className="text-[12px] text-down">Last check: {p.check_error}</div>}
          <div>
            <Row k="Client" v={<ClientCell id={p.user_id} />} />
            <Row k="NOWPayments status" v={<span className="font-mono text-[12px]">{p.np_status ?? "—"}</span>} />
            <Row k="NOWPayments id" v={<span className="font-mono text-[12px]">{p.np_payment_id ?? "—"}</span>} />
            <Row k="Order id" v={<span className="font-mono text-[12px]">{p.order_id}</span>} />
            <Row k="Price" v={`${usd(p.price_amount)} USD`} />
            <Row k="Pay amount" v={<span className="font-mono text-[12px]">{p.pay_amount ?? "—"} {p.coin.ticker}</span>} />
            <Row k="Actually paid" v={<span className="font-mono text-[12px]">{p.actually_paid ?? "—"} {p.coin.ticker}{p.actually_paid_at_fiat && Number(p.actually_paid_at_fiat) > 0 ? ` ≈ ${usd(p.actually_paid_at_fiat)} USD` : ""}</span>} />
            <Row k="Outcome" v={<span className="font-mono text-[12px]">{p.outcome_amount ?? "—"} {p.outcome_currency ?? ""}</span>} />
            <Row k="Address" v={<span className="break-all font-mono text-[12px]">{p.pay_address ?? "—"}</span>} />
            {p.payin_extra_id && <Row k="Memo / tag" v={<span className="font-mono text-[12px]">{p.payin_extra_id}</span>} />}
            <Row k="Pay-in hash" v={<span className="break-all font-mono text-[12px]">{p.payin_hash ?? "—"}</span>} />
            <Row k="Expires" v={p.expires_at ? when(p.expires_at, true) : "—"} />
            <Row k="Credited" v={p.credited_at ? `${usd(p.credit_amount)} USD · ${when(p.credited_at, true)}${p.ledger_txn_id ? ` · txn ${p.ledger_txn_id}` : ""}${p.credited_by ? ` · ${p.credited_by}` : ""}` : "—"} />
            <Row k="IPNs / checks" v={`${p.ipn_count} IPN${p.ipn_count === 1 ? "" : "s"}${p.last_ipn_at ? ` (last ${when(p.last_ipn_at, true)})` : ""} · ${p.checks} API checks`} />
            <Row k="Created" v={`${when(p.created_at, true)}${p.ip ? ` · ${p.ip}` : ""}`} />
          </div>
          {mode && (
            <div className="space-y-3 rounded-[16px] border border-line bg-surface-2 p-4">
              <div className="text-[13.5px] font-medium">{mode === "approve" ? (p.state === "approval" ? "Credit the proposed amount" : "Credit this payment") : own ? "Withdraw your proposal (back to review)" : "Reject this payment"}</div>
              {mode === "approve" && (
                <Field label="Amount to credit (USD = USDT)" hint={p.state === "approval" ? "Fixed by the first approver." : `Suggested from what was paid: ${usd(p.suggested_credit)} USD. Above ${p.threshold_usd ? `${usd(p.threshold_usd)} USD` : "the threshold"} a second staff member must approve.`}>
                  <Input value={amount} disabled={p.state === "approval"} onChange={(e) => setAmount(e.target.value.replace(/[^\d.]/g, ""))} aria-label="Amount to credit" />
                </Field>
              )}
              <Field label="Reason" hint="Kept in the audit log; a rejection reason is also sent to the client.">
                <Input value={reason} onChange={(e) => setReason(e.target.value)} placeholder={mode === "approve" ? "Client confirmed the partial payment" : "Refunded by NOWPayments"} aria-label="Reason" />
              </Field>
              {err && <div className="text-[12.5px] text-down">{err}</div>}
              <div className="flex gap-2">
                <Button variant="ghost" onClick={() => setMode(null)}>
                  Cancel
                </Button>
                {mode === "approve" ? (
                  <Button variant="ember" disabled={busy || (p.state === "review" && (reason.trim().length < 3 || !(Number(amount) > 0)))} onClick={() => act("approve", p.state === "approval" ? { reason } : { amount, reason }, "Payment approved")}>
                    <CheckCircle2 /> Confirm
                  </Button>
                ) : (
                  <Button variant="down-outline" disabled={busy || reason.trim().length < 3} onClick={() => act("reject", { reason }, own ? "Proposal withdrawn" : "Payment rejected")}>
                    Confirm
                  </Button>
                )}
              </div>
            </div>
          )}
          <div>
            <div className="k-label mb-2">Last IPN (signature verified)</div>
            <Json v={p.last_ipn} />
          </div>
          <div>
            <div className="k-label mb-2">NOWPayments API answer</div>
            <Json v={p.np_payment} />
          </div>
          {!!p.history?.length && (
            <div>
              <div className="k-label mb-2">History</div>
              <div className="space-y-1.5">
                {p.history.map((h) => (
                  <div key={h.id} className="k-row px-3 py-2 text-[12.5px]">
                    <div className="flex justify-between gap-2">
                      <span className="font-medium">{h.action.replace("wallet.nowpayments.", "")}</span>
                      <span className="text-fg-3">{when(h.at)}</span>
                    </div>
                    <div className="text-fg-3">
                      {h.actor_name ?? h.actor_kind}
                      {h.reason ? ` · ${h.reason}` : ""}
                    </div>
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

type NpTab = "queue" | "open" | "credited" | "closed" | "all";
const PER = 50;

export function NowPaymentsDeposits() {
  const now = useNow();
  const [tab, setTab] = React.useState<NpTab>("queue");
  const [q, setQ] = React.useState("");
  const [page, setPage] = React.useState(1);
  const [open, setOpen] = React.useState<number | null>(null);
  const dq = useDebounced(q.trim(), 300);
  React.useEffect(() => setPage(1), [tab, dq]);
  const { data, error, loading, reload } = useApi<NpList>(`/api/wallet/nowpayments/payments${qs({ status: tab === "all" ? undefined : tab, q: dq, page, limit: PER })}`, { refreshMs: 10_000 });
  const cols: Column<NpPayment>[] = [
    { key: "id", header: "ID", cell: (r) => <span className="font-mono text-[12px] text-fg-3">#{r.id}</span> },
    { key: "c", header: "Client", cell: (r) => <ClientCell id={r.user_id} /> },
    { key: "a", header: "USD", align: "right", cell: (r) => <span className="k-num font-mono text-[12.5px] font-medium">{usd(r.credit_amount ?? r.price_amount)}</span> },
    { key: "coin", header: "Coin", cell: (r) => <span className="text-[12px]">{coinName(r.coin)}</span> },
    { key: "paid", header: "Paid / asked", hideOn: "xl", cell: (r) => <span className="font-mono text-[12px]">{r.actually_paid ?? "0"} / {r.pay_amount ?? "—"}</span> },
    { key: "np", header: "NOWPayments", cell: (r) => <span className="font-mono text-[12px] text-fg-2">{r.np_status ?? "—"}</span> },
    { key: "s", header: "Status", cell: (r) => <Status map={NP_STATE} s={r.state} /> },
    { key: "t", header: "Created", align: "right", cell: (r) => <span className="whitespace-nowrap text-[11.5px] text-fg-3" title={when(r.created_at)}>{ago(r.created_at, now)}</span> },
  ];
  const c = data?.counts;
  return (
    <Card className="px-4 py-5 sm:px-6">
      <div className="mb-4 flex flex-wrap items-center gap-3">
        <Tabs
          value={tab}
          onChange={setTab}
          tabs={[
            { value: "queue", label: "Review", count: c ? c.review + c.approval : undefined },
            { value: "open", label: "Open", count: c?.open },
            { value: "credited", label: "Credited" },
            { value: "closed", label: "Closed" },
            { value: "all", label: "All" },
          ]}
        />
        <span className="text-[12px] text-fg-3">{c ? `Last 24 h: ${c.credited_24h} credited · $${usd2(c.credited_24h_usd)}` : ""}</span>
        <div className="ml-auto flex items-center gap-2">
          <div className="flex h-9 items-center gap-2 rounded-full border border-line bg-surface-2 px-3.5">
            <Search className="size-3.5 text-fg-3" />
            <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Payment id, order id, address" className="w-52 bg-transparent text-[13px] outline-none placeholder:text-fg-3" aria-label="Search NOWPayments payments" />
          </div>
          <Button variant="surface" onClick={reload}>
            <RefreshCw /> Refresh
          </Button>
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
            exportName="nowpayments"
            empty={<EmptyState title={tab === "queue" ? "Nothing to review" : "No payments"} text={tab === "queue" ? "Partial, over- and late payments and repeated deposits wait here for staff." : "Crypto deposits through NOWPayments appear here."} illustration="inbox_tray" />}
          />
          <Pager page={data.page} perPage={data.limit} total={data.total} onPage={setPage} />
        </div>
      )}
      <PaymentDrawer id={open} onClose={() => setOpen(null)} onChanged={reload} />
    </Card>
  );
}
