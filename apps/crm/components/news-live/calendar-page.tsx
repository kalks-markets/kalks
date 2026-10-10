"use client";

import * as React from "react";
import Link from "next/link";
import { AnimatePresence, motion } from "motion/react";
import { toast } from "sonner";
import { BellOff, BellPlus, BellRing, ChevronDown, ChevronLeft, ChevronRight, Clock3, Flame, Info } from "lucide-react";
import { Button, Card, Chip, Delta, PageHeader, PriceText, Reveal, Segmented, Skeleton, SymbolAvatar, Toggle, cn, useQuotes } from "@/components/kit";
import { INSTRUMENT_MAP } from "@kalks/mock";
import { ColumnBars } from "@/components/portfolio/charts";
import { NewsError, newsApi, useNewsApi, type CalDetail, type CalEvent, type CalendarWeek, type MyCalendar } from "./api";
import { ActualValue, Flag, ImpactBars, countdown, gmt, localTime, useNow } from "./shared";
import { tr, useFormat, useT } from "@kalks/i18n/react";
import { TERMINAL_URL } from "@/lib/live";
import { PageHero } from "@/components/page-hero";
type Formatter = ReturnType<typeof useFormat>;

const CCYS = ["USD", "EUR", "GBP", "JPY", "AUD", "CAD", "CHF", "NZD", "CNY"] as const;
const CCY_COUNTRY: Record<string, string> = { USD: "us", EUR: "eu", GBP: "gb", JPY: "jp", AUD: "au", CAD: "ca", CHF: "ch", NZD: "nz", CNY: "cn", INR: "in" };
type Zone = "server" | "local";

function figure(s: string): number | null {
  const t = s.trim().replace(/^[<>~]/, "").replace(/,/g, "");
  const m = t.match(/^(-?\d+(?:\.\d+)?)\s*([KMBT%]?)$/i);
  if (!m) return null;
  return Number(m[1]);
}
function unitOf(s: string) {
  return s.trim().match(/[KMBT%]$/i)?.[0] ?? "";
}

/** Monday 00:00 server time of the week containing `t` (ISO), for the week switcher. */
function addDays(iso: string, d: number) {
  return new Date(new Date(iso).getTime() + d * 86400_000).toISOString();
}

function dayLabel(date: string, f: Formatter) {
  const d = new Date(`${date}T12:00:00Z`);
  return { label: f.date(d, { weekday: "long", timeZone: "UTC" }), date: f.date(d, { day: "numeric", month: "short", timeZone: "UTC" }) };
}

function useMyCalendar() {
  const my = useNewsApi<MyCalendar>("me/calendar");
  const toggleReminder = async (e: CalEvent) => {
    const on = my.data?.reminders.includes(e.id);
    try {
      if (on) await newsApi(`me/calendar/reminders/${e.id}`, { method: "DELETE" });
      else await newsApi("me/calendar/reminders", { body: { eventId: e.id, minutes: 15 } });
      my.setData((d) => (d ? { ...d, reminders: on ? d.reminders.filter((x) => x !== e.id) : [...d.reminders, e.id] } : d));
      toast.success(on ? tr("news.cal.reminder.removed") : tr("news.cal.reminder.set"), { description: on ? e.title : tr("news.cal.reminder.setDesc", { currency: e.currency, title: e.title }) });
    } catch (err) {
      toast.error(err instanceof NewsError ? err.message : tr("news.cal.reminder.error"));
    }
  };
  return { my, toggleReminder };
}

/* ------------------------------------------------------------------ */

