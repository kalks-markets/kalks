"use client";

import * as React from "react";
import Link from "next/link";
import { ArrowUpRight, BarChart3, ChevronDown, Handshake, Play, RefreshCw, ShieldAlert, Users, Wallet } from "lucide-react";
import { toast } from "sonner";
import { Avatar, Button, Card, CardHeader, Chip, Donut, Flag, KpiCard, ListRow, Menu, Money, PageHeader, Reveal, Skeleton, cn } from "@kalks/ui";
import { ColumnChart, MiniStat } from "@/components/config/kit";
import { ago, day, useApi, useNow } from "@/components/live/kit";
import { useConfirm } from "@/components/confirm";
import { P, ibSend, type BatchesDoc, type FlagsDoc, type Level, type Overview } from "./api";
import { BATCH_STATUS, EmptyNote, FLAG_KIND, LevelChip, PartnersError, SEV_TONE, StatusPill, int, levelStyle, lots, monthLabel, usd, usdK, usePerms, useLevels } from "./kit";

/* ------------------------------------------------------------------ */
/* Run-now menu (ops)                                                   */
/* ------------------------------------------------------------------ */

const JOBS: { job: string; label: string; hint: string }[] = [
  { job: "sync", label: "Sync referral tree", hint: "from the gateway" },
  { job: "deals", label: "Poll closed deals", hint: "engine" },
  { job: "deposits", label: "Scan first deposits", hint: "CPA" },
  { job: "reversals", label: "Sweep reversals", hint: "clawbacks" },
  { job: "payouts", label: "Retry wallet transfers", hint: "approved" },
  { job: "levels", label: "Apply level changes", hint: "month so far · applies now" },
];

function jobSummary(job: string, d: Record<string, unknown>) {
  const v = (k: string) => Number(d[k] ?? 0);
  switch (job) {
    case "sync":
      return `${int(v("synced"))} members synced`;
    case "deals":
      return `${int(v("processed"))} deals processed`;
    case "reversals":
      return `${int(v("reversed"))} deals reversed`;
    case "payouts":
      return `${int(v("paid"))} credited · ${int(v("pending"))} still pending · ${int(v("failed"))} failed`;
    case "levels":
      return `${int(v("changed"))} level changes applied`;
    default:
      return "Done";
  }
}

function RunMenu({ onDone }: { onDone: () => void }) {
  const [busy, setBusy] = React.useState<string | null>(null);
  const [ask, confirmDialog] = useConfirm();
  const run = async (job: string, label: string) => {
    if (
      job === "levels" &&
      !(await ask({
        title: "Apply level changes now?",
        text: "Every partner's level is re-evaluated on this month's results so far and the changes are applied immediately: partners who qualify are promoted and, if demotion is allowed, others are demoted (locked levels stay as they are). New levels change commission rates from now on and are recorded in level history as this month's evaluation. This is not a preview and can't be undone from here.",
        confirm: "Apply level changes",
        tone: "danger",
      }))
    )
      return;
    setBusy(job);
    const r = await ibSend<Record<string, unknown>>(`run/${job}`, {});
    setBusy(null);
    if (!r.ok) {
      toast.error(`${label} failed`, { description: r.error.message });
      return;
    }
    toast.success(label, { description: jobSummary(job, r.data) });
    onDone();
  };
  return (
    <>
      {confirmDialog}
      <Menu
        width={260}
        header={<div className="text-[11.5px] text-fg-3">Runs a background job now instead of waiting for its schedule. Logged in the audit trail.</div>}
        trigger={
          <Button variant="ghost" disabled={!!busy}>
            <Play /> {busy ? "Running…" : "Run now"} <ChevronDown />
          </Button>
        }
        items={JOBS.map((j) => ({ label: j.label, hint: j.hint, onSelect: () => void run(j.job, j.label) }))}
      />
    </>
  );
}

/* ------------------------------------------------------------------ */
/* Cards                                                                */
/* ------------------------------------------------------------------ */

/** Last 12 calendar months ("YYYY-MM"), oldest first. */
function last12() {
  const d = new Date();
  const out: string[] = [];
  for (let i = 11; i >= 0; i--) {
    const x = new Date(Date.UTC(d.getUTCFullYear(), d.getUTCMonth() - i, 1));
    out.push(`${x.getUTCFullYear()}-${String(x.getUTCMonth() + 1).padStart(2, "0")}`);
  }
  return out;
}

