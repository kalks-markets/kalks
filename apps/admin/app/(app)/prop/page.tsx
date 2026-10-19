"use client";

import * as React from "react";
import Link from "next/link";
import { Activity, CircleDollarSign, Layers, Percent, Plus, Scale, Trophy } from "lucide-react";
import { Button, Chip, KpiCard, Money, PageHeader, Reveal } from "@kalks/ui";
import { auditToast } from "@/components/config/kit";
import { OVERVIEW } from "@/components/prop/data";
import { ActivityCard, BreachReasonsCard, FeesPayoutsCard, FunnelCard, PlanMixCard, PlanPerformanceStrip } from "@/components/prop/overview";
import { IS_DEMO } from "@kalks/mock/mode";
import { LivePropOverview } from "@/components/prop-live/overview";

export default function PropOverviewPage() {
  return IS_DEMO ? <DemoPropOverviewPage /> : <LivePropOverview />;
}

function DemoPropOverviewPage() {
  const net = OVERVIEW.feesMonth - OVERVIEW.payoutsMonth;
  return (
    <div className="pb-24">
      <PageHeader
        title="Prop Firm"
        subtitle="Evaluation pipeline, funded traders and the fee vs payout book · September 2026"
        actions={
          <>
            <Button variant="surface" size="sm" onClick={() => auditToast("Monthly prop report queued", "PDF + CSV will be emailed to finance@kalkstrade.com")}>
              <Layers /> Export report
            </Button>
            <Link href="/prop/payouts">
              <Button variant="surface" size="sm">
                <CircleDollarSign /> Payout queue <Chip size="sm" tone="warn">16</Chip>
              </Button>
            </Link>
            <Link href="/prop/plans">
              <Button variant="ember" size="sm">
                <Plus /> New plan
              </Button>
            </Link>
          </>
        }
      />

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-5">
        <KpiCard label="Active challenges" icon={<Activity />} value={<span className="k-num">{OVERVIEW.activeChallenges.toLocaleString()}</span>} chip="+184 this week" chipTone="up" href="/prop/challenges" />
        <KpiCard label="Pass rate" icon={<Percent />} value={<span className="k-num">{OVERVIEW.passRate}%</span>} chip="Phase 1 → Funded · 90d" chipTone="neutral" href="/prop/challenges" delay={0.05} />
        <KpiCard label="Funded traders" icon={<Trophy />} value={<span className="k-num">{OVERVIEW.funded}</span>} chip="$28.4M simulated capital" chipTone="gold" href="/prop/funded" delay={0.1} />
        <KpiCard
          label="Fees vs payouts"
          icon={<Scale />}
          value={<Money value={OVERVIEW.feesMonth} decimals={0} />}
          footer={
            <div className="flex items-center gap-1.5">
              <Chip size="sm" tone="gold">
                Paid <Money value={OVERVIEW.payoutsMonth} decimals={0} countUp={false} />
              </Chip>
              <Chip size="sm">{((OVERVIEW.payoutsMonth / OVERVIEW.feesMonth) * 100).toFixed(0)}% ratio</Chip>
            </div>
          }
          href="/prop/payouts"
          delay={0.15}
        />
        <KpiCard label="Net this month" value={<Money value={net} decimals={0} />} hot illustration="money_bag" chip="+12.4% vs Aug" chipTone="up" delay={0.2} className="sm:col-span-2 xl:col-span-1" />
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal delay={0.1} className="xl:col-span-8">
          <FeesPayoutsCard />
        </Reveal>
        <Reveal delay={0.15} className="xl:col-span-4">
          <FunnelCard />
        </Reveal>
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4 lg:grid-cols-2 xl:grid-cols-3">
        <Reveal delay={0.05}>
          <PlanMixCard />
        </Reveal>
        <Reveal delay={0.1}>
          <ActivityCard />
        </Reveal>
        <Reveal delay={0.15} className="lg:col-span-2 xl:col-span-1">
          <BreachReasonsCard />
        </Reveal>
      </div>

      <Reveal delay={0.1} className="mt-4">
        <PlanPerformanceStrip />
      </Reveal>
    </div>
  );
}
