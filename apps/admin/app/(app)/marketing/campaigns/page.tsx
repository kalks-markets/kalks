"use client";

import * as React from "react";
import { BadgeDollarSign, MoreHorizontal, MousePointerClick, Pause, Play, Plug, Plus, Target, TrendingUp, UserPlus } from "lucide-react";
import { toast } from "sonner";
import {
  Button,
  Card,
  CardHeader,
  Chip,
  DataTable,
  Donut,
  IconButton,
  KpiCard,
  Menu,
  Money,
  PageHeader,
  Reveal,
  Segmented,
  StatusChip,
  type Column,
  cn,
} from "@kalks/ui";
import { UTM_CAMPAIGNS as MKT_CAMPAIGNS, UTM_SPEND_REVENUE as MKT_SPEND_REVENUE, type UtmCampaign as MktCampaign } from "@kalks/mock/admin-campaigns";
import { FunnelViz } from "@/components/analytics/funnel";
import { ComboChart } from "@/components/marketing/charts";
import { MiniStat, fmtK } from "@/components/marketing/kit";
import { UtmBuilderDialog } from "@/components/marketing/utm-builder-dialog";
import { IS_DEMO } from "@kalks/mock/mode";
import { LiveCampaigns } from "@/components/marketing/live/campaigns";

const SRC_COLOR: Record<string, string> = {
  google: "#ff5a1f",
  meta: "#e9b949",
  tiktok: "#f04438",
  youtube: "#ff8a3d",
  telegram: "#38bdf8",
  x: "#a1a1aa",
  affiliate: "#22c55e",
  newsletter: "#14b8a6",
  bing: "#f5d58a",
  ib_network: "#c9971f",
};
const SRC_LABEL: Record<string, string> = { google: "Google", meta: "Meta", tiktok: "TikTok", youtube: "YouTube", telegram: "Telegram", x: "X", affiliate: "Affiliates", newsletter: "Newsletter", bing: "Bing", ib_network: "IB network" };

const cpa = (c: MktCampaign) => (c.ftds && c.spend ? c.spend / c.ftds : 0);
const roi = (c: MktCampaign) => (c.spend ? ((c.revenue - c.spend) / c.spend) * 100 : null);

function SourceDot({ source }: { source: string }) {
  return (
    <span className="inline-flex items-center gap-2 whitespace-nowrap text-[13px] font-medium">
      <span className="grid size-6 place-items-center rounded-full text-[10px] font-semibold text-black" style={{ background: SRC_COLOR[source] ?? "#a1a1aa" }}>
        {(SRC_LABEL[source] ?? source)[0]}
      </span>
      {SRC_LABEL[source] ?? source}
    </span>
  );
}

/** Live workspaces read UTM attribution from the reports service; the demo showcase keeps the mock. */
export default function CampaignsPage() {
  if (!IS_DEMO) return <LiveCampaigns />;
  return <DemoCampaigns />;
}

