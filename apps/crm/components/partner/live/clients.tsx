"use client";

import * as React from "react";
import { EyeOff, Filter, Mail, UserPlus } from "lucide-react";
import { toast } from "sonner";
import {
  Button,
  Card,
  Chip,
  CopyButton,
  DataTable,
  Dialog,
  Money,
  PageHeader,
  Reveal,
  Segmented,
  Skeleton,
  
  cn,
  formatMoney,
  type Column,
} from "@/components/kit";
// engine symbols include Kalks FX Options series codes, which @kalks/ui SymbolAvatar / SymbolCell (static list) throw on
import { TradeSymbolAvatar as SymbolAvatar, symbolLabel } from "@/components/trading/instrument";
import {
  fmtDate,
  fmtLots,
  isOptionLine,
  linkBase,
  optionsLabel,
  partnerApi,
  referralLink,
  relTime,
  shortUrl,
  usePartner,
  type CampaignsResp,
  type ClientStatus,
  type ClientTrade,
  type ClientsResp,
  type NetworkClient,
  type PartnerApiError,
} from "./api";
import { tr, useT } from "@kalks/i18n/react";
import {
  CardEmpty,
  ClientStatusChip,
  LoadProblem,
  MiniStat,
  PageFallback,
  PersonCell,
  SkeletonGrid,
  TierChip,
} from "./ui";

type TierF = "all" | "1" | "2" | "3";
type StatusF = "all" | ClientStatus;

const KYC: Record<
  string,
  { label: string; tone: "up" | "warn" | "down" | "neutral" }
> = {
  verified: { label: "Verified", tone: "up" },
  approved: { label: "Verified", tone: "up" },
  pending: { label: "In review", tone: "warn" },
  rejected: { label: "Rejected", tone: "down" },
  unverified: { label: "Not started", tone: "neutral" },
};
const kyc = (k: string) => {
  const m = KYC[k] ?? { label: k.replace(/_/g, " "), tone: "neutral" as const };
  return { ...m, label: tr.dyn(`partner.kyc.${k}`, m.label) };
};

function fmtDuration(ms: number) {
  const s = Math.max(0, Math.round(ms / 1000));
  if (s < 60) return tr("partner.dur.s", { s });
  if (s < 3600) return tr("partner.dur.m", { m: Math.round(s / 60) });
  const h = Math.floor(s / 3600);
  return h < 24
    ? tr("partner.dur.hm", { h, m: Math.round((s % 3600) / 60) })
    : tr("partner.dur.dh", { d: Math.floor(h / 24), h: h % 24 });
}

const NOT_QUALIFIED: Record<string, string> = {
  short_duration: "Held too briefly",
  excluded_group: "Excluded account group",
  mam_master: "MAM master account (counted on the managed accounts)",
  pamm_fund: "PAMM fund account (counted per investor)",
  self_referral: "Self-referral check",
  demo: "Not a live account",
  reversed: "Reversed",
  price_correction: "Price correction",
  no_volume: "No volume",
  no_symbol_group: "Symbol not in rate card",
  no_referrer: "No referrer",
};
const reasonLabel = (r: string | null) =>
  r
    ? tr.dyn(`partner.reason.${r}`, NOT_QUALIFIED[r] ?? r.replace(/_/g, " "))
    : tr("partner.reason.notEligible");

/* ------------------------------------------------------------------ */

