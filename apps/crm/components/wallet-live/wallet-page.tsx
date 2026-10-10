"use client";

import * as React from "react";
import Link from "next/link";
import { ArrowDownToLine, ArrowLeftRight, ArrowUpFromLine, ArrowUpRight, Bell, ChevronRight, History, Lock } from "lucide-react";
import { Button, Card, CardHeader, Chip, EmptyState, PageHeader, Skeleton, formatDateTime } from "@/components/kit";
import { Trans, useT } from "@kalks/i18n/react";
import { useReadOnly, useSession } from "@/components/session";
import { toUsd, useAccounts } from "@/components/trading/api";
import { CHAIN_LABEL, fmt, usdtAvailable, useWallet, walletApi, type ActivityItem, type Notification, type Overview, type Page } from "./api";
import { ActivityRow, Confirmations, DEPOSIT_STATUS, HashLink, KycNotice, StatusTag, WITHDRAWAL_STATUS, WalletUnavailable } from "./ui";
import { isPropAccount } from "@/components/trading/ui";
import { PageHero } from "@/components/page-hero";

function BalanceCard({ o, loading }: { o: Overview | null; loading: boolean }) {
  const t = useT();
  const b = usdtAvailable(o);
  const total = Number(b.available) + Number(b.locked);
  return (
    <Card className="h-full">
      <div className="p-6">
        <div className="k-label">{t("wallet.balance.title")}</div>
        <div className="mt-3 flex flex-wrap items-baseline gap-2">
          {loading ? (
            <Skeleton className="h-11 w-48" />
          ) : (
            <span dir="ltr" className="k-num text-[40px] font-semibold leading-none tracking-[-0.02em]">
              {fmt(b.available)} <span className="text-[18px] font-medium text-fg-3">USDT</span>
            </span>
          )}
        </div>
        <div className="mt-2 text-[13px] text-fg-2">{t("wallet.balance.sub")}</div>
        <div className="mt-6 grid grid-cols-2 gap-2 sm:grid-cols-3">
          <div className="k-row px-4 py-3">
            <div className="text-[12px] text-fg-3">{t("wallet.available")}</div>
            <div className="k-num mt-1 text-[16px] font-semibold">{fmt(b.available)}</div>
          </div>
          <div className="k-row px-4 py-3">
            <div className="flex items-center gap-1 text-[11px] uppercase tracking-wider text-fg-3">
              <Lock className="size-3" /> {t("wallet.inProgress")}
            </div>
            <div className="k-num mt-1 text-[16px] font-semibold">{fmt(b.locked)}</div>
          </div>
          <div className="k-row col-span-2 px-4 py-3 sm:col-span-1">
            <div className="text-[12px] text-fg-3">{t("common.total")}</div>
            <div className="k-num mt-1 text-[16px] font-semibold">{fmt(total)}</div>
          </div>
        </div>
      </div>
    </Card>
  );
}

function QuickActions({ kyc }: { kyc: string }) {
  const t = useT();
  const items = [
    { href: "/wallet/deposit", title: t("common.deposit"), sub: t("wallet.quick.depositSub"), icon: <ArrowDownToLine />, primary: true },
    { href: "/wallet/withdraw", title: t("common.withdraw"), sub: kyc === "verified" ? t("wallet.quick.withdrawSub") : t("wallet.quick.withdrawKyc"), icon: <ArrowUpFromLine /> },
    { href: "/wallet/transfer", title: t("common.transfer"), sub: t("wallet.quick.transferSub"), icon: <ArrowLeftRight /> },
  ];
  return (
    <div className="grid h-full grid-cols-1 gap-3 sm:grid-cols-3 xl:grid-cols-1">
      {items.map((it) => (
        <Link key={it.href} href={it.href} className="k-card group flex h-full items-center gap-4 px-5 py-4 transition-colors hover:border-[var(--k-border-top)]">
          <span className={it.primary ? "k-ember-btn grid size-11 shrink-0 place-items-center rounded-full [&_svg]:size-[18px]" : "grid size-11 shrink-0 place-items-center rounded-full border border-line bg-surface-2 text-fg-2 group-hover:text-fg [&_svg]:size-[18px]"}>{it.icon}</span>
          <div className="min-w-0 flex-1">
            <div className="text-[15px] font-medium">{it.title}</div>
            <div className="truncate text-[12px] text-fg-3">{it.sub}</div>
          </div>
          <ChevronRight className="size-4 text-fg-3 group-hover:text-fg rtl:-scale-x-100" />
        </Link>
      ))}
    </div>
  );
}

