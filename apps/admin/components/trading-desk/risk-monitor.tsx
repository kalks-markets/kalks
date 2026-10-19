"use client";

import * as React from "react";
import Link from "next/link";
import { BellRing, Check, SlidersHorizontal, Volume2, VolumeX } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, CardHeader, Chip, IconButton, Segmented, Tooltip, cn } from "@kalks/ui";
import { IS_DEMO } from "@kalks/mock/mode";
import { ago, useNow } from "@/components/live/kit";
import { useCan } from "@/components/staff-session";
import { RiskError } from "@/components/trading-live/risk-kit";
import { ALERT_KIND_LABEL, REJECT_LABEL, ackAlert, onNewRedAlert, useRiskAlerts, type RiskAlert } from "@/lib/trading-desk";

const SOUND_KEY = "kalks.risk.sound";

function readSound() {
  try {
    return window.localStorage.getItem(SOUND_KEY) === "1";
  } catch {
    return false;
  }
}
function writeSound(on: boolean) {
  try {
    window.localStorage.setItem(SOUND_KEY, on ? "1" : "0");
  } catch {
    /* storage blocked: the toggle still works for this page */
  }
}

let audio: AudioContext | null = null;
/** Short two-tone beep (WebAudio, no asset). */
function beep() {
  try {
    const Ctx = window.AudioContext ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
    if (!Ctx) return;
    audio ??= new Ctx();
    if (audio.state === "suspended") void audio.resume();
    const t = audio.currentTime;
    for (const [i, f] of [880, 660].entries()) {
      const o = audio.createOscillator();
      const g = audio.createGain();
      o.type = "sine";
      o.frequency.value = f;
      g.gain.setValueAtTime(0.0001, t + i * 0.16);
      g.gain.exponentialRampToValueAtTime(0.18, t + i * 0.16 + 0.02);
      g.gain.exponentialRampToValueAtTime(0.0001, t + i * 0.16 + 0.14);
      o.connect(g).connect(audio.destination);
      o.start(t + i * 0.16);
      o.stop(t + i * 0.16 + 0.15);
    }
  } catch {
    /* audio unavailable */
  }
}

const LEVEL_ROW: Record<RiskAlert["level"], string> = {
  red: "border-down/40 bg-down-soft",
  amber: "border-warn/40 bg-warn-soft",
  info: "",
};
const LEVEL_DOT: Record<RiskAlert["level"], string> = { red: "bg-down", amber: "bg-warn", info: "bg-fg-3" };

/** Extra detail of an alert (reject code, usage). */
function detailOf(a: RiskAlert) {
  const d = a.data ?? {};
  const out: string[] = [];
  if (typeof d.code === "string") out.push(REJECT_LABEL[d.code] ?? d.code);
  if (typeof d.usedPct === "number") out.push(`${Math.round(d.usedPct)}% of cap`);
  return out;
}

/**
 * Live risk monitor (dealer desk, Command Center › Live risk): open alerts, red first then newest, with
 * acknowledge, count chips and an optional sound on a new red alert. Fed by GET dealing/risk/alerts and the
 * `risk_alert` stream frames.
 */
