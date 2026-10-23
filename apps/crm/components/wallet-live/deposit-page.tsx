"use client";

import * as React from "react";
import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { QRCodeSVG } from "qrcode.react";
import { ArrowLeft, ArrowLeftRight, Check, CircleAlert, Clock, Loader2, Smartphone, Wallet } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, CardHeader, CoinIcon, CopyButton, Field, Illustration, Input, PageHeader, Skeleton, cn } from "@/components/kit";
import { Trans, useT } from "@kalks/i18n/react";
import { CHAIN_LABEL, WalletError, fmt, useWallet, walletApi, type Chain, type Deposit, type Intent, type WalletConfig } from "./api";
import { PayError, hasMetaMask, hasTronLink, isMobile, metamaskDeepLink, payWithMetaMask, payWithTronLink } from "./pay";
import { Confirmations, DEPOSIT_STATUS, HashLink, InlineError, StatusTag, Tile, WalletUnavailable, cleanAmount } from "./ui";
import { CryptoDeposit, CryptoHowItWorks, MethodPicker, type NpConfig } from "./crypto-deposit";

type IntentView = { intent: Intent; deposit: Deposit | null };

function NetworkPicker({ cfg, value, onChange }: { cfg: WalletConfig; value: Chain; onChange: (c: Chain) => void }) {
  const t = useT();
  return (
    <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
      {cfg.chains.map((c) => {
        const on = c.chain === value;
        const off = !c.deposits_enabled;
        return (
          <button
            key={c.chain}
            type="button"
            disabled={off}
            onClick={() => onChange(c.chain)}
            className={cn("k-row flex items-center gap-3 px-4 py-3 text-start transition-colors disabled:opacity-50", on ? "border-ember/60 bg-ember-soft" : "hover:border-[var(--k-border-top)]")}
          >
            <span className="relative">
              <CoinIcon coin="usdt" size={32} />
              <CoinIcon coin={c.chain === "bsc" ? "bnb" : "trx"} size={15} className="absolute -bottom-0.5 -end-1 ring-2 ring-surface-2" />
            </span>
            <div className="min-w-0 flex-1">
              <div className="text-[14px] font-medium">
                USDT <span className="rounded-md bg-surface-3 px-1.5 py-px text-[10.5px] font-medium text-fg-2">{CHAIN_LABEL[c.chain].short}</span>
              </div>
              <div className="line-clamp-2 text-[12px] leading-snug text-fg-3">
                {t("wallet.deposit.payWithShort", { network: CHAIN_LABEL[c.chain].name, wallet: CHAIN_LABEL[c.chain].wallet })}
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
  );
}

function Countdown({ until }: { until: string }) {
  const tr = useT();
  const [now, setNow] = React.useState(() => Date.now());
  React.useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, []);
  const left = Math.max(0, Math.floor((Date.parse(until) - now) / 1000));
  if (left === 0) return <span className="text-warn">{tr("wallet.deposit.expired")}</span>;
  return (
    <span className="k-num" dir="ltr">
      {Math.floor(left / 60)}:{String(left % 60).padStart(2, "0")}
    </span>
  );
}

/** Step 1: network + amount → deposit request. */
function StartForm({ cfg, onCreated }: { cfg: WalletConfig; onCreated: (i: Intent) => void }) {
  const t = useT();
  const enabled = cfg.chains.filter((c) => c.deposits_enabled);
  const [chain, setChain] = React.useState<Chain>(enabled.find((c) => c.chain === "bsc")?.chain ?? enabled[0]?.chain ?? "tron");
  const [amount, setAmount] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [err, setErr] = React.useState<string | null>(null);
  const c = cfg.chains.find((x) => x.chain === chain);
  const min = Number(c?.min_deposit ?? 0);
  const valid = /^\d{1,12}(\.\d{1,6})?$/.test(amount.trim()) && Number(amount) >= min && Number(amount) > 0;

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!valid || busy) return;
    setBusy(true);
    setErr(null);
    try {
      const r = await walletApi<{ intent: Intent }>("deposits/intents", { body: { chain, amount: amount.trim() } });
      onCreated(r.intent);
    } catch (e) {
      setErr(e instanceof Error ? e.message : t("wallet.deposit.startFailed"));
    } finally {
      setBusy(false);
    }
  };

  if (!enabled.length) {
    return (
      <Card>
        <div className="p-6 text-[13.5px] text-fg-2">{t("wallet.deposit.paused")}</div>
      </Card>
    );
  }
  return (
    <Card>
      <CardHeader title={t("wallet.newDeposit")} subtitle={t("wallet.deposit.newSubtitle")} />
      <form onSubmit={submit} className="space-y-5 px-4 pb-6 pt-4 sm:px-6">
        <NetworkPicker cfg={cfg} value={chain} onChange={setChain} />
        <Field label={t("common.amount")} hint={t("wallet.deposit.amountHint", { min: fmt(c?.min_deposit) })}>
          <Input
            inputMode="decimal"
            placeholder="100.00"
            value={amount}
            onChange={(e) => setAmount(cleanAmount(e.target.value))}
            trailing={<span className="text-[12.5px] font-medium text-fg-2">USDT</span>}
            aria-label={t("wallet.deposit.amountAria")}
          />
        </Field>
        <InlineError>{err}</InlineError>
        <Button type="submit" variant="ember" size="lg" className="w-full sm:w-auto" disabled={!valid || busy}>
          {busy ? <Loader2 className="animate-spin" /> : <Wallet />} {t("common.continue")}
        </Button>
      </form>
    </Card>
  );
}

/** Step 2: pay with the wallet app, or send manually and paste the hash. */
function PayPanel({ view, onSubmitted }: { view: IntentView; onSubmitted: () => void }) {
  const t = useT();
  const it = view.intent;
  const [busy, setBusy] = React.useState(false);
  const [err, setErr] = React.useState<string | null>(null);
  const [hash, setHash] = React.useState("");
  const [provider, setProvider] = React.useState<{ mm: boolean; tl: boolean; mobile: boolean }>({ mm: false, tl: false, mobile: false });
  React.useEffect(() => {
    // wallets inject their providers after load
    const check = () => setProvider({ mm: hasMetaMask(), tl: hasTronLink(), mobile: isMobile() });
    check();
    const t = setTimeout(check, 800);
    return () => clearTimeout(t);
  }, []);
  const expired = Date.parse(it.expires_at) < Date.now();

  const submitHash = async (h: string) => {
    await walletApi("deposits/submit", { body: { intent_id: it.id, tx_hash: h } });
    onSubmitted();
  };

  const pay = async () => {
    setBusy(true);
    setErr(null);
    try {
      const pay = it.chain === "bsc" ? payWithMetaMask : payWithTronLink;
      const r = await pay({ token: it.token_contract, to: it.address, amount: it.amount, decimals: it.decimals });
      try {
        await submitHash(r.hash);
        toast.success(t("wallet.deposit.toastSent"), { description: t("wallet.deposit.toastSentText") });
      } catch (e) {
        // the transfer went out: keep the hash visible so it can be submitted again
        setHash(r.hash);
        setErr(t("wallet.deposit.registerFailed", { hash: r.hash.slice(0, 10), error: e instanceof Error ? e.message : t("wallet.deposit.tryAgain") }));
      }
    } catch (e) {
      setErr(e instanceof PayError || e instanceof WalletError ? e.message : t("wallet.deposit.payFailed"));
    } finally {
      setBusy(false);
    }
  };

  const manual = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setErr(null);
    try {
      await submitHash(hash.trim());
      toast.success(t("wallet.deposit.toastSubmitted"), { description: t("wallet.deposit.toastSubmittedText") });
    } catch (e2) {
      setErr(e2 instanceof Error ? e2.message : t("wallet.deposit.submitFailed"));
    } finally {
      setBusy(false);
    }
  };

  const wallet = CHAIN_LABEL[it.chain].wallet;
  const injected = it.chain === "bsc" ? provider.mm : provider.tl;

  return (
    <Card>
      <CardHeader
        title={t("wallet.deposit.sendTitle", { amount: fmt(it.amount) })}
        subtitle={t("wallet.deposit.sendSubtitle", { network: CHAIN_LABEL[it.chain].name, short: CHAIN_LABEL[it.chain].short, id: it.id.slice(4, 12) })}
        action={
          <span className="inline-flex items-center gap-1.5 text-[12.5px] text-fg-3">
            <Clock className="size-3.5" /> {expired ? t("wallet.deposit.requestExpired") : <Trans k="wallet.deposit.expiresIn" tags={{ time: () => <Countdown until={it.expires_at} /> }} />}
          </span>
        }
      />
      <div className="space-y-5 px-4 pb-6 pt-4 sm:px-6">
        <div className="rounded-[16px] border border-line bg-surface-2 p-4">
          <div className="text-[13.5px] font-medium">{t("wallet.deposit.payWith", { wallet })}</div>
          <p className="mt-1 text-[12.5px] text-fg-3">
            {it.chain === "bsc"
              ? t("wallet.deposit.metamaskText")
              : t("wallet.deposit.tronlinkText")}
          </p>
          <div className="mt-3 flex flex-wrap gap-2">
            {injected || !provider.mobile || it.chain === "tron" ? (
              <Button variant="ember" onClick={pay} disabled={busy || expired || !injected} data-testid="pay-wallet">
                {busy ? <Loader2 className="animate-spin" /> : <Wallet />} {t("wallet.deposit.payWith", { wallet })}
              </Button>
            ) : (
              <a href={metamaskDeepLink(it.id)}>
                <Button variant="ember">
                  <Smartphone /> {t("wallet.deposit.openMetaMask")}
                </Button>
              </a>
            )}
          </div>
          {!injected && (
            <p className="mt-2 text-[12px] text-fg-3">
              {it.chain === "bsc"
                ? provider.mobile
                  ? t("wallet.deposit.metamaskApp")
                  : t("wallet.deposit.noMetaMask")
                : t("wallet.deposit.noTronLink")}
            </p>
          )}
        </div>

        <div>
          <div className="k-label mb-3">{t("wallet.deposit.orManual")}</div>
          <div className="grid grid-cols-1 gap-5 md:grid-cols-[auto_1fr]">
            <div className="flex flex-col items-center">
              <div className="rounded-[16px] bg-white p-3">
                <QRCodeSVG value={it.address} size={148} level="M" bgColor="#ffffff" fgColor="#0e0e12" />
              </div>
              <div className="mt-2 text-[11.5px] text-fg-3">{t("wallet.deposit.companyAddress", { network: CHAIN_LABEL[it.chain].short })}</div>
            </div>
            <div className="min-w-0 space-y-3">
              <div>
                <div className="mb-1 text-[11.5px] uppercase tracking-wider text-fg-3">{t("wallet.deposit.sendTo")}</div>
                <div className="k-row flex items-center gap-2 px-3 py-2.5">
                  <span dir="ltr" className="min-w-0 flex-1 break-all font-mono text-[12.5px]" data-testid="deposit-address">
                    {it.address}
                  </span>
                  <CopyButton value={it.address} label={t("wallet.address")} className="size-8" />
                </div>
              </div>
              <div className="grid grid-cols-2 gap-2">
                <Tile label={t("common.amount")} value={<span dir="ltr" className="inline-flex items-center gap-1">{fmt(it.amount)} USDT <CopyButton value={it.amount} label={t("common.amount")} className="size-6" /></span>} />
                <Tile label={t("wallet.network")} value={`${CHAIN_LABEL[it.chain].name}`} />
              </div>
              <div className="flex items-start gap-2 rounded-[12px] border border-warn/30 bg-warn-soft px-3 py-2 text-[12px] text-fg-2">
                <CircleAlert className="mt-0.5 size-3.5 shrink-0 text-warn" />
                <span>
                  {t("wallet.deposit.warning", { network: CHAIN_LABEL[it.chain].name, short: CHAIN_LABEL[it.chain].short, amount: fmt(it.amount) })}
                </span>
              </div>
              <form onSubmit={manual} className="space-y-2">
                <Field label={t("wallet.transactionHash")} hint={t("wallet.deposit.hashHint")}>
                  <Input value={hash} onChange={(e) => setHash(e.target.value)} placeholder={it.chain === "bsc" ? "0x…" : t("wallet.deposit.hashPlaceholder")} inputClassName="font-mono text-[12.5px]" aria-label={t("wallet.transactionHash")} dir="ltr" />
                </Field>
                <Button type="submit" variant="surface" disabled={busy || !/^(0x)?[0-9a-fA-F]{64}$/.test(hash.trim())}>
                  {t("wallet.deposit.iSentIt")}
                </Button>
              </form>
            </div>
          </div>
        </div>
        <InlineError>{err}</InlineError>
      </div>
    </Card>
  );
}

