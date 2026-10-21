"use client";

import * as React from "react";
import { Download, Globe2, UserPlus } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, CardHeader, DataTable, EmptyState, Money, PageHeader, Progress, Reveal, Segmented, StatusChip, type Column } from "@kalks/ui";
import { SUB_BROKERS, type SubBroker } from "@kalks/mock/admin-partners";
import { IS_DEMO } from "@kalks/mock/mode";
import { ColumnChart, MiniStat, PersonCell } from "@/components/config/kit";
import { fmtInt, fmtUsdK } from "@/components/partners/common";
import { AddSubBrokerDialog, Flags, SubBrokerCard, SubBrokerDrawer } from "@/components/partners/sub-broker-parts";

function DemoSubBrokers() {
  const [rows, setRows] = React.useState(SUB_BROKERS);
  const [sel, setSel] = React.useState<string | null>(null);
  const [add, setAdd] = React.useState(false);
  const [status, setStatus] = React.useState<"all" | SubBroker["status"]>("all");
  const top = [...rows].sort((a, b) => b.lotsMtd - a.lotsMtd).slice(0, 4);
  const view = rows.filter((r) => status === "all" || r.status === status);
  const s = rows.find((r) => r.id === sel) ?? null;
  const totals = rows.reduce((a, r) => ({ p: a.p + r.partners, lots: a.lots + r.lotsMtd, ov: a.ov + r.overrideEarned, dep: a.dep + r.netDeposits }), { p: 0, lots: 0, ov: 0, dep: 0 });

  const cols: Column<SubBroker>[] = [
    { key: "n", header: "Sub-broker", sort: (r) => r.name, cell: (r) => <PersonCell name={r.name} photo={r.photo} country={r.country} sub={<span className="font-mono">{r.id}</span>} /> },
    { key: "r", header: "Region", cell: (r) => <span className="flex items-center gap-2.5"><Flags countries={r.countries} max={4} /><span className="whitespace-nowrap text-[12.5px] text-fg-2">{r.region}</span></span> },
    { key: "p", header: "Partners", align: "right", sort: (r) => r.partners, cell: (r) => <span className="k-num"><span className="font-medium">{r.activePartners}</span><span className="text-fg-3"> / {r.partners}</span></span> },
    { key: "o", header: "Override", align: "right", sort: (r) => r.overridePct, cell: (r) => <span className="k-num font-medium text-gold">{r.overridePct}%</span> },
    { key: "l", header: "Lots MTD", align: "right", sort: (r) => r.lotsMtd, cell: (r) => <span className="k-num">{fmtInt(r.lotsMtd)}</span> },
    { key: "t", header: "Target", hideOn: "lg", width: "140px", sort: (r) => r.lotsMtd / r.target, cell: (r) => <span className="flex items-center gap-2"><Progress value={(r.lotsMtd / r.target) * 100} tone={r.lotsMtd >= r.target ? "up" : "ember"} className="w-16" /><span className="k-num text-[11.5px] text-fg-3">{Math.round((r.lotsMtd / r.target) * 100)}%</span></span> },
    { key: "e", header: "Override earned", align: "right", sort: (r) => r.overrideEarned, cell: (r) => <Money value={r.overrideEarned} countUp={false} className="font-medium" /> },
    { key: "d", header: "Net deposits", align: "right", hideOn: "md", sort: (r) => r.netDeposits, cell: (r) => <span className="k-num text-fg-2">{fmtUsdK(r.netDeposits)}</span> },
    { key: "s", header: "Status", cell: (r) => <StatusChip status={r.status} /> },
  ];

  return (
    <div className="pb-16">
      <PageHeader
        title="Sub-brokers"
        subtitle="Country managers with an override on their regional partner networks"
        actions={
          <>
            <Button variant="surface" onClick={() => toast.success("Territory map exported", { description: "38 countries · 10 managers · PDF" })}>
              <Globe2 /> Territory map
            </Button>
            <Button variant="surface" onClick={() => toast.success("sub-brokers.csv exported", { description: `${rows.length} rows` })}>
              <Download /> Export
            </Button>
            <Button variant="ember" onClick={() => setAdd(true)}>
              <UserPlus /> Add sub-broker
            </Button>
          </>
        }
      />

      <Reveal>
        <div className="mb-4 grid grid-cols-2 gap-3 lg:grid-cols-4">
          <MiniStat label="Sub-brokers" value={rows.length} sub={`${rows.filter((r) => r.status === "active").length} active · ${new Set(rows.flatMap((r) => r.countries)).size} countries`} />
          <MiniStat label="Partners managed" value={fmtInt(totals.p)} sub="Across all territories" />
          <MiniStat label="Network lots MTD" value={fmtInt(totals.lots)} sub="Sub-broker territories" />
          <MiniStat label="Override accrued" value={<Money value={totals.ov} countUp={false} />} sub={`Net deposits ${fmtUsdK(totals.dep)}`} tone="gold" />
        </div>
      </Reveal>

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        {top.map((s, i) => (
          <Reveal key={s.id} delay={0.05 + i * 0.05}>
            <SubBrokerCard s={s} onOpen={() => setSel(s.id)} />
          </Reveal>
        ))}
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4">
        <Reveal delay={0.1}>
          <Card className="h-full p-4 sm:p-6">
            <DataTable
              columns={cols}
              rows={view}
              dense
              pageSize={10}
              rowKey={(r) => r.id}
              onRowClick={(r) => setSel(r.id)}
              search={(r) => `${r.name} ${r.region} ${r.countries.join(" ")}`}
              searchPlaceholder="Name, region, country…"
              toolbar={<Segmented size="xs" value={status} onChange={setStatus} options={[{ value: "all", label: "All" }, { value: "active", label: "Active" }, { value: "review", label: "Review" }, { value: "paused", label: "Paused" }]} />}
            />
          </Card>
        </Reveal>
        <Reveal delay={0.15}>
          <Card className="h-full">
            <CardHeader title="Lots vs target" subtitle="September · by territory" />
            <div className="px-4 pb-5 pt-4 sm:px-6">
              <ColumnChart
                mode="group"
                height={220}
                data={rows.map((r) => ({ label: r.region, values: [r.lotsMtd, r.target] }))}
                series={[{ label: "Lots MTD", tone: "ember" }, { label: "Target", tone: "fg3" }]}
                format={(v) => (v >= 1000 ? `${(v / 1000).toFixed(0)}k` : v.toFixed(0))}
              />
            </div>
          </Card>
        </Reveal>
      </div>

      <SubBrokerDrawer s={s} open={!!s} onOpenChange={(o) => !o && setSel(null)} onChange={(n) => setRows((rs) => rs.map((r) => (r.id === n.id ? n : r)))} />
      <AddSubBrokerDialog open={add} onOpenChange={setAdd} />
    </div>
  );
}

// No backend yet: the IB service has partner uplines (sub-IBs, see Partners → Partners) but no regional sub-broker
// desks with override commissions. Live builds say so instead of showing the demo's sample sub-brokers.
function LiveSubBrokers() {
  return (
    <>
      <PageHeader title="Sub-brokers" subtitle="Regional desks that recruit and manage IBs for an override commission." />
      <Card>
        <EmptyState
          title="Not available yet"
          text="Sub-broker desks aren't connected to the partner service yet. Partner uplines and sub-IBs are on Partners → Partners."
        />
      </Card>
    </>
  );
}

export default function SubBrokersPage() {
  return IS_DEMO ? <DemoSubBrokers /> : <LiveSubBrokers />;
}
