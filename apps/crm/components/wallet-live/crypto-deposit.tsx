"use client";

// Crypto deposits through NOWPayments (services/wallet README "NOWPayments"), next to the direct USDT deposit:
//   step 1  amount in USD + coin                 -> POST wallet/nowpayments/payments {amount, pay_currency}
//   step 2  send exactly <pay_amount> <coin> to the address (+ memo / tag when the coin needs one), QR, countdown
//   step 3  live status (GET wallet/nowpayments/payments/{id} every 5 s): waiting -> confirming -> credited
// The option is shown only when the broker switched it on in the Back Office (GET wallet/nowpayments/config).

import * as React from "react";
import Link from "next/link";
import { QRCodeSVG } from "qrcode.react";
import { ArrowLeftRight, Check, CircleAlert, Clock, Loader2, ShieldAlert, Wallet } from "lucide-react";
import { Button, Card, CardHeader, CoinIcon, CopyButton, Field, Illustration, Input, cn } from "@/components/kit";
import { Trans, useT } from "@kalks/i18n/react";
import type { MessageKey } from "@kalks/i18n";
import { fmt, useWallet, walletApi } from "./api";
import { InlineError, StatusTag, cleanAmount } from "./ui";
import type { ChipTone } from "@/components/kit";

export interface NpCoin {
  code: string;
  ticker: string;
  name: string;
  network: string;
  icon: string;
  chain_icon: string | null;
  needs_extra_id: boolean;
  min_usd: string;
}

export interface NpConfig {
  enabled: boolean;
  sandbox?: boolean;
  min_deposit_usd?: string;
  max_deposit_usd?: string;
  fee_paid_by_user?: boolean;
  coins: NpCoin[];
}

export type NpStatus = "creating" | "waiting" | "confirming" | "credited" | "review" | "expired" | "failed" | "refunded" | "rejected";

export interface NpPayment {
  id: number;
  order_id: string;
  status: NpStatus;
  np_status: string | null;
  price_amount: string;
  pay_currency: string;
  coin: Omit<NpCoin, "min_usd">;
  pay_amount: string | null;
  pay_address: string | null;
  payin_extra_id: string | null;
  network: string | null;
  actually_paid: string | null;
  payin_hash: string | null;
  expires_at: string | null;
  credit_amount: string | null;
  credited_at: string | null;
  failure_reason: string | null;
  sandbox: boolean;
  created_at: string;
}

export const NP_STATUS: Record<NpStatus, { tone: ChipTone; label: MessageKey }> = {
  creating: { tone: "warn", label: "wallet.nowpayments.status.waiting" },
  waiting: { tone: "warn", label: "wallet.nowpayments.status.waiting" },
  confirming: { tone: "info", label: "wallet.nowpayments.status.confirming" },
  credited: { tone: "up", label: "wallet.nowpayments.status.credited" },
  review: { tone: "warn", label: "wallet.nowpayments.status.review" },
  expired: { tone: "neutral", label: "wallet.nowpayments.status.expired" },
  failed: { tone: "down", label: "wallet.nowpayments.status.failed" },
  refunded: { tone: "neutral", label: "wallet.nowpayments.status.refunded" },
  rejected: { tone: "down", label: "wallet.nowpayments.status.rejected" },
};

const KNOWN_ICONS = new Set(["ada", "avax", "bnb", "btc", "doge", "dot", "eth", "link", "ltc", "matic", "sol", "trx", "usdt", "xrp"]);

/** The coin's logo with the network badge for tokens (USDT on TRON); a letter disc for coins without a logo. */
export function NpCoinIcon({ coin, size = 32 }: { coin: Pick<NpCoin, "icon" | "chain_icon" | "ticker">; size?: number }) {
  const main = KNOWN_ICONS.has(coin.icon) ? (
    <CoinIcon coin={coin.icon} size={size} />
  ) : (
    <span className="grid shrink-0 place-items-center rounded-full bg-surface-3 font-semibold text-fg-2" style={{ width: size, height: size, fontSize: size * 0.38 }}>
      {coin.ticker.slice(0, 1)}
    </span>
  );
  return (
    <span className="relative inline-flex shrink-0">
      {main}
      {coin.chain_icon && KNOWN_ICONS.has(coin.chain_icon) && <CoinIcon coin={coin.chain_icon} size={Math.round(size * 0.47)} className="absolute -bottom-0.5 -end-1 ring-2 ring-surface-2" />}
    </span>
  );
}