function TradesList({
  id,
  minSeconds,
}: {
  id: number;
  minSeconds: number | null;
}) {
  const t = useT();
  const [state, setState] = React.useState<{
    items: ClientTrade[] | null;
    error: PartnerApiError | null;
  }>({ items: null, error: null });
  const [tick, setTick] = React.useState(0);
  React.useEffect(() => {
    const ctl = new AbortController();
    setState({ items: null, error: null });
    partnerApi<{ items: ClientTrade[] }>(`clients/${id}/trades`, {
      signal: ctl.signal,
    })
      .then((r) => setState({ items: r.items, error: null }))
      .catch((e) => {
        if ((e as Error).name !== "AbortError")
          setState({ items: null, error: e as PartnerApiError });
      });
    return () => ctl.abort();
  }, [id, tick]);

  if (state.error)
    return state.error.status === 403 ? (
      <CardEmpty
        title={t("partner.clients.tradesNotShared")}
        text={state.error.message}
      />
    ) : (
      <LoadProblem error={state.error} onRetry={() => setTick((n) => n + 1)} />
    );
  if (!state.items)
    return (
      <div className="space-y-1.5">
        {Array.from({ length: 4 }, (_, i) => (
          <Skeleton key={i} className="h-[52px] w-full rounded-[14px]" />
        ))}
      </div>
    );
  if (state.items.length === 0)
    return (
      <CardEmpty
        title={t("partner.clients.noTrades")}
        text={t("partner.clients.noTradesText")}
      />
    );
  return (
    <div className="space-y-1.5">
      {state.items.map((x) => {
        const ok = x.qualified && !x.reversed;
        return (
          <div
            key={`${x.source}-${x.dealId}`}
            className={cn(
              "k-row flex items-center gap-3 px-3 py-2",
              !ok && "opacity-75",
            )}
          >
            <SymbolAvatar symbol={x.symbol} size={22} />
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-1.5 text-[13px] font-medium">
                {symbolLabel(t, x.symbol)}
                <Chip size="sm" tone={x.side === "buy" ? "up" : "down"}>
                  {x.side === "buy"
                    ? t("common.buy").toUpperCase()
                    : x.side === "sell"
                      ? t("common.sell").toUpperCase()
                      : x.side.toUpperCase()}
                  {/* option deals are counted in contracts (shown next to the side), never lots */}
                  {isOptionLine(x) ? null : <> {fmtLots(x.lots)}</>}
                </Chip>
                {isOptionLine(x) && (
                  <Chip size="sm">{optionsLabel(x.contracts)}</Chip>
                )}
                {x.source !== "engine" && (
                  <Chip size="sm">
                    {x.source === "pamm"
                      ? "PAMM"
                      : x.source === "copy"
                        ? t("partner.clients.copy")
                        : x.source}
                  </Chip>
                )}
              </div>
              <div className="truncate font-mono text-[10.5px] text-fg-3">
                #{x.dealId} ·{" "}
                {t("partner.clients.held", {
                  duration: fmtDuration(
                    Date.parse(x.closeTime) - Date.parse(x.openTime),
                  ),
                })}{" "}
                · {relTime(x.closeTime)}
              </div>
            </div>
            <div className="shrink-0 text-end text-[12px]">
              {x.reversed ? (
                <span className="text-fg-3">{t("partner.reason.reversed")}</span>
              ) : ok ? (
                <span className="k-num font-medium text-up">
                  {t("partner.clients.toYou", {
                    amount: `+${formatMoney(x.earned)}`,
                  })}
                </span>
              ) : (
                <span className="text-down">{reasonLabel(x.reason)}</span>
              )}
            </div>
          </div>
        );
      })}
      {minSeconds !== null && (
        <p className="pt-1 text-[11.5px] text-fg-3">
          {t("partner.clients.minHoldNote", {
            duration:
              minSeconds >= 60
                ? t("partner.unit.min", { n: Math.round(minSeconds / 60) })
                : t("partner.unit.sec", { n: minSeconds }),
          })}
        </p>
      )}
    </div>
  );
}