/** Step 3: waiting → confirmations → credited. */
function Tracker({ view, onNew }: { view: IntentView; onNew: () => void }) {
  const t = useT();
  const d = view.deposit!;
  const st = DEPOSIT_STATUS[d.status];
  const credited = d.status === "credited";
  const bad = d.status === "failed" || d.status === "rejected";
  return (
    <Card>
      <CardHeader title={credited ? t("wallet.deposit.credited") : bad ? t("wallet.deposit.notCredited") : t("wallet.deposit.onItsWay")} subtitle={`${CHAIN_LABEL[d.chain].name} · USDT ${CHAIN_LABEL[d.chain].short}`} action={<StatusTag {...st} />} />
      <div className="space-y-5 px-4 pb-6 pt-4 sm:px-6">
        <div className="flex flex-wrap items-end justify-between gap-3">
          <div>
            <div dir="ltr" className="k-num text-[30px] font-semibold leading-none" data-testid="deposit-amount">
              +{fmt(d.amount ?? d.expected_amount)} <span className="text-[15px] text-fg-3">USDT</span>
            </div>
            <div className="mt-2 text-[12.5px] text-fg-3">
              {t("wallet.deposit.transaction")} <HashLink hash={d.tx_hash} url={d.explorer_url} />
            </div>
          </div>
          {credited && <Illustration name="depositCredited" width={140} maxHeight={140} className="shrink-0" />}
        </div>
        {!bad && d.status !== "review" && <Confirmations d={d} />}
        <ol className="grid grid-cols-1 gap-2 sm:grid-cols-3">
          {[
            [t("wallet.deposit.stepSent"), true],
            [t("wallet.deposit.stepFound"), d.status !== "pending"],
            [t("wallet.deposit.stepCredited"), credited],
          ].map(([label, done], i) => (
            <li key={i} className={cn("k-row flex items-center gap-2 px-3 py-2.5 text-[12.5px]", done ? "text-fg" : "text-fg-3")}>
              <span className={cn("grid size-5 place-items-center rounded-full border", done ? "border-up/40 bg-up-soft text-up" : "border-line")}>{done ? <Check className="size-3" /> : <span className="text-[10px]">{i + 1}</span>}</span>
              {label}
            </li>
          ))}
        </ol>
        {d.status === "review" && (
          <div className="rounded-[12px] border border-warn/30 bg-warn-soft px-3 py-2.5 text-[12.5px] text-fg-2">
            {d.review_reason ? t("wallet.deposit.reviewReason", { reason: d.review_reason }) : t("wallet.deposit.review")}
          </div>
        )}
        {bad && <InlineError>{d.failure_reason ?? t("wallet.deposit.couldNotCredit")}</InlineError>}
        <div className="flex flex-wrap gap-2">
          {credited && (
            <Link href="/wallet/transfer">
              <Button variant="ember">
                <ArrowLeftRight /> {t("wallet.fundTradingAccount")}
              </Button>
            </Link>
          )}
          <Button variant="surface" onClick={onNew}>
            {t("wallet.newDeposit")}
          </Button>
          <Link href="/wallet">
            <Button variant="ghost">{t("wallet.backToWallet")}</Button>
          </Link>
        </div>
      </div>
    </Card>
  );
}

