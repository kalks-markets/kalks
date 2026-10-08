"use client";

import * as React from "react";
import Link from "next/link";
import { Archive, ArchiveRestore, ArrowLeftRight, Ban, CandlestickChart, CirclePause, CirclePlay, Coins, Gauge as GaugeIcon, Gift, Layers, List, Lock, LockOpen, MoreHorizontal, RefreshCw, Search, ShieldAlert, SlidersHorizontal, UserRound, Wallet } from "lucide-react";
import { Button, Card, Chip, DataTable, Dialog, EmptyState, Field, Input, KpiCard, Menu, PageHeader, Reveal, Segmented, SymbolCell, Tabs, Toggle, cn, formatNumber, type Column } from "@kalks/ui";
import { ErrorState, FilterSelect, Pager, TableSkeleton, ago, qs, useApi, useDebounced, useNow, when } from "@/components/live/kit";
import { useCan, useStaff } from "@/components/staff-session";
import { BulkDialog, BulkMenu, CloseAccountDialog, ReopenAccountDialog, type BulkKind } from "./account-ops";
import { ProductChip } from "./groups";
import { MiniClient, SideChip, fmtPrice } from "@/components/trading/shared";
import { BookChip, DeskDialog, MetaTile } from "@/components/trading-desk/kit";
import { CreateTradeDrawer } from "@/components/trading-desk/create-trade";
import { PositionDrawer } from "@/components/trading-desk/position-drawer";
import { DeskStatusChip } from "@/components/trading-desk/status";
import { actionText } from "@/components/trading-desk/labels";
import { ChangeLine } from "@/components/trading-desk/position-drawer";
import {
  clientName,
  groupLabel,
  liveClientEmail,
  normAccount,
  productOf,
  serverStamp,
  toUsdOf,
  upsertAccounts,
  useDesk,
  useLiveDirectory,
  useRestDesk,
  type Book,
  type DeskOrder,
  type DeskPosition,
  type LiveAccount,
} from "@/lib/trading-desk";
import { AdjustDialog } from "@/components/clients/adjust-dialog";
import { ACC_ARCHIVE_REASONS, ACC_REASONS, ACC_RESTORE_REASONS, FIN_REASONS, KIND_LABEL, KIND_TONE, LEDGER_KIND, LIFECYCLE_STATUSES, SETTABLE_STATUS, STATUS_LABEL, STATUS_TONE, accountKind, money2, signed2, tradingWrite, type AccountKind, type History, type Ledger } from "./kit";

const PER = 50;
const KIND_SCAN = 500; // engine maximum page size
type RawAccount = Parameters<typeof normAccount>[0];
type AccountsPage = { items: RawAccount[]; page: number; limit: number; total: number };
type Summary = { accounts: { live: number; demo: number }; positions: { total: number; demo: number; A: { positions: number; lots: number; floating: number }; B: { positions: number; lots: number; floating: number } } };

const level = (a: LiveAccount) => (a.margin > 0 && a.marginLevel !== null ? a.marginLevel : Infinity);

function MlCell({ a }: { a: LiveAccount }) {
  const v = level(a);
  if (v === Infinity) return <span className="text-fg-3">—</span>;
  const tone = v <= a.stopOutLevel ? "down" : v <= a.marginCallLevel ? "warn" : v < 300 ? "gold" : "up";
  return (
    <span className="inline-flex items-center gap-2">
      <span className="h-1.5 w-14 overflow-hidden rounded-full bg-surface-3">
        <span className={cn("block h-full rounded-full", { down: "bg-down", warn: "bg-warn", gold: "bg-gold", up: "bg-up" }[tone])} style={{ width: `${Math.max(6, Math.min(100, v / 10))}%` }} />
      </span>
      <span className={cn("k-num w-16 text-right font-mono text-[12px]", { down: "text-down", warn: "text-warn", gold: "text-gold", up: "text-fg-2" }[tone])}>{formatNumber(v, 0)}%</span>
    </span>
  );
}

export function StatusChip({ status }: { status: string }) {
  return (
    <Chip size="sm" dot tone={STATUS_TONE[status] ?? "neutral"}>
      {STATUS_LABEL[status] ?? status}
    </Chip>
  );
}

/** Product chip (Copy / PAMM / MAM / Prop) from the group code; nothing for regular accounts. */
export function KindChip({ group }: { group: string }) {
  const k = accountKind(group);
  if (k === "regular") return null;
  return (
    <Chip size="sm" tone={KIND_TONE[k]}>
      {KIND_LABEL[k]}
    </Chip>
  );
}

/** Archived / closed accounts are retired: no trading, no generic status change; archived ones can be restored. */
const retired = (a: LiveAccount) => LIFECYCLE_STATUSES.includes(a.status);

type Act = { k: "adjust" | "funds" | "status" | "group" | "leverage" | "trade" | "controls" | "archive" | "restore" | "close" | "reopen"; a: LiveAccount } | { k: "route"; a: LiveAccount; book: Book | null } | null;

/** Lifecycle extras the engine adds to account rows (closure request, dormancy, default star). */
type Extras = { dormantSince?: string | null; closureRequest?: { id: number; status: string } | null; lastActivityAt?: string | null; anonymised?: boolean };
const ext = (a: LiveAccount) => a as LiveAccount & Extras;
const isSuper = (role: string) => role === "super_admin" || role === "platform_owner";

/* ------------------------------------------------------------------ */
/* Page                                                                */
/* ------------------------------------------------------------------ */

