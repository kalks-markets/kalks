"use client";

import * as React from "react";
import Link from "next/link";
import { Check, ChevronDown, Columns3, Crown, Landmark, LineChart, Repeat, Search, ShieldCheck, SlidersHorizontal, Trophy, Users, Wallet, X as XIcon } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, CardHeader, Chip, EmptyState, Menu, Popover, Segmented, Skeleton, Toggle, Tooltip, cn } from "@/components/kit";
import { useFormat, useT } from "@kalks/i18n/react";
import { compactUsd, pct, useSocial, type Leaderboard, type MasterView } from "./api";
import { SocialError } from "./bits";
import { COMPARE_MAX, CompareDialog } from "./compare";
import { FollowDialog } from "./follow-dialog";
import { InvestDialog } from "./invest-dialog";
import { PageHero } from "@/components/page-hero";
import { VcBadge, VisitingCard } from "@/components/visiting-card";

type Period = "1m" | "3m" | "1y" | "all";
type SortKey = "return" | "dd" | "aum" | "followers" | "age";
type ProgramF = "all" | "copy" | "pamm";
type RiskF = "all" | "low" | "med" | "high";

// Label keys, translated at render
const PERIOD_LABEL = { "1m": "social.lb.period.1m", "3m": "social.lb.period.3m", "1y": "social.lb.period.1y", all: "common.all" } as const;
const retOf = (m: MasterView, p: Period) => (p === "1m" ? m.stats.return1m : p === "3m" ? m.stats.return3m : p === "1y" ? m.stats.return1y : m.stats.returnAll);
const TRACK = [
  { v: 0, label: "social.lb.track.any" },
  { v: 30, label: "social.lb.track.30" },
  { v: 90, label: "social.lb.track.90" },
  { v: 180, label: "social.lb.track.180" },
  { v: 365, label: "social.lb.track.365" },
] as const;
// A10 "More filters": 0 = any
const DD_STEPS = [0, 10, 20, 30] as const;
const FEE_STEPS = [0, 10, 20, 30] as const;
const FOLLOWER_STEPS = [0, 10, 50, 100] as const;

function FilterRow({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <div className="mb-1.5 text-[12px] font-medium text-fg-2">{label}</div>
      {children}
    </div>
  );
}

