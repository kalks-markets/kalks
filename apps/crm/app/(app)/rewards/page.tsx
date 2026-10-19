"use client";

import * as React from "react";
import Link from "next/link";
import { Gift, History, Medal, Sparkles } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, CardHeader, Chip, Icon3D, KpiCard, PageHeader, Reveal } from "@/components/kit";
import { ContestHero, Leaderboard, PastContests, PrizeCard, UpcomingContests } from "@/components/rewards/contests";
import { IS_DEMO } from "@kalks/mock/mode";
import { LiveContestsPage } from "@/components/growth/contests";

function RewardsShortcuts() {
  return (
    <Card className="flex h-full flex-col">
      <CardHeader title="Your rewards" subtitle="Everything you earn while trading" />
      <div className="mt-4 flex-1 space-y-2 px-4 pb-5 sm:px-6">
        {[
          { href: "/rewards/loyalty", icon: "gem_stone", title: "18,420 points", sub: "Gold tier · ≈ $184.20 value", chip: "Redeem" },
          { href: "/rewards/cashback", icon: "money_with_wings", title: "$186.42 cashback", sub: "Ready to withdraw to wallet", chip: "Withdraw" },
          { href: "/prop", icon: "rocket", title: "Prop challenge", sub: "Trade $25k–$200k of firm capital", chip: "Start" },
        ].map((r) => (
          <Link key={r.href} href={r.href} className="k-row flex items-center gap-3 px-3.5 py-3 transition-colors hover:bg-surface-3/60">
            <Icon3D name={r.icon} size={36} />
            <div className="min-w-0 flex-1">
              <div className="k-num text-[14px] font-medium">{r.title}</div>
              <div className="truncate text-[11.5px] text-fg-3">{r.sub}</div>
            </div>
            <Chip size="sm" tone="ember">
              {r.chip}
            </Chip>
          </Link>
        ))}
      </div>
    </Card>
  );
}

function DemoContestsPage() {
  return (
    <div className="pb-16">
      <PageHeader
        title="Contests"
        subtitle="Compete on demo or live accounts, climb the leaderboard and win real USDT prizes."
        actions={
          <>
            <Button variant="surface" onClick={() => toast("Contest history", { description: "You've entered 6 contests · best finish #9 (Demo Sprint · July)" })}>
              <History /> My history
            </Button>
            <Button variant="ember" shimmer onClick={() => toast.success("Invite link copied", { description: "app.kalkstrade.com/rewards/contests/gold-rush?ref=ARJUN24 · both get +250 pts" })}>
              <Sparkles /> Invite a friend
            </Button>
          </>
        }
      />

      <Reveal>
        <ContestHero />
      </Reveal>

      <div className="mt-4 grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <KpiCard label="Contests entered" icon={<Medal />} value="6" chip="2 prize finishes" chipTone="gold" delay={0.05} />
        <KpiCard label="Prizes won" icon={<Gift />} value={<span className="k-num">$1,350<span className="opacity-40">.00</span></span>} chip="Paid to wallet" chipTone="up" delay={0.1} />
        <KpiCard label="Best finish" icon={<Medal />} value="#9" chip="Demo Sprint · Jul" delay={0.15} />
        <KpiCard label="Active contests" value="1" hot illustration="trophy" chip="3 upcoming" chipTone="ember" delay={0.2} />
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal delay={0.1} className="xl:col-span-8">
          <Leaderboard />
        </Reveal>
        <div className="flex flex-col gap-4 xl:col-span-4">
          <Reveal delay={0.15}>
            <PrizeCard />
          </Reveal>
          <Reveal delay={0.2} className="flex-1">
            <RewardsShortcuts />
          </Reveal>
        </div>
      </div>

      <Reveal delay={0.1} className="mt-8">
        <div className="mb-4 flex items-end justify-between">
          <div>
            <h2 className="text-[19px] font-medium tracking-tight">Upcoming contests</h2>
            <p className="text-[13px] text-fg-3">Register early — seats on live contests are limited.</p>
          </div>
        </div>
        <UpcomingContests />
      </Reveal>

      <Reveal delay={0.1} className="mt-8">
        <div className="mb-4">
          <h2 className="text-[19px] font-medium tracking-tight">Past contests</h2>
          <p className="text-[13px] text-fg-3">Winners and final standings.</p>
        </div>
        <PastContests />
      </Reveal>
    </div>
  );
}

export default function Page() {
  return IS_DEMO ? <DemoContestsPage /> : <LiveContestsPage />;
}
