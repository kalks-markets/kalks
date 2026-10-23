"use client";

import * as React from "react";
import Link from "next/link";
import { ArrowDownLeft, ArrowLeftRight, ArrowUpRight, Award, CircleAlert, ExternalLink, IdCard, RotateCw, ShieldCheck } from "lucide-react";
import { Button, Card, Chip, CoinIcon, EmptyState, Progress, cn, formatDateTime, shortHash, type ChipTone } from "@/components/kit";
import { Trans, useT } from "@kalks/i18n/react";
import type { MessageKey, T } from "@kalks/i18n";
import { CHAIN_LABEL, fmt, type ActivityItem, type Chain, type Deposit, type DepositStatus, type WithdrawalStatus } from "./api";

/** Status chips: `label` is a message key, translated when rendered. */
export const DEPOSIT_STATUS: Record<DepositStatus, { tone: ChipTone; label: MessageKey }> = {
  pending: { tone: "warn", label: "wallet.status.deposit.pending" },
  confirming: { tone: "info", label: "wallet.status.deposit.confirming" },
  credited: { tone: "up", label: "wallet.status.deposit.credited" },
  failed: { tone: "down", label: "common.failed" },
  review: { tone: "warn", label: "wallet.status.deposit.review" },
  unmatched: { tone: "warn", label: "wallet.status.deposit.review" },
  rejected: { tone: "down", label: "wallet.status.deposit.rejected" },
};

export const WITHDRAWAL_STATUS: Record<WithdrawalStatus, { tone: ChipTone; label: MessageKey }> = {
  requested: { tone: "warn", label: "wallet.status.withdrawal.requested" },
  approved: { tone: "info", label: "common.approved" },
  paid: { tone: "info", label: "wallet.status.withdrawal.paid" },
  completed: { tone: "up", label: "common.completed" },
  rejected: { tone: "down", label: "common.rejected" },
  cancelled: { tone: "neutral", label: "common.cancelled" },
};

const TRANSFER_STATUS: Record<string, { tone: ChipTone; label: MessageKey }> = {
  pending: { tone: "warn", label: "common.processing" },
  completed: { tone: "up", label: "common.completed" },
  failed: { tone: "down", label: "common.failed" },
};

export const KIND_LABEL: Record<string, MessageKey> = {
  commission: "wallet.kind.commission",
  ib_payout: "wallet.kind.ibPayout",
  prop_purchase: "wallet.kind.propPurchase",
  prop_payout: "wallet.kind.propPayout",
  pamm_invest: "wallet.kind.pammInvest",
  pamm_redeem: "wallet.kind.pammRedeem",
  copy_fee: "wallet.kind.copyFee",
  mam_fee: "wallet.kind.mamFee",
  adjustment: "wallet.kind.adjustment",
  // Back Office "Balance & credit" (manual adjustments): the statement note is shown underneath
  adjustment_in: "wallet.kind.adjustment",
  adjustment_out: "wallet.kind.adjustment",
  manual_deposit: "wallet.txType.deposit",
  manual_withdrawal: "wallet.txType.withdrawal",
  refund: "wallet.kind.refund",
};

/** Amount fields: accepts a comma as the decimal point, drops anything that isn't a digit, keeps one point and 2 decimals. */
export function cleanAmount(raw: string): string {
  const v = raw.replace(/,/g, ".").replace(/[^\d.]/g, "");
  const dot = v.indexOf(".");
  return dot < 0 ? v : v.slice(0, dot + 1) + v.slice(dot + 1).replace(/\./g, "").slice(0, 2);
}

export function StatusTag({ tone, label }: { tone: ChipTone; label: MessageKey }) {
  const t = useT();
  return (
    <Chip size="sm" tone={tone} dot>
      {t(label)}
    </Chip>
  );
}

export function ChainBadge({ chain }: { chain: Chain }) {
  return (
    <span className="inline-flex items-center gap-1.5">
      <span className="relative">
        <CoinIcon coin="usdt" size={20} />
        <CoinIcon coin={chain === "bsc" ? "bnb" : "trx"} size={11} className="absolute -bottom-0.5 -end-1 ring-2 ring-surface" />
      </span>
      <span className="text-[12.5px] text-fg-2">
        USDT <span className="text-fg-3">· {CHAIN_LABEL[chain].short}</span>
      </span>
    </span>
  );
}