export function LiveDiscoverPage() {
  const t = useT();
  const f = useFormat();
  const [period, setPeriod] = React.useState<Period>("3m");
  const [sort, setSort] = React.useState<SortKey>("return");
  const [program, setProgram] = React.useState<ProgramF>("all");
  const [risk, setRisk] = React.useState<RiskF>("all");
  const [track, setTrack] = React.useState(0);
  const [maxDd, setMaxDd] = React.useState(0);
  const [maxFee, setMaxFee] = React.useState(0);
  const [minFollowers, setMinFollowers] = React.useState(0);
  const [openOnly, setOpenOnly] = React.useState(false);
  const [copyM, setCopyM] = React.useState<MasterView | null>(null);
  const [investFund, setInvestFund] = React.useState<number | null>(null);
  // A10 compare: up to 3 masters picked on the board (kept as rows so a filter change doesn't drop them)
  const [picked, setPicked] = React.useState<MasterView[]>([]);
  const [comparing, setComparing] = React.useState(false);

  const qs = new URLSearchParams({ period, program, sort, risk });
  if (track) qs.set("minDays", String(track));
  if (maxDd) qs.set("maxDd", String(maxDd));
  if (maxFee) qs.set("maxFee", String(maxFee));
  if (minFollowers) qs.set("minFollowers", String(minFollowers));
  if (openOnly) qs.set("openOnly", "true");
  const { data, error, loading, reload } = useSocial<Leaderboard>(`leaderboard?${qs}`, 30000);
  const rows = data?.items ?? [];
  const totals = data?.totals;
  const more = (maxDd ? 1 : 0) + (maxFee ? 1 : 0) + (minFollowers ? 1 : 0) + (openOnly ? 1 : 0);
  const filtered = program !== "all" || risk !== "all" || track > 0 || more > 0;
  const clearMore = () => {
    setMaxDd(0);
    setMaxFee(0);
    setMinFollowers(0);
    setOpenOnly(false);
  };
  const isPicked = (id: number) => picked.some((x) => x.id === id);
  const togglePick = (m: MasterView) => {
    if (isPicked(m.id)) return setPicked((p) => p.filter((x) => x.id !== m.id));
    if (picked.length >= COMPARE_MAX) return toast.error(t("social.compare.max", { n: COMPARE_MAX }));
    setPicked((p) => [...p, m]);
  };

  // the board as visiting cards (founder 2026-10-10), searchable, 12 at a time
  const [q, setQ] = React.useState("");
  const [shown, setShown] = React.useState(12);
  const needle = q.trim().toLowerCase();
  const list = needle ? rows.filter((m) => `${m.nickname} ${m.strategy}`.toLowerCase().includes(needle)) : rows;
  const programLabel = (m: MasterView) => (m.program === "pamm" ? "PAMM" : m.program === "both" && m.fund ? `${t("social.program.copy")} · PAMM` : t("social.program.copy"));

  const masterCard = (m: MasterView, i: number) => {
    const r = retOf(m, period);
    const on = isPicked(m.id);
    const ivory = m.program === "pamm";
    return (
      <div key={m.id} className="flex flex-col" data-testid="master-card">
        <div className="relative">
          <Link href={`/social/masters/${m.id}`} className="block transition-transform hover:-translate-y-0.5">
            <VisitingCard
              finish={ivory ? "ivory" : "black"}
              kicker={`#${i + 1} · ${programLabel(m)}`}
              name={m.nickname}
              title={m.strategy}
              badges={
                (m.house || (m.program !== "pamm" && m.acceptingNew === false)) && (
                  <>
                    {m.house && <VcBadge finish={ivory ? "ivory" : "black"}>{t("social.house.badge")}</VcBadge>}
                    {m.program !== "pamm" && m.acceptingNew === false && <VcBadge finish={ivory ? "ivory" : "black"}>{t("social.lb.closedChip")}</VcBadge>}
                  </>
                )
              }
              stats={[
                { label: t("social.lb.col.return", { period: t(PERIOD_LABEL[period]) }), value: pct(r, 1), tone: r > 0 ? "up" : r < 0 ? "down" : undefined },
                { label: t("social.maxDd"), value: m.stats.maxDd > 0 ? `-${m.stats.maxDd.toFixed(1)}%` : "0.0%", tone: m.stats.maxDd > 0 ? "down" : undefined },
                { label: t("social.followers"), value: f.number(m.stats.followers, 0) },
                { label: t("social.risk"), value: `${Math.max(1, Math.min(10, Math.round(m.stats.riskScore || 1)))}/10` },
              ]}
            />
          </Link>
          {/* pick up to three to compare */}
          <Tooltip content={on ? t("social.compare.remove", { name: m.nickname }) : t("social.compare.add")}>
            <button
              type="button"
              aria-pressed={on}
              aria-label={on ? t("social.compare.remove", { name: m.nickname }) : t("social.compare.add")}
              onClick={() => togglePick(m)}
              className={cn(
                "absolute end-3 top-3 grid size-7 place-items-center rounded-full border transition-colors",
                on ? "border-ember bg-ember text-white" : ivory ? "border-black/20 bg-black/5 text-transparent hover:border-ember/60" : "border-white/25 bg-white/10 text-transparent hover:border-white/60",
              )}
              data-testid="copy-compare-toggle"
            >
              <Check className="size-4" />
            </button>
          </Tooltip>
        </div>
        <div className="mt-3 flex items-center gap-2">
          {m.program !== "pamm" && !m.frozen && (
            m.acceptingNew === false ? (
              <Tooltip content={t("social.notAccepting")}>
                <span className="flex-1">
                  <Button size="sm" variant="surface" disabled className="w-full">
                    {t("social.program.copy")}
                  </Button>
                </span>
              </Tooltip>
            ) : (
              <Button size="sm" variant="ember" className="flex-1" onClick={() => setCopyM(m)}>
                <Repeat /> {t("social.program.copy")}
              </Button>
            )
          )}
          {m.fund && m.program !== "copy" && m.fund.status === "active" && (
            <Button size="sm" variant="surface" className="flex-1" onClick={() => setInvestFund(m.fund!.id)}>
              <Wallet /> {t("social.invest")}
            </Button>
          )}
          <Link href={`/social/masters/${m.id}`} className="shrink-0">
            <Button size="sm" variant="ghost">
              {t("common.details")}
            </Button>
          </Link>
        </div>
      </div>
    );
  };

  return (
    <div className="pb-24">
      <PageHero
        page="copy"
        overlap
        title={t("social.lb.title")}
        lead={t("social.lb.subtitle")}
        actions={
          <>
            <Link href="/social/copy">
              <Button variant="surface" size="lg">
                <Repeat /> {t("social.mySubscriptions")}
              </Button>
            </Link>
            <Link href="/social/master">
              <Button variant="ember" size="lg">
                <Crown /> {t("social.becomeMaster")}
              </Button>
            </Link>
          </>
        }
      />

      <Card>
        <div className="grid grid-cols-1 gap-6 p-6 sm:p-7 lg:grid-cols-[1.2fr_1fr] lg:items-center">
          <div>
            <Chip tone="ember" className="mb-3 h-auto max-w-full whitespace-normal py-1 leading-snug">
              <ShieldCheck className="size-3.5 shrink-0" /> {rows.some((m) => m.house) ? t("social.lb.hero.chipClient") : t("social.lb.hero.chipAll")}
            </Chip>
            <h2 className="text-[22px] font-medium leading-tight tracking-tight sm:text-[26px]">{t("social.lb.hero.title")}</h2>
            <p className="mt-2 max-w-xl text-[14px] text-fg-2">{t("social.lb.hero.text")}</p>
          </div>
          <div className="grid grid-cols-2 gap-2.5">
            {[
              { icon: <Crown />, k: t("social.lb.stat.masters"), v: totals ? f.number(totals.masters, 0) : null },
              { icon: <Wallet />, k: t("social.lb.stat.aum"), v: totals ? compactUsd(totals.aum) : null },
              { icon: <Users />, k: t("social.lb.stat.followers"), v: totals ? f.number(totals.followers, 0) : null },
              { icon: <Landmark />, k: t("social.lb.stat.investors"), v: totals ? f.number(totals.investors, 0) : null },
            ].map((x) => (
              <div key={x.k} className="k-row px-4 py-3">
                <div className="flex items-center gap-1.5 text-[11.5px] text-fg-3 [&_svg]:size-3.5">
                  {x.icon}
                  {x.k}
                </div>
                <div className="k-num mt-1 text-[20px] font-semibold">{x.v ?? (error ? "—" : <Skeleton className="mt-1 h-6 w-16" />)}</div>
              </div>
            ))}
          </div>
        </div>
      </Card>

      <div className="mt-4">
        {error && !data ? (
          <SocialError onRetry={reload} message={error.message} />
        ) : (
          <Card>
            <CardHeader
              title={t("social.lb.leaderboard")}
              subtitle={data ? t("social.lb.leaderboardSub", { count: rows.length }) : t("common.loading")}
              icon={<Trophy />}
              action={<Segmented size="xs" value={period} onChange={setPeriod} options={(Object.keys(PERIOD_LABEL) as Period[]).map((p) => ({ value: p, label: t(PERIOD_LABEL[p]) }))} />}
            />
            <div className="px-4 pb-5 pt-4 sm:px-6">
              <div className="mb-3 flex flex-wrap items-center gap-2">
                <Segmented
                  size="xs"
                  value={sort}
                  onChange={setSort}
                  options={[
                    { value: "return", label: t("social.lb.sort.return") },
                    { value: "dd", label: t("social.lb.sort.dd") },
                    { value: "aum", label: t("social.aum") },
                    { value: "followers", label: t("social.followers") },
                    { value: "age", label: t("social.lb.sort.age") },
                  ]}
                />
                <Segmented
                  size="xs"
                  value={program}
                  onChange={setProgram}
                  options={[
                    { value: "all", label: t("common.all") },
                    { value: "copy", label: t("social.program.copy") },
                    { value: "pamm", label: "PAMM" },
                  ]}
                />
                <Segmented
                  size="xs"
                  value={risk}
                  onChange={setRisk}
                  options={[
                    { value: "all", label: t("social.lb.anyRisk") },
                    { value: "low", label: "1–3" },
                    { value: "med", label: "4–6" },
                    { value: "high", label: "7–10" },
                  ]}
                />
                <Menu
                  align="start"
                  width={200}
                  trigger={
                    <Button size="sm" variant="surface">
                      {t(TRACK.find((x) => x.v === track)!.label)} <ChevronDown className="opacity-60" />
                    </Button>
                  }
                  items={TRACK.map((x) => ({ label: t(x.label), onSelect: () => setTrack(x.v), hint: x.v === track ? t("social.selected") : undefined }))}
                />
                <Popover
                  align="start"
                  width={340}
                  trigger={
                    <Button size="sm" variant={more ? "outline" : "surface"} data-testid="copy-more-filters">
                      <SlidersHorizontal /> {t("social.lb.more.title")}
                      {more > 0 && <span className="k-num grid size-4.5 place-items-center rounded-full bg-ember text-[10.5px] font-semibold text-white">{more}</span>}
                    </Button>
                  }
                >
                  <div className="space-y-4 p-4">
                    <FilterRow label={t("social.lb.more.maxDd")}>
                      <Segmented size="xs" value={String(maxDd)} onChange={(v) => setMaxDd(+v)} options={DD_STEPS.map((v) => ({ value: String(v), label: v ? `≤${v}%` : t("social.lb.more.any") }))} />
                    </FilterRow>
                    <FilterRow label={t("social.lb.more.maxFee")}>
                      <Segmented size="xs" value={String(maxFee)} onChange={(v) => setMaxFee(+v)} options={FEE_STEPS.map((v) => ({ value: String(v), label: v ? `≤${v}%` : t("social.lb.more.any") }))} />
                    </FilterRow>
                    <FilterRow label={t("social.lb.more.minFollowers")}>
                      <Segmented size="xs" value={String(minFollowers)} onChange={(v) => setMinFollowers(+v)} options={FOLLOWER_STEPS.map((v) => ({ value: String(v), label: v ? `${v}+` : t("social.lb.more.any") }))} />
                    </FilterRow>
                    <div className="flex items-center justify-between gap-3">
                      <div>
                        <div className="text-[12.5px] font-medium text-fg-2">{t("social.lb.more.openOnly")}</div>
                        <div className="text-[11.5px] text-fg-3">{t("social.lb.more.openOnlyHint")}</div>
                      </div>
                      <Toggle checked={openOnly} onChange={setOpenOnly} label={t("social.lb.more.openOnly")} />
                    </div>
                    {more > 0 && (
                      <Button size="xs" variant="ghost" onClick={clearMore}>
                        <XIcon /> {t("social.clearFilters")}
                      </Button>
                    )}
                  </div>
                </Popover>
              </div>
              {loading ? (
                <div className="space-y-2">
                  {Array.from({ length: 5 }, (_, i) => (
                    <Skeleton key={i} className="h-14 w-full rounded-[14px]" />
                  ))}
                </div>
              ) : rows.length === 0 ? (
                <EmptyState
                  illustration="trophy"
                  art={filtered ? undefined : "copyTrading"}
                  title={filtered ? t("social.lb.empty.filteredTitle") : t("social.lb.empty.title")}
                  text={filtered ? t("social.lb.empty.filteredText") : t("social.lb.empty.text")}
                  action={
                    filtered ? (
                      <Button
                        variant="surface"
                        onClick={() => {
                          setProgram("all");
                          setRisk("all");
                          setTrack(0);
                          clearMore();
                        }}
                      >
                        {t("social.clearFilters")}
                      </Button>
                    ) : (
                      <Link href="/social/master">
                        <Button variant="ember">
                          <Crown /> {t("social.becomeMaster")}
                        </Button>
                      </Link>
                    )
                  }
                />
              ) : (
                <>
                  <div className="mb-5 flex h-10 max-w-[360px] items-center gap-2 rounded-full border border-line bg-surface-2 px-3.5">
                    <Search className="size-4 text-fg-3" />
                    <input
                      value={q}
                      onChange={(e) => {
                        setQ(e.target.value);
                        setShown(12);
                      }}
                      placeholder={t("social.lb.searchPlaceholder")}
                      aria-label={t("social.lb.searchPlaceholder")}
                      className="w-full bg-transparent text-[13px] outline-none placeholder:text-fg-3"
                    />
                  </div>
                  {list.length === 0 ? (
                    <p className="py-10 text-center text-[13px] text-fg-3">{t("social.lb.empty.filteredText")}</p>
                  ) : (
                    <div className="grid grid-cols-1 gap-x-5 gap-y-7 md:grid-cols-2 xl:grid-cols-3" data-testid="master-grid">
                      {list.slice(0, shown).map((m) => masterCard(m, rows.indexOf(m)))}
                    </div>
                  )}
                  {list.length > shown && (
                    <div className="mt-6 flex justify-center">
                      <Button variant="surface" onClick={() => setShown((n) => n + 12)}>
                        {t.dyn("common.showMore", "Show more")} <ChevronDown />
                      </Button>
                    </div>
                  )}
                </>
              )}
            </div>
          </Card>
        )}
      </div>

      <p className="mt-6 flex items-start gap-2 text-[12px] leading-relaxed text-fg-3">
        <LineChart className="mt-0.5 size-3.5 shrink-0" />
        {t("social.lb.disclaimer")}
      </p>
      {rows.some((m) => m.house) && (
        <p className="mt-2 ps-5.5 text-[12px] leading-relaxed text-fg-3">
          {t("social.lb.houseNote")}
        </p>
      )}

      {picked.length > 0 && (
        <div className="pointer-events-none fixed inset-x-0 bottom-5 z-40 flex justify-center px-4" data-testid="copy-compare-bar">
          <div className="k-card pointer-events-auto flex max-w-full items-center gap-2 rounded-full bg-surface py-1.5 pe-1.5 ps-4 shadow-[0_24px_60px_-20px_rgba(0,0,0,0.7)]">
            <span className="hidden min-w-0 truncate text-[12.5px] text-fg-2 sm:block">{picked.map((m) => m.nickname).join(" · ")}</span>
            <span className="text-[12px] text-fg-3 sm:hidden">{t("social.compare.picked", { n: picked.length, max: COMPARE_MAX })}</span>
            <Button size="sm" variant="ghost" onClick={() => setPicked([])} aria-label={t("social.compare.clear")}>
              <XIcon />
            </Button>
            <Button size="sm" variant="ember" disabled={picked.length < 2} onClick={() => setComparing(true)}>
              <Columns3 /> {t("social.compare.button", { n: picked.length })}
            </Button>
          </div>
        </div>
      )}

      <CompareDialog
        masters={picked}
        open={comparing && picked.length > 0}
        onOpenChange={setComparing}
        onRemove={(id) => setPicked((p) => p.filter((x) => x.id !== id))}
      />
      <FollowDialog master={copyM} open={!!copyM} onOpenChange={(o) => !o && setCopyM(null)} />
      <InvestDialog fundId={investFund} open={investFund !== null} onOpenChange={(o) => !o && setInvestFund(null)} />
    </div>
  );
}
