"use client";

import * as React from "react";
import Link from "next/link";
import { toast } from "sonner";
import { Bookmark, ExternalLink, Link2, Pin, RefreshCw, Sparkles, X } from "lucide-react";
import { Button, Card, CardHeader, Chip, Delta, Dialog, MarketSessions, PageHeader, Reveal, Segmented, Skeleton, SymbolAvatar, WorldMap, cn, useQuotes, type MapPin } from "@/components/kit";
import { INSTRUMENT_MAP } from "@kalks/mock";
import { useNewsApi, type Brief, type Feed, type NewsItem, type NewsMap, type Sentiment } from "./api";
import { COUNTRY_NAME, Flag, SENT, SentimentChip, SymbolPill, ago, categoryLabel, countryName, coverFor, heatOf, useNow } from "./shared";
import { tr, useFormat, useT } from "@kalks/i18n/react";
import { TERMINAL_URL } from "@/lib/live";
import { PageHero } from "@/components/page-hero";

const CATS = ["all", "macro", "forex", "metals", "indices", "energies", "crypto", "stocks"] as const;
type Cat = (typeof CATS)[number];
const SAVED_KEY = "kalks.news.saved";

function useSaved() {
  const [saved, setSaved] = React.useState<number[]>([]);
  React.useEffect(() => {
    try {
      const v = JSON.parse(localStorage.getItem(SAVED_KEY) ?? "[]");
      if (Array.isArray(v)) setSaved(v.filter((x) => typeof x === "number").slice(0, 200));
    } catch {}
  }, []);
  const toggle = (id: number) =>
    setSaved((x) => {
      const on = x.includes(id);
      const next = on ? x.filter((y) => y !== id) : [id, ...x].slice(0, 200);
      try {
        localStorage.setItem(SAVED_KEY, JSON.stringify(next));
      } catch {}
      toast.success(on ? tr("news.saved.removed") : tr("news.saved.added"));
      return next;
    });
  return { saved, toggle };
}

const MOOD: Record<string, { tone: "up" | "down" | "warn" | "neutral"; labelKey: "news.mood.riskOn" | "news.mood.riskOff" | "news.mood.mixed" | "news.mood.cautious" }> = {
  "risk-on": { tone: "up", labelKey: "news.mood.riskOn" },
  "risk-off": { tone: "down", labelKey: "news.mood.riskOff" },
  mixed: { tone: "neutral", labelKey: "news.mood.mixed" },
  cautious: { tone: "warn", labelKey: "news.mood.cautious" },
};

function BriefCard() {
  const b = useNewsApi<Brief>("brief", 10 * 60_000);
  const brief = b.data?.brief;
  const watch = (brief?.watch ?? []).filter((s) => INSTRUMENT_MAP[s]);
  const qs = useQuotes(watch);
  const t = useT();
  const f = useFormat();
  return (
    <Card hot className="relative h-full overflow-hidden">
      <div className="relative flex h-full flex-col p-6">
        <div className="flex items-center justify-between gap-3">
          <div className="flex items-center gap-2.5">
            <span className="k-ember-btn grid size-9 place-items-center rounded-full">
              <Sparkles className="size-4" />
            </span>
            <div>
              <div className="text-[15px] font-medium">{t("news.brief.title")}</div>
              <div className="text-[11.5px] text-fg-3">
                {b.data?.createdAt ? t("news.brief.written", { time: f.dateTime(b.data.createdAt, { weekday: "short", hour: "2-digit", minute: "2-digit", timeZone: undefined }) }) : t("news.brief.writtenDaily")}
              </div>
            </div>
          </div>
          {brief && <Chip tone={MOOD[brief.mood]?.tone ?? "neutral"}>{MOOD[brief.mood] ? t(MOOD[brief.mood]!.labelKey) : brief.mood}</Chip>}
        </div>
        {b.loading ? (
          <div className="mt-5 space-y-3">
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-5/6" />
            <Skeleton className="h-4 w-4/6" />
          </div>
        ) : !brief ? (
          <p className="mt-5 flex-1 text-[13px] leading-relaxed text-fg-3">
            {b.data?.configured === false ? t("news.brief.off") : t("news.brief.pending")}
          </p>
        ) : (
          <>
            <p className="mt-4 text-[14px] font-medium leading-snug text-fg">{brief.headline}</p>
            <ul className="mt-3 flex-1 space-y-2.5">
              {brief.points.map((p, i) => (
                <li key={i} className="flex gap-3 text-[13px] leading-relaxed text-fg-2">
                  <span className={cn("mt-1.5 size-1.5 shrink-0 rounded-full", p.tone === "up" ? "bg-up" : p.tone === "down" ? "bg-down" : "bg-warn")} />
                  {p.text}
                </li>
              ))}
            </ul>
            {brief.calendarNote && <p className="mt-3 rounded-xl border border-line bg-surface/60 px-3 py-2 text-[12px] leading-relaxed text-fg-2">{brief.calendarNote}</p>}
            {watch.length > 0 && (
              <div className="mt-4 grid grid-cols-2 gap-2">
                {watch.map((s) => (
                  <Link key={s} target="_blank" rel="noopener" href={`${TERMINAL_URL}/?symbol=${s}`} className="flex items-center gap-2 rounded-xl border border-line bg-surface/60 px-2.5 py-2 transition-colors hover:border-ember/40">
                    <SymbolAvatar symbol={s} size={18} />
                    <span className="text-[12px] font-medium">{s}</span>
                    {qs[s] && <Delta value={qs[s]!.change} className="ms-auto text-[11px]" />}
                  </Link>
                ))}
              </div>
            )}
            <p className="mt-3 text-[10.5px] leading-snug text-fg-3">{t("news.brief.disclaimer")}</p>
          </>
        )}
      </div>
    </Card>
  );
}