function Detail({ e, zone, offset, reminded, onRemind }: { e: CalEvent; zone: Zone; offset: number; reminded: boolean; onRemind: () => void }) {
  const d = useNewsApi<CalDetail>(`calendar/${e.id}`);
  const syms = e.symbols.filter((s) => INSTRUMENT_MAP[s]);
  const qs = useQuotes(syms);
  const unit = unitOf(e.previous || e.forecast || e.actual);
  const t = useT();
  const f = useFormat();
  const hist = (d.data?.history ?? [])
    .map((h) => ({ at: h.startsAt, v: figure(h.actual || "") }))
    .filter((h): h is { at: string; v: number } => h.v !== null);
  const bars = [...hist.map((h) => ({ label: f.date(h.at, { month: "short", year: "2-digit", timeZone: undefined }), value: h.v })), ...(figure(e.actual) !== null ? [{ label: t("news.cal.now"), value: figure(e.actual)! }] : [])];
  const future = new Date(e.startsAt).getTime() > Date.now();
  return (
    <motion.div initial={{ height: 0, opacity: 0 }} animate={{ height: "auto", opacity: 1 }} exit={{ height: 0, opacity: 0 }} transition={{ duration: 0.25, ease: [0.16, 1, 0.3, 1] }} className="overflow-hidden">
      <div className="grid grid-cols-1 gap-4 border-b border-line bg-surface-2/50 px-5 py-5 lg:grid-cols-12">
        <div className="lg:col-span-5">
          <div className="k-label mb-2">{t("news.cal.history")}</div>
          {d.loading ? (
            <Skeleton className="h-[150px] w-full" />
          ) : bars.length >= 2 ? (
            <ColumnBars data={bars} height={150} tone="gold" fit format={(v) => `${v}${unit}`} />
          ) : (
            <div className="k-row grid h-[150px] place-items-center px-4 text-center text-[12.5px] text-fg-3">
              {e.forecast || e.previous ? t("news.cal.historyBuilding") : t("news.cal.noFigures")}
            </div>
          )}
        </div>
        <div className="lg:col-span-4">
          <div className="k-label mb-2">{t("news.cal.thisRelease")}</div>
          <div className="grid grid-cols-3 gap-2">
            {[
              ["actual", t("news.cal.col.actual"), e.actual || (future ? t("common.pending") : "—")],
              ["forecast", t("news.cal.col.forecast"), e.forecast || "—"],
              ["previous", t("news.cal.col.previous"), e.previous || "—"],
            ].map(([id, k, v]) => (
              <div key={id} className="rounded-xl border border-line bg-surface px-3 py-2">
                <div className="text-[11px] text-fg-3">{k}</div>
                <div className={cn("k-num mt-0.5 text-[13.5px] font-medium", id === "actual" && e.surprise === 1 && "text-up", id === "actual" && e.surprise === -1 && "text-down")}>{v}</div>
              </div>
            ))}
          </div>
          <p className="mt-3 text-[12.5px] leading-relaxed text-fg-2">
            {e.allDay ? t("news.cal.allDay") : zone === "server" ? t("news.cal.timeServer", { server: e.serverTime, tz: gmt(offset), local: localTime(e.startsAt) }) : t("news.cal.timeLocal", { server: e.serverTime, local: localTime(e.startsAt) })}
            {e.lowerIsBetter && e.forecast ? ` · ${t("news.cal.lowerIsBetter")}` : ""}
          </p>
          <div className="mt-3 flex flex-wrap gap-2">
            {future && !e.allDay && (
              <Button size="sm" variant={reminded ? "ghost" : "surface"} onClick={onRemind}>
                {reminded ? <BellOff /> : <BellPlus />} {reminded ? t("news.cal.removeReminder") : t("news.cal.remindMe")}
              </Button>
            )}
          </div>
        </div>
        <div className="lg:col-span-3">
          <div className="k-label mb-2">{t("news.cal.toWatch")}</div>
          <div className="space-y-1.5">
            {syms.map((s) => (
              <Link key={s} target="_blank" rel="noopener" href={`${TERMINAL_URL}/?symbol=${s}`} className="k-row flex items-center gap-2.5 bg-surface px-3 py-2 transition-colors hover:border-ember/40">
                <SymbolAvatar symbol={s} size={20} />
                <span className="flex-1 text-[12.5px] font-medium">{s}</span>
                {qs[s] && <PriceText symbol={s} value={qs[s]!.bid} dir={qs[s]!.dir} className="text-[12px]" />}
                {qs[s] && <Delta value={qs[s]!.change} className="w-14 justify-end text-[11px]" />}
              </Link>
            ))}
            {syms.length === 0 && <div className="text-[12px] text-fg-3">{t("news.cal.noLinked")}</div>}
          </div>
        </div>
      </div>
    </motion.div>
  );
}

