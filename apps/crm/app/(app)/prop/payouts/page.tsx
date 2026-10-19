"use client";

import * as React from "react";
import Link from "next/link";
import { motion } from "motion/react";
import { ArrowRight, ArrowUpRight, Award, Banknote, CalendarClock, Check, CheckCircle2, Gem, Percent, Receipt, Rocket, TrendingUp, Trophy, Wallet } from "lucide-react";
import { toast } from "sonner";
import {
  Button,
  Card,
  CardHeader,
  Chip,
  CopyButton,
  DataTable,
  Donut,
  Icon3D,
  KpiCard,
  Money,
  PageHeader,
  Progress,
  Reveal,
  Starfield,
  StatusChip,
  cn,
  formatDateTime,
  formatMoney,
  type Column,
} from "@/components/kit";
import { ME } from "@kalks/mock";
import { FUNDED, FUNDED_SHARE, PROP_CERTIFICATES, PROP_PAYOUTS, SCALING, type PropPayout } from "@kalks/mock/prop";
import { RequestPayoutDialog } from "@/components/prop/payout-dialog";
import { CertificateCard } from "@/components/prop/certificate-card";
import { CountUp } from "@/components/prop/prop-ui";
import { IS_DEMO as DEMO_BUILD } from "@kalks/mock/mode";
import { LivePropPayouts } from "@/components/prop-live/payouts";
import { TERMINAL_URL } from "@/lib/live";

/* ------------------------------------------------------------------ */

function FundedCard() {
  return (
    <Card hot className="relative flex h-full flex-col overflow-hidden">
      <Starfield density={45} />
      <div className="relative flex flex-1 flex-col p-6">
        <div className="flex items-start justify-between gap-3">
          <div>
            <div className="flex items-center gap-2">
              <Chip tone="gold" size="sm" className="font-semibold tracking-wider">
                FUNDED
              </Chip>
              <Chip size="sm">1-Step · Simulated</Chip>
            </div>
            <div className="mt-3 text-[22px] font-semibold tracking-tight">$100,000 account</div>
            <div className="mt-1 flex items-center gap-1 font-mono text-[12px] text-fg-2">
              #{FUNDED.login} · {FUNDED.server}
              <CopyButton value={FUNDED.login} label="Login" />
            </div>
          </div>
          <Icon3D name="gem_stone" size={68} />
        </div>
        <div className="mt-5 grid grid-cols-2 gap-2">
          {[
            ["Balance", <Money key="b" value={FUNDED.balance} />],
            ["Equity", <Money key="e" value={FUNDED.equity} />],
            ["Funded since", "02 Jun 2026"],
            ["Current cycle", "10 – 24 Sep"],
          ].map(([l, v], i) => (
            <div key={i} className="rounded-[14px] border border-white/10 light:border-line bg-black/20 light:bg-white/70 px-3.5 py-2.5">
              <div className="text-[11.5px] text-fg-3">{l}</div>
              <div className="k-num mt-0.5 text-[15px] font-medium">{v}</div>
            </div>
          ))}
        </div>
        <div className="mt-auto flex flex-wrap gap-2 pt-5">
          <Link href="/prop/mine">
            <Button size="sm" variant="surface">
              Rules dashboard <ArrowUpRight />
            </Button>
          </Link>
          <Link target="_blank" rel="noopener" href={`${TERMINAL_URL}/?login=${FUNDED.login}`}>
            <Button size="sm" variant="ghost">
              Trade
            </Button>
          </Link>
        </div>
      </div>
    </Card>
  );
}