export function HashLink({ hash, url, className }: { hash: string | null | undefined; url?: string | null; className?: string }) {
  if (!hash) return <span className="text-fg-3">—</span>;
  return (
    <a href={url ?? "#"} target="_blank" rel="noreferrer" onClick={(e) => e.stopPropagation()} className={cn("inline-flex items-center gap-1 font-mono text-[12px] text-fg-2 hover:text-ember", className)}>
      {shortHash(hash, 8, 6)}
      <ExternalLink className="size-3" />
    </a>
  );
}

/** Confirmations x / y as a bar. */
export function Confirmations({ d }: { d: Pick<Deposit, "confirmations" | "required_confirmations" | "status"> }) {
  const t = useT();
  const done = d.status === "credited";
  const pct = done ? 100 : d.required_confirmations ? (Math.min(d.confirmations, d.required_confirmations) / d.required_confirmations) * 100 : 0;
  return (
    <div>
      <div className="mb-1.5 flex items-center justify-between text-[12px]">
        <span className="text-fg-2">
          <Trans k="wallet.confirmations.progress" vars={{ done: done ? d.required_confirmations : Math.min(d.confirmations, d.required_confirmations), required: d.required_confirmations }} tags={{ num: (c) => <span className="k-num font-semibold text-fg">{c}</span> }} />
        </span>
        <span className="text-fg-3">{done ? t("wallet.confirmations.complete") : d.status === "pending" ? t("wallet.confirmations.firstBlock") : t("wallet.confirmations.confirming")}</span>
      </div>
      <Progress value={pct} tone={done ? "up" : "ember"} />
    </div>
  );
}

export function KycNotice({ status }: { status: string }) {
  const t = useT();
  if (status === "verified") return null;
  const pending = status === "pending";
  return (
    <div className="flex flex-col gap-3 rounded-[18px] border border-warn/30 bg-warn-soft px-5 py-4 sm:flex-row sm:items-center">
      <span className="grid size-10 shrink-0 place-items-center rounded-full border border-warn/40 bg-warn/15 text-warn">
        <IdCard className="size-5" />
      </span>
      <div className="min-w-0 flex-1">
        <div className="text-[14px] font-medium">{pending ? t("wallet.kyc.inReview") : status === "rejected" ? t("wallet.kyc.needsAttention") : t("wallet.kyc.verifyToWithdraw")}</div>
        <p className="mt-0.5 text-[12.5px] text-fg-2">
          {pending ? t("wallet.kyc.pendingText") : t("wallet.kyc.requiredText")}
        </p>
      </div>
      <Link href="/profile/verification" className="shrink-0">
        <Button variant="surface" size="sm">
          <ShieldCheck /> {pending ? t("wallet.kyc.viewVerification") : t("wallet.kyc.verifyNow")}
        </Button>
      </Link>
    </div>
  );
}

export function WalletUnavailable({ onRetry, message }: { onRetry: () => void; message?: string }) {
  const t = useT();
  return (
    <Card>
      <EmptyState
        art="connectionLost"
        title={t("wallet.unavailable.title")}
        text={message ?? t("wallet.unavailable.text")}
        action={
          <Button variant="surface" onClick={onRetry}>
            <RotateCw /> {t("common.retry")}
          </Button>
        }
      />
    </Card>
  );
}

/** A NOWPayments coin code as people read it ("usdttrc20" → "USDT TRC20", "bnbbsc" → "BNB"). */
export function npCoinName(code: string | null | undefined): string {
  const c = (code ?? "").toLowerCase();
  const nets: Record<string, string> = { trc20: "TRC20", bsc: "BEP20", erc20: "ERC20", sol: "SOL", matic: "Polygon", ton: "TON" };
  for (const base of ["usdt", "usdc"]) if (c.startsWith(base) && c.length > base.length) return `${base.toUpperCase()} ${nets[c.slice(base.length)] ?? c.slice(base.length).toUpperCase()}`;
  if (c === "bnbbsc" || c === "bnbmainnet") return "BNB";
  return c.toUpperCase();
}

