"use client";

import * as React from "react";
import { Gauge as GaugeIcon, RefreshCw, ShieldAlert, TrendingDown, TrendingUp, Users } from "lucide-react";
import { Button, Card, Chip, DataTable, EmptyState, KpiCard, PageHeader, Reveal, Segmented, cn, formatNumber, type Column } from "@kalks/ui";
import { IS_DEMO } from "@kalks/mock/mode";
import { TableSkeleton, ago, useApi, useNow } from "@/components/live/kit";
import { useCan } from "@/components/staff-session";
import { DeskStatusChip } from "@/components/trading-desk/status";
import { signedMoney } from "@/components/trading-desk/kit";
import { CategoryChip, ConnectEngineState, LoginCell, RiskError, ScoreBar, ScoreDrawer, recomputeScores } from "@/components/trading-live/risk-kit";
import { CATEGORY_LABEL, ensureClientNames, flagLabel, groupLabel, holdText, liveClientName, useLiveDirectory, type ClientScore, type ScoreCategory, type ScoreList } from "@/lib/trading-desk";

type Filter = "all" | ScoreCategory;
const ORDER: ScoreCategory[] = ["toxic", "likely_profitable", "neutral", "likely_losing"];
const pctOf = (v: number | null) => (v === null ? "—" : `${formatNumber(v <= 1 ? v * 100 : v, 0)}%`);

