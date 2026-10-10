"use client";

// Client Area → Social → Managed accounts: find a MAM programme, link one of your live accounts with an explicit
// consent to the manager's terms, watch it, set limits and revoke. The account and the money stay yours: the
// manager only trades it.

import * as React from "react";
import Link from "next/link";
import { ArrowUpRight, Briefcase, FileText, Loader2, Settings2, ShieldCheck, Unlink, Users } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, CardHeader, Chip, DataTable, Dialog, EmptyState, Field, Input, PageHeader, StatusChip, cn, type Column } from "@/components/kit";
import { Trans, useT } from "@kalks/i18n/react";
import { Checkbox } from "@/components/social/controls";
import { TradeButton } from "@/components/trading/ui";
import { fmtDate, fmtPrice, serverTime } from "@/components/trading/api";
import { PERIOD_LABEL, pct, socialApi, usd, useSocial } from "./api";
import { BlockSkeleton, InfoBox, SocialError, Tile, useNumber } from "./bits";
import { FeesTable } from "./subscriptions";
import { VisitingCard } from "@/components/visiting-card";
import { METHOD_HINT, METHOD_LABEL, STOP_REASON, lots, valueText, type Candidate, type LinkDetail, type LinkView, type ManagerDetail, type ManagerView } from "./mam-api";

const tone = (v: number) => (v > 0 ? "text-up" : v < 0 ? "text-down" : "text-fg-2");

function feesText(m: { perfFeePct: number; mgmtFeePct: number; feePeriod: ManagerView["feePeriod"] }, t: ReturnType<typeof useT>) {
  const period = PERIOD_LABEL[m.feePeriod].toLowerCase();
  return m.mgmtFeePct > 0 ? t("social.mam.feesTextMgmt", { perf: m.perfFeePct, mgmt: m.mgmtFeePct, period }) : t("social.mam.feesText", { perf: m.perfFeePct, period });
}

/* ------------------------------------------------------------------ */
/* Connect (consent) dialog                                            */
/* ------------------------------------------------------------------ */