function FeaturedCard({ n, onOpen }: { n: NewsItem; onOpen: () => void }) {
  const now = useNow(60_000) ?? Date.now();
  const t = useT();
  return (
    <button onClick={onOpen} className="k-card group relative block h-full min-h-[380px] w-full overflow-hidden text-start">
      {/* eslint-disable-next-line @next/next/no-img-element */}
      <img src={coverFor(n)} alt="" className="absolute inset-0 size-full object-cover" />
      <div className="absolute inset-0 bg-gradient-to-t from-bg via-bg/75 to-bg/10" />
      <div className="absolute inset-0 bg-gradient-to-r from-bg/70 to-transparent rtl:bg-gradient-to-l" />
      <div className="relative flex h-full flex-col justify-end p-6 sm:p-8">
        <div className="flex flex-wrap items-center gap-2 text-[12px] text-fg-2">
          <Chip tone="ember">{n.pinned ? t("news.pinned") : t("news.featured.topStory")}</Chip>
          <SentimentChip s={n.sentiment} />
          <span>{n.source.name}</span>·<span suppressHydrationWarning>{ago(n.publishedAt, now)}</span>
        </div>
        <h2 className="mt-3 max-w-2xl text-[24px] font-medium leading-tight tracking-tight text-fg sm:text-[30px]">{n.title}</h2>
        {n.summary && <p className="mt-3 max-w-2xl text-[14px] leading-relaxed text-fg-2">{n.summary}</p>}
        <div className="mt-4 flex flex-wrap items-center gap-2">
          {n.symbols.map((s) => (
            <SymbolPill key={s} s={s} />
          ))}
          <span className="ms-auto hidden text-[12.5px] font-medium text-ember group-hover:underline sm:inline">{t("news.featured.details")}</span>
        </div>
      </div>
    </button>
  );
}

function StoryCard({ n, saved, onSave, onOpen, now }: { n: NewsItem; saved: boolean; onSave: () => void; onOpen: () => void; now: number }) {
  const t = useT();
  return (
    <div role="button" tabIndex={0} onClick={onOpen} onKeyDown={(e) => e.key === "Enter" && onOpen()} className="k-card group flex h-full cursor-pointer flex-col overflow-hidden transition-colors hover:border-[var(--k-border-top)]" data-testid="news-card">
      <div className="relative h-36 overflow-hidden">
        {/* eslint-disable-next-line @next/next/no-img-element */}
        <img src={coverFor(n)} alt="" className="size-full object-cover opacity-80" />
        <div className="absolute inset-0 bg-gradient-to-t from-surface via-transparent to-transparent" />
        <div className="absolute start-3 top-3 flex gap-1.5">
          <SentimentChip s={n.sentiment} className="bg-bg/75" />
          <Chip size="sm" className="bg-bg/70">
            {categoryLabel(n.category, t)}
          </Chip>
          {n.pinned && (
            <Chip size="sm" tone="ember" className="bg-bg/80">
              <Pin className="size-3" /> {t("news.pinned")}
            </Chip>
          )}
        </div>
        <button
          onClick={(e) => {
            e.stopPropagation();
            onSave();
          }}
          aria-label={saved ? t("news.saved.removeAria") : t("news.saved.addAria")}
          className={cn("absolute end-3 top-3 grid size-8 place-items-center rounded-full border border-line bg-bg/70 transition-colors hover:text-fg", saved ? "text-ember" : "text-fg-2")}
        >
          <Bookmark className={cn("size-3.5", saved && "fill-current")} />
        </button>
      </div>
      <div className="flex flex-1 flex-col px-5 pb-5 pt-3">
        <div className="flex items-center gap-2 text-[11.5px] text-fg-3">
          {n.countries.slice(0, 3).map((c) => (
            <Flag key={c} country={c} />
          ))}
          <span className="truncate text-fg-2">{n.source.name}</span>·<span className="shrink-0" suppressHydrationWarning>{ago(n.publishedAt, now)}</span>
        </div>
        <h3 className="mt-2 line-clamp-2 text-[15px] font-medium leading-snug">{n.title}</h3>
        {n.summary && <p className="mt-1.5 line-clamp-2 text-[12.5px] leading-relaxed text-fg-3">{n.summary}</p>}
        <div className="mt-auto flex flex-wrap gap-1.5 pt-3">
          {n.symbols.map((s) => (
            <SymbolPill key={s} s={s} />
          ))}
        </div>
      </div>
    </div>
  );
}