const SERIES = [
  { label: "Tier 1", tone: "ember" },
  { label: "Tier 2", tone: "gold" },
  { label: "Tier 3+", tone: "fg3" },
  { label: "Sub-IB split", tone: "info" },
  { label: "Client rebate", tone: "up" },
  { label: "CPA", tone: "warn" },
] as const;

function CommissionsCard({ ov }: { ov: Overview }) {
  const months = last12();
  const byMonth = new Map(ov.months.map((m) => [m.month, m]));
  const data = months.map((m) => {
    const r = byMonth.get(m);
    return { label: monthLabel(m), values: r ? [r.tier1, r.tier2, r.tier3, r.split, r.rebate, r.cpa] : [0, 0, 0, 0, 0, 0] };
  });
  const cur = byMonth.get(months[11]!);
  const lotAmt = cur ? cur.tier1 + cur.tier2 + cur.tier3 + cur.split : 0;
  const any = ov.months.some((m) => m.tier1 || m.tier2 || m.tier3 || m.split || m.rebate || m.cpa || m.clawback);
  return (
    <Card className="h-full">
      <CardHeader title="Network commissions" subtitle="Accrued per month, last 12 months · void and rejected lines excluded" />
      <div className="grid grid-cols-2 gap-3 px-4 pt-4 sm:grid-cols-4 sm:px-6">
        <MiniStat label={`${monthLabel(months[11]!)} accrued`} value={usd(ov.kpis.commissionMonth)} sub={`${monthLabel(months[10]!)}: ${usd(ov.kpis.commissionPrevMonth)}`} />
        <MiniStat label="Network lots" value={lots(ov.kpis.lotsMonth)} sub="Qualified, this month" />
        <MiniStat label="Avg per lot" value={ov.kpis.lotsMonth > 0 ? usd(lotAmt / ov.kpis.lotsMonth) : "—"} sub="Tier + split lines" />
        <MiniStat label="Clawbacks" value={usd(cur?.clawback ?? 0)} sub="Reversed deals, this month" tone={(cur?.clawback ?? 0) < 0 ? "down" : undefined} />
      </div>
      <div className="px-4 pb-5 pt-5 sm:px-6">
        {any ? (
          <div className="overflow-x-auto">
            <ColumnChart data={data} series={[...SERIES]} height={240} format={usdK} />
          </div>
        ) : (
          <EmptyNote className="h-[240px]" title="No commission accrued yet" text="Lines appear here once referred clients close qualifying live trades or a CPA is earned." />
        )}
      </div>
    </Card>
  );
}

function PayoutsCard({ ov }: { ov: Overview }) {
  const { data } = useApi<BatchesDoc>(`${P("batches")}?status=pending_approval&limit=4`);
  const pending = data?.items ?? [];
  return (
    <Card className="flex h-full flex-col">
      <CardHeader title="Payouts" subtitle={data ? `${data.schedule[0]!.toUpperCase()}${data.schedule.slice(1)} batches · next close ${day(data.nextClose)}` : "Batches and wallet transfers"} action={<Link href="/partners/payouts"><Button size="sm" variant="surface">Batches</Button></Link>} />
      <div className="grid grid-cols-2 gap-2.5 px-4 pt-4 sm:px-6">
        <MiniStat label="Not yet batched" value={data ? usd(data.unbatched.amount) : "—"} sub={data ? `${int(data.unbatched.payees)} payee${data.unbatched.payees === 1 ? "" : "s"}, payable now` : undefined} />
        <MiniStat label="Approved, unpaid" value={usd(ov.kpis.approved)} sub={`${int(ov.kpis.transfersInFlight)} transfers in flight`} tone={ov.kpis.transfersInFlight ? "warn" : undefined} />
      </div>
      <div className="mt-4 flex-1 space-y-2 px-4 pb-5 sm:px-6">
        <div className="k-label mb-1">Awaiting approval</div>
        {!data ? (
          <Skeleton className="h-14 w-full" />
        ) : pending.length === 0 ? (
          <EmptyNote className="py-6" title="Nothing to approve" text={`The next batch is created when the period closes${data.unbatched.amount > 0 ? "" : " and there is something to pay"}.`} />
        ) : (
          pending.map((b) => (
            <ListRow key={b.id} href={`/partners/payouts?batch=${b.id}`} className="py-2.5">
              <div className="min-w-0 flex-1">
                <div className="font-mono text-[12.5px]">Batch #{b.id}</div>
                <div className="mt-0.5 truncate text-[11.5px] text-fg-3">
                  To {day(b.periodEnd)} · <span className="k-num">{int(b.payees)}</span> payees
                </div>
              </div>
              <div className="text-right">
                <Money value={b.total} countUp={false} className="text-[13px] font-medium" />
                <div className="mt-1">
                  <StatusPill map={BATCH_STATUS} status={b.status} />
                </div>
              </div>
            </ListRow>
          ))
        )}
      </div>
    </Card>
  );
}

