"use client";

import { IS_DEMO } from "@kalks/mock/mode";
import { LiveMaintenance } from "@/components/rbac/settings";

import * as React from "react";
import { AnimatePresence, motion } from "motion/react";
import { toast } from "sonner";
import { AlertTriangle, CalendarClock, ExternalLink, Info, MoreHorizontal, Plus, Power, Repeat, ShieldCheck, Trash2, Wrench, X } from "lucide-react";
import { Avatar, Button, Card, CardHeader, Chip, Dialog, Field, IconButton, Input, Menu, PageHeader, Reveal, Segmented, Starfield, Toggle, cn, formatDateTime } from "@kalks/ui";
import { SET_BYPASS_IPS, SET_MAINTENANCE_WINDOWS, type SetMaintenanceWindow } from "@kalks/mock/admin-platform-settings";
import { BrandImg, SectionLabel } from "@/components/settings/kit";

const SCOPES = ["Client Area", "Trading terminal", "API", "Wallet deposits", "Back Office"] as const;

function durationLabel(m: number) {
  const h = Math.floor(m / 60);
  return h ? `${h}h${m % 60 ? ` ${m % 60}m` : ""}` : `${m}m`;
}

function ScopePicker({ value, onChange }: { value: string[]; onChange: (v: string[]) => void }) {
  return (
    <div className="flex flex-wrap gap-1.5">
      {SCOPES.map((s) => {
        const on = value.includes(s);
        return (
          <button
            key={s}
            type="button"
            onClick={() => onChange(on ? value.filter((x) => x !== s) : [...value, s])}
            className={cn("rounded-full border px-3 py-1.5 text-[12px] font-medium transition-colors", on ? "border-ember/40 bg-ember-soft text-ember" : "border-line text-fg-3 hover:text-fg-2")}
          >
            {s}
          </button>
        );
      })}
    </div>
  );
}

