"use client";

import * as React from "react";
import { Ban, CandlestickChart, Gauge, Plus, ScrollText, ShieldAlert, Timer, UserCog } from "lucide-react";
import { Button, Card, KpiCard, PageHeader, Reveal, Tabs, useQuotes } from "@kalks/ui";
import { priceFeed } from "@kalks/mock";
import { accountMetrics, useDesk } from "@/lib/trading-desk";
import { CreateTradeDrawer } from "@/components/trading-desk/create-trade";
import { AccountControls, SymbolControls, TenantDelayCard } from "@/components/trading-desk/controls";
import { MarginMonitor } from "@/components/trading-desk/monitor";
import { AuditTrail } from "@/components/trading-desk/audit";
import { DeskStatusChip } from "@/components/trading-desk/status";
import { useCan } from "@/components/staff-session";
import { RiskMonitor } from "@/components/trading-desk/risk-monitor";

type Tab = "accounts" | "symbols" | "margin" | "audit";
const TABS: Tab[] = ["accounts", "symbols", "margin", "audit"];

export default function DealerDeskPage() {
  const { state } = useDesk();
  const canDeal = useCan("dealing.write");
  const [tab, setTab] = React.useState<Tab>("accounts");
  const [trade, setTrade] = React.useState(false);
  const [add, setAdd] = React.useState(false);

  React.useEffect(() => {
    const t = new URLSearchParams(window.location.search).get("tab") as Tab | null;
    if (t && TABS.includes(t)) setTab(t);
  }, []);
  const choose = (t: Tab) => {
    setTab(t);
    const u = new URL(window.location.href);
    u.searchParams.set("tab", t);
    window.history.replaceState(null, "", u.toString());
  };

  const symbols = React.useMemo(() => Array.from(new Set(state.positions.map((p) => p.symbol))).sort(), [state.positions]);
  const qs = useQuotes(symbols);
  const quote = React.useCallback((s: string) => qs[s] ?? priceFeed().quote(s), [qs]);
  const atRisk = React.useMemo(() => {
    const logins = Array.from(new Set(state.positions.map((p) => p.login)));
    return logins.filter((l) => (accountMetrics(state, l, quote)?.level ?? Infinity) < state.tenant.marginCallPct).length;
  }, [state, quote]);
  const today = new Date().toISOString().slice(0, 10);
  const dealerToday = state.audit.filter((a) => a.at.startsWith(today) && a.action !== "trade.rejected").length;
  const halted = state.symbolControls.filter((c) => c.mode === "halt").length;

  return (
    <div className="pb-10">
      <PageHeader
        title="Dealer desk"
        subtitle={<span className="inline-flex flex-wrap items-center gap-2">Risk alerts, account and symbol controls, margin monitor and the dealing audit trail — all reason-coded. <DeskStatusChip /></span>}
        actions={
          canDeal && (
          <>
            <Button
              variant="surface"
              size="lg"
              onClick={() => {
                choose("accounts");
                setAdd(true);
              }}
            >
              <Plus /> Add account control
            </Button>
            <Button variant="ember" size="lg" onClick={() => setTrade(true)}>
              <CandlestickChart /> Create trade
            </Button>
          </>
          )
        }
      />
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
        <div className="grid grid-cols-2 gap-4 xl:col-span-7">
          <KpiCard label="Account controls" icon={<UserCog />} value={<span className="k-num">{state.accountControls.length}</span>} chip={`${state.accountControls.filter((r) => r.tradingDisabled).length} disabled · ${state.accountControls.filter((r) => r.closeOnly).length} close-only`} />
          <KpiCard label="Delayed execution" icon={<Timer />} value={<span className="k-num text-warn">{state.accountControls.filter((r) => r.execDelayMs > 0).length}</span>} chip={state.tenant.execDelayEnabled ? "Tenant policy: allowed" : "Tenant policy: blocked"} chipTone="warn" delay={0.04} />
          <KpiCard label="Symbol controls" icon={<Ban />} value={<span className="k-num">{state.symbolControls.length}</span>} chip={`${halted} halted · ${state.symbolControls.length - halted} close-only`} chipTone={halted ? "down" : undefined} delay={0.08} />
          <KpiCard label="Below margin call" icon={<ShieldAlert />} value={<span className="k-num text-down">{atRisk}</span>} chip={`${dealerToday} dealer actions today`} chipTone="ember" delay={0.12} />
        </div>
        <Reveal delay={0.1} className="xl:col-span-5">
          <TenantDelayCard />
        </Reveal>
      </div>
      <Reveal delay={0.11} className="mt-4">
        <RiskMonitor />
      </Reveal>
      <Reveal delay={0.12} className="mt-4">
        <Card className="px-4 py-5 sm:px-6">
          <Tabs
            className="mb-5"
            value={tab}
            onChange={choose}
            tabs={[
              { value: "accounts", label: <span className="inline-flex items-center gap-1.5"><UserCog className="size-3.5" /> Account controls</span>, count: state.accountControls.length },
              { value: "symbols", label: <span className="inline-flex items-center gap-1.5"><Ban className="size-3.5" /> Symbol controls</span>, count: state.symbolControls.length },
              { value: "margin", label: <span className="inline-flex items-center gap-1.5"><Gauge className="size-3.5" /> Margin monitor</span>, count: atRisk },
              { value: "audit", label: <span className="inline-flex items-center gap-1.5"><ScrollText className="size-3.5" /> Audit trail</span>, count: state.audit.length },
            ]}
          />
          {tab === "accounts" ? <AccountControls addOpen={add} onAddOpenChange={setAdd} /> : tab === "symbols" ? <SymbolControls /> : tab === "margin" ? <MarginMonitor /> : <AuditTrail />}
        </Card>
      </Reveal>
      <CreateTradeDrawer open={trade} onOpenChange={setTrade} />
    </div>
  );
}
