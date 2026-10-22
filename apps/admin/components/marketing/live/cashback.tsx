"use client";

import * as React from "react";
import { Coins, HandCoins, Pencil, Plus, RefreshCw, Users, Wallet } from "lucide-react";
import { Button, Card, CardHeader, Chip, DataTable, KpiCard, PageHeader, Reveal, Toggle, type Column } from "@kalks/ui";
import { FilterSelect, Pager, TableSkeleton, day, qs, useApi, when } from "@/components/live/kit";
import { M, mkSend, type Accrual, type CashbackPayout, type Overview, type Paged, type Programme, type ProgrammeInput } from "./api";
import {
  AreaF,
  ClientCell,
  DateTimeF,
  EmptyNote,
  FormDialog,
  MkError,
  NumF,
  PAY_STATUS,
  PillPicker,
  ReadOnlyNote,
  StatusPill,
  TextF,
  ToggleRow,
  fromLocalInput,
  int,
  num,
  numOrNull,
  numStr,
  splitList,
  toLocalInput,
  usd,
  usdK,
  useAction,
  usePerms,
  windowState,
} from "./kit";
import { AWAITING, FirstApproval, FourEyesDecide, fourEyesRule } from "./four-eyes";

const ASSET_CLASSES = ["forex", "metals", "indices", "energies", "crypto", "stocks"].map((c) => ({ value: c, label: c[0]!.toUpperCase() + c.slice(1) }));
const PER = 25;

function scope(p: Pick<Programme, "assetClasses" | "symbols" | "accountGroups">) {
  const parts: string[] = [];
  if (p.symbols?.length) parts.push(p.symbols.length <= 4 ? p.symbols.join(", ") : `${p.symbols.length} symbols`);
  if (p.assetClasses?.length) parts.push(p.assetClasses.join(", "));
  if (!parts.length) parts.push("All symbols");
  if (p.accountGroups?.length) parts.push(`groups ${p.accountGroups.join(", ")}`);
  return parts.join(" · ");
}