export const coinLabel = (c: Pick<NpCoin, "ticker" | "network">) => (c.network ? `${c.ticker} · ${c.network}` : c.ticker);

/* ------------------------------------------------------------------ step 1 */

function CryptoStartForm({ cfg, onCreated }: { cfg: NpConfig; onCreated: (p: NpPayment) => void }) {
  const t = useT();
  const [code, setCode] = React.useState(cfg.coins[0]?.code ?? "");
  const [amount, setAmount] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [err, setErr] = React.useState<string | null>(null);
  const coin = cfg.coins.find((c) => c.code === code);
  const min = Number(coin?.min_usd ?? cfg.min_deposit_usd ?? 0);
  const max = Number(cfg.max_deposit_usd ?? Infinity);
  const n = Number(amount);
  const valid = !!coin && /^\d{1,12}(\.\d{1,2})?$/.test(amount.trim()) && n > 0 && n >= min && n <= max;

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!valid || busy) return;
    setBusy(true);
    setErr(null);
    try {
      const r = await walletApi<{ payment: NpPayment }>("nowpayments/payments", { body: { amount: amount.trim(), pay_currency: code } });
      onCreated(r.payment);
    } catch (e2) {
      setErr(e2 instanceof Error ? e2.message : t("wallet.nowpayments.startFailed"));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card>
      <CardHeader title={t("wallet.nowpayments.title")} subtitle={t("wallet.nowpayments.subtitle")} />
      <form onSubmit={submit} className="space-y-5 px-4 pb-6 pt-4 sm:px-6" data-testid="np-start">
        <Field label={t("wallet.nowpayments.amountLabel")} hint={t("wallet.nowpayments.amountHint", { min: fmt(coin?.min_usd ?? cfg.min_deposit_usd), max: fmt(cfg.max_deposit_usd) })}>
          <Input
            inputMode="decimal"
            placeholder="100.00"
            value={amount}
            onChange={(e) => setAmount(cleanAmount(e.target.value))}
            trailing={<span className="text-[12.5px] font-medium text-fg-2">USD</span>}
            aria-label={t("wallet.nowpayments.amountLabel")}
          />
        </Field>
        <div>
          <div className="k-label mb-2">{t("wallet.nowpayments.coin")}</div>
          <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
            {cfg.coins.map((c) => {
              const on = c.code === code;
              return (
                <button
                  key={c.code}
                  type="button"
                  onClick={() => setCode(c.code)}
                  aria-pressed={on}
                  className={cn("k-row flex items-center gap-3 px-4 py-3 text-start transition-colors", on ? "border-ember/60 bg-ember-soft" : "hover:border-[var(--k-border-top)]")}
                >
                  <NpCoinIcon coin={c} />
                  <div className="min-w-0 flex-1">
                    <div className="text-[14px] font-medium">{c.ticker}</div>
                    <div className="truncate text-[12px] text-fg-3">
                      {c.network || c.name} · {t("wallet.nowpayments.coinMin", { min: fmt(c.min_usd) })}
                    </div>
                  </div>
                  {on && (
                    <span className="grid size-6 place-items-center rounded-full bg-ember text-white">
                      <Check className="size-3.5" />
                    </span>
                  )}
                </button>
              );
            })}
          </div>
        </div>
        {cfg.fee_paid_by_user && <p className="text-[12.5px] text-fg-3">{t("wallet.nowpayments.feeNote")}</p>}
        {cfg.sandbox && <div className="rounded-[12px] border border-warn/30 bg-warn-soft px-3 py-2 text-[12.5px]">{t("wallet.nowpayments.sandbox")}</div>}
        <InlineError>{err}</InlineError>
        <Button type="submit" variant="ember" size="lg" className="w-full sm:w-auto" disabled={!valid || busy}>
          {busy ? <Loader2 className="animate-spin" /> : <Wallet />} {t("wallet.nowpayments.getDetails")}
        </Button>
        <p className="text-[11.5px] text-fg-3">{t("wallet.nowpayments.poweredBy")}</p>
      </form>
    </Card>
  );
}

