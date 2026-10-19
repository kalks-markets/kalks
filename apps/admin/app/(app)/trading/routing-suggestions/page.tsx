"use client";

import * as React from "react";
import Link from "next/link";
import { ArrowRight, Check, Clock, Info, Route as RouteIcon, X } from "lucide-react";
import { Button, Card, Chip, DataTable, EmptyState, KpiCard, PageHeader, Reveal, SymbolCell, Tabs, Tooltip, buttonVariants, cn, formatNumber, type Column } from "@kalks/ui";
import { IS_DEMO } from "@kalks/mock/mode";
import { TableSkeleton, ago, useApi, useNow } from "@/components/live/kit";
import { useCan } from "@/components/staff-session";
import { BookChip, DeskDialog } from "@/components/trading-desk/kit";
import { DeskStatusChip } from "@/components/trading-desk/status";
import { tradingWrite } from "@/components/trading-live/kit";
import { CategoryChip, ConnectEngineState, LoginCell, RiskError } from "@/components/trading-live/risk-kit";
import { ensureClientNames, liveClientName, serverStamp, useLiveDirectory, useRestDesk, type RiskLimitsDoc, type RoutingSuggestion } from "@/lib/trading-desk";

type Tab = "pending" | "history";
const STATUS_TONE = { pending: "warn", approved: "up", rejected: "down", expired: "neutral" } as const;
const RISK_CODE = "DLR-03 · Risk management";

function Target({ s }: { s: RoutingSuggestion }) {
  return s.kind === "position" && s.symbol ? (
    <SymbolCell
      symbol={s.symbol}
      size={22}
      sub={
        <span className="font-mono text-[11px] text-fg-3">
          #{s.ticket} · {s.volume === null ? "—" : `${formatNumber(s.volume, 2)} lots`}
        </span>
      }
    />
  ) : (
    <span className="block">
      <Chip size="sm" tone="info">
        Whole account
      </Chip>
      <span className="mt-0.5 block text-[11px] text-fg-3">Open positions + new trades</span>
    </span>
  );
}

function Move({ s }: { s: RoutingSuggestion }) {
  return (
    <span className="inline-flex items-center gap-1.5">
      <BookChip book={s.from} />
      <ArrowRight className="size-3.5 text-fg-3" />
      <BookChip book={s.to} />
    </span>
  );
}

function Reasons({ s }: { s: RoutingSuggestion }) {
  return (
    <span className="flex max-w-[340px] flex-wrap gap-1">
      {s.rule && (
        <Chip size="sm" tone="neutral">
          Rule: {s.rule}
        </Chip>
      )}
      {s.reasons.map((r) => (
        <Tooltip key={r.label} content={r.detail || r.label}>
          <span className="rounded-full border border-line bg-surface-2 px-2 py-0.5 text-[11px] text-fg-2">{r.label}</span>
        </Tooltip>
      ))}
    </span>
  );
}