export function LiveAccountsPage() {
  const now = useNow();
  const dir = useLiveDirectory();
  const [q, setQ] = React.useState("");
  const [type, setType] = React.useState<"all" | "live" | "demo">("all");
  const [status, setStatus] = React.useState<string>("all");
  const [group, setGroup] = React.useState<string>("all");
  const [kind, setKind] = React.useState<"all" | AccountKind>("all");
  const [life, setLife] = React.useState<"all" | "dormant">("all");
  const [product, setProduct] = React.useState<"all" | "cfd" | "options">("all");
  const [bulk, setBulk] = React.useState<BulkKind | null>(null);
  const [page, setPage] = React.useState(1);
  const [open, setOpen] = React.useState<string | null>(null);
  const [act, setAct] = React.useState<Act>(null);
  const dq = useDebounced(q.trim(), 300);
  React.useEffect(() => setPage(1), [dq, type, status, group, kind, life, product]);
  React.useEffect(() => {
    const l = new URLSearchParams(window.location.search).get("login");
    if (l) setOpen(l);
  }, []);

  // the engine has no product-kind filter: with a kind selected, the latest KIND_SCAN accounts are fetched and filtered here
  const byKind = kind !== "all";
  const { data, error, loading, reload } = useApi<AccountsPage>(`/api/trading/admin/accounts${qs({ q: dq, type, status, group, product, dormant: life === "dormant" ? true : undefined, page: byKind ? 1 : page, limit: byKind ? KIND_SCAN : PER })}`, { refreshMs: byKind ? 15_000 : 5000 });
  const sum = useApi<Summary>("/api/trading/summary", { refreshMs: 15_000 });
  const fetched = React.useMemo(() => (data?.items ?? []).map(normAccount), [data]);
  React.useEffect(() => upsertAccounts(fetched), [fetched]);
  const rows = React.useMemo(() => (byKind ? fetched.filter((a) => accountKind(a.group) === kind) : fetched), [fetched, byKind, kind]);
  const all = [...dir.accounts.values()];
  const atRisk = all.filter((a) => level(a) <= a.marginCallLevel).length;
  const liveEquity = all.filter((a) => a.type === "live").reduce((s, a) => s + toUsdOf(a.equity, a.currency), 0);

  const cols: Column<LiveAccount>[] = [
    {
      key: "l",
      header: "Login",
      cell: (r) => (
        <span className="flex items-center gap-1.5">
          <span className="font-mono text-[12.5px] font-medium">{r.login}</span>
          {r.type === "demo" && <Chip size="sm" tone="info">Demo</Chip>}
          <KindChip group={r.group} />
          {productOf(r) === "options" && <ProductChip product="options" />}
        </span>
      ),
    },
    { key: "c", header: "Client", cell: (r) => <MiniClient clientId={r.userId} login={`#${r.userId}`} /> },
    { key: "g", header: "Group", cell: (r) => <span className="whitespace-nowrap text-[12.5px]">{r.groupName}<span className="block text-[10.5px] text-fg-3">{r.mode}{r.cent ? " · cent" : ""}</span></span> },
    { key: "lev", header: "Leverage", align: "right", cell: (r) => <span className="k-num font-mono text-[12px]">1:{r.leverage}</span> },
    { key: "b", header: "Balance", align: "right", cell: (r) => <span className="k-num whitespace-nowrap font-mono text-[12.5px]">{money2(r.balance, r.currency)}</span> },
    { key: "e", header: "Equity", align: "right", cell: (r) => <span className="k-num whitespace-nowrap font-mono text-[12.5px]">{money2(r.equity, r.currency)}</span> },
    { key: "cr", header: "Credit · bonus", align: "right", hideOn: "xl", cell: (r) => <span className="k-num whitespace-nowrap font-mono text-[12px] text-fg-3">{r.credit || r.bonus ? `${money2(r.credit, r.currency)} · ${money2(r.bonus, r.currency)}` : "—"}</span> },
    { key: "ml", header: "Margin level", align: "right", cell: (r) => <MlCell a={r} /> },
    { key: "p", header: "Open", align: "right", cell: (r) => <span className="k-num text-[12px]">{r.positions}<span className="text-fg-3"> · {r.orders}</span></span> },
    { key: "r", header: "Book", align: "center", cell: (r) => <BookChip book={r.route} /> },
    {
      key: "st",
      header: "Status",
      cell: (r) => (
        <span className="flex flex-wrap items-center gap-1">
          <StatusChip status={r.status} />
          {r.controls.tradingDisabled && <Chip size="sm" tone="down">Dealer: off</Chip>}
          {r.controls.closeOnly && <Chip size="sm" tone="warn">Dealer: close-only</Chip>}
          {r.marginCall && <Chip size="sm" tone="warn">Margin call</Chip>}
          {ext(r).dormantSince && <Chip size="sm" tone="warn">Dormant</Chip>}
          {ext(r).closureRequest?.status === "pending" && <Chip size="sm" tone="info">Closure pending</Chip>}
          {ext(r).anonymised && <Chip size="sm">Anonymised</Chip>}
        </span>
      ),
    },
    { key: "cd", header: "Opened", align: "right", hideOn: "xl", cell: (r) => <span className="whitespace-nowrap text-[11.5px] text-fg-3" title={when(r.createdAt)}>{ago(r.createdAt, now)}</span> },
    {
      key: "a",
      header: "",
      align: "right",
      cell: (r) => (
        <span onClick={(e) => e.stopPropagation()}>
          <AccountMenu a={r} onOpen={() => setOpen(r.login)} onAct={setAct} />
        </span>
      ),
    },
  ];

  const filtered = !!dq || type !== "all" || status !== "all" || group !== "all" || byKind || life !== "all" || product !== "all";
  const filters = { q: dq, type, status, group, product, dormant: life === "dormant" ? true : undefined };
  return (
    <div className="pb-10">
      <PageHeader
        title="Trading accounts"
        subtitle={<span className="inline-flex flex-wrap items-center gap-2">Live and demo accounts on the trading engine — balances, margin, status, group and leverage. <DeskStatusChip /></span>}
        actions={
          <div className="flex flex-wrap items-center gap-2">
            <BulkMenu onPick={setBulk} />
            <Button variant="surface" size="lg" onClick={() => (reload(), sum.reload())}>
              <RefreshCw /> Refresh
            </Button>
          </div>
        }
      />
      <div className="grid grid-cols-2 gap-4 xl:grid-cols-4">
        <KpiCard label="Live accounts" icon={<Wallet />} value={<span className="k-num">{sum.data?.accounts.live ?? "—"}</span>} chip={`${sum.data?.accounts.demo ?? "—"} demo`} chipTone="info" />
        <KpiCard label="Live equity" icon={<GaugeIcon />} value={<span className="k-num">{money2(liveEquity)}</span>} chip="USD equivalent" delay={0.04} />
        <KpiCard label="At or below margin call" icon={<ShieldAlert />} value={<span className={cn("k-num", atRisk ? "text-warn" : undefined)}>{atRisk}</span>} chip="Watch list" chipTone="warn" delay={0.08} />
        <KpiCard label="Open positions" icon={<Layers />} value={<span className="k-num">{sum.data?.positions.total ?? "—"}</span>} chip={sum.data ? `${sum.data.positions.A.positions} A · ${sum.data.positions.B.positions} B` : "—"} delay={0.12} />
      </div>
      <Reveal delay={0.08} className="mt-4">
        <Card className="px-4 py-5 sm:px-6">
          <div className="mb-4 flex flex-wrap items-center gap-2">
            <div className="flex h-9 w-full min-w-0 items-center gap-2 rounded-full border border-line bg-surface-2 px-3.5 sm:w-auto sm:max-w-xs sm:flex-1">
              <Search className="size-3.5 shrink-0 text-fg-3" />
              <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Login, name or client ID" className="min-w-0 flex-1 bg-transparent text-[13px] outline-none placeholder:text-fg-3" aria-label="Search accounts" />
            </div>
            <Segmented size="xs" value={type} onChange={setType} options={[{ value: "all", label: "Live + demo" }, { value: "live", label: "Live" }, { value: "demo", label: "Demo" }]} />
            <FilterSelect label="Product" value={product} onChange={(v) => setProduct(v as typeof product)} options={[{ value: "all", label: "CFD + Options" }, { value: "cfd", label: "CFD" }, { value: "options", label: "Options" }]} />
            <FilterSelect label="Group" value={group} onChange={setGroup} options={[{ value: "all", label: "All groups" }, ...dir.groups.filter((g) => product === "all" || productOf(g) === product).map((g) => ({ value: g.code, label: g.name }))]} />
            <FilterSelect label="Status" value={status} onChange={setStatus} options={[{ value: "all", label: "Any status" }, ...Object.entries(STATUS_LABEL).map(([value, label]) => ({ value, label }))]} />
            <FilterSelect label="Kind" value={kind} onChange={(v) => setKind(v as typeof kind)} options={[{ value: "all", label: "All" }, ...(["copy", "pamm", "mam", "prop", "regular"] as const).map((k) => ({ value: k, label: KIND_LABEL[k] }))]} />
            <FilterSelect label="Activity" value={life} onChange={(v) => setLife(v as typeof life)} options={[{ value: "all", label: "Any" }, { value: "dormant", label: "Dormant only" }]} />
            {filtered && (
              <Button size="xs" variant="ghost" onClick={() => (setQ(""), setType("all"), setStatus("all"), setGroup("all"), setKind("all"), setLife("all"), setProduct("all"))}>
                Clear filters
              </Button>
            )}
          </div>
          {error ? (
            <ErrorState error={error} onRetry={reload} />
          ) : !data ? (
            <TableSkeleton />
          ) : (
            <div className={loading ? "opacity-90 transition-opacity" : "transition-opacity"}>
              <DataTable
                columns={cols}
                rows={rows}
                dense
                pageSize={PER}
                rowKey={(r) => r.login}
                onRowClick={(r) => setOpen(r.login)}
                exportName="trading-accounts"
                empty={filtered ? <EmptyState title="No accounts match" text="Try a different search or clear the filters." illustration="magnifying_glass_tilted_left" /> : <EmptyState title="No trading accounts yet" text="Accounts opened in the Client Area appear here." illustration="bank" />}
              />
              {byKind ? (
                data.total > data.items.length && <p className="mt-3 text-[11.5px] text-fg-3">{KIND_LABEL[kind as AccountKind]} accounts among the latest {formatNumber(data.items.length, 0)} of {formatNumber(data.total, 0)} — narrow it with search, group or status to reach older accounts.</p>
              ) : (
                <Pager page={data.page} perPage={data.limit} total={data.total} onPage={setPage} />
              )}
            </div>
          )}
        </Card>
      </Reveal>
      <AccountDrawer login={open} onClose={() => setOpen(null)} onAct={setAct} />
      <AccountActions act={act} onClose={() => setAct(null)} onDone={reload} />
      <BulkDialog kind={bulk} filters={filters} onClose={() => setBulk(null)} onDone={reload} />
    </div>
  );
}