export default function ClientScoresPage() {
  const now = useNow();
  const canWrite = useCan("dealing.write");
  useLiveDirectory();
  const { data, error, loading, reload } = useApi<ScoreList>(IS_DEMO ? null : "/api/trading/dealing/risk/scores?limit=500", { refreshMs: 60_000 });
  const [filter, setFilter] = React.useState<Filter>("all");
  const [open, setOpen] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);
  const items = React.useMemo(() => data?.items ?? [], [data]);
  React.useEffect(() => ensureClientNames(items.map((s) => s.userId)), [items]);
  const count = (c: ScoreCategory) => items.filter((s) => s.category === c).length;
  const rows = filter === "all" ? items : items.filter((s) => s.category === filter);
  const flagged = items.filter((s) => s.toxicFlags.length > 0).length;

  const cols: Column<ClientScore>[] = [
    { key: "l", header: "Account", cell: (r) => <LoginCell login={r.login} userId={r.userId} sub={r.kind === "demo" ? "demo" : undefined} />, sort: (r) => r.login, csv: (r) => r.login },
    { key: "g", header: "Group", hideOn: "lg", cell: (r) => <span className="whitespace-nowrap text-[12px] text-fg-2">{groupLabel(r.group)}</span>, sort: (r) => r.group },
    { key: "c", header: "Category", cell: (r) => <CategoryChip category={r.category} />, sort: (r) => ORDER.indexOf(r.category), csv: (r) => r.category },
    { key: "s", header: "Score", cell: (r) => <ScoreBar score={r.score} category={r.category} />, sort: (r) => r.score, csv: (r) => r.score },
    { key: "t", header: "Trades", align: "right", hideOn: "md", cell: (r) => <span className="k-num font-mono text-[12px]">{formatNumber(r.trades, 0)}</span>, sort: (r) => r.trades },
    { key: "w", header: "Win rate", align: "right", hideOn: "md", cell: (r) => <span className="k-num font-mono text-[12px]">{pctOf(r.winRate)}</span>, sort: (r) => r.winRate ?? -1 },
    { key: "pf", header: "PF", align: "right", hideOn: "xl", cell: (r) => <span className="k-num font-mono text-[12px]">{r.profitFactor === null ? "—" : formatNumber(r.profitFactor, 2)}</span>, sort: (r) => r.profitFactor ?? -1 },
    { key: "p", header: "Net P&L", align: "right", cell: (r) => <span className={cn("k-num font-mono text-[12px]", r.netPnlUsd >= 0 ? "text-up" : "text-down")}>{signedMoney(r.netPnlUsd, 0)}</span>, sort: (r) => r.netPnlUsd, csv: (r) => r.netPnlUsd.toFixed(2) },
    { key: "h", header: "Avg hold", align: "right", hideOn: "lg", cell: (r) => <span className={cn("k-num font-mono text-[12px]", r.avgHoldSecs !== null && r.avgHoldSecs < 60 ? "text-warn" : "text-fg-2")}>{holdText(r.avgHoldSecs)}</span>, sort: (r) => r.avgHoldSecs ?? Infinity },
    {
      key: "f",
      header: "Flags",
      cell: (r) =>
        r.toxicFlags.length ? (
          <span className="flex max-w-[220px] flex-wrap gap-1">
            {r.toxicFlags.map((f) => (
              <Chip key={f} size="sm" tone="down">
                {flagLabel(f)}
              </Chip>
            ))}
          </span>
        ) : (
          <span className="text-fg-3">—</span>
        ),
      sort: (r) => r.toxicFlags.length,
      csv: (r) => r.toxicFlags.join(" "),
    },
    { key: "at", header: "Computed", hideOn: "xl", cell: (r) => <span className="whitespace-nowrap text-[11.5px] text-fg-3">{ago(r.computedAt, now)}</span>, sort: (r) => r.computedAt },
  ];

  return (
    <div className="pb-10">
      <PageHeader
        title="Client scores"
        subtitle={
          <span className="inline-flex flex-wrap items-center gap-2">
            Engine score 0-100 per account from closed trades: high = likely profitable for the client, so risk for the B-book. Click a row for the reasons. <DeskStatusChip />
            {data?.running && <Chip size="sm" tone="warn" dot>Scoring run in progress</Chip>}
          </span>
        }
        actions={
          canWrite && !IS_DEMO ? (
            <Button
              variant="ember"
              size="lg"
              disabled={busy}
              onClick={async () => {
                setBusy(true);
                if (await recomputeScores()) setTimeout(reload, 4000);
                setBusy(false);
              }}
            >
              <RefreshCw /> {busy ? "Queuing…" : "Recompute all"}
            </Button>
          ) : undefined
        }
      />
      <div className="grid grid-cols-2 gap-4 xl:grid-cols-4">
        <KpiCard label="Scored accounts" icon={<Users />} value={<span className="k-num">{formatNumber(items.length, 0)}</span>} chip={data?.computedAt ? `Last run ${ago(data.computedAt, now)}` : "No run yet"} />
        <KpiCard label="Toxic" icon={<ShieldAlert />} value={<span className="k-num text-down">{count("toxic")}</span>} chip={`${flagged} with toxic-flow flags`} chipTone="down" delay={0.04} />
        <KpiCard label="Likely profitable" icon={<TrendingUp />} value={<span className="k-num text-warn">{count("likely_profitable")}</span>} chip="Risk for the B-book" chipTone="warn" delay={0.08} />
        <KpiCard label="Likely losing" icon={<TrendingDown />} value={<span className="k-num text-up">{count("likely_losing")}</span>} chip={`${count("neutral")} neutral`} chipTone="up" delay={0.12} />
      </div>
      <Reveal delay={0.06} className="mt-4">
        <Card className="px-4 py-5 sm:px-6">
          {IS_DEMO ? (
            <ConnectEngineState what="Client scores" />
          ) : error ? (
            <RiskError error={error} onRetry={reload} />
          ) : loading && !data ? (
            <TableSkeleton rows={8} />
          ) : (
            <DataTable
              columns={cols}
              rows={rows}
              dense
              pageSize={25}
              rowKey={(r) => r.login}
              exportName="client-scores"
              onRowClick={(r) => setOpen(r.login)}
              search={(r) => `${r.login} ${r.userId} ${liveClientName(r.userId, r.login)} ${r.group}`}
              searchPlaceholder="Search login, client or group…"
              toolbar={
                <div className="max-w-full overflow-x-auto">
                  <Segmented
                    size="sm"
                    value={filter}
                    onChange={setFilter}
                    options={[{ value: "all" as Filter, label: `All ${items.length}` }, ...ORDER.map((c) => ({ value: c as Filter, label: `${CATEGORY_LABEL[c]} ${count(c)}` }))]}
                  />
                </div>
              }
              empty={<EmptyState title={items.length ? "No accounts in this category" : "No scores yet"} text={items.length ? "Pick another category." : "The engine scores accounts with closed trades on its schedule. Recompute all to run it now."} illustration="chart_increasing" />}
            />
          )}
          <p className="mt-4 flex items-start gap-2 text-[11.5px] text-fg-3">
            <GaugeIcon className="mt-0.5 size-3.5 shrink-0" /> Categories: toxic (latency arbitrage, gap trading, copied trades, bonus abuse), likely profitable (70+), neutral, likely losing. Routing rules can match “Risk score” and “Risk category”.
          </p>
        </Card>
      </Reveal>
      <ScoreDrawer
        login={open}
        onClose={() => {
          setOpen(null);
        }}
      />
    </div>
  );
}