function TopPartnersCard({ ov, levels }: { ov: Overview; levels: Level[] }) {
  return (
    <Card className="flex h-full flex-col">
      <CardHeader
        title={`Top partners · ${monthLabel(last12()[11]!)}`}
        subtitle="By commission earned this month (rebates excluded)"
        action={
          <Link href="/partners/list">
            <Button size="sm" variant="surface">All partners</Button>
          </Link>
        }
      />
      <div className="mt-4 flex-1 space-y-2 px-4 pb-5 sm:px-6">
        {ov.top.length === 0 ? (
          <EmptyNote className="min-h-48" title="No commission earned this month yet" text="Partners show up here as soon as their network generates commission." />
        ) : (
          ov.top.map((p, i) => (
            <ListRow key={p.id} href={`/partners/list?partner=${p.id}`} className="gap-3 py-2.5">
              <span className={cn("k-num w-6 shrink-0 text-center text-[13px] font-semibold", i === 0 ? "text-gold" : i < 3 ? "text-fg" : "text-fg-3")}>{i + 1}</span>
              <Avatar name={p.name} size={32} />
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5 text-[13.5px] font-medium">
                  <span className="truncate">{p.name}</span>
                  {p.country && <Flag country={p.country.toLowerCase()} className="size-3.5" />}
                </div>
                <div className="mt-0.5 flex items-center gap-2 text-[11.5px] text-fg-3">
                  <LevelChip level={p.level} levels={levels} />
                  <span className="k-num hidden sm:inline">{int(p.clients)} direct client{p.clients === 1 ? "" : "s"}</span>
                </div>
              </div>
              <div className="w-28 text-right">
                <Money value={p.commissionMonth} countUp={false} className="text-[14px] font-semibold" />
                <div className="text-[11px] text-fg-3">this month</div>
              </div>
            </ListRow>
          ))
        )}
      </div>
    </Card>
  );
}

function LevelsCard({ ov }: { ov: Overview }) {
  const total = ov.levels.reduce((s, l) => s + l.members, 0);
  const sorted = [...ov.levels].sort((a, b) => a.rank - b.rank);
  return (
    <Card className="flex h-full flex-col">
      <CardHeader title="Level distribution" subtitle={`${int(total)} members`} action={<Link href="/partners/levels"><Button size="sm" variant="surface">Levels</Button></Link>} />
      <div className="flex justify-center py-5">
        <Donut
          size={170}
          thickness={18}
          data={total ? sorted.map((l) => ({ label: l.name, value: l.members, color: levelStyle(l.key, sorted).color })) : [{ label: "None", value: 0 }]}
          center={
            <div>
              <div className="k-num text-[24px] font-semibold">{int(total)}</div>
              <div className="text-[11.5px] text-fg-3">members</div>
            </div>
          }
        />
      </div>
      <div className="flex-1 space-y-1 px-4 pb-5 sm:px-6">
        {[...sorted].reverse().map((l) => (
          <div key={l.key} className="flex items-center gap-3 rounded-[12px] px-2 py-1.5 text-[13px]">
            <span className="size-2.5 rounded-full" style={{ background: levelStyle(l.key, sorted).color }} />
            <span className="flex-1 text-fg-2">{l.name}</span>
            <span className="k-num w-12 text-right font-medium">{int(l.members)}</span>
            <span className="k-num w-14 text-right text-fg-3">{total ? `${((l.members / total) * 100).toFixed(1)}%` : "—"}</span>
          </div>
        ))}
      </div>
    </Card>
  );
}