function MaintenancePage() {
  const [on, setOn] = React.useState(false);
  const [confirm, setConfirm] = React.useState(false);
  const [scope, setScope] = React.useState<string[]>(["Client Area", "Trading terminal", "API"]);
  const [title, setTitle] = React.useState("We're upgrading Kalks");
  const [message, setMessage] = React.useState("Scheduled maintenance is in progress. Trading, deposits and withdrawals will be back by 04:00 server time (GMT+3). Open positions are safe; stop-loss and take-profit orders stay active on the server.");
  const [until, setUntil] = React.useState("04:00");
  const [banner, setBanner] = React.useState(true);
  const [bannerTone, setBannerTone] = React.useState<"info" | "warn">("warn");
  const [bannerText, setBannerText] = React.useState("Scheduled maintenance Sat 26 Sep, 02:00–04:00 GMT+3. The trading terminal and API will be unavailable.");
  const [preview, setPreview] = React.useState<"banner" | "page">("page");
  const [ips, setIps] = React.useState(SET_BYPASS_IPS);
  const [newIp, setNewIp] = React.useState("");
  const [windows, setWindows] = React.useState<SetMaintenanceWindow[]>(SET_MAINTENANCE_WINDOWS);
  const [sched, setSched] = React.useState(false);
  const [draft, setDraft] = React.useState({ title: "", date: "2026-10-17", time: "02:00", duration: "60", scope: ["Trading terminal"] as string[], notify: true });

  const upcoming = windows.filter((w) => w.status !== "completed");
  const past = windows.filter((w) => w.status === "completed");

  return (
    <div className="pb-16">
      <PageHeader
        title="Maintenance"
        subtitle="Take the platform offline gracefully, warn clients with a banner and schedule recurring windows. Staff on bypass IPs keep full access."
        actions={
          <>
            <Button variant="surface" onClick={() => toast("Opening status.kalkstrade.com")}>
              <ExternalLink /> Status page
            </Button>
            <Button variant="ember" onClick={() => setSched(true)}>
              <CalendarClock /> Schedule window
            </Button>
          </>
        }
      />

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
        {/* Mode */}
        <Reveal className="xl:col-span-7">
          <Card hot={on} className={cn("h-full overflow-hidden transition-colors", on && "border-ember/30")}>
            {on && <Starfield density={40} />}
            <div className="relative flex flex-wrap items-center gap-4 px-6 pt-6">
              <span className={cn("grid size-14 place-items-center rounded-full border transition-colors [&_svg]:size-6", on ? "border-ember/40 bg-ember-soft text-ember shadow-[0_0_40px_-6px_rgba(255,90,31,0.7)]" : "border-line bg-surface-2 text-up")}>
                {on ? <Wrench /> : <Power />}
              </span>
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-2">
                  <h2 className="text-[20px] font-medium tracking-tight">Maintenance mode</h2>
                  {on ? (
                    <Chip tone="ember" dot>
                      <span className="animate-pulse">Active</span>
                    </Chip>
                  ) : (
                    <Chip tone="up" dot>
                      Off · all systems operational
                    </Chip>
                  )}
                </div>
                <p className="mt-1 text-[13px] text-fg-2">{on ? `Clients see the maintenance screen on ${scope.join(", ")}.` : "Turning this on logs everyone out of the selected surfaces except bypass IPs."}</p>
              </div>
              <div className="scale-125">
                <Toggle
                  checked={on}
                  onChange={(v) => {
                    if (v) setConfirm(true);
                    else {
                      setOn(false);
                      toast.success("Maintenance mode ended", { description: "Clients can log in again · status page updated" });
                    }
                  }}
                  label="Maintenance mode"
                />
              </div>
            </div>

            <div className="relative space-y-4 px-6 pb-6 pt-6">
              <div>
                <div className="mb-2 text-[12.5px] font-medium text-fg-2">Affected surfaces</div>
                <ScopePicker value={scope} onChange={setScope} />
              </div>
              <div className="grid grid-cols-1 gap-4 sm:grid-cols-[1fr_150px]">
                <Field label="Screen title">
                  <Input value={title} onChange={(e) => setTitle(e.target.value)} className={on ? "bg-black/30" : undefined} />
                </Field>
                <Field label="Back by (GMT+3)">
                  <Input value={until} onChange={(e) => setUntil(e.target.value)} inputClassName="font-mono" className={on ? "bg-black/30" : undefined} />
                </Field>
              </div>
              <Field label="Message" hint={`${message.length} / 280`}>
                <textarea
                  value={message}
                  rows={3}
                  onChange={(e) => setMessage(e.target.value.slice(0, 280))}
                  className={cn("resize-none rounded-[14px] border border-line bg-surface-2 px-3.5 py-3 text-[13px] leading-relaxed text-fg outline-none focus:border-ember/50 focus:ring-4 focus:ring-ember/10", on && "bg-black/30")}
                />
              </Field>
              <div className="flex flex-wrap items-center gap-2 text-[12px] text-fg-3">
                <Info className="size-3.5" /> Auto-translated into 22 languages ·
                <button type="button" className="text-ember hover:underline" onClick={() => toast("Opening Content → Translations for key maintenance.*")}>
                  edit translations
                </button>
              </div>
            </div>
          </Card>
        </Reveal>

        {/* Preview */}
        <Reveal delay={0.05} className="xl:col-span-5">
          <Card className="flex h-full flex-col">
            <CardHeader
              title="Client preview"
              subtitle="What clients see right now"
              action={
                <Segmented
                  size="xs"
                  value={preview}
                  onChange={setPreview}
                  options={[
                    { value: "page", label: "Screen" },
                    { value: "banner", label: "Banner" },
                  ]}
                />
              }
            />
            <div className="flex-1 px-4 pb-6 pt-4 sm:px-6">
              <div className="relative h-full min-h-[340px] overflow-hidden rounded-[18px] border border-line bg-bg">
                {/* eslint-disable-next-line @next/next/no-img-element */}
                <img src="/assets/photos/trading-screen.jpg" alt="" className={cn("absolute inset-0 size-full object-cover transition-opacity", preview === "page" ? "opacity-15" : "opacity-30")} />
                <div className="absolute inset-0 bg-gradient-to-b from-transparent via-bg/60 to-bg" />
                <div className="pointer-events-none absolute inset-x-0 -top-20 h-48 bg-[radial-gradient(360px_140px_at_50%_0%,rgba(255,92,31,0.5),transparent_70%)]" />
                <AnimatePresence mode="wait">
                  {preview === "page" ? (
                    <motion.div key="page" initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0 }} className="relative flex h-full min-h-[340px] flex-col items-center justify-center px-6 py-8 text-center">
                      <BrandImg src="/assets/brand/kalks-logo.svg" className="h-5" />
                      <span className="mt-6 grid size-14 place-items-center rounded-full border border-ember/40 bg-ember-soft text-ember shadow-[0_0_40px_-6px_rgba(255,90,31,0.7)]">
                        <Wrench className="size-6" />
                      </span>
                      <h3 className="mt-5 text-[20px] font-medium tracking-tight">{title || "Maintenance"}</h3>
                      <p className="mt-2 max-w-sm text-[12.5px] leading-relaxed text-fg-2">{message}</p>
                      <div className="mt-5 flex items-center gap-2 font-mono text-[13px]">
                        {["01", "42", "18"].map((d, i) => (
                          <React.Fragment key={i}>
                            <span className="rounded-[10px] border border-line bg-surface-2 px-2.5 py-1.5">{d}</span>
                            {i < 2 && <span className="text-fg-3">:</span>}
                          </React.Fragment>
                        ))}
                      </div>
                      <div className="mt-2 text-[11px] text-fg-3">Back by {until} GMT+3 · status.kalkstrade.com</div>
                    </motion.div>
                  ) : (
                    <motion.div key="banner" initial={{ opacity: 0, y: -8 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0 }} className="relative p-4">
                      <div className={cn("flex items-start gap-3 rounded-[14px] border px-4 py-3 text-[12.5px] backdrop-blur", bannerTone === "warn" ? "border-warn/30 bg-warn-soft text-fg" : "border-info/30 bg-info-soft text-fg")}>
                        {bannerTone === "warn" ? <AlertTriangle className="mt-0.5 size-4 shrink-0 text-warn" /> : <Info className="mt-0.5 size-4 shrink-0 text-info" />}
                        <span className="flex-1 leading-relaxed">{bannerText}</span>
                        <X className="mt-0.5 size-3.5 shrink-0 text-fg-3" />
                      </div>
                      <div className="mt-5 px-1">
                        <div className="text-[15px] font-medium">Good evening, Arjun</div>
                        <div className="mt-3 grid grid-cols-2 gap-2">
                          <div className="k-card rounded-[14px] p-3">
                            <div className="text-[9.5px] font-semibold uppercase tracking-wider text-fg-2">Total equity</div>
                            <div className="k-num mt-1 text-[17px] font-semibold">
                              $48,915<span className="opacity-40">.60</span>
                            </div>
                          </div>
                          <div className="k-card rounded-[14px] p-3">
                            <div className="text-[9.5px] font-semibold uppercase tracking-wider text-fg-2">Wallet</div>
                            <div className="k-num mt-1 text-[17px] font-semibold">
                              $6,120<span className="opacity-40">.00</span>
                            </div>
                          </div>
                        </div>
                      </div>
                    </motion.div>
                  )}
                </AnimatePresence>
              </div>
            </div>
          </Card>
        </Reveal>

        {/* Banner */}
        <Reveal delay={0.05} className="xl:col-span-7">
          <Card className="h-full">
            <CardHeader
              icon={<AlertTriangle />}
              title="Announcement banner"
              subtitle="Non-blocking notice shown at the top of the Client Area before a window"
              action={
                <Toggle
                  checked={banner}
                  onChange={(v) => {
                    setBanner(v);
                    toast.success(v ? "Banner is live for all clients" : "Banner hidden");
                  }}
                  label="Show banner"
                />
              }
            />
            <div className={cn("space-y-4 px-4 pb-6 pt-5 sm:px-6", !banner && "opacity-50")}>
              <Field label="Banner text" hint={`${bannerText.length} / 160`}>
                <Input value={bannerText} onChange={(e) => setBannerText(e.target.value.slice(0, 160))} />
              </Field>
              <div className="flex flex-wrap items-center gap-3">
                <span className="text-[12.5px] font-medium text-fg-2">Tone</span>
                <Segmented
                  size="xs"
                  value={bannerTone}
                  onChange={setBannerTone}
                  options={[
                    { value: "warn", label: "Warning" },
                    { value: "info", label: "Info" },
                  ]}
                />
                <span className="text-[12.5px] text-fg-3">Visible until the window ends · segment: all clients</span>
                <Button size="sm" variant="surface" className="ml-auto" onClick={() => { setPreview("banner"); toast.success("Banner saved"); }}>
                  Save banner
                </Button>
              </div>
            </div>
          </Card>
        </Reveal>

        {/* Bypass */}
        <Reveal delay={0.1} className="xl:col-span-5">
          <Card className="h-full">
            <CardHeader icon={<ShieldCheck />} title="Staff bypass" subtitle="These IPs keep access during maintenance" />
            <div className="space-y-2 px-4 pb-6 pt-4 sm:px-6">
              {ips.map((ip) => (
                <div key={ip} className="k-row flex items-center gap-3 px-4 py-2.5">
                  <span className="size-1.5 rounded-full bg-up" />
                  <span className="flex-1 font-mono text-[12.5px]">{ip}</span>
                  <button
                    type="button"
                    onClick={() => {
                      setIps((s) => s.filter((x) => x !== ip));
                      toast.success(`${ip} removed from bypass list`);
                    }}
                    className="grid size-7 place-items-center rounded-full text-fg-3 hover:bg-down-soft hover:text-down"
                    aria-label={`Remove ${ip}`}
                  >
                    <Trash2 className="size-3.5" />
                  </button>
                </div>
              ))}
              <form
                className="flex gap-2 pt-1"
                onSubmit={(e) => {
                  e.preventDefault();
                  if (!/^[0-9a-f:.]+(\/\d{1,3})?$/i.test(newIp)) {
                    toast.error("Enter an IP or CIDR range");
                    return;
                  }
                  setIps((s) => [...s, newIp]);
                  setNewIp("");
                  toast.success("Bypass IP added");
                }}
              >
                <Input value={newIp} onChange={(e) => setNewIp(e.target.value)} placeholder="94.206.41.18 or 10.0.0.0/24" className="h-10 flex-1" inputClassName="font-mono text-[12.5px]" />
                <Button size="sm" variant="surface" type="submit" className="h-10">
                  <Plus /> Add
                </Button>
              </form>
            </div>
          </Card>
        </Reveal>
      </div>

      {/* Windows */}
      <Reveal delay={0.05}>
        <Card className="mt-4">
          <CardHeader title="Scheduled windows" subtitle="Times in server time (GMT+3). Clients are notified 24h and 1h before." action={<Chip tone="ember">{upcoming.length} upcoming</Chip>} />
          <div className="space-y-5 px-4 pb-6 pt-4 sm:px-6">
            {[
              ["Upcoming", upcoming],
              ["Completed", past],
            ].map(([label, rows]) => (
              <div key={label as string}>
                <SectionLabel>{label as string}</SectionLabel>
                <div className="mt-3 space-y-2">
                  {(rows as SetMaintenanceWindow[]).map((w) => (
                    <div key={w.id} className={cn("k-row flex flex-col gap-3 px-4 py-3 md:flex-row md:items-center", w.status === "completed" && "opacity-70")}>
                      <div className="flex w-full items-center gap-3 md:w-56 md:shrink-0">
                        <div className="grid w-14 shrink-0 place-items-center rounded-[12px] border border-line bg-surface-3 py-1.5 text-center">
                          <div className="text-[10px] uppercase text-fg-3">{formatDateTime(w.start, { month: "short" })}</div>
                          <div className="k-num text-[18px] font-semibold leading-tight">{formatDateTime(w.start, { day: "2-digit" })}</div>
                        </div>
                        <div className="min-w-0">
                          <div className="font-mono text-[13px]">
                            {formatDateTime(w.start, { hour: "2-digit", minute: "2-digit" })} · {durationLabel(w.durationMin)}
                          </div>
                          <div className="text-[11.5px] text-fg-3">{formatDateTime(w.start, { weekday: "long" })}</div>
                        </div>
                      </div>
                      <div className="min-w-0 flex-1">
                        <div className="flex flex-wrap items-center gap-2">
                          <span className="text-[14px] font-medium">{w.title}</span>
                          {w.recurring && (
                            <Chip size="sm" tone="info">
                              <Repeat className="size-3" /> {w.recurring}
                            </Chip>
                          )}
                        </div>
                        <div className="mt-1.5 flex flex-wrap gap-1">
                          {w.scope.map((s) => (
                            <span key={s} className="rounded-md bg-surface-3 px-1.5 py-0.5 text-[10.5px] text-fg-2">
                              {s}
                            </span>
                          ))}
                        </div>
                      </div>
                      <div className="flex items-center gap-3">
                        <Avatar src={w.ownerPhoto} name={w.owner} size={26} />
                        <label className="flex items-center gap-2 text-[12px] text-fg-3">
                          Notify
                          <Toggle
                            checked={w.notify}
                            onChange={(v) => {
                              setWindows((ws) => ws.map((x) => (x.id === w.id ? { ...x, notify: v } : x)));
                              toast.success(v ? "Clients will be notified" : "Client notifications off");
                            }}
                            label="Notify clients"
                          />
                        </label>
                        <Chip tone={w.status === "completed" ? "up" : "ember"} dot>
                          {w.status === "completed" ? "Completed" : "Scheduled"}
                        </Chip>
                        <Menu
                          trigger={
                            <IconButton size="sm" aria-label="Window actions">
                              <MoreHorizontal />
                            </IconButton>
                          }
                          items={[
                            { label: "Start now", icon: <Power />, onSelect: () => { setScope(w.scope.filter((s) => (SCOPES as readonly string[]).includes(s))); setConfirm(true); } },
                            { label: "Post to status page", icon: <ExternalLink />, onSelect: () => toast.success("Posted to status.kalkstrade.com", { description: w.title }) },
                            "sep",
                            { label: "Cancel window", icon: <Trash2 />, danger: true, onSelect: () => { setWindows((ws) => ws.filter((x) => x.id !== w.id)); toast.success("Window cancelled", { description: "Clients notified of the cancellation" }); } },
                          ]}
                        />
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </Card>
      </Reveal>

      {/* Confirm enable */}
      <Dialog
        open={confirm}
        onOpenChange={setConfirm}
        title="Enable maintenance mode?"
        description="Clients on the selected surfaces are logged out immediately. Open positions, SL/TP and pending orders stay on the server."
        footer={
          <>
            <Button variant="ghost" onClick={() => setConfirm(false)}>
              Cancel
            </Button>
            <Button
              variant="ember"
              onClick={() => {
                setOn(true);
                setConfirm(false);
                setPreview("page");
                toast.warning("Maintenance mode is ON", { description: `${scope.join(", ")} · ${ips.length} bypass IPs · status page updated` });
              }}
            >
              <Wrench /> Enable now
            </Button>
          </>
        }
      >
        <div className="space-y-3">
          <ScopePicker value={scope} onChange={setScope} />
          <div className="k-row flex items-start gap-3 px-4 py-3 text-[12.5px] text-fg-2">
            <AlertTriangle className="mt-0.5 size-4 shrink-0 text-warn" />
            <span>
              <span className="k-num font-medium text-fg">3,184</span> clients are online now. <span className="k-num font-medium text-fg">412</span> have open positions — consider using the announcement banner first.
            </span>
          </div>
        </div>
      </Dialog>

      {/* Schedule */}
      <Dialog
        open={sched}
        onOpenChange={setSched}
        title="Schedule maintenance window"
        description="Server time (GMT+3). A banner goes live 24h before."
        width={560}
        footer={
          <>
            <Button variant="ghost" onClick={() => setSched(false)}>
              Cancel
            </Button>
            <Button
              variant="ember"
              onClick={() => {
                if (!draft.title) {
                  toast.error("Give the window a title");
                  return;
                }
                const iso = new Date(`${draft.date}T${draft.time}:00+03:00`).toISOString();
                setWindows((ws) => [{ id: `mw_${ws.length + 10}`, title: draft.title, scope: draft.scope, start: iso, durationMin: Number(draft.duration) || 60, status: "scheduled", notify: draft.notify, owner: "Priya Nair", ownerPhoto: "/assets/people/women-68.jpg" }, ...ws]);
                setSched(false);
                toast.success("Window scheduled", { description: `${draft.date} ${draft.time} GMT+3 · ${durationLabel(Number(draft.duration) || 60)}` });
              }}
            >
              <Plus /> Schedule
            </Button>
          </>
        }
      >
        <div className="space-y-4">
          <Field label="Title">
            <Input value={draft.title} onChange={(e) => setDraft((d) => ({ ...d, title: e.target.value }))} placeholder="e.g. Matching engine upgrade" />
          </Field>
          <div className="grid grid-cols-3 gap-3">
            <Field label="Date">
              <Input type="date" value={draft.date} onChange={(e) => setDraft((d) => ({ ...d, date: e.target.value }))} inputClassName="font-mono text-[12.5px] [color-scheme:dark]" />
            </Field>
            <Field label="Start">
              <Input type="time" value={draft.time} onChange={(e) => setDraft((d) => ({ ...d, time: e.target.value }))} inputClassName="font-mono text-[12.5px] [color-scheme:dark]" />
            </Field>
            <Field label="Minutes">
              <Input value={draft.duration} onChange={(e) => setDraft((d) => ({ ...d, duration: e.target.value.replace(/\D/g, "") }))} inputClassName="k-num" />
            </Field>
          </div>
          <div>
            <div className="mb-2 text-[12.5px] font-medium text-fg-2">Surfaces</div>
            <ScopePicker value={draft.scope} onChange={(v) => setDraft((d) => ({ ...d, scope: v }))} />
          </div>
          <div className="k-row flex items-center justify-between px-4 py-3">
            <span className="text-[13px]">Notify clients (email + in-app, 24h and 1h before)</span>
            <Toggle checked={draft.notify} onChange={(v) => setDraft((d) => ({ ...d, notify: v }))} />
          </div>
        </div>
      </Dialog>
    </div>
  );
}

export default function Page() {
  return IS_DEMO ? <MaintenancePage /> : <LiveMaintenance />;
}
