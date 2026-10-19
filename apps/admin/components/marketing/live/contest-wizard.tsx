"use client";

/**
 * Contest wizard (phase 4): 1 pick a template, 2 dates and prize pool, 3 review and publish. Everything the old long
 * form had (account type, instrument, scoring, eligibility, manual prize rows, rules, anti-cheat) sits behind
 * "More options" on step 2. Editing a contest opens on step 2; once a contest has started the service locks timing,
 * scoring, eligibility and prizes, so those fields are read-only here too.
 */
import * as React from "react";
import { ChevronDown, ChevronLeft, ChevronRight, LayoutTemplate, Plus, Trash2, Trophy, Wand2 } from "lucide-react";
import { toast } from "sonner";
import { Button, Chip, Dialog, Segmented, Stepper, cn } from "@kalks/ui";
import { ErrorBanner } from "@/components/trading-desk/kit";
import { useApi } from "@/components/live/kit";
import type { LiveGroup } from "@/lib/trading-desk";
import { M, errText, mkSend, type AntiCheat, type Contest, type ContestInput, type ContestInstrument, type Prize, type Scoring } from "./api";
import { SCHEDULE_LABEL, bandsError, bandsOf, nextWindow, prizeTotal, splitPool, type Band, type ContestTemplate, type SplitPreset, type TemplatesResp } from "./challenges-api";
import { AreaF, DateTimeF, NumF, PillPicker, SelectF, TextF, ToggleRow, fromLocalInput, int, numOrNull, splitList, toLocalInput, usd } from "./kit";

const SCORING_LABEL: Record<Scoring, string> = { return_pct: "Return %", profit: "Profit", lots: "Lots traded", contracts: "Contracts traded" };
const ASSET_LABEL: Record<string, string> = { forex: "Forex", metals: "Metals", indices: "Indices", energies: "Energies", crypto: "Crypto", stocks: "Stocks" };

function scoringOptions(instrument: ContestInstrument): { value: Scoring; label: string }[] {
  return [
    { value: "return_pct", label: instrument === "options" ? "Return % (realised option P&L ÷ start equity)" : "Return % (realised + floating ÷ start equity)" },
    { value: "profit", label: instrument === "options" ? "Profit (realised option P&L, USD)" : "Profit (USD)" },
    instrument === "options" ? { value: "contracts", label: "Contracts traded" } : { value: "lots", label: "Lots traded" },
  ];
}

const numText = (v: number | null | undefined) => (v === null || v === undefined ? "" : String(v));
type PrizeRow = { rankFrom: string; rankTo: string; amount: string; payout: "wallet" | "credit" };
const DEFAULT_ANTI: AntiCheat = { minHoldSeconds: 60, maxSingleTradePct: 80, disqualifyOnBalanceChange: true };

/** Live account groups for the picker (needs accounts.read); null = not available, free text instead. */
function useLiveGroups(open: boolean): { groups: { code: string; name: string }[] | null; loading: boolean } {
  const { data, error } = useApi<{ groups: LiveGroup[] }>(open ? "/api/trading/admin/groups" : null);
  if (error) return { groups: null, loading: false };
  if (!data) return { groups: [], loading: true };
  return { groups: data.groups.filter((g) => g.enabled && g.accountTypes !== "demo").map((g) => ({ code: g.code, name: g.name })), loading: false };
}

/** Pick live groups (all = none picked). Falls back to a comma-separated field when the groups can't be read. */
function GroupPicker({ value, onChange, open, disabled }: { value: string[]; onChange: (v: string[]) => void; open: boolean; disabled?: boolean }) {
  const { groups, loading } = useLiveGroups(open);
  const [text, setText] = React.useState(value.join(", "));
  React.useEffect(() => setText(value.join(", ")), [value]);
  if (groups === null)
    return <TextF label="Account groups" value={text} onChange={(v) => (setText(v), onChange(splitList(v).map((x) => x.toLowerCase())))} placeholder="All live groups" hint="comma separated group codes" disabled={disabled} />;
  const known = new Set(groups.map((g) => g.code));
  const options = [...groups.map((g) => ({ value: g.code, label: g.name || g.code })), ...value.filter((v) => !known.has(v)).map((v) => ({ value: v, label: v }))];
  return (
    <div className={cn(disabled && "pointer-events-none opacity-60")}>
      {loading ? (
        <div className="text-[12px] text-fg-3">Loading account groups…</div>
      ) : options.length === 0 ? (
        <div className="text-[12px] text-fg-3">No live groups found: every live account can join.</div>
      ) : (
        <PillPicker label="Account groups" options={options} value={value} onChange={onChange} />
      )}
      <div className="mt-1 text-[11.5px] text-fg-3">{value.length ? `Only ${value.length} group(s) can join.` : "None picked: every live group can join."}</div>
    </div>
  );
}

