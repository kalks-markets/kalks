"use client";

import * as React from "react";
import Link from "next/link";
import { toast } from "sonner";
import { CalendarDays, ExternalLink, Eye, EyeOff, MoreHorizontal, Newspaper, Pin, PinOff, RefreshCw, RotateCcw, Rss, Search, ShieldAlert, Sparkles, Tag } from "lucide-react";
import { Button, Card, CardHeader, Chip, Dialog, DialogClose, EmptyState, IconButton, KpiCard, Menu, PageHeader, Reveal, Segmented, Skeleton, SymbolAvatar, Toggle, Tooltip, cn } from "@kalks/ui";
import { useStaff } from "@/components/staff-session";
import { contentAllows } from "@/lib/academy";
import { COUNTRIES, INSTRUMENTS, KIND_LABEL, NewsError, ago, newsApi, useNews, type AdminItem, type AuditEntry, type Brief, type Source, type Stats } from "./api";

type View = "all" | "pinned" | "hidden" | "retagged";
const PAGE = 40;

function flag(c: string) {
  return <span className={`fi fis fi-${c} size-3.5 shrink-0 rounded-full`} aria-hidden />;
}

/* ------------------------------------------------------------------ */
/* Tag editor                                                          */
/* ------------------------------------------------------------------ */

function TagEditor({ item, onClose, onSaved }: { item: AdminItem | null; onClose: () => void; onSaved: (n: AdminItem) => void }) {
  const [symbols, setSymbols] = React.useState<string[]>([]);
  const [countries, setCountries] = React.useState<string[]>([]);
  const [importance, setImportance] = React.useState(50);
  const [busy, setBusy] = React.useState(false);
  React.useEffect(() => {
    if (!item) return;
    setSymbols(item.symbols);
    setCountries(item.countries);
    setImportance(item.importance);
  }, [item]);
  if (!item) return null;
  const save = async () => {
    setBusy(true);
    try {
      const r = await newsApi<{ item: AdminItem }>(`news/${item.id}`, { method: "PUT", body: { symbols, countries, importance } });
      onSaved(r.item);
      toast.success("Tags saved", { description: symbols.join(", ") || "No instruments" });
      onClose();
    } catch (e) {
      toast.error(e instanceof NewsError ? e.message : "Couldn't save the tags");
    } finally {
      setBusy(false);
    }
  };
  const toggle = (arr: string[], v: string, max: number) => (arr.includes(v) ? arr.filter((x) => x !== v) : arr.length >= max ? arr : [...arr, v]);
  return (
    <Dialog
      open={!!item}
      onOpenChange={(o) => !o && onClose()}
      width={640}
      title="Edit tags"
      description={item.title}
      footer={
        <>
          <DialogClose asChild>
            <Button variant="ghost" size="sm">
              Cancel
            </Button>
          </DialogClose>
          <Button variant="ember" size="sm" onClick={save} disabled={busy} data-testid="tags-save">
            {busy ? "Saving…" : "Save"}
          </Button>
        </>
      }
    >
      <div className="space-y-5">
        <div>
          <div className="mb-1.5 flex items-center justify-between text-[12.5px] font-medium text-fg-2">
            Instruments <span className="font-normal text-fg-3">Up to 6 · automatic: {item.auto.symbols.join(", ") || "none"}</span>
          </div>
          <div className="flex flex-wrap gap-1.5">
            {INSTRUMENTS.map((s) => {
              const on = symbols.includes(s);
              return (
                <button key={s} onClick={() => setSymbols((x) => toggle(x, s, 6))} className={cn("flex items-center gap-1.5 rounded-full border py-0.5 pl-1 pr-2.5 font-mono text-[11.5px] transition-colors", on ? "border-ember/40 bg-ember-soft text-fg" : "border-line bg-surface-2 text-fg-3 hover:text-fg")}>
                  <SymbolAvatar symbol={s} size={16} />
                  {s}
                </button>
              );
            })}
          </div>
        </div>
        <div>
          <div className="mb-1.5 flex items-center justify-between text-[12.5px] font-medium text-fg-2">
            Countries <span className="font-normal text-fg-3">Pins on the world map · up to 6</span>
          </div>
          <div className="flex flex-wrap gap-1.5">
            {COUNTRIES.map(([c, name]) => {
              const on = countries.includes(c);
              return (
                <button key={c} onClick={() => setCountries((x) => toggle(x, c, 6))} className={cn("flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-[11.5px] transition-colors", on ? "border-ember/40 bg-ember-soft text-fg" : "border-line bg-surface-2 text-fg-3 hover:text-fg")}>
                  {flag(c)}
                  {name}
                </button>
              );
            })}
          </div>
        </div>
        <div>
          <div className="mb-1.5 flex items-center justify-between text-[12.5px] font-medium text-fg-2">
            Importance <span className="k-num font-normal text-fg-3">{importance} · automatic {item.auto.importance} · stories below 20 are hidden from the default feed</span>
          </div>
          <input type="range" min={0} max={100} step={5} value={importance} onChange={(e) => setImportance(Number(e.target.value))} className="w-full accent-[var(--k-ember)]" aria-label="Importance" />
        </div>
      </div>
    </Dialog>
  );
}