function ConnectDialog({ managerId, onClose, onLinked }: { managerId: number | null; onClose: () => void; onLinked: () => void }) {
  const t = useT();
  const { data, error, reload } = useSocial<ManagerDetail>(managerId ? `mam/managers/${managerId}` : null);
  const [login, setLogin] = React.useState<number | null>(null);
  const [accept, setAccept] = React.useState(false);
  const maxLot = useNumber(null);
  const equityStop = useNumber(null);
  const [busy, setBusy] = React.useState(false);

  React.useEffect(() => {
    setLogin(null);
    setAccept(false);
    maxLot.set(null);
    equityStop.set(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [managerId]);
  React.useEffect(() => {
    if (data && login === null) setLogin(data.accounts.find((a) => a.eligible)?.login ?? null);
  }, [data, login]);

  if (!managerId) return null;
  const m = data?.manager;
  const chosen = data?.accounts.find((a) => a.login === login);
  const err = !chosen
    ? t("social.mam.err.chooseAccount")
    : maxLot.raw && !(maxLot.value! >= 0.01 && maxLot.value! <= 100)
      ? t("social.mam.err.maxLot")
      : equityStop.raw && !(equityStop.value! > 0 && equityStop.value! < chosen.equity)
        ? t("social.mam.err.equityStopRange")
        : !accept
          ? t("social.mam.err.acceptTerms")
          : undefined;

  const submit = async () => {
    if (!data || err) return;
    setBusy(true);
    try {
      await socialApi("mam/links", { body: { managerId, login, termsHash: data.terms.hash, accept: true, maxLot: maxLot.value ?? undefined, equityStop: equityStop.value ?? undefined } });
      toast.success(t("social.mam.toast.linked", { login: login ?? "", name: m?.name ?? "" }), { description: t("social.mam.toast.linkedDesc") });
      onLinked();
      onClose();
    } catch (e) {
      toast.error(t("social.mam.toast.linkFailed"), { description: e instanceof Error ? e.message : undefined });
      if ((e as { code?: string }).code === "terms_changed") reload();
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open={!!managerId}
      onOpenChange={(o) => !o && onClose()}
      width={640}
      title={m ? t("social.mam.connect.title", { name: m.name }) : t("social.mam.connect.titleEmpty")}
      description={m ? `${m.nickname ?? t("social.mam.manager")} · ${METHOD_LABEL[m.method]} · ${feesText(m, t)}` : undefined}
      footer={
        <>
          <Button variant="ghost" size="sm" onClick={onClose} disabled={busy}>
            {t("common.cancel")}
          </Button>
          <Button variant="ember" onClick={submit} disabled={busy || !!err || !data} title={err}>
            {busy && <Loader2 className="animate-spin" />} {t("social.mam.connect.grant")}
          </Button>
        </>
      }
    >
      {error && !data ? (
        <SocialError onRetry={reload} message={error.message} title={t("social.mam.programmeUnavailable")} />
      ) : !data || !m ? (
        <BlockSkeleton n={2} h={90} />
      ) : (
        <div className="space-y-5">
          <div>
            <div className="mb-2 text-[12.5px] font-medium text-fg-2">{t("social.mam.connect.accountToLink")}</div>
            {data.accounts.length === 0 ? (
              <div className="k-row px-4 py-5 text-[13px] text-fg-3">
                <Trans k="social.mam.connect.noLive" tags={{ link: (c) => <Link href="/accounts/new" className="text-ember hover:underline">{c}</Link> }} />
              </div>
            ) : (
              <div role="radiogroup" className="space-y-2">
                {data.accounts.map((a: Candidate) => (
                  <button
                    key={a.login}
                    type="button"
                    role="radio"
                    aria-checked={login === a.login}
                    disabled={!a.eligible}
                    onClick={() => setLogin(a.login)}
                    className={cn(
                      "flex w-full items-center gap-3 rounded-[14px] border px-3.5 py-3 text-start transition-colors",
                      login === a.login ? "border-ember/60 bg-ember-soft" : "border-line bg-surface-2 hover:border-fg-3",
                      !a.eligible && "cursor-not-allowed opacity-60 hover:border-line",
                    )}
                  >
                    <span className={cn("grid size-4 shrink-0 place-items-center rounded-full border", login === a.login ? "border-ember" : "border-fg-3")}>
                      {login === a.login && <span className="size-2 rounded-full bg-ember" />}
                    </span>
                    <span className="min-w-0 flex-1">
                      <span className="block font-mono text-[13.5px]">#{a.login}</span>
                      <span className="block truncate text-[12px] text-fg-3">{a.eligible ? `${a.group} · ${t("social.mam.openPositions", { count: a.positions })}` : a.reason}</span>
                    </span>
                    <span className="k-num text-[13.5px]">{usd(a.equity)}</span>
                  </button>
                ))}
              </div>
            )}
          </div>

          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <Field label={t("social.mam.maxLotPerTrade")} hint={t("social.subs.settings.emptyNoCap")}>
              <Input type="number" inputMode="decimal" min={0.01} step={0.01} placeholder={t("social.noCap")} value={maxLot.raw} onChange={(e) => maxLot.setRaw(e.target.value)} trailing={t("social.lotsUnit")} inputClassName="k-num" />
            </Field>
            <Field label={t("social.equityStop")} hint={t("social.subs.settings.emptyOff")}>
              <Input type="number" inputMode="decimal" min={0} placeholder={t("common.off")} value={equityStop.raw} onChange={(e) => equityStop.setRaw(e.target.value)} leading="$" inputClassName="k-num" />
            </Field>
          </div>
          <p className="-mt-2 text-[12px] text-fg-3">{t("social.mam.connect.stopNote")}</p>

          <div>
            <div className="mb-2 flex items-center gap-2 text-[12.5px] font-medium text-fg-2">
              <FileText className="size-4 text-fg-3" /> {t("social.mam.connect.terms")}
            </div>
            <div className="max-h-56 overflow-y-auto rounded-[14px] border border-line bg-surface-2 px-4 py-3 text-[12.5px] leading-relaxed text-fg-2" data-testid="mam-terms">
              {data.terms.text.split(/(?=\d\. )/).map((p, i) => (
                <p key={i} className={i ? "mt-2" : ""}>
                  {p.trim()}
                </p>
              ))}
            </div>
          </div>
          <Checkbox checked={accept} onChange={setAccept}>
            {t("social.mam.connect.consent", { name: m.nickname ?? t("social.mam.theManager"), account: login ? `#${login}` : t("social.mam.theAccountIChose") })}
          </Checkbox>
        </div>
      )}
    </Dialog>
  );
}

/* ------------------------------------------------------------------ */
/* Link details, limits, revoke                                        */
/* ------------------------------------------------------------------ */

function LinkDrawer({ id, onClose }: { id: number | null; onClose: () => void }) {
  const t = useT();
  const { data, error, reload } = useSocial<LinkDetail>(id ? `mam/links/${id}` : null, 5000);
  if (!id) return null;
  const l = data?.link;
  const posCols: Column<LinkDetail["positions"][number]>[] = [
    { key: "t", header: t("social.mam.col.ticket"), cell: (p) => <span className="font-mono text-[12px] text-fg-2">#{p.ticket}</span> },
    { key: "s", header: t("social.col.symbol"), cell: (p) => <span className="font-medium">{p.symbol} <span className={p.side === "buy" ? "text-up" : "text-down"}>{t.dyn(`common.${p.side}`, p.side).toLowerCase()}</span></span> },
    { key: "v", header: t("social.col.lots"), align: "right", cell: (p) => <span className="k-num">{lots(p.volume)}</span> },
    { key: "o", header: t("social.col.open"), align: "right", cell: (p) => <span className="k-num text-fg-2">{fmtPrice(p.openPrice)}</span>, hideOn: "sm" },
    { key: "p", header: t("social.pnl"), align: "right", cell: (p) => <span className={cn("k-num", tone(p.profit ?? 0))}>{usd(p.profit ?? 0, 2, true)}</span> },
  ];
  const dealCols: Column<LinkDetail["deals"][number]>[] = [
    { key: "at", header: t("common.time"), cell: (d) => <span className="whitespace-nowrap text-fg-2">{serverTime(d.time)}</span> },
    { key: "s", header: t("social.mam.col.deal"), cell: (d) => <span>{d.symbol} <span className="text-fg-3">{d.entry === "in" ? t("social.mam.dealOpen") : t("social.mam.dealClose")} {t.dyn(`common.${d.side}`, d.side).toLowerCase()}</span></span> },
    { key: "v", header: t("social.col.lots"), align: "right", cell: (d) => <span className="k-num">{lots(d.volume)}</span> },
    { key: "p", header: t("social.inv.col.result"), align: "right", cell: (d) => (d.entry === "in" ? <span className="k-num text-fg-3">{d.commission ? usd(-d.commission, 2, true) : "—"}</span> : <span className={cn("k-num", tone(d.profit + d.swap))}>{usd(d.profit + d.swap, 2, true)}</span>) },
  ];
  return (
    <Dialog open={!!id} onOpenChange={(o) => !o && onClose()} side="right" title={l ? `${l.manager?.name ?? "MAM"} · #${l.login}` : t("social.mam.managedAccount")} description={l ? `${t("social.mam.linkedOn", { date: fmtDate(l.createdAt) })} · ${l.manager ? METHOD_LABEL[l.manager.method] : ""}` : undefined}>
      {error && !data ? (
        <SocialError onRetry={reload} message={error.message} title={t("social.mam.linkUnavailable")} />
      ) : !data || !l ? (
        <BlockSkeleton n={3} h={100} />
      ) : (
        <div className="space-y-5">
          <div className="grid grid-cols-2 gap-2">
            <Tile label={t("common.equity")}>{usd(l.equity)}</Tile>
            <Tile label={t("social.mam.result")}>
              <span className={tone(l.mamResult)}>{usd(l.mamResult, 2, true)}</span>
            </Tile>
            <Tile label={t("social.funds.explain.hwmT")}>{usd(l.hwm)}</Tile>
            <Tile label={t("social.mam.feesPaidPending")}>
              {usd(l.feesPaid)} / {usd(l.feesPending)}
            </Tile>
          </div>
          <div>
            <div className="mb-2 text-[12.5px] font-medium text-fg-2">{t("social.mam.openTrades")}</div>
            {data.positions.length ? <DataTable columns={posCols} rows={data.positions} dense pageSize={8} rowKey={(p) => String(p.ticket)} /> : <div className="k-row px-4 py-5 text-center text-[13px] text-fg-3">{t("social.mam.noOpenTrades")}</div>}
          </div>
          <div>
            <div className="mb-2 text-[12.5px] font-medium text-fg-2">{t("social.mam.tradeHistory")}</div>
            {data.deals.length ? <DataTable columns={dealCols} rows={data.deals} dense pageSize={8} rowKey={(d) => String(d.id)} /> : <div className="k-row px-4 py-5 text-center text-[13px] text-fg-3">{t("social.mam.noTrades")}</div>}
          </div>
          <div>
            <div className="mb-2 text-[12.5px] font-medium text-fg-2">{t("social.fees")}</div>
            <FeesTable fees={data.fees} empty={t("social.mam.noFees")} />
          </div>
          <div>
            <div className="mb-2 text-[12.5px] font-medium text-fg-2">{t("social.mam.activity")}</div>
            <div className="space-y-1.5">
              {data.log.slice(0, 20).map((e, i) => (
                <div key={i} className="k-row flex items-start gap-3 px-3 py-2 text-[12.5px]">
                  <span className="w-[120px] shrink-0 whitespace-nowrap text-fg-3">{serverTime(e.at, false)}</span>
                  <span className="min-w-0 flex-1 truncate">
                    <span className="font-medium capitalize">{t.dyn(`social.logAction.${e.action}`, e.action.replace(/_/g, " "))}</span> <span className="text-fg-3">{e.message}</span>
                  </span>
                  <Chip size="sm" tone={e.status === "done" ? "up" : e.status === "failed" ? "down" : "neutral"}>
                    {t.dyn(`social.logStatus.${e.status}`, e.status)}
                  </Chip>
                </div>
              ))}
              {!data.log.length && <div className="k-row px-4 py-5 text-center text-[13px] text-fg-3">{t("social.mam.nothingYet")}</div>}
            </div>
          </div>
          {data.terms && (
            <div>
              <div className="mb-2 text-[12.5px] font-medium text-fg-2">{t("social.mam.yourConsent", { date: fmtDate(l.consentAt) })}</div>
              <div className="max-h-48 overflow-y-auto rounded-[14px] border border-line bg-surface-2 px-4 py-3 text-[12px] leading-relaxed text-fg-3">{data.terms}</div>
            </div>
          )}
        </div>
      )}
    </Dialog>
  );
}

function LimitsDialog({ link, onClose, onSaved }: { link: LinkView | null; onClose: () => void; onSaved: () => void }) {
  const t = useT();
  const maxLot = useNumber(null);
  const equityStop = useNumber(null);
  const [busy, setBusy] = React.useState(false);
  React.useEffect(() => {
    if (!link) return;
    maxLot.set(link.maxLot);
    equityStop.set(link.equityStop);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [link?.id]);
  if (!link) return null;
  const maxErr = maxLot.raw && !(maxLot.value! >= 0.01 && maxLot.value! <= 100) ? t("social.mam.err.maxLot") : undefined;
  const stopErr = equityStop.raw && !(equityStop.value! > 0 && equityStop.value! < link.equity) ? t("social.mam.err.equityStopBelow") : undefined;
  const err = maxErr ?? stopErr;
  const save = async () => {
    setBusy(true);
    try {
      await socialApi(`mam/links/${link.id}`, { method: "PATCH", body: { maxLot: maxLot.value, equityStop: equityStop.value } });
      toast.success(t("social.mam.toast.limitsSaved"), { description: t("social.mam.toast.limitsSavedDesc") });
      onSaved();
      onClose();
    } catch (e) {
      toast.error(t("social.mam.toast.limitsFailed"), { description: e instanceof Error ? e.message : undefined });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog
      open={!!link}
      onOpenChange={(o) => !o && onClose()}
      title={t("social.mam.limits.title")}
      description={`${t("social.mam.accountNo", { login: link.login })} · ${link.manager?.name ?? ""}`}
      footer={
        <>
          <Button variant="ghost" size="sm" onClick={onClose} disabled={busy}>
            {t("common.cancel")}
          </Button>
          <Button variant="ember" onClick={save} disabled={busy || !!err}>
            {busy && <Loader2 className="animate-spin" />} {t("social.mam.limits.save")}
          </Button>
        </>
      }
    >
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <Field label={t("social.mam.maxLotPerTrade")} hint={t("social.subs.settings.emptyNoCap")} error={maxErr}>
          <Input type="number" inputMode="decimal" min={0.01} step={0.01} placeholder={t("social.noCap")} value={maxLot.raw} onChange={(e) => maxLot.setRaw(e.target.value)} trailing={t("social.lotsUnit")} inputClassName="k-num" />
        </Field>
        <Field label={t("social.equityStop")} hint={t("social.mam.equityHint", { amount: usd(link.equity) })} error={stopErr}>
          <Input type="number" inputMode="decimal" min={0} placeholder={t("common.off")} value={equityStop.raw} onChange={(e) => equityStop.setRaw(e.target.value)} leading="$" inputClassName="k-num" />
        </Field>
      </div>
    </Dialog>
  );
}

function RevokeDialog({ link, onClose, onDone }: { link: LinkView | null; onClose: () => void; onDone: () => void }) {
  const t = useT();
  const [close, setClose] = React.useState(false);
  const [busy, setBusy] = React.useState(false);
  React.useEffect(() => setClose(false), [link?.id]);
  if (!link) return null;
  const open = link.mamPositions + link.mamOrders;
  const revoke = async () => {
    setBusy(true);
    try {
      const r = await socialApi<{ closed: number[]; failed: unknown[]; fee: number | null }>(`mam/links/${link.id}/revoke`, { body: { closePositions: close } });
      toast.success(t("social.mam.toast.revoked"), {
        description: `${t("social.mam.toast.revokedDesc", { name: link.manager?.name ?? t("social.mam.theManagerCap"), login: link.login })}${close ? ` ${t("social.mam.toast.tradesClosed", { count: r.closed.length })}` : ""}${r.fee ? ` ${t("social.mam.toast.feesSettled", { amount: usd(r.fee) })}` : ""}`,
      });
      onDone();
      onClose();
    } catch (e) {
      toast.error(t("social.mam.toast.revokeFailed"), { description: e instanceof Error ? e.message : undefined });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog
      open={!!link}
      onOpenChange={(o) => !o && onClose()}
      title={t("social.mam.revoke.title")}
      description={`${link.manager?.name ?? "MAM"} · ${t("social.mam.accountLower", { login: link.login })}`}
      footer={
        <>
          <Button variant="ghost" size="sm" onClick={onClose} disabled={busy}>
            {t("common.cancel")}
          </Button>
          <Button variant="sell" onClick={revoke} disabled={busy}>
            {busy && <Loader2 className="animate-spin" />} {t("social.mam.revoke.now")}
          </Button>
        </>
      }
    >
      <div className="space-y-4 text-[13px] text-fg-2">
        <p>{t("social.mam.revoke.text")}</p>
        {open > 0 ? (
          <div className="space-y-2">
            <Checkbox checked={close} onChange={setClose}>
              {t("social.mam.revoke.closeTrades", { count: open })}
            </Checkbox>
            <p className="ps-[30px] text-[12px] text-fg-3">{t("social.mam.revoke.keepNote")}</p>
          </div>
        ) : (
          <p className="text-fg-3">{t("social.mam.revoke.noTrades")}</p>
        )}
      </div>
    </Dialog>
  );
}

/* ------------------------------------------------------------------ */
/* Page                                                                */
/* ------------------------------------------------------------------ */

function LinkCard({ l, onDetails, onLimits, onRevoke }: { l: LinkView; onDetails: () => void; onLimits: () => void; onRevoke: () => void }) {
  const t = useT();
  const active = l.status === "active";
  return (
    <div className="k-row grid grid-cols-1 gap-5 px-4 py-4 md:grid-cols-[minmax(0,320px)_minmax(0,1fr)]" data-testid={`mam-link-${l.id}`}>
      <VisitingCard
        finish="graphite"
        kicker={`MAM · ${l.manager ? METHOD_LABEL[l.manager.method] : ""}`}
        name={l.manager?.name ?? t("social.mam.programme")}
        title={l.manager?.nickname}
        stats={[
          { label: t("common.equity"), value: usd(l.equity) },
          { label: t("social.mam.result"), value: usd(l.mamResult, 2, true), tone: l.mamResult > 0 ? "up" : l.mamResult < 0 ? "down" : undefined },
        ]}
      />
      <div className="min-w-0">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex min-w-0 items-center gap-3">
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <span className="truncate text-[14px] font-medium">{l.manager?.name ?? t("social.mam.programme")}</span>
              <StatusChip status={l.status} label={active ? t("common.active") : l.status === "revoked" ? t("social.mam.revoked") : STOP_REASON[l.stopReason ?? ""] ?? t("social.subStatus.stopped")} />
            </div>
            <div className="truncate text-[12px] text-fg-3">
              {l.manager?.nickname} · <Trans k="social.mam.linkSub" vars={{ login: l.login, date: fmtDate(l.createdAt) }} tags={{ acc: (c) => <span className="font-mono text-fg-2">{c}</span> }} />
            </div>
          </div>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button size="sm" variant="surface" onClick={onDetails}>
            {t("common.details")}
          </Button>
          {active && (
            <>
              <Button size="sm" variant="surface" onClick={onLimits}>
                <Settings2 /> {t("social.mam.limits")}
              </Button>
              <Button size="sm" variant="down-outline" onClick={onRevoke}>
                <Unlink /> {t("social.mam.revoke")}
              </Button>
            </>
          )}
          <TradeButton a={{ login: Number(l.login), status: "active" }} />
        </div>
      </div>
      <div className="mt-3 grid grid-cols-2 gap-2 sm:grid-cols-4">
        <Tile label={t("social.mam.openTrades")}>
          {l.mamPositions}
          {l.mamOrders ? <span className="text-fg-3"> {t("social.mam.plusPending", { n: l.mamOrders })}</span> : null}
        </Tile>
        <Tile label={t("social.fees")}>{`${l.perfFeePct}%${l.mgmtFeePct ? ` + ${l.mgmtFeePct}%/y` : ""}`}</Tile>
        <Tile label={t("social.mam.maxLotEquityStop")}>
          {l.maxLot ?? "—"} · {l.equityStop ? usd(l.equityStop, 0) : "—"}
        </Tile>
        <Tile label={t("social.inv.kpi.feesPaid")}>{usd(l.feesPaid)}</Tile>
      </div>
      </div>
    </div>
  );
}

export function LiveManagedPage() {
  const t = useT();
  const links = useSocial<{ items: LinkView[]; accounts: Candidate[] }>("mam/links", 10000);
  const managers = useSocial<{ items: ManagerView[] }>("mam/managers");
  const [connect, setConnect] = React.useState<number | null>(null);
  const [detail, setDetail] = React.useState<number | null>(null);
  const [limits, setLimits] = React.useState<LinkView | null>(null);
  const [revoke, setRevoke] = React.useState<LinkView | null>(null);
  const items = links.data?.items ?? [];
  const active = items.filter((l) => l.status === "active");
  const ended = items.filter((l) => l.status !== "active");
  const linkedTo = new Set(active.map((l) => l.managerId));

  // the programmes as visiting cards (founder 2026-10-10)
  const managerCard = (m: ManagerView) => {
    const r = m.track?.return1y ?? 0;
    return (
      <div key={m.id} className="flex flex-col">
        <VisitingCard
          finish="graphite"
          kicker={`MAM · ${METHOD_LABEL[m.method]}`}
          name={m.name}
          title={m.nickname ?? undefined}
          stats={[
            { label: t("social.lb.col.return", { period: t("social.lb.period.1y") }), value: pct(r, 1), tone: r > 0 ? "up" : r < 0 ? "down" : undefined },
            { label: t("social.maxDd"), value: `${(m.track?.maxDd ?? 0).toFixed(1)}%` },
            { label: t("social.fees"), value: `${m.perfFeePct}%${m.mgmtFeePct ? `+${m.mgmtFeePct}` : ""}` },
            { label: t("common.accounts"), value: String(m.accounts) },
          ]}
        />
        <div className="mt-3 flex items-center gap-2">
          <span className="min-w-0 flex-1 truncate text-[12px] text-fg-3" title={METHOD_HINT[m.method]}>
            {METHOD_HINT[m.method]}
          </span>
          {linkedTo.has(m.id) ? (
            <Chip size="sm" tone="up">
              {t("social.mam.linked")}
            </Chip>
          ) : (
            <Button size="sm" variant="ember" onClick={() => setConnect(m.id)} data-testid={`mam-connect-${m.id}`}>
              {t("social.mam.connect")}
            </Button>
          )}
        </div>
      </div>
    );
  };

  return (
    <div className="pb-24">
      <PageHeader
        title={t("social.mam.page.title")}
        subtitle={t("social.mam.page.subtitle")}
        actions={
          <Link href="/social/mam">
            <Button variant="surface">
              <Briefcase /> {t("social.mam.page.run")} <ArrowUpRight className="rtl:-scale-x-100" />
            </Button>
          </Link>
        }
      />

      <InfoBox icon={<ShieldCheck />} className="mb-4">
        {t("social.mam.page.info")}
      </InfoBox>

      <Card className="mb-4">
        <CardHeader title={t("social.mam.page.yours")} subtitle={active.length ? t("social.mam.page.activeLinks", { count: active.length }) : t("social.mam.page.noneManaged")} icon={<Users />} />
        <div className="space-y-2.5 px-4 pb-5 pt-4 sm:px-6">
          {links.error && !links.data ? (
            <SocialError onRetry={links.reload} message={links.error.message} title={t("social.mam.page.unavailable")} />
          ) : !links.data ? (
            <BlockSkeleton n={1} h={120} />
          ) : items.length === 0 ? (
            <div className="k-row px-4 py-8 text-center text-[13px] text-fg-3">{t("social.mam.page.connectHint")}</div>
          ) : (
            <>
              {active.map((l) => (
                <LinkCard key={l.id} l={l} onDetails={() => setDetail(l.id)} onLimits={() => setLimits(l)} onRevoke={() => setRevoke(l)} />
              ))}
              {ended.length > 0 && <div className="pt-2 text-[12px] uppercase tracking-wider text-fg-3">{t("social.mam.page.ended")}</div>}
              {ended.map((l) => (
                <LinkCard key={l.id} l={l} onDetails={() => setDetail(l.id)} onLimits={() => undefined} onRevoke={() => undefined} />
              ))}
            </>
          )}
        </div>
      </Card>

      <Card>
        <CardHeader title={t("social.mam.page.programmes")} subtitle={t("social.mam.page.programmesSub")} icon={<Briefcase />} />
        <div className="px-4 pb-5 pt-4 sm:px-6">
          {managers.error && !managers.data ? (
            <SocialError onRetry={managers.reload} message={managers.error.message} title={t("social.mam.page.programmesUnavailable")} />
          ) : !managers.data ? (
            <BlockSkeleton n={2} h={60} />
          ) : managers.data.items.length === 0 ? (
            <EmptyState art="copyTrading" title={t("social.mam.page.noProgrammes")} text={t("social.mam.page.noProgrammesText")} />
          ) : (
            <div className="grid grid-cols-1 gap-x-5 gap-y-7 md:grid-cols-2 xl:grid-cols-3">{managers.data.items.map(managerCard)}</div>
          )}
        </div>
      </Card>

      <ConnectDialog managerId={connect} onClose={() => setConnect(null)} onLinked={links.reload} />
      <LinkDrawer id={detail} onClose={() => setDetail(null)} />
      <LimitsDialog link={limits} onClose={() => setLimits(null)} onSaved={links.reload} />
      <RevokeDialog link={revoke} onClose={() => setRevoke(null)} onDone={links.reload} />
    </div>
  );
}
