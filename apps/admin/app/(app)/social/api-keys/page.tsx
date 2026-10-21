"use client";

import * as React from "react";
import { Activity, Download, Gauge, KeyRound, MoreHorizontal, Power, ShieldOff, Timer } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, CardHeader, Chip, DataTable, Flag, Icon3D, IconButton, KpiCard, Menu, PageHeader, Reveal, Segmented, Sparkline, Starfield, StatusChip, Toggle, cn, type Column } from "@kalks/ui";
import { API_KEYS, type ApiKey } from "@kalks/mock/admin-partners";
import { IS_DEMO } from "@kalks/mock/mode";
import { AlgoKeysPage } from "@/components/algo/admin";
import { ColumnChart, PersonCell, auditToast, useReason } from "@/components/config/kit";
import { ago, fmtInt } from "@/components/partners/common";
import { STATUS_LABEL } from "@/components/social/common";

function DemoApiKeys() {
  const [rows, setRows] = React.useState(API_KEYS);
  const [halted, setHalted] = React.useState(false);
  const [filter, setFilter] = React.useState<"all" | ApiKey["status"] | "anomaly">("all");
  const reason = useReason();
  const set = (k: ApiKey, patch: Partial<ApiKey>) => setRows((rs) => rs.map((r) => (r.id === k.id ? { ...r, ...patch } : r)));
  const view = rows.filter((k) => filter === "all" || (filter === "anomaly" ? !!k.anomaly : k.status === filter));
  const live = rows.filter((k) => k.status !== "killed");
  const rpm = halted ? 0 : live.reduce((s, k) => s + k.rpm, 0);
  const agg = Array.from({ length: 30 }, (_, i) => ({ label: `${String(14).padStart(2, "0")}:${String(2 + i).padStart(2, "0")}`, values: [live.reduce((s, k) => s + (k.anomaly ? 0 : k.usage[i]!), 0), live.reduce((s, k) => s + (k.anomaly ? k.usage[i]! : 0), 0)] }));

  const toggleKey = (k: ApiKey) => {
    if (k.status === "killed")
      return reason.ask({ title: `Re-enable ${k.label}`, description: `${k.prefix}•••• · ${k.owner}`, reasons: ["Owner confirmed activity", "Investigation closed", "Rotated credentials"], confirmLabel: "Re-enable", tone: "buy", onConfirm: (r) => { set(k, { status: "active", rpm: Math.round(k.limit * 0.1) }); auditToast(`${k.label} re-enabled`, r); } });
    reason.ask({
      title: `Kill key ${k.prefix}••••`,
      description: `Revokes ${k.label} (${k.owner}) immediately. ${k.openPositions} open positions stay open unless closed from Trading.`,
      reasons: ["Abusive order rate", "Suspected compromise", "Latency arbitrage", "Owner request", "Terms of service breach"],
      confirmLabel: "Kill key",
      tone: "sell",
      onConfirm: (r) => { set(k, { status: "killed", rpm: 0 }); auditToast(`API key ${k.prefix} killed`, r); },
    });
  };

  const cols: Column<ApiKey>[] = [
    { key: "k", header: "Key", sort: (k) => k.label, cell: (k) => <span><span className="block text-[13px] font-medium">{k.label}</span><span className="font-mono text-[11.5px] text-fg-3">{k.prefix}••••••••</span></span> },
    { key: "o", header: "Owner", sort: (k) => k.owner, cell: (k) => <PersonCell name={k.owner} photo={k.photo} size={26} sub={<span className="font-mono">{k.login}</span>} /> },
    { key: "sc", header: "Scopes", hideOn: "lg", cell: (k) => <span className="flex flex-wrap gap-1">{k.scopes.map((s) => <Chip key={s} size="sm" tone={s === "trade" ? "ember" : s.startsWith("withdraw") ? "neutral" : s === "transfer" ? "warn" : "neutral"}>{s}</Chip>)}</span> },
    {
      key: "u",
      header: "Req / min",
      sort: (k) => k.rpm,
      cell: (k) => (
        <span className="flex items-center gap-2.5">
          <Sparkline data={k.usage} width={64} height={22} tone={k.anomaly ? "down" : "gold"} />
          <span className="w-16">
            <span className={cn("k-num block text-[12.5px] font-medium", k.rpm / k.limit > 0.9 && "text-down")}>{halted || k.status === "killed" ? 0 : k.rpm}<span className="text-fg-3">/{k.limit}</span></span>
            <span className="mt-1 block h-1 overflow-hidden rounded-full bg-surface-3"><span className={cn("block h-full rounded-full", k.rpm / k.limit > 0.9 ? "bg-down" : "bg-ember")} style={{ width: `${Math.min(100, (k.rpm / k.limit) * 100)}%` }} /></span>
          </span>
        </span>
      ),
    },
    { key: "h", header: "429s · 24h", align: "right", sort: (k) => k.rateHits24h, cell: (k) => <span className={cn("k-num", k.rateHits24h > 100 ? "font-medium text-down" : "text-fg-2")}>{fmtInt(k.rateHits24h)}</span> },
    { key: "ord", header: "Orders 24h", align: "right", hideOn: "md", sort: (k) => k.orders24h, cell: (k) => <span className="k-num text-fg-2">{fmtInt(k.orders24h)}</span> },
    { key: "ip", header: "Last IP", hideOn: "md", cell: (k) => <span><span className="flex items-center gap-1.5 font-mono text-[12px]"><Flag country={k.ipCountry} className="size-3.5" />{k.lastIp}</span><span className="text-[11px] text-fg-3">{ago(k.lastSeen)}</span></span> },
    { key: "s", header: "Status", cell: (k) => <span className="flex flex-col items-start gap-1"><StatusChip status={k.status === "killed" ? "rejected" : k.status === "throttled" ? "pending" : "active"} label={STATUS_LABEL[k.status]} />{k.anomaly && <span className="max-w-36 truncate text-[10.5px] text-down">{k.anomaly}</span>}</span> },
    {
      key: "x",
      header: "Kill",
      align: "right",
      cell: (k) => (
        <span className="flex items-center justify-end gap-1.5" onClick={(e) => e.stopPropagation()}>
          <Toggle checked={k.status !== "killed"} onChange={() => toggleKey(k)} label={`Key ${k.label} enabled`} />
          <Menu
            trigger={<IconButton size="sm" aria-label="Key actions"><MoreHorizontal /></IconButton>}
            items={[
              { label: k.status === "throttled" ? "Remove throttle" : "Throttle to 25%", icon: <Timer />, onSelect: () => { set(k, { status: k.status === "throttled" ? "active" : "throttled", rpm: k.status === "throttled" ? k.rpm : Math.round(k.limit * 0.25) }); auditToast(`${k.label} ${k.status === "throttled" ? "unthrottled" : "throttled to 25%"}`); } },
              { label: "Raise limit ×2", icon: <Gauge />, onSelect: () => { set(k, { limit: k.limit * 2 }); auditToast(`${k.label} limit raised to ${k.limit * 2}/min`); } },
              { label: "View request log", icon: <Activity />, onSelect: () => toast.success(`Request log · ${k.prefix}`, { description: `${fmtInt(k.orders24h)} orders · ${fmtInt(k.rateHits24h)} rejected (429) in 24h` }) },
              "sep",
              { label: "Kill key", icon: <Power />, danger: true, onSelect: () => k.status !== "killed" && toggleKey(k) },
            ]}
          />
        </span>
      ),
    },
  ];

  return (
    <div className="pb-16">
      <PageHeader
        title="API keys & algo trading"
        subtitle="Live monitor of client API keys, rate limits and anomalies"
        actions={
          <>
            <Button variant="surface" onClick={() => toast.success("api-keys.csv exported", { description: `${rows.length} keys` })}>
              <Download /> Export
            </Button>
            {halted ? (
              <Button variant="up-outline" onClick={() => reason.ask({ title: "Resume algo trading", description: "API order endpoints reopen for all non-killed keys.", reasons: ["Incident resolved", "LP connectivity restored", "Risk sign-off"], confirmLabel: "Resume", tone: "buy", onConfirm: (r) => { setHalted(false); auditToast("Algo trading resumed", r); } })}>
                <Power /> Resume algo trading
              </Button>
            ) : (
              <Button
                variant="sell"
                onClick={() =>
                  reason.ask({
                    title: "Kill all algo trading",
                    description: `Rejects every API order on all servers immediately (${live.length} live keys, ${fmtInt(rpm)} req/min). Manual trading in terminals is unaffected.`,
                    reasons: ["Market-wide volatility event", "LP / feed outage", "Suspected coordinated abuse", "Regulatory instruction"],
                    confirmLabel: "Kill all algo trading",
                    tone: "sell",
                    body: (
                      <div className="flex items-center gap-3 rounded-[12px] border border-down/30 bg-down-soft px-3.5 py-3 text-[12.5px] text-fg">
                        <ShieldOff className="size-5 shrink-0 text-down" /> Global switch — requires a second approver within 15 minutes or it auto-reverts.
                      </div>
                    ),
                    onConfirm: (r) => { setHalted(true); auditToast("All algo trading halted", r); },
                  })
                }
              >
                <Power /> Kill all algo trading
              </Button>
            )}
          </>
        }
      />

      {halted && (
        <Reveal>
          <Card hot className="relative mb-4 overflow-hidden">
            <Starfield density={30} />
            <div className="relative flex flex-col gap-3 px-6 py-4 sm:flex-row sm:items-center">
              <Icon3D name="warning" size={44} />
              <div className="flex-1">
                <div className="text-[15px] font-medium">Algo trading is halted platform-wide</div>
                <div className="text-[12.5px] text-fg-2">All API order requests return 503 · halted 14:32 GMT+3 by you · awaiting second approver</div>
              </div>
              <Chip tone="down" dot>HALTED</Chip>
            </div>
          </Card>
        </Reveal>
      )}

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <KpiCard label="Live keys" icon={<KeyRound />} value={<span className="k-num">{live.length}</span>} chip={`${rows.length - live.length} killed · ${rows.filter((k) => k.status === "throttled").length} throttled`} />
        <KpiCard label="Requests / min" icon={<Activity />} value={<span className="k-num">{fmtInt(rpm)}</span>} chip={halted ? "Halted" : "Peak 7,412 today"} chipTone={halted ? "down" : "neutral"} delay={0.05} />
        <KpiCard label="Rate-limit hits · 24h" icon={<Gauge />} value={<span className="k-num">{fmtInt(rows.reduce((s, k) => s + k.rateHits24h, 0))}</span>} chip={`${rows.filter((k) => k.rateHits24h > 100).length} keys over 100`} chipTone="warn" delay={0.1} />
        <KpiCard label="Anomalies" value={<span className="k-num">{rows.filter((k) => k.anomaly).length}</span>} hot illustration="robot" chip="Order-to-trade, quote stuffing, geo" chipTone="down" delay={0.15} />
      </div>

      <Reveal delay={0.1}>
        <Card className="mt-4">
          <CardHeader title="API traffic · last 30 minutes" subtitle="Requests per minute across live keys" action={<Chip tone={halted ? "down" : "up"} dot>{halted ? "Halted" : "Live"}</Chip>} />
          <div className="px-4 pb-5 pt-4 sm:px-6">
            <ColumnChart data={agg} series={[{ label: "Normal", tone: "gold" }, { label: "Anomalous keys", tone: "down" }]} height={180} labelEvery={5} />
          </div>
        </Card>
      </Reveal>

      <Reveal delay={0.15}>
        <Card className="mt-4 p-4 sm:p-6">
          <DataTable
            columns={cols}
            rows={view}
            dense
            pageSize={10}
            rowKey={(k) => k.id}
            search={(k) => `${k.label} ${k.prefix} ${k.owner} ${k.login} ${k.lastIp}`}
            searchPlaceholder="Key, owner, IP…"
            toolbar={<Segmented size="xs" value={filter} onChange={setFilter} options={[{ value: "all", label: "All" }, { value: "anomaly", label: "Anomalies" }, { value: "active", label: "Active" }, { value: "throttled", label: "Throttled" }, { value: "killed", label: "Killed" }]} />}
          />
        </Card>
      </Reveal>
      {reason.node}
    </div>
  );
}

// Live builds: client API keys on the algo service (/api/algo/keys), the same list as Algo & API → API keys.
export default function ApiKeysPage() {
  return IS_DEMO ? <DemoApiKeys /> : <AlgoKeysPage />;
}