function DemoCampaigns() {
  const [rows, setRows] = React.useState<MktCampaign[]>(MKT_CAMPAIGNS);
  const [src, setSrc] = React.useState<string>("all");
  const [metric, setMetric] = React.useState<"ftds" | "spend" | "deposits">("ftds");
  const [open, setOpen] = React.useState(false);

  const scoped = rows.filter((c) => src === "all" || c.source === src);
  const sum = (k: keyof MktCampaign, list = scoped) => list.reduce((s, c) => s + (c[k] as number), 0);
  const all = { spend: sum("spend", rows), signups: sum("signups", rows), ftds: sum("ftds", rows), revenue: sum("revenue", rows), deposits: sum("ftdAmount", rows), clicks: sum("clicks", rows) };
  const paidFtds = rows.filter((c) => c.spend > 0).reduce((s, c) => s + c.ftds, 0);

  const stages = [
    { key: "clicks", label: "Clicks", hint: "utm-tagged sessions", value: sum("clicks") },
    { key: "signups", label: "Sign-ups", hint: "registered", value: sum("signups") },
    { key: "kyc", label: "KYC approved", hint: "verified", value: sum("kyc") },
    { key: "ftds", label: "FTDs", hint: "first deposit", value: sum("ftds") },
    { key: "active", label: "Active traders", hint: "traded in 30d", value: sum("active") },
  ];

  const channels = Object.keys(SRC_LABEL)
    .map((s) => {
      const l = rows.filter((c) => c.source === s);
      return { source: s, ftds: sum("ftds", l), spend: sum("spend", l), deposits: sum("ftdAmount", l) };
    })
    .filter((c) => c[metric] > 0)
    .sort((a, b) => b[metric] - a[metric]);
  const chTotal = channels.reduce((s, c) => s + c[metric], 0);
  const fmtMetric = (v: number) => (metric === "ftds" ? v.toLocaleString("en-US") : `$${fmtK(v)}`);

  const toggle = (id: string) =>
    setRows((rs) =>
      rs.map((c) => {
        if (c.id !== id) return c;
        const next = c.status === "running" ? "paused" : "running";
        toast.success(`${c.campaign} ${next === "paused" ? "paused" : "resumed"}`, { description: next === "paused" ? "Ad spend stops syncing; tracked links keep attributing." : "Budget sync resumed" });
        return { ...c, status: next };
      }),
    );

  const cols: Column<MktCampaign>[] = [
    { key: "source", header: "Source", cell: (c) => <SourceDot source={c.source} />, sort: (c) => c.source },
    { key: "medium", header: "Medium", cell: (c) => <Chip size="sm">{c.medium}</Chip>, sort: (c) => c.medium, hideOn: "md" },
    { key: "campaign", header: "Campaign", cell: (c) => <span className="whitespace-nowrap font-mono text-[12.5px]">{c.campaign}</span>, sort: (c) => c.campaign },
    { key: "spend", header: "Spend", align: "right", cell: (c) => (c.spend ? <Money value={c.spend} decimals={0} countUp={false} className="text-[13px]" /> : <span className="text-fg-3">Organic</span>), sort: (c) => c.spend },
    { key: "clicks", header: "Clicks", align: "right", cell: (c) => <span className="k-num text-[13px]">{c.clicks.toLocaleString("en-US")}</span>, sort: (c) => c.clicks, hideOn: "lg" },
    { key: "signups", header: "Sign-ups", align: "right", cell: (c) => <span className="k-num text-[13px]">{c.signups.toLocaleString("en-US")}</span>, sort: (c) => c.signups },
    { key: "ftds", header: "FTDs", align: "right", cell: (c) => <span className="k-num text-[13px] font-medium">{c.ftds.toLocaleString("en-US")}</span>, sort: (c) => c.ftds },
    { key: "cpa", header: "CPA", align: "right", cell: (c) => (cpa(c) ? <span className={cn("k-num text-[13px]", cpa(c) > 400 ? "text-warn" : "text-fg")}>${Math.round(cpa(c)).toLocaleString("en-US")}</span> : <span className="text-fg-3">—</span>), sort: (c) => cpa(c) || 1e9 },
    { key: "deposits", header: "Deposits", align: "right", cell: (c) => <Money value={c.ftdAmount} decimals={0} countUp={false} className="text-[13px]" />, sort: (c) => c.ftdAmount, hideOn: "md" },
    {
      key: "roi",
      header: "ROI",
      align: "right",
      cell: (c) => {
        const r = roi(c);
        return r === null ? (
          <Chip size="sm" tone="gold">
            ∞
          </Chip>
        ) : (
          <Chip size="sm" tone={r >= 100 ? "up" : r >= 0 ? "gold" : "down"}>
            {r >= 0 ? "+" : ""}
            {Math.round(r)}%
          </Chip>
        );
      },
      sort: (c) => roi(c) ?? 1e9,
    },
    { key: "status", header: "Status", cell: (c) => <StatusChip status={c.status === "running" ? "running" : c.status === "paused" ? "paused" : "completed"} />, sort: (c) => c.status, hideOn: "lg" },
    {
      key: "act",
      header: "",
      align: "right",
      cell: (c) => (
        <div onClick={(e) => e.stopPropagation()}>
          <Menu
            trigger={
              <IconButton size="sm" aria-label={`Actions for ${c.campaign}`}>
                <MoreHorizontal />
              </IconButton>
            }
            items={[
              c.status === "completed"
                ? { label: "Duplicate campaign", icon: <Plus />, onSelect: () => toast.success(`${c.campaign}_v2 drafted`) }
                : { label: c.status === "running" ? "Pause" : "Resume", icon: c.status === "running" ? <Pause /> : <Play />, onSelect: () => toggle(c.id) },
              { label: "Copy tracked link", icon: <Plug />, onSelect: () => (navigator.clipboard?.writeText(`https://app.kalkstrade.com/register?utm_source=${c.source}&utm_medium=${c.medium}&utm_campaign=${c.campaign}`).catch(() => {}), toast.success("Tracked link copied")) },
              { label: "Open cohort in Analytics", icon: <TrendingUp />, href: "/analytics/cohorts" },
            ]}
          />
        </div>
      ),
    },
  ];

  const sources = ["all", ...Array.from(new Set(rows.map((c) => c.source)))];

  return (
    <div className="pb-16">
      <PageHeader
        title="Campaigns"
        subtitle="UTM-attributed acquisition: spend, sign-ups, first deposits and return on ad spend, per channel."
        actions={
          <>
            <Button variant="surface" onClick={() => toast.success("Ad accounts synced", { description: "Google Ads, Meta, TikTok · spend updated 4 min ago" })}>
              <Plug /> Sync ad spend
            </Button>
            <Button variant="ember" shimmer onClick={() => setOpen(true)}>
              <Plus /> New campaign
            </Button>
          </>
        }
      />

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <KpiCard label="Ad spend · 30d" icon={<BadgeDollarSign />} value={<Money value={all.spend} decimals={0} />} chip={`${rows.filter((c) => c.status === "running").length} running campaigns`} />
        <KpiCard label="Sign-ups" icon={<UserPlus />} value={<span className="k-num">{all.signups.toLocaleString("en-US")}</span>} chip={`${((all.signups / all.clicks) * 100).toFixed(1)}% of clicks`} chipTone="info" delay={0.05} />
        <KpiCard label="FTDs · blended CPA" icon={<Target />} value={<span className="k-num">{all.ftds.toLocaleString("en-US")}</span>} chip={`$${Math.round(all.spend / paidFtds)} CPA (paid)`} chipTone="gold" delay={0.1} />
        <KpiCard label="Return on ad spend" icon={<TrendingUp />} value={<span className="k-num">{(all.revenue / all.spend).toFixed(2)}×</span>} hot illustration="rocket" footer={<Chip tone="up">${fmtK(all.revenue)} net revenue</Chip>} delay={0.15} />
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal delay={0.05} className="min-w-0 xl:col-span-8">
          <Card className="h-full">
            <CardHeader title="Acquisition funnel" subtitle={src === "all" ? "All UTM sources · last 30 days" : `${SRC_LABEL[src]} · last 30 days`} icon={<MousePointerClick />} />
            <div className="-mx-1 mt-4 overflow-x-auto px-5 sm:px-6">
              <Segmented size="xs" value={src} onChange={setSrc} options={sources.map((s) => ({ value: s, label: s === "all" ? "All sources" : SRC_LABEL[s] ?? s }))} />
            </div>
            <div className="px-4 pb-4 pt-5 sm:px-6">
              <FunnelViz stages={stages} />
            </div>
            <div className="grid grid-cols-2 gap-2 px-4 pb-5 sm:grid-cols-4 sm:px-6">
              <MiniStat label="Click → sign-up" value={`${((stages[1]!.value / stages[0]!.value) * 100).toFixed(1)}%`} />
              <MiniStat label="Sign-up → FTD" value={`${((stages[3]!.value / stages[1]!.value) * 100).toFixed(1)}%`} tone="up" />
              <MiniStat label="Avg FTD" value={`$${Math.round(sum("ftdAmount") / Math.max(1, sum("ftds"))).toLocaleString("en-US")}`} tone="gold" />
              <MiniStat label="CPA" value={sum("spend") ? `$${Math.round(sum("spend") / Math.max(1, sum("ftds")))}` : "Organic"} />
            </div>
          </Card>
        </Reveal>

        <Reveal delay={0.1} className="xl:col-span-4">
          <Card className="flex h-full flex-col">
            <CardHeader
              title="Channel breakdown"
              subtitle="Share by source"
              action={
                <Segmented
                  size="xs"
                  value={metric}
                  onChange={setMetric}
                  options={[
                    { value: "ftds", label: "FTDs" },
                    { value: "spend", label: "Spend" },
                    { value: "deposits", label: "Deposits" },
                  ]}
                />
              }
            />
            <div className="flex justify-center pt-6">
              <Donut
                data={channels.map((c) => ({ label: c.source, value: c[metric], color: SRC_COLOR[c.source] }))}
                size={168}
                thickness={18}
                center={
                  <div>
                    <div className="k-num text-[20px] font-semibold">{fmtMetric(chTotal)}</div>
                    <div className="text-[11px] text-fg-3">{metric === "ftds" ? "first deposits" : metric === "spend" ? "ad spend" : "FTD volume"}</div>
                  </div>
                }
              />
            </div>
            <div className="mt-5 flex-1 space-y-1.5 px-4 pb-5 sm:px-6">
              {channels.map((c) => (
                <button
                  key={c.source}
                  type="button"
                  onClick={() => setSrc(src === c.source ? "all" : c.source)}
                  className={cn("flex w-full items-center gap-2.5 rounded-[12px] px-2.5 py-1.5 text-[12.5px] transition-colors hover:bg-surface-2", src === c.source && "bg-surface-2")}
                >
                  <span className="size-2 shrink-0 rounded-full" style={{ background: SRC_COLOR[c.source] }} />
                  <span className="flex-1 text-left text-fg-2">{SRC_LABEL[c.source]}</span>
                  <span className="k-num text-fg-3">{fmtMetric(c[metric])}</span>
                  <span className="k-num w-10 text-right font-medium text-fg">{((c[metric] / chTotal) * 100).toFixed(0)}%</span>
                </button>
              ))}
            </div>
          </Card>
        </Reveal>
      </div>

      <Reveal delay={0.1} className="mt-4">
        <Card>
          <CardHeader
            title="Spend vs attributed revenue"
            subtitle="Weekly · last 12 weeks"
            action={
              <div className="flex items-center gap-3 text-[12px] text-fg-2">
                <span className="flex items-center gap-1.5">
                  <span className="size-2 rounded-full bg-fg-3" /> Spend
                </span>
                <span className="flex items-center gap-1.5">
                  <span className="size-2 rounded-full bg-gold" /> Revenue
                </span>
              </div>
            }
          />
          <div className="px-4 pb-5 pt-4 sm:px-6">
            <ComboChart data={MKT_SPEND_REVENUE.map((d) => ({ label: d.label, a: d.spend, b: d.revenue }))} aLabel="Spend" bLabel="Revenue" mode="bar-line" height={220} format={(v) => `$${fmtK(Math.round(v))}`} />
          </div>
        </Card>
      </Reveal>

      <Reveal delay={0.1} className="mt-4">
        <Card>
          <CardHeader title="UTM campaigns" subtitle="source / medium / campaign · click a row to filter the funnel" />
          <div className="mt-4 px-4 pb-5 sm:px-6">
            <DataTable
              columns={cols}
              rows={rows}
              pageSize={12}
              dense
              rowKey={(c) => c.id}
              search={(c) => `${c.source} ${c.medium} ${c.campaign}`}
              searchPlaceholder="Search source, medium, campaign…"
              exportName="utm-campaigns"
              onRowClick={(c) => {
                setSrc(c.source);
                toast.info(`Funnel filtered to ${SRC_LABEL[c.source]}`);
              }}
            />
          </div>
        </Card>
      </Reveal>

      <UtmBuilderDialog
        open={open}
        onOpenChange={setOpen}
        onCreate={(v) =>
          setRows((rs) => [
            { id: `cmp_${Date.now()}`, source: v.source, medium: v.medium, campaign: v.campaign, spend: 0, clicks: 0, signups: 0, kyc: 0, ftds: 0, ftdAmount: 0, active: 0, revenue: 0, status: "running" },
            ...rs,
          ])
        }
      />
    </div>
  );
}