/* ------------------------------------------------------------------ step 2 + 3 */

function Countdown({ until }: { until: string }) {
  const tr = useT();
  const [now, setNow] = React.useState(() => Date.now());
  React.useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);
  const left = Math.max(0, Math.floor((Date.parse(until) - now) / 1000));
  if (left === 0) return <span className="text-warn">{tr("wallet.deposit.expired")}</span>;
  const h = Math.floor(left / 3600);
  const m = Math.floor((left % 3600) / 60);
  return (
    <span className="k-num" dir="ltr">
      {h > 0 ? `${h}:${String(m).padStart(2, "0")}` : m}:{String(left % 60).padStart(2, "0")}
    </span>
  );
}

function Steps({ p }: { p: NpPayment }) {
  const t = useT();
  const found = p.status !== "waiting" && p.status !== "creating";
  const steps: [string, boolean][] = [
    [t("wallet.nowpayments.stepWaiting"), true],
    [t("wallet.nowpayments.stepConfirming"), found],
    [t("wallet.nowpayments.stepCredited"), p.status === "credited"],
  ];
  return (
    <ol className="grid grid-cols-1 gap-2 sm:grid-cols-3">
      {steps.map(([label, done], i) => (
        <li key={i} className={cn("k-row flex items-center gap-2 px-3 py-2.5 text-[12.5px]", done ? "text-fg" : "text-fg-3")}>
          <span className={cn("grid size-5 place-items-center rounded-full border", done ? "border-up/40 bg-up-soft text-up" : "border-line")}>{done ? <Check className="size-3" /> : <span className="text-[10px]">{i + 1}</span>}</span>
          {label}
        </li>
      ))}
    </ol>
  );
}