/** Pool + split preset → prize table, with the per-rank amounts and the total. */
function PrizeCalculator({ pool, setPool, split, setSplit, payout, setPayout, presets, demo, disabled }: { pool: string; setPool: (v: string) => void; split: string; setSplit: (v: string) => void; payout: "wallet" | "credit"; setPayout: (v: "wallet" | "credit") => void; presets: SplitPreset[]; demo: boolean; disabled?: boolean }) {
  const bands = bandsOf(split, presets);
  const rows = splitPool(Number(pool) || 0, bands, demo ? "wallet" : payout);
  return (
    <div className={cn("space-y-3", disabled && "pointer-events-none opacity-60")}>
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <NumF label="Prize pool" value={pool} onChange={setPool} prefix="$" step={50} hint="Funded by the broker; entry is free" />
        <div>
          <div className="mb-1.5 text-[12.5px] font-medium text-fg-2">Paid as</div>
          {demo ? (
            <div className="flex h-10 items-center text-[12.5px] text-fg-3">Wallet (demo contests)</div>
          ) : (
            <Segmented
              size="sm"
              value={payout}
              onChange={setPayout}
              options={[
                { value: "wallet", label: "Cash to wallet" },
                { value: "credit", label: "Trading credit" },
              ]}
            />
          )}
        </div>
      </div>
      <div>
        <div className="mb-1.5 text-[12.5px] font-medium text-fg-2">Split</div>
        <div role="group" aria-label="Prize split" className="flex flex-wrap gap-1.5">
          {presets.map((p) => (
            <button
              key={p.key}
              type="button"
              aria-pressed={split === p.key}
              onClick={() => setSplit(p.key)}
              className={cn("inline-flex h-8 items-center rounded-full border px-3 text-[12.5px] font-medium transition-colors", split === p.key ? "border-ember/40 bg-ember-soft text-ember" : "border-line bg-surface-2 text-fg-2 hover:text-fg")}
            >
              {p.label}
            </button>
          ))}
        </div>
      </div>
      <div className="k-row divide-y divide-line px-4" data-testid="prize-calculator">
        {rows.length === 0 ? (
          <div className="py-3 text-[12.5px] text-fg-3">Set a prize pool to see the prizes (or keep it at 0 for a ranking-only contest).</div>
        ) : (
          rows.map((r) => (
            <div key={`${r.rankFrom}-${r.rankTo}`} className="flex items-center justify-between py-2 text-[13px]">
              <span className="text-fg-2">{r.rankFrom === r.rankTo ? `Rank ${r.rankFrom}` : `Ranks ${r.rankFrom}–${r.rankTo}`}</span>
              <span className="k-num font-semibold">
                {usd(r.amount, r.amount % 1 ? 2 : 0)}
                {r.rankTo > r.rankFrom && <span className="font-normal text-fg-3"> each</span>}
              </span>
            </div>
          ))
        )}
        {rows.length > 0 && (
          <div className="flex items-center justify-between py-2.5 text-[13px]">
            <span className="text-fg-3">Total</span>
            <span className="k-num text-[15px] font-semibold text-gold">{usd(prizeTotal(rows), 0)}</span>
          </div>
        )}
      </div>
    </div>
  );
}