export function LiveCashback() {
  const perms = usePerms();
  const act = useAction();
  const ov = useApi<Overview>(M("overview"));
  const progs = useApi<{ items: Programme[] }>(M("cashback/programmes"));
  const [editing, setEditing] = React.useState<Programme | null>(null);
  const [open, setOpen] = React.useState(false);
  const [tick, setTick] = React.useState(0);
  const items = progs.data?.items ?? [];
  const cb = ov.data?.cashback;
  const reloadAll = () => {
    progs.reload();
    ov.reload();
    setTick((n) => n + 1);
  };

  const toggle = (p: Programme) =>
    act.ask({
      title: p.active ? `Stop ${p.name}` : `Start ${p.name}`,
      description: p.active ? "New deals stop accruing. Accruals so far are still paid after the hold." : "Matching deals accrue cashback again.",
      confirmLabel: p.active ? "Stop programme" : "Start programme",
      confirmVariant: p.active ? "surface" : "ember",
      note: "none",
      run: () => mkSend(`cashback/programmes/${p.id}`, { active: !p.active }, "PATCH"),
      success: p.active ? `${p.name} stopped` : `${p.name} started`,
      onDone: reloadAll,
    });
  const runPayouts = () =>
    act.ask({
      title: "Run cashback payouts now",
      description: "Pays every accrual past the hold as one wallet refund per client. Transfers are idempotent and retried on failure.",
      body: cb ? (
        <div className="flex items-baseline justify-between rounded-[12px] border border-gold/30 bg-gold-soft px-4 py-3">
          <span className="text-[13px] text-fg-2">Accrued, not yet paid</span>
          <span className="k-num text-[20px] font-semibold text-gold">{usd(cb.accrued)}</span>
        </div>
      ) : undefined,
      confirmLabel: "Run payouts",
      confirmVariant: "buy",
      confirmTestId: "cashback-run-confirm",
      note: "none",
      run: () => mkSend<{ created: number; amount: number }>("cashback/payouts/run", {}),
      success: (r: { created: number; amount: number }) => `${int(r.created)} payout(s) created · ${usd(r.amount)}`,
      onDone: reloadAll,
    });

  const cols: Column<Programme>[] = [
    { key: "n", header: "Programme", cell: (p) => <span className="block max-w-64"><span className="block truncate font-medium">{p.name}</span><span className="block truncate text-[11.5px] text-fg-3">{scope(p)}</span></span>, sort: (p) => p.name, csv: (p) => p.name },
    { key: "r", header: "Rate", align: "right", cell: (p) => <span className="k-num font-medium text-gold">{usd(p.usdPerLot)}/lot</span>, sort: (p) => p.usdPerLot, csv: (p) => p.usdPerLot },
    { key: "cap", header: "Monthly cap", align: "right", hideOn: "md", cell: (p) => <span className="k-num text-fg-2">{p.maxPerMonth ? usd(p.maxPerMonth, 0) : "None"}</span>, csv: (p) => p.maxPerMonth ?? "" },
    { key: "e", header: "Enrolment", cell: (p) => (p.optIn ? <Chip size="sm" tone="info">Opt-in · {int(p.enrolled)}</Chip> : <Chip size="sm">Automatic</Chip>), csv: (p) => (p.optIn ? p.enrolled : "auto") },
    { key: "l", header: "Lots 30d", align: "right", hideOn: "lg", cell: (p) => <span className="k-num text-fg-2">{num(p.lots30d)}</span>, sort: (p) => p.lots30d, csv: (p) => p.lots30d },
    { key: "a", header: "Accrued", align: "right", cell: (p) => <span className="k-num">{usd(p.accrued)}</span>, sort: (p) => p.accrued, csv: (p) => p.accrued },
    { key: "p", header: "Paid", align: "right", cell: (p) => <span className="k-num text-up">{usd(p.paid)}</span>, sort: (p) => p.paid, csv: (p) => p.paid },
    { key: "w", header: "Window", hideOn: "lg", cell: (p) => <span className="k-num whitespace-nowrap text-[12px] text-fg-3">{day(p.startsAt)} – {p.endsAt ? day(p.endsAt) : "open"}</span>, csv: (p) => p.startsAt },
    {
      key: "s",
      header: "Status",
      align: "right",
      cell: (p) => {
        const w = windowState(p.active, p.startsAt, p.endsAt);
        return (
          <span className="flex items-center justify-end gap-2">
            <Chip size="sm" dot tone={w.tone}>
              {w.label}
            </Chip>
            {perms.write && (
              <>
                <Toggle checked={p.active} onChange={() => toggle(p)} label={`${p.name} active`} />
                <Button size="xs" variant="surface" onClick={() => (setEditing(p), setOpen(true))} aria-label={`Edit ${p.name}`}>
                  <Pencil />
                </Button>
              </>
            )}
          </span>
        );
      },
      csv: (p) => (p.active ? "active" : "inactive"),
    },
  ];

  return (
    <div className="pb-16">
      <PageHeader
        title="Cashback"
        subtitle="USD per lot on matching symbols and groups, live accounts only, paid to the wallet after the hold."
        actions={
          <>
            {perms.loaded && !perms.write && <ReadOnlyNote what="edit programmes" />}
            <Button variant="surface" onClick={reloadAll}>
              <RefreshCw /> Refresh
            </Button>
            {perms.approve && (
              <Button variant="surface" onClick={runPayouts} data-testid="cashback-run">
                <HandCoins /> Run payouts now
              </Button>
            )}
            {perms.write && (
              <Button variant="ember" onClick={() => (setEditing(null), setOpen(true))} data-testid="new-programme">
                <Plus /> New programme
              </Button>
            )}
          </>
        }
      />

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <KpiCard label="Active programmes" icon={<Coins />} value={<span className="k-num">{progs.data ? int(items.filter((p) => p.active).length) : "—"}</span>} chip={progs.data ? `${items.length} total` : "Loading"} />
        <KpiCard label="Accrued, unpaid" icon={<Wallet />} value={<span className="k-num">{cb ? usdK(cb.accrued) : "—"}</span>} chip="Waiting for the hold" chipTone="gold" delay={0.05} />
        <KpiCard label="Paid · 30d" icon={<HandCoins />} value={<span className="k-num">{cb ? usdK(cb.paid30d) : "—"}</span>} chip="Wallet refunds" chipTone="up" delay={0.1} />
        <KpiCard label="Opt-in enrolments" icon={<Users />} value={<span className="k-num">{progs.data ? int(items.reduce((s, p) => s + (p.optIn ? p.enrolled : 0), 0)) : "—"}</span>} chip="Across opt-in programmes" delay={0.15} />
      </div>

      <Reveal delay={0.05} className="mt-4">
        <Card>
          <CardHeader title="Programmes" subtitle="Each closed live deal accrues once per matching programme, capped per client per month" />
          <div className="mt-4 px-4 pb-5 sm:px-6">
            {progs.error && !progs.data ? (
              <MkError error={progs.error} onRetry={progs.reload} />
            ) : !progs.data ? (
              <TableSkeleton rows={3} />
            ) : (
              <DataTable columns={cols} rows={items} pageSize={20} dense rowKey={(p) => String(p.id)} exportName="cashback-programmes" empty={<EmptyNote className="mt-3" title="No cashback programmes" text="Create one to pay clients a fixed amount per lot traded." />} />
            )}
          </div>
        </Card>
      </Reveal>

      <Reveal delay={0.1} className="mt-4">
        <AccrualsCard programmes={items} tick={tick} />
      </Reveal>
      <Reveal delay={0.1} className="mt-4">
        <PayoutsCard tick={tick} approve={perms.approve} onChanged={reloadAll} />
      </Reveal>

      <ProgrammeDialog open={open} onOpenChange={setOpen} programme={editing} onSaved={reloadAll} />
      {act.node}
    </div>
  );
}