function CryptoPayPanel({ p, onNew }: { p: NpPayment; onNew: () => void }) {
  const t = useT();
  const coin = p.coin;
  const ticker = coin.ticker;
  const st = NP_STATUS[p.status] ?? NP_STATUS.waiting;
  const open = p.status === "waiting" || p.status === "creating" || p.status === "confirming";
  const done = p.status === "credited";
  const ended = ["expired", "failed", "refunded", "rejected"].includes(p.status);
  return (
    <Card>
      <CardHeader
        title={done ? t("wallet.deposit.credited") : open && p.pay_amount ? t("wallet.nowpayments.sendTitle", { amount: p.pay_amount, coin: ticker }) : t("wallet.nowpayments.title")}
        subtitle={t("wallet.nowpayments.sendSubtitle", { network: coin.network || coin.name, usd: fmt(p.price_amount) })}
        action={<StatusTag {...st} />}
      />
      <div className="space-y-5 px-4 pb-6 pt-4 sm:px-6" data-testid="np-payment">
        {p.sandbox && <div className="rounded-[12px] border border-warn/30 bg-warn-soft px-3 py-2 text-[12.5px]">{t("wallet.nowpayments.sandbox")}</div>}
        {p.status === "waiting" && p.pay_address && (
          <>
            {p.expires_at && (
              <div className="inline-flex items-center gap-1.5 text-[12.5px] text-fg-3">
                <Clock className="size-3.5" /> <Trans k="wallet.deposit.expiresIn" tags={{ time: () => <Countdown until={p.expires_at!} /> }} />
              </div>
            )}
            <div className="grid grid-cols-1 gap-5 md:grid-cols-[auto_1fr]">
              <div className="flex flex-col items-center">
                <div className="rounded-[16px] bg-white p-3">
                  <QRCodeSVG value={p.pay_address} size={168} level="M" bgColor="#ffffff" fgColor="#0e0e12" />
                </div>
                <div className="mt-2 flex items-center gap-1.5 text-[11.5px] text-fg-3">
                  <NpCoinIcon coin={coin} size={16} /> {coinLabel(coin)}
                </div>
              </div>
              <div className="min-w-0 space-y-3">
                <div>
                  <div className="mb-1 text-[11.5px] uppercase tracking-wider text-fg-3">{t("wallet.nowpayments.exactAmount")}</div>
                  <div className="k-row flex items-center gap-2 px-3 py-2.5">
                    <span dir="ltr" className="k-num min-w-0 flex-1 text-[18px] font-semibold" data-testid="np-pay-amount">
                      {p.pay_amount} <span className="text-[13px] text-fg-3">{ticker}</span>
                    </span>
                    {p.pay_amount && <CopyButton value={p.pay_amount} label={t("wallet.nowpayments.exactAmount")} className="size-8" />}
                  </div>
                </div>
                <div>
                  <div className="mb-1 text-[11.5px] uppercase tracking-wider text-fg-3">{t("wallet.nowpayments.address")}</div>
                  <div className="k-row flex items-center gap-2 px-3 py-2.5">
                    <span dir="ltr" className="min-w-0 flex-1 break-all font-mono text-[12.5px]" data-testid="np-address">
                      {p.pay_address}
                    </span>
                    <CopyButton value={p.pay_address} label={t("wallet.nowpayments.address")} className="size-8" />
                  </div>
                </div>
                {p.payin_extra_id && (
                  <div className="rounded-[14px] border border-down/35 bg-down-soft p-3">
                    <div className="mb-1 flex items-center gap-1.5 text-[12px] font-medium text-fg">
                      <ShieldAlert className="size-3.5 text-down" /> {t("wallet.nowpayments.memo")}
                    </div>
                    <div className="flex items-center gap-2">
                      <span dir="ltr" className="min-w-0 flex-1 break-all font-mono text-[15px] font-semibold" data-testid="np-memo">
                        {p.payin_extra_id}
                      </span>
                      <CopyButton value={p.payin_extra_id} label={t("wallet.nowpayments.memo")} className="size-8" />
                    </div>
                    <p className="mt-1.5 text-[12px] text-fg-2">{t("wallet.nowpayments.memoWarning")}</p>
                  </div>
                )}
                <div className="flex items-start gap-2 rounded-[12px] border border-warn/30 bg-warn-soft px-3 py-2 text-[12px] text-fg-2">
                  <CircleAlert className="mt-0.5 size-3.5 shrink-0 text-warn" />
                  <span>{t("wallet.nowpayments.warning", { coin: ticker, network: coin.network || coin.name })}</span>
                </div>
              </div>
            </div>
          </>
        )}
        {(p.status === "confirming" || done || p.status === "review") && (
          <div className="flex flex-wrap items-end justify-between gap-3">
            <div>
              <div dir="ltr" className="k-num text-[30px] font-semibold leading-none" data-testid="np-amount">
                +{fmt(p.credit_amount ?? p.price_amount)} <span className="text-[15px] text-fg-3">USDT</span>
              </div>
              {p.actually_paid && Number(p.actually_paid) > 0 && (
                <div className="mt-2 text-[12.5px] text-fg-3">{t("wallet.nowpayments.received", { paid: p.actually_paid, coin: ticker })}</div>
              )}
            </div>
            {done && <Illustration name="depositCredited" width={140} maxHeight={140} className="shrink-0" />}
          </div>
        )}
        {!ended && <Steps p={p} />}
        {done && <div className="text-[13px] text-fg-2">{t("wallet.nowpayments.creditedText", { amount: fmt(p.credit_amount ?? p.price_amount) })}</div>}
        {p.status === "review" && <div className="rounded-[12px] border border-warn/30 bg-warn-soft px-3 py-2.5 text-[12.5px] text-fg-2">{t("wallet.nowpayments.reviewText")}</div>}
        {p.status === "expired" && <InlineError>{t("wallet.nowpayments.expiredText")}</InlineError>}
        {(p.status === "failed" || p.status === "refunded" || p.status === "rejected") && <InlineError>{t("wallet.nowpayments.failedText")}</InlineError>}
        <div className="flex flex-wrap gap-2">
          {done && (
            <Link href="/wallet/transfer">
              <Button variant="ember">
                <ArrowLeftRight /> {t("wallet.fundTradingAccount")}
              </Button>
            </Link>
          )}
          {!open && (
            <Button variant="surface" onClick={onNew}>
              {t("wallet.newDeposit")}
            </Button>
          )}
          <Link href="/wallet">
            <Button variant="ghost">{t("wallet.backToWallet")}</Button>
          </Link>
        </div>
      </div>
    </Card>
  );
}