export function activityTitle(a: ActivityItem, t: T): string {
  const kind = KIND_LABEL[a.kind ?? ""];
  switch (a.type) {
    case "deposit":
      if (a.kind === "nowpayments") return t("wallet.nowpayments.activityLine", { coin: npCoinName(a.note) });
      return t("wallet.depositLine", { network: a.chain ? CHAIN_LABEL[a.chain].short : "" }).trim();
    case "withdrawal":
      return t("wallet.withdrawalLine", { network: a.chain ? CHAIN_LABEL[a.chain].short : "" }).trim();
    case "transfer":
      return a.direction === "out" ? t("wallet.activity.toTrading", { login: a.login }) : t("wallet.activity.fromTrading", { login: a.login });
    default:
      return kind ? t(kind) : t("wallet.activity.walletTx");
  }
}

export function activityStatus(a: ActivityItem): { tone: ChipTone; label: MessageKey } | null {
  if (a.type === "deposit") return DEPOSIT_STATUS[a.status as DepositStatus] ?? null;
  if (a.type === "withdrawal") return WITHDRAWAL_STATUS[a.status as WithdrawalStatus] ?? null;
  if (a.type === "transfer") return TRANSFER_STATUS[a.status] ?? null;
  return null;
}

function ActivityIcon({ a }: { a: ActivityItem }) {
  const inbound = a.direction === "in";
  const dim = ["failed", "rejected", "cancelled"].includes(a.status);
  const icon = a.type === "transfer" ? <ArrowLeftRight /> : a.type === "other" ? <Award /> : inbound ? <ArrowDownLeft /> : <ArrowUpRight />;
  return (
    <span className={cn("grid size-9 shrink-0 place-items-center rounded-full border [&_svg]:size-4", dim ? "border-line bg-surface-3 text-fg-3" : inbound ? "border-up/25 bg-up-soft text-up" : "border-line bg-surface-3 text-fg-2")}>
      {icon}
    </span>
  );
}

export function ActivityRow({ a }: { a: ActivityItem }) {
  const t = useT();
  const st = activityStatus(a);
  const dim = ["failed", "rejected", "cancelled"].includes(a.status);
  const sub =
    a.type === "deposit" && a.status === "confirming" && a.required_confirmations
      ? t("wallet.activity.confirmations", { done: Math.min(a.confirmations ?? 0, a.required_confirmations), required: a.required_confirmations })
      : a.type === "withdrawal" && a.address
        ? t("wallet.activity.to", { address: shortHash(a.address, 6, 4) })
        : a.kind === "nowpayments"
          ? ""
          : a.note ?? "";
  return (
    <div className="k-row flex items-center gap-3 px-4 py-3">
      <ActivityIcon a={a} />
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2 text-[13.5px] font-medium">
          {activityTitle(a, t)}
          {st && a.status !== "completed" && a.status !== "credited" && <StatusTag {...st} />}
        </div>
        <div className="truncate text-[11.5px] text-fg-3">
          {formatDateTime(a.created_at)}
          {sub ? ` · ${sub}` : ""}
          {a.tx_hash && (
            <>
              {" · "}
              <HashLink hash={a.tx_hash} url={a.explorer_url} className="text-[11.5px]" />
            </>
          )}
        </div>
      </div>
      <span dir="ltr" className={cn("k-num whitespace-nowrap text-[14px] font-semibold", dim ? "text-fg-3 line-through" : a.direction === "in" ? "text-up" : "text-fg")}>
        {a.direction === "in" ? "+" : "−"}
        {fmt(a.amount)}
        <span className="ms-1 text-[0.8em] font-medium text-fg-3">{a.type === "transfer" && a.direction === "in" ? "USD" : "USDT"}</span>
      </span>
    </div>
  );
}

export function InlineError({ children }: { children: React.ReactNode }) {
  if (!children) return null;
  return (
    <div className="flex items-start gap-2 rounded-[12px] border border-down/30 bg-down-soft px-3 py-2 text-[12.5px] text-fg">
      <CircleAlert className="mt-0.5 size-3.5 shrink-0 text-down" />
      <span>{children}</span>
    </div>
  );
}

export function Tile({ label, value, hint }: { label: string; value: React.ReactNode; hint?: React.ReactNode }) {
  return (
    <div className="k-row px-3 py-2.5">
      <div className="text-[11.5px] text-fg-3">{label}</div>
      <div className="k-num mt-0.5 text-[13px] font-semibold">{value}</div>
      {hint && <div className="mt-0.5 text-[11px] text-fg-3">{hint}</div>}
    </div>
  );
}
