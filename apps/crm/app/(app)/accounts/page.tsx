"use client";

import * as React from "react";
import Link from "next/link";
import { Archive, ArrowRight, ArrowUpRight, Download, FlaskConical, Layers, Plus, RotateCcw, ShieldCheck, TrendingUp, Wallet } from "lucide-react";
import { toast } from "sonner";
import { useT } from "@kalks/i18n/react";
import { Button, Card, CardHeader, Chip, Donut, EmptyState, Icon3D, KpiCard, Money, PageHeader, Reveal, Segmented, cn } from "@/components/kit";
import { ACCOUNTS, ACCOUNT_GROUPS, POSITIONS, accountUsd, freeMargin, type TradingAccount } from "@kalks/mock";
import { ARCHIVED_ACCOUNTS } from "@kalks/mock/accounts-extra";
import { AccountBadge, AccountRow, accountTitle } from "@/components/account-row";
import { GroupCard } from "@/components/accounts/group-card";
import { IS_DEMO as DEMO_BUILD } from "@kalks/mock/mode";
import { LiveAccountsPage } from "@/components/trading/accounts-page";
import { TERMINAL_URL } from "@/lib/live";

const live = ACCOUNTS.filter((a) => a.type === "live");
const demo = ACCOUNTS.filter((a) => a.type === "demo");
const usd = (a: TradingAccount, v: number) => (a.cent ? v / 100 : v);

function ArchivedRow({ a }: { a: TradingAccount }) {
  const t = useT();
  const [restored, setRestored] = React.useState(false);
  return (
    <div className="k-row flex flex-wrap items-center gap-3 p-4 sm:p-5">
      <AccountBadge a={a} />
      <div className="min-w-0">
        <Link href={`/accounts/${a.login}`} className="text-[15px] font-medium text-fg-2 hover:text-ember">
          {accountTitle(a, t)}
        </Link>
        <div className="font-mono text-[12.5px] text-fg-3">
          #{a.login} · {a.server} · {a.type === "demo" ? t("accounts.archived.onExpiry") : t("accounts.archived.byYou")}
        </div>
      </div>
      <div className="ms-auto flex items-center gap-4">
        <div className="text-end">
          <div className="text-[12px] text-fg-3">{t("accounts.archived.finalBalance")}</div>
          <Money value={a.balance} className="text-[15px] font-medium text-fg-2" countUp={false} />
        </div>
        {a.type === "live" ? (
          <Button
            size="sm"
            variant="surface"
            disabled={restored}
            onClick={() => {
              setRestored(true);
              toast.success(t("accounts.archived.restoreRequested"), { description: t("accounts.archived.restoreDesc", { login: a.login }) });
            }}
          >
            <RotateCcw /> {restored ? t("accounts.archived.requested") : t("accounts.archived.restore")}
          </Button>
        ) : (
          <Chip>{t("accounts.archived.expired")}</Chip>
        )}
      </div>
    </div>
  );
}

function AllocationCard() {
  const t = useT();
  const data = live.map((a, i) => ({ label: `#${a.login}`, value: accountUsd(a, "equity"), color: ["var(--k-ember)", "#e9b949", "#22c55e"][i] }));
  const total = data.reduce((s, d) => s + d.value, 0);
  return (
    <Card className="h-full">
      <CardHeader title={t("accounts.allocation.title")} subtitle={t("accounts.allocation.subtitle")} />
      <div className="flex flex-col items-center gap-6 px-6 pb-6 pt-4 sm:flex-row">
        <Donut
          data={data}
          size={168}
          thickness={18}
          center={
            <div className="text-center">
              <div className="text-[12px] text-fg-3">{t("common.total")}</div>
              <Money value={total} decimals={0} className="text-[18px] font-semibold" />
            </div>
          }
        />
        <div className="w-full flex-1 space-y-2">
          {live.map((a, i) => (
            <Link key={a.login} href={`/accounts/${a.login}`} className="k-row flex items-center gap-3 px-3 py-2.5 transition-colors hover:bg-surface-3/60">
              <span className="size-2.5 rounded-full" style={{ background: data[i]!.color }} />
              <div className="min-w-0 flex-1">
                <div className="truncate text-[13px] font-medium">{a.nickname ?? accountTitle(a, t)}</div>
                <div className="font-mono text-[11px] text-fg-3">
                  #{a.login} · {a.group}
                </div>
              </div>
              <div className="k-num text-[12.5px] text-fg-2">{((data[i]!.value / total) * 100).toFixed(1)}%</div>
            </Link>
          ))}
        </div>
      </div>
    </Card>
  );
}