function InProgress({ o }: { o: Overview }) {
  const t = useT();
  if (!o.pending_deposits.length && !o.open_withdrawals.length) return null;
  return (
    <Card>
      <CardHeader title={t("wallet.inProgress")} subtitle={t("wallet.progress.subtitle")} />
      <div className="mt-4 space-y-2 px-4 pb-5 sm:px-6">
        {o.pending_deposits.map((d) => (
          <Link key={`d${d.id}`} href={d.intent_id ? `/wallet/deposit?intent=${d.intent_id}` : "/wallet/history?type=deposit"} className="k-row block px-4 py-3 hover:border-[var(--k-border-top)]">
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-[13.5px] font-medium">{t("wallet.depositLine", { network: CHAIN_LABEL[d.chain].short })}</span>
              <StatusTag {...DEPOSIT_STATUS[d.status]} />
              <span dir="ltr" className="k-num ms-auto text-[14px] font-semibold text-up">+{fmt(d.amount ?? d.expected_amount)} USDT</span>
            </div>
            <div className="mt-1 text-[11.5px] text-fg-3">
              {formatDateTime(d.created_at)} · <HashLink hash={d.tx_hash} url={d.explorer_url} className="text-[11.5px]" />
            </div>
            {d.status !== "review" && (
              <div className="mt-2.5">
                <Confirmations d={d} />
              </div>
            )}
          </Link>
        ))}
        {o.open_withdrawals.map((w) => (
          <Link key={`w${w.id}`} href="/wallet/withdraw" className="k-row flex flex-wrap items-center gap-2 px-4 py-3 hover:border-[var(--k-border-top)]">
            <span className="text-[13.5px] font-medium">{t("wallet.withdrawalLine", { network: CHAIN_LABEL[w.chain].short })}</span>
            <StatusTag {...WITHDRAWAL_STATUS[w.status]} />
            <span dir="ltr" className="k-num ms-auto text-[14px] font-semibold">−{fmt(w.amount)} USDT</span>
            <div className="w-full text-[11.5px] text-fg-3">
              {formatDateTime(w.created_at)} · {t("wallet.progress.withdrawalNet", { net: fmt(w.net_amount), fee: fmt(w.fee) })}
            </div>
          </Link>
        ))}
      </div>
    </Card>
  );
}

function FundAccounts() {
  const t = useT();
  const { data } = useAccounts(15000);
  const live = (data?.accounts ?? []).filter((a) => a.type === "live" && !isPropAccount(a));
  return (
    <Card className="h-full">
      <CardHeader
        title={t("wallet.fundTradingAccount")}
        subtitle={t("wallet.fund.subtitle")}
        action={
          <Link href="/wallet/transfer">
            <Button size="xs" variant="surface">
              {t("common.transfer")}
            </Button>
          </Link>
        }
      />
      <div className="mt-4 space-y-2 px-4 pb-5 sm:px-6">
        {data && live.length === 0 && (
          <div className="k-row px-4 py-4 text-[13px] text-fg-2">
            <Trans
              k="wallet.fund.noLive"
              tags={{
                link: (c) => (
                  <Link href="/accounts/new?type=live" className="text-ember hover:underline">
                    {c}
                  </Link>
                ),
              }}
            />
          </div>
        )}
        {!data && <Skeleton className="h-16 w-full rounded-[14px]" />}
        {live.map((a) => (
          <Link key={a.login} href={`/wallet/transfer?to=${a.login}`} className="k-row group flex items-center gap-3 px-4 py-3 hover:border-[var(--k-border-top)]">
            <Chip size="sm" tone="ember" className="font-semibold tracking-wider">
              {t("wallet.liveBadge")}
            </Chip>
            <div className="min-w-0 flex-1">
              <div className="truncate text-[13.5px] font-medium">
                {a.groupName} <span className="font-mono text-[12px] text-fg-3">#{a.login}</span>
              </div>
              <div className="k-num text-[11.5px] text-fg-3">
                {t("common.balance")} {a.cent ? "USC " : "$"}
                {fmt(a.balance)}
                {a.cent && <> · ≈ ${fmt(toUsd(a, a.balance))}</>}
              </div>
            </div>
            <span className="inline-flex items-center gap-1 text-[12.5px] font-medium text-fg-3 group-hover:text-ember">
              {t("wallet.fund.topUp")} <ChevronRight className="size-4 rtl:-scale-x-100" />
            </span>
          </Link>
        ))}
      </div>
    </Card>
  );
}