function ClientDialog({
  c,
  via,
  onClose,
  minSeconds,
}: {
  c: NetworkClient | null;
  via: string | null;
  onClose: () => void;
  minSeconds: number | null;
}) {
  const t = useT();
  return (
    <Dialog
      open={!!c}
      onOpenChange={(o) => !o && onClose()}
      side="right"
      title={t("partner.clients.details")}
      description={
        c
          ? `${t("partner.clients.joinedOn", { date: fmtDate(c.joinedAt) })} · ${c.tier === 1 ? t("partner.clients.yourDirect") : via ? t("partner.clients.via", { name: via }) : t("partner.clients.tierN", { n: c.tier })}`
          : undefined
      }
    >
      {c && (
        <div className="space-y-5">
          <div className="flex items-center gap-3">
            <div className="min-w-0 flex-1">
              <PersonCell name={c.name} country={c.country} size={52} />
            </div>
            <TierChip tier={c.tier} />
            <ClientStatusChip status={c.status} />
          </div>

          {c.email && (
            <div className="k-row flex items-center gap-3 px-3.5 py-2.5 text-[13px]">
              <Mail className="size-4 shrink-0 text-fg-3" />
              <span className="min-w-0 flex-1 truncate">{c.email}</span>
              <CopyButton value={c.email} label={t("common.email")} />
            </div>
          )}

          <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
            {(
              [
                [
                  "KYC",
                  <Chip key="k" size="sm" tone={kyc(c.kycStatus).tone}>
                    {kyc(c.kycStatus).label}
                  </Chip>,
                ],
                [
                  t("partner.clients.firstDeposit"),
                  c.firstDepositAt ? (
                    c.firstDepositAmount !== null ? (
                      <Money
                        key="d"
                        value={c.firstDepositAmount}
                        countUp={false}
                      />
                    ) : (
                      fmtDate(c.firstDepositAt)
                    )
                  ) : (
                    t("partner.clients.notYet")
                  ),
                ],
                [
                  t("partner.clients.firstTrade"),
                  c.firstTradeAt
                    ? fmtDate(c.firstTradeAt)
                    : t("partner.clients.notYet"),
                ],
                [
                  t("partner.lotsThisMonth"),
                  <span key="l" className="k-num">
                    {fmtLots(c.lotsMonth)}
                  </span>,
                ],
                [
                  t("partner.clients.lotsLifetime"),
                  <span key="lt" className="k-num">
                    {fmtLots(c.lotsTotal)}
                  </span>,
                ],
                [
                  t("partner.clients.yourCommission"),
                  <Money
                    key="c"
                    value={c.earned}
                    countUp={false}
                    className="text-up"
                  />,
                ],
                [
                  t("partner.clients.source"),
                  c.campaign ?? t("partner.referralLink"),
                ],
                [
                  t("partner.clients.theirReferrals"),
                  <span key="r" className="k-num">
                    {c.referrals}
                  </span>,
                ],
                [t("partner.clients.lastTrade"), relTime(c.lastTradeAt)],
              ] as [string, React.ReactNode][]
            ).map(([k, v]) => (
              <div key={k} className="k-row min-w-0 px-3.5 py-2.5">
                <div className="truncate text-[12px] text-fg-3">
                  {k}
                </div>
                <div className="mt-1 truncate text-[13.5px] font-medium">
                  {v}
                </div>
              </div>
            ))}
          </div>

          <div>
            <h4 className="mb-2 text-[14px] font-medium">
              {t("partner.clients.closedTrades")}
            </h4>
            <TradesList id={c.id} minSeconds={minSeconds} />
          </div>
        </div>
      )}
    </Dialog>
  );
}

/* ------------------------------------------------------------------ */