function SplitCard({ available, pending, onRequested }: { available: number; pending: number; onRequested: (n: number) => void }) {
  const kalks = FUNDED.profit - FUNDED_SHARE;
  return (
    <Card className="h-full">
      <CardHeader title="Profit split" subtitle="Current cycle · 10 Sep – 24 Sep 2026" icon={<Percent />} action={<Chip tone="up" dot>Eligible now</Chip>} />
      <div className="grid grid-cols-1 items-center gap-6 px-4 pb-6 pt-4 sm:px-6 md:grid-cols-[auto_1fr]">
        <div className="relative mx-auto">
          <Donut
            size={196}
            thickness={20}
            data={[
              { label: "You", value: FUNDED.splitPct, color: "var(--k-ember)" },
              { label: "Kalks", value: 100 - FUNDED.splitPct, color: "var(--k-gold)" },
            ]}
            center={
              <div>
                <div className="k-num text-[34px] font-semibold leading-none tracking-tight">80%</div>
                <div className="mt-1 text-[11.5px] text-fg-3">your share</div>
              </div>
            }
          />
        </div>
        <div className="min-w-0">
          <div className="k-label">Available to withdraw</div>
          <div className="mt-1.5 flex flex-wrap items-baseline gap-3">
            <Money value={available} className="text-[38px] font-semibold tracking-[-0.02em]" />
            {pending > 0 && (
              <Chip tone="warn" dot>
                {formatMoney(pending)} pending
              </Chip>
            )}
          </div>
          {/* split bar */}
          <div className="mt-4">
            <div className="flex h-9 w-full overflow-hidden rounded-full border border-line bg-surface-2 p-1">
              <motion.div
                initial={{ width: 0 }}
                animate={{ width: `${FUNDED.splitPct}%` }}
                transition={{ duration: 1, ease: [0.16, 1, 0.3, 1] }}
                className="k-ember-btn flex items-center rounded-full px-3 text-[12px] font-semibold"
              >
                You · {formatMoney(FUNDED_SHARE)}
              </motion.div>
              <div className="flex flex-1 items-center justify-end whitespace-nowrap px-2 text-[11.5px] font-medium text-gold">20% · {formatMoney(kalks, "USD", 0)}</div>
            </div>
          </div>
          <div className="mt-3 grid grid-cols-3 gap-2 text-[12px]">
            <div className="k-row px-3 py-2">
              <div className="text-fg-3">Cycle profit</div>
              <div className="k-num font-medium">{formatMoney(FUNDED.profit)}</div>
            </div>
            <div className="k-row px-3 py-2">
              <div className="text-fg-3">Next split</div>
              <div className="k-num font-medium text-gold">85% at $125k</div>
            </div>
            <div className="k-row px-3 py-2">
              <div className="text-fg-3">Next cycle</div>
              <div className="k-num font-medium">08 Oct</div>
            </div>
          </div>
          <div className="mt-4 flex flex-wrap items-center gap-2">
            <RequestPayoutDialog
              available={available}
              onRequested={onRequested}
              trigger={
                <Button variant="ember" size="lg" shimmer disabled={available < 50}>
                  <Banknote /> Request payout
                </Button>
              }
            />
            <span className="text-[12px] text-fg-3">Paid to your Kalks wallet · avg. 7.4h approval</span>
          </div>
        </div>
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */

function ScalingCard() {
  const s = SCALING;
  const criteria = [
    { l: "Months funded", v: `${s.monthsDone.toFixed(1)} / ${s.monthsRequired}`, p: (s.monthsDone / s.monthsRequired) * 100 },
    { l: "Net profit", v: `${s.profitDone}% / ${s.profitRequired}%`, p: (s.profitDone / s.profitRequired) * 100 },
    { l: "Payouts processed", v: `${s.payoutsDone} / ${s.payoutsRequired}`, p: 100 },
  ];
  const fmtSize = (n: number) => (n >= 1e6 ? `$${n / 1e6}M` : `$${Math.round(n / 1000)}k`);
  return (
    <Card className="h-full">
      <CardHeader
        title="Scaling plan"
        subtitle={`$100k → $125k after ${s.monthsRequired} months at ${s.profitRequired}% · review ${new Date(s.reviewDate).toLocaleDateString("en-GB", { day: "2-digit", month: "short" })}`}
        icon={<Rocket />}
        action={<Chip tone="gold">84% there</Chip>}
      />
      <div className="px-4 pb-6 pt-6 sm:px-6">
        {/* milestone track */}
        <div className="relative overflow-x-auto pb-1">
          <div className="relative min-w-[520px] px-2">
            <div className="absolute left-6 right-6 top-[18px] h-1 rounded-full bg-surface-3" />
            <motion.div
              className="absolute left-6 top-[18px] h-1 rounded-full bg-gradient-to-r from-[#f3cf6b] to-[var(--k-ember)]"
              initial={{ width: 0 }}
              animate={{ width: "calc((100% - 48px) * 0.21)" }}
              transition={{ duration: 1.2, ease: [0.16, 1, 0.3, 1], delay: 0.2 }}
            />
            <div className="relative flex justify-between">
              {s.milestones.map((m) => (
                <div key={m.size} className="flex w-20 flex-col items-center text-center">
                  <span
                    className={cn(
                      "grid size-9 place-items-center rounded-full border-2",
                      m.done && "border-gold bg-gold text-[#1a1204]",
                      m.next && "border-ember bg-surface shadow-[0_0_24px_-2px_color-mix(in_oklab,var(--k-ember)_80%,transparent)]",
                      !m.done && !m.next && "border-line bg-surface-2",
                    )}
                  >
                    {m.done ? <Check className="size-4" /> : m.next ? <span className="size-2.5 animate-pulse rounded-full bg-ember" /> : m.size >= 1e6 ? <Gem className="size-3.5 text-fg-3" /> : <span className="size-1.5 rounded-full bg-fg-3" />}
                  </span>
                  <div className={cn("k-num mt-2 text-[14px] font-semibold", m.next ? "text-ember" : m.done ? "text-gold" : "text-fg-2")}>{fmtSize(m.size)}</div>
                  <div className="text-[10.5px] text-fg-3">{m.date || m.label}</div>
                </div>
              ))}
            </div>
          </div>
        </div>
        <div className="mt-6 grid grid-cols-1 gap-2 sm:grid-cols-3">
          {criteria.map((c) => (
            <div key={c.l} className="k-row px-3.5 py-3">
              <div className="flex items-center justify-between text-[11.5px]">
                <span className="text-fg-3">{c.l}</span>
                {c.p >= 100 ? <CheckCircle2 className="size-3.5 text-up" /> : <span className="k-num text-fg-3">{Math.round(c.p)}%</span>}
              </div>
              <div className="k-num mt-1 text-[16px] font-semibold">{c.v}</div>
              <Progress value={c.p} tone={c.p >= 100 ? "up" : "gold"} className="mt-2" />
            </div>
          ))}
        </div>
        <div className="mt-3 flex items-center gap-2 rounded-[12px] bg-surface-2 px-3.5 py-2.5 text-[12px] text-fg-2">
          <TrendingUp className="size-4 shrink-0 text-gold" />
          On scale-up, your balance grows 25%, your split rises to 85%, and loss limits scale with it.
        </div>
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */

function History({ rows }: { rows: PropPayout[] }) {
  const cols: Column<PropPayout>[] = [
    { key: "id", header: "Payout", cell: (r) => <span className="font-mono text-[12.5px]">{r.id}</span> },
    {
      key: "acc",
      header: "Account",
      cell: (r) => (
        <span className="flex items-center gap-2">
          <span className="font-mono text-[12.5px] text-fg-2">#{r.account}</span>
          <Chip size="sm" tone="gold">
            ${r.size / 1000}k
          </Chip>
        </span>
      ),
      hideOn: "md",
    },
    { key: "date", header: "Requested", cell: (r) => <span className="k-num text-fg-2">{formatDateTime(r.requestedAt, { day: "2-digit", month: "short", year: "numeric" })}</span>, sort: (r) => r.requestedAt },
    { key: "gross", header: "Gross profit", align: "right", cell: (r) => <span className="k-num text-fg-2">{formatMoney(r.gross)}</span>, hideOn: "lg" },
    { key: "split", header: "Split", align: "right", cell: (r) => <span className="k-num">{r.split}%</span>, hideOn: "lg" },
    { key: "refund", header: "Fee refund", align: "right", cell: (r) => (r.refund ? <span className="k-num text-gold">+{formatMoney(r.refund)}</span> : <span className="text-fg-3">—</span>), hideOn: "md" },
    { key: "share", header: "You received", align: "right", sort: (r) => r.share + r.refund, cell: (r) => <span className={cn("k-num font-semibold", r.status === "rejected" ? "text-fg-3 line-through" : "text-up")}>{formatMoney(r.share + r.refund)}</span> },
    { key: "status", header: "Status", cell: (r) => <StatusChip status={r.status} label={r.status === "pending" ? "Risk review" : undefined} /> },
    { key: "tx", header: "Wallet tx", cell: (r) => <span className="font-mono text-[12px] text-fg-3">{r.tx}</span>, hideOn: "lg" },
  ];
  return (
    <Card>
      <CardHeader title="Payout history" subtitle="All prop payouts credited to your Kalks wallet" icon={<Receipt />} />
      <div className="px-4 pb-6 pt-4 sm:px-6">
        <DataTable columns={cols} rows={rows} pageSize={8} search={(r) => `${r.id} ${r.account} ${r.status}`} exportName="prop-payouts" rowKey={(r) => r.id} />
        <div className="mt-3 flex items-start gap-2 text-[12px] text-fg-3">
          <span className="mt-1 size-1.5 shrink-0 rounded-full bg-down" />
          PO-21602 was rejected: a cross-account hedge on 14 Feb 2026 (#80517702 vs #80412512) broke the banned-strategies rule.
        </div>
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */

function DemoPropPayoutsPage() {
  const [available, setAvailable] = React.useState(FUNDED_SHARE);
  const [rows, setRows] = React.useState<PropPayout[]>(PROP_PAYOUTS);
  const pending = rows.filter((r) => r.status === "pending").reduce((s, r) => s + r.share, 0);
  const paid = rows.filter((r) => r.status === "completed").reduce((s, r) => s + r.share + r.refund, 0);

  const onRequested = (n: number) => {
    setAvailable((a) => Math.max(0, +(a - n).toFixed(2)));
    setRows((r) => [
      { id: "PO-24512", account: FUNDED.login, size: 100000, requestedAt: "2026-09-24T15:12:00Z", gross: +(n / 0.8).toFixed(2), split: 80, share: n, refund: 0, status: "pending", tx: "—" },
      ...r,
    ]);
  };

  return (
    <div className="pb-24">
      <PageHeader
        title="Payouts"
        subtitle="Your funded profit share, scaling progress and certificates."
        actions={
          <>
            <Link href="/prop/mine">
              <Button variant="surface" size="lg">
                <Trophy /> My challenges
              </Button>
            </Link>
            <RequestPayoutDialog
              available={available}
              onRequested={onRequested}
              trigger={
                <Button variant="ember" size="lg" disabled={available < 50}>
                  <Banknote /> Request payout
                </Button>
              }
            />
          </>
        }
      />

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <KpiCard label="Available now" icon={<Wallet />} value={<Money value={available} />} chip="80% of cycle profit" chipTone="up" hot illustration="money_bag" />
        <KpiCard label="Total paid out" icon={<Banknote />} value={<Money value={paid} />} chip={`${rows.filter((r) => r.status === "completed").length} payouts`} chipTone="gold" delay={0.05} />
        <KpiCard label="Fees refunded" icon={<Receipt />} value={<Money value={FUNDED.feePaid + 289} />} chip="2 challenges" chipTone="gold" delay={0.1} />
        <KpiCard
          label="Avg. approval time"
          icon={<CalendarClock />}
          value={
            <span>
              <CountUp value={6.2} decimals={1} />
              <span className="text-fg-3">h</span>
            </span>
          }
          chip="Fastest 3.7h"
          chipTone="up"
          delay={0.15}
        />
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal delay={0.05} className="xl:col-span-4">
          <FundedCard />
        </Reveal>
        <Reveal delay={0.1} className="xl:col-span-8">
          <SplitCard available={available} pending={pending} onRequested={onRequested} />
        </Reveal>
      </div>

      <Reveal delay={0.05} className="mt-4 block">
        <ScalingCard />
      </Reveal>

      <Reveal delay={0.05} className="mt-4 block">
        <History rows={rows} />
      </Reveal>

      <Reveal delay={0.05} className="mt-4 block">
        <Card>
          <CardHeader
            title="Certificates"
            subtitle="Verified on kalkstrade.com — share your milestones"
            icon={<Award />}
            action={
              <Button size="sm" variant="surface" onClick={() => toast.success("All certificates downloaded", { description: `${PROP_CERTIFICATES.length} PNG files · kalks-certificates.zip` })}>
                Download all
              </Button>
            }
          />
          <div className="grid grid-cols-1 gap-5 px-4 pb-6 pt-5 sm:grid-cols-2 sm:px-6 xl:grid-cols-4">
            {PROP_CERTIFICATES.map((c, i) => (
              <motion.div key={c.id} initial={{ opacity: 0, y: 16 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 0.15 + i * 0.07, duration: 0.5 }}>
                <CertificateCard cert={c} name={ME.name} />
              </motion.div>
            ))}
          </div>
        </Card>
      </Reveal>

      <Reveal delay={0.05} className="mt-4 block">
        <Card className="relative overflow-hidden">
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img src="/assets/photos/money.jpg" alt="" className="absolute inset-0 size-full object-cover opacity-25" />
          <div className="absolute inset-0 bg-gradient-to-r from-bg via-bg/85 to-transparent" />
          <div className="relative flex flex-col gap-4 p-6 md:flex-row md:items-center md:justify-between">
            <div className="flex items-center gap-4">
              <Icon3D name="bank" size={56} className="hidden sm:block" />
              <div>
                <div className="text-[17px] font-medium">Put your payout to work</div>
                <div className="mt-0.5 text-[13px] text-fg-2">Move profit from your wallet into a live Pro account, or withdraw to TRC20 in minutes.</div>
              </div>
            </div>
            <div className="flex flex-wrap gap-2">
              <Link href="/wallet">
                <Button variant="surface">
                  <Wallet /> Wallet
                </Button>
              </Link>
              <Link href="/accounts/new">
                <Button variant="ember">
                  Open live account <ArrowRight />
                </Button>
              </Link>
            </div>
          </div>
        </Card>
      </Reveal>
    </div>
  );
}

/** Live builds: plans, challenges and payouts from the prop service (via /api/prop). Demo builds: mock data. */
export default function PropPayoutsPage() {
  return DEMO_BUILD ? <DemoPropPayoutsPage /> : <LivePropPayouts />;
}