function AccountMenu({ a, onOpen, onAct }: { a: LiveAccount; onOpen?: () => void; onAct: (x: Act) => void }) {
  const canFunds = useCan("finance.adjust");
  const canCredit = useCan("finance.credit");
  const canAcc = useCan("accounts.write");
  const canDeal = useCan("dealing.write");
  const canClose = useCan("accounts.close");
  const superAdmin = isSuper(useStaff().role);
  const items = [
    ...(onOpen ? [{ label: "Open account", icon: <UserRound />, onSelect: onOpen }] : []),
    ...(canFunds || canCredit ? [{ label: "Balance & credit", icon: <Coins />, onSelect: () => onAct({ k: "adjust", a }) }] : []),
    ...(canFunds ? [{ label: "Bonus", icon: <Gift />, onSelect: () => onAct({ k: "funds", a }) }] : []),
    ...(canAcc && !retired(a)
      ? [
          { label: "Change status", icon: <CirclePause />, onSelect: () => onAct({ k: "status", a }) },
          { label: "Change group", icon: <Layers />, onSelect: () => onAct({ k: "group", a }) },
          { label: "Change leverage", icon: <SlidersHorizontal />, onSelect: () => onAct({ k: "leverage", a }) },
          { label: "Archive account", icon: <Archive />, onSelect: () => onAct({ k: "archive", a }) },
        ]
      : []),
    ...(canAcc && a.status === "archived" ? [{ label: "Restore account", icon: <ArchiveRestore />, onSelect: () => onAct({ k: "restore", a }) }] : []),
    ...(canClose && a.status !== "closed" ? [{ label: "Close permanently…", icon: <Lock />, danger: true, onSelect: () => onAct({ k: "close", a }) }] : []),
    ...(superAdmin && a.status === "closed" ? [{ label: "Reopen account…", icon: <LockOpen />, onSelect: () => onAct({ k: "reopen", a }) }] : []),
    ...(canDeal && !retired(a)
      ? [
          "sep" as const,
          { label: "Create trade", icon: <CandlestickChart />, onSelect: () => onAct({ k: "trade", a }) },
          { label: "Dealer controls", icon: <Ban />, onSelect: () => onAct({ k: "controls", a }) },
          { label: `Route new trades to ${a.route === "A" ? "B" : "A"}-book`, icon: <ArrowLeftRight />, onSelect: () => onAct({ k: "route", a, book: a.route === "A" ? "B" : "A" }) },
          { label: "Follow routing rules", icon: <ArrowLeftRight />, onSelect: () => onAct({ k: "route", a, book: null }) },
        ]
      : []),
    "sep" as const,
    { label: "Open positions", icon: <List />, href: `/trading?login=${a.login}` },
  ];
  return (
    <Menu
      width={230}
      items={items}
      trigger={
        <button className="grid size-7 place-items-center rounded-full text-fg-3 hover:bg-surface-3 hover:text-fg" aria-label={`Actions for ${a.login}`}>
          <MoreHorizontal className="size-4" />
        </button>
      }
    />
  );
}

/* ------------------------------------------------------------------ */
/* Account drawer                                                      */
/* ------------------------------------------------------------------ */

type Detail = { account: RawAccount; positions: DeskPosition[]; orders: DeskOrder[]; lastActivityAt: string | null };
type DTab = "positions" | "orders" | "history" | "ledger" | "audit";

