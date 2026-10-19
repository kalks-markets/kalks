"use client";

import * as React from "react";
import Link from "next/link";
import { AnimatePresence, motion } from "motion/react";
import { FileDown, Rocket, Share2, Workflow } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, Chip, EquityChart, Menu, Money, PageHeader, Reveal, Segmented, SymbolAvatar, cn, type SeriesPoint } from "@/components/kit";
import { hashString } from "@kalks/mock";
import { BACKTEST_STRATEGIES, COST_MODELS, DEFAULT_BACKTEST, PAST_RUNS, formatDateLabel, runBacktest, serverTime, type BacktestParams, type BacktestResult, type PastRun } from "@kalks/mock/algo";
import { BacktestForm, PastRuns } from "@/components/developer/backtest-form";
import { BacktestKpis, DrawdownChart, MonteCarloCard, MonthlyReturns, ReturnDistribution, TradeStats, TradesTable } from "@/components/developer/backtest-report";
import { IS_DEMO as DEMO_BUILD } from "@kalks/mock/mode";
import { LiveBacktestsPage } from "@/components/algo/backtests-page";

const STAGES: [number, string][] = [
  [0, "Loading M1 history…"],
  [22, "Reconstructing ticks from M1 OHLC…"],
  [48, "Simulating orders & fills…"],
  [78, "Applying spread & commission model…"],
  [92, "Computing metrics…"],
];

function EquityCard({ r }: { r: BacktestResult }) {
  const [view, setView] = React.useState<"equity" | "weekly">("equity");
  const [hover, setHover] = React.useState<SeriesPoint | null>(null);
  const onHover = React.useCallback((p: SeriesPoint | null) => setHover(p), []);
  const data = React.useMemo(() => {
    if (view === "equity") return r.equity;
    return r.equity.filter((_, i) => i % 7 === 0 || i === r.equity.length - 1).map((p, i, a) => ({ ...p, volume: i ? Math.abs(p.value - a[i - 1]!.value) : 0 }));
  }, [r, view]);
  const last = r.equity[r.equity.length - 1]!.value;
  const shown = hover?.value ?? last;
  const diff = shown - r.params.balance;
  return (
    <Card className="overflow-hidden">
      <div className="flex flex-col gap-4 px-6 pt-6 sm:flex-row sm:items-start sm:justify-between">
        <div>
          <div className="k-label">Equity curve</div>
          <div className="mt-2 flex flex-wrap items-baseline gap-3">
            <Money value={shown} countUp={!hover} className="text-[30px] font-semibold tracking-tight" />
            <Chip tone={diff >= 0 ? "up" : "down"}>
              {diff >= 0 ? "+" : ""}
              {((diff / r.params.balance) * 100).toFixed(2)}%
            </Chip>
          </div>
          <div className="mt-1 text-xs text-fg-3">{hover ? serverTime(hover.time, false) : `${formatDateLabel(r.params.from)} → ${formatDateLabel(r.params.to)} · bars = trades per day`}</div>
        </div>
        <Segmented
          size="xs"
          value={view}
          onChange={setView}
          options={[
            { value: "equity", label: "Daily" },
            { value: "weekly", label: "Weekly" },
          ]}
        />
      </div>
      <div className="px-3 pt-2">
        <EquityChart key={`${r.seed}-${view}`} data={data} height={280} onHover={onHover} />
      </div>
      <div className="border-t border-line px-3 pb-3 pt-3">
        <DrawdownChart key={r.seed} data={r.drawdown} height={110} />
      </div>
    </Card>
  );
}