function Notifications() {
  const { data, reload } = useWallet<{ items: Notification[]; unread: number }>("notifications", 30000);
  const t = useT();
  const items = data?.items.slice(0, 5) ?? [];
  if (!items.length) return null;
  return (
    <Card>
      <CardHeader
        title={t("wallet.updates.title")}
        icon={<Bell />}
        subtitle={data?.unread ? t("wallet.updates.new", { count: data.unread }) : t("wallet.updates.allRead")}
        action={
          data?.unread ? (
            <Button
              size="xs"
              variant="ghost"
              onClick={async () => {
                await walletApi("notifications/read", { body: {} }).catch(() => {});
                reload();
              }}
            >
              {t("shell.markAllRead")}
            </Button>
          ) : undefined
        }
      />
      <div className="mt-3 space-y-1 px-4 pb-4 sm:px-6">
        {items.map((n) => (
          <div key={n.id} className="flex gap-3 rounded-[12px] px-2 py-2">
            <span className={n.read ? "mt-1.5 size-1.5 shrink-0 rounded-full bg-transparent" : "mt-1.5 size-1.5 shrink-0 rounded-full bg-ember"} />
            <div className="min-w-0">
              <div className="text-[13px] font-medium">{n.title}</div>
              <div className="text-[12px] text-fg-3">{n.body}</div>
              <div className="mt-0.5 text-[11px] text-fg-3">{formatDateTime(n.created_at)}</div>
            </div>
          </div>
        ))}
      </div>
    </Card>
  );
}

export function LiveWalletPage() {
  const t = useT();
  const me = useSession();
  // view-only logins (and read-only staff sessions) see balances and activity, never money actions
  const readOnly = useReadOnly();
  const { data: o, error, loading, reload } = useWallet<Overview>("overview", 10000);
  const act = useWallet<Page<ActivityItem>>("activity?limit=8", 15000);

  return (
    <div className="pb-16">
      <PageHero
        page="wallet"
        overlap
        title={t("wallet.wallet")}
        lead={t("wallet.page.subtitle")}
        actions={
          <>
            <Link href="/wallet/history">
              <Button variant="surface" size="lg">
                <History /> {t("wallet.history")}
              </Button>
            </Link>
            {!readOnly && (
              <Link href="/wallet/deposit">
                <Button variant="ember" size="lg">
                  <ArrowDownToLine /> {t("wallet.depositUsdt")}
                </Button>
              </Link>
            )}
          </>
        }
      />

      {error && !o ? (
        <WalletUnavailable onRetry={reload} message={error.status > 0 && error.status < 500 ? error.message : undefined} />
      ) : (
        <div className="space-y-4">
          <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
            <div className={readOnly ? "xl:col-span-12" : "xl:col-span-8"}>
              <BalanceCard o={o} loading={loading} />
            </div>
            {!readOnly && (
              <div className="xl:col-span-4">
                <QuickActions kyc={me.kyc_status} />
              </div>
            )}
          </div>
          {!readOnly && <KycNotice status={me.kyc_status} />}
          {o && <InProgress o={o} />}
          <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
            <Card className={readOnly ? "xl:col-span-12" : "xl:col-span-7"}>
              <CardHeader
                title={t("wallet.recent.title")}
                subtitle={act.data ? t("wallet.recent.count", { count: act.data.total }) : undefined}
                action={
                  <Link href="/wallet/history">
                    <Button size="sm" variant="surface">
                      {t("common.viewAll")} <ArrowUpRight className="rtl:-scale-x-100" />
                    </Button>
                  </Link>
                }
              />
              <div className="mt-4 space-y-2 px-4 pb-5 sm:px-6">
                {act.loading && <Skeleton className="h-16 w-full rounded-[14px]" />}
                {act.data && act.data.items.length === 0 && (
                  <EmptyState art="emptyHistory" title={t("wallet.recent.emptyTitle")} text={t("wallet.recent.emptyText")} action={readOnly ? undefined : <Link href="/wallet/deposit"><Button variant="ember">{t("wallet.recent.firstDeposit")}</Button></Link>} />
                )}
                {act.data?.items.map((a) => <ActivityRow key={`${a.type}${a.id}`} a={a} />)}
              </div>
            </Card>
            {!readOnly && (
              <div className="space-y-4 xl:col-span-5">
                <FundAccounts />
                {/* wallet notices belong to the account holder, not to a view-only login */}
                {!me.viewer && <Notifications />}
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
