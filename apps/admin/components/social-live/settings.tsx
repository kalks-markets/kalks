"use client";

import * as React from "react";
import { RotateCcw, Save } from "lucide-react";
import { Button, Card, CardHeader, Chip, PageHeader, Reveal, cn } from "@kalks/ui";
import { MiniField, NumInput, SettingRow, Slider } from "@/components/config/kit";
import { useApi } from "@/components/live/kit";
import { ReadOnlyNote, SocialError, socialWrite, useNoteAction, useSocialCan, usd, type SocialSettings } from "./kit";

/** Engine limit for the maximum performance fee (services/trading api/social_admin.rs save_settings). */
const FEE_MAX_PCT = 90;

const KEYS: (keyof SocialSettings)[] = ["feeMinPct", "feeMaxPct", "platformCutPct", "minTrackDays", "minOwnCapitalPct", "minMasterEquity", "minAllocation", "tradeDelayMinutes"];

const LABEL: Record<keyof SocialSettings, string> = {
  feeMinPct: "Minimum performance fee",
  feeMaxPct: "Maximum performance fee",
  platformCutPct: "Platform cut",
  minTrackDays: "Track record",
  minOwnCapitalPct: "Master's own capital in a fund",
  minMasterEquity: "Minimum master equity",
  minAllocation: "Minimum copy allocation",
  tradeDelayMinutes: "Trade history delay",
};

function validate(s: SocialSettings): string | null {
  for (const k of KEYS) if (!Number.isFinite(s[k]) || s[k] < 0) return `${LABEL[k]} must be zero or more.`;
  if (s.feeMaxPct > FEE_MAX_PCT) return `The maximum performance fee can't exceed ${FEE_MAX_PCT}%.`;
  if (s.feeMinPct > s.feeMaxPct) return "The minimum performance fee is above the maximum.";
  if (s.platformCutPct > 100) return "The platform cut can't exceed 100%.";
  if (s.minOwnCapitalPct > 100) return "The master's own capital can't exceed 100%.";
  if (!Number.isInteger(s.minTrackDays)) return "Track record is a whole number of days.";
  if (!Number.isInteger(s.tradeDelayMinutes) || s.tradeDelayMinutes > 1440) return "Trade history delay is 0–1440 whole minutes.";
  return null;
}

