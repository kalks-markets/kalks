"use client";

/**
 * Challenges tabs on Marketing › Contests (phase 4): Templates (the wizard's starting points, editable), Missions
 * (daily / weekly / one-off goals with XP and optional cash or credit), Levels (XP thresholds and badges), Payouts
 * (mission rewards and contest prizes waiting for approval above the auto-pay threshold; at or above the four-eyes
 * threshold two different staff members approve) and Settings (auto-pay, four-eyes threshold, auto-finalize, public
 * leaderboards, streaks, mission anti-abuse, XP awards). Every change goes to the growth
 * service through /api/marketing (marketing.write; approvals marketing.approve) and is audited there.
 */
import * as React from "react";
import { CircleDollarSign, Globe2, Layers, ListChecks, Medal, Pencil, Plus, Settings2, Trash2, Trophy } from "lucide-react";
import { Button, Card, CardHeader, Chip, Segmented, cn } from "@kalks/ui";
import { TableSkeleton, useApi, when } from "@/components/live/kit";
import { M, mkSend, type Scoring, type Settings } from "./api";
import { MISSION_KIND, SCHEDULE_LABEL, bandsOf, prizeTotal, splitPool, type ContestTemplate, type Level, type LevelsResp, type Mission, type MissionKind, type MissionsResp, type PayoutsResp, type TemplatesResp } from "./challenges-api";
import { AreaF, AuditNotice, EmptyNote, FormDialog, MkError, NumF, PAY_STATUS, PillPicker, SelectF, StatusPill, TextF, ToggleRow, int, numOrNull, usd, useAction } from "./kit";
import { FirstApproval, FourEyesDecide, fourEyesRule } from "./four-eyes";

export type ChallengesTab = "contests" | "templates" | "missions" | "levels" | "payouts" | "settings";
type Perms = { write: boolean; approve: boolean; loaded: boolean };

const SCORING_LABEL: Record<Scoring, string> = { return_pct: "Return %", profit: "Profit", lots: "Lots traded", contracts: "Contracts traded" };
const ASSETS = ["forex", "metals", "indices", "energies", "crypto", "stocks"];
const assetLabel = (a: string) => a[0]!.toUpperCase() + a.slice(1);

export function ChallengesTabs({ tab, onTab, perms, onNewContest }: { tab: ChallengesTab; onTab: (t: ChallengesTab) => void; perms: Perms; onNewContest: () => void }) {
  return (
    <>
      <div className="mb-4 overflow-x-auto">
        <Segmented
          value={tab}
          onChange={onTab}
          aria-label="Challenges"
          options={[
            { value: "contests", label: "Contests" },
            { value: "templates", label: "Templates" },
            { value: "missions", label: "Missions" },
            { value: "levels", label: "Levels" },
            { value: "payouts", label: "Payouts" },
            { value: "settings", label: "Settings" },
          ]}
        />
      </div>
      {tab === "templates" && <TemplatesPanel perms={perms} onNewContest={onNewContest} />}
      {tab === "missions" && <MissionsPanel perms={perms} />}
      {tab === "levels" && <LevelsPanel perms={perms} />}
      {tab === "payouts" && <PayoutsPanel perms={perms} />}
      {tab === "settings" && <SettingsPanel perms={perms} />}
    </>
  );
}

/* ------------------------------------------------------------------ */
/* Templates                                                            */
/* ------------------------------------------------------------------ */