function AccrualsCard({ programmes, tick }: { programmes: Programme[]; tick: number }) {
  const [programme, setProgramme] = React.useState("all");
  const [status, setStatus] = React.useState("all");
  const [user, setUser] = React.useState("");
  const [page, setPage] = React.useState(1);
  React.useEffect(() => setPage(1), [programme, status, user]);
  const { data, error, loading, reload } = useApi<Paged<Accrual>>(`${M("cashback/accruals")}${qs({ programme, status, user: /^\d+$/.test(user) ? user : null, page, limit: PER })}`);
  React.useEffect(() => {
    if (tick) reload();
  }, [tick, reload]);
  const pName = (a: Accrual) => a.programme ?? programmes.find((p) => p.id === a.programmeId)?.name ?? `#${a.programmeId}`;
  const cols: Column<Accrual>[] = [
    { key: "at", header: "Time", cell: (a) => <span className="k-num whitespace-nowrap text-[12.5px] text-fg-2">{when(a.createdAt)}</span>, csv: (a) => a.createdAt },
    { key: "c", header: "Client", cell: (a) => (a.userId ? <ClientCell id={a.userId} name={a.name} sub={<span>acc {a.login}</span>} /> : <span className="font-mono text-[12.5px]">{a.login}</span>), csv: (a) => a.userId ?? "" },
    { key: "p", header: "Programme", hideOn: "md", cell: (a) => <span className="text-[12.5px] text-fg-2">{pName(a)}</span>, csv: (a) => pName(a) },
    { key: "d", header: "Deal", cell: (a) => <span className="font-mono text-[12.5px]">{a.symbol} · #{a.dealId}</span>, csv: (a) => a.dealId },
    { key: "l", header: "Lots", align: "right", cell: (a) => <span className="k-num">{num(a.lots)}</span>, csv: (a) => a.lots },
    { key: "a", header: "Amount", align: "right", cell: (a) => <span className="k-num text-gold">{usd(a.amount)}</span>, csv: (a) => a.amount },
    { key: "s", header: "Status", cell: (a) => <StatusPill map={PAY_STATUS} status={a.status} />, csv: (a) => a.status },
  ];
  return (
    <Card>
      <CardHeader title="Accruals" subtitle="One line per deal and programme · reopened deals void unpaid accruals" />
      <div className="mt-4 px-4 pb-5 sm:px-6">
        {error && !data ? (
          <MkError error={error} onRetry={reload} />
        ) : !data ? (
          <TableSkeleton />
        ) : (
          <div className={loading ? "opacity-60 transition-opacity" : undefined}>
            <DataTable
              columns={cols}
              rows={data.items}
              pageSize={PER}
              dense
              rowKey={(a) => String(a.id)}
              exportName="cashback-accruals"
              toolbar={
                <>
                  <FilterSelect label="Programme" value={programme} onChange={setProgramme} options={[{ value: "all", label: "All" }, ...programmes.map((p) => ({ value: String(p.id), label: p.name }))]} />
                  <FilterSelect label="Status" value={status} onChange={setStatus} options={[{ value: "all", label: "All" }, { value: "accrued", label: "Accrued" }, { value: "paid", label: "Paid" }, { value: "void", label: "Void" }]} />
                  <label className="flex h-9 items-center gap-2 rounded-full border border-line bg-surface-2 pl-3.5 pr-3 text-[12.5px] text-fg-3">
                    Client ID
                    <input value={user} onChange={(e) => setUser(e.target.value.replace(/\D/g, ""))} className="w-20 bg-transparent font-mono text-fg outline-none" />
                  </label>
                </>
              }
              empty={<EmptyNote className="mt-3" title="No accruals" />}
            />
            <Pager page={page} perPage={PER} total={data.total} onPage={setPage} />
          </div>
        )}
      </div>
    </Card>
  );
}

type PayoutsPage = Paged<CashbackPayout> & { open?: number; fourEyesThreshold?: number };

function PayoutsCard({ tick, approve, onChanged }: { tick: number; approve: boolean; onChanged: () => void }) {
  const [status, setStatus] = React.useState("all");
  const [page, setPage] = React.useState(1);
  React.useEffect(() => setPage(1), [status]);
  const { data, error, loading, reload } = useApi<PayoutsPage>(`${M("cashback/payouts")}${qs({ status, page, limit: PER })}`);
  const act = useAction();
  React.useEffect(() => {
    if (tick) reload();
  }, [tick, reload]);
  const threshold = data?.fourEyesThreshold;
  const cols: Column<CashbackPayout>[] = [
    { key: "id", header: "Payout", cell: (p) => <span className="font-mono text-[12.5px]">#{p.id}</span>, csv: (p) => p.id },
    { key: "c", header: "Client", cell: (p) => <ClientCell id={p.userId} name={p.name} />, csv: (p) => p.userId },
    { key: "a", header: "Amount", align: "right", cell: (p) => <span className="k-num font-medium">{usd(p.amount)}</span>, sort: (p) => p.amount, csv: (p) => p.amount },
    {
      key: "s",
      header: "Status",
      cell: (p) => (
        <span className="flex flex-col items-start gap-0.5">
          <StatusPill map={PAY_STATUS} status={p.status} />
          {p.status === AWAITING && <FirstApproval fe={p.fourEyes} className="flex max-w-56 items-center gap-1 text-[10.5px] text-warn" />}
          {p.error && <span className="max-w-48 truncate text-[10.5px] text-down" title={p.error}>{p.error}</span>}
          {p.status === "rejected" && p.rejectReason && <span className="max-w-48 truncate text-[10.5px] text-fg-3" title={p.rejectReason}>{p.rejectReason}</span>}
        </span>
      ),
      csv: (p) => p.status,
    },
    { key: "t", header: "Attempts", align: "right", hideOn: "md", cell: (p) => <span className="k-num text-fg-2">{int(p.attempts)}</span>, csv: (p) => p.attempts },
    { key: "cr", header: "Created", align: "right", cell: (p) => <span className="k-num text-[12.5px] text-fg-2">{when(p.createdAt)}</span>, csv: (p) => p.createdAt },
    { key: "pd", header: "Paid", align: "right", hideOn: "md", cell: (p) => <span className="k-num text-[12.5px] text-fg-3">{when(p.paidAt)}</span>, csv: (p) => p.paidAt ?? "" },
    {
      key: "act",
      header: "",
      align: "right",
      cell: (p) =>
        approve && (p.status === "approval" || p.status === AWAITING) ? (
          <FourEyesDecide<{ payout?: CashbackPayout }>
            act={act}
            what="payout"
            status={p.status}
            amount={p.amount}
            fe={p.fourEyes}
            threshold={threshold}
            approvePath={`cashback/payouts/${p.id}/approve`}
            rejectPath={`cashback/payouts/${p.id}/reject`}
            statusOf={(d) => d.payout?.status}
            paidText="Payout approved · paid to the wallet by the payout run"
            describe="One wallet refund to the client (idempotent, retried on failure). Rejecting marks its accruals rejected: they are never paid."
            onDone={onChanged}
            testId={`cashback-payout-${p.id}`}
          />
        ) : null,
    },
  ];
  return (
    <Card>
      <CardHeader
        title="Payouts"
        subtitle={`One wallet refund per client per run. ${threshold != null ? `${fourEyesRule(threshold)} Larger payouts are held here until approved.` : ""}`}
        action={data?.open ? <Chip size="sm" tone="warn">{int(data.open)} waiting for approval</Chip> : undefined}
      />
      <div className="mt-4 px-4 pb-5 sm:px-6">
        {error && !data ? (
          <MkError error={error} onRetry={reload} />
        ) : !data ? (
          <TableSkeleton />
        ) : (
          <div className={loading ? "opacity-60 transition-opacity" : undefined}>
            <DataTable
              columns={cols}
              rows={data.items}
              pageSize={PER}
              dense
              rowKey={(p) => String(p.id)}
              exportName="cashback-payouts"
              toolbar={<FilterSelect label="Status" value={status} onChange={setStatus} options={[
                    { value: "all", label: "All" },
                    { value: "open", label: "Needs approval" },
                    { value: "awaiting_second_approval", label: "Awaiting 2nd approval" },
                    { value: "pending", label: "Pending" },
                    { value: "paid", label: "Paid" },
                    { value: "failed", label: "Failed" },
                    { value: "rejected", label: "Rejected" },
                  ]} />}
              empty={<EmptyNote className="mt-3" title="No payouts yet" text="Payouts run every 30 seconds for accruals past the hold." />}
            />
            <Pager page={page} perPage={PER} total={data.total} onPage={setPage} />
          </div>
        )}
      </div>
      {act.node}
    </Card>
  );
}

function ProgrammeDialog({ open, onOpenChange, programme, onSaved }: { open: boolean; onOpenChange: (o: boolean) => void; programme: Programme | null; onSaved: () => void }) {
  const [name, setName] = React.useState("");
  const [description, setDescription] = React.useState("");
  const [classes, setClasses] = React.useState<string[]>([]);
  const [symbols, setSymbols] = React.useState("");
  const [groups, setGroups] = React.useState("");
  const [usdPerLot, setUsdPerLot] = React.useState("2");
  const [maxPerMonth, setMaxPerMonth] = React.useState("");
  const [optIn, setOptIn] = React.useState(false);
  const [active, setActive] = React.useState(true);
  const [startsAt, setStartsAt] = React.useState("");
  const [endsAt, setEndsAt] = React.useState("");
  React.useEffect(() => {
    if (!open) return;
    setName(programme?.name ?? "");
    setDescription(programme?.description ?? "");
    setClasses(programme?.assetClasses ?? []);
    setSymbols((programme?.symbols ?? []).join(", "));
    setGroups((programme?.accountGroups ?? []).join(", "));
    setUsdPerLot(numStr(programme?.usdPerLot ?? 2));
    setMaxPerMonth(numStr(programme?.maxPerMonth));
    setOptIn(programme?.optIn ?? false);
    setActive(programme?.active ?? true);
    setStartsAt(toLocalInput(programme?.startsAt ?? new Date().toISOString()));
    setEndsAt(toLocalInput(programme?.endsAt));
  }, [open, programme]);
  return (
    <FormDialog
      open={open}
      onOpenChange={onOpenChange}
      width={640}
      title={programme ? `Edit · ${programme.name}` : "New cashback programme"}
      submitLabel={programme ? "Save programme" : "Create programme"}
      submitTestId="programme-form-submit"
      submit={() => {
        if (!name.trim()) return "Give the programme a name.";
        if (!(Number(usdPerLot) > 0)) return "USD per lot must be above 0.";
        const s = fromLocalInput(startsAt);
        const e = fromLocalInput(endsAt);
        if (s && e && e <= s) return "Ends must be after Starts.";
        const body: ProgrammeInput = {
          name: name.trim(),
          description: description.trim(),
          assetClasses: classes,
          symbols: splitList(symbols, true),
          accountGroups: splitList(groups),
          usdPerLot: Number(usdPerLot),
          maxPerMonth: numOrNull(maxPerMonth),
          optIn,
          active,
          ...(s ? { startsAt: s } : {}),
          endsAt: e,
        };
        return programme ? mkSend(`cashback/programmes/${programme.id}`, body, "PATCH") : mkSend("cashback/programmes", body);
      }}
      success={programme ? "Programme saved" : "Programme created"}
      onDone={onSaved}
    >
      <TextF label="Name" value={name} onChange={setName} placeholder="e.g. Gold cashback" />
      <AreaF label="Description" value={description} onChange={setDescription} rows={2} placeholder="Shown to clients under Rewards → Cashback" />
      <PillPicker label="Asset classes" options={ASSET_CLASSES} value={classes} onChange={setClasses} />
      <div className="grid grid-cols-2 gap-3">
        <TextF label="Symbols" value={symbols} onChange={setSymbols} mono placeholder="Any in the classes" hint="comma separated" />
        <TextF label="Account groups" value={groups} onChange={setGroups} placeholder="All live groups" hint="comma separated" />
        <NumF label="USD per lot" value={usdPerLot} onChange={setUsdPerLot} prefix="$" suffix="/ lot" />
        <NumF label="Monthly cap" value={maxPerMonth} onChange={setMaxPerMonth} prefix="$" placeholder="No cap" hint="per client" />
        <DateTimeF label="Starts" value={startsAt} onChange={setStartsAt} />
        <DateTimeF label="Ends" value={endsAt} onChange={setEndsAt} hint="optional" />
      </div>
      <div className="k-row divide-y divide-line px-4">
        <ToggleRow label="Opt-in" hint="Clients must enrol in the Client Area before deals accrue" checked={optIn} onChange={setOptIn} />
        <ToggleRow label="Active" checked={active} onChange={setActive} />
      </div>
    </FormDialog>
  );
}
