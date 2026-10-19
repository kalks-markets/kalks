"use client";

import * as React from "react";
import Link from "next/link";
import { toast } from "sonner";
import { Bookmark, Clock3, ExternalLink, Share2, Sparkles, TrendingDown, TrendingUp, Minus, X } from "lucide-react";
import { Button, Card, CardHeader, Chip, Delta, Dialog, MarketSessions, PageHeader, Reveal, Segmented, Starfield, SymbolAvatar, WorldMap, cn, useQuotes, type MapPin } from "@/components/kit";
import { ASSET_CLASS_LABEL, INSTRUMENT_MAP, type AssetClass } from "@kalks/mock";
import { AI_BRIEF, NEWS_STORIES, type NewsStory } from "@kalks/mock/news-extra";
import { IS_DEMO } from "@kalks/mock/mode";
import { LiveNewsPage } from "@/components/news-live/news-page";
import { TERMINAL_URL } from "@/lib/live";

type Sentiment = NewsStory["sentiment"];
const SENT: Record<Sentiment, { tone: "up" | "down" | "neutral"; icon: React.ReactNode; label: string }> = {
  bullish: { tone: "up", icon: <TrendingUp className="size-3" />, label: "Bullish" },
  bearish: { tone: "down", icon: <TrendingDown className="size-3" />, label: "Bearish" },
  neutral: { tone: "neutral", icon: <Minus className="size-3" />, label: "Neutral" },
};
const COUNTRY: Record<string, string> = { us: "United States", de: "Germany", sa: "Saudi Arabia", jp: "Japan", gb: "United Kingdom", in: "India", sg: "Singapore", br: "Brazil", cn: "China" };

const ago = (m: number) => (m < 60 ? `${m}m ago` : `${Math.floor(m / 60)}h ${m % 60}m ago`);

function SymbolPill({ s }: { s: string }) {
  if (!INSTRUMENT_MAP[s])
    return <span className="rounded-full border border-line bg-surface-3 px-2 py-0.5 font-mono text-[10.5px] text-fg-2">{s}</span>;
  return (
    <Link target="_blank" rel="noopener" href={`${TERMINAL_URL}/?symbol=${s}`} onClick={(e) => e.stopPropagation()} className="flex items-center gap-1 rounded-full border border-line bg-surface-3 py-0.5 pl-0.5 pr-2 font-mono text-[10.5px] text-fg-2 transition-colors hover:border-ember/40 hover:text-ember">
      <SymbolAvatar symbol={s} size={14} />
      {s}
    </Link>
  );
}

function SentimentChip({ s, className }: { s: Sentiment; className?: string }) {
  return (
    <Chip size="sm" tone={SENT[s].tone} className={className}>
      {SENT[s].icon}
      {SENT[s].label}
    </Chip>
  );
}

/* ------------------------------------------------------------------ */