export function StoryDialog({ open, onClose, saved, onSave }: { open: NewsItem | null; onClose: () => void; saved?: boolean; onSave?: () => void }) {
  const now = useNow(60_000) ?? Date.now();
  const tradable = open?.symbols.find((s) => INSTRUMENT_MAP[s]);
  const t = useT();
  const f = useFormat();
  return (
    <Dialog
      open={!!open}
      onOpenChange={(o) => !o && onClose()}
      width={680}
      title={open?.source.name ?? ""}
      description={open ? `${ago(open.publishedAt, now)} · ${f.dateTime(open.publishedAt, { dateStyle: "medium", timeStyle: "short", timeZone: undefined })}` : undefined}
      footer={
        open ? (
          <>
            {onSave && (
              <Button variant="ghost" onClick={onSave}>
                <Bookmark /> {saved ? t("news.story.saved") : t("common.save")}
              </Button>
            )}
            {open.link && (
              <Button
                variant="surface"
                onClick={() => {
                  navigator.clipboard?.writeText(open.link).then(() => toast.success(t("news.story.linkCopied")), () => toast.error(t("news.story.copyFailed")));
                }}
              >
                <Link2 /> {t("news.story.copyLink")}
              </Button>
            )}
            {tradable && (
              <Link target="_blank" rel="noopener" href={`${TERMINAL_URL}/?symbol=${tradable}`}>
                <Button variant="ember">{t("news.story.trade", { symbol: tradable })}</Button>
              </Link>
            )}
          </>
        ) : null
      }
    >
      {open && (
        <div>
          <div className="flex flex-wrap items-center gap-2">
            <SentimentChip s={open.sentiment} />
            <Chip size="sm">{categoryLabel(open.category, t)}</Chip>
            {open.countries.map((c) => (
              <span key={c} className="flex items-center gap-1.5 text-[12px] text-fg-3">
                <Flag country={c} />
                {countryName(c, t)}
              </span>
            ))}
          </div>
          <h2 className="mt-3 text-[21px] font-medium leading-tight tracking-tight">{open.title}</h2>
          {open.summary && <p className="mt-3 text-[14px] leading-relaxed text-fg-2">{open.summary}</p>}
          {open.symbols.length > 0 && (
            <div className="mt-5">
              <div className="k-label mb-2">{t("news.story.instruments")}</div>
              <div className="flex flex-wrap gap-2">
                {open.symbols.map((s) => (
                  <SymbolPill key={s} s={s} />
                ))}
              </div>
            </div>
          )}
          {open.link && (
            <a href={open.link} target="_blank" rel="noopener noreferrer nofollow" className="k-row mt-5 flex items-center gap-3 px-4 py-3 text-[13px] transition-colors hover:border-ember/40">
              <ExternalLink className="size-4 shrink-0 text-ember" />
              <span className="min-w-0 flex-1">
                <span className="font-medium text-fg">{t("news.story.readFull", { source: open.source.name })}</span>
                <span dir="ltr" className="block truncate text-start text-[11.5px] text-fg-3">{open.link}</span>
              </span>
            </a>
          )}
          <p className="mt-3 text-[11px] leading-snug text-fg-3">{t("news.story.attribution", { source: open.source.name })}</p>
        </div>
      )}
    </Dialog>
  );
}

export function useMapPins(map: NewsMap | null) {
  const t = useT();
  return React.useMemo(() => {
    const countries = map?.countries ?? [];
    const pins: MapPin[] = countries.map((c) => ({ country: c.country, count: c.count, label: t.dyn(`news.country.${c.country}`, c.name || COUNTRY_NAME[c.country] || c.country.toUpperCase()), tone: c.sentiment > 0.2 ? "up" : c.sentiment < -0.2 ? "down" : "ember" }));
    return { pins, heat: heatOf(countries) };
  }, [map, t]);
}

