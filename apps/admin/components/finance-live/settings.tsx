"use client";

import * as React from "react";
import { History, Save, ShieldAlert } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, CardHeader, Chip, Field, Input, PageHeader, Toggle } from "@kalks/ui";
import { ErrorState, TableSkeleton, useApi, when } from "@/components/live/kit";
import { useCan } from "@/components/staff-session";
import { Addr, CHAIN_NAME, CHAIN_SHORT, walletWrite, type AuditItem, type Chain } from "./kit";
import { NowPaymentsSettingsCard } from "./nowpayments";

type ChainSettings = {
  chain: Chain;
  network: string;
  token_contract: string;
  receiving_address: string;
  payout_address: string | null;
  confirmations: number;
  deposits_enabled: boolean;
  withdrawals_enabled: boolean;
  min_deposit: string;
  withdraw_fee: string;
  available: boolean;
};

type Limits = {
  withdraw_min: string;
  withdraw_max: string;
  withdraw_daily_max: string;
  withdraw_fee_flat: string;
  withdraw_fee_pct: string;
  deposit_cooldown_hours: number;
  intent_ttl_minutes: number;
};

type Settings = {
  limits: Limits;
  limits_updated_at: string | null;
  limits_updated_by: string | null;
  chains: ChainSettings[];
  missing_chains: Chain[];
  addresses: { chain: Chain; address: string; role: string; active: boolean; created_at: string }[];
  history: AuditItem[];
};

const LIMIT_FIELDS: [keyof Limits, string, string][] = [
  ["withdraw_min", "Minimum withdrawal", "USDT"],
  ["withdraw_max", "Maximum per withdrawal", "USDT"],
  ["withdraw_daily_max", "Daily limit per client", "USDT"],
  ["withdraw_fee_flat", "Withdrawal fee (flat)", "USDT"],
  ["withdraw_fee_pct", "Withdrawal fee (percent)", "%"],
  ["deposit_cooldown_hours", "Wait after a deposit", "hours"],
  ["intent_ttl_minutes", "Deposit request valid for", "minutes"],
];

function ChainForm({ c, canEdit, onSave }: { c: ChainSettings; canEdit: boolean; onSave: (patch: Record<string, unknown>) => Promise<boolean> }) {
  const [f, setF] = React.useState(c);
  React.useEffect(() => setF(c), [c]);
  const dirty = JSON.stringify(f) !== JSON.stringify(c);
  const addrChanged = f.receiving_address.trim() !== c.receiving_address || (f.payout_address ?? "").trim() !== (c.payout_address ?? "");
  const set = <K extends keyof ChainSettings>(k: K, v: ChainSettings[K]) => setF((x) => ({ ...x, [k]: v }));
  return (
    <Card>
      <CardHeader
        title={`${CHAIN_NAME[c.chain]} · USDT ${CHAIN_SHORT[c.chain]}`}
        subtitle={<span className="font-mono text-[11.5px]">Token {c.token_contract}</span>}
        action={!c.available ? <Chip size="sm" tone="warn">No RPC configured</Chip> : undefined}
      />
      <div className="space-y-4 px-6 pb-6 pt-4">
        <Field label="Receiving address (deposits)" hint="Clients send deposits here. Earlier addresses stay recognised for late payments.">
          <Input value={f.receiving_address} disabled={!canEdit} onChange={(e) => set("receiving_address", e.target.value)} inputClassName="font-mono text-[12.5px]" aria-label={`${c.chain} receiving address`} />
        </Field>
        <Field label="Payout address (optional)" hint="Withdrawals must be paid from the receiving or this address.">
          <Input value={f.payout_address ?? ""} disabled={!canEdit} onChange={(e) => set("payout_address", e.target.value)} placeholder="Same as receiving" inputClassName="font-mono text-[12.5px]" aria-label={`${c.chain} payout address`} />
        </Field>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          <Field label="Confirmations">
            <Input type="number" min={1} max={500} disabled={!canEdit} value={String(f.confirmations)} onChange={(e) => set("confirmations", Number(e.target.value))} aria-label={`${c.chain} confirmations`} />
          </Field>
          <Field label="Minimum deposit (USDT)">
            <Input disabled={!canEdit} value={f.min_deposit} onChange={(e) => set("min_deposit", e.target.value)} aria-label={`${c.chain} minimum deposit`} />
          </Field>
          <Field label="Network fee on withdrawals">
            <Input disabled={!canEdit} value={f.withdraw_fee} onChange={(e) => set("withdraw_fee", e.target.value)} aria-label={`${c.chain} withdrawal network fee`} />
          </Field>
        </div>
        <div className="flex flex-wrap gap-6">
          <label className="flex items-center gap-2 text-[13px]">
            <Toggle checked={f.deposits_enabled} onChange={(v) => canEdit && set("deposits_enabled", v)} /> Deposits enabled
          </label>
          <label className="flex items-center gap-2 text-[13px]">
            <Toggle checked={f.withdrawals_enabled} onChange={(v) => canEdit && set("withdrawals_enabled", v)} /> Withdrawals enabled
          </label>
        </div>
        {addrChanged && (
          <div className="flex gap-2 rounded-[14px] border border-warn/35 bg-warn-soft px-3.5 py-2.5 text-[12.5px]">
            <ShieldAlert className="mt-0.5 size-4 shrink-0 text-warn" />
            Changing an address redirects client money. Check it character by character against the hardware wallet before saving.
          </div>
        )}
        {canEdit && (
          <Button
            variant="ember"
            disabled={!dirty}
            onClick={() =>
              onSave({
                chain: c.chain,
                receiving_address: f.receiving_address.trim(),
                payout_address: (f.payout_address ?? "").trim(),
                confirmations: f.confirmations,
                deposits_enabled: f.deposits_enabled,
                withdrawals_enabled: f.withdrawals_enabled,
                min_deposit: f.min_deposit,
                withdraw_fee: f.withdraw_fee,
              })
            }
          >
            <Save /> Save {CHAIN_SHORT[c.chain]}
          </Button>
        )}
      </div>
    </Card>
  );
}