/** Step 1: template cards (and a blank contest). */
function TemplatePicker({ data, error, onPick }: { data: TemplatesResp | null; error: boolean; onPick: (t: ContestTemplate | null) => void }) {
  const items = (data?.items ?? []).filter((t) => t.active);
  return (
    <div className="grid grid-cols-1 gap-3 md:grid-cols-2" data-testid="template-picker">
      {!data && !error
        ? [0, 1, 2].map((i) => <div key={i} className="k-row h-[132px] animate-pulse" />)
        : items.map((t) => (
            <button
              key={t.id}
              type="button"
              onClick={() => onPick(t)}
              data-testid={`template-${t.key}`}
              className="k-row group flex flex-col items-start gap-2 px-4 py-3.5 text-left transition-colors hover:border-ember/40 hover:bg-ember-soft/30"
            >
              <div className="flex w-full items-start justify-between gap-2">
                <span className="text-[14px] font-medium">{t.name}</span>
                <ChevronRight className="size-4 shrink-0 text-fg-3 transition-transform group-hover:translate-x-0.5" />
              </div>
              <span className="line-clamp-2 text-[12px] text-fg-3">{t.description}</span>
              <span className="mt-auto flex flex-wrap gap-1.5">
                <Chip size="sm" tone={t.kind === "live" ? "ember" : "gold"}>
                  {t.kind.toUpperCase()}
                </Chip>
                <Chip size="sm">{SCORING_LABEL[t.scoring]}</Chip>
                <Chip size="sm">{SCHEDULE_LABEL[t.schedule]}</Chip>
                {t.assetClasses.length > 0 && <Chip size="sm">{t.assetClasses.map((a) => ASSET_LABEL[a] ?? a).join(", ")}</Chip>}
                <Chip size="sm" tone="gold">
                  {usd(t.prizePool, 0)}
                </Chip>
              </span>
            </button>
          ))}
      <button type="button" onClick={() => onPick(null)} data-testid="template-blank" className="k-row flex flex-col items-start gap-2 border-dashed px-4 py-3.5 text-left transition-colors hover:border-ember/40">
        <span className="flex items-center gap-2 text-[14px] font-medium">
          <Plus className="size-4" /> Blank contest
        </span>
        <span className="text-[12px] text-fg-3">Start from scratch. Every option is under More options on the next step.</span>
      </button>
      {error && <div className="text-[12px] text-fg-3 md:col-span-2">Templates couldn&apos;t be loaded; start from a blank contest.</div>}
    </div>
  );
}

function Review({ k, v }: { k: string; v: React.ReactNode }) {
  return (
    <div className="flex items-start justify-between gap-4 border-b border-line py-2 text-[13px] last:border-0">
      <span className="shrink-0 text-fg-3">{k}</span>
      <span className="text-right">{v}</span>
    </div>
  );
}

