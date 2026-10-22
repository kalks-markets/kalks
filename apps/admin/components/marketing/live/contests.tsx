"use client";

import * as React from "react";
import { AlertTriangle, CalendarRange, Copy, Flag as FlagIcon, LayoutTemplate, Pencil, Plus, RefreshCw, Trophy, Users, Wallet } from "lucide-react";
import { Button, Card, CardHeader, Chip, Dialog, KpiCard, PageHeader, Progress, Reveal, Segmented, cn } from "@kalks/ui";
import { TableSkeleton, ago, countryName, useApi, useNow, when } from "@/components/live/kit";
import { M, mkSend, type Contest, type ContestDetail, type ContestFlag, type ContestInstrument, type Overview, type PayResult, type Scoring, type Standing } from "./api";
import { useStaff } from "@/components/staff-session";
import { AWAITING, FirstApproval, FourEyesDecide, fourEyesRule } from "./four-eyes";
import { ContestWizard } from "./contest-wizard";
import { ChallengesTabs, type ChallengesTab } from "./challenges";
import { CONTEST_STATUS, EmptyNote, MkError, PAY_STATUS, ReadOnlyNote, StatusPill, Tile, int, num, pct, usd, usdK, useAction, usePerms } from "./kit";

type F = "all" | "running" | "scheduled" | "ended" | "done";
const SCORING: Record<Scoring, string> = { return_pct: "Return %", profit: "Profit", lots: "Lots traded", contracts: "Contracts traded" };
const FLAG_KIND: Record<string, { label: string; desc: string }> = {
  balance_change: { label: "Balance change", desc: "Deposit, withdrawal, transfer, refill or staff adjustment on the account during the contest" },
  single_trade: { label: "Single trade", desc: "One trade made most of the positive profit" },
  short_holds: { label: "Short holds", desc: "More than half of the trades were held below the minimum hold" },
  self_trade: { label: "Self-trade", desc: "Option trades crossed or hedged with another account of the same client. They are already left out of the score; disqualify if it looks deliberate" },
};
const isOptions = (c: { instrument?: ContestInstrument }) => c.instrument === "options";

/** OPTIONS badge next to LIVE / DEMO (Kalks FX Options contests, O36). */
function InstrumentChip({ c }: { c: { instrument?: ContestInstrument } }) {
  return isOptions(c) ? (
    <Chip size="sm" tone="info">
      OPTIONS
    </Chip>
  ) : null;
}

function useUrlParam(name: string): [string | null, (v: string | null) => void] {
  const [v, setV] = React.useState<string | null>(null);
  React.useEffect(() => {
    const read = () => setV(new URLSearchParams(window.location.search).get(name));
    read();
    window.addEventListener("popstate", read);
    return () => window.removeEventListener("popstate", read);
  }, [name]);
  const set = React.useCallback(
    (next: string | null) => {
      const u = new URL(window.location.href);
      if (next === null) u.searchParams.delete(name);
      else u.searchParams.set(name, next);
      window.history.replaceState(window.history.state, "", u.pathname + u.search);
      setV(next);
    },
    [name],
  );
  return [v, set];
}

const scoreText = (c: { scoring: Scoring }, s: Standing) =>
  c.scoring === "return_pct" ? pct(s.returnPct, 2) : c.scoring === "profit" ? usd(s.profit) : c.scoring === "contracts" ? `${num(s.contracts ?? 0)} contracts` : `${num(s.lots)} lots`;

