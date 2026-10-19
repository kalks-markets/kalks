"use client";

import * as React from "react";
import { BadgeCheck, Crown, FileDown, HandCoins, PhoneCall, Star, Target, Trophy, Wallet } from "lucide-react";
import { toast } from "sonner";
import { Avatar, Button, Card, CardHeader, Chip, DataTable, Dialog, DialogClose, Flag, Icon3D, KpiCard, Money, PageHeader, Progress, Reveal, Segmented, Sparkline, type Column, cn } from "@kalks/ui";
import { KPI_AGENTS, KPI_PERIOD_FACTOR, KPI_PERIOD_LABEL, KPI_PLANS, type KpiAgent, type KpiPeriod } from "@kalks/mock/admin-desks";

type DeskFilter = "all" | "sales" | "retention" | "support";
type Row = KpiAgent & { rank: number };

const planOf = (id: string) => KPI_PLANS.find((p) => p.id === id)!;
const fmtUsd = (v: number) => (v >= 1e6 ? `$${(v / 1e6).toFixed(2)}M` : v >= 1e4 ? `$${(v / 1e3).toFixed(1)}K` : `$${Math.round(v).toLocaleString("en-US")}`);

export default function KpisPage() {
  const [period, setPeriod] = React.useState<KpiPeriod>("mtd");
  const [desk, setDesk] = React.useState<DeskFilter>("all");
  const [plans, setPlans] = React.useState<Record<string, string>>(() => Object.fromEntries(KPI_AGENTS.map((a) => [a.id, a.planId])));
  const [open, setOpen] = React.useState<Row | null>(null);
  const f = KPI_PERIOD_FACTOR[period];

  const scaled: Row[] = KPI_AGENTS.filter((a) => desk === "all" || (desk === "sales" ? a.desk.startsWith("sales") : a.desk === desk))
    .map((a) => ({
      ...a,
      planId: plans[a.id]!,
      calls: Math.round(a.calls * f),
      ftds: Math.round(a.ftds * f),
      netDeposits: Math.round(a.netDeposits * f),
      commission: +(a.commission * f).toFixed(2),
      rank: 0,
    }))
    .sort((a, b) => b.netDeposits - a.netDeposits)
    .map((a, i) => ({ ...a, rank: i + 1 }));

  const tot = {
    calls: scaled.reduce((s, a) => s + a.calls, 0),
    ftds: scaled.reduce((s, a) => s + a.ftds, 0),
    nda: scaled.reduce((s, a) => s + a.netDeposits, 0),
    comm: scaled.reduce((s, a) => s + a.commission, 0),
    csat: scaled.reduce((s, a) => s + a.csat, 0) / Math.max(1, scaled.length),
  };
  const podium = scaled.slice(0, 3);

  const cols: Column<Row>[] = [
    { key: "rank", header: "#", width: "44px", cell: (a) => <span className={cn("k-num text-[13px]", a.rank <= 3 ? "font-semibold text-gold" : "text-fg-3")}>{a.rank}</span>, sort: (a) => a.rank },
    {
      key: "agent",
      header: "Agent",
      cell: (a) => (
        <div className="flex items-center gap-3">
          <Avatar src={a.person.photo} name={a.person.name} size={34} />
          <div className="min-w-0">
            <div className="flex items-center gap-1.5 whitespace-nowrap text-[13.5px] font-medium">
              {a.person.name} <Flag country={a.person.country} className="size-3.5" />
            </div>
            <div className="text-[11.5px] text-fg-3">{a.deskName}</div>
          </div>
        </div>
      ),
      sort: (a) => a.person.name,
    },
    { key: "calls", header: "Calls", align: "right", cell: (a) => <span className="k-num text-[13px]">{a.calls.toLocaleString("en-US")}</span>, sort: (a) => a.calls },
    { key: "ftds", header: "FTDs", align: "right", cell: (a) => <span className="k-num text-[13px] font-medium">{a.ftds}</span>, sort: (a) => a.ftds },
    { key: "conv", header: "Conv.", align: "right", cell: (a) => <span className="k-num text-[13px]">{a.conversion}%</span>, sort: (a) => a.conversion, hideOn: "md" },
    { key: "nda", header: "Net deposits", align: "right", cell: (a) => <Money value={a.netDeposits} decimals={0} countUp={false} className="text-[13px]" />, sort: (a) => a.netDeposits },
    { key: "ret", header: "Retention", align: "right", cell: (a) => <span className={cn("k-num text-[13px]", a.retention >= 70 ? "text-up" : "text-fg")}>{a.retention}%</span>, sort: (a) => a.retention, hideOn: "lg" },
    {
      key: "csat",
      header: "CSAT",
      align: "right",
      cell: (a) => (
        <span className={cn("k-num inline-flex items-center gap-1 text-[13px]", a.csat >= 4.7 ? "text-gold" : "text-fg")}>
          <Star className="size-3 fill-current" />
          {a.csat.toFixed(2)}
        </span>
      ),
      sort: (a) => a.csat,
      hideOn: "lg",
    },
    {
      key: "target",
      header: "Target",
      width: "130px",
      cell: (a) => (
        <div className="min-w-[100px]">
          <div className="k-num mb-1 text-right text-[11.5px] text-fg-2">{a.target}%</div>
          <Progress value={a.target} tone={a.target >= 100 ? "up" : a.target >= 80 ? "gold" : "warn"} />
        </div>
      ),
      sort: (a) => a.target,
      hideOn: "md",
    },
    { key: "comm", header: "Commission", align: "right", cell: (a) => <Money value={a.commission} countUp={false} className="text-[13.5px] font-medium text-gold" />, sort: (a) => a.commission },
  ];

  return (
    <div className="pb-16">
      <PageHeader
        title="KPIs & commissions"
        subtitle="Staff performance and variable pay. Commissions accrue daily and are approved by Finance at month end."
        actions={
          <>
            <Button variant="surface" onClick={() => toast.success("Payroll export ready", { description: `staff-commissions-${period}-2026-09.csv · ${scaled.length} agents` })}>
              <FileDown /> Export payroll
            </Button>
            <Button variant="ember" shimmer onClick={() => toast.success("Commissions sent for approval", { description: `${fmtUsd(tot.comm)} across ${scaled.length} agents · Finance four-eyes approval required` })}>
              <BadgeCheck /> Submit for approval
            </Button>
          </>
        }
      />

      <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
        <Segmented
          size="sm"
          value={period}
          onChange={setPeriod}
          options={[
            { value: "today", label: "Today" },
            { value: "week", label: "This week" },
            { value: "mtd", label: "Month" },
            { value: "qtd", label: "Quarter" },
          ]}
        />
        <Segmented
          size="sm"
          value={desk}
          onChange={setDesk}
          options={[
            { value: "all", label: "All desks" },
            { value: "sales", label: "Sales" },
            { value: "retention", label: "Retention" },
            { value: "support", label: "Support" },
          ]}
        />
      </div>

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <KpiCard label="Calls" icon={<PhoneCall />} value={<span className="k-num">{tot.calls.toLocaleString("en-US")}</span>} chip={KPI_PERIOD_LABEL[period]} />
        <KpiCard label="FTDs" icon={<Target />} value={<span className="k-num">{tot.ftds.toLocaleString("en-US")}</span>} chip={`${tot.calls ? ((tot.ftds / tot.calls) * 100).toFixed(1) : "0"}% of calls`} chipTone="up" delay={0.05} />
        <KpiCard label="Net deposits" icon={<Wallet />} value={<Money value={tot.nda} decimals={0} />} chip={`CSAT ${tot.csat.toFixed(2)} avg`} chipTone="gold" delay={0.1} />
        <KpiCard label="Commission earned" icon={<HandCoins />} value={<Money value={tot.comm} decimals={0} />} hot illustration="money_bag" footer={<Chip tone="up">{((tot.comm / Math.max(1, tot.nda)) * 100).toFixed(2)}% of net deposits</Chip>} delay={0.15} />
      </div>

      {/* podium */}
      <div className="mt-4 grid grid-cols-1 gap-4 md:grid-cols-3">
        {podium.map((a, i) => (
          <Reveal key={a.id} delay={0.05 * i}>
            <button type="button" onClick={() => setOpen(a)} className={cn("relative w-full overflow-hidden rounded-[20px] text-left", i === 0 ? "k-hot-card" : "k-card")}>
              <div className="flex items-center gap-4 p-5">
                <div className="relative">
                  <Avatar src={a.person.photo} name={a.person.name} size={64} className={cn("rounded-full", i === 0 && "ring-2 ring-gold/70 ring-offset-2 ring-offset-transparent")} />
                  <span className={cn("absolute -bottom-1 -right-1 grid size-6 place-items-center rounded-full text-[11px] font-bold ring-2 ring-surface", i === 0 ? "bg-gold text-[#1a1204]" : i === 1 ? "bg-fg-2 text-bg" : "bg-[#c47a3a] text-white")}>{i + 1}</span>
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-1.5">
                    {i === 0 && <Crown className="size-4 text-gold" />}
                    <span className="truncate text-[15px] font-medium">{a.person.name}</span>
                  </div>
                  <div className="text-[12px] text-fg-3">{a.deskName}</div>
                  <div className="mt-2 flex items-baseline gap-3">
                    <Money value={a.netDeposits} decimals={0} className="text-[20px] font-semibold" />
                    <span className="k-num text-[12px] text-fg-3">{a.ftds} FTDs</span>
                  </div>
                </div>
                <Icon3D name={i === 0 ? "trophy" : i === 1 ? "gem_stone" : "coin"} size={52} className="opacity-90" />
              </div>
              <div className="flex items-center justify-between border-t border-line bg-black/15 px-5 py-2.5 text-[12px]">
                <span className="text-fg-3">Commission</span>
                <Money value={a.commission} countUp={false} className="font-medium text-gold" />
              </div>
            </button>
          </Reveal>
        ))}
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4">
        <Reveal delay={0.1} className="min-w-0">
          <Card>
            <CardHeader title="Leaderboard" subtitle={`${KPI_PERIOD_LABEL[period]} · ranked by net deposits · click an agent for the breakdown`} icon={<Trophy />} />
            <div className="mt-4 px-4 pb-5 sm:px-6">
              <DataTable columns={cols} rows={scaled} pageSize={13} dense rowKey={(a) => a.id} search={(a) => `${a.person.name} ${a.deskName}`} searchPlaceholder="Search agent…" exportName="staff-kpis" onRowClick={setOpen} />
            </div>
          </Card>
        </Reveal>
        <Reveal delay={0.15}>
          <Card>
            <CardHeader title="Commission plans" subtitle="Assigned per agent" icon={<HandCoins />} />
            <div className="grid grid-cols-1 gap-2 px-4 pb-5 pt-4 sm:grid-cols-2 sm:px-6 xl:grid-cols-4">
              {KPI_PLANS.map((p) => {
                const agents = scaled.filter((a) => a.planId === p.id);
                return (
                  <div key={p.id} className="k-row px-4 py-3">
                    <div className="flex items-center justify-between gap-2">
                      <Chip size="sm" tone={p.tone}>
                        {p.name}
                      </Chip>
                      <span className="k-num text-[11.5px] text-fg-3">{agents.length} agents</span>
                    </div>
                    <ul className="mt-2 space-y-0.5 text-[12px] text-fg-2">
                      {p.rules.map((r) => (
                        <li key={r} className="flex gap-1.5">
                          <span className="text-ember">·</span>
                          {r}
                        </li>
                      ))}
                    </ul>
                    <div className="mt-2 flex justify-between border-t border-line pt-2 text-[11.5px]">
                      <span className="text-fg-3">Cap {p.cap}</span>
                      <span className="k-num text-gold">{fmtUsd(agents.reduce((s, a) => s + a.commission, 0))}</span>
                    </div>
                  </div>
                );
              })}
            </div>
          </Card>
        </Reveal>
      </div>

      {/* agent drawer */}
      <Dialog
        open={!!open}
        onOpenChange={(o) => !o && setOpen(null)}
        side="right"
        title={open?.person.name ?? ""}
        description={open ? `${open.deskName} · rank #${open.rank} · ${KPI_PERIOD_LABEL[period]}` : undefined}
        footer={
          <>
            <DialogClose asChild>
              <Button size="sm" variant="ghost">
                Close
              </Button>
            </DialogClose>
            <Button
              size="sm"
              variant="ember"
              onClick={() => {
                if (!open) return;
                toast.success(`Plan saved for ${open.person.name}`, { description: `${planOf(plans[open.id]!).name} · effective 01 Oct 2026` });
                setOpen(null);
              }}
            >
              Save plan
            </Button>
          </>
        }
      >
        {open && (
          <div className="space-y-5">
            <div className="flex items-center gap-4">
              <Avatar src={open.person.photo} name={open.person.name} size={60} />
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5 text-[16px] font-medium">
                  {open.person.name} <Flag country={open.person.country} className="size-4" />
                </div>
                <div className="text-[12.5px] text-fg-3">{open.person.email.replace(/@.*/, "@kalkstrade.com")}</div>
              </div>
              <Sparkline data={open.trend} width={96} height={36} tone="gold" />
            </div>
            <div className="grid grid-cols-3 gap-2">
              {[
                ["Calls", open.calls.toLocaleString("en-US")],
                ["FTDs", String(open.ftds)],
                ["Conversion", `${open.conversion}%`],
                ["Net deposits", fmtUsd(open.netDeposits)],
                ["Retention", `${open.retention}%`],
                ["CSAT", open.csat.toFixed(2)],
              ].map(([l, v]) => (
                <div key={l} className="k-row px-3 py-2.5">
                  <div className="text-[10.5px] uppercase tracking-wider text-fg-3">{l}</div>
                  <div className="k-num mt-1 text-[15px] font-medium">{v}</div>
                </div>
              ))}
            </div>
            <div>
              <div className="mb-1.5 flex justify-between text-[12.5px]">
                <span className="text-fg-2">Monthly target</span>
                <span className="k-num">{open.target}%</span>
              </div>
              <Progress value={open.target} tone={open.target >= 100 ? "up" : "gold"} />
            </div>
            <div>
              <div className="k-label mb-2">Commission plan</div>
              <div className="space-y-1.5">
                {KPI_PLANS.map((p) => {
                  const on = plans[open.id] === p.id;
                  return (
                    <button key={p.id} type="button" onClick={() => setPlans((m) => ({ ...m, [open.id]: p.id }))} className={cn("flex w-full items-center justify-between gap-3 rounded-[14px] border px-3.5 py-2.5 text-left", on ? "border-ember/40 bg-ember-soft" : "border-line bg-surface-2 hover:bg-surface-3")}>
                      <span>
                        <span className="block text-[13px] font-medium">{p.name}</span>
                        <span className="block text-[11.5px] text-fg-3">{p.rules[0]}</span>
                      </span>
                      <span className={cn("size-4 shrink-0 rounded-full border-2", on ? "border-ember bg-ember shadow-[inset_0_0_0_2px_var(--k-surface)]" : "border-line")} />
                    </button>
                  );
                })}
              </div>
            </div>
            <div className="k-row px-4 py-3">
              <div className="k-label mb-2">Earned · {KPI_PERIOD_LABEL[period]}</div>
              {plans[open.id]!.startsWith("plan_sales") ? (
                <>
                  <Line l={`${open.ftds} FTDs × $${plans[open.id]! === "plan_sales" ? 40 : 30}`} v={open.ftds * (plans[open.id]! === "plan_sales" ? 40 : 30)} />
                  <Line l="Net deposit share" v={Math.max(0, open.commission - open.ftds * (plans[open.id]! === "plan_sales" ? 40 : 30))} />
                </>
              ) : plans[open.id]! === "plan_ret" ? (
                <>
                  <Line l={`${open.ftds} reactivations × $60`} v={open.ftds * 60} />
                  <Line l="Redeposit share" v={Math.max(0, open.commission - open.ftds * 60)} />
                </>
              ) : (
                <Line l="CSAT bonus + ticket volume" v={open.commission} />
              )}
              <div className="mt-1 text-[11px] text-fg-3">Cap {planOf(plans[open.id]!).cap} applied</div>
              <div className="mt-2 flex justify-between border-t border-line pt-2 text-[13.5px] font-medium">
                <span>Total</span>
                <Money value={open.commission} countUp={false} className="text-gold" />
              </div>
            </div>
          </div>
        )}
      </Dialog>
    </div>
  );
}

function Line({ l, v }: { l: string; v: number }) {
  return (
    <div className="flex justify-between py-0.5 text-[12.5px]">
      <span className="text-fg-2">{l}</span>
      <Money value={v} countUp={false} />
    </div>
  );
}