export function AccountDrawer({ login, onClose, onAct }: { login: string | null; onClose: () => void; onAct: (x: Act) => void }) {
  const now = useNow();
  const { state } = useDesk();
  const [tab, setTab] = React.useState<DTab>("positions");
  const [pos, setPos] = React.useState<string | null>(null);
  const { data, error, reload } = useApi<Detail>(login ? `/api/trading/admin/accounts/${login}` : null, { refreshMs: 4000 });
  React.useEffect(() => setTab("positions"), [login]);
  // refresh at once after any staff action on this login (audit entries arrive over the dealing stream)
  const lastAudit = state.audit.find((e) => e.login === login)?.id;
  React.useEffect(() => {
    if (lastAudit) reload();
  }, [lastAudit, reload]);
  const a = data && String(data.account.login) === login ? normAccount(data.account) : null;
  React.useEffect(() => {
    if (a) upsertAccounts([a]);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [data]);
  // positions and orders stream live through the desk; the detail call is the fallback before the desk loads
  const positions = login ? (state.positions.some((p) => p.login === login) || !data ? state.positions.filter((p) => p.login === login) : data.positions) : [];
  const orders = login ? state.orders.filter((o) => o.login === login) : [];
  const ccy = a?.currency ?? "USD";

  return (
    <Dialog
      side="right"
      open={!!login}
      onOpenChange={(o) => !o && onClose()}
      title={
        <span className="flex items-center gap-2">
          Account <span className="font-mono">{login}</span>
          {a && <StatusChip status={a.status} />}
          {a?.type === "demo" && <Chip size="sm" tone="info">Demo</Chip>}
          {a && <KindChip group={a.group} />}
          {a && <ProductChip product={a.product} />}
        </span>
      }
      description={a ? `${clientName(a.userId, a.login)}${liveClientEmail(a.userId) ? ` · ${liveClientEmail(a.userId)}` : ""} · client #${a.userId}` : "Loading…"}
      footer={a ? <AccountButtons a={a} onAct={onAct} /> : undefined}
    >
      {error ? (
        <ErrorState error={error} onRetry={reload} />
      ) : !a ? (
        <TableSkeleton rows={4} />
      ) : (
        <div className="space-y-5">
          <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
            <MetaTile label="Balance" value={money2(a.balance, ccy)} />
            <MetaTile label="Equity" value={money2(a.equity, ccy)} />
            <MetaTile label="Credit" value={money2(a.credit, ccy)} />
            <MetaTile label="Bonus" value={money2(a.bonus, ccy)} />
            <MetaTile label="Floating" value={signed2(a.profit + a.swap, ccy)} tone={a.profit + a.swap >= 0 ? "up" : "down"} />
            <MetaTile label="Margin" value={money2(a.margin, ccy)} />
            <MetaTile label="Free margin" value={money2(a.freeMargin, ccy)} tone={a.freeMargin < 0 ? "down" : undefined} />
            <MetaTile label="Margin level" value={a.marginLevel === null ? "—" : `${formatNumber(a.marginLevel, 0)}%`} tone={level(a) <= a.stopOutLevel ? "down" : level(a) <= a.marginCallLevel ? "warn" : undefined} />
          </div>
          <div className="grid grid-cols-2 gap-x-6 gap-y-1.5 rounded-[14px] border border-line bg-surface-2/60 px-4 py-3 text-[12.5px] sm:grid-cols-3">
            <Info k="Group" v={`${a.groupName} (${a.group})`} />
            <Info k="Mode" v={`${a.mode}${a.cent ? " · cent (USC)" : ""}`} />
            <Info k="Leverage" v={`1:${a.leverage}`} />
            <Info k="New trades" v={<BookChip book={a.route} />} />
            <Info k="MC / SO" v={`${a.marginCallLevel}% / ${a.stopOutLevel}%`} />
            <Info k="Withdrawable" v={money2(a.withdrawable, ccy)} />
            <Info k="Opened" v={when(a.createdAt)} />
            <Info k="Last activity" v={data?.lastActivityAt ? ago(data.lastActivityAt, now) : "—"} />
            <Info k="Dealer controls" v={[a.controls.tradingDisabled && "trading off", a.controls.closeOnly && "close-only", a.controls.maxLot && `max ${a.controls.maxLot} lots`, a.controls.execDelayMs > 0 && `${a.controls.execDelayMs} ms delay`, a.controls.markupPips && `+${a.controls.markupPips} pips`].filter(Boolean).join(" · ") || "none"} />
            {a.demo && <Info k="Demo" v={`initial ${money2(a.demo.initialBalance, ccy)} · ${a.demo.refillsUsedToday}/${a.demo.refillsPerDay} refills · expires after ${a.demo.expiryDays} d idle`} />}
          </div>
          <div className="flex flex-wrap gap-2 text-[12px]">
            <Link href={`/clients/${a.userId}`} className="inline-flex items-center gap-1 rounded-full border border-line bg-surface-2 px-2.5 py-1 text-fg-2 hover:text-fg">
              <UserRound className="size-3" /> Client profile
            </Link>
            <Link href={`/trading?login=${a.login}`} className="inline-flex items-center gap-1 rounded-full border border-line bg-surface-2 px-2.5 py-1 text-fg-2 hover:text-fg">
              <List className="size-3" /> Positions page
            </Link>
          </div>

          <Tabs
            value={tab}
            onChange={setTab}
            tabs={[
              { value: "positions", label: "Positions", count: positions.length },
              { value: "orders", label: "Orders", count: orders.length },
              { value: "history", label: "History" },
              { value: "ledger", label: "Ledger" },
              { value: "audit", label: "Audit" },
            ]}
          />
          {tab === "positions" ? (
            <PositionsList rows={positions} onOpen={setPos} />
          ) : tab === "orders" ? (
            <OrdersList rows={orders} />
          ) : tab === "history" ? (
            <HistoryTab login={a.login} ccy={ccy} />
          ) : tab === "ledger" ? (
            <LedgerTab login={a.login} />
          ) : (
            <AuditTab login={a.login} />
          )}
        </div>
      )}
      <PositionDrawer ticket={pos} onOpenChange={(o) => !o && setPos(null)} onSelectTicket={setPos} />
    </Dialog>
  );
}

function Info({ k, v }: { k: string; v: React.ReactNode }) {
  return (
    <div className="flex min-w-0 items-center justify-between gap-2 sm:block">
      <div className="text-[10.5px] uppercase tracking-wider text-fg-3">{k}</div>
      <div className="break-words text-fg-2">{v}</div>
    </div>
  );
}

function AccountButtons({ a, onAct }: { a: LiveAccount; onAct: (x: Act) => void }) {
  const canFunds = useCan("finance.adjust");
  const canCredit = useCan("finance.credit");
  const canAcc = useCan("accounts.write");
  const canDeal = useCan("dealing.write");
  const canClose = useCan("accounts.close");
  const superAdmin = isSuper(useStaff().role);
  return (
    <div className="flex w-full flex-wrap items-center justify-end gap-2">
      {superAdmin && a.status === "closed" && (
        <Button size="sm" variant="gold" onClick={() => onAct({ k: "reopen", a })}>
          <LockOpen /> Reopen…
        </Button>
      )}
      {canClose && a.status !== "closed" && (
        <Button size="sm" variant="surface" onClick={() => onAct({ k: "close", a })}>
          <Lock /> Close permanently…
        </Button>
      )}
      {(canFunds || canCredit) && (
        <Button size="sm" variant="gold" onClick={() => onAct({ k: "adjust", a })}>
          <Coins /> Balance & credit
        </Button>
      )}
      {canAcc && a.status === "archived" && (
        <Button size="sm" variant="gold" onClick={() => onAct({ k: "restore", a })}>
          <ArchiveRestore /> Restore account
        </Button>
      )}
      {canAcc && !retired(a) && (
        <>
          <Button size="sm" variant="surface" onClick={() => onAct({ k: "archive", a })}>
            <Archive /> Archive
          </Button>
          <Button size="sm" variant="surface" onClick={() => onAct({ k: "status", a })}>
            <CirclePause /> Status
          </Button>
          <Button size="sm" variant="surface" onClick={() => onAct({ k: "group", a })}>
            <Layers /> Group
          </Button>
          <Button size="sm" variant="surface" onClick={() => onAct({ k: "leverage", a })}>
            <SlidersHorizontal /> Leverage
          </Button>
        </>
      )}
      {canDeal && !retired(a) && (
        <>
          <Button size="sm" variant="surface" onClick={() => onAct({ k: "controls", a })}>
            <Ban /> Controls
          </Button>
          <Button size="sm" variant="ember" onClick={() => onAct({ k: "trade", a })}>
            <CandlestickChart /> Create trade
          </Button>
        </>
      )}
    </div>
  );
}

function PositionsList({ rows, onOpen }: { rows: DeskPosition[]; onOpen: (t: string) => void }) {
  const cols: Column<DeskPosition>[] = [
    { key: "t", header: "Ticket", cell: (r) => <span className="font-mono text-[12px]">{r.ticket}</span> },
    { key: "s", header: "Symbol", cell: (r) => <SymbolCell symbol={r.symbol} size={20} sub={<SideChip side={r.side} volume={r.volume} />} /> },
    { key: "o", header: "Open · now", align: "right", cell: (r) => <span className="k-num whitespace-nowrap font-mono text-[12px]">{fmtPrice(r.symbol, r.openPrice)}<span className="block text-[11px] text-fg-3">{r.currentPrice ? fmtPrice(r.symbol, r.currentPrice) : "—"}</span></span> },
    { key: "p", header: "P&L", align: "right", cell: (r) => { const v = (r.profit ?? 0) + r.swap - r.commission; return <span className={cn("k-num font-mono text-[12px]", v >= 0 ? "text-up" : "text-down")}>{signed2(v, r.currency)}</span>; } },
    { key: "b", header: "Book", align: "center", cell: (r) => <BookChip book={r.route} /> },
  ];
  return <DataTable columns={cols} rows={rows} dense pageSize={10} rowKey={(r) => r.ticket} onRowClick={(r) => onOpen(r.ticket)} empty={<EmptyState title="No open positions" text="Dealer trades and client trades on this account appear here live." illustration="chart_increasing" className="py-8" />} />;
}

function OrdersList({ rows }: { rows: DeskOrder[] }) {
  const cols: Column<DeskOrder>[] = [
    { key: "t", header: "Ticket", cell: (r) => <span className="font-mono text-[12px]">{r.ticket}</span> },
    { key: "s", header: "Symbol", cell: (r) => <SymbolCell symbol={r.symbol} size={20} sub={r.type} /> },
    { key: "v", header: "Volume", align: "right", cell: (r) => <span className="k-num font-mono text-[12px]">{r.volume}</span> },
    { key: "p", header: "Price", align: "right", cell: (r) => <span className="k-num font-mono text-[12px]">{fmtPrice(r.symbol, r.price)}</span> },
    { key: "e", header: "Expiry", cell: (r) => <span className="text-[12px] text-fg-2">{r.expiry}</span> },
  ];
  return <DataTable columns={cols} rows={rows} dense pageSize={10} rowKey={(r) => r.ticket} empty={<EmptyState title="No pending orders" text="Limit, stop and stop-limit orders appear here." illustration="receipt" className="py-8" />} />;
}

function HistoryTab({ login, ccy }: { login: string; ccy: string }) {
  const [page, setPage] = React.useState(1);
  const { data, error, reload } = useApi<History>(`/api/trading/accounts/${login}/history${qs({ page, limit: 25 })}`, { refreshMs: 10_000 });
  if (error) return <ErrorState error={error} onRetry={reload} />;
  if (!data) return <TableSkeleton rows={4} />;
  const cols: Column<History["deals"][number]>[] = [
    { key: "t", header: "Time (GMT+3)", cell: (r) => <span className="whitespace-nowrap font-mono text-[11.5px] text-fg-2">{serverStamp(r.time)}<span className="block text-[10.5px] text-fg-3">deal {r.id} · #{r.positionTicket}</span></span> },
    { key: "s", header: "Deal", cell: (r) => <span className="whitespace-nowrap text-[12px]">{r.symbol} <span className={r.side === "buy" ? "text-up" : "text-down"}>{r.side}</span> <span className="text-fg-3">{r.entry}</span><span className="block text-[10.5px] text-fg-3">{r.reason}{r.priceCorrection ? " · price correction" : ""}{r.reversed ? " · reversed" : ""}</span></span> },
    { key: "p", header: "Volume @ price", align: "right", cell: (r) => <span className="k-num whitespace-nowrap font-mono text-[12px]">{r.volume} @ {fmtPrice(r.symbol, r.price)}</span> },
    { key: "pl", header: "Profit", align: "right", cell: (r) => (r.entry === "in" ? <span className="text-fg-3">—</span> : <span className={cn("k-num font-mono text-[12px]", r.profit >= 0 ? "text-up" : "text-down")}>{signed2(r.profit, ccy)}</span>) },
  ];
  return (
    <div>
      <div className="mb-3 grid grid-cols-3 gap-2">
        <MetaTile label="Profit" value={signed2(data.totals.profit, ccy)} tone={data.totals.profit >= 0 ? "up" : "down"} />
        <MetaTile label="Swap" value={signed2(data.totals.swap, ccy)} />
        <MetaTile label="Commission" value={money2(data.totals.commission, ccy)} />
      </div>
      <DataTable columns={cols} rows={data.deals} dense pageSize={25} rowKey={(r) => String(r.id)} empty={<EmptyState title="No deals yet" text="Entry and exit deals appear here." illustration="receipt" className="py-8" />} />
      <Pager page={data.page} perPage={data.limit} total={data.total} onPage={setPage} />
    </div>
  );
}

function LedgerTab({ login }: { login: string }) {
  const [page, setPage] = React.useState(1);
  const { data, error, reload } = useApi<Ledger>(`/api/trading/accounts/${login}/ledger${qs({ page, limit: 25 })}`, { refreshMs: 10_000 });
  if (error) return <ErrorState error={error} onRetry={reload} />;
  if (!data) return <TableSkeleton rows={4} />;
  const cols: Column<Ledger["items"][number]>[] = [
    { key: "t", header: "Time (GMT+3)", cell: (r) => <span className="whitespace-nowrap font-mono text-[11.5px] text-fg-2">{serverStamp(r.at)}<span className="block text-[10.5px] text-fg-3">txn {r.txn}</span></span> },
    { key: "k", header: "Type", cell: (r) => <span className="whitespace-nowrap text-[12px]">{LEDGER_KIND[r.kind] ?? r.kind}<span className="block text-[10.5px] text-fg-3">{r.subLedger}</span></span> },
    { key: "a", header: "Amount", align: "right", cell: (r) => <span className={cn("k-num font-mono text-[12.5px]", r.amount >= 0 ? "text-up" : "text-down")}>{signed2(r.amount, r.currency)}</span> },
    { key: "r", header: "Reason", cell: (r) => (r.reasonCode || r.note ? <span className="block max-w-[130px] text-[12px]" title={[r.reasonCode, r.note].filter(Boolean).join(" — ")}><span className="block truncate"><span className="font-mono text-[10.5px] text-ember">{r.reasonCode?.split(" · ")[0]}</span> {r.reasonCode?.split(" · ")[1]}</span>{r.note && <span className="block truncate text-[11px] text-fg-3">“{r.note}”</span>}</span> : <span className="block max-w-[130px] truncate text-[11.5px] text-fg-3">{r.reference ?? "—"}</span>) },
  ];
  return (
    <div>
      <DataTable columns={cols} rows={data.items} dense pageSize={25} rowKey={(r) => `${r.txn}-${r.subLedger}`} empty={<EmptyState title="No ledger entries" text="Transfers, P&L, commissions and staff adjustments are booked here (double entry)." illustration="receipt" className="py-8" />} />
      <Pager page={data.page} perPage={data.limit} total={data.total} onPage={setPage} />
    </div>
  );
}

function AuditTab({ login }: { login: string }) {
  const { data, error, reload } = useApi<import("@/lib/trading-desk").AuditEntry[]>(`/api/trading/dealing/audit${qs({ login, limit: 100 })}`, { refreshMs: 10_000 });
  if (error) return <ErrorState error={error} onRetry={reload} />;
  if (!data) return <TableSkeleton rows={4} />;
  if (!data.length) return <EmptyState title="No staff actions yet" text="Dealer trades, controls and account changes on this login are listed here." illustration="receipt" className="py-8" />;
  return (
    <ol className="relative space-y-3 border-l border-line pl-4">
      {data.map((e) => (
        <li key={e.id} className="relative">
          <span className={cn("absolute -left-[21px] top-1.5 size-2.5 rounded-full border-2 border-surface", /rejected/.test(e.action) ? "bg-down" : e.action.startsWith("account.") ? "bg-gold" : "bg-ember")} />
          <div className="flex flex-wrap items-center gap-x-2 text-[12.5px]">
            <span className="font-medium">{actionText(e.action)}</span>
            {e.tickets?.length ? <span className="font-mono text-[11px] text-fg-3">{e.tickets.map((t) => `#${t}`).join(" · ")}</span> : null}
            <span className="font-mono text-[10.5px] text-fg-3">{e.id}</span>
          </div>
          <div className="mt-0.5 text-[11.5px] text-fg-3">
            {e.staff.name} ({e.staff.role}) · {serverStamp(e.at)} · {e.reasonCode}
            {e.note && <> · “{e.note}”</>}
          </div>
          <ChangeLine before={e.before} after={e.after} />
        </li>
      ))}
    </ol>
  );
}

/* ------------------------------------------------------------------ */
/* Account actions (engine writes, reason-coded, audited)              */
/* ------------------------------------------------------------------ */

type FundsType = "deposit" | "withdrawal" | "adjustment" | "credit" | "bonus";

export function AccountActions({ act, onClose, onDone }: { act: Act; onClose: () => void; onDone?: () => void }) {
  const { api, state } = useDesk();
  const rest = useRestDesk();
  const dir = useLiveDirectory();
  const a = act?.a;
  // balance and credit go through Balance & credit (AdjustDialog); this dialog books the bonus sub-ledger only
  const [fType, setFType] = React.useState<FundsType>("bonus");
  const [dirn, setDirn] = React.useState<"add" | "remove">("add");
  const [amount, setAmount] = React.useState("");
  const [idem, setIdem] = React.useState("");
  const [status, setStatus] = React.useState<string>("active");
  const [grp, setGrp] = React.useState("");
  const [lev, setLev] = React.useState("");
  const [ctl, setCtl] = React.useState({ tradingDisabled: false, closeOnly: false, maxLot: "", execDelayMs: "0", markupPips: "0" });
  const [empty, setEmpty] = React.useState(false);
  const [clientRestorable, setClientRestorable] = React.useState(true);
  React.useEffect(() => {
    if (!a) return;
    setFType("bonus");
    setDirn("add");
    setAmount("");
    setIdem(typeof crypto !== "undefined" && "randomUUID" in crypto ? crypto.randomUUID() : String(Date.now()));
    setStatus(LIFECYCLE_STATUSES.includes(a.status) ? "active" : a.status);
    setEmpty(false);
    setClientRestorable(true);
    setGrp(a.group);
    setLev(String(a.leverage));
    setCtl({ tradingDisabled: a.controls.tradingDisabled, closeOnly: a.controls.closeOnly, maxLot: a.controls.maxLot ? String(a.controls.maxLot) : "", execDelayMs: String(a.controls.execDelayMs), markupPips: String(a.controls.markupPips) });
  }, [act, a]);
  if (!a) return null;
  const close = (o: boolean) => {
    if (!o) {
      onClose();
      onDone?.();
    }
  };
  const done = <T,>(r: import("@/lib/trading-desk").DeskResult<T>) => {
    if (r.ok) onDone?.();
    return r;
  };
  const ccy = a.currency;
  const n = Number(amount.replace(/,/g, "")) || 0;
  const signedAmt = fType === "deposit" ? n : fType === "withdrawal" ? -n : dirn === "add" ? n : -n;
  const target = dir.groups.find((g) => g.code === grp);
  // an account never changes product (the engine refuses a CFD ↔ Options move)
  const groupsFor = dir.groups.filter((g) => g.enabled && (g.accountTypes === "both" || g.accountTypes === a.type) && g.cent === a.cent && productOf(g) === productOf(a));
  const levList = dir.groups.find((g) => g.code === a.group)?.leverages ?? a.leverages;
  const delayOn = state.tenant.execDelayEnabled;
  const after = fType === "credit" ? a.credit + signedAmt : fType === "bonus" ? a.bonus + signedAmt : a.balance + signedAmt;

  return (
    <>
      <AdjustDialog open={act?.k === "adjust"} onOpenChange={close} userId={Number(a.userId)} clientName={clientName(a.userId, a.login)} preset={{ target: "trading", login: Number(a.login) }} onDone={() => onDone?.()} />
      <DeskDialog
        open={act?.k === "funds"}
        onOpenChange={close}
        title={`Bonus · ${a.login}`}
        description={`${clientName(a.userId, a.login)} · ${a.groupName} · ${ccy}. Booked on the double-entry ledger and shown on the client statement.`}
        codes={FIN_REASONS}
        requireNote
        confirmLabel={n > 0 ? `${fType === "withdrawal" || (fType !== "deposit" && dirn === "remove") ? "Debit" : "Credit"} ${money2(n, ccy)}` : "Enter an amount"}
        confirmVariant="gold"
        disabled={n > 0 ? (fType === "withdrawal" && n > a.withdrawable ? `Withdrawable is ${money2(a.withdrawable, ccy)}` : false) : "Enter an amount"}
        onConfirm={async (r) => done(await tradingWrite<{ balance: number; txn: number }>(`admin/accounts/${a.login}/balance`, { type: fType, amount: signedAmt, idempotencyKey: idem }, r, rest))}
        success={(d) => `Booked · balance ${money2(d?.balance ?? 0, ccy)}${d?.txn ? ` · txn ${d.txn}` : ""}`}
      >
        <div className="space-y-4">
          <p className="text-[12px] text-fg-3">Bonus is a separate sub-ledger that counts toward equity. Balance and credit changes are made with Balance &amp; credit.</p>
          <div className="grid grid-cols-2 gap-3">
            {fType !== "deposit" && fType !== "withdrawal" ? (
              <Field label="Direction">
                <Segmented size="sm" value={dirn} onChange={setDirn} options={[{ value: "add", label: "Add" }, { value: "remove", label: "Remove" }]} />
              </Field>
            ) : (
              <MetaTile label={fType === "withdrawal" ? "Withdrawable" : "Balance now"} value={money2(fType === "withdrawal" ? a.withdrawable : a.balance, ccy)} className="py-2.5" />
            )}
            <Field label={`Amount (${ccy})`}>
              <Input value={amount} onChange={(e) => setAmount(e.target.value.replace(/[^0-9.,]/g, ""))} placeholder="0.00" aria-label="Amount" className="font-mono" />
            </Field>
          </div>
          <div className="grid grid-cols-3 gap-2">
            <MetaTile label={fType === "credit" ? "Credit now" : fType === "bonus" ? "Bonus now" : "Balance now"} value={money2(fType === "credit" ? a.credit : fType === "bonus" ? a.bonus : a.balance, ccy)} />
            <MetaTile label="Change" value={signed2(signedAmt, ccy)} tone={signedAmt >= 0 ? "up" : "down"} />
            <MetaTile label="After" value={money2(after, ccy)} tone={after < 0 ? "down" : undefined} />
          </div>
          {a.type === "demo" && <div className="text-[12px] text-fg-3">Demo account: postings go to the demo funding house account, never to real money.</div>}
        </div>
      </DeskDialog>

      <DeskDialog
        open={act?.k === "status"}
        onOpenChange={close}
        title={`Account status · ${a.login}`}
        description="Close-only: positions can only be reduced. Read-only: terminal login works, no trading. Disabled: no login, no trading."
        codes={ACC_REASONS}
        confirmLabel={`Set ${STATUS_LABEL[status] ?? status}`}
        disabled={status === a.status ? "Choose a different status" : false}
        onConfirm={async (r) => done(await tradingWrite(`admin/accounts/${a.login}/status`, { status }, r, rest))}
        success={`${a.login} is now ${STATUS_LABEL[status]?.toLowerCase() ?? status}`}
      >
        <Segmented size="sm" value={status} onChange={setStatus} options={SETTABLE_STATUS} />
      </DeskDialog>

      <DeskDialog
        open={act?.k === "archive"}
        onOpenChange={close}
        title={`Archive account · ${a.login}`}
        description={`${clientName(a.userId, a.login)} · ${a.groupName} · ${a.type}. An archived account can't trade, receive transfers or sign in to the terminal; history and statements are kept. The client is notified with the client-facing reason; your note stays internal.`}
        codes={ACC_ARCHIVE_REASONS}
        confirmLabel="Archive account"
        confirmVariant="sell"
        disabled={!empty && (a.positions > 0 || a.orders > 0) ? `${a.positions} open position(s) · ${a.orders} order(s): turn on “Close open trades” or close them first` : false}
        onConfirm={async (r) => done(await tradingWrite(`admin/accounts/${a.login}/archive`, { empty, clientRestorable }, r, rest))}
        success={`${a.login} archived${empty ? " · trades closed, balance moved to the wallet" : ""}`}
      >
        <div className="space-y-3">
          <div className="grid grid-cols-3 gap-2">
            <MetaTile label="Balance" value={money2(a.balance, ccy)} />
            <MetaTile label="Credit · bonus" value={`${money2(a.credit, ccy)} · ${money2(a.bonus, ccy)}`} />
            <MetaTile label="Open" value={`${a.positions} pos · ${a.orders} ord`} tone={a.positions || a.orders ? "warn" : undefined} />
          </div>
          <label className="flex items-start justify-between gap-3 rounded-[12px] border border-line bg-surface-2/60 px-3 py-2.5 text-[12.5px]">
            <span>
              <span className="block font-medium">Close open trades and move balance to wallet first</span>
              <span className="block text-[11.5px] text-fg-3">
                {a.type === "demo" ? "Closes positions and cancels orders at market; the demo balance is virtual and is not moved." : `Closes positions and cancels orders at market, then moves the balance to the client's wallet. Credit and bonus are forfeited${a.credit || a.bonus ? ` (${money2(a.credit + a.bonus, ccy)})` : ""}.`}
              </span>
            </span>
            <Toggle checked={empty} onChange={setEmpty} label="Close open trades and move balance to wallet first" />
          </label>
          <label className="flex items-start justify-between gap-3 rounded-[12px] border border-line bg-surface-2/60 px-3 py-2.5 text-[12.5px]">
            <span>
              <span className="block font-medium">Client may restore it themselves</span>
              <span className="block text-[11.5px] text-fg-3">Shown under Archived in the Client Area with a Restore button. Turn off for compliance or duplicate archives — then only staff can restore it.</span>
            </span>
            <Toggle checked={clientRestorable} onChange={setClientRestorable} label="Client may restore it themselves" />
          </label>
        </div>
      </DeskDialog>

      <DeskDialog
        open={act?.k === "restore"}
        onOpenChange={close}
        title={`Restore account · ${a.login}`}
        description={`${clientName(a.userId, a.login)} · ${a.groupName}. The account becomes active again with its previous group and leverage; the client is notified.`}
        codes={ACC_RESTORE_REASONS}
        confirmLabel="Restore account"
        confirmVariant="gold"
        disabled={a.status !== "archived" ? "Only archived accounts can be restored here" : false}
        onConfirm={async (r) => done(await tradingWrite(`admin/accounts/${a.login}/restore`, {}, r, rest))}
        success={`${a.login} restored`}
      />

      <CloseAccountDialog a={a} open={act?.k === "close"} onOpenChange={close} />
      <ReopenAccountDialog a={a} open={act?.k === "reopen"} onOpenChange={close} />

      <DeskDialog
        open={act?.k === "group"}
        onOpenChange={close}
        title={`Change group · ${a.login}`}
        description={`Now ${a.groupName}. Netting ↔ hedging only while the account is flat; cent ↔ standard and CFD ↔ Options never.`}
        codes={ACC_REASONS}
        confirmLabel={target ? `Move to ${target.name}` : "Move"}
        disabled={grp === a.group ? "Choose another group" : target && target.mode !== a.mode && a.positions > 0 ? "Close the positions first (mode change)" : false}
        onConfirm={async (r) => done(await tradingWrite(`admin/accounts/${a.login}/group`, { group: grp }, r, rest))}
        success={`${a.login} moved to ${target?.name ?? grp}`}
      >
        <div className="flex flex-wrap gap-1.5">
          {groupsFor.map((g) => (
            <button key={g.code} type="button" onClick={() => setGrp(g.code)} aria-pressed={grp === g.code} className={cn("rounded-[12px] border px-3 py-2 text-left text-[12.5px]", grp === g.code ? "border-ember/50 bg-ember-soft" : "border-line bg-surface-2 hover:border-fg-3")}>
              <span className="block font-medium">{g.name}</span>
              <span className="block text-[11px] text-fg-3">{g.mode} · MC {g.marginCallPct}% / SO {g.stopOutPct}% · {g.commissionPerLot ? `$${g.commissionPerLot}/lot` : "no commission"}</span>
            </button>
          ))}
        </div>
      </DeskDialog>

      <DeskDialog
        open={act?.k === "leverage"}
        onOpenChange={close}
        title={`Change leverage · ${a.login}`}
        description={`Now 1:${a.leverage}. Staff may change it with open positions — margin is recalculated at once (stop-out still applies).`}
        codes={ACC_REASONS}
        confirmLabel={`Set 1:${lev}`}
        disabled={lev === String(a.leverage) ? "Choose a different leverage" : false}
        onConfirm={async (r) => done(await tradingWrite(`admin/accounts/${a.login}/leverage`, { leverage: Number(lev) }, r, rest))}
        success={`Leverage set to 1:${lev}`}
      >
        <Segmented size="sm" value={lev} onChange={setLev} options={levList.map((l) => ({ value: String(l), label: `1:${l}` }))} />
      </DeskDialog>

      <DeskDialog
        open={act?.k === "controls"}
        onOpenChange={close}
        title={`Dealer controls · ${a.login}`}
        description="Enforced by the engine on every new trade, dealer or client."
        confirmLabel="Apply controls"
        onConfirm={async (r) =>
          done(
            await api.setAccountControl(
              a.login,
              { tradingDisabled: ctl.tradingDisabled, closeOnly: ctl.closeOnly, maxLot: ctl.maxLot ? Number(ctl.maxLot) : (null as unknown as number), execDelayMs: Number(ctl.execDelayMs) || 0, markupPips: Number(ctl.markupPips) || 0 },
              r,
            ),
          )
        }
        success="Dealer controls applied"
      >
        <div className="grid grid-cols-3 gap-3">
          <Field label="Max lot" hint="empty = none">
            <Input value={ctl.maxLot} onChange={(e) => setCtl({ ...ctl, maxLot: e.target.value.replace(/[^0-9.]/g, "") })} aria-label="Max lot" className="font-mono" />
          </Field>
          <Field label="Markup (pips)">
            <Input value={ctl.markupPips} onChange={(e) => setCtl({ ...ctl, markupPips: e.target.value.replace(/[^0-9.-]/g, "") })} aria-label="Markup pips" className="font-mono" />
          </Field>
          <Field label="Delay (ms)" hint={delayOn ? `≤ ${state.tenant.execDelayCapMs}` : "tenant: blocked"}>
            <Input value={ctl.execDelayMs} onChange={(e) => setCtl({ ...ctl, execDelayMs: e.target.value.replace(/\D/g, "") })} aria-label="Execution delay" className="font-mono" />
          </Field>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button size="xs" variant={ctl.tradingDisabled ? "sell" : "surface"} onClick={() => setCtl({ ...ctl, tradingDisabled: !ctl.tradingDisabled })}>
            {ctl.tradingDisabled ? <CirclePlay /> : <Ban />} {ctl.tradingDisabled ? "Trading disabled (click to enable)" : "Disable trading"}
          </Button>
          <Button size="xs" variant={ctl.closeOnly ? "gold" : "surface"} onClick={() => setCtl({ ...ctl, closeOnly: !ctl.closeOnly })}>
            <CirclePause /> {ctl.closeOnly ? "Close-only (click to lift)" : "Set close-only"}
          </Button>
        </div>
      </DeskDialog>

      <DeskDialog
        open={act?.k === "route"}
        onOpenChange={close}
        title={act?.k === "route" ? (act.book ? `Route new trades of ${a.login} to ${act.book}-book` : `Remove the account route · ${a.login}`) : ""}
        description="Applies to new trades only. Move open positions from the Positions page (full or partial)."
        confirmLabel="Apply route"
        onConfirm={async (r) => done(await api.quickRoute({ login: a.login }, act?.k === "route" ? act.book : null, r))}
        success="Routing updated"
      />

      <CreateTradeDrawer open={act?.k === "trade"} onOpenChange={(o) => close(o)} initialLogin={a.login} />
    </>
  );
}