export function LiveSocialSettingsPage() {
  const canWrite = useSocialCan("social.write");
  const { data, error, reload } = useApi<{ settings?: SocialSettings } & Partial<SocialSettings>>("/api/social/admin/settings");
  const loaded = React.useMemo<SocialSettings | null>(() => {
    if (!data) return null;
    const src = (data.settings ?? data) as Partial<SocialSettings>;
    return KEYS.reduce((o, k) => ({ ...o, [k]: Number(src[k] ?? 0) }), {} as SocialSettings);
  }, [data]);
  const [s, setS] = React.useState<SocialSettings | null>(null);
  React.useEffect(() => {
    if (loaded) setS(loaded);
  }, [loaded]);
  const act = useNoteAction();

  if (error && !data) {
    return (
      <div className="pb-16">
        <PageHeader title="Fee caps & rules" subtitle="Global limits for copy-trading and PAMM performance fees and master eligibility" />
        <SocialError error={error} onRetry={reload} />
      </div>
    );
  }
  if (!s || !loaded) {
    return (
      <div className="pb-16">
        <PageHeader title="Fee caps & rules" subtitle="Global limits for copy-trading and PAMM performance fees and master eligibility" />
        <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
          <div className="h-[380px] animate-pulse rounded-[22px] bg-surface xl:col-span-7" />
          <div className="h-[380px] animate-pulse rounded-[22px] bg-surface xl:col-span-5" />
        </div>
      </div>
    );
  }

  const up = (p: Partial<SocialSettings>) => setS((x) => (x ? { ...x, ...p } : x));
  const changed = KEYS.filter((k) => s[k] !== loaded[k]);
  const dirty = changed.length > 0;
  const invalid = validate(s);
  const exampleFee = 20;
  const exampleProfit = 1000;
  const fee = (exampleProfit * exampleFee) / 100;

  const save = () =>
    act.ask({
      title: "Publish social trading rules",
      description: "Fee caps apply to new applications and fee changes; fees already locked on running subscriptions are not changed.",
      confirmLabel: "Publish",
      body: (
        <div className="space-y-1">
          {changed.map((k) => (
            <div key={k} className="k-row flex items-center justify-between px-3 py-2 text-[12.5px]">
              <span className="text-fg-2">{LABEL[k]}</span>
              <span className="k-num font-mono">
                <span className="text-fg-3">{loaded[k]}</span> → <span className="text-fg">{s[k]}</span>
              </span>
            </div>
          ))}
        </div>
      ),
      disabled: invalid ?? false,
      run: (note) => socialWrite("admin/settings", { ...s, note }, "PUT"),
      success: "Social trading rules published",
      onDone: reload,
    });

  return (
    <div className="pb-16">
      <PageHeader
        title="Fee caps & rules"
        subtitle="Global limits for copy-trading and PAMM performance fees and master eligibility"
        actions={
          canWrite ? (
            <>
              {dirty && (
                <Chip tone={invalid ? "down" : "warn"} dot>
                  {invalid ? "Fix errors" : `${changed.length} unsaved change${changed.length > 1 ? "s" : ""}`}
                </Chip>
              )}
              <Button variant="ghost" disabled={!dirty} onClick={() => setS(loaded)}>
                <RotateCcw /> Discard
              </Button>
              <Button variant="ember" disabled={!dirty || !!invalid} onClick={save}>
                <Save /> Save changes
              </Button>
            </>
          ) : (
            <ReadOnlyNote what="change these rules" />
          )
        }
      />

      {invalid && dirty && <div className="mb-4 rounded-[14px] border border-down/30 bg-down-soft px-4 py-2.5 text-[12.5px] text-fg">{invalid}</div>}

      <fieldset disabled={!canWrite} className="contents">
        <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
          <Reveal className="xl:col-span-7">
            <Card className="h-full">
              <CardHeader title="Performance fee caps" subtitle="The range a master may charge followers and PAMM investors (high-water mark, per settlement period)" />
              <div className="space-y-6 px-4 pb-6 pt-5 sm:px-6">
                <div>
                  <div className="mb-2 flex items-center justify-between text-[13px]">
                    <span className="font-medium">Allowed range</span>
                    <span className="k-num text-fg-2">
                      {s.feeMinPct}% – {s.feeMaxPct}%
                    </span>
                  </div>
                  <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                    <MiniField label="Minimum">
                      <NumInput value={s.feeMinPct} onChange={(v) => up({ feeMinPct: v })} min={0} max={FEE_MAX_PCT} step={1} suffix="%" />
                    </MiniField>
                    <MiniField label="Maximum">
                      <NumInput value={s.feeMaxPct} onChange={(v) => up({ feeMaxPct: v })} min={0} max={FEE_MAX_PCT} step={1} suffix="%" />
                    </MiniField>
                  </div>
                </div>
                <div className="divide-y divide-line border-t border-line">
                  <SettingRow label="Master track record" hint="Minimum age of the live account a client applies with">
                    <NumInput size="sm" className="w-32" value={s.minTrackDays} onChange={(v) => up({ minTrackDays: Math.round(v) })} min={0} step={1} suffix="days" />
                  </SettingRow>
                  <SettingRow label="Minimum master equity" hint="Equity of the applying account">
                    <NumInput size="sm" className="w-36" value={s.minMasterEquity} onChange={(v) => up({ minMasterEquity: v })} min={0} step={100} prefix="$" />
                  </SettingRow>
                  <SettingRow label="Master's own capital in a PAMM fund" hint="The master can't redeem, and investments are refused, below this share of units">
                    <NumInput size="sm" className="w-28" value={s.minOwnCapitalPct} onChange={(v) => up({ minOwnCapitalPct: v })} min={0} max={100} step={0.5} suffix="%" />
                  </SettingRow>
                  <SettingRow label="Minimum copy allocation" hint="Smallest amount a follower can allocate (a master may set a higher one)">
                    <NumInput size="sm" className="w-36" value={s.minAllocation} onChange={(v) => up({ minAllocation: v })} min={0} step={10} prefix="$" />
                  </SettingRow>
                  <SettingRow label="Public trade history delay" hint="Closed trades on a master profile are shown after this delay">
                    <NumInput size="sm" className="w-32" value={s.tradeDelayMinutes} onChange={(v) => up({ tradeDelayMinutes: Math.round(v) })} min={0} max={1440} step={5} suffix="min" />
                  </SettingRow>
                </div>
              </div>
            </Card>
          </Reveal>

          <Reveal delay={0.05} className="xl:col-span-5">
            <Card className="h-full overflow-hidden">
              <CardHeader title="Platform cut" subtitle="Broker share of every performance fee, kept when the fee is paid to the master" />
              <div className="px-4 pb-6 pt-4 sm:px-6">
                <div className="k-num text-[44px] font-semibold leading-none tracking-tight">{s.platformCutPct}%</div>
                <Slider className="mt-4" value={s.platformCutPct} onChange={(v) => up({ platformCutPct: v })} max={50} marks={[0, 10, 20, 30, 40, 50]} />
                <div className="mt-6 text-[11.5px] uppercase tracking-wider text-fg-3">Worked example · {exampleFee}% performance fee</div>
                <div className="mt-2 space-y-1.5">
                  {(
                    [
                      ["Follower profit above high-water mark", exampleProfit],
                      ["Performance fee charged", fee],
                      ["Master receives", fee * (1 - s.platformCutPct / 100)],
                      ["Platform keeps", fee * (s.platformCutPct / 100)],
                    ] as const
                  ).map(([k, v], i) => (
                    <div key={k} className={cn("flex items-center justify-between rounded-[12px] px-3.5 py-2.5 text-[13px]", i === 3 ? "border border-ember/40 bg-ember-soft" : "k-row")}>
                      <span className="text-fg-2">{k}</span>
                      <span className={cn("k-num font-medium", i === 3 && "text-ember")}>{usd(v)}</span>
                    </div>
                  ))}
                </div>
                <div className="mt-4 flex h-2 overflow-hidden rounded-full">
                  <span className="bg-gold" style={{ width: `${100 - s.platformCutPct}%` }} />
                  <span className="bg-ember" style={{ width: `${s.platformCutPct}%` }} />
                </div>
                <div className="mt-2 flex justify-between text-[11.5px] text-fg-3">
                  <span>Master {100 - s.platformCutPct}%</span>
                  <span>Platform {s.platformCutPct}%</span>
                </div>
              </div>
            </Card>
          </Reveal>
        </div>
      </fieldset>
      {act.node}
    </div>
  );
}