function TemplatesPanel({ perms, onNewContest }: { perms: Perms; onNewContest: () => void }) {
  const { data, error, reload } = useApi<TemplatesResp>(M("contest-templates"));
  const [edit, setEdit] = React.useState<ContestTemplate | "new" | null>(null);
  return (
    <Card>
      <CardHeader
        title="Contest templates"
        subtitle="The starting points of the New contest wizard: scoring, schedule, eligibility, prize pool and split"
        icon={<Layers />}
        action={
          perms.write && (
            <div className="flex gap-2">
              <Button size="sm" variant="surface" onClick={() => setEdit("new")}>
                <Plus /> New template
              </Button>
              <Button size="sm" variant="ember" onClick={onNewContest}>
                <Trophy /> New contest
              </Button>
            </div>
          )
        }
      />
      <div className="mt-4 space-y-2 px-4 pb-6 sm:px-6">
        {error && !data ? (
          <MkError error={error} onRetry={reload} />
        ) : !data ? (
          <TableSkeleton rows={3} />
        ) : data.items.length === 0 ? (
          <EmptyNote title="No templates" text="Create one, or reload to restore the starter templates." />
        ) : (
          data.items.map((t) => {
            const prizes = splitPool(t.prizePool, bandsOf(t.split, data.presets), t.payout);
            return (
              <div key={t.id} className={cn("k-row flex flex-wrap items-center gap-4 px-4 py-3", !t.active && "opacity-60")} data-testid={`template-row-${t.key}`}>
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-[14px] font-medium">{t.name}</span>
                    <Chip size="sm" tone={t.kind === "live" ? "ember" : "gold"}>
                      {t.kind.toUpperCase()}
                    </Chip>
                    {!t.active && <Chip size="sm">Hidden</Chip>}
                  </div>
                  <div className="mt-0.5 truncate text-[12px] text-fg-3">
                    {SCORING_LABEL[t.scoring]} · {SCHEDULE_LABEL[t.schedule]}
                    {t.schedule === "next_monday" || t.schedule === "tomorrow" ? ` · ${t.durationDays} days` : ""}
                    {t.minTrades ? ` · min ${t.minTrades} trades` : ""}
                    {t.assetClasses.length ? ` · ${t.assetClasses.map(assetLabel).join(", ")} only` : ""}
                    {t.kycRequired ? " · KYC" : ""}
                  </div>
                </div>
                <div className="w-40 text-right">
                  <div className="k-num text-[14px] font-semibold text-gold">{usd(prizeTotal(prizes), 0)}</div>
                  <div className="text-[11px] text-fg-3">{typeof t.split === "string" ? (data.presets.find((p) => p.key === t.split)?.label ?? t.split) : `${t.split.length} custom bands`} · {t.payout === "credit" ? "credit" : "wallet"}</div>
                </div>
                {perms.write && (
                  <Button size="sm" variant="surface" onClick={() => setEdit(t)} aria-label={`Edit ${t.name}`}>
                    <Pencil /> Edit
                  </Button>
                )}
              </div>
            );
          })
        )}
      </div>
      {data && <TemplateDialog value={edit} presets={data} onClose={() => setEdit(null)} onSaved={reload} />}
    </Card>
  );
}

function TemplateDialog({ value, presets, onClose, onSaved }: { value: ContestTemplate | "new" | null; presets: TemplatesResp; onClose: () => void; onSaved: () => void }) {
  const t = value && value !== "new" ? value : null;
  const [f, setF] = React.useState({ name: "", description: "", scoring: "return_pct" as Scoring, schedule: "next_monday" as ContestTemplate["schedule"], days: "7", minTrades: "5", minEquity: "", pool: "1000", split: "top3", payout: "wallet" as "wallet" | "credit", rules: "", kyc: true, active: true, assets: [] as string[] });
  React.useEffect(() => {
    if (!value) return;
    setF({
      name: t?.name ?? "",
      description: t?.description ?? "",
      scoring: t?.scoring ?? "return_pct",
      schedule: t?.schedule ?? "next_monday",
      days: String(t?.durationDays ?? 7),
      minTrades: String(t?.minTrades ?? 5),
      minEquity: t?.minEquity ? String(t.minEquity) : "",
      pool: String(t?.prizePool ?? 1000),
      split: typeof t?.split === "string" ? t.split : t ? "custom" : "top3",
      payout: t?.payout ?? "wallet",
      rules: t?.rules ?? "",
      kyc: t?.kycRequired ?? true,
      active: t?.active ?? true,
      assets: t?.assetClasses ?? [],
    });
  }, [value, t]);
  const set = <K extends keyof typeof f>(k: K) => (v: (typeof f)[K]) => setF((x) => ({ ...x, [k]: v }));
  const submit = () => {
    if (!f.name.trim()) return "Give the template a name.";
    const body: Record<string, unknown> = {
      name: f.name.trim(),
      description: f.description.trim(),
      kind: "live",
      scoring: f.scoring,
      schedule: f.schedule,
      durationDays: Math.max(1, Math.round(Number(f.days) || 7)),
      minTrades: Math.max(0, Math.round(Number(f.minTrades) || 0)),
      minEquity: numOrNull(f.minEquity),
      prizePool: Number(f.pool) || 0,
      payout: f.payout,
      rules: f.rules.trim(),
      kycRequired: f.kyc,
      active: f.active,
      assetClasses: f.assets,
    };
    if (f.split !== "custom") body.split = f.split;
    return t ? mkSend<{ template: ContestTemplate }>(`contest-templates/${t.id}`, body, "PATCH") : mkSend<{ template: ContestTemplate }>("contest-templates", body);
  };
  const prizes = f.split === "custom" && t ? splitPool(Number(f.pool) || 0, bandsOf(t.split, presets.presets), f.payout) : splitPool(Number(f.pool) || 0, bandsOf(f.split, presets.presets), f.payout);
  return (
    <FormDialog
      open={!!value}
      onOpenChange={(o) => !o && onClose()}
      side="right"
      title={t ? `Edit ${t.name}` : "New template"}
      description="Live contests, free entry, prize pool funded by the broker."
      submitLabel={t ? "Save template" : "Create template"}
      submit={submit}
      success={t ? "Template saved" : "Template created"}
      onDone={onSaved}
      footerNote={<span className="k-num">Pool {usd(prizeTotal(prizes), 0)}</span>}
    >
      <TextF label="Name" value={f.name} onChange={set("name")} maxLength={80} />
      <AreaF label="Description" value={f.description} onChange={set("description")} rows={2} />
      <div className="grid grid-cols-2 gap-3">
        <SelectF label="Scoring" value={f.scoring} onChange={set("scoring")} options={(["return_pct", "profit", "lots"] as Scoring[]).map((s) => ({ value: s, label: SCORING_LABEL[s] }))} />
        <SelectF label="Schedule" value={f.schedule} onChange={set("schedule")} options={Object.entries(SCHEDULE_LABEL).map(([value, label]) => ({ value: value as ContestTemplate["schedule"], label }))} />
        <NumF label="Length" value={f.days} onChange={set("days")} suffix="days" step={1} hint="next week / from tomorrow" />
        <NumF label="Min trades" value={f.minTrades} onChange={set("minTrades")} step={1} />
        <NumF label="Prize pool" value={f.pool} onChange={set("pool")} prefix="$" />
        <SelectF label="Split" value={f.split} onChange={set("split")} options={[...presets.presets.map((p) => ({ value: p.key, label: p.label })), ...(t && typeof t.split !== "string" ? [{ value: "custom", label: "Custom bands (kept)" }] : [])]} />
        <SelectF label="Paid as" value={f.payout} onChange={set("payout")} options={[{ value: "wallet", label: "Cash to wallet" }, { value: "credit", label: "Trading credit" }]} />
        <NumF label="Minimum equity" value={f.minEquity} onChange={set("minEquity")} prefix="$" placeholder="None" />
      </div>
      <PillPicker label="Asset classes that count (none = all CFDs)" options={ASSETS.map((a) => ({ value: a, label: assetLabel(a) }))} value={f.assets} onChange={set("assets")} />
      <AreaF label="Rules" value={f.rules} onChange={set("rules")} rows={4} />
      <div className="k-row px-4">
        <ToggleRow label="Require verified KYC" checked={f.kyc} onChange={set("kyc")} />
        <ToggleRow label="Show in the wizard" checked={f.active} onChange={set("active")} />
      </div>
    </FormDialog>
  );
}