export function ContestWizard({ open, onOpenChange, onSaved, initial }: { open: boolean; onOpenChange: (o: boolean) => void; onSaved: (c: Contest | undefined) => void; initial?: Contest }) {
  const editing = !!initial;
  const locked = !!initial && !["draft", "scheduled"].includes(initial.status);
  const tpl = useApi<TemplatesResp>(open ? M("contest-templates") : null);
  const presets = tpl.data?.presets?.length ? tpl.data.presets : [];
  const autoPay = tpl.data?.autoPayBelow ?? 0;

  const [step, setStep] = React.useState(0);
  const [more, setMore] = React.useState(false);
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [templateId, setTemplateId] = React.useState<number | null>(null);
  const [name, setName] = React.useState("");
  const [description, setDescription] = React.useState("");
  const [kind, setKind] = React.useState<"demo" | "live">("live");
  const [instrument, setInstrument] = React.useState<ContestInstrument>("cfd");
  const [startsAt, setStartsAt] = React.useState("");
  const [endsAt, setEndsAt] = React.useState("");
  const [scoring, setScoring] = React.useState<Scoring>("return_pct");
  const [minTrades, setMinTrades] = React.useState("5");
  const [minPremium, setMinPremium] = React.useState("");
  const [maxEntrants, setMaxEntrants] = React.useState("");
  const [startingBalance, setStartingBalance] = React.useState("10000");
  const [demoGroup, setDemoGroup] = React.useState("");
  const [groups, setGroups] = React.useState<string[]>([]);
  const [assetClasses, setAssetClasses] = React.useState<string[]>([]);
  const [minEquity, setMinEquity] = React.useState("");
  const [kyc, setKyc] = React.useState(true);
  const [pool, setPool] = React.useState("1000");
  const [split, setSplit] = React.useState("top3");
  const [payout, setPayout] = React.useState<"wallet" | "credit">("wallet");
  const [manual, setManual] = React.useState(false);
  const [prizes, setPrizes] = React.useState<PrizeRow[]>([]);
  const [rules, setRules] = React.useState("");
  const [anti, setAnti] = React.useState({ minHold: "60", maxSingle: "80", dq: true });
  const [status, setStatus] = React.useState<"draft" | "scheduled">("scheduled");

  // the drawer re-reads the contest every 15 s: fill the form once per opening, never while editing
  const initialRef = React.useRef(initial);
  initialRef.current = initial;
  const initialId = initial?.id;
  React.useEffect(() => {
    if (!open) return;
    const c = initialRef.current;
    setError(null);
    setBusy(false);
    setMore(!!c);
    setStep(c ? 1 : 0);
    setTemplateId(c?.templateId ?? null);
    setName(c?.name ?? "");
    setDescription(c?.description ?? "");
    setKind(c?.kind ?? "live");
    setInstrument(c?.instrument ?? "cfd");
    const w = nextWindow("next_monday", 7);
    setStartsAt(toLocalInput(c?.startsAt ?? w.start.toISOString()));
    setEndsAt(toLocalInput(c?.endsAt ?? w.end.toISOString()));
    setScoring(c?.scoring ?? "return_pct");
    setMinTrades(c ? String(c.minTrades) : "5");
    setMinPremium(numText(c?.minPremium));
    setMaxEntrants(numText(c?.maxEntrants));
    setStartingBalance(c ? numText(c.startingBalance) || "10000" : "10000");
    setDemoGroup(c?.demoGroup ?? "");
    setGroups(c?.accountGroups ?? []);
    setAssetClasses(c?.assetClasses ?? []);
    setMinEquity(numText(c?.minEquity));
    setKyc(c?.kycRequired ?? true);
    // an existing contest keeps its own prize table (manual rows); new ones use the calculator
    setManual(!!c);
    setPrizes(c ? c.prizes.map((p) => ({ rankFrom: String(p.rankFrom), rankTo: String(p.rankTo), amount: String(p.amount), payout: p.payout })) : []);
    setPool(c ? String(c.prizePool || 0) : "1000");
    setSplit("top3");
    setPayout(c?.prizes[0]?.payout ?? "wallet");
    setRules(c?.rules ?? "");
    const a = c?.antiCheat ?? DEFAULT_ANTI;
    setAnti({ minHold: String(a.minHoldSeconds), maxSingle: String(a.maxSingleTradePct), dq: a.disqualifyOnBalanceChange });
    setStatus(c?.status === "draft" ? "draft" : "scheduled");
  }, [open, initialId]);

  const pick = (t: ContestTemplate | null) => {
    if (t) {
      setTemplateId(t.id);
      setName(t.name);
      setDescription(t.description);
      setKind(t.kind);
      setInstrument(t.instrument);
      setScoring(t.scoring);
      setMinTrades(String(t.minTrades));
      setMaxEntrants(numText(t.maxEntrants));
      setGroups(t.accountGroups);
      setAssetClasses(t.assetClasses);
      setKyc(t.kycRequired);
      setMinEquity(numText(t.minEquity));
      setPool(String(t.prizePool));
      setPayout(t.payout);
      if (Array.isArray(t.split)) {
        setManual(true);
        setPrizes(splitPool(t.prizePool, t.split, t.payout).map((p) => ({ rankFrom: String(p.rankFrom), rankTo: String(p.rankTo), amount: String(p.amount), payout: p.payout })));
      } else {
        setManual(false);
        setSplit(t.split);
      }
      setRules(t.rules);
      setAnti({ minHold: String(t.antiCheat.minHoldSeconds), maxSingle: String(t.antiCheat.maxSingleTradePct), dq: t.antiCheat.disqualifyOnBalanceChange });
      const w = nextWindow(t.schedule, t.durationDays);
      setStartsAt(toLocalInput(w.start.toISOString()));
      setEndsAt(toLocalInput(w.end.toISOString()));
      setMore(false);
    } else {
      setTemplateId(null);
      setName("");
      setDescription("");
      setMore(true);
    }
    setStep(1);
  };

  const options = instrument === "options";
  const demo = kind === "demo";
  const parsedManual: Prize[] = prizes.map((p) => ({ rankFrom: Math.round(Number(p.rankFrom)), rankTo: Math.round(Number(p.rankTo)), amount: Number(p.amount), payout: demo ? "wallet" : p.payout }));
  const finalPrizes: Prize[] = manual ? parsedManual : splitPool(Number(pool) || 0, bandsOf(split, presets), demo ? "wallet" : payout);
  const total = prizeTotal(finalPrizes.filter((p) => p.rankTo >= p.rankFrom && p.amount > 0));
  const upd = (i: number, patch: Partial<PrizeRow>) => setPrizes((x) => x.map((p, j) => (j === i ? { ...p, ...patch } : p)));
  const antiCheat: AntiCheat = { minHoldSeconds: Math.max(0, Math.round(Number(anti.minHold) || 0)), maxSingleTradePct: Number(anti.maxSingle) || 0, disqualifyOnBalanceChange: anti.dq };
  const pickInstrument = (v: ContestInstrument) => {
    setInstrument(v);
    if (v === "options" && scoring === "lots") setScoring("contracts");
    if (v === "cfd" && scoring === "contracts") setScoring("lots");
    if (v === "options") setAssetClasses([]);
  };

  /** Step 2 check; returns an error text or null. */
  const validate = (): string | null => {
    if (!name.trim()) return "Give the contest a name.";
    if (locked) return null;
    const s = fromLocalInput(startsAt);
    const e = fromLocalInput(endsAt);
    if (!s || !e) return "Set when the contest starts and ends.";
    if (e <= s) return "The end must be after the start.";
    if (demo && !(Number(startingBalance) > 0)) return "Set the demo starting balance.";
    if (options && minPremium.trim() !== "" && !(Number(minPremium) >= 0)) return "The minimum premium must be 0 or more.";
    if (manual) {
      let last = 0;
      for (const p of parsedManual) {
        if (!(p.rankFrom >= 1) || !(p.rankTo >= p.rankFrom)) return "Each prize row needs Rank from ≤ Rank to.";
        if (p.rankFrom <= last) return "Prize ranks must not overlap and must go down the table in order.";
        if (!(p.amount > 0)) return "Each prize needs an amount above 0.";
        last = p.rankTo;
      }
    } else {
      if (!(Number(pool) >= 0)) return "The prize pool must be 0 or more.";
      const be = bandsError(bandsOf(split, presets) as Band[]);
      if (be) return be;
    }
    return null;
  };

  const body = (): ContestInput | Partial<ContestInput> => {
    if (locked) return { name: name.trim(), description: description.trim(), rules: rules.trim(), maxEntrants: numOrNull(maxEntrants), kycRequired: demo ? false : kyc, antiCheat };
    return {
      name: name.trim(),
      description: description.trim(),
      kind,
      instrument,
      startsAt: fromLocalInput(startsAt)!,
      endsAt: fromLocalInput(endsAt)!,
      scoring,
      minTrades: Math.max(0, Math.round(Number(minTrades) || 0)),
      maxEntrants: numOrNull(maxEntrants),
      startingBalance: demo ? Number(startingBalance) : null,
      demoGroup: demo ? demoGroup.trim() || null : null,
      accountGroups: demo ? [] : groups,
      assetClasses: options ? [] : assetClasses,
      kycRequired: demo ? false : kyc,
      minEquity: demo ? null : numOrNull(minEquity),
      prizes: finalPrizes,
      rules: rules.trim(),
      antiCheat,
      minPremium: options ? numOrNull(minPremium) : null,
      status,
      templateId,
    };
  };

  const next = () => {
    const e = validate();
    setError(e);
    if (!e) setStep(2);
  };

  const publish = async () => {
    const e = validate();
    if (e) {
      setError(e);
      setStep(1);
      return;
    }
    setBusy(true);
    setError(null);
    const r = editing ? await mkSend<{ contest: Contest }>(`contests/${initial!.id}`, body(), "PATCH") : await mkSend<{ contest: Contest }>("contests", body());
    setBusy(false);
    if (!r.ok) {
      const msg = errText(r.error);
      setError(msg);
      toast.error("Not saved", { description: msg });
      return;
    }
    toast.success(editing ? `${r.data.contest?.name ?? name} saved` : `${r.data.contest?.name ?? name} ${status === "draft" ? "saved as draft" : "scheduled"}`, { description: "Recorded in the marketing audit log" });
    onSaved(r.data.contest);
    onOpenChange(false);
  };

  const steps = ["Template", "Dates and prizes", "Review and publish"];
  const s = fromLocalInput(startsAt);
  const e = fromLocalInput(endsAt);
  const days = s && e ? Math.max(0, (Date.parse(e) - Date.parse(s)) / 86_400_000) : 0;
  const above = finalPrizes.filter((p) => autoPay > 0 && p.amount >= autoPay).length;

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => !busy && onOpenChange(o)}
      width={860}
      title={editing ? `Edit ${initial!.name}` : "New contest"}
      description={locked ? "The contest has started: timing, instrument, scoring, eligibility and prizes are locked." : step === 0 ? "Pick a template; you can change everything on the next step." : undefined}
      footer={
        <>
          <span className="mr-auto hidden text-[12px] text-fg-3 sm:block">
            <span className="k-num">Pool {usd(total, 0)}</span>
            {templateId && tpl.data ? ` · from ${tpl.data.items.find((t) => t.id === templateId)?.name ?? "a template"}` : ""}
          </span>
          {step > (editing ? 1 : 0) && (
            <Button variant="ghost" size="sm" disabled={busy} onClick={() => (setError(null), setStep(step - 1))}>
              <ChevronLeft /> Back
            </Button>
          )}
          {step === 1 && (
            <Button variant="ember" size="sm" onClick={next} data-testid="wizard-next">
              Review <ChevronRight />
            </Button>
          )}
          {step === 2 && (
            <Button variant="ember" size="sm" disabled={busy} onClick={publish} data-testid="contest-form-submit">
              {busy ? "Saving…" : editing ? "Save changes" : status === "draft" ? "Save draft" : "Publish contest"}
            </Button>
          )}
        </>
      }
    >
      <div className="space-y-5">
        <Stepper steps={editing ? steps.slice(1) : steps} current={editing ? step - 1 : step} />

        {step === 0 && <TemplatePicker data={tpl.data} error={!!tpl.error} onPick={pick} />}

        {step === 1 && (
          <div className="space-y-5">
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <TextF label="Name" value={name} onChange={setName} placeholder="e.g. October FX Masters" maxLength={80} className="sm:col-span-2" />
              <fieldset disabled={locked} className={cn("contents", locked && "opacity-60")}>
                <DateTimeF label="Starts" value={startsAt} onChange={setStartsAt} hint="Your local time" />
                <DateTimeF label="Ends" value={endsAt} onChange={setEndsAt} hint={days ? `${+days.toFixed(1)} days` : undefined} />
              </fieldset>
            </div>

            <div className="space-y-2">
              <div className="k-label flex items-center gap-1.5">
                <Trophy className="size-3.5" /> Prizes
              </div>
              {manual ? (
                <div className={cn("space-y-2", locked && "pointer-events-none opacity-60")}>
                  {prizes.map((p, i) => (
                    <div key={i} className="grid grid-cols-[1fr_1fr_1.3fr_1.2fr_auto] items-end gap-2">
                      <NumF label={`Rank from (${i + 1})`} value={p.rankFrom} onChange={(v) => upd(i, { rankFrom: v })} step={1} />
                      <NumF label={`Rank to (${i + 1})`} value={p.rankTo} onChange={(v) => upd(i, { rankTo: v })} step={1} />
                      <NumF label={`Amount (${i + 1})`} value={p.amount} onChange={(v) => upd(i, { amount: v })} prefix="$" />
                      {demo ? (
                        <div className="flex h-10 items-center text-[12.5px] text-fg-3">Wallet</div>
                      ) : (
                        <SelectF label={`Payout (${i + 1})`} value={p.payout} onChange={(v) => upd(i, { payout: v })} options={[{ value: "wallet", label: "Wallet" }, { value: "credit", label: "Credit" }]} />
                      )}
                      <button type="button" onClick={() => setPrizes((x) => x.filter((_, j) => j !== i))} className="grid size-10 place-items-center rounded-[12px] border border-line text-fg-3 hover:text-down" aria-label={`Remove prize row ${i + 1}`}>
                        <Trash2 className="size-4" />
                      </button>
                    </div>
                  ))}
                  <div className="flex flex-wrap gap-2">
                    <Button
                      size="sm"
                      variant="surface"
                      onClick={() => {
                        const last = prizes[prizes.length - 1];
                        const from = last ? Number(last.rankTo) + 1 : 1;
                        setPrizes((x) => [...x, { rankFrom: String(from), rankTo: String(from), amount: "50", payout }]);
                      }}
                    >
                      <Plus /> Add prize row
                    </Button>
                    <Button size="sm" variant="ghost" onClick={() => setManual(false)}>
                      <Wand2 /> Use the calculator
                    </Button>
                  </div>
                  <div className="k-row flex items-center justify-between px-4 py-2.5">
                    <span className="text-[12.5px] text-fg-3">Prize pool</span>
                    <span className="k-num text-[15px] font-semibold text-gold">{usd(total, 0)}</span>
                  </div>
                </div>
              ) : (
                <>
                  <PrizeCalculator pool={pool} setPool={setPool} split={split} setSplit={setSplit} payout={payout} setPayout={setPayout} presets={presets} demo={demo} disabled={locked} />
                  {!locked && (
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => {
                        setPrizes(finalPrizes.map((p) => ({ rankFrom: String(p.rankFrom), rankTo: String(p.rankTo), amount: String(p.amount), payout: p.payout })));
                        setManual(true);
                      }}
                    >
                      Edit prize rows by hand
                    </Button>
                  )}
                </>
              )}
            </div>

            {!demo && (
              <div className="space-y-2">
                <div className="k-label">Who can join</div>
                <GroupPicker value={groups} onChange={setGroups} open={open} disabled={locked} />
              </div>
            )}

            <button type="button" onClick={() => setMore((m) => !m)} className="flex w-full items-center justify-between rounded-[12px] border border-line bg-surface-2 px-4 py-2.5 text-[13px] font-medium" aria-expanded={more} data-testid="more-options">
              More options
              <ChevronDown className={cn("size-4 transition-transform", more && "rotate-180")} />
            </button>

            {more && (
              <div className="space-y-4" data-testid="more-options-panel">
                <AreaF label="Description" value={description} onChange={setDescription} rows={2} />
                <fieldset disabled={locked} className={cn("grid min-w-0 grid-cols-1 gap-3 sm:grid-cols-2", locked && "opacity-60")}>
                  <SelectF label="Account type" value={kind} onChange={setKind} options={[{ value: "live", label: "Live (client's own account)" }, { value: "demo", label: "Demo (new account per entrant)" }]} />
                  <SelectF
                    label="Instrument"
                    value={instrument}
                    onChange={pickInstrument}
                    options={[
                      { value: "cfd", label: "CFD (lots)" },
                      { value: "options", label: "Options (Kalks FX Options, contracts)" },
                    ]}
                    hint={options ? "Only option trades count, on realised P&L" : "CFD trades only; options never count"}
                  />
                  <SelectF label="Scoring" value={scoring} onChange={setScoring} options={scoringOptions(instrument)} className="sm:col-span-2" />
                  <NumF label="Min trades" value={minTrades} onChange={setMinTrades} step={1} hint="to rank" />
                  <NumF label="Max entrants" value={maxEntrants} onChange={setMaxEntrants} step={1} placeholder="Unlimited" />
                  {options && (
                    <NumF
                      label="Minimum premium per trade"
                      value={minPremium}
                      onChange={setMinPremium}
                      prefix="$"
                      placeholder="None"
                      hint="Trades opened for less add no contracts and don't count as a trade (their P&L still counts)"
                      className="sm:col-span-2"
                    />
                  )}
                  {demo ? (
                    <>
                      <NumF label="Starting balance" value={startingBalance} onChange={setStartingBalance} prefix="$" />
                      <TextF label="Demo group" value={demoGroup} onChange={setDemoGroup} placeholder="Default demo group" mono hint={options ? "Must have options enabled" : undefined} />
                    </>
                  ) : (
                    <NumF label="Minimum equity" value={minEquity} onChange={setMinEquity} prefix="$" placeholder="None" />
                  )}
                </fieldset>
                {!options && !demo && (
                  <div className={cn(locked && "pointer-events-none opacity-60")}>
                    <PillPicker label="Asset classes that count (none = all CFDs)" options={Object.entries(ASSET_LABEL).map(([value, label]) => ({ value, label }))} value={assetClasses} onChange={setAssetClasses} />
                  </div>
                )}
                {!demo && (
                  <div className="k-row px-4">
                    <ToggleRow label="Require verified KYC" hint="Only verified clients can join (recommended for live contests with cash prizes)" checked={kyc} onChange={setKyc} />
                  </div>
                )}
                <AreaF label="Rules" value={rules} onChange={setRules} rows={4} placeholder="Shown to clients on the contest page" />
                <div className="grid grid-cols-2 gap-3">
                  <NumF label="Minimum hold" value={anti.minHold} onChange={(v) => setAnti((a) => ({ ...a, minHold: v }))} suffix="seconds" step={1} />
                  <NumF label="Max single trade" value={anti.maxSingle} onChange={(v) => setAnti((a) => ({ ...a, maxSingle: v }))} suffix="% of profit" />
                </div>
                <div className="k-row px-4">
                  <ToggleRow label="Disqualify on balance change" hint="Deposits, withdrawals, transfers or staff adjustments during the contest" checked={anti.dq} onChange={(v) => setAnti((a) => ({ ...a, dq: v }))} />
                </div>
              </div>
            )}
          </div>
        )}

        {step === 2 && (
          <div className="grid grid-cols-1 gap-4 md:grid-cols-2" data-testid="wizard-review">
            <div className="k-row px-4 py-2">
              <Review k="Name" v={<span className="font-medium">{name}</span>} />
              <Review k="Window" v={<span className="k-num">{s && e ? `${new Date(s).toLocaleString()} – ${new Date(e).toLocaleString()}` : "—"}</span>} />
              <Review k="Accounts" v={demo ? "Demo (new account per entrant)" : `Live${groups.length ? ` · ${groups.join(", ")}` : " · all groups"}`} />
              <Review k="Ranked by" v={`${SCORING_LABEL[scoring]}${Number(minTrades) ? ` · min ${int(Number(minTrades))} trades` : ""}`} />
              {!demo && <Review k="Counts" v={assetClasses.length ? assetClasses.map((a) => ASSET_LABEL[a] ?? a).join(", ") : options ? "Kalks FX Options" : "Every CFD"} />}
              <Review k="Entry" v={`Free${!demo && kyc ? " · verified KYC" : ""}${maxEntrants ? ` · max ${maxEntrants}` : ""}${!demo && minEquity ? ` · min equity ${usd(Number(minEquity), 0)}` : ""}`} />
              <Review k="Anti-cheat" v={`hold ≥ ${antiCheat.minHoldSeconds}s · single trade ≤ ${antiCheat.maxSingleTradePct}%${antiCheat.disqualifyOnBalanceChange ? " · balance changes disqualify" : ""}`} />
            </div>
            <div className="space-y-3">
              <div className="k-row divide-y divide-line px-4">
                {finalPrizes.length === 0 ? (
                  <div className="py-3 text-[12.5px] text-fg-3">No prizes: ranking only.</div>
                ) : (
                  finalPrizes.map((p) => (
                    <div key={`${p.rankFrom}-${p.rankTo}`} className="flex items-center justify-between py-2 text-[13px]">
                      <span className="text-fg-2">{p.rankFrom === p.rankTo ? `Rank ${p.rankFrom}` : `Ranks ${p.rankFrom}–${p.rankTo}`}</span>
                      <span className="flex items-center gap-2">
                        <span className="text-[11px] text-fg-3">{p.payout === "credit" ? "credit" : "wallet"}</span>
                        <span className="k-num font-semibold">{usd(p.amount, p.amount % 1 ? 2 : 0)}</span>
                      </span>
                    </div>
                  ))
                )}
                <div className="flex items-center justify-between py-2.5 text-[13px]">
                  <span className="text-fg-3">Prize pool</span>
                  <span className="k-num text-[15px] font-semibold text-gold">{usd(total, 0)}</span>
                </div>
              </div>
              <div className="rounded-[12px] border border-line bg-surface-2/60 px-3.5 py-2.5 text-[12px] leading-relaxed text-fg-3">
                {autoPay > 0
                  ? `After the results are final, prizes under ${usd(autoPay, 0)} are paid automatically${above ? `; ${above} prize row(s) at or above it wait for approval (Pay prizes, marketing.approve)` : ""}.`
                  : "Prizes are paid when a staff member with marketing.approve presses Pay prizes after finalizing. Set an auto-pay threshold under Settings to pay small prizes automatically."}
              </div>
              {!locked && <SelectF label="Publish as" value={status} onChange={setStatus} options={[{ value: "scheduled", label: "Scheduled (visible, opens at start)" }, { value: "draft", label: "Draft (hidden)" }]} />}
            </div>
          </div>
        )}
        <ErrorBanner error={error} />
        {step === 0 && (
          <div className="flex items-center gap-2 text-[12px] text-fg-3">
            <LayoutTemplate className="size-3.5" /> Templates are editable under the Templates tab.
          </div>
        )}
      </div>
    </Dialog>
  );
}