function DemoNewsPage() {
  const [cls, setCls] = React.useState<"all" | AssetClass>("all");
  const [sent, setSent] = React.useState<Sentiment[]>([]);
  const [country, setCountry] = React.useState<string | null>(null);
  const [open, setOpen] = React.useState<NewsStory | null>(null);
  const [saved, setSaved] = React.useState<string[]>([]);
  const watchQs = useQuotes(AI_BRIEF.watch);

  const featured = NEWS_STORIES[0]!;
  const list = NEWS_STORIES.slice(1).filter((n) => (cls === "all" || n.assetClass === cls) && (!sent.length || sent.includes(n.sentiment)) && (!country || n.country === country));

  const pins: MapPin[] = React.useMemo(() => {
    const m = new Map<string, number>();
    NEWS_STORIES.forEach((n) => m.set(n.country, (m.get(n.country) ?? 0) + 1));
    return [...m.entries()].map(([c, count]) => ({ country: c, count: count * 3 + (c === "us" ? 9 : 1), label: COUNTRY[c] ?? c.toUpperCase() }));
  }, []);
  const heat = { us: 0.7, gb: 0.4, de: -0.6, jp: -0.2, cn: 0.3, sa: -0.5, br: 0.5, in: 0.1 };

  const toggleSent = (s: Sentiment) => setSent((x) => (x.includes(s) ? x.filter((y) => y !== s) : [...x, s]));
  const toggleSave = (id: string) => {
    const on = saved.includes(id);
    toast.success(on ? "Removed from reading list" : "Saved to reading list");
    setSaved((x) => (on ? x.filter((y) => y !== id) : [...x, id]));
  };

  const counts = { bullish: NEWS_STORIES.filter((n) => n.sentiment === "bullish").length, bearish: NEWS_STORIES.filter((n) => n.sentiment === "bearish").length };

  return (
    <div className="pb-24">
      <PageHeader
        title="Market news"
        subtitle={`${NEWS_STORIES.length} stories today · curated from Reuters, Bloomberg, FT and more`}
        actions={
          <div className="flex items-center gap-2">
            <Chip tone="up">{counts.bullish} bullish</Chip>
            <Chip tone="down">{counts.bearish} bearish</Chip>
          </div>
        }
      />

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
        {/* Featured */}
        <Reveal className="xl:col-span-8">
          <button onClick={() => setOpen(featured)} className="k-card group relative block h-full min-h-[380px] w-full overflow-hidden text-left">
            {/* eslint-disable-next-line @next/next/no-img-element */}
            <img src={featured.image} alt="" className="absolute inset-0 size-full object-cover transition-transform duration-700 group-hover:scale-[1.03]" />
            <div className="absolute inset-0 bg-gradient-to-t from-bg via-bg/70 to-bg/5" />
            <div className="absolute inset-0 bg-gradient-to-r from-bg/70 to-transparent" />
            <div className="relative flex h-full flex-col justify-end p-6 sm:p-8">
              <div className="flex flex-wrap items-center gap-2 text-[12px] text-fg-2">
                <Chip tone="ember" dot>
                  Top story
                </Chip>
                <SentimentChip s={featured.sentiment} />
                <span>{featured.source}</span>·<span>{ago(featured.minutesAgo)}</span>·
                <span className="flex items-center gap-1">
                  <Clock3 className="size-3" /> {featured.readMin} min read
                </span>
              </div>
              <h2 className="mt-3 max-w-2xl text-[24px] font-medium leading-tight tracking-tight text-fg sm:text-[32px]">{featured.title}</h2>
              <p className="mt-3 max-w-2xl text-[14px] leading-relaxed text-fg-2">{featured.summary}</p>
              <div className="mt-4 flex flex-wrap items-center gap-2">
                {featured.symbols.map((s) => (
                  <SymbolPill key={s} s={s} />
                ))}
                <span className="ml-auto hidden text-[12.5px] font-medium text-ember group-hover:underline sm:inline">Read story →</span>
              </div>
            </div>
          </button>
        </Reveal>

        {/* AI summary */}
        <Reveal delay={0.08} className="xl:col-span-4">
          <Card hot className="relative h-full overflow-hidden">
            <Starfield density={40} />
            <div className="relative flex h-full flex-col p-6">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2.5">
                  <span className="k-ember-btn grid size-9 place-items-center rounded-full">
                    <Sparkles className="size-4" />
                  </span>
                  <div>
                    <div className="text-[15px] font-medium">AI summary of today</div>
                    <div className="text-[11.5px] text-fg-3">Updated {AI_BRIEF.updated}</div>
                  </div>
                </div>
                <Chip tone="up" dot>
                  {AI_BRIEF.mood}
                </Chip>
              </div>
              <ul className="mt-5 flex-1 space-y-3">
                {AI_BRIEF.points.map((p, i) => (
                  <li key={i} className="flex gap-3 text-[13px] leading-relaxed text-fg-2">
                    <span className={cn("mt-1.5 size-1.5 shrink-0 rounded-full", p.tone === "up" ? "bg-up" : p.tone === "down" ? "bg-down" : "bg-warn")} />
                    {p.text}
                  </li>
                ))}
              </ul>
              <div className="mt-4 grid grid-cols-2 gap-2">
                {AI_BRIEF.watch.map((s) => (
                  <Link key={s} target="_blank" rel="noopener" href={`${TERMINAL_URL}/?symbol=${s}`} className="flex items-center gap-2 rounded-xl border border-line bg-surface/60 px-2.5 py-2 transition-colors hover:border-ember/40">
                    <SymbolAvatar symbol={s} size={18} />
                    <span className="text-[12px] font-medium">{s}</span>
                    <Delta value={watchQs[s]!.change} className="ml-auto text-[11px]" />
                  </Link>
                ))}
              </div>
              <Link href="/academy/coach?q=Summarise%20today%27s%20markets" className="mt-4">
                <Button variant="surface" className="w-full">
                  <Sparkles /> Ask AI about today's markets
                </Button>
              </Link>
            </div>
          </Card>
        </Reveal>
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
          <Reveal delay={0.1} className="xl:col-span-8">
            <Card className="h-full">
              <CardHeader title="News around the world" subtitle="Click a pin to filter by country" action={<Chip tone="ember" dot>Live</Chip>} />
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
                    <span className={`fi fis fi-${p.country} size-3 rounded-full`} />
                    {p.label}
                  </button>
                ))}
              </div>
            </Card>
          </Reveal>
          <Reveal delay={0.15} className="xl:col-span-4">
            <Card className="h-full">
              <CardHeader title="Most mentioned" subtitle="Symbols in today's headlines" />
              <div className="space-y-1.5 px-4 pb-5 pt-3 sm:px-6">
                {["XAUUSD", "EURUSD", "NAS100", "BTCUSD", "USOIL"].map((s, i) => (
                  <Link key={s} target="_blank" rel="noopener" href={`${TERMINAL_URL}/?symbol=${s}`} className="k-row flex items-center gap-3 px-3.5 py-2.5 transition-colors hover:bg-surface-3/60">
                    <span className="k-num w-4 text-[12px] text-fg-3">{i + 1}</span>
                    <SymbolAvatar symbol={s} size={22} />
                    <span className="flex-1 text-[13px] font-medium">{s}</span>
                    <span className="k-num text-[11.5px] text-fg-3">{[14, 11, 9, 7, 6][i]} stories</span>
                  </Link>
                ))}
              </div>
              <div className="px-6 pb-6 pt-1">
                <MarketSessions />
              </div>
            </Card>
          </Reveal>
      </div>

      {/* Filters */}
      <Reveal delay={0.05} className="mt-6">
        <div className="flex flex-wrap items-center gap-2">
          <div className="max-w-full overflow-x-auto">
            <Segmented
              value={cls}
              onChange={setCls}
              options={[{ value: "all", label: "All" }, ...(["forex", "metals", "indices", "energies", "crypto", "stocks"] as AssetClass[]).map((c) => ({ value: c, label: ASSET_CLASS_LABEL[c] }))]}
            />
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
                  {SENT[s].label}
                </button>
              );
            })}
          </div>
          {country && (
            <button onClick={() => setCountry(null)} className="flex h-8 items-center gap-1.5 rounded-full border border-ember/40 bg-ember-soft px-3 text-[12px] font-medium text-ember">
              <span className={`fi fis fi-${country} size-3.5 rounded-full`} />
              {COUNTRY[country]}
              <X className="size-3.5" />
            </button>
          )}
          <span className="ml-auto text-[12.5px] text-fg-3">{list.length} stories</span>
        </div>
      </Reveal>

      <div className="mt-4">
        <div>
          {list.length === 0 ? (
            <Card className="py-16 text-center text-[13.5px] text-fg-3">No stories match these filters.</Card>
          ) : (
            <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
              {list.map((n, i) => (
                <Reveal key={n.id} delay={Math.min(i * 0.04, 0.3)}>
                  <div role="button" tabIndex={0} onClick={() => setOpen(n)} onKeyDown={(e) => e.key === "Enter" && setOpen(n)} className="k-card group flex h-full cursor-pointer flex-col overflow-hidden transition-colors hover:border-[var(--k-border-top)]">
                    <div className="relative h-40 overflow-hidden">
                      {/* eslint-disable-next-line @next/next/no-img-element */}
                      <img src={n.image} alt="" className="size-full object-cover transition-transform duration-500 group-hover:scale-105" />
                      <div className="absolute inset-0 bg-gradient-to-t from-surface via-transparent to-transparent" />
                      <div className="absolute left-3 top-3 flex gap-1.5">
                        <SentimentChip s={n.sentiment} className="bg-bg/75 backdrop-blur" />
                        <Chip size="sm" className="bg-bg/70 backdrop-blur">
                          {ASSET_CLASS_LABEL[n.assetClass]}
                        </Chip>
                      </div>
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          toggleSave(n.id);
                        }}
                        aria-label="Save"
                        className={cn("absolute right-3 top-3 grid size-8 place-items-center rounded-full border border-line bg-bg/60 backdrop-blur transition-colors hover:text-fg", saved.includes(n.id) ? "text-ember" : "text-fg-2")}
                      >
                        <Bookmark className={cn("size-3.5", saved.includes(n.id) && "fill-current")} />
                      </button>
                    </div>
                    <div className="flex flex-1 flex-col px-5 pb-5 pt-3">
                      <div className="flex items-center gap-2 text-[11.5px] text-fg-3">
                        <span className={`fi fis fi-${n.country} size-3.5 rounded-full`} />
                        <span className="text-fg-2">{n.source}</span>·<span>{ago(n.minutesAgo)}</span>
                      </div>
                      <h3 className="mt-2 line-clamp-2 text-[15.5px] font-medium leading-snug">{n.title}</h3>
                      <p className="mt-1.5 line-clamp-2 text-[12.5px] leading-relaxed text-fg-3">{n.summary}</p>
                      <div className="mt-auto flex flex-wrap gap-1.5 pt-3">
                        {n.symbols.map((s) => (
                          <SymbolPill key={s} s={s} />
                        ))}
                      </div>
                    </div>
                  </div>
                </Reveal>
              ))}
            </div>
          )}
        </div>
      </div>

      <Dialog
        open={!!open}
        onOpenChange={(o) => !o && setOpen(null)}
        width={720}
        title={open?.source ?? ""}
        description={open ? `${ago(open.minutesAgo)} · ${open.readMin} min read` : undefined}
        footer={
          open ? (
            <>
              <Button variant="ghost" onClick={() => toggleSave(open.id)}>
                <Bookmark /> {saved.includes(open.id) ? "Saved" : "Save"}
              </Button>
              <Button
                variant="surface"
                onClick={() => {
                  navigator.clipboard?.writeText(`https://app.kalkstrade.com/news/${open.id}`).catch(() => {});
                  toast.success("Link copied");
                }}
              >
                <Share2 /> Share
              </Button>
              {INSTRUMENT_MAP[open.symbols[0]!] && (
                <Link target="_blank" rel="noopener" href={`${TERMINAL_URL}/?symbol=${open.symbols[0]}`}>
                  <Button variant="ember">Trade {open.symbols[0]}</Button>
                </Link>
              )}
            </>
          ) : null
        }
      >
        {open && (
          <div>
            {/* eslint-disable-next-line @next/next/no-img-element */}
            <img src={open.image} alt="" className="h-56 w-full rounded-2xl object-cover" />
            <div className="mt-4 flex flex-wrap items-center gap-2">
              <SentimentChip s={open.sentiment} />
              <Chip size="sm">{ASSET_CLASS_LABEL[open.assetClass]}</Chip>
              <span className="flex items-center gap-1.5 text-[12px] text-fg-3">
                <span className={`fi fis fi-${open.country} size-3.5 rounded-full`} />
                {COUNTRY[open.country]}
              </span>
            </div>
            <h2 className="mt-3 text-[22px] font-medium leading-tight tracking-tight">{open.title}</h2>
            <p className="mt-3 text-[14px] leading-relaxed text-fg-2">{open.summary}</p>
            <div className="k-row mt-5 flex gap-3 p-4">
              <Sparkles className="mt-0.5 size-4 shrink-0 text-ember" />
              <p className="text-[13px] leading-relaxed text-fg-2">
                <span className="font-medium text-fg">What it means for you: </span>
                {open.sentiment === "bullish" ? "Momentum favours buyers in the short term." : open.sentiment === "bearish" ? "Sellers are in control; watch support levels." : "Expect range trading until the next catalyst."} Keep position sizes in check around scheduled data in the calendar.
              </p>
            </div>
            <div className="mt-4 flex flex-wrap items-center gap-2">
              {open.symbols.map((s) => (
                <SymbolPill key={s} s={s} />
              ))}
              <button onClick={() => toast.info(`Opening original story on ${open.source}`)} className="ml-auto flex items-center gap-1 text-[12.5px] text-fg-3 hover:text-fg">
                Source <ExternalLink className="size-3.5" />
              </button>
            </div>
          </div>
        )}
      </Dialog>
    </div>
  );
}

export default function NewsPage() {
  // live builds: real headlines / calendar (services/news); demo builds keep the showcase above
  return IS_DEMO ? <DemoNewsPage /> : <LiveNewsPage />;
}