/** The crypto method: the start form, or the payment `paymentId` (polled every 5 s while it can still change). */
export function CryptoDeposit({ cfg, paymentId, onPayment }: { cfg: NpConfig; paymentId: number | null; onPayment: (id: number | null) => void }) {
  const t = useT();
  const view = useWallet<{ payment: NpPayment }>(paymentId ? `nowpayments/payments/${paymentId}` : null, 5000);
  if (!paymentId) return <CryptoStartForm cfg={cfg} onCreated={(p) => onPayment(p.id)} />;
  if (view.error && !view.data) {
    return (
      <Card>
        <div className="space-y-3 p-6 text-[13.5px] text-fg-2">
          <InlineError>{view.error.status === 404 ? t("wallet.nowpayments.notFound") : view.error.message}</InlineError>
          <Button variant="surface" onClick={() => onPayment(null)}>
            {t("wallet.newDeposit")}
          </Button>
        </div>
      </Card>
    );
  }
  if (!view.data) return <div className="h-[420px] w-full animate-pulse rounded-[20px] bg-surface-2" />;
  return <CryptoPayPanel p={view.data.payment} onNew={() => onPayment(null)} />;
}

export function CryptoHowItWorks() {
  const t = useT();
  return (
    <Card>
      <CardHeader title={t("wallet.howDepositsWork")} />
      <ol className="space-y-4 px-6 pb-6 pt-4 text-[13px]">
        {(
          [
            ["wallet.nowpayments.how1Title", "wallet.nowpayments.how1Text"],
            ["wallet.nowpayments.how2Title", "wallet.nowpayments.how2Text"],
            ["wallet.nowpayments.how3Title", "wallet.nowpayments.how3Text"],
          ] as const
        ).map(([title, d], i) => (
          <li key={i} className="flex gap-3">
            <span className="grid size-6 shrink-0 place-items-center rounded-full border border-line text-[11px] text-fg-2">{i + 1}</span>
            <div>
              <div className="font-medium">{t(title)}</div>
              <div className="text-[12.5px] text-fg-3">{t(d)}</div>
            </div>
          </li>
        ))}
      </ol>
    </Card>
  );
}

/** Method switch at the top of the deposit page: USDT direct or crypto through NOWPayments. */
export function MethodPicker({ value, onChange }: { value: "usdt" | "crypto"; onChange: (m: "usdt" | "crypto") => void }) {
  const t = useT();
  const opts = [
    { id: "usdt" as const, title: t("wallet.nowpayments.methodDirect"), text: t("wallet.nowpayments.methodDirectText"), icon: <CoinIcon coin="usdt" size={30} /> },
    { id: "crypto" as const, title: t("wallet.nowpayments.methodCrypto"), text: t("wallet.nowpayments.methodCryptoText"), icon: <CoinIcon coin="btc" size={30} /> },
  ];
  return (
    <div className="mb-4 grid grid-cols-1 gap-2 sm:grid-cols-2" role="radiogroup" aria-label={t("wallet.nowpayments.coin")}>
      {opts.map((o) => {
        const on = o.id === value;
        return (
          <button
            key={o.id}
            type="button"
            role="radio"
            aria-checked={on}
            data-testid={`method-${o.id}`}
            onClick={() => onChange(o.id)}
            className={cn("k-row flex items-center gap-3 px-4 py-3 text-start transition-colors", on ? "border-ember/60 bg-ember-soft" : "hover:border-[var(--k-border-top)]")}
          >
            {o.icon}
            <div className="min-w-0 flex-1">
              <div className="text-[14px] font-medium">{o.title}</div>
              <div className="text-[12px] leading-snug text-fg-3">{o.text}</div>
            </div>
            {on && (
              <span className="grid size-6 place-items-center rounded-full bg-ember text-white">
                <Check className="size-3.5" />
              </span>
            )}
          </button>
        );
      })}
    </div>
  );
}

