"use client";

import * as React from "react";
import { Clock, Mail, X } from "lucide-react";
import { toast } from "sonner";
import { Button, Dialog, Field, Input, Segmented, Toggle, cn } from "@kalks/ui";
import { ANL_REPORT_TYPES, type AnlFreq, type AnlSchedule } from "@kalks/mock/admin-growth-analytics";

const DAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const MDAYS = ["1st", "2nd", "5th", "15th", "Last"];
const EMAIL = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

export type ScheduleDraft = Pick<AnlSchedule, "name" | "report" | "frequency" | "day" | "time" | "recipients" | "format" | "enabled">;

/** Create / edit a scheduled report — recipients chips input, frequency, day and time. */
export function ScheduleDialog({
  open,
  onOpenChange,
  initial,
  onSave,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  initial?: AnlSchedule | null;
  onSave: (d: ScheduleDraft) => void;
}) {
  const blank: ScheduleDraft = { name: "", report: ANL_REPORT_TYPES[0]!, frequency: "daily", day: undefined, time: "08:00", recipients: ["priya.nair@kalkstrade.com"], format: "PDF", enabled: true };
  const [d, setD] = React.useState<ScheduleDraft>(blank);
  const [entry, setEntry] = React.useState("");
  const [err, setErr] = React.useState<string | undefined>();

  React.useEffect(() => {
    if (open) {
      setD(initial ? { name: initial.name, report: initial.report, frequency: initial.frequency, day: initial.day, time: initial.time, recipients: [...initial.recipients], format: initial.format, enabled: initial.enabled } : blank);
      setEntry("");
      setErr(undefined);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, initial]);

  const add = (raw: string) => {
    const parts = raw.split(/[\s,;]+/).filter(Boolean);
    const bad = parts.filter((p) => !EMAIL.test(p));
    const good = parts.filter((p) => EMAIL.test(p) && !d.recipients.includes(p));
    if (good.length) setD((x) => ({ ...x, recipients: [...x.recipients, ...good] }));
    setErr(bad.length ? `Not a valid email: ${bad.join(", ")}` : undefined);
    setEntry(bad.join(" "));
  };

  const setFreq = (f: AnlFreq) => setD((x) => ({ ...x, frequency: f, day: f === "weekly" ? "Mon" : f === "monthly" ? "1st" : undefined }));

  const save = () => {
    if (!d.name.trim()) return setErr("Give the schedule a name");
    if (!d.recipients.length) return setErr("Add at least one recipient");
    onSave(d);
    onOpenChange(false);
  };

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      width={600}
      title={initial ? `Edit · ${initial.name}` : "New scheduled report"}
      description="Reports run on server time (GMT+3) and are delivered as attachments or SFTP drops."
      footer={
        <>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button variant="surface" onClick={() => toast.success("Test delivery sent", { description: `${d.report} → ${d.recipients[0] ?? "no recipients"}` })}>
            Send test
          </Button>
          <Button variant="ember" onClick={save}>
            {initial ? "Save changes" : "Create schedule"}
          </Button>
        </>
      }
    >
      <div className="space-y-5">
        <Field label="Name">
          <Input value={d.name} onChange={(e) => setD({ ...d, name: e.target.value })} placeholder="e.g. Daily P&L flash" />
        </Field>

        <Field label="Report">
          <div className="flex flex-wrap gap-1.5">
            {ANL_REPORT_TYPES.map((r) => (
              <button
                key={r}
                type="button"
                onClick={() => setD({ ...d, report: r })}
                className={cn("h-8 rounded-full border px-3 text-[12.5px] transition-colors", d.report === r ? "border-ember/40 bg-ember-soft text-ember" : "border-line bg-surface-2 text-fg-2 hover:bg-surface-3")}
              >
                {r}
              </button>
            ))}
          </div>
        </Field>

        <div className="grid grid-cols-1 gap-4 sm:grid-cols-[auto_1fr]">
          <Field label="Frequency">
            <Segmented value={d.frequency} onChange={setFreq} options={[{ value: "daily", label: "Daily" }, { value: "weekly", label: "Weekly" }, { value: "monthly", label: "Monthly" }]} />
          </Field>
          <Field label="Time" hint="GMT+3">
            <Input type="time" value={d.time} onChange={(e) => setD({ ...d, time: e.target.value })} leading={<Clock />} className="h-10" />
          </Field>
        </div>

        {d.frequency !== "daily" && (
          <Field label={d.frequency === "weekly" ? "Day of week" : "Day of month"}>
            <div className="flex flex-wrap gap-1.5">
              {(d.frequency === "weekly" ? DAYS : MDAYS).map((x) => (
                <button
                  key={x}
                  type="button"
                  onClick={() => setD({ ...d, day: x })}
                  className={cn("h-9 min-w-12 rounded-full border px-3 text-[12.5px] font-medium transition-colors", d.day === x ? "border-ember/40 bg-ember-soft text-ember" : "border-line bg-surface-2 text-fg-2 hover:bg-surface-3")}
                >
                  {x}
                </button>
              ))}
            </div>
          </Field>
        )}

        <Field label="Recipients" hint={`${d.recipients.length} added · Enter or comma to add`} error={err}>
          <div className="flex min-h-11 flex-wrap items-center gap-1.5 rounded-[14px] border border-line bg-surface-2 px-2.5 py-2 focus-within:border-ember/50 focus-within:ring-4 focus-within:ring-ember/10">
            <Mail className="ml-1 size-4 text-fg-3" />
            {d.recipients.map((r) => (
              <span key={r} className="inline-flex h-7 items-center gap-1.5 rounded-full border border-line bg-surface-3 pl-2.5 pr-1 text-[12px] text-fg">
                {r}
                <button type="button" aria-label={`Remove ${r}`} onClick={() => setD({ ...d, recipients: d.recipients.filter((x) => x !== r) })} className="grid size-5 place-items-center rounded-full text-fg-3 hover:bg-surface-2 hover:text-fg">
                  <X className="size-3" />
                </button>
              </span>
            ))}
            <input
              value={entry}
              onChange={(e) => {
                const v = e.target.value;
                if (/[,;]$/.test(v)) add(v);
                else setEntry(v);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  add(entry);
                } else if (e.key === "Backspace" && !entry && d.recipients.length) {
                  setD({ ...d, recipients: d.recipients.slice(0, -1) });
                }
              }}
              onBlur={() => entry && add(entry)}
              placeholder={d.recipients.length ? "" : "name@kalkstrade.com"}
              className="h-7 min-w-[140px] flex-1 bg-transparent px-1 text-[13px] text-fg outline-none placeholder:text-fg-3"
            />
          </div>
        </Field>

        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <Field label="Format">
            <Segmented value={d.format} onChange={(f) => setD({ ...d, format: f })} options={["PDF", "XLSX", "CSV", "XML"] as const} />
          </Field>
          <Field label="Enabled">
            <div className="flex h-10 items-center gap-3">
              <Toggle checked={d.enabled} onChange={(v) => setD({ ...d, enabled: v })} label="Enabled" />
              <span className="text-[13px] text-fg-2">{d.enabled ? "Runs on schedule" : "Paused"}</span>
            </div>
          </Field>
        </div>
      </div>
    </Dialog>
  );
}