function DemoBacktestsPage() {
  const [runs, setRuns] = React.useState<PastRun[]>(PAST_RUNS);
  const [activeId, setActiveId] = React.useState(PAST_RUNS[0]!.id);
  const [params, setParams] = React.useState<BacktestParams>(DEFAULT_BACKTEST);
  const [extra, setExtra] = React.useState<Record<string, BacktestResult>>({});
  const [running, setRunning] = React.useState(false);
  const [progress, setProgress] = React.useState(0);
  const timer = React.useRef<ReturnType<typeof setInterval> | null>(null);
  React.useEffect(() => () => void (timer.current && clearInterval(timer.current)), []);

  const base = React.useMemo(() => Object.fromEntries(PAST_RUNS.map((r) => [r.id, runBacktest(r.params, r.seed)])), []);
  const results = React.useMemo(() => ({ ...base, ...extra }), [base, extra]);
  const result = results[activeId]!;
  const run = runs.find((r) => r.id === activeId)!;
  const strat = BACKTEST_STRATEGIES.find((s) => s.id === result.params.strategyId)!;
  const stage = [...STAGES].reverse().find(([p]) => progress >= p)![1];

  const start = () => {
    if (params.from >= params.to) return toast.error("Start date must be before end date");
    if (params.balance < 100) return toast.error("Initial balance must be at least $100");
    setRunning(true);
    setProgress(0);
    const t0 = performance.now();
    timer.current = setInterval(() => {
      const p = Math.min(100, ((performance.now() - t0) / 2000) * 100);
      setProgress(p);
      if (p >= 100) {
        clearInterval(timer.current!);
        const n = runs.length;
        const seed = hashString(`${JSON.stringify(params)}#${n}`);
        const res = runBacktest(params, seed);
        const id = `bt-${2292 + n}`;
        const now = new Date(Date.now() + 3 * 3600 * 1000);
        const created = `24 Sep · ${String(now.getUTCHours()).padStart(2, "0")}:${String(now.getUTCMinutes()).padStart(2, "0")}`;
        setExtra((e) => ({ ...e, [id]: res }));
        setRuns((rs) => [{ id, seed, created, duration: "2.0s", params }, ...rs]);
        setActiveId(id);
        setRunning(false);
        toast.success("Backtest complete", {
          description: `${res.kpis.trades} trades · ${res.kpis.netProfit >= 0 ? "+" : "−"}$${Math.abs(res.kpis.netProfit).toFixed(0)} · PF ${res.kpis.profitFactor.toFixed(2)}`,
        });
      }
    }, 40);
  };

  const pick = (id: string) => {
    setActiveId(id);
    const r = runs.find((x) => x.id === id)!;
    setParams(r.params);
    toast(`Loaded run #${id.replace("bt-", "")}`, { description: `${r.params.symbol} · ${r.params.timeframe} · ${r.created}` });
  };

  return (
    <>
      <PageHeader
        title="Backtests"
        subtitle="Replay any strategy on M1 history with realistic spreads, commission and slippage."
        actions={
          <>
            <Link href="/developer/strategies">
              <Button variant="surface">
                <Workflow /> Strategy builder
              </Button>
            </Link>
            <Menu
              trigger={
                <Button variant="surface">
                  <FileDown /> Export
                </Button>
              }
              items={[
                { label: "PDF report", icon: <FileDown />, onSelect: () => toast.success("Report exported", { description: `${run.id}.pdf · 6 pages` }) },
                { label: "Trades CSV", icon: <FileDown />, onSelect: () => toast.success("Trades exported", { description: `${result.trades.length} rows` }) },
                { label: "Share link", icon: <Share2 />, onSelect: () => toast.success("Share link copied", { description: `app.kalkstrade.com/developer/backtests/${run.id}` }) },
              ]}
            />
            <Button variant="ember" shimmer onClick={() => toast.success(`Deploying “${strat.name.replace(" (template)", "")}”`, { description: "Pick an account in Strategy builder › Deploy" })}>
              <Rocket /> Deploy
            </Button>
          </>
        }
      />

      <div className="grid grid-cols-1 gap-5 xl:grid-cols-[340px_minmax(0,1fr)]">
        <div className="space-y-5">
          <Reveal>
            <BacktestForm params={params} onChange={setParams} onRun={start} running={running} progress={progress} stage={stage} />
          </Reveal>
          <Reveal delay={0.08} className="hidden xl:block">
            <PastRuns runs={runs} results={results} active={activeId} onPick={pick} />
          </Reveal>
          <Reveal delay={0.12} className="hidden xl:block">
            <MonteCarloCard key={activeId} r={result} />
          </Reveal>
        </div>

        <div className="relative min-w-0">
          <AnimatePresence>
            {running && (
              <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} className="pointer-events-none absolute inset-0 z-20 rounded-[20px] bg-bg/40 backdrop-blur-[2px]">
                <div className="sticky top-28 mx-auto mt-24 w-[min(360px,90%)] rounded-[20px] border border-ember/30 bg-surface/95 p-5 shadow-[0_24px_60px_-20px_color-mix(in_oklab,var(--k-ember)_50%,transparent)]">
                  <div className="flex items-center justify-between text-[13px]">
                    <span className="font-medium">Running backtest</span>
                    <span className="k-num text-ember">{Math.round(progress)}%</span>
                  </div>
                  <div className="mt-3 h-1.5 overflow-hidden rounded-full bg-surface-3">
                    <div className="h-full rounded-full bg-gradient-to-r from-ember-2 to-ember" style={{ width: `${progress}%` }} />
                  </div>
                  <div className="mt-2.5 truncate font-mono text-[11px] text-fg-3">{stage}</div>
                </div>
              </motion.div>
            )}
          </AnimatePresence>

          <motion.div key={activeId} initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.45, ease: [0.16, 1, 0.3, 1] }} className={cn("space-y-5 transition-[filter] duration-300", running && "blur-[1px]")}>
            <Card className="overflow-hidden">
              <div className="flex flex-col gap-3 px-5 pt-5 sm:flex-row sm:items-center sm:justify-between sm:px-6">
                <div className="flex min-w-0 items-center gap-3">
                  <SymbolAvatar symbol={result.params.symbol} size={34} />
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2">
                      <h3 className="truncate text-[17px] font-medium tracking-tight">{strat.name.replace(" (template)", "")}</h3>
                      <Chip size="sm" tone="neutral">
                        <span className="font-mono">#{run.id.replace("bt-", "")}</span>
                      </Chip>
                      <Chip size="sm" tone="up" dot>
                        Completed
                      </Chip>
                    </div>
                    <div className="mt-0.5 truncate font-mono text-[11.5px] text-fg-3">
                      {result.params.symbol} · {result.params.timeframe} · {formatDateLabel(result.params.from)} → {formatDateLabel(result.params.to)} · {(result.bars / 1e6).toFixed(2)}M bars · {COST_MODELS.find((c) => c.value === result.params.costModel)!.label}
                    </div>
                  </div>
                </div>
                <div className="flex shrink-0 items-center gap-2 text-[11.5px] text-fg-3">
                  <span>
                    Ran {run.created} · {run.duration}
                  </span>
                </div>
              </div>
              <div className="px-4 pb-5 pt-4 sm:px-6">
                <BacktestKpis r={result} />
              </div>
            </Card>

            <EquityCard r={result} />

            <MonthlyReturns r={result} />

            <div className="grid grid-cols-1 gap-5 lg:grid-cols-2">
              <ReturnDistribution r={result} />
              <TradeStats r={result} />
            </div>
          </motion.div>
        </div>

        <motion.div key={`t-${activeId}`} initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.45 }} className="min-w-0 xl:col-span-2">
          <TradesTable r={result} />
        </motion.div>

        <Reveal delay={0.08} className="grid grid-cols-1 gap-5 md:grid-cols-2 xl:hidden">
          <PastRuns runs={runs} results={results} active={activeId} onPick={pick} />
          <MonteCarloCard key={activeId} r={result} />
        </Reveal>
      </div>
    </>
  );
}

export default function BacktestsPage() {
  return DEMO_BUILD ? <DemoBacktestsPage /> : <React.Suspense fallback={null}><LiveBacktestsPage /></React.Suspense>;
}