function PlatformCard() {
  const t = useT();
  return (
    <Card className="relative h-full overflow-hidden">
      {/* eslint-disable-next-line @next/next/no-img-element */}
      <img src="/assets/photos/trading-screen.jpg" alt="" className="absolute inset-0 size-full object-cover opacity-25" />
      <div className="absolute inset-0 bg-gradient-to-t from-[var(--k-surface)] via-[var(--k-surface)]/80 to-transparent" />
      <div className="relative flex h-full flex-col justify-end p-6">
        <Chip tone="ember" dot>
          {t("accounts.platform.mt5Compatible")}
        </Chip>
        <h3 className="mt-3 text-[17px] font-medium tracking-tight">{t("accounts.platform.title")}</h3>
        <p className="mt-1 text-[13px] text-fg-2">{t("accounts.platform.text")}</p>
        <div className="mt-4 flex flex-wrap gap-2">
          <Link target="_blank" rel="noopener" href={TERMINAL_URL}>
            <Button size="sm" variant="ember">
              WebTerminal <ArrowUpRight />
            </Button>
          </Link>
          <Button size="sm" variant="surface" onClick={() => toast.success(t("accounts.platform.downloading"), { description: "kalks5setup.exe · 24.1 MB" })}>
            <Download /> {t("accounts.platform.mt5Desktop")}
          </Button>
        </div>
      </div>
    </Card>
  );
}