/* ------------------------------------------------------------------ */
/* Sources                                                             */
/* ------------------------------------------------------------------ */

function SourcesDialog({ open, onOpenChange, canWrite, onChanged }: { open: boolean; onOpenChange: (o: boolean) => void; canWrite: boolean; onChanged: () => void }) {
  const src = useNews<{ sources: Source[]; canToggle: boolean }>(open ? "sources" : null);
  const [busy, setBusy] = React.useState<string | null>(null);
  const can = canWrite && !!src.data?.canToggle;
  const set = async (s: Source, enabled: boolean) => {
    setBusy(s.id);
    try {
      const r = await newsApi<{ sources: Source[]; canToggle: boolean }>(`sources/${s.id}`, { method: "PUT", body: { enabled } });
      src.setData(r);
      onChanged();
      toast.success(`${s.name} ${enabled ? "enabled" : "disabled"}`, { description: enabled ? "Fetched within a minute" : "Its stories are no longer shown to clients" });
    } catch (e) {
      toast.error(e instanceof NewsError ? e.message : "Couldn't update the source");
    } finally {
      setBusy(null);
    }
  };
  const refresh = async (s: Source) => {
    setBusy(s.id);
    try {
      const r = await newsApi<{ result: string }>(`sources/${s.id}/refresh`, { method: "POST", body: {} });
      toast.success(`${s.name} fetched`, { description: r.result });
      src.reload();
      onChanged();
    } catch (e) {
      toast.error(e instanceof NewsError ? e.message : "Couldn't fetch the feed");
    } finally {
      setBusy(null);
    }
  };
  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      width={760}
      title="News sources"
      description="Feeds aggregated into the Client Area, world map and Kalks Trader. Only headline, a short teaser and the link are stored; every story links to the publisher."
    >
      <div className="space-y-2" data-testid="sources-list">
        {!src.data && <Skeleton className="h-64 w-full" />}
        {src.data && !src.data.canToggle && <div className="rounded-xl border border-line bg-surface-2 px-4 py-3 text-[12.5px] text-fg-2">Feed sources are managed by the platform. You can pin, hide and retag stories for your brand.</div>}
        {src.data?.sources.map((s) => (
          <div key={s.id} className={cn("k-row flex flex-wrap items-start gap-3 px-4 py-3", !s.enabled && "opacity-70")} data-testid={`source-${s.id}`}>
            <span className="grid size-9 shrink-0 place-items-center rounded-xl border border-line bg-surface-3 text-fg-2">
              <Rss className="size-4" />
            </span>
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-2 text-[13.5px] font-medium">
                {s.name}
                <Chip size="sm">{KIND_LABEL[s.kind]}</Chip>
                {s.lastError && s.enabled && (
                  <Chip size="sm" tone="down">
                    Failing
                  </Chip>
                )}
                {s.kind === "provider" && !s.connected && <Chip size="sm">Not connected</Chip>}
              </div>
              <div className="mt-0.5 text-[11.5px] text-fg-3">
                <span className="k-num">{s.last24h}</span> stories today · <span className="k-num">{s.items}</span> stored · last fetch {ago(s.lastFetchAt)}
                {s.intervalSecs > 0 && ` · every ${Math.round(s.intervalSecs / 60)} min`}
              </div>
              <div className="mt-1 text-[11.5px] leading-snug text-fg-3">{s.terms}</div>
              {s.lastError && <div className="mt-1 text-[11.5px] text-down">{s.lastError}</div>}
            </div>
            <div className="flex items-center gap-2">
              {can && s.enabled && s.kind !== "provider" && (
                <Tooltip content="Fetch now">
                  <IconButton size="sm" aria-label={`Fetch ${s.name} now`} onClick={() => refresh(s)} disabled={busy === s.id}>
                    <RefreshCw />
                  </IconButton>
                </Tooltip>
              )}
              <Toggle checked={s.enabled} onChange={(v) => can && busy !== s.id && set(s, v)} label={`${s.name} enabled`} />
            </div>
          </div>
        ))}
      </div>
    </Dialog>
  );
}