export default function RoutingSuggestionsPage() {
  const now = useNow();
  const canWrite = useCan("dealing.write");
  const rest = useRestDesk();
  useLiveDirectory();
  const [tab, setTab] = React.useState<Tab>("pending");
  const pending = useApi<RoutingSuggestion[]>(IS_DEMO ? null : "/api/trading/dealing/routing/suggestions?status=pending&limit=200", { refreshMs: 10_000 });
  const all = useApi<RoutingSuggestion[]>(IS_DEMO ? null : "/api/trading/dealing/routing/suggestions?status=all&limit=200", { refreshMs: 30_000 });
  const limits = useApi<RiskLimitsDoc>(IS_DEMO ? null : "/api/trading/dealing/risk/limits");
  const [act, setAct] = React.useState<{ s: RoutingSuggestion; op: "approve" | "reject" } | null>(null);
  const mode = IS_DEMO ? "recommend" : (limits.data?.settings.routingMode ?? null);
  const queue = React.useMemo(() => (Array.isArray(pending.data) ? pending.data : []), [pending.data]);
  const history = React.useMemo(() => (Array.isArray(all.data) ? all.data.filter((s) => s.status !== "pending") : []), [all.data]);
  React.useEffect(() => ensureClientNames([...queue, ...history].map((s) => s.userId)), [queue, history]);
  const today = new Date().toISOString().slice(0, 10);
  const decidedToday = history.filter((s) => s.decidedAt?.startsWith(today));
  const toA = queue.filter((s) => s.to === "A").length;

  const base: Column<RoutingSuggestion>[] = [
    { key: "c", header: "Client", cell: (s) => <LoginCell login={s.login} userId={s.userId} />, sort: (s) => s.login, csv: (s) => s.login },
    { key: "t", header: "Position / account", cell: (s) => <Target s={s} />, sort: (s) => s.symbol ?? "", csv: (s) => (s.kind === "position" ? `${s.ticket} ${s.symbol} ${s.volume}` : "account") },
    { key: "m", header: "Move", cell: (s) => <Move s={s} />, sort: (s) => s.to, csv: (s) => `${s.from}->${s.to}` },
    { key: "r", header: "Why", cell: (s) => <Reasons s={s} />, csv: (s) => [s.rule, ...s.reasons.map((r) => r.label)].filter(Boolean).join("; ") },
    {
      key: "sc",
      header: "Score",
      hideOn: "lg",
      cell: (s) => (
        <span className="flex flex-col items-start gap-1">
          {s.score !== null && <span className="k-num font-mono text-[12px]">{Math.round(s.score)}</span>}
          <CategoryChip category={s.category} />
        </span>
      ),
      sort: (s) => s.score ?? -1,
    },
  ];
  const pendingCols: Column<RoutingSuggestion>[] = [
    ...base,
    { key: "at", header: "Raised", hideOn: "md", cell: (s) => <span className="whitespace-nowrap text-[11.5px] text-fg-3">{ago(s.createdAt, now)}</span>, sort: (s) => s.createdAt },
    {
      key: "a",
      header: "",
      align: "right",
      cell: (s) =>
        canWrite ? (
          <span className="inline-flex gap-1.5" onClick={(e) => e.stopPropagation()}>
            <Button size="xs" variant="ember" onClick={() => setAct({ s, op: "approve" })}>
              <Check /> Approve
            </Button>
            <Button size="xs" variant="ghost" onClick={() => setAct({ s, op: "reject" })}>
              <X /> Reject
            </Button>
          </span>
        ) : (
          <span className="text-[11px] text-fg-3">Read only</span>
        ),
    },
  ];
  const historyCols: Column<RoutingSuggestion>[] = [
    { key: "st", header: "Status", cell: (s) => <Chip size="sm" tone={STATUS_TONE[s.status]}>{s.status}</Chip>, sort: (s) => s.status, csv: (s) => s.status },
    ...base,
    {
      key: "d",
      header: "Decided",
      cell: (s) => (
        <span className="block whitespace-nowrap text-[12px]">
          {s.decidedBy ?? (s.status === "expired" ? "Expired" : "—")}
          <span className="block font-mono text-[10.5px] text-fg-3">{s.decidedAt ? serverStamp(s.decidedAt) : serverStamp(s.createdAt)}</span>
          {s.note && (
            <span className="block max-w-[200px] truncate text-[11px] text-fg-3" title={s.note}>
              “{s.note}”
            </span>
          )}
        </span>
      ),
      sort: (s) => s.decidedAt ?? s.createdAt,
      csv: (s) => `${s.decidedBy ?? ""} ${s.decidedAt ?? ""} ${s.note ?? ""}`,
    },
    {
      key: "res",
      header: "Result",
      cell: (s) =>
        s.result ? (
          <span className="flex flex-col gap-0.5 text-[11.5px]">
            {s.result.done.length > 0 && <span className="text-up">{s.result.done.length} moved{s.result.done.length <= 3 ? `: ${s.result.done.map((t) => `#${t}`).join(" ")}` : ""}</span>}
            {s.result.failed.map((f) => (
              <span key={f.ticket} className="text-down" title={f.error}>
                #{f.ticket} failed: {f.error}
              </span>
            ))}
            {!s.result.done.length && !s.result.failed.length && <span className="text-fg-3">Nothing to move</span>}
          </span>
        ) : (
          <span className="text-fg-3">—</span>
        ),
      csv: (s) => (s.result ? `done ${s.result.done.join(" ")}; failed ${s.result.failed.map((f) => f.ticket).join(" ")}` : ""),
    },
  ];

  const err = tab === "pending" ? pending.error : all.error;
  const loading = tab === "pending" ? !pending.data && pending.loading : !all.data && all.loading;

  return (
    <div className="pb-10">
      <PageHeader
        title="Routing suggestions"
        subtitle={<span className="inline-flex flex-wrap items-center gap-2">Book moves proposed by the routing rules and client scores, decided by a dealer. Every decision is reason-coded and audited. <DeskStatusChip /></span>}
        actions={
          <Link href="/trading/routing" className={buttonVariants({ variant: "surface", size: "lg" })}>
            <RouteIcon /> Routing rules
          </Link>
        }
      />
      <Reveal>
        <div className={cn("mb-4 flex flex-wrap items-center gap-3 rounded-[16px] border px-5 py-3.5", mode === "execute" ? "border-down/30 bg-down-soft" : "border-warn/30 bg-warn-soft")}>
          <Info className={cn("size-4", mode === "execute" ? "text-down" : "text-warn")} />
          <span className="flex-1 text-[13px]">
            {mode === "execute" ? (
              <>
                <span className="font-medium">Execute mode.</span> <span className="text-fg-2">Routing rules move matching trades themselves; suggestions here come from client scores and rules that need a dealer.</span>
              </>
            ) : (
              <>
                <span className="font-medium">Recommend mode.</span> <span className="text-fg-2">The engine keeps every trade on the default book (100% B-book at launch) and only proposes moves. Approving one transfers the position (or the account's open positions plus a quick route for new trades).</span>
              </>
            )}
          </span>
          <Chip tone={mode === "execute" ? "down" : "warn"} dot>
            {mode === null ? "Mode loading…" : mode === "execute" ? "Execute" : "Recommend"}
          </Chip>
          <Link href="/trading/risk-limits" className="text-[12.5px] text-fg-2 hover:text-fg">
            Change mode
          </Link>
        </div>
      </Reveal>
      <div className="grid grid-cols-2 gap-4 xl:grid-cols-4">
        <KpiCard label="Pending" icon={<Clock />} value={<span className="k-num">{queue.length}</span>} chip={`${toA} to A-book · ${queue.length - toA} to B-book`} chipTone="warn" />
        <KpiCard label="Accounts" icon={<RouteIcon />} value={<span className="k-num">{queue.filter((s) => s.kind === "account").length}</span>} chip="Whole-account moves" delay={0.04} />
        <KpiCard label="Approved today" icon={<Check />} value={<span className="k-num text-up">{decidedToday.filter((s) => s.status === "approved").length}</span>} chip={`${history.filter((s) => s.status === "approved").length} in recent history`} chipTone="up" delay={0.08} />
        <KpiCard label="Rejected / expired" icon={<X />} value={<span className="k-num">{decidedToday.filter((s) => s.status !== "approved").length}</span>} chip="Today" delay={0.12} />
      </div>
      <Reveal delay={0.06} className="mt-4">
        <Card className="px-4 py-5 sm:px-6">
          <Tabs
            className="mb-5"
            value={tab}
            onChange={setTab}
            tabs={[
              { value: "pending", label: "Pending", count: queue.length },
              { value: "history", label: "History", count: history.length },
            ]}
          />
          {IS_DEMO ? (
            <ConnectEngineState what="Routing suggestions" />
          ) : err ? (
            <RiskError error={err} onRetry={tab === "pending" ? pending.reload : all.reload} />
          ) : loading ? (
            <TableSkeleton rows={6} />
          ) : tab === "pending" ? (
            <DataTable
              columns={pendingCols}
              rows={queue}
              dense
              pageSize={20}
              rowKey={(s) => s.id}
              exportName="routing-suggestions"
              search={(s) => `${s.login} ${s.ticket ?? ""} ${s.symbol ?? ""} ${liveClientName(s.userId, s.login)} ${s.rule ?? ""}`}
              searchPlaceholder="Search login, ticket, symbol…"
              empty={<EmptyState title="No pending suggestions" text="Suggestions appear when a routing rule or a client score proposes moving a trade or an account to another book." illustration="check_mark_button" />}
            />
          ) : (
            <DataTable
              columns={historyCols}
              rows={history}
              dense
              pageSize={20}
              rowKey={(s) => s.id}
              exportName="routing-suggestions-history"
              search={(s) => `${s.login} ${s.ticket ?? ""} ${s.symbol ?? ""} ${s.status} ${s.decidedBy ?? ""} ${liveClientName(s.userId, s.login)}`}
              searchPlaceholder="Search history…"
              empty={<EmptyState title="No decisions yet" text="Approved, rejected and expired suggestions are kept here with their result." illustration="receipt" />}
            />
          )}
        </Card>
      </Reveal>

      <DeskDialog
        open={!!act}
        onOpenChange={(o) => !o && setAct(null)}
        title={act ? (act.op === "approve" ? `Approve move to ${act.s.to}-book` : "Reject suggestion") : ""}
        description={
          act
            ? act.op === "approve"
              ? act.s.kind === "position"
                ? `Transfers ticket #${act.s.ticket} (${act.s.symbol}) from ${act.s.from}-book to ${act.s.to}-book at the market price.`
                : `Transfers every open position of ${act.s.login} to ${act.s.to}-book and routes the account's new trades there (quick route).`
              : "The trade stays on its current book. The suggestion is closed with your reason."
            : undefined
        }
        defaultCode={RISK_CODE}
        confirmLabel={act?.op === "approve" ? "Approve and move" : "Reject"}
        confirmVariant={act?.op === "approve" ? "ember" : "surface"}
        onConfirm={async (reason) => {
          const r = await tradingWrite<{ suggestion: RoutingSuggestion; done?: string[]; failed?: { ticket: string; error: string }[] }>(`dealing/routing/suggestions/${act!.s.id}/${act!.op}`, {}, reason, rest);
          if (r.ok) {
            pending.reload();
            all.reload();
          }
          return r;
        }}
        success={(d) => (act?.op === "approve" ? `Approved · ${d?.done?.length ?? 0} moved${d?.failed?.length ? ` · ${d.failed.length} failed` : ""}` : "Suggestion rejected")}
      >
        {act && (
          <div className="k-row flex flex-wrap items-center justify-between gap-3 px-3.5 py-3">
            <LoginCell login={act.s.login} userId={act.s.userId} />
            <Move s={act.s} />
            <div className="w-full">
              <Reasons s={act.s} />
            </div>
          </div>
        )}
      </DeskDialog>
    </div>
  );
}