export function LiveNewsPage() {
  const [cls, setCls] = React.useState<Cat>("all");
  const [sent, setSent] = React.useState<Sentiment[]>([]);
  const [country, setCountry] = React.useState<string | null>(null);
  const [open, setOpen] = React.useState<NewsItem | null>(null);
  const [more, setMore] = React.useState<NewsItem[]>([]);
  const [next, setNext] = React.useState<string | null>(null);
  const [loadingMore, setLoadingMore] = React.useState(false);
  const { saved, toggle } = useSaved();
  const now = useNow(60_000) ?? Date.now();
  const t = useT();

  const qs = new URLSearchParams({ limit: "36" });
  if (cls !== "all") qs.set("category", cls);
  if (country) qs.set("country", country);
  if (sent.length === 1) qs.set("sentiment", sent[0]!);
  const feed = useNewsApi<Feed>(`feed?${qs}`, 2 * 60_000);
  const map = useNewsApi<NewsMap>("map?hours=48", 5 * 60_000);
  const { pins, heat } = useMapPins(map.data);
  React.useEffect(() => {
    setMore([]);
    setNext(feed.data?.next ?? null);
  }, [feed.data]);

  const loadMore = async () => {
    if (!next) return;
    setLoadingMore(true);
    try {
      const q2 = new URLSearchParams(qs);
      q2.set("before", next);
      const r = await (await fetch(`/api/news/feed?${q2}`, { cache: "no-store" })).json();
      setMore((m) => [...m, ...((r.items as NewsItem[]) ?? [])]);
      setNext(r.next ?? null);
    } catch {
      toast.error(t("news.list.loadMoreError"));
    } finally {
      setLoadingMore(false);
    }
  };

  const all = [...(feed.data?.items ?? []), ...more].filter((n) => !sent.length || sent.includes(n.sentiment));
  const pinned = feed.data?.pinned ?? [];
  const featured = pinned[0] ?? all.slice().sort((a, b) => b.importance - a.importance || +new Date(b.publishedAt) - +new Date(a.publishedAt))[0];
  const list = [...pinned.filter((p) => p.id !== featured?.id), ...all.filter((n) => n.id !== featured?.id)];
  const toggleSent = (s: Sentiment) => setSent((x) => (x.includes(s) ? x.filter((y) => y !== s) : [...x, s]));
  const total = map.data?.total ?? null;
  const pos = map.data?.countries.reduce((s, c) => s + c.bullish, 0) ?? 0;
  const neg = map.data?.countries.reduce((s, c) => s + c.bearish, 0) ?? 0;

  return (
    <div className="pb-24">
      <PageHero page="news"
        title={t("news.page.title")}
        lead={total !== null ? t("news.page.subtitleCount", { count: total }) : t("news.page.subtitle")}
        actions={
          <div className="flex items-center gap-2">
            <Chip tone="up">{t("news.page.positive", { count: pos })}</Chip>
            <Chip tone="down">{t("news.page.negative", { count: neg })}</Chip>
            <Button variant="surface" size="sm" onClick={() => (feed.reload(), map.reload())} aria-label={t("common.refresh")}>
              <RefreshCw />
            </Button>
          </div>
        }
      />

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal className="xl:col-span-8">
          {feed.loading ? (
            <Skeleton className="h-[380px] w-full rounded-[22px]" />
          ) : featured ? (
            <FeaturedCard n={featured} onOpen={() => setOpen(featured)} />
          ) : (
            <Card className="grid h-full min-h-[380px] place-items-center px-6 text-center text-[13.5px] text-fg-3">{feed.error ? feed.error.message : t("news.page.empty")}</Card>
          )}
        </Reveal>
        <Reveal delay={0.08} className="xl:col-span-4">
          <BriefCard />
        </Reveal>
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal delay={0.1} className="xl:col-span-8">
          <Card className="h-full">
            <CardHeader title={t("news.world.title")} subtitle={t("news.world.subtitle")} action={<Chip tone="ember">{t("news.world.countries", { count: pins.length })}</Chip>} />
            <div className="px-3 pt-2 sm:px-8">
              <WorldMap pins={pins} heat={heat} onPin={(p) => setCountry((c) => (c === p.country ? null : p.country))} />
            </div>
            <div className="flex flex-wrap gap-1.5 px-6 pb-6">
              {pins.map((p) => (
                <button
                  key={p.country}
                  onClick={() => setCountry((c) => (c === p.country ? null : p.country))}
                  className={cn("flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-[11px] transition-colors", country === p.country ? "border-ember/40 bg-ember-soft text-ember" : "border-line bg-surface-2 text-fg-2 hover:text-fg")}
                >
                  <Flag country={p.country} className="size-3" />
                  {p.label}
                  <span className="k-num text-fg-3">{p.count}</span>
                </button>
              ))}
            </div>
          </Card>
        </Reveal>
        <Reveal delay={0.15} className="xl:col-span-4">
          <Card className="h-full">
            <CardHeader title={t("news.mentions.title")} subtitle={t("news.mentions.subtitle")} />
            <div className="space-y-1.5 px-4 pb-5 pt-3 sm:px-6">
              {(map.data?.mentions ?? []).slice(0, 5).map((m, i) => (
                <Link key={m.symbol} target="_blank" rel="noopener" href={`${TERMINAL_URL}/?symbol=${m.symbol}`} className="k-row flex items-center gap-3 px-3.5 py-2.5 transition-colors hover:bg-surface-3/60">
                  <span className="k-num w-4 text-[12px] text-fg-3">{i + 1}</span>
                  <SymbolAvatar symbol={m.symbol} size={22} />
                  <span className="flex-1 text-[13px] font-medium">{m.symbol}</span>
                  <span className="k-num text-[11.5px] text-fg-3">
                    {t("news.stories", { count: m.count })}
                  </span>
                </Link>
              ))}
              {map.data && map.data.mentions.length === 0 && <div className="py-6 text-center text-[12.5px] text-fg-3">{t("news.mentions.empty")}</div>}
            </div>
            <div className="px-6 pb-6 pt-1">
              <MarketSessions />
            </div>
          </Card>
        </Reveal>
      </div>

      <Reveal delay={0.05} className="mt-6">
        <div className="flex flex-wrap items-center gap-2">
          <div className="max-w-full overflow-x-auto">
            <Segmented value={cls} onChange={setCls} options={CATS.map((c) => ({ value: c, label: c === "all" ? t("common.all") : c === "macro" ? t("news.category.macroShort") : categoryLabel(c, t) }))} />
          </div>
          <div className="flex items-center gap-1.5">
            {(Object.keys(SENT) as Sentiment[]).map((s) => {
              const on = sent.includes(s);
              return (
                <button
                  key={s}
                  onClick={() => toggleSent(s)}
                  className={cn(
                    "flex h-8 items-center gap-1.5 rounded-full border px-3 text-[12px] font-medium transition-colors [&_svg]:size-3.5",
                    on ? (s === "bullish" ? "border-up/40 bg-up-soft text-up" : s === "bearish" ? "border-down/40 bg-down-soft text-down" : "border-fg-3/40 bg-surface-3 text-fg") : "border-line bg-surface-2 text-fg-3 hover:text-fg-2",
                  )}
                >
                  {SENT[s].icon}
                  {t(SENT[s].labelKey)}
                </button>
              );
            })}
          </div>
          {country && (
            <button onClick={() => setCountry(null)} className="flex h-8 items-center gap-1.5 rounded-full border border-ember/40 bg-ember-soft px-3 text-[12px] font-medium text-ember">
              <Flag country={country} />
              {countryName(country, t)}
              <X className="size-3.5" />
            </button>
          )}
          <span className="ms-auto text-[12.5px] text-fg-3">{t("news.stories", { count: list.length })}</span>
        </div>
      </Reveal>

      <div className="mt-4">
        {feed.loading ? (
          <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
            {Array.from({ length: 6 }).map((_, i) => (
              <Skeleton key={i} className="h-72 rounded-[22px]" />
            ))}
          </div>
        ) : list.length === 0 ? (
          <Card className="py-16 text-center text-[13.5px] text-fg-3">{feed.error ? feed.error.message : t("news.list.empty")}</Card>
        ) : (
          <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
            {list.map((n) => (
              <StoryCard key={n.id} n={n} now={now} saved={saved.includes(n.id)} onSave={() => toggle(n.id)} onOpen={() => setOpen(n)} />
            ))}
          </div>
        )}
        {next && (
          <div className="mt-6 flex justify-center">
            <Button variant="surface" onClick={loadMore} disabled={loadingMore}>
              {loadingMore ? t("common.loading") : t("news.list.loadMore")}
            </Button>
          </div>
        )}
      </div>

      <StoryDialog open={open} onClose={() => setOpen(null)} saved={open ? saved.includes(open.id) : false} onSave={open ? () => toggle(open.id) : undefined} />
    </div>
  );
}