export function LiveContests() {
  const perms = usePerms();
  const now = useNow();
  const ov = useApi<Overview>(M("overview"));
  const list = useApi<{ items: Contest[] }>(M("contests"), { refreshMs: 60_000 });
  const [f, setF] = React.useState<F>("all");
  const [wizard, setWizard] = React.useState(false);
  const [sel, setSel] = useUrlParam("contest");
  const [tabParam, setTabParam] = useUrlParam("tab");
  const tab = (["contests", "templates", "missions", "levels", "payouts", "settings"].includes(tabParam ?? "") ? tabParam : "contests") as ChallengesTab;
  const items = list.data?.items ?? [];
  const match = (c: Contest) => f === "all" || (f === "done" ? ["finalized", "paid", "cancelled"].includes(c.status) : f === "scheduled" ? c.status === "scheduled" || c.status === "draft" : c.status === f);
  const rows = items.filter(match).sort((a, b) => Date.parse(b.startsAt) - Date.parse(a.startsAt));
  const running = items.filter((c) => c.status === "running");
  const k = ov.data?.contests;

  return (
    <div className="pb-16">
      <PageHeader
        title="Contests & challenges"
        subtitle="Live contests from templates, missions, XP levels and streaks, leaderboards and prize payouts."
        actions={
          <>
            {perms.loaded && !perms.write && <ReadOnlyNote what="create contests" />}
            <Button variant="surface" onClick={() => (list.reload(), ov.reload())}>
              <RefreshCw /> Refresh
            </Button>
            {perms.write && (
              <Button variant="ember" onClick={() => setWizard(true)} data-testid="new-contest">
                <Plus /> New contest
              </Button>
            )}
          </>
        }
      />

      <ChallengesTabs tab={tab} onTab={(t) => setTabParam(t === "contests" ? null : t)} perms={perms} onNewContest={() => setWizard(true)} />

      <div className={cn("grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4", tab !== "contests" && "hidden")}>
        <KpiCard label="Running contests" icon={<Trophy />} value={<span className="k-num">{k ? int(k.running) : "—"}</span>} chip={k ? `${int(k.scheduled)} scheduled` : "Loading"} />
        <KpiCard label="Entrants" icon={<Users />} value={<span className="k-num">{k ? int(k.entrants) : "—"}</span>} chip="Running and scheduled" delay={0.05} />
        <KpiCard label="Live prize pools" icon={<Wallet />} value={<span className="k-num">{list.data ? usdK(running.reduce((s, c) => s + (c.prizePool || 0), 0)) : "—"}</span>} chip="Running contests" chipTone="gold" delay={0.1} />
        <KpiCard label="Awaiting payout" icon={<AlertTriangle />} value={<span className="k-num">{list.data ? int(items.filter((c) => c.status === "ended" || c.status === "finalized").length) : "—"}</span>} chip="Ended or finalized" chipTone="warn" delay={0.15} />
      </div>

      <Reveal delay={0.05} className={cn("mt-4", tab !== "contests" && "hidden")}>
        <Card>
          <CardHeader
            title="All contests"
            subtitle="Open a contest for its leaderboard, flags and payouts"
            action={
              <Segmented
                size="xs"
                value={f}
                onChange={setF}
                options={[
                  { value: "all", label: <>All <span className="text-fg-3">{items.length}</span></> },
                  { value: "running", label: "Running" },
                  { value: "scheduled", label: "Scheduled" },
                  { value: "ended", label: "Ended" },
                  { value: "done", label: "Closed" },
                ]}
              />
            }
          />
          <div className="mt-4 space-y-2 px-4 pb-6 sm:px-6">
            {list.error && !list.data ? (
              <MkError error={list.error} onRetry={list.reload} />
            ) : !list.data ? (
              <TableSkeleton rows={4} />
            ) : rows.length === 0 ? (
              <EmptyNote title={items.length ? "No contests match this filter" : "No contests yet"} text={items.length ? undefined : "Start from a template: a weekly % return contest, a monthly lots race or a weekend crypto sprint."} />
            ) : (
              rows.map((c) => {
                const endsIn = Date.parse(c.endsAt) - now;
                const startsIn = Date.parse(c.startsAt) - now;
                return (
                  <button
                    key={c.id}
                    type="button"
                    data-testid={`contest-row-${c.id}`}
                    onClick={() => setSel(String(c.id))}
                    className={cn("k-row relative flex w-full flex-wrap items-center gap-4 px-4 py-3 text-left transition-colors hover:bg-surface-3/60 sm:flex-nowrap", String(c.id) === sel && "border-ember/40")}
                  >
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="truncate text-[14px] font-medium">{c.name}</span>
                        <Chip size="sm" tone={c.kind === "live" ? "ember" : "gold"}>
                          {c.kind === "live" ? "LIVE" : "DEMO"}
                        </Chip>
                        <InstrumentChip c={c} />
                      </div>
                      <div className="mt-0.5 flex items-center gap-1.5 truncate text-[12px] text-fg-3">
                        <CalendarRange className="size-3.5 shrink-0" /> {when(c.startsAt)} – {when(c.endsAt)} · {SCORING[c.scoring] ?? c.scoring}
                        {c.minTrades ? ` · min ${c.minTrades} trades` : ""}
                        {isOptions(c) && c.minPremium ? ` · min premium ${usd(c.minPremium)}` : ""}
                      </div>
                    </div>
                    <div className="hidden w-36 md:block">
                      <div className="mb-1 flex justify-between text-[11px]">
                        <span className="k-num text-fg-2">{int(c.entrants)}</span>
                        <span className="k-num text-fg-3">/ {c.maxEntrants ? int(c.maxEntrants) : "∞"}</span>
                      </div>
                      <Progress value={c.maxEntrants ? (c.entrants / c.maxEntrants) * 100 : 0} tone={c.kind === "live" ? "ember" : "gold"} />
                    </div>
                    <div className="w-24 text-right">
                      <div className="k-num text-[14px] font-semibold text-gold">{usdK(c.prizePool)}</div>
                      <div className="text-[11px] text-fg-3">prize pool</div>
                    </div>
                    <div className="w-32 text-right">
                      <StatusPill map={CONTEST_STATUS} status={c.status} />
                      <div className="k-num mt-1 text-[11px] text-fg-3">
                        {c.status === "running" ? (endsIn > 0 ? `ends ${ago(c.endsAt, now)}` : "past end") : c.status === "scheduled" ? (startsIn > 0 ? `starts ${ago(c.startsAt, now)}` : "starting") : c.status === "ended" ? "finalize to freeze" : c.status === "finalized" ? "ready to pay" : ""}
                      </div>
                    </div>
                  </button>
                );
              })
            )}
          </div>
        </Card>
      </Reveal>

      <ContestDrawer id={sel} onClose={() => setSel(null)} onOpen={setSel} perms={perms} onChanged={() => (list.reload(), ov.reload())} />
      <ContestWizard open={wizard} onOpenChange={setWizard} onSaved={(c) => (list.reload(), ov.reload(), c?.id && setSel(String(c.id)))} />
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Detail drawer                                                        */
/* ------------------------------------------------------------------ */

function ContestDrawer({ id, onClose, onOpen, perms, onChanged }: { id: string | null; onClose: () => void; onOpen: (id: string) => void; perms: { write: boolean; approve: boolean }; onChanged: () => void }) {
  const valid = id && /^\d+$/.test(id) ? id : null;
  const { data, error, loading, reload } = useApi<ContestDetail>(valid ? M(`contests/${valid}`) : null, { refreshMs: 15_000 });
  const act = useAction();
  const [editing, setEditing] = React.useState(false);
  const now = useNow(15_000);
  const c = data && String(data.contest.id) === valid ? data.contest : null;
  const board = c ? data!.leaderboard : [];
  const flags = c ? data!.flags : [];
  const done = () => {
    reload();
    onChanged();
  };

  const post = <T = unknown,>(path: string, body: object = {}) => mkSend<T>(`contests/${valid}/${path}`, body);
  const ended = c ? Date.parse(c.endsAt) <= now : false;
  const canFinalize = !!c && (c.status === "ended" || (c.status === "running" && ended));
  const me = String(useStaff().id);
  const threshold = data?.fourEyesThreshold;
  const unpaid = board.filter((s) => s.status === "active" && s.prize && ["pending", "approval", AWAITING, "failed"].includes(s.prizeStatus));
  // what the bulk Pay does to each prize (four-eyes: at or above the threshold two different staff members)
  const payStep = (s: Standing): "pay" | "first" | "second" | "same" => {
    const first = s.fourEyes?.firstApprovedById ? String(s.fourEyes.firstApprovedById) : null;
    if (s.prizeStatus === AWAITING) return first === me ? "same" : "second";
    if (!first && threshold != null && (s.prize ?? 0) >= threshold) return "first";
    return "pay";
  };
  const STEP_LABEL = { pay: "Pays now", second: "2nd approval · pays now", first: "1st approval · nothing paid yet", same: "You gave the 1st approval · waits for another staff member" } as const;
  const payTotal = unpaid.filter((s) => ["pay", "second"].includes(payStep(s))).reduce((s, x) => s + (x.prize ?? 0), 0);
  const byEntry = new Map(board.map((s) => [s.entryId, s]));

  const refresh = async () => {
    const r = await post("refresh");
    if (r.ok) reload();
  };
  const duplicate = () =>
    act.ask({
      title: `Duplicate ${c!.name}`,
      description: "Creates a draft with the same settings and prizes for the next window of the same length. Review it and publish.",
      confirmLabel: "Create draft copy",
      confirmTestId: "contest-duplicate-confirm",
      note: "none",
      run: () => post<{ contest?: Contest }>("duplicate"),
      success: (r: { contest?: Contest }) => `Draft created: ${r.contest?.name ?? c!.name}`,
      onDone: (r: { contest?: Contest }) => {
        onChanged();
        if (r?.contest?.id) onOpen(String(r.contest.id));
      },
    });
  const saveTemplate = () =>
    act.ask({
      title: `Save ${c!.name} as a template`,
      description: "Keeps the scoring, eligibility, rules, anti-cheat and the prize split (as shares of the pool) for the next contest wizard.",
      confirmLabel: "Save template",
      note: "none",
      run: () => post("save-template", {}),
      success: "Template saved",
    });
  const finalize = () =>
    act.ask({
      title: `Finalize ${c!.name}`,
      description: "Freezes ranks and prize allocation. Scores stop updating. Open flags should be resolved first.",
      body: flags.some((f) => f.status === "open") ? <div className="rounded-[12px] border border-warn/30 bg-warn-soft px-3.5 py-2.5 text-[12.5px] text-fg">{flags.filter((f) => f.status === "open").length} flag(s) are still open.</div> : undefined,
      confirmLabel: "Finalize contest",
      confirmTestId: "contest-finalize-confirm",
      note: "none",
      run: () => post("finalize"),
      success: `${c!.name} finalized`,
      onDone: done,
    });
  const pay = () =>
    act.ask({
      title: `Pay prizes · ${c!.name}`,
      description: `Pays every unpaid prize, including those waiting for approval above the auto-pay threshold. ${fourEyesRule(threshold)} For those, this press is the first approval (nothing is paid) and another staff member's Pay releases them. Wallet prizes are credited to the client's wallet; credit prizes go to the entered live account as engine credit. Each payout is idempotent.`,
      body: (
        <div className="space-y-2">
          <div className="flex items-baseline justify-between rounded-[12px] border border-gold/30 bg-gold-soft px-4 py-3">
            <span className="text-[13px] text-fg-2">Paid by this press</span>
            <span className="k-num text-[22px] font-semibold text-gold" data-testid="contest-pay-total">
              {usd(payTotal)}
            </span>
          </div>
          <div className="max-h-48 space-y-1 overflow-y-auto">
            {unpaid.map((s) => (
              <div key={s.entryId} className="flex items-center justify-between px-1 text-[12.5px]">
                <span className="min-w-0 text-fg-2">
                  #{s.rank} · {s.name}
                  {s.login ? <span className="font-mono text-fg-3"> · {s.login}</span> : null}
                  <span className={cn("block text-[11px]", payStep(s) === "pay" || payStep(s) === "second" ? "text-up" : "text-warn")}>{STEP_LABEL[payStep(s)]}</span>
                </span>
                <span className="k-num">{usd(s.prize)}</span>
              </div>
            ))}
          </div>
        </div>
      ),
      confirmLabel: "Pay prizes",
      confirmVariant: "buy",
      confirmTestId: "contest-pay-confirm",
      note: "none",
      run: () => post<PayResult>("pay"),
      success: (r: PayResult) =>
        [
          `Paid ${int(r.paid ?? 0)} prize(s) · ${usd(r.amount ?? 0)}`,
          r.failed ? `${r.failed} failed, will retry` : "",
          r.firstApproved ? `${r.firstApproved} first approval(s) recorded · waiting for a second approver` : "",
          r.skippedSameStaff ? `${r.skippedSameStaff} wait for another staff member (four-eyes)` : "",
        ]
          .filter(Boolean)
          .join(" · "),
      onDone: done,
    });
  const cancel = () =>
    act.ask({
      title: `Cancel ${c!.name}`,
      description: "No prizes are paid. Entrants keep their accounts.",
      confirmLabel: "Cancel contest",
      confirmVariant: "sell",
      note: "required",
      run: (note) => post("cancel", { note }),
      success: `${c!.name} cancelled`,
      onDone: done,
    });
  const disqualify = (s: Standing) =>
    act.ask({
      title: `Disqualify ${s.name}`,
      description: `Entry #${s.entryId}${s.login ? ` · account ${s.login}` : ""}. The row stays on the board without a rank or prize.`,
      confirmLabel: "Disqualify",
      confirmVariant: "sell",
      note: "required",
      noteLabel: "Reason",
      run: (reason) => post(`entries/${s.entryId}/disqualify`, { reason }),
      success: `${s.name} disqualified`,
      onDone: done,
    });
  const reinstate = (s: Standing) =>
    act.ask({
      title: `Reinstate ${s.name}`,
      description: `Entry #${s.entryId} is ranked again from the next refresh.`,
      confirmLabel: "Reinstate",
      note: "required",
      noteLabel: "Reason",
      run: (reason) => post(`entries/${s.entryId}/reinstate`, { reason }),
      success: `${s.name} reinstated`,
      onDone: done,
    });
  const resolve = (f: ContestFlag, action: "clear" | "disqualify") =>
    act.ask({
      title: action === "clear" ? `Clear flag #${f.id}` : `Disqualify for flag #${f.id}`,
      description: `${FLAG_KIND[f.kind]?.label ?? f.kind} · ${byEntry.get(f.entryId)?.name ?? f.name ?? `entry #${f.entryId}`}`,
      confirmLabel: action === "clear" ? "Clear flag" : "Disqualify entrant",
      confirmVariant: action === "clear" ? "surface" : "sell",
      note: "required",
      run: (note) => post(`flags/${f.id}/resolve`, { action, note }),
      success: action === "clear" ? `Flag #${f.id} cleared` : "Entrant disqualified",
      onDone: done,
    });

  return (
    <Dialog
      open={!!valid}
      onOpenChange={(o) => !o && onClose()}
      width={1120}
      title={c ? c.name : "Contest"}
      description={c ? `${c.kind === "live" ? "Live accounts" : "Demo accounts"}${isOptions(c) ? " · Kalks FX Options" : ""} · ${SCORING[c.scoring] ?? c.scoring} · ${when(c.startsAt)} – ${when(c.endsAt)}` : undefined}
    >
      {error && !data ? (
        <MkError error={error} onRetry={reload} />
      ) : !c ? (
        <TableSkeleton rows={6} />
      ) : (
        <div className="space-y-5">
          <div className="flex flex-wrap items-center gap-2">
            <StatusPill map={CONTEST_STATUS} status={c.status} />
            <InstrumentChip c={c} />
            <span className="text-[12px] text-fg-3">Leaderboard refreshes every 15 s · updated {ago(c.updatedAt, now)}</span>
            {loading && <span className="text-[12px] text-fg-3">Loading…</span>}
            <span className="ml-auto flex flex-wrap gap-2">
              {perms.write && ["draft", "scheduled", "running", "ended"].includes(c.status) && (
                <Button size="sm" variant="surface" onClick={() => setEditing(true)} data-testid="contest-edit">
                  <Pencil /> Edit
                </Button>
              )}
              {perms.write && (
                <Button size="sm" variant="surface" onClick={duplicate} data-testid="contest-duplicate">
                  <Copy /> Duplicate
                </Button>
              )}
              {perms.write && (
                <Button size="sm" variant="ghost" onClick={saveTemplate} data-testid="contest-save-template">
                  <LayoutTemplate /> Save as template
                </Button>
              )}
              {perms.write && ["running", "ended"].includes(c.status) && (
                <Button size="sm" variant="surface" onClick={refresh}>
                  <RefreshCw /> Refresh scores
                </Button>
              )}
              {perms.write && canFinalize && (
                <Button size="sm" variant="ember" onClick={finalize} data-testid="contest-finalize">
                  Finalize
                </Button>
              )}
              {perms.approve && c.status === "finalized" && unpaid.length > 0 && (
                <Button size="sm" variant="buy" onClick={pay} data-testid="contest-pay">
                  <Wallet /> Pay prizes
                </Button>
              )}
              {perms.write && ["draft", "scheduled", "running"].includes(c.status) && (
                <Button size="sm" variant="down-outline" onClick={cancel}>
                  Cancel contest
                </Button>
              )}
            </span>
          </div>

          <div className={cn("grid grid-cols-2 gap-2", isOptions(c) ? "sm:grid-cols-3 lg:grid-cols-6" : "sm:grid-cols-5")}>
            <Tile label="Entrants" value={`${int(c.entrants)}${c.maxEntrants ? ` / ${int(c.maxEntrants)}` : ""}`} />
            <Tile label="Prize pool" value={usd(c.prizePool, 0)} tone="gold" />
            <Tile label="Min trades" value={int(c.minTrades)} />
            {isOptions(c) && <Tile label="Min premium / trade" value={c.minPremium ? usd(c.minPremium) : "None"} />}
            <Tile label={c.kind === "demo" ? "Starting balance" : "Min equity"} value={c.kind === "demo" ? usd(c.startingBalance, 0) : c.minEquity ? usd(c.minEquity, 0) : "None"} />
            <Tile label="Open flags" value={int(flags.filter((f) => f.status === "open").length)} tone={flags.some((f) => f.status === "open") ? "warn" : undefined} />
          </div>

          <div className="grid grid-cols-1 gap-4 lg:grid-cols-[1fr_300px]">
            <div className="min-w-0">
              <div className="k-label mb-2">Leaderboard</div>
              {board.length === 0 ? (
                <EmptyNote title="No entrants yet" text="Clients join from the Client Area under Rewards → Contests." />
              ) : (
                <div className="overflow-x-auto">
                  <table className="w-full min-w-[760px] text-[13px]" data-testid="contest-leaderboard">
                    <thead>
                      <tr className="text-[11px] uppercase tracking-[0.05em] text-fg-3">
                        <th className="pb-2 text-left font-medium">#</th>
                        <th className="pb-2 text-left font-medium">Trader</th>
                        <th className="pb-2 text-right font-medium">{SCORING[c.scoring]}</th>
                        <th className="pb-2 text-right font-medium">Profit</th>
                        <th className="pb-2 text-right font-medium">{isOptions(c) ? "Contracts" : "Lots"}</th>
                        <th className="pb-2 text-right font-medium">Trades</th>
                        <th className="pb-2 text-right font-medium">Prize</th>
                        <th className="pb-2 text-right font-medium" />
                      </tr>
                    </thead>
                    <tbody>
                      {board.map((s) => (
                        <tr key={s.entryId} className={cn("border-t border-line", s.status === "disqualified" && "opacity-60")}>
                          <td className="py-2 pr-2">
                            {s.rank ? (
                              <span className={cn("k-num grid size-6 place-items-center rounded-full text-[11px] font-semibold", s.rank <= 3 ? "bg-gold-soft text-gold" : "bg-surface-3 text-fg-2")}>{s.rank}</span>
                            ) : (
                              <span className="text-fg-3">—</span>
                            )}
                          </td>
                          <td className="py-2">
                            <div className="min-w-0">
                              <div className="flex items-center gap-1.5 truncate font-medium">
                                {s.name}
                                {s.country && <span className="text-[11px] font-normal text-fg-3" title={countryName(s.country)}>{s.country}</span>}
                              </div>
                              <div className="flex flex-wrap items-center gap-1 text-[11px] text-fg-3">
                                <span className="font-mono">
                                  {s.userId ? `#${s.userId}` : ""}
                                  {s.login ? ` · ${s.login}` : ""}
                                </span>
                                {!s.qualified && <Chip size="sm">below min trades</Chip>}
                                {!!s.selfTrades && (
                                  <Chip size="sm" tone="warn">
                                    {s.selfTrades} self-trade{s.selfTrades === 1 ? "" : "s"} left out
                                  </Chip>
                                )}
                                {!!s.smallTrades && <Chip size="sm">{s.smallTrades} below min premium</Chip>}
                                {s.status === "disqualified" && (
                                  <Chip size="sm" tone="down">
                                    Disqualified{s.disqualifyReason ? `: ${s.disqualifyReason}` : ""}
                                  </Chip>
                                )}
                                {(s.flags ?? []).map((fl) => (
                                  <Chip key={fl} size="sm" tone="warn">
                                    {FLAG_KIND[fl]?.label ?? fl}
                                  </Chip>
                                ))}
                              </div>
                            </div>
                          </td>
                          <td className="k-num py-2 text-right font-semibold">{scoreText(c, s)}</td>
                          <td className={cn("k-num py-2 text-right", s.profit > 0 ? "text-up" : s.profit < 0 ? "text-down" : "text-fg-2")}>{usd(s.profit)}</td>
                          <td className="k-num py-2 text-right text-fg-2">{isOptions(c) ? num(s.contracts ?? 0) : num(s.lots)}</td>
                          <td className="k-num py-2 text-right text-fg-2">{int(s.trades)}</td>
                          <td className="py-2 text-right">
                            {s.prize ? (
                              <span className="flex flex-col items-end gap-0.5">
                                <span className="k-num text-gold">{usd(s.prize, 0)}</span>
                                {s.prizeStatus && s.prizeStatus !== "none" && <StatusPill map={PAY_STATUS} status={s.prizeStatus} />}
                                {s.prizeStatus === AWAITING && <FirstApproval fe={s.fourEyes} className="flex max-w-56 items-center gap-1 text-[10.5px] text-warn" />}
                                {perms.approve && ["finalized", "paid"].includes(c.status) && ["approval", AWAITING].includes(s.prizeStatus) && (
                                  <FourEyesDecide<{ prize?: { status?: string } }>
                                    act={act}
                                    what="prize"
                                    status={s.prizeStatus}
                                    amount={s.prize}
                                    fe={s.fourEyes}
                                    threshold={threshold}
                                    approvePath={`contests/${c.id}/prizes/${s.entryId}/approve`}
                                    rejectPath={`contests/${c.id}/prizes/${s.entryId}/reject`}
                                    statusOf={(d) => d.prize?.status}
                                    paidText="Prize approved and paid"
                                    describe={`Rank #${s.rank} · ${s.prizePayout === "credit" ? "trading credit on the entered live account" : "cash to the client's wallet"}.`}
                                    onDone={done}
                                    testId={`prize-${s.entryId}`}
                                  />
                                )}
                              </span>
                            ) : (
                              <span className="text-fg-3">—</span>
                            )}
                          </td>
                          <td className="py-2 pl-2 text-right">
                            {perms.write && !["paid", "cancelled"].includes(c.status) && (
                              s.status === "active" ? (
                                <Button size="xs" variant="down-outline" onClick={() => disqualify(s)}>
                                  Disqualify
                                </Button>
                              ) : (
                                <Button size="xs" variant="surface" onClick={() => reinstate(s)}>
                                  Reinstate
                                </Button>
                              )
                            )}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>

            <div className="space-y-4">
              <div>
                <div className="k-label mb-2">Prizes</div>
                <div className="space-y-1.5">
                  {c.prizes.length === 0 ? (
                    <div className="text-[12.5px] text-fg-3">No prizes set.</div>
                  ) : (
                    c.prizes.map((p, i) => (
                      <div key={i} className={cn("k-row flex items-center gap-3 px-3 py-2", i === 0 && "border-gold/30 bg-gold-soft")}>
                        <span className="flex-1 text-[13px] font-medium">{p.rankFrom === p.rankTo ? `Rank ${p.rankFrom}` : `Ranks ${p.rankFrom}–${p.rankTo}`}</span>
                        <span className="text-[11px] text-fg-3">{p.payout === "credit" ? "credit" : "wallet"}</span>
                        <span className="k-num text-[13.5px] font-semibold">{usd(p.amount, 0)}</span>
                      </div>
                    ))
                  )}
                </div>
              </div>

              <div>
                <div className="k-label mb-2 flex items-center gap-1.5">
                  <FlagIcon className="size-3.5" /> Anti-cheat flags
                </div>
                {flags.length === 0 ? (
                  <div className="k-row px-3 py-3 text-[12.5px] text-fg-3">No flags. Checks run with every leaderboard refresh.</div>
                ) : (
                  <div className="space-y-2">
                    {flags.map((f) => {
                      const s = byEntry.get(f.entryId);
                      return (
                        <div key={f.id} className="k-row relative overflow-hidden px-3.5 py-2.5 pl-4">
                          <span className={cn("absolute inset-y-2 left-0 w-[3px] rounded-r-full", f.status === "open" ? "bg-warn" : f.status === "disqualified" ? "bg-down" : "bg-fg-3")} />
                          <div className="flex items-center justify-between gap-2">
                            <span className="truncate text-[13px] font-medium">{s?.name ?? f.name ?? `Entry #${f.entryId}`}</span>
                            <StatusPill map={PAY_STATUS} status={f.status} />
                          </div>
                          <div className="mt-0.5 text-[12px] text-fg-2">{FLAG_KIND[f.kind]?.label ?? f.kind}</div>
                          <div className="text-[11.5px] text-fg-3">{FLAG_KIND[f.kind]?.desc ?? ""}</div>
                          {f.note && <div className="mt-1 text-[11.5px] text-fg-3">Note: {f.note}</div>}
                          {perms.write && f.status === "open" && (
                            <div className="mt-2 flex gap-1.5">
                              <Button size="xs" variant="surface" onClick={() => resolve(f, "clear")}>
                                Clear
                              </Button>
                              <Button size="xs" variant="down-outline" onClick={() => resolve(f, "disqualify")}>
                                Disqualify
                              </Button>
                            </div>
                          )}
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>

              {isOptions(c) && (
                <div>
                  <div className="k-label mb-2">Options scoring</div>
                  <ul className="k-row space-y-1.5 px-3 py-2.5 text-[12px] text-fg-2" data-testid="contest-options-rules">
                    <li>Only Kalks FX Options trades count: closes, expiry settlements and knock-outs, on realised P&L (no floating part). CFD trades on the account don't count.</li>
                    <li>Volume is in contracts{c.minPremium ? `; a trade with an opening premium under ${usd(c.minPremium)} adds no volume and no trade count (its P&L still counts)` : ""}.</li>
                    <li>Self-trades between a client's own accounts (one fill on both, a hedge in the same series, or a cross at the same moment) are left out and flagged.</li>
                    <li>Joining needs the options intro and an account that can trade options (no copy, PAMM, MAM or prop). Options never earn loyalty points, cashback or bonus release.</li>
                  </ul>
                </div>
              )}

              {c.rules && (
                <div>
                  <div className="k-label mb-2">Rules</div>
                  <div className="k-row max-h-40 overflow-y-auto whitespace-pre-wrap px-3 py-2.5 text-[12px] text-fg-2">{c.rules}</div>
                </div>
              )}
            </div>
          </div>
        </div>
      )}
      {act.node}
      {c && <ContestWizard open={editing} onOpenChange={setEditing} initial={c} onSaved={() => done()} />}
    </Dialog>
  );
}