function AlertsCard({ my, reload }: { my: MyCalendar | null; reload: () => void }) {
  const [busy, setBusy] = React.useState(false);
  const on = !!my?.alerts?.highImpact;
  const minutes = my?.alerts?.minutes ?? 15;
  const t = useT();
  const save = async (enable: boolean, m = minutes) => {
    setBusy(true);
    try {
      if (enable) await newsApi("me/calendar/alerts", { method: "PUT", body: { highImpact: true, currencies: my?.alerts?.currencies ?? [], minutes: m } });
      else await newsApi("me/calendar/alerts", { method: "DELETE" });
      reload();
      toast.success(enable ? t("news.alerts.saved", { minutes: m }) : t("news.alerts.off"));
    } catch (e) {
      toast.error(e instanceof NewsError ? e.message : t("news.alerts.error"));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card className="h-full px-6 py-5">
      <div className="flex items-center justify-between gap-3">
        <div className="k-label">{t("news.alerts.title")}</div>
        <Toggle checked={on} onChange={(v) => !busy && save(v)} label={t("news.alerts.toggle")} />
      </div>
      <p className="mt-2 text-[12.5px] leading-relaxed text-fg-2">
        {on ? t("news.alerts.onText", { minutes }) : t("news.alerts.offText")}
      </p>
      {on && (
        <div className="mt-3">
          <Segmented size="xs" value={String(minutes)} onChange={(v) => save(true, Number(v))} options={["5", "15", "30", "60"].map((m) => ({ value: m, label: t("news.alerts.minutes", { m }) }))} />
        </div>
      )}
      <div className="mt-3 flex items-center gap-1.5 text-[11.5px] text-fg-3">
        <BellRing className="size-3.5" /> {t("news.alerts.reminders", { count: my?.reminders.length ?? 0 })}
      </div>
    </Card>
  );
}

export function LiveCalendarPage() {
  const [weekFrom, setWeekFrom] = React.useState<string | null>(null);
  const cal = useNewsApi<CalendarWeek>(weekFrom ? `calendar?from=${encodeURIComponent(weekFrom)}&to=${encodeURIComponent(addDays(weekFrom, 7))}` : "calendar", 5 * 60_000);
  const next = useNewsApi<{ event: CalEvent | null }>("calendar/next?impact=3", 5 * 60_000);
  const { my, toggleReminder } = useMyCalendar();
  const [day, setDay] = React.useState<string | null>(null);
  const [impacts, setImpacts] = React.useState<number[]>([0, 1, 2, 3]);
  const [ccys, setCcys] = React.useState<string[]>([]);
  const [openId, setOpenId] = React.useState<number | null>(null);
  const [zone, setZone] = React.useState<Zone>("server");
  const now = useNow(1000);
  const t = useT();
  const f = useFormat();

  const data = cal.data;
  const offset = data?.serverOffset ?? 3;
  const serverToday = now ? new Date(now + offset * 3600_000).toISOString().slice(0, 10) : null;
  const days = React.useMemo(() => {
    if (!data) return [];
    const start = new Date(data.from).getTime() + offset * 3600_000;
    return Array.from({ length: 7 }, (_, i) => new Date(start + i * 86400_000).toISOString().slice(0, 10)).filter((d, i) => i < 5 || data.events.some((e) => e.serverDate === d));
  }, [data, offset]);
  React.useEffect(() => {
    if (!days.length) return;
    setDay((d) => (d && days.includes(d) ? d : serverToday && days.includes(serverToday) ? serverToday : days[0]!));
  }, [days, serverToday]);

  const toggle = <T,>(arr: T[], v: T) => (arr.includes(v) ? arr.filter((x) => x !== v) : [...arr, v]);
  const all = data?.events ?? [];
  const events = all.filter((e) => e.serverDate === day && impacts.includes(e.impact) && (!ccys.length || ccys.includes(e.currency)));
  const nowIdx = day === serverToday && now ? events.findIndex((e) => new Date(e.startsAt).getTime() > now) : -1;
  const nextHigh = next.data?.event ?? null;
  const weekLabel = data ? `${f.date(new Date(data.from).getTime() + offset * 3600_000, { day: "numeric", month: "short", timeZone: "UTC" })} – ${f.date(new Date(data.to).getTime() + offset * 3600_000 - 1, { day: "numeric", month: "short", year: "numeric", timeZone: "UTC" })}` : "";
  const shiftWeek = (n: number) => {
    if (!data) return;
    setWeekFrom(addDays(data.from, 7 * n));
    setDay(null);
    setOpenId(null);
  };
  const rowTime = (e: CalEvent) => (e.allDay ? t("news.cal.allDay") : zone === "server" ? e.serverTime : localTime(e.startsAt));

  return (
    <div className="pb-24">
      <PageHero page="news"
        title={t("news.cal.title")}
        lead={data ? t("news.cal.subtitleWeek", { week: weekLabel, zone: zone === "server" ? t("news.cal.zoneServer", { tz: gmt(offset) }) : t("news.cal.zoneLocal") }) : t("news.cal.subtitle")}
        actions={
          <div className="flex items-center gap-2">
            <Segmented size="sm" value={zone} onChange={setZone} options={[{ value: "server", label: t("news.cal.server", { tz: gmt(offset) }) }, { value: "local", label: t("news.cal.myTime") }]} />
            <Button variant="surface" size="sm" onClick={() => shiftWeek(-1)} aria-label={t("news.cal.prevWeek")} disabled={!data}>
              <ChevronLeft className="rtl:-scale-x-100" />
            </Button>
            <Button variant="surface" size="sm" onClick={() => (weekFrom ? (setWeekFrom(null), setDay(null)) : undefined)} disabled={!weekFrom}>
              {t("news.cal.thisWeek")}
            </Button>
            <Button variant="surface" size="sm" onClick={() => shiftWeek(1)} aria-label={t("news.cal.nextWeek")} disabled={!data}>
              <ChevronRight className="rtl:-scale-x-100" />
            </Button>
          </div>
        }
      />

      <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
        <Reveal>
          <Card hot className="relative h-full overflow-hidden px-6 py-5">
            <div className="k-label">{t("news.cal.nextHigh")}</div>
            {nextHigh ? (
              <>
                <div className="mt-2 flex items-center gap-2">
                  <Flag country={nextHigh.country} className="size-5" />
                  <span className="text-[17px] font-medium">
                    {nextHigh.currency} {nextHigh.title}
                  </span>
                </div>
                <div className="mt-2 flex flex-wrap items-center gap-2 text-[12.5px] text-fg-2">
                  <Clock3 className="size-3.5 text-ember" /> {dayLabel(nextHigh.serverDate, f).label} {zone === "server" ? nextHigh.serverTime : localTime(nextHigh.startsAt)} ·
                  <span className="k-num rounded-lg border border-line bg-surface/60 px-2 py-0.5 font-mono text-fg" suppressHydrationWarning>
                    {now ? countdown(new Date(nextHigh.startsAt).getTime() - now) : "--:--:--"}
                  </span>
                </div>
                <div className="mt-2 text-[12px] text-fg-3">
                  {t("news.cal.forecastPrevious", { forecast: nextHigh.forecast || "—", previous: nextHigh.previous || "—" })}
                </div>
              </>
            ) : (
              <div className="mt-2 text-[13px] text-fg-3">{next.loading ? t("common.loading") : t("news.cal.noHigh")}</div>
            )}
          </Card>
        </Reveal>
        <Reveal delay={0.05}>
          <Card className="h-full px-6 py-5">
            <div className="k-label">{t("news.cal.thisWeek")}</div>
            <div className="mt-2 flex items-baseline gap-2">
              <span className="k-num text-[30px] font-semibold">{data ? all.length : "—"}</span>
              <span className="text-[13px] text-fg-3">{t("news.cal.eventsWord")}</span>
            </div>
            <div className="mt-2 flex gap-2">
              <Chip tone="down">
                <Flame className="size-3" /> {t("news.cal.high", { count: all.filter((e) => e.impact === 3).length })}
              </Chip>
              <Chip tone="warn">{t("news.cal.medium", { count: all.filter((e) => e.impact === 2).length })}</Chip>
              <Chip>{t("news.cal.low", { count: all.filter((e) => e.impact === 1).length })}</Chip>
            </div>
            {data?.updatedAt && (
              <div className="mt-3 text-[11px] text-fg-3" suppressHydrationWarning>
                {t("news.cal.updated", { time: new Date(data.updatedAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }), source: data.source.name })}
              </div>
            )}
          </Card>
        </Reveal>
        <Reveal delay={0.1}>
          <AlertsCard my={my.data} reload={my.reload} />
        </Reveal>
      </div>

      <Reveal delay={0.1} className="mt-4">
        <Card className="overflow-hidden">
          <div className="flex gap-2 overflow-x-auto border-b border-line px-4 pt-4 sm:px-6">
            {cal.loading &&
              Array.from({ length: 5 }).map((_, i) => (
                <Skeleton key={i} className="mb-3 h-12 min-w-[96px] flex-1" />
              ))}
            {days.map((d) => {
              const on = d === day;
              const evs = all.filter((e) => e.serverDate === d);
              const l = dayLabel(d, f);
              return (
                <button key={d} onClick={() => setDay(d)} className={cn("relative min-w-[96px] flex-1 rounded-t-2xl px-4 pb-3 pt-2.5 text-start transition-colors", on ? "bg-surface-2" : "hover:bg-surface-2/50")}>
                  <div className="flex items-center gap-2">
                    <span className={cn("text-[13.5px] font-medium", on ? "text-fg" : "text-fg-2")}>{l.label}</span>
                    {d === serverToday && (
                      <Chip size="sm" tone="ember">
                        {t("common.today")}
                      </Chip>
                    )}
                  </div>
                  <div className="mt-0.5 flex items-center justify-between text-[11.5px] text-fg-3">
                    <span>{l.date}</span>
                    <span className="flex items-center gap-1">
                      {evs
                        .filter((e) => e.impact === 3)
                        .slice(0, 6)
                        .map((e) => (
                          <span key={e.id} className="size-1.5 rounded-full bg-down" />
                        ))}
                      <span className="k-num ms-1">{evs.length}</span>
                    </span>
                  </div>
                  {on && <motion.span layoutId="cal-day-live" className="absolute inset-x-3 bottom-0 h-0.5 rounded-full bg-ember" />}
                </button>
              );
            })}
          </div>

          <div className="flex flex-wrap items-center gap-3 px-4 py-4 sm:px-6">
            <div className="flex items-center gap-1.5">
              <span className="me-1 text-[12px] text-fg-3">{t("news.cal.impact")}</span>
              {([3, 2, 1] as const).map((i) => (
                <button
                  key={i}
                  onClick={() => setImpacts((x) => (x.includes(i) && x.filter((y) => y > 0).length === 1 ? x : toggle(x, i)))}
                  className={cn("flex h-8 items-center gap-2 rounded-full border px-3 text-[12px] transition-colors", impacts.includes(i) ? "border-line bg-surface-3 text-fg" : "border-line bg-transparent text-fg-3 opacity-60")}
                >
                  <ImpactBars impact={i} />
                  {i === 3 ? t("news.impact.high") : i === 2 ? t("news.impact.medium") : t("news.impact.low")}
                </button>
              ))}
              <button onClick={() => setImpacts((x) => toggle(x, 0))} className={cn("flex h-8 items-center rounded-full border px-3 text-[12px] transition-colors", impacts.includes(0) ? "border-line bg-surface-3 text-fg" : "border-line text-fg-3 opacity-60")}>
                {t("news.impact.holidays")}
              </button>
            </div>
            <div className="flex flex-wrap items-center gap-1.5">
              <span className="me-1 text-[12px] text-fg-3">{t("news.cal.currency")}</span>
              {CCYS.map((c) => {
                const on = ccys.includes(c);
                return (
                  <button
                    key={c}
                    onClick={() => setCcys((x) => toggle(x, c))}
                    className={cn("flex h-8 items-center gap-1.5 rounded-full border px-2 text-[11.5px] font-medium transition-all", on ? "border-ember/40 bg-ember-soft text-ember" : "border-line bg-surface-2 text-fg-3 hover:text-fg-2", ccys.length && !on && "opacity-50")}
                  >
                    <Flag country={CCY_COUNTRY[c]!} className="size-4" />
                    <span className="hidden sm:inline">{c}</span>
                  </button>
                );
              })}
              {ccys.length > 0 && (
                <button onClick={() => setCcys([])} className="px-2 text-[12px] text-fg-3 hover:text-fg">
                  {t("news.cal.clear")}
                </button>
              )}
            </div>
          </div>

          <div className="overflow-x-auto">
            <div className="min-w-[760px]">
              <div className="mx-4 grid grid-cols-[72px_110px_1fr_70px_90px_90px_90px_32px] items-center rounded-[14px] border border-line bg-surface-2 px-4 py-2.5 text-[11px] font-medium uppercase tracking-[0.05em] text-fg-3 sm:mx-6">
                <span>{t("news.cal.col.time")}</span>
                <span>{t("news.cal.col.currency")}</span>
                <span>{t("news.cal.col.event")}</span>
                <span>{t("news.cal.col.impact")}</span>
                <span className="text-end">{t("news.cal.col.actual")}</span>
                <span className="text-end">{t("news.cal.col.forecast")}</span>
                <span className="text-end">{t("news.cal.col.previous")}</span>
                <span />
              </div>
              <div className="px-4 pb-4 sm:px-6">
                {cal.error && <div className="py-12 text-center text-[13px] text-fg-3">{cal.error.message}</div>}
                {data && events.length === 0 && <div className="py-12 text-center text-[13px] text-fg-3">{t("news.cal.empty")}</div>}
                {events.map((e, i) => {
                  const open = openId === e.id;
                  const past = now !== null && new Date(e.startsAt).getTime() < now;
                  const reminded = !!my.data?.reminders.includes(e.id);
                  return (
                    <React.Fragment key={e.id}>
                      {i === nowIdx && now && (
                        <div className="relative my-1 flex items-center gap-2 py-1">
                          <span className="k-num rounded-full bg-ember px-2 py-0.5 font-mono text-[10.5px] font-semibold text-white" suppressHydrationWarning>
                            {zone === "server" ? new Date(now + offset * 3600_000).toISOString().slice(11, 16) : new Date(now).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
                          </span>
                          <span className="h-px flex-1 bg-ember/50" />
                          <span className="text-[11px] text-ember">{t("news.cal.now")}</span>
                        </div>
                      )}
                      <button
                        onClick={() => setOpenId(open ? null : e.id)}
                        data-testid="calendar-row"
                        className={cn("relative grid w-full grid-cols-[72px_110px_1fr_70px_90px_90px_90px_32px] items-center border-b border-line px-4 py-3 text-start text-[13.5px] transition-colors hover:bg-surface-2/60", open && "bg-surface-2/60", past && !open && "opacity-80")}
                      >
                        <span className={cn("absolute inset-y-2 start-0 w-[3px] rounded-e-full", e.impact === 3 ? "bg-down" : e.impact === 2 ? "bg-warn" : "bg-fg-3/40")} />
                        <span className="k-num font-mono text-[12.5px] text-fg-2">{rowTime(e)}</span>
                        <span className="flex items-center gap-2">
                          <Flag country={e.country} className="size-5 ring-1 ring-black/20" />
                          <span className="font-mono text-[12.5px] font-medium">{e.currency}</span>
                        </span>
                        <span className="flex min-w-0 items-center gap-2 pe-3">
                          <span className="truncate font-medium">{e.title}</span>
                          {reminded && <BellRing className="size-3.5 shrink-0 text-ember" aria-label={t("news.cal.reminderSetAria")} />}
                        </span>
                        {e.impact ? <ImpactBars impact={e.impact} /> : <span className="text-[11.5px] text-fg-3">{t("news.impact.holiday")}</span>}
                        <span className="k-num text-end">
                          <ActualValue e={e} />
                        </span>
                        <span className="k-num text-end text-fg-2">{e.forecast || "—"}</span>
                        <span className="k-num text-end text-fg-3">{e.previous || "—"}</span>
                        <ChevronDown className={cn("ms-auto size-4 text-fg-3 transition-transform", open && "rotate-180 text-fg")} />
                      </button>
                      <AnimatePresence initial={false}>{open && <Detail e={e} zone={zone} offset={offset} reminded={reminded} onRemind={() => toggleReminder(e)} />}</AnimatePresence>
                    </React.Fragment>
                  );
                })}
              </div>
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-4 border-t border-line px-6 py-3 text-[11.5px] text-fg-3">
            <span className="flex items-center gap-1.5">
              <span className="font-semibold text-up">{t("news.cal.legend.green")}</span> {t("news.cal.legend.greenText")}
            </span>
            <span className="flex items-center gap-1.5">
              <span className="font-semibold text-down">{t("news.cal.legend.red")}</span> {t("news.cal.legend.redText")}
            </span>
            <span className="flex items-center gap-1.5">
              <Info className="size-3.5" /> {t("news.cal.source", { source: data?.source.name ?? "Forex Factory" })}
            </span>
          </div>
        </Card>
      </Reveal>
    </div>
  );
}
