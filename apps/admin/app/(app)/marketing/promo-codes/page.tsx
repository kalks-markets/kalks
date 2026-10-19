"use client";

import * as React from "react";
import { Ban, Copy, Download, MoreHorizontal, Pause, Play, Plus, ShieldAlert, Ticket, TimerOff, TrendingUp, Users, Wallet } from "lucide-react";
import { toast } from "sonner";
import {
  Avatar,
  Button,
  Card,
  CardHeader,
  Chip,
  CopyButton,
  DataTable,
  Donut,
  Flag,
  Icon3D,
  IconButton,
  KpiCard,
  Menu,
  Money,
  PageHeader,
  Progress,
  Reveal,
  Segmented,
  Sparkline,
  type ChipTone,
  type Column,
  cn,
} from "@kalks/ui";
import { PROMO_CODES, PROMO_DAILY, PROMO_KPIS, PROMO_REDEMPTIONS, PROMO_TYPE_META, type PromoCode, type PromoStatus, type PromoType } from "@kalks/mock/admin-promo-codes";
import { MiniStat, daysFromToday, fmtDate, fmtDateTime, fmtK } from "@/components/marketing/kit";
import { PromoCreateDialog } from "@/components/marketing/promo-create-dialog";
import { StackedBars } from "@/components/marketing/stacked-bars";
import { IS_DEMO } from "@kalks/mock/mode";
import { LivePromoCodes } from "@/components/marketing/live/promos";

const TYPE_COLOR: Record<PromoType, string> = { "deposit-bonus": "#ff5a1f", "fee-waiver": "#e9b949", "prop-retry": "#22c55e" };
const STATUS_META: Record<PromoStatus, { tone: ChipTone; label: string }> = {
  active: { tone: "up", label: "Active" },
  paused: { tone: "neutral", label: "Paused" },
  scheduled: { tone: "info", label: "Scheduled" },
  expired: { tone: "neutral", label: "Expired" },
  exhausted: { tone: "warn", label: "Limit reached" },
};
type Filter = "all" | "active" | "scheduled" | "paused" | "ended";

function valueText(c: PromoCode) {
  return c.type === "deposit-bonus" ? `${c.value}%` : c.type === "fee-waiver" ? `${c.value}% off` : `${c.value}× retry`;
}