/* ------------------------------------------------------------------ */
/* Missions                                                             */
/* ------------------------------------------------------------------ */

function MissionsPanel({ perms }: { perms: Perms }) {
  const { data, error, reload } = useApi<MissionsResp>(M("missions"), { refreshMs: 60_000 });
  const [edit, setEdit] = React.useState<Mission | "new" | null>(null);
  return (
    <Card>
      <CardHeader
        title="Missions"
        subtitle="Goals clients complete for XP and, optionally, cash or trading credit. One reward per period."
        icon={<ListChecks />}
        action={
          perms.write && (
            <Button size="sm" variant="ember" onClick={() => setEdit("new")} data-testid="new-mission">
              <Plus /> New mission
            </Button>
          )
        }
      />
      <div className="mt-4 space-y-2 px-4 pb-6 sm:px-6">
        {error && !data ? (
          <MkError error={error} onRetry={reload} />
        ) : !data ? (
          <TableSkeleton rows={4} />
        ) : data.items.length === 0 ? (
          <EmptyNote title="No missions yet" text="Create a daily or weekly trading goal, or a one-off like Complete KYC." />
        ) : (
          data.items.map((m) => (
            <div key={m.id} className={cn("k-row flex flex-wrap items-center gap-4 px-4 py-3", !m.active && "opacity-60")} data-testid={`mission-row-${m.id}`}>
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="text-[14px] font-medium">{m.name}</span>
                  <Chip size="sm">{m.schedule === "once" ? "One-off" : m.schedule === "daily" ? "Daily" : "Weekly"}</Chip>
                  {!m.active && <Chip size="sm">Off</Chip>}
                </div>
                <div className="mt-0.5 truncate text-[12px] text-fg-3">
                  {MISSION_KIND[m.kind]?.label ?? m.kind}
                  {MISSION_KIND[m.kind]?.unit ? ` · ${m.target} ${MISSION_KIND[m.kind]!.unit}` : ""}
                  {m.params.assetClasses?.length ? ` · ${m.params.assetClasses.map(assetLabel).join(", ")}` : ""}
                  {m.endsAt ? ` · until ${when(m.endsAt)}` : ""}
                </div>
              </div>
              <div className="flex items-center gap-1.5">
                <Chip size="sm" tone="ember">
                  +{m.xp} XP
                </Chip>
                {m.rewardAmount ? (
                  <Chip size="sm" tone="gold">
                    {usd(m.rewardAmount, 0)} {m.rewardPayout === "credit" ? "credit" : "cash"}
                  </Chip>
                ) : null}
              </div>
              <div className="w-36 text-right text-[11.5px] text-fg-3">
                <div className="k-num text-[13px] font-medium text-fg">{int(m.completions)} done</div>
                {int(m.completions7d)} this week · {int(m.participants)} clients
              </div>
              {perms.write && (
                <Button size="sm" variant="surface" onClick={() => setEdit(m)} aria-label={`Edit ${m.name}`}>
                  <Pencil /> Edit
                </Button>
              )}
            </div>
          ))
        )}
        {data && (
          <p className="pt-1 text-[11.5px] text-fg-3">
            Trades count from {data.defaults.minLots} lots held at least {data.defaults.minHoldSeconds} s on live accounts (Settings). Cash rewards under {usd(data.defaults.autoPayBelow, 0)} are paid automatically; the rest appear under Payouts for approval.
          </p>
        )}
      </div>
      <MissionDialog value={edit} onClose={() => setEdit(null)} onSaved={reload} />
    </Card>
  );
}