export function RiskMonitor({ className, max = 8 }: { className?: string; max?: number }) {
  const { alerts, loaded, error, reload } = useRiskAlerts();
  const canAck = useCan("dealing.write");
  const now = useNow(15_000);
  const [sound, setSound] = React.useState(false);
  const [show, setShow] = React.useState<"open" | "all">("open");
  const [busy, setBusy] = React.useState<string | null>(null);
  const [more, setMore] = React.useState(false);
  React.useEffect(() => setSound(readSound()), []);
  const soundRef = React.useRef(sound);
  soundRef.current = sound;
  React.useEffect(
    () =>
      onNewRedAlert((a) => {
        if (soundRef.current) beep();
        toast.error(a.title, { description: a.message });
      }),
    [],
  );

  const open = alerts.filter((a) => a.status === "open");
  const red = open.filter((a) => a.level === "red").length;
  const amber = open.filter((a) => a.level === "amber").length;
  const list = show === "open" ? open : alerts;
  const visible = more ? list : list.slice(0, max);

  const ack = async (a: RiskAlert) => {
    setBusy(a.id);
    const r = await ackAlert(a.id);
    setBusy(null);
    if (r.ok) toast.success("Alert acknowledged", { description: a.title });
    else toast.error("Not acknowledged", { description: r.error.message });
  };

  return (
    <Card className={cn("flex flex-col", className)}>
      <CardHeader
        title="Risk monitor"
        subtitle="Exposure, daily loss, news windows, limit rejects and toxic clients"
        icon={<BellRing />}
        action={
          <>
            <Chip size="sm" tone={red ? "down" : "neutral"} dot>
              <span className="k-num">{red}</span> red
            </Chip>
            <Chip size="sm" tone={amber ? "warn" : "neutral"} dot>
              <span className="k-num">{amber}</span> amber
            </Chip>
            <Tooltip content={sound ? "Sound on for new red alerts" : "Sound off"}>
              <IconButton
                size="sm"
                variant="ghost"
                aria-label={sound ? "Turn alert sound off" : "Turn alert sound on"}
                aria-pressed={sound}
                onClick={() => {
                  const v = !sound;
                  setSound(v);
                  writeSound(v);
                  if (v) beep();
                  toast(v ? "Alert sound on" : "Alert sound off", { description: v ? "A short beep plays on every new red alert" : undefined });
                }}
              >
                {sound ? <Volume2 /> : <VolumeX />}
              </IconButton>
            </Tooltip>
          </>
        }
      />
      <div className="flex flex-wrap items-center justify-between gap-2 px-4 pt-4 sm:px-6">
        <Segmented size="xs" value={show} onChange={setShow} options={[{ value: "open", label: `Open ${open.length}` }, { value: "all", label: `Incl. acknowledged ${alerts.length}` }]} />
        <Link href="/trading/risk-limits" className="inline-flex items-center gap-1.5 text-[12px] text-fg-2 hover:text-ember">
          <SlidersHorizontal className="size-3.5" /> Risk limits
        </Link>
      </div>
      <div className="flex-1 space-y-1.5 px-4 pb-5 pt-3 sm:px-6">
        {IS_DEMO ? (
          <div className="rounded-[14px] border border-line bg-surface-2/60 px-3.5 py-3 text-[12.5px] text-fg-3">Risk alerts come from the trading engine; none in this demo build.</div>
        ) : error ? (
          <RiskError error={error} compact onRetry={reload} />
        ) : !loaded ? (
          <div className="text-[12.5px] text-fg-3">Loading alerts…</div>
        ) : !list.length ? (
          <div className="flex items-center gap-2 rounded-[14px] border border-line bg-surface-2/60 px-3.5 py-3 text-[12.5px] text-fg-2">
            <Check className="size-3.5 text-up" /> No open risk alerts.
          </div>
        ) : (
          visible.map((a) => (
            <div key={a.id} className={cn("k-row flex items-start gap-3 px-3.5 py-2.5", a.status === "open" ? LEVEL_ROW[a.level] : "opacity-70")}>
              <span className={cn("mt-1.5 size-2 shrink-0 rounded-full", LEVEL_DOT[a.level], a.status === "open" && a.level === "red" && "animate-pulse")} />
              <span className="min-w-0 flex-1">
                <span className="flex flex-wrap items-center gap-1.5">
                  <span className="text-[13px] font-medium">{a.title}</span>
                  <Chip size="sm" tone={a.level === "red" ? "down" : a.level === "amber" ? "warn" : "neutral"}>
                    {ALERT_KIND_LABEL[a.kind] ?? a.kind}
                  </Chip>
                  {a.count > 1 && (
                    <Chip size="sm" tone="neutral">
                      <span className="k-num">×{a.count}</span>
                    </Chip>
                  )}
                  {detailOf(a).map((d) => (
                    <Chip key={d} size="sm" tone="neutral">
                      {d}
                    </Chip>
                  ))}
                </span>
                <span className="mt-0.5 block text-[12px] leading-snug text-fg-2">{a.message}</span>
                <span className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-0.5 text-[11px] text-fg-3">
                  <span>{ago(a.updatedAt, now)}</span>
                  {a.symbol && <span className="font-mono">{a.symbol}</span>}
                  {a.login && (
                    <Link href={`/trading/accounts?login=${a.login}`} className="font-mono hover:text-ember">
                      {a.login}
                    </Link>
                  )}
                  {a.status === "acked" && <span>Acknowledged by {a.ackedBy ?? "staff"}</span>}
                </span>
              </span>
              {a.status === "open" && canAck && (
                <Button size="xs" variant="surface" disabled={busy === a.id} onClick={() => void ack(a)}>
                  <Check /> {busy === a.id ? "…" : "Ack"}
                </Button>
              )}
            </div>
          ))
        )}
        {list.length > max && (
          <button type="button" onClick={() => setMore((m) => !m)} className="w-full pt-1 text-center text-[12px] text-fg-2 hover:text-fg">
            {more ? "Show fewer" : `Show all ${list.length}`}
          </button>
        )}
      </div>
    </Card>
  );
}