function DemoPromoCodesPage() {
  const [codes, setCodes] = React.useState<PromoCode[]>(PROMO_CODES);
  const [filter, setFilter] = React.useState<Filter>("all");
  const [open, setOpen] = React.useState(false);

  const setStatus = (id: string, status: PromoStatus) => setCodes((cs) => cs.map((c) => (c.id === id ? { ...c, status } : c)));

  const rows = codes.filter((c) => {
    if (filter === "all") return true;
    if (filter === "ended") return c.status === "expired" || c.status === "exhausted";
    return c.status === filter;
  });
  const k = PROMO_KPIS;
  const active = codes.filter((c) => c.status === "active").length;
  const roi = k.attributedDeposits / k.cost;

  const cols: Column<PromoCode>[] = [
    {
      key: "code",
      header: "Code",
      cell: (c) => (
        <div className="flex items-center gap-3">
          <span className="grid size-9 shrink-0 place-items-center rounded-[12px] border border-line bg-surface-2">
            <Icon3D name={PROMO_TYPE_META[c.type].icon} size={24} />
          </span>
          <div className="min-w-0">
            <div className="flex items-center gap-1">
              <span className="font-mono text-[13px] font-semibold tracking-wide">{c.code}</span>
              <CopyButton value={c.code} label={c.code} />
            </div>
            <div className="text-[11.5px] text-fg-3">by {c.createdBy.name.split(" ")[0]} · {fmtDate(c.starts, false)}</div>
          </div>
        </div>
      ),
      sort: (c) => c.code,
    },
    {
      key: "type",
      header: "Type",
      cell: (c) => (
        <Chip tone={PROMO_TYPE_META[c.type].tone} size="sm">
          {PROMO_TYPE_META[c.type].short}
        </Chip>
      ),
      sort: (c) => c.type,
    },
    {
      key: "value",
      header: "Value",
      cell: (c) => (
        <div className="whitespace-nowrap">
          <div className="k-num text-[13.5px] font-medium">{valueText(c)}</div>
          <div className="text-[11.5px] text-fg-3">{c.cap}</div>
        </div>
      ),
      sort: (c) => c.value,
    },
    {
      key: "usage",
      header: "Usage / limit",
      width: "170px",
      cell: (c) => {
        const pct = (c.uses / c.limit) * 100;
        return (
          <div className="min-w-[130px]">
            <div className="mb-1 flex justify-between text-[11.5px]">
              <span className="k-num text-fg">{c.uses.toLocaleString("en-US")}</span>
              <span className="k-num text-fg-3">/ {c.limit.toLocaleString("en-US")}</span>
            </div>
            <Progress value={pct} tone={pct >= 100 ? "warn" : pct > 85 ? "gold" : "ember"} />
          </div>
        );
      },
      sort: (c) => c.uses / c.limit,
    },
    {
      key: "expiry",
      header: "Expiry",
      cell: (c) => {
        const d = daysFromToday(c.expires);
        return (
          <div className="whitespace-nowrap">
            <div className="k-num text-[13px]">{fmtDate(c.expires)}</div>
            <div className={cn("k-num text-[11px]", d < 0 ? "text-fg-3" : d <= 14 ? "text-warn" : "text-fg-3")}>{d < 0 ? `ended ${-d}d ago` : d === 0 ? "ends today" : `in ${d} days`}</div>
          </div>
        );
      },
      sort: (c) => c.expires,
      hideOn: "md",
    },
    { key: "segment", header: "Segment", cell: (c) => <span className="whitespace-nowrap text-[12.5px] text-fg-2">{c.segment}</span>, sort: (c) => c.segment, hideOn: "lg" },
    {
      key: "deposits",
      header: "Deposits",
      align: "right",
      cell: (c) => (c.deposits ? <Money value={c.deposits} decimals={0} countUp={false} className="text-[13px]" /> : <span className="text-fg-3">—</span>),
      sort: (c) => c.deposits,
      hideOn: "lg",
    },
    {
      key: "status",
      header: "Status",
      cell: (c) => (
        <Chip tone={STATUS_META[c.status].tone} dot>
          {STATUS_META[c.status].label}
        </Chip>
      ),
      sort: (c) => c.status,
    },
    {
      key: "act",
      header: "",
      align: "right",
      cell: (c) => (
        <div onClick={(e) => e.stopPropagation()}>
          <Menu
            trigger={
              <IconButton size="sm" aria-label={`Actions for ${c.code}`}>
                <MoreHorizontal />
              </IconButton>
            }
            items={[
              c.status === "active"
                ? { label: "Pause code", icon: <Pause />, onSelect: () => (setStatus(c.id, "paused"), toast.success(`${c.code} paused`, { description: "New redemptions are blocked; existing benefits stay." })) }
                : { label: "Activate code", icon: <Play />, onSelect: () => (setStatus(c.id, "active"), toast.success(`${c.code} is live`)) },
              { label: "Copy share link", icon: <Copy />, onSelect: () => (navigator.clipboard?.writeText(`https://app.kalkstrade.com/r/${c.code}`).catch(() => {}), toast.success("Link copied", { description: `app.kalkstrade.com/r/${c.code}` })) },
              { label: "Export redemptions", icon: <Download />, onSelect: () => toast.success("Export started", { description: `${c.code.toLowerCase()}-redemptions.csv · ${c.uses.toLocaleString("en-US")} rows` }) },
              "sep",
              { label: "Expire now", icon: <TimerOff />, danger: true, onSelect: () => (setStatus(c.id, "expired"), toast.success(`${c.code} expired`, { description: "Logged in the admin audit trail." })) },
            ]}
          />
        </div>
      ),
    },
  ];

  const byType = (Object.keys(PROMO_TYPE_META) as PromoType[]).map((t) => ({
    type: t,
    label: PROMO_TYPE_META[t].label,
    uses: codes.filter((c) => c.type === t).reduce((s, c) => s + c.uses, 0),
    deposits: codes.filter((c) => c.type === t).reduce((s, c) => s + c.deposits, 0),
    color: TYPE_COLOR[t],
  }));
  const totalUses = byType.reduce((s, x) => s + x.uses, 0);
  const top = [...codes].sort((a, b) => b.deposits - a.deposits).slice(0, 5);

  return (
    <div className="pb-16">
      <PageHeader
        title="Promo codes"
        subtitle="Deposit bonuses, fee waivers and free prop retries, targeted by segment with per-client limits."
        actions={
          <>
            <Button variant="surface" onClick={() => toast.info("Abuse protection", { description: "1 code per device, IP and payment instrument · 214 redemptions blocked this month" })}>
              <ShieldAlert /> Abuse rules
            </Button>
            <Button variant="ember" shimmer onClick={() => setOpen(true)}>
              <Plus /> New code
            </Button>
          </>
        }
      />

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <KpiCard label="Active codes" icon={<Ticket />} value={<span className="k-num">{active}</span>} chip={`${codes.length} total · ${codes.filter((c) => c.status === "scheduled").length} scheduled`} />
        <KpiCard label="Redemptions · 30d" icon={<Users />} value={<span className="k-num">{k.redemptions30d.toLocaleString("en-US")}</span>} chip="+18.2% vs prior 30d" chipTone="up" delay={0.05} />
        <KpiCard label="Attributed deposits" icon={<Wallet />} value={<Money value={k.attributedDeposits} decimals={0} />} chip={`${k.redemptionToFtd}% redemption → FTD`} chipTone="gold" delay={0.1} />
        <KpiCard label="Return on promo cost" icon={<TrendingUp />} value={<span className="k-num">{roi.toFixed(1)}×</span>} hot illustration="money_bag" footer={<Chip tone="up">${fmtK(k.cost)} cost · 30d</Chip>} delay={0.15} />
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal delay={0.05} className="xl:col-span-8">
          <Card className="h-full">
            <CardHeader
              title="Redemption stats"
              subtitle="Daily redemptions by benefit type · last 30 days · GMT+3"
              action={
                <div className="flex flex-wrap items-center gap-3 text-[12px] text-fg-2">
                  {byType.map((t) => (
                    <span key={t.type} className="flex items-center gap-1.5">
                      <span className="size-2 rounded-full" style={{ background: t.color }} />
                      {PROMO_TYPE_META[t.type].short}
                    </span>
                  ))}
                </div>
              }
            />
            <div className="px-4 pt-5 sm:px-6">
              <StackedBars
                data={PROMO_DAILY}
                series={[
                  { key: "bonus", label: "Deposit bonus", color: TYPE_COLOR["deposit-bonus"] },
                  { key: "fee", label: "Fee waiver", color: TYPE_COLOR["fee-waiver"] },
                  { key: "prop", label: "Prop retry", color: TYPE_COLOR["prop-retry"] },
                ]}
                height={262}
                labelEvery={5}
              />
            </div>
            <div className="grid grid-cols-2 gap-2 px-4 pb-5 pt-5 sm:grid-cols-4 sm:px-6">
              <MiniStat label="Avg / day" value={Math.round(k.redemptions30d / 30).toLocaleString("en-US")} />
              <MiniStat label="Redemption → FTD" value={`${k.redemptionToFtd}%`} tone="up" />
              <MiniStat label="Avg bonus credit" value="$186" tone="gold" />
              <MiniStat label="Abuse blocked" value={k.abuseBlocked} tone="down" />
            </div>
          </Card>
        </Reveal>

        <Reveal delay={0.1} className="xl:col-span-4">
          <Card className="flex h-full flex-col">
            <CardHeader title="By benefit type" subtitle="Lifetime redemptions" />
            <div className="flex items-center gap-5 px-6 pt-5">
              <Donut
                data={byType.map((t) => ({ label: t.label, value: t.uses, color: t.color }))}
                size={124}
                thickness={14}
                center={
                  <div>
                    <div className="k-num text-[16px] font-semibold">{fmtK(totalUses)}</div>
                    <div className="text-[10.5px] text-fg-3">redeemed</div>
                  </div>
                }
              />
              <div className="min-w-0 flex-1 space-y-2.5">
                {byType.map((t) => (
                  <div key={t.type} className="text-[12px]">
                    <div className="flex items-center gap-2">
                      <span className="size-2 shrink-0 rounded-full" style={{ background: t.color }} />
                      <span className="min-w-0 flex-1 truncate text-fg-2">{PROMO_TYPE_META[t.type].short}</span>
                      <span className="k-num text-fg">{((t.uses / totalUses) * 100).toFixed(0)}%</span>
                    </div>
                    <div className="k-num pl-4 text-[11px] text-fg-3">${fmtK(t.deposits)} deposits</div>
                  </div>
                ))}
              </div>
            </div>
            <div className="mt-5 flex-1 border-t border-line px-4 pb-5 pt-4 sm:px-6">
              <div className="k-label mb-2.5">Top codes by deposits</div>
              <div className="space-y-1.5">
                {top.map((c, i) => (
                  <div key={c.id} className="k-row flex items-center gap-3 px-3 py-2">
                    <span className="k-num w-4 text-[11.5px] text-fg-3">{i + 1}</span>
                    <span className="min-w-0 flex-1 truncate font-mono text-[12.5px] font-medium">{c.code}</span>
                    <Sparkline data={c.daily.some((v) => v > 0) ? c.daily : [1, 1.2, 1.1, 1.3, 1.2, 1.4]} width={56} height={20} tone="gold" fill={false} />
                    <Money value={c.deposits} decimals={0} countUp={false} className="w-20 text-right text-[12.5px]" />
                  </div>
                ))}
              </div>
            </div>
          </Card>
        </Reveal>
      </div>

      <Reveal delay={0.1} className="mt-4">
        <Card>
          <CardHeader title="All codes" subtitle="Click a row for details · actions in the ⋯ menu" />
          <div className="mt-4 px-4 pb-5 sm:px-6">
            <DataTable
              columns={cols}
              rows={rows}
              pageSize={8}
              rowKey={(c) => c.id}
              search={(c) => `${c.code} ${c.segment} ${PROMO_TYPE_META[c.type].label}`}
              searchPlaceholder="Search code or segment…"
              exportName="promo-codes"
              onRowClick={(c) => toast.info(`${c.code} · ${PROMO_TYPE_META[c.type].label}`, { description: `${valueText(c)} ${c.cap} · ${c.uses.toLocaleString("en-US")} of ${c.limit.toLocaleString("en-US")} used · min deposit $${c.minDeposit}` })}
              toolbar={
                <Segmented
                  size="xs"
                  value={filter}
                  onChange={setFilter}
                  options={[
                    { value: "all", label: <>All <span className="text-fg-3">{codes.length}</span></> },
                    { value: "active", label: "Active" },
                    { value: "scheduled", label: "Scheduled" },
                    { value: "paused", label: "Paused" },
                    { value: "ended", label: "Ended" },
                  ]}
                />
              }
            />
          </div>
        </Card>
      </Reveal>

      <Reveal delay={0.1} className="mt-4">
        <Card>
          <CardHeader
            title="Live redemptions"
            subtitle="Most recent first · blocked attempts are held for Risk review"
            action={
              <Button size="sm" variant="surface" onClick={() => toast.success("Export started", { description: "promo-redemptions-2026-09-24.csv" })}>
                <Download /> Export
              </Button>
            }
          />
          <div className="grid grid-cols-1 gap-2 px-4 pb-5 pt-4 sm:px-6 md:grid-cols-2">
            {PROMO_REDEMPTIONS.slice(0, 10).map((r) => (
              <div key={r.id} className={cn("k-row flex items-center gap-3 px-3.5 py-2.5", r.status === "blocked" && "border-down/25")}>
                <Avatar src={r.person.photo} name={r.person.name} size={34} />
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-1.5 text-[13px] font-medium">
                    <span className="truncate">{r.person.name}</span>
                    <Flag country={r.person.country} className="size-3.5" />
                  </div>
                  <div className="truncate text-[11.5px] text-fg-3">
                    <span className="font-mono">{r.code}</span> · <span className="font-mono">{r.login}</span> · {fmtDateTime(r.at)}
                  </div>
                </div>
                <div className="shrink-0 text-right">
                  <div className="k-num text-[12.5px] text-fg">{r.benefit}</div>
                  {r.status === "blocked" ? (
                    <button
                      type="button"
                      onClick={() => toast.warning(`${r.id} blocked`, { description: r.reason })}
                      className="inline-flex items-center gap-1 text-[11px] text-down hover:underline"
                    >
                      <Ban className="size-3" /> Blocked
                    </button>
                  ) : r.status === "pending" ? (
                    <span className="text-[11px] text-warn">Awaiting deposit</span>
                  ) : (
                    <span className="k-num text-[11px] text-fg-3">{r.deposit ? `on $${r.deposit.toLocaleString("en-US")}` : "applied"}</span>
                  )}
                </div>
              </div>
            ))}
          </div>
        </Card>
      </Reveal>

      <PromoCreateDialog open={open} onOpenChange={setOpen} onCreate={(c) => setCodes((cs) => [c, ...cs])} />
    </div>
  );
}

export default function PromoCodesPage() {
  return IS_DEMO ? <DemoPromoCodesPage /> : <LivePromoCodes />;
}