function FunnelCard({ ov }: { ov: Overview }) {
  const f = ov.funnel;
  const steps = [
    { label: "Link clicks", value: f.clicks, hint: "unique" },
    { label: "Sign-ups", value: f.signups, hint: "referred" },
    { label: "KYC passed", value: f.kyc },
    { label: "First deposit", value: f.ftds },
    { label: "First trade", value: f.traders },
  ];
  const max = Math.max(1, ...steps.map((s) => s.value));
  const empty = steps.every((s) => s.value === 0);
  return (
    <Card className="h-full">
      <CardHeader title="Referral funnel" subtitle="Last 30 days · all partner links" action={f.signups > 0 ? <Chip tone="up">FTD {((f.ftds / f.signups) * 100).toFixed(1)}%</Chip> : undefined} />
      <div className="px-4 pb-6 pt-5 sm:px-6">
        {empty ? (
          <EmptyNote title="No referral activity in 30 days" text="Clicks on partner links and referred sign-ups show up here." />
        ) : (
          <div className="space-y-2.5">
            {steps.map((s, i) => {
              // clicks → sign-ups is step conversion; later steps are shares of referred sign-ups (not strictly sequential)
              const base = i === 0 ? null : i === 1 ? steps[0]!.value : steps[1]!.value;
              return (
                <div key={s.label} className="flex items-center gap-3">
                  <div className="w-24 shrink-0 text-[12.5px] text-fg-2 sm:w-28">{s.label}</div>
                  <div className="relative h-8 flex-1 overflow-hidden rounded-[10px] bg-surface-2">
                    <div className="absolute inset-y-0 left-0 rounded-[10px] bg-ember" style={{ width: `${(s.value / max) * 100}%`, opacity: 0.9 - i * 0.14 }} />
                    <span className={cn("k-num relative flex h-full items-center px-3 text-[12.5px] font-semibold", s.value / max > 0.15 ? "text-white" : "text-fg")}>{int(s.value)}</span>
                  </div>
                  <div className="k-num w-14 shrink-0 text-right text-[12px] text-fg-3">{base === null ? "" : base > 0 ? `${((s.value / base) * 100).toFixed(0)}%` : "—"}</div>
                </div>
              );
            })}
            <div className="pt-1 text-[11.5px] text-fg-3">Sign-ups as a share of clicks; later steps as a share of sign-ups. Counts are referred clients who joined in the last 30 days.</div>
          </div>
        )}
      </div>
    </Card>
  );
}