function MissionDialog({ value, onClose, onSaved }: { value: Mission | "new" | null; onClose: () => void; onSaved: () => void }) {
  const m = value && value !== "new" ? value : null;
  const blank = { name: "", description: "", kind: "lots" as MissionKind, schedule: "weekly" as Mission["schedule"], target: "5", xp: "100", reward: "", payout: "wallet" as "wallet" | "credit", kyc: true, active: true, minLots: "", minHold: "", assets: [] as string[], endsAt: "" };
  const [f, setF] = React.useState(blank);
  React.useEffect(() => {
    if (!value) return;
    setF(
      m
        ? { name: m.name, description: m.description, kind: m.kind, schedule: m.schedule, target: String(m.target), xp: String(m.xp), reward: m.rewardAmount ? String(m.rewardAmount) : "", payout: m.rewardPayout ?? "wallet", kyc: m.kycRequired, active: m.active, minLots: m.params.minLots !== undefined ? String(m.params.minLots) : "", minHold: m.params.minHoldSeconds !== undefined ? String(m.params.minHoldSeconds) : "", assets: m.params.assetClasses ?? [], endsAt: m.endsAt ? m.endsAt.slice(0, 10) : "" }
        : blank,
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value]);
  const set = <K extends keyof typeof f>(k: K) => (v: (typeof f)[K]) => setF((x) => ({ ...x, [k]: v }));
  const kind = MISSION_KIND[f.kind];
  const once = !!kind?.once;
  const trades = f.kind === "lots" || f.kind === "trading_days" || f.kind === "hold";
  const submit = () => {
    if (!f.name.trim()) return "Give the mission a name.";
    if (!(Number(f.xp) >= 0)) return "XP must be 0 or more.";
    if (!(Number(f.xp) > 0) && !(Number(f.reward) > 0)) return "Give the mission XP or a reward.";
    const params: Record<string, unknown> = {};
    if (trades && f.minLots.trim()) params.minLots = Number(f.minLots);
    if (trades && f.minHold.trim()) params.minHoldSeconds = Math.round(Number(f.minHold));
    if (trades && f.assets.length) params.assetClasses = f.assets;
    const body = {
      name: f.name.trim(),
      description: f.description.trim(),
      kind: f.kind,
      schedule: once ? "once" : f.schedule,
      target: once ? 1 : Number(f.target),
      xp: Math.round(Number(f.xp) || 0),
      rewardAmount: numOrNull(f.reward),
      rewardPayout: f.payout,
      kycRequired: f.kyc,
      active: f.active,
      params,
      endsAt: f.endsAt || null,
    };
    return m ? mkSend<{ mission: Mission }>(`missions/${m.id}`, body, "PATCH") : mkSend<{ mission: Mission }>("missions", body);
  };
  return (
    <FormDialog
      open={!!value}
      onOpenChange={(o) => !o && onClose()}
      side="right"
      title={m ? `Edit ${m.name}` : "New mission"}
      description="Progress is measured from closed live trades, deposits, KYC, contest entries and referrals."
      submitLabel={m ? "Save mission" : "Create mission"}
      submitTestId="mission-form-submit"
      submit={submit}
      success={m ? "Mission saved" : "Mission created"}
      onDone={onSaved}
    >
      <TextF label="Name" value={f.name} onChange={set("name")} maxLength={80} placeholder="e.g. Trade 5 lots this week" />
      <AreaF label="Description" value={f.description} onChange={set("description")} rows={2} placeholder="Shown to clients under the mission name" />
      <div className="grid grid-cols-2 gap-3">
        <SelectF label="Goal" value={f.kind} onChange={set("kind")} options={(Object.keys(MISSION_KIND) as MissionKind[]).map((k) => ({ value: k, label: MISSION_KIND[k].label }))} className="col-span-2" />
        {!once && <SelectF label="Resets" value={f.schedule} onChange={set("schedule")} options={[{ value: "daily", label: "Daily" }, { value: "weekly", label: "Weekly (Monday)" }, { value: "once", label: "Never (one-off)" }]} />}
        {!once && <NumF label={kind?.target || "Target"} value={f.target} onChange={set("target")} suffix={kind?.unit} />}
        <NumF label="XP" value={f.xp} onChange={set("xp")} step={10} />
        <NumF label="Cash / credit reward" value={f.reward} onChange={set("reward")} prefix="$" placeholder="None (XP only)" />
        {Number(f.reward) > 0 && <SelectF label="Paid as" value={f.payout} onChange={set("payout")} options={[{ value: "wallet", label: "Cash to wallet" }, { value: "credit", label: "Trading credit" }]} />}
        <TextF label="Ends (optional)" value={f.endsAt} onChange={set("endsAt")} placeholder="YYYY-MM-DD" />
      </div>
      {trades && (
        <div className="space-y-3">
          <div className="grid grid-cols-2 gap-3">
            <NumF label="Min lots per trade" value={f.minLots} onChange={set("minLots")} placeholder="Default" />
            <NumF label="Min hold" value={f.minHold} onChange={set("minHold")} suffix="seconds" placeholder="Default" step={1} />
          </div>
          <PillPicker label="Only these asset classes (none = all)" options={ASSETS.map((a) => ({ value: a, label: assetLabel(a) }))} value={f.assets} onChange={set("assets")} />
        </div>
      )}
      <div className="k-row px-4">
        {Number(f.reward) > 0 && <ToggleRow label="Reward needs verified KYC" hint="XP is given at once; the cash waits until the client is verified" checked={f.kyc} onChange={set("kyc")} />}
        <ToggleRow label="Active" checked={f.active} onChange={set("active")} />
      </div>
      <AuditNotice />
    </FormDialog>
  );
}

/* ------------------------------------------------------------------ */
/* Levels                                                               */
/* ------------------------------------------------------------------ */

const BADGE_TONE: Record<string, string> = { bronze: "#d98b4a", silver: "#c7ccd4", gold: "#e9b949", platinum: "#b9c6cf", diamond: "#9fe3f5", crown: "#e9b949", rocket: "#ff7a45", gem: "#9fe3f5" };

function LevelsPanel({ perms }: { perms: Perms }) {
  const { data, error, reload } = useApi<LevelsResp>(M("levels"));
  const [rows, setRows] = React.useState<Level[] | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [err, setErr] = React.useState<string | null>(null);
  React.useEffect(() => {
    if (data) setRows(data.levels.map((l) => ({ ...l })));
  }, [data]);
  const upd = (i: number, p: Partial<Level>) => setRows((x) => x!.map((l, j) => (j === i ? { ...l, ...p } : l)));
  const save = async () => {
    setBusy(true);
    setErr(null);
    const r = await mkSend<{ levels: Level[] }>("levels", { levels: rows!.map((l, i) => ({ level: i + 1, name: l.name, minXp: Math.round(Number(l.minXp) || 0), badge: l.badge })) }, "PUT");
    setBusy(false);
    if (!r.ok) return setErr(r.error.message);
    reload();
  };
  return (
    <Card>
      <CardHeader title="Levels" subtitle="XP thresholds, names and badges clients see in the Client Area, the app and on leaderboards" icon={<Medal />} />
      <div className="mt-4 space-y-2 px-4 pb-6 sm:px-6">
        {error && !data ? (
          <MkError error={error} onRetry={reload} />
        ) : !rows || !data ? (
          <TableSkeleton rows={6} />
        ) : (
          <>
            {rows.map((l, i) => (
              <div key={i} className="grid grid-cols-[40px_1.4fr_1fr_1fr_auto_auto] items-end gap-2" data-testid={`level-row-${i + 1}`}>
                <span className="k-num grid size-10 place-items-center rounded-full text-[13px] font-bold text-black/70" style={{ background: BADGE_TONE[l.badge] ?? "#d98b4a" }}>
                  {i + 1}
                </span>
                <TextF label={`Name (level ${i + 1})`} value={l.name} onChange={(v) => upd(i, { name: v })} disabled={!perms.write} maxLength={40} />
                <NumF label="From XP" value={String(l.minXp)} onChange={(v) => upd(i, { minXp: Number(v) })} step={50} disabled={!perms.write || i === 0} />
                <SelectF label="Badge" value={l.badge} onChange={(v) => upd(i, { badge: v })} options={data.badges.map((b) => ({ value: b, label: b[0]!.toUpperCase() + b.slice(1) }))} />
                <span className="k-num pb-2.5 text-[12px] text-fg-3">{int(l.clients ?? 0)} clients</span>
                {perms.write && i > 0 ? (
                  <button type="button" onClick={() => setRows((x) => x!.filter((_, j) => j !== i))} className="grid size-10 place-items-center rounded-[12px] border border-line text-fg-3 hover:text-down" aria-label={`Remove level ${i + 1}`}>
                    <Trash2 className="size-4" />
                  </button>
                ) : (
                  <span className="size-10" />
                )}
              </div>
            ))}
            {perms.write && (
              <div className="flex flex-wrap gap-2 pt-2">
                <Button size="sm" variant="surface" onClick={() => setRows((x) => [...x!, { level: x!.length + 1, name: `Level ${x!.length + 1}`, minXp: (x!.at(-1)?.minXp ?? 0) * 2 || 100, badge: "gold" }])}>
                  <Plus /> Add level
                </Button>
                <Button size="sm" variant="ember" disabled={busy} onClick={save} data-testid="levels-save">
                  {busy ? "Saving…" : "Save levels"}
                </Button>
              </div>
            )}
            {err && <div className="text-[12.5px] text-down">{err}</div>}
            <p className="text-[11.5px] text-fg-3">XP is never taken away when thresholds change: a client&apos;s level is recalculated from their XP total.</p>
          </>
        )}
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Payouts                                                              */
/* ------------------------------------------------------------------ */

/** Auto-pay releases only amounts under min(autoPayBelow, fourEyesThreshold). */
function autoPayText(autoPayBelow: number, fourEyes: number | undefined): string {
  const limit = Math.min(autoPayBelow, fourEyes ?? autoPayBelow);
  if (autoPayBelow <= 0) return "Auto-pay is off: every prize and mission reward waits for approval. Set a threshold under Settings.";
  if (limit <= 0) return "Auto-pay releases nothing while the four-eyes threshold is 0.";
  return `Prizes and mission rewards under ${usd(limit, 0)} are paid automatically; the rest wait here for marketing.approve.`;
}

type PF = "open" | "awaiting_second_approval" | "failed" | "kyc" | "pending" | "paid" | "rejected" | "all";

function PayoutsPanel({ perms }: { perms: Perms }) {
  const [f, setF] = React.useState<PF>("open");
  const { data, error, reload } = useApi<PayoutsResp>(M(`payouts${f === "all" ? "" : `?status=${f}`}`), { refreshMs: 30_000 });
  const act = useAction();
  const openContest = (id: number | null) => {
    if (!id) return;
    const u = new URL(window.location.href);
    u.searchParams.delete("tab");
    u.searchParams.set("contest", String(id));
    window.location.assign(u.pathname + u.search);
  };
  return (
    <Card>
      <CardHeader
        title="Payouts"
        subtitle={
          data
            ? `${autoPayText(data.autoPayBelow, data.fourEyesThreshold)} ${fourEyesRule(data.fourEyesThreshold)}`
            : "Mission rewards and contest prizes"
        }
        icon={<CircleDollarSign />}
        action={
          <Segmented
            size="xs"
            value={f}
            onChange={setF}
            options={[
              { value: "open", label: "Needs approval" },
              { value: "awaiting_second_approval", label: "Awaiting 2nd" },
              { value: "failed", label: "Failed" },
              { value: "kyc", label: "Waiting KYC" },
              { value: "pending", label: "Paying" },
              { value: "paid", label: "Paid" },
              { value: "rejected", label: "Rejected" },
              { value: "all", label: "All" },
            ]}
          />
        }
      />
      <div className="mt-4 space-y-2 px-4 pb-6 sm:px-6" data-testid="payouts">
        {error && !data ? (
          <MkError error={error} onRetry={reload} />
        ) : !data ? (
          <TableSkeleton rows={4} />
        ) : data.items.length === 0 ? (
          <EmptyNote title={f === "open" ? "Nothing waits for approval" : "No payouts here"} />
        ) : (
          data.items.map((r) => (
            <div key={`${r.source}-${r.id}`} className="k-row flex flex-wrap items-center gap-4 px-4 py-3">
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="truncate text-[13.5px] font-medium">{r.name || `Client #${r.userId}`}</span>
                  <span className="font-mono text-[11px] text-fg-3">#{r.userId}</span>
                  <Chip size="sm">{r.source === "mission" ? "Mission" : "Contest prize"}</Chip>
                </div>
                <div className="truncate text-[12px] text-fg-3">
                  {r.title}
                  {r.at ? ` · ${when(r.at)}` : ""}
                  {r.error ? ` · ${r.error}` : ""}
                </div>
                <FirstApproval fe={r.fourEyes} />
              </div>
              <div className="w-28 text-right">
                <div className="k-num text-[14px] font-semibold text-gold">{usd(r.amount)}</div>
                <div className="text-[11px] text-fg-3">{r.payout === "credit" ? "credit" : "wallet"}</div>
              </div>
              <StatusPill map={PAY_STATUS} status={r.status} />
              {perms.approve && (r.source === "mission" ? ["approval", "awaiting_second_approval", "failed"] : ["approval", "awaiting_second_approval"]).includes(r.status) && (
                <FourEyesDecide<{ reward?: { rewardStatus?: string }; prize?: { status?: string } }>
                  act={act}
                  what={r.source === "mission" ? "reward" : "prize"}
                  status={r.status}
                  amount={r.amount}
                  fe={r.fourEyes}
                  threshold={data.fourEyesThreshold}
                  approvePath={r.source === "mission" ? `missions/rewards/${r.id}/approve` : `contests/${r.contestId}/prizes/${r.id}/approve`}
                  rejectPath={r.source === "mission" ? `missions/rewards/${r.id}/reject` : `contests/${r.contestId}/prizes/${r.id}/reject`}
                  statusOf={(d) => d.reward?.rewardStatus ?? d.prize?.status}
                  paidText={r.source === "mission" ? "Reward approved and paid" : "Prize approved and paid"}
                  describe={`${r.title} · ${r.payout === "credit" ? "trading credit on the client's live account" : "cash to the client's wallet"}.`}
                  onDone={reload}
                  testId={`payout-${r.source}-${r.id}`}
                />
              )}
              {r.source === "contest" && ["approval", "awaiting_second_approval", "failed", "pending"].includes(r.status) && (
                <Button size="xs" variant="surface" onClick={() => openContest(r.contestId)}>
                  Open contest
                </Button>
              )}
            </div>
          ))
        )}
      </div>
      {act.node}
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Settings                                                             */
/* ------------------------------------------------------------------ */

function SettingsPanel({ perms }: { perms: Perms }) {
  const { data, error, reload } = useApi<Settings>(M("settings"));
  const [f, setF] = React.useState<Record<string, string | boolean> | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [err, setErr] = React.useState<string | null>(null);
  const [saved, setSaved] = React.useState(false);
  React.useEffect(() => {
    if (!data) return;
    setF({
      autoPayBelow: String(data.autoPayBelow ?? 0),
      fourEyesThreshold: String(data.fourEyesThreshold ?? 1000),
      autoFinalize: !!data.autoFinalize,
      autoFinalizeDelayMinutes: String(data.autoFinalizeDelayMinutes ?? 60),
      publicLeaderboards: data.publicLeaderboards ?? true,
      nicknamePublicDefault: data.nicknamePublicDefault ?? true,
      streakGraceDays: String(data.streakGraceDays ?? 1),
      missionMinLots: String(data.missionMinLots ?? 0.01),
      missionMinHoldSeconds: String(data.missionMinHoldSeconds ?? 60),
      xpContestJoin: String(data.xpContestJoin ?? 25),
      xpStreakWeek: String(data.xpStreakWeek ?? 50),
    });
  }, [data]);
  if (error && !data) return <MkError error={error} onRetry={reload} />;
  if (!f) return <TableSkeleton rows={6} />;
  const s = (k: string) => (v: string) => setF((x) => ({ ...x!, [k]: v }));
  const b = (k: string) => (v: boolean) => setF((x) => ({ ...x!, [k]: v }));
  const n = (k: string) => Number(f[k] as string);
  const save = async () => {
    setBusy(true);
    setErr(null);
    setSaved(false);
    const r = await mkSend<Settings>(
      "settings",
      {
        autoPayBelow: n("autoPayBelow") || 0,
        // empty keeps the current threshold (0 would mean "every payout")
        fourEyesThreshold: String(f.fourEyesThreshold).trim() === "" ? undefined : n("fourEyesThreshold"),
        autoFinalize: f.autoFinalize,
        autoFinalizeDelayMinutes: Math.round(n("autoFinalizeDelayMinutes") || 0),
        publicLeaderboards: f.publicLeaderboards,
        nicknamePublicDefault: f.nicknamePublicDefault,
        streakGraceDays: Math.round(n("streakGraceDays") || 0),
        missionMinLots: n("missionMinLots") || 0,
        missionMinHoldSeconds: Math.round(n("missionMinHoldSeconds") || 0),
        xpContestJoin: Math.round(n("xpContestJoin") || 0),
        xpStreakWeek: Math.round(n("xpStreakWeek") || 0),
      },
      "PUT",
    );
    setBusy(false);
    if (!r.ok) return setErr(r.error.message);
    setSaved(true);
    reload();
  };
  return (
    <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
      <Card>
        <CardHeader title="Payouts" subtitle="Who approves money out" icon={<CircleDollarSign />} />
        <div className="space-y-3 px-6 pb-6 pt-3">
          <NumF label="Pay automatically below" value={f.autoPayBelow as string} onChange={s("autoPayBelow")} prefix="$" disabled={!perms.write} hint="Contest prizes and mission rewards under this amount (and under the four-eyes threshold) are paid when the results are final; larger ones wait for marketing.approve. 0 = staff approve every payout." />
          <NumF
            label="Four-eyes threshold (USD)"
            value={f.fourEyesThreshold as string}
            onChange={s("fourEyesThreshold")}
            prefix="$"
            disabled={!perms.write}
            hint="Payouts at or above this amount need a second staff member to approve. 0 = every payout. Applies to contest prizes, mission rewards, cashback payouts and cashback redemptions; never paid automatically."
          />
          <div className="k-row px-4">
            <ToggleRow label="Finalize contests automatically" hint="After the end, when no anti-cheat flag is open" checked={!!f.autoFinalize} onChange={b("autoFinalize")} disabled={!perms.write} />
          </div>
          {f.autoFinalize && <NumF label="Finalize after" value={f.autoFinalizeDelayMinutes as string} onChange={s("autoFinalizeDelayMinutes")} suffix="minutes after the end" step={5} disabled={!perms.write} />}
        </div>
      </Card>
      <Card>
        <CardHeader title="Leaderboards" subtitle="Nicknames only: never names, emails or account numbers" icon={<Globe2 />} />
        <div className="space-y-3 px-6 pb-6 pt-3">
          <div className="k-row px-4">
            <ToggleRow label="Public leaderboard API" hint="GET https://api.kalkstrade.com/v1/public/leaderboard for the website (cached 15 s)" checked={!!f.publicLeaderboards} onChange={b("publicLeaderboards")} disabled={!perms.write} />
            <ToggleRow label="Show clients publicly by default" hint="Clients can always change it in their profile" checked={!!f.nicknamePublicDefault} onChange={b("nicknamePublicDefault")} disabled={!perms.write} />
          </div>
        </div>
      </Card>
      <Card>
        <CardHeader title="Missions and streaks" subtitle="Anti-abuse and streak rules" icon={<ListChecks />} />
        <div className="grid grid-cols-2 gap-3 px-6 pb-6 pt-3">
          <NumF label="Min lots per trade" value={f.missionMinLots as string} onChange={s("missionMinLots")} disabled={!perms.write} />
          <NumF label="Min hold per trade" value={f.missionMinHoldSeconds as string} onChange={s("missionMinHoldSeconds")} suffix="seconds" step={1} disabled={!perms.write} />
          <NumF label="Streak grace" value={f.streakGraceDays as string} onChange={s("streakGraceDays")} suffix="missed days / week" step={1} disabled={!perms.write} hint="Weekends never break a trading streak" />
        </div>
      </Card>
      <Card>
        <CardHeader title="XP awards" subtitle="Besides each mission's own XP" icon={<Settings2 />} />
        <div className="grid grid-cols-2 gap-3 px-6 pb-6 pt-3">
          <NumF label="Joining a live contest" value={f.xpContestJoin as string} onChange={s("xpContestJoin")} suffix="XP" step={5} disabled={!perms.write} />
          <NumF label="Every 7 days of trading streak" value={f.xpStreakWeek as string} onChange={s("xpStreakWeek")} suffix="XP" step={5} disabled={!perms.write} />
        </div>
      </Card>
      {perms.write && (
        <div className="flex items-center gap-3 xl:col-span-2">
          <Button variant="ember" disabled={busy} onClick={save} data-testid="challenges-settings-save">
            {busy ? "Saving…" : "Save settings"}
          </Button>
          {saved && <span className="text-[12.5px] text-up">Saved · recorded in the marketing audit log</span>}
          {err && <span className="text-[12.5px] text-down">{err}</span>}
        </div>
      )}
    </div>
  );
}