export function LiveWalletSettingsPage() {
  const { data, error, reload } = useApi<Settings>("/api/wallet/settings");
  const canEdit = useCan("finance.settings");
  const [limits, setLimits] = React.useState<Limits | null>(null);
  const [reason, setReason] = React.useState("");
  React.useEffect(() => {
    if (data) setLimits(data.limits);
  }, [data]);

  const save = async (body: Record<string, unknown>): Promise<boolean> => {
    if (reason.trim().length < 3) {
      toast.error("Enter a reason first", { description: "Every settings change is kept in the audit log with its reason." });
      return false;
    }
    const r = await walletWrite<Settings>("settings", { ...body, reason: reason.trim() }, "PUT");
    if (!r.ok) {
      toast.error("Not saved", { description: r.error.message });
      return false;
    }
    toast.success("Settings saved");
    setReason("");
    reload();
    return true;
  };

  return (
    <div className="pb-10">
      <PageHeader title="Wallet settings" subtitle="Company addresses, confirmations, deposit and withdrawal limits and fees, and NOWPayments crypto deposits. Every change is audited with before and after values." />
      {error ? (
        <ErrorState error={error} onRetry={reload} />
      ) : !data || !limits ? (
        <TableSkeleton />
      ) : (
        <div className="space-y-4">
          {canEdit && (
            <Card className="p-5">
              <Field label="Reason for the change" hint="Required for every save on this page.">
                <Input value={reason} onChange={(e) => setReason(e.target.value)} placeholder="e.g. Rotated the TRON hot wallet (ticket FIN-112)" aria-label="Reason for the change" />
              </Field>
            </Card>
          )}
          {!canEdit && <div className="rounded-[14px] border border-line bg-surface-2 px-4 py-3 text-[12.5px] text-fg-3">Read-only: only admins change wallet settings.</div>}
          <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
            {data.chains.map((c) => (
              <ChainForm key={c.chain} c={c} canEdit={canEdit} onSave={(chain) => save({ chains: [chain] })} />
            ))}
          </div>
          <Card>
            <CardHeader title="Limits and fees" subtitle={data.limits_updated_at ? `Last changed ${when(data.limits_updated_at)}${data.limits_updated_by ? ` by ${data.limits_updated_by}` : ""}` : undefined} />
            <div className="grid grid-cols-1 gap-3 px-6 pb-6 pt-4 sm:grid-cols-2 xl:grid-cols-4">
              {LIMIT_FIELDS.map(([k, label, unit]) => (
                <Field key={k} label={`${label} (${unit})`}>
                  <Input disabled={!canEdit} value={String(limits[k])} onChange={(e) => setLimits((l) => (l ? { ...l, [k]: typeof l[k] === "number" ? Number(e.target.value) : e.target.value } : l))} aria-label={label} />
                </Field>
              ))}
            </div>
            {canEdit && (
              <div className="px-6 pb-6">
                <Button variant="ember" disabled={JSON.stringify(limits) === JSON.stringify(data.limits)} onClick={() => save({ limits })}>
                  <Save /> Save limits
                </Button>
              </div>
            )}
          </Card>
          <NowPaymentsSettingsCard />
          <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
            <Card>
              <CardHeader title="Address book" subtitle="Every company address the wallet recognises" />
              <div className="space-y-1.5 px-6 pb-5 pt-3">
                {data.addresses.map((a) => (
                  <div key={`${a.chain}${a.role}${a.address}`} className="k-row flex items-center gap-3 px-3 py-2 text-[12.5px]">
                    <span className="w-14 text-fg-3">{CHAIN_SHORT[a.chain]}</span>
                    <span className="w-20 text-fg-2">{a.role}</span>
                    <Addr a={a.address} />
                    <span className="ml-auto">{a.active ? <Chip size="sm" tone="up">Current</Chip> : <span className="text-fg-3">since {when(a.created_at)}</span>}</span>
                  </div>
                ))}
              </div>
            </Card>
            <Card>
              <CardHeader title="Change history" icon={<History />} />
              <div className="space-y-2 px-6 pb-5 pt-3">
                {data.history.length === 0 && <div className="text-[12.5px] text-fg-3">No changes yet.</div>}
                {data.history.map((h) => (
                  <div key={h.id} className="k-row px-3 py-2 text-[12.5px]">
                    <div className="flex justify-between gap-2">
                      <span className="font-medium">{h.action.replace("wallet.settings.", "")} · {h.target_id}</span>
                      <span className="text-fg-3">{when(h.at)}</span>
                    </div>
                    <div className="text-fg-3">
                      {h.actor_name ?? h.actor_kind}
                      {h.reason ? ` · ${h.reason}` : ""}
                    </div>
                  </div>
                ))}
              </div>
            </Card>
          </div>
        </div>
      )}
    </div>
  );
}