export function LivePartnerClients() {
  const t = useT();
  const TITLE = t("partner.clients.title");
  const { data, error, reload } = usePartner<ClientsResp>("clients");
  const { data: camp } = usePartner<CampaignsResp>("campaigns");
  const { data: prog } = usePartner<{ minTradeSeconds: number }>("programme");
  const [tier, setTier] = React.useState<TierF>("all");
  const [status, setStatus] = React.useState<StatusF>("all");
  const [sel, setSel] = React.useState<NetworkClient | null>(null);

  const all = React.useMemo(() => data?.items ?? [], [data]);
  const byId = React.useMemo(() => new Map(all.map((c) => [c.id, c])), [all]);
  const rows = React.useMemo(
    () =>
      all.filter(
        (c) =>
          (tier === "all" || String(c.tier) === tier) &&
          (status === "all" || c.status === status),
      ),
    [all, tier, status],
  );
  const tiers = data?.tiers ?? 3;
  const subtitle = t("partner.clients.subtitle", { count: tiers });

  if (!data)
    return (
      <PageFallback
        title={TITLE}
        subtitle={subtitle}
        error={error}
        onRetry={reload}
        skeleton={
          <SkeletonGrid
            rows={[
              { cols: "grid-cols-2 lg:grid-cols-5", h: "h-[92px]", n: 5 },
              { cols: "", h: "h-[420px]", n: 1 },
            ]}
          />
        }
      />
    );

  const full = data.visibility === "full";
  const code = camp?.code;
  const link = code ? referralLink(linkBase(), code) : null;
  const direct = all.filter((c) => c.tier === 1).length;
  const lotsMonth = all.reduce((s, c) => s + c.lotsMonth, 0);
  const earned = all.reduce((s, c) => s + c.earned, 0);
  const funded = all.filter((c) => c.firstDepositAt).length;
  const subIbs = all.filter((c) => c.referrals > 0).length;
  const viaName = (c: NetworkClient) =>
    c.parentId ? (byId.get(c.parentId)?.name ?? null) : null;

  const columns: Column<NetworkClient>[] = [
    {
      key: "name",
      header: t("partner.client"),
      cell: (c) => (
        <PersonCell
          name={c.name}
          country={c.country}
          size={34}
          sub={c.email ?? undefined}
        />
      ),
      sort: (c) => c.name,
      csv: (c) => c.name,
      width: "240px",
    },
    {
      key: "tier",
      header: t("partner.tier"),
      cell: (c) => (
        <span className="flex flex-col items-start gap-0.5">
          <TierChip tier={c.tier} />
          {c.tier > 1 && viaName(c) && (
            <span className="max-w-[110px] truncate text-[10.5px] text-fg-3">
              {t("partner.clients.via", { name: viaName(c) ?? "" })}
            </span>
          )}
        </span>
      ),
      sort: (c) => c.tier,
    },
    {
      key: "joined",
      header: t("partner.clients.joined"),
      cell: (c) => (
        <span className="k-num text-fg-2">{fmtDate(c.joinedAt)}</span>
      ),
      sort: (c) => c.joinedAt,
      hideOn: "md",
    },
    {
      key: "campaign",
      header: t("partner.clients.source"),
      cell: (c) => (
        <span className="block max-w-[140px] truncate text-fg-2">
          {c.campaign ?? t("partner.referralLink")}
        </span>
      ),
      sort: (c) => c.campaign ?? "",
      hideOn: "lg",
    },
    {
      key: "lots",
      header: t("partner.lotsMonth"),
      align: "right",
      cell: (c) => (
        <span className="block">
          <span className="k-num font-medium">
            {c.lotsMonth ? fmtLots(c.lotsMonth) : "—"}
          </span>
          <span className="k-num block text-[11px] text-fg-3">
            {t("partner.clients.lotsTotal", { lots: fmtLots(c.lotsTotal, 1) })}
          </span>
        </span>
      ),
      sort: (c) => c.lotsMonth,
      csv: (c) => c.lotsMonth,
    },
    {
      key: "earned",
      header: t("partner.clients.earned"),
      align: "right",
      cell: (c) =>
        c.earned ? (
          <Money
            value={c.earned}
            countUp={false}
            className="font-medium text-up"
          />
        ) : (
          <span className="text-fg-3">—</span>
        ),
      sort: (c) => c.earned,
    },
    {
      key: "status",
      header: t("common.status"),
      align: "right",
      cell: (c) => (
        <span className="flex flex-col items-end gap-0.5">
          <ClientStatusChip status={c.status} />
          <span className="k-num text-[11px] text-fg-3">
            {c.lastTradeAt ? relTime(c.lastTradeAt) : t("partner.noTrades")}
          </span>
        </span>
      ),
      sort: (c) => (c.status === "active" ? 2 : c.status === "funded" ? 1 : 0),
      csv: (c) => c.status,
    },
  ];

  return (
    <div className="pb-24">
      <PageHeader
        title={TITLE}
        subtitle={subtitle}
        actions={
          link ? (
            <Button
              variant="ember"
              size="lg"
              onClick={() => {
                navigator.clipboard?.writeText(link).catch(() => {});
                toast.success(t("partner.toast.linkCopied"), {
                  description: shortUrl(link),
                });
              }}
            >
              <UserPlus /> {t("partner.clients.invite")}
            </Button>
          ) : undefined
        }
      />

      <Reveal>
        <div className="grid grid-cols-2 gap-3 lg:grid-cols-5">
          <MiniStat
            label={t("partner.clients.referred")}
            value={all.length}
            sub={t("partner.clients.directVia", {
              direct,
              via: all.length - direct,
            })}
          />
          <MiniStat
            label={t("partner.clients.activeMonth")}
            value={all.filter((c) => c.status === "active").length}
            sub={t("partner.tradedThisMonth")}
          />
          <MiniStat
            label={t("partner.clientStatus.funded")}
            value={funded}
            sub={
              all.length
                ? t("partner.clients.pctOfReferrals", {
                    pct: Math.round((funded / all.length) * 100),
                  })
                : t("partner.clients.firstDepositMade")
            }
          />
          <MiniStat
            label={t("partner.lotsThisMonth")}
            value={fmtLots(lotsMonth, 1)}
            sub={t("partner.subIbCount", { count: subIbs })}
          />
          <div className="col-span-2 lg:col-span-1">
            <MiniStat
              label={t("partner.clients.earnedFrom")}
              value={<Money value={earned} countUp={false} />}
              sub={t("partner.clients.lifetimeAllTiers")}
            />
          </div>
        </div>
      </Reveal>

      {!full && (
        <Reveal delay={0.04}>
          <div className="mt-4 flex items-start gap-3 rounded-[16px] border border-line bg-surface-2 px-4 py-3 text-[12.5px] text-fg-2">
            <EyeOff className="mt-0.5 size-4 shrink-0 text-fg-3" />
            <span>
              {t("partner.clients.privacyNote")}
            </span>
          </div>
        </Reveal>
      )}

      <Reveal delay={0.08}>
        <Card className="mt-4 px-4 py-5 sm:px-6">
          <DataTable
            columns={columns}
            rows={rows}
            pageSize={12}
            rowKey={(c) => String(c.id)}
            onRowClick={full ? setSel : undefined}
            search={(c) =>
              `${c.name} ${c.email ?? ""} ${c.country} ${c.campaign ?? ""}`
            }
            searchPlaceholder={
              full
                ? t("partner.clients.searchFull")
                : t("partner.clients.searchLimited")
            }
            exportName={all.length ? "kalks-referred-clients" : undefined}
            empty={
              all.length === 0 ? (
                <div className="py-6">
                  <CardEmpty
                    art="partnerIb"
                    contentSlot={{ page: "empty", slot: "referrals.image" }}
                    title={t("partner.clients.emptyTitle")}
                    text={t("partner.clients.emptyText")}
                  >
                    {link && (
                      <Button
                        size="sm"
                        variant="surface"
                        onClick={() => {
                          navigator.clipboard?.writeText(link).catch(() => {});
                          toast.success(t("partner.toast.linkCopied"), {
                            description: shortUrl(link),
                          });
                        }}
                      >
                        {t("partner.copyReferralLink")}
                      </Button>
                    )}
                  </CardEmpty>
                </div>
              ) : (
                <div className="py-6">
                  <CardEmpty
                    title={t("partner.clients.noMatch")}
                    text={t("partner.clients.noMatchText")}
                  />
                </div>
              )
            }
            toolbar={
              <div className="flex flex-wrap items-center gap-2">
                <Segmented
                  size="xs"
                  value={tier}
                  onChange={setTier}
                  options={[
                    { value: "all" as TierF, label: t("partner.allTiers") },
                    ...Array.from({ length: Math.min(tiers, 3) }, (_, i) => ({
                      value: String(i + 1) as TierF,
                      label: `L${i + 1}`,
                    })),
                  ]}
                />
                <Segmented
                  size="xs"
                  value={status}
                  onChange={setStatus}
                  options={[
                    { value: "all", label: t("common.all") },
                    { value: "active", label: t("partner.clientStatus.active") },
                    { value: "funded", label: t("partner.clientStatus.funded") },
                    {
                      value: "registered",
                      label: t("partner.clientStatus.registered"),
                    },
                  ]}
                />
                {(tier !== "all" || status !== "all") && (
                  <Button
                    size="xs"
                    variant="ghost"
                    onClick={() => {
                      setTier("all");
                      setStatus("all");
                    }}
                  >
                    <Filter /> {t("partner.clear")}
                  </Button>
                )}
                <span className="k-num text-[12px] text-fg-3">
                  {t("partner.clients.count", { count: rows.length })}
                </span>
              </div>
            }
          />
          <div className="mt-3 text-[11.5px] text-fg-3">
            {t("partner.clients.legend")}
            {full ? ` ${t("partner.clients.selectHint")}` : ""}
          </div>
        </Card>
      </Reveal>

      <ClientDialog
        c={sel}
        via={sel ? viaName(sel) : null}
        onClose={() => setSel(null)}
        minSeconds={prog?.minTradeSeconds ?? null}
      />
    </div>
  );
}