function HowItWorks({ cfg }: { cfg: WalletConfig }) {
  const t = useT();
  const conf = (c: Chain) => cfg.chains.find((x) => x.chain === c)?.confirmations;
  return (
    <Card>
      <CardHeader title={t("wallet.howDepositsWork")} />
      <ol className="space-y-4 px-6 pb-6 pt-4 text-[13px]">
        {[
          [t("wallet.deposit.how1Title"), t("wallet.deposit.how1Text")],
          [t("wallet.deposit.how2Title"), t("wallet.deposit.how2Text")],
          [t("wallet.deposit.how3Title"), t("wallet.deposit.how3Text", { bsc: conf("bsc") ?? 15, tron: conf("tron") ?? 20 })],
          [t("wallet.deposit.how4Title"), t("wallet.deposit.how4Text")],
        ].map(([title, d], i) => (
          <li key={i} className="flex gap-3">
            <span className="grid size-6 shrink-0 place-items-center rounded-full border border-line text-[11px] text-fg-2">{i + 1}</span>
            <div>
              <div className="font-medium">{title}</div>
              <div className="text-[12.5px] text-fg-3">{d}</div>
            </div>
          </li>
        ))}
      </ol>
    </Card>
  );
}

function Inner() {
  const t = useT();
  const sp = useSearchParams();
  // the request id lives in state and is mirrored into the URL (?intent=…) so a reload or the MetaMask app
  // deep link resumes the same deposit
  const [intentId, setIntentId] = React.useState<string | null>(() => sp.get("intent"));
  // crypto deposits through NOWPayments (?method=crypto, a payment resumes with ?np=<id>), when the broker offers them
  const [npId, setNpId] = React.useState<number | null>(() => (/^\d{1,18}$/.test(sp.get("np") ?? "") ? Number(sp.get("np")) : null));
  const [method, setMethodState] = React.useState<"usdt" | "crypto">(() => (sp.get("method") === "crypto" || sp.get("np") ? "crypto" : "usdt"));
  const cfg = useWallet<WalletConfig>("config");
  const np = useWallet<NpConfig>("nowpayments/config");
  const valid = !!intentId && /^dep_[0-9a-f]{24}$/.test(intentId);
  const view = useWallet<IntentView>(valid ? `deposits/intents/${intentId}` : null, 5000);
  const setIntent = (id: string | null) => {
    setIntentId(id);
    window.history.replaceState(window.history.state, "", id ? `/wallet/deposit?intent=${id}` : "/wallet/deposit");
  };
  const setNp = (id: number | null) => {
    setNpId(id);
    window.history.replaceState(window.history.state, "", id ? `/wallet/deposit?np=${id}` : "/wallet/deposit?method=crypto");
  };
  const setMethod = (m: "usdt" | "crypto") => {
    setMethodState(m);
    window.history.replaceState(window.history.state, "", m === "crypto" ? (npId ? `/wallet/deposit?np=${npId}` : "/wallet/deposit?method=crypto") : intentId ? `/wallet/deposit?intent=${intentId}` : "/wallet/deposit");
  };
  const cryptoOn = !!np.data?.enabled && (np.data.coins?.length ?? 0) > 0;
  // a payment link keeps working even if the broker switched the option off since
  const crypto = method === "crypto" && (cryptoOn || npId !== null);

  let main: React.ReactNode;
  if (crypto) main = np.data ? <CryptoDeposit cfg={np.data} paymentId={npId} onPayment={setNp} /> : <Skeleton className="h-[420px] w-full rounded-[20px]" />;
  else if (cfg.error && !cfg.data) main = <WalletUnavailable onRetry={cfg.reload} />;
  else if (!cfg.data || (valid && !view.data && !view.error)) main = <Skeleton className="h-[420px] w-full rounded-[20px]" />;
  else if (valid && view.error) main = <WalletUnavailable onRetry={view.reload} message={view.error.status === 404 ? t("wallet.deposit.notFound") : undefined} />;
  else if (valid && view.data?.deposit) main = <Tracker view={view.data} onNew={() => setIntent(null)} />;
  else if (valid && view.data) main = <PayPanel view={view.data} onSubmitted={view.reload} />;
  else main = <StartForm cfg={cfg.data} onCreated={(i) => setIntent(i.id)} />;

  return (
    <div className="pb-16">
      <PageHeader
        title={crypto ? t("wallet.nowpayments.title") : t("wallet.depositUsdt")}
        subtitle={crypto ? t("wallet.nowpayments.subtitle") : t("wallet.deposit.subtitle")}
        actions={
          <Link href="/wallet">
            <Button variant="surface">
              <ArrowLeft className="rtl:-scale-x-100" /> {t("wallet.wallet")}
            </Button>
          </Link>
        }
      />
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
        <div className="xl:col-span-8">
          {cryptoOn && <MethodPicker value={crypto ? "crypto" : "usdt"} onChange={setMethod} />}
          {main}
        </div>
        <div className="xl:col-span-4">{crypto ? <CryptoHowItWorks /> : cfg.data && <HowItWorks cfg={cfg.data} />}</div>
      </div>
    </div>
  );
}

export function LiveDepositPage() {
  return (
    <React.Suspense fallback={null}>
      <Inner />
    </React.Suspense>
  );
}