/* ------------------------------------------------------------------ */
/* Page                                                                */
/* ------------------------------------------------------------------ */

export function LiveNewsCms() {
  const staff = useStaff();
  const canRead = contentAllows(staff, "content.read");
  const canWrite = contentAllows(staff, "content.write");
  const [view, setView] = React.useState<View>("all");
  const [q, setQ] = React.useState("");
  const [search, setSearch] = React.useState("");
  const [source, setSource] = React.useState("");
  const [offset, setOffset] = React.useState(0);
  const [editing, setEditing] = React.useState<AdminItem | null>(null);
  const [sourcesOpen, setSourcesOpen] = React.useState(false);
  const [briefBusy, setBriefBusy] = React.useState(false);
  React.useEffect(() => {
    const t = setTimeout(() => setSearch(q.trim()), 300);
    return () => clearTimeout(t);
  }, [q]);
  React.useEffect(() => setOffset(0), [view, search, source]);

  const qs = new URLSearchParams({ status: view, limit: String(PAGE), offset: String(offset) });
  if (search.length >= 2) qs.set("q", search);
  if (source) qs.set("source", source);
  const list = useNews<{ items: AdminItem[]; total: number }>(canRead ? `news?${qs}` : null);
  const pinned = useNews<{ items: AdminItem[]; total: number }>(canRead ? "news?status=pinned&limit=10" : null);
  const stats = useNews<Stats>(canRead ? "stats" : null);
  const sources = useNews<{ sources: Source[]; canToggle: boolean }>(canRead ? "sources" : null);
  const brief = useNews<Brief>(canRead ? "brief" : null);
  const audit = useNews<{ entries: AuditEntry[] }>(canRead ? "audit" : null);
  const refreshAll = () => {
    list.reload();
    pinned.reload();
    stats.reload();
    sources.reload();
    audit.reload();
  };

  if (!canRead) return <EmptyState illustration="identification_card" title="No access" text="Your role doesn't include content." />;
  if (list.error && !list.data) return <EmptyState illustration="satellite_antenna" title="The news service is unavailable" text={list.error.message} action={<Button variant="surface" onClick={refreshAll}>Try again</Button>} />;

  const patch = async (n: AdminItem, body: Record<string, unknown>, done: string) => {
    try {
      const r = await newsApi<{ item: AdminItem }>(`news/${n.id}`, { method: "PUT", body });
      list.setData((d) => (d ? { ...d, items: d.items.map((x) => (x.id === n.id ? r.item : x)) } : d));
      toast.success(done, { description: n.title });
      pinned.reload();
      stats.reload();
      audit.reload();
    } catch (e) {
      toast.error(e instanceof NewsError ? e.message : "Couldn't update the story");
    }
  };
  const reset = async (n: AdminItem) => {
    try {
      const r = await newsApi<{ item: AdminItem }>(`news/${n.id}`, { method: "DELETE" });
      list.setData((d) => (d ? { ...d, items: d.items.map((x) => (x.id === n.id ? r.item : x)) } : d));
      toast.success("Back to automatic tags", { description: n.title });
      pinned.reload();
      stats.reload();
      audit.reload();
    } catch (e) {
      toast.error(e instanceof NewsError ? e.message : "Couldn't reset the story");
    }
  };
  const regenerate = async () => {
    setBriefBusy(true);
    try {
      await newsApi("brief", { method: "POST", body: {} });
      brief.reload();
      stats.reload();
      toast.success("Today's brief regenerated");
    } catch (e) {
      toast.error(e instanceof NewsError ? e.message : "Couldn't write the brief");
    } finally {
      setBriefBusy(false);
    }
  };

  const s = stats.data;
  const items = list.data?.items ?? [];
  const total = list.data?.total ?? 0;
  const liveSources = (sources.data?.sources ?? []).filter((x) => x.enabled && x.kind !== "provider");
  const maxToday = Math.max(1, ...liveSources.map((x) => x.last24h));
  // feeds, calendar and brief are platform-wide: the news service says whether this tenant may change them
  const isPlatform = !!sources.data?.canToggle;

  return (
    <div className="pb-16">
      <PageHeader
        title="News"
        subtitle="Pin, hide and retag the headlines your clients see on the dashboard, the news page, the world map and Kalks Trader."
        actions={
          <>
            <Link href="/content/calendar">
              <Button variant="surface">
                <CalendarDays /> Calendar
              </Button>
            </Link>
            <Button variant="surface" onClick={() => setSourcesOpen(true)} data-testid="open-sources">
              <Rss /> Sources
            </Button>
          </>
        }
      />

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <KpiCard label="Stories in the last 24 hours" value={<span className="k-num">{s?.news.last24h ?? "—"}</span>} icon={<Newspaper />} chip={s ? `${s.news.total} stored` : undefined} />
        <KpiCard label="Pinned" value={<span className="k-num">{s?.news.pinned ?? "—"}</span>} icon={<Pin />} chip="lead the feed for your clients" delay={0.05} />
        <KpiCard label="Hidden" value={<span className="k-num">{s?.news.hidden ?? "—"}</span>} icon={<ShieldAlert />} chip="for your brand only" delay={0.1} />
        <KpiCard label="Sources live" value={<span className="k-num">{s ? `${s.sources.enabled}/${s.sources.total}` : "—"}</span>} icon={<Rss />} chip={s ? (s.sources.failing ? `${s.sources.failing} failing` : "all healthy") : undefined} chipTone={s?.sources.failing ? "down" : "up"} delay={0.15} />
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal delay={0.05} className="xl:col-span-8">
          <Card className="h-full">
            <CardHeader
              title="Feed"
              subtitle={`${total} stories · newest first`}
              icon={<Newspaper />}
              action={
                <div className="hidden h-9 items-center gap-2 rounded-full border border-line bg-surface-2 px-3.5 md:flex">
                  <Search className="size-3.5 text-fg-3" />
                  <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Search headlines…" className="w-48 bg-transparent text-[13px] outline-none placeholder:text-fg-3" data-testid="news-search" />
                </div>
              }
            />
            <div className="mt-4 flex flex-wrap items-center gap-2 px-4 sm:px-6">
              <select value={source} onChange={(e) => setSource(e.target.value)} className="h-8 rounded-full border border-line bg-surface-2 px-3 text-[12px] text-fg-2 outline-none" aria-label="Source">
                <option value="">All sources</option>
                {(sources.data?.sources ?? []).map((x) => (
                  <option key={x.id} value={x.id}>
                    {x.name}
                  </option>
                ))}
              </select>
              <Segmented
                size="xs"
                className="ml-auto"
                value={view}
                onChange={setView}
                options={[
                  { value: "all", label: "All" },
                  { value: "pinned", label: <><Pin className="size-3" /> Pinned</> },
                  { value: "hidden", label: <><EyeOff className="size-3" /> Hidden</> },
                  { value: "retagged", label: <><Tag className="size-3" /> Retagged</> },
                ]}
              />
            </div>
            <div className="mt-4 space-y-2 px-4 pb-6 sm:px-6" data-testid="admin-news-list">
              {list.loading && Array.from({ length: 6 }).map((_, i) => <Skeleton key={i} className="h-[92px] w-full" />)}
              {items.map((n) => (
                <div key={n.id} data-testid="admin-news-row" data-id={n.id} className={cn("k-row flex flex-col gap-3 p-3 sm:flex-row sm:items-center", n.pinned && "border-ember/30 bg-ember/[0.05]", n.hidden && "opacity-50")}>
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-1.5 text-[11.5px] text-fg-3">
                      {n.countries.slice(0, 3).map((c) => (
                        <React.Fragment key={c}>{flag(c)}</React.Fragment>
                      ))}
                      <span className="font-medium text-fg-2">{n.source.name}</span>·<span>{ago(n.publishedAt)}</span>
                      <Chip size="sm" tone={n.importance >= 70 ? "down" : n.importance >= 40 ? "warn" : "neutral"}>
                        importance {n.importance}
                      </Chip>
                      {n.pinned && (
                        <Chip size="sm" tone="ember">
                          Pinned
                        </Chip>
                      )}
                      {n.hidden && (
                        <Chip size="sm" tone="down">
                          Hidden
                        </Chip>
                      )}
                      {n.retagged && (
                        <Chip size="sm" tone="gold">
                          Retagged
                        </Chip>
                      )}
                      {!n.sourceEnabled && <Chip size="sm">Source off</Chip>}
                    </div>
                    <div className="mt-1 line-clamp-1 text-[14px] font-medium leading-snug">{n.title}</div>
                    {n.summary && <div className="mt-0.5 line-clamp-1 text-[12.5px] text-fg-3">{n.summary}</div>}
                    <div className="mt-2 flex flex-wrap items-center gap-1.5">
                      {n.symbols.map((sym) => (
                        <span key={sym} className="flex items-center gap-1 rounded-md bg-surface-3 py-0.5 pl-0.5 pr-1.5 font-mono text-[10.5px] text-fg-2">
                          <SymbolAvatar symbol={sym} size={14} />
                          {sym}
                        </span>
                      ))}
                      {n.currencies.map((c) => (
                        <span key={c} className="font-mono text-[10.5px] text-fg-3">
                          {c}
                        </span>
                      ))}
                    </div>
                  </div>
                  {canWrite && (
                    <div className="flex shrink-0 items-center gap-1.5">
                      <Tooltip content={n.pinned ? "Unpin" : "Pin to the top of the feed"}>
                        <IconButton size="sm" active={n.pinned} aria-label={n.pinned ? "Unpin" : "Pin"} data-testid="pin" onClick={() => patch(n, n.pinned ? { pinned: false } : { pinned: true, hidden: false }, n.pinned ? "Unpinned" : "Pinned for your clients")}>
                          {n.pinned ? <PinOff /> : <Pin />}
                        </IconButton>
                      </Tooltip>
                      <Tooltip content={n.hidden ? "Show again" : "Hide from your clients"}>
                        <IconButton size="sm" aria-label={n.hidden ? "Show" : "Hide"} data-testid="hide" className={n.hidden ? "border-down/30 bg-down-soft text-down" : undefined} onClick={() => patch(n, n.hidden ? { hidden: false } : { hidden: true, pinned: false }, n.hidden ? "Story visible again" : "Story hidden from your clients")}>
                          {n.hidden ? <EyeOff /> : <Eye />}
                        </IconButton>
                      </Tooltip>
                      <Menu
                        width={220}
                        trigger={
                          <IconButton size="sm" aria-label="More">
                            <MoreHorizontal />
                          </IconButton>
                        }
                        items={[
                          { label: "Edit tags", icon: <Tag />, onSelect: () => setEditing(n) },
                          ...(n.link ? [{ label: "Open original", icon: <ExternalLink />, onSelect: () => window.open(n.link, "_blank", "noopener,noreferrer") }] : []),
                          ...(n.pinned || n.hidden || n.retagged ? (["sep", { label: "Reset to automatic", icon: <RotateCcw />, onSelect: () => reset(n) }] as const) : []),
                        ]}
                      />
                    </div>
                  )}
                </div>
              ))}
              {list.data && items.length === 0 && <div className="py-12 text-center text-[13px] text-fg-3">No stories match these filters.</div>}
              {total > PAGE && (
                <div className="flex items-center justify-between pt-2 text-[12px] text-fg-3">
                  <span className="k-num">
                    {offset + 1}–{Math.min(offset + PAGE, total)} of {total}
                  </span>
                  <div className="flex gap-2">
                    <Button size="sm" variant="surface" disabled={offset === 0} onClick={() => setOffset((o) => Math.max(0, o - PAGE))}>
                      Newer
                    </Button>
                    <Button size="sm" variant="surface" disabled={offset + PAGE >= total} onClick={() => setOffset((o) => o + PAGE)}>
                      Older
                    </Button>
                  </div>
                </div>
              )}
            </div>
          </Card>
        </Reveal>

        <div className="flex flex-col gap-4 xl:col-span-4">
          <Reveal delay={0.1}>
            <Card>
              <CardHeader title="Pinned" subtitle="Shown first to your clients, newest pin first" icon={<Pin />} />
              <div className="mt-4 space-y-2 px-4 pb-5 sm:px-6" data-testid="pinned-list">
                {(pinned.data?.items ?? []).map((n, i) => (
                  <div key={n.id} className="k-row flex items-center gap-3 px-3 py-2.5">
                    <span className="k-num w-4 text-center text-[12px] text-ember">{i + 1}</span>
                    <div className="min-w-0 flex-1">
                      <div className="line-clamp-2 text-[12.5px] font-medium leading-snug">{n.title}</div>
                      <div className="text-[11px] text-fg-3">
                        {n.source.name} · pinned by {n.updatedBy ?? "staff"} {ago(n.updatedAt)}
                      </div>
                    </div>
                    {canWrite && (
                      <IconButton size="sm" aria-label="Unpin" onClick={() => patch(n, { pinned: false }, "Unpinned")}>
                        <PinOff />
                      </IconButton>
                    )}
                  </div>
                ))}
                {pinned.data && pinned.data.items.length === 0 && <div className="rounded-[14px] border border-dashed border-line py-6 text-center text-[12.5px] text-fg-3">Nothing pinned</div>}
              </div>
            </Card>
          </Reveal>
          <Reveal delay={0.15}>
            <Card>
              <CardHeader title="Daily market brief" subtitle={brief.data?.configured ? "Written by Claude each morning (server time) from stored headlines and the calendar" : "Not configured (ANTHROPIC_API_KEY)"} icon={<Sparkles />} />
              <div className="mt-3 px-4 pb-5 sm:px-6">
                {brief.data?.brief ? (
                  <>
                    <div className="text-[13px] font-medium leading-snug">{brief.data.brief.headline}</div>
                    <div className="mt-1 text-[11.5px] text-fg-3">
                      {brief.data.day} · {brief.data.model} · {ago(brief.data.createdAt)}
                    </div>
                  </>
                ) : (
                  <div className="text-[12.5px] text-fg-3">No brief yet today.</div>
                )}
                {/* one brief a day for every broker's clients: only the platform tenant regenerates it (the service enforces it) */}
                {canWrite && brief.data?.configured && isPlatform && (
                  <Button size="sm" variant="surface" className="mt-3" onClick={regenerate} disabled={briefBusy}>
                    <RefreshCw /> {briefBusy ? "Writing… (up to a minute)" : "Regenerate today's brief"}
                  </Button>
                )}
                {brief.data?.configured && sources.data && !isPlatform && <div className="mt-3 text-[11.5px] text-fg-3">Shared by every brand on the platform and managed by the platform.</div>}
              </div>
            </Card>
          </Reveal>
          <Reveal delay={0.2}>
            <Card>
              <CardHeader title="Sources" subtitle="Stories in the last 24 hours" icon={<Rss />} action={<Button size="sm" variant="surface" onClick={() => setSourcesOpen(true)}>Manage</Button>} />
              <div className="mt-4 space-y-2.5 px-4 pb-5 sm:px-6">
                {liveSources.map((x) => (
                  <div key={x.id}>
                    <div className="flex items-center justify-between text-[12.5px]">
                      <span className="flex items-center gap-2">
                        <span className={cn("size-1.5 rounded-full", x.lastError ? "bg-down" : "bg-up")} />
                        {x.name}
                      </span>
                      <span className="k-num text-fg-3">{x.last24h}</span>
                    </div>
                    <div className="mt-1 h-1.5 overflow-hidden rounded-full bg-surface-3">
                      <div className="h-full rounded-full bg-gold" style={{ width: `${(x.last24h / maxToday) * 100}%` }} />
                    </div>
                  </div>
                ))}
              </div>
            </Card>
          </Reveal>
          <Reveal delay={0.25}>
            <Card>
              <CardHeader title="Recent changes" subtitle="Pins, hides, retags and source changes" />
              <div className="mt-3 divide-y divide-line px-4 pb-4 sm:px-6">
                {(audit.data?.entries ?? []).slice(0, 8).map((e, i) => (
                  <div key={i} className="py-2 text-[12px]">
                    <span className="font-medium text-fg">{e.action.replace(".", " ")}</span> <span className="text-fg-3">{e.target}</span>
                    <div className="text-[11px] text-fg-3">
                      {e.staff} · {ago(e.at)}
                    </div>
                  </div>
                ))}
                {audit.data && audit.data.entries.length === 0 && <div className="py-4 text-center text-[12px] text-fg-3">No changes yet.</div>}
              </div>
            </Card>
          </Reveal>
        </div>
      </div>

      <TagEditor
        item={editing}
        onClose={() => setEditing(null)}
        onSaved={(n) => {
          list.setData((d) => (d ? { ...d, items: d.items.map((x) => (x.id === n.id ? n : x)) } : d));
          audit.reload();
        }}
      />
      <SourcesDialog open={sourcesOpen} onOpenChange={setSourcesOpen} canWrite={canWrite} onChanged={refreshAll} />
    </div>
  );
}