function FraudCard({ open }: { open: number }) {
  const now = useNow();
  const { data } = useApi<FlagsDoc>(`${P("flags")}?status=open&limit=5`);
  const items = data?.items ?? [];
  return (
    <Card className="flex h-full flex-col">
      <CardHeader title="Fraud watch" subtitle={`${int(open)} open flag${open === 1 ? "" : "s"}`} action={<Link href="/partners/fraud"><Button size="sm" variant="surface"><ShieldAlert /> Review</Button></Link>} />
      <div className="mt-4 flex-1 space-y-2 px-4 pb-5 sm:px-6">
        {!data ? (
          <Skeleton className="h-14 w-full" />
        ) : items.length === 0 ? (
          <EmptyNote className="h-full min-h-40" title="No open flags" text="Self-referral, wash-trading and short-trade checks run on every sign-up and closed deal." />
        ) : (
          items.map((f) => (
            <ListRow key={f.id} href={`/partners/fraud?flag=${f.id}`} className="relative overflow-hidden py-2.5 pl-5">
              <span className={cn("absolute inset-y-2 left-0 w-[3px] rounded-r-full", f.severity === "high" ? "bg-down" : f.severity === "medium" ? "bg-warn" : "bg-fg-3")} />
              <div className="min-w-0 flex-1">
                <div className="truncate text-[13px] font-medium">{FLAG_KIND[f.kind]?.label ?? f.kind}</div>
                <div className="truncate text-[11.5px] text-fg-3">
                  {f.client.name || `#${f.client.id}`} → {f.ib.name || (f.ib.id ? `#${f.ib.id}` : "no IB")} · {ago(f.createdAt, now)}
                </div>
              </div>
              <Chip size="sm" tone={SEV_TONE[f.severity]}>{f.severity}</Chip>
            </ListRow>
          ))
        )}
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Page                                                                 */
/* ------------------------------------------------------------------ */

export function LivePartnersOverview() {
  const perms = usePerms();
  const { data: ov, error, reload } = useApi<Overview>(P("overview"), { refreshMs: 60_000 });
  const { levels } = useLevels();
  const k = ov?.kpis;
  const change = k && k.commissionPrevMonth > 0 ? ((k.commissionMonth - k.commissionPrevMonth) / k.commissionPrevMonth) * 100 : null;

  return (
    <div className="pb-16">
      <PageHeader
        title="IB overview"
        subtitle="Referral network, commissions and payouts"
        actions={
          <>
            {perms.write && <RunMenu onDone={reload} />}
            <Button variant="surface" onClick={reload}>
              <RefreshCw /> Refresh
            </Button>
            <Link href="/partners/list">
              <Button variant="ember">
                Manage partners <ArrowUpRight />
              </Button>
            </Link>
          </>
        }
      />

      {error && !ov ? (
        <PartnersError error={error} onRetry={reload} />
      ) : !ov || !k ? (
        <div className="space-y-4">
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
            {[0, 1, 2, 3].map((i) => (
              <Skeleton key={i} className="h-40 w-full rounded-[20px]" />
            ))}
          </div>
          <Skeleton className="h-80 w-full rounded-[20px]" />
        </div>
      ) : (
        <>
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
            <KpiCard label="Members" icon={<Handshake />} value={<span className="k-num">{int(k.members)}</span>} chip={`${int(k.ibsWithClients)} IB${k.ibsWithClients === 1 ? "" : "s"} with clients`} href="/partners/list" />
            <KpiCard
              label="Commission this month"
              icon={<Wallet />}
              value={<Money value={k.commissionMonth} countUp={false} />}
              chip={change === null ? `Last month ${usd(k.commissionPrevMonth)}` : `${change >= 0 ? "+" : ""}${change.toFixed(1)}% vs last month`}
              chipTone={change === null ? "neutral" : change >= 0 ? "up" : "down"}
              href="/partners/commissions"
              delay={0.05}
            />
            <KpiCard label="Network lots this month" icon={<BarChart3 />} value={<span className="k-num">{lots(k.lotsMonth)}</span>} chip={`${int(k.earningIbs)} IB${k.earningIbs === 1 ? "" : "s"} earning`} delay={0.1} />
            <KpiCard
              label="Pending commission"
              icon={<Users />}
              value={<Money value={k.pending} countUp={false} />}
              footer={
                <div className="flex flex-wrap items-center gap-1.5">
                  <Chip size="sm" tone="info">Approved {usdK(k.approved)}</Chip>
                  <Chip size="sm" tone="up">Paid {usdK(k.paid)}</Chip>
                </div>
              }
              delay={0.15}
            />
          </div>

          <div className="mt-4 grid grid-cols-2 gap-3 lg:grid-cols-4">
            <Link href="/partners/fraud" className="block">
              <MiniStat label="Open fraud flags" value={int(k.openFlags)} sub="Review in Fraud flags" tone={k.openFlags ? "warn" : undefined} className="h-full hover:bg-surface-3/60" />
            </Link>
            <Link href="/partners/payouts" className="block">
              <MiniStat label="Batches to approve" value={int(k.pendingBatches)} sub="Payout batches" tone={k.pendingBatches ? "warn" : undefined} className="h-full hover:bg-surface-3/60" />
            </Link>
            <MiniStat label="Transfers in flight" value={int(k.transfersInFlight)} sub="Pending or failed wallet credits" tone={k.transfersInFlight ? "warn" : undefined} />
            <MiniStat label="Referred clients" value={int(k.referredClients)} sub="Members with an upline" />
          </div>

          <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
            <Reveal delay={0.05} className="min-w-0 xl:col-span-8">
              <CommissionsCard ov={ov} />
            </Reveal>
            <Reveal delay={0.1} className="min-w-0 xl:col-span-4">
              <PayoutsCard ov={ov} />
            </Reveal>
          </div>

          <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12 xl:items-start">
            <Reveal delay={0.05} className="min-w-0 xl:col-span-8">
              <TopPartnersCard ov={ov} levels={levels} />
            </Reveal>
            <Reveal delay={0.1} className="min-w-0 xl:col-span-4">
              <LevelsCard ov={ov} />
            </Reveal>
          </div>

          <div className="mt-4 grid grid-cols-1 gap-4 lg:grid-cols-2">
            <Reveal delay={0.05} className="min-w-0">
              <FunnelCard ov={ov} />
            </Reveal>
            <Reveal delay={0.1} className="min-w-0">
              <FraudCard open={k.openFlags} />
            </Reveal>
          </div>
        </>
      )}
    </div>
  );
}