function DemoAccountsPage() {
  const t = useT();
  const [tab, setTab] = React.useState<"live" | "demo" | "archived">("live");
  const totalEquity = live.reduce((s, a) => s + accountUsd(a, "equity"), 0);
  const totalFree = live.reduce((s, a) => s + usd(a, freeMargin(a)), 0);
  const openPos = POSITIONS.filter((p) => live.some((a) => a.login === p.login)).length;
  const list = tab === "live" ? live : tab === "demo" ? demo : ARCHIVED_ACCOUNTS;

  return (
    <div className="pb-16">
      <PageHeader
        title={t("accounts.list.title")}
        subtitle={t("accounts.list.subtitleDemo")}
        actions={
          <>
            <Link href="/wallet/transfer">
              <Button variant="surface" size="lg">
                <Wallet /> {t("accounts.list.transferFunds")}
              </Button>
            </Link>
            <Link href="/accounts/new">
              <Button variant="ember" size="lg" shimmer>
                <Plus /> {t("accounts.list.openAccount")}
              </Button>
            </Link>
          </>
        }
      />

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <KpiCard label={t("accounts.kpi.totalLiveEquity")} icon={<TrendingUp />} value={<Money value={totalEquity} />} chip={t("accounts.kpi.changeToday", { change: "+2.52%" })} chipTone="up" href="/portfolio" />
        <KpiCard
          label={t("accounts.label.freeMargin")}
          icon={<ShieldCheck />}
          value={<Money value={totalFree} />}
          chip={t("accounts.kpi.ofEquity", { pct: ((totalFree / totalEquity) * 100).toFixed(1) })}
          chipTone="neutral"
          delay={0.05}
        />
        <KpiCard
          label={t("common.accounts")}
          icon={<Layers />}
          value={<span className="k-num">{ACCOUNTS.length}</span>}
          footer={
            <div className="flex items-center gap-1.5">
              <Chip size="sm" tone="ember">
                {t("accounts.kpi.liveCount", { count: live.length })}
              </Chip>
              <Chip size="sm" tone="gold">
                {t("accounts.kpi.demoCount", { count: demo.length })}
              </Chip>
              <Chip size="sm">{t("accounts.kpi.openPositions", { count: openPos })}</Chip>
            </div>
          }
          delay={0.1}
        />
        <KpiCard
          label={t("accounts.kpi.demoAccounts")}
          icon={<FlaskConical />}
          value={<span className="k-num">{demo.length}</span>}
          hot
          illustration="rocket"
          footer={<span className="text-[11.5px] text-fg-2">{t("accounts.kpi.demoFooter")}</span>}
          delay={0.15}
        />
      </div>

      <div className="mt-4">
        <Reveal delay={0.1}>
          <Card>
            <CardHeader
              title={t("accounts.list.myAccounts")}
              subtitle={tab === "archived" ? t("accounts.list.archivedHint") : t("accounts.list.tapHint")}
              action={
                <Segmented
                  className="hidden sm:inline-flex"
                  size="xs"
                  value={tab}
                  onChange={setTab}
                  options={[
                    { value: "live", label: <>{t("common.live")} <span className="text-fg-3">{live.length}</span></> },
                    { value: "demo", label: <>{t("common.demo")} <span className="text-fg-3">{demo.length}</span></> },
                    { value: "archived", label: <><Archive className="size-3" /> <span className="hidden sm:inline">{t("accounts.tab.archived")}</span> <span className="text-fg-3">{ARCHIVED_ACCOUNTS.length}</span></> },
                  ]}
                />
              }
            />
            <div className="px-4 pt-4 sm:hidden">
              <Segmented
                size="xs"
                value={tab}
                onChange={setTab}
                options={[
                  { value: "live", label: <>{t("common.live")} <span className="text-fg-3">{live.length}</span></> },
                  { value: "demo", label: <>{t("common.demo")} <span className="text-fg-3">{demo.length}</span></> },
                  { value: "archived", label: <>{t("accounts.tab.archived")} <span className="text-fg-3">{ARCHIVED_ACCOUNTS.length}</span></> },
                ]}
              />
            </div>
            <div className="mt-4 space-y-3 px-4 pb-5 sm:px-6">
              {list.length === 0 && <EmptyState art="welcome" contentSlot={{ page: "empty", slot: "accounts.image" }} title={t("accounts.empty.noAccounts")} text={t("accounts.empty.openFirst")} />}
              {list.map((a, i) => (
                <Reveal key={a.login} delay={i * 0.05}>{tab === "archived" ? <ArchivedRow a={a} /> : <AccountRow a={a} />}</Reveal>
              ))}
              <Link href={`/accounts/new${tab === "demo" ? "?type=demo" : ""}`} className="flex items-center justify-center gap-2 rounded-[14px] border border-dashed border-line py-4 text-[13.5px] text-fg-3 transition-colors hover:border-ember/40 hover:bg-ember-soft hover:text-ember">
                <Plus className="size-4" /> {tab === "demo" ? t("accounts.list.openNewDemo") : t("accounts.list.openNewLive")}
              </Link>
            </div>
          </Card>
        </Reveal>
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4 lg:grid-cols-12">
        <Reveal delay={0.15} className="lg:col-span-7">
          <AllocationCard />
        </Reveal>
        <Reveal delay={0.2} className="lg:col-span-5">
          <PlatformCard />
        </Reveal>
      </div>

      <Reveal delay={0.1} className="mt-4 block">
        <Card>
          <CardHeader
            title={t("accounts.compare.title")}
            subtitle={t("accounts.compare.subtitle")}
            action={
              <Link href="/accounts/new" className="hidden sm:block">
                <Button size="sm" variant="surface">
                  {t("accounts.list.openAccount")} <ArrowRight className="rtl:-scale-x-100" />
                </Button>
              </Link>
            }
          />
          <div className="grid grid-cols-1 gap-4 px-4 pb-6 pt-4 sm:grid-cols-2 sm:px-6 xl:grid-cols-4">
            {ACCOUNT_GROUPS.map((g) => (
              <Link key={g.id} href={`/accounts/new?group=${g.id}`} className={cn("block")}>
                <GroupCard g={g} />
              </Link>
            ))}
          </div>
          <div className="flex flex-col items-start gap-3 border-t border-line px-6 py-4 sm:flex-row sm:items-center">
            <Icon3D name="shield" size={36} />
            <p className="flex-1 text-[12.5px] text-fg-3">
              {t("accounts.compare.footer")}
            </p>
          </div>
        </Card>
      </Reveal>
    </div>
  );
}

/** Live builds: real accounts from the trading engine (via /api/trading). Demo builds: mock data. */
export default function AccountsPage() {
  return DEMO_BUILD ? <DemoAccountsPage /> : <LiveAccountsPage />;
}
