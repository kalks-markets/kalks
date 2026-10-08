"use client";

import * as React from "react";
import dynamic from "next/dynamic";
import { useTheme } from "next-themes";
import { toast } from "@/lib/notify";
import { ArrowUpRight, BarChart2, CandlestickChart, ChevronDown, History, Languages, List, LogOut, Moon, RefreshCw, Search, Sun, UserRound, Wallet, X, Zap } from "lucide-react";
import { INSTRUMENTS, getInstrument } from "@kalks/mock";
import { useMarketScope } from "@/lib/scope";
import { LogoMark, PriceText, SymbolAvatar, cn, useQuote } from "@kalks/ui";
import { useMetrics, usePositionProfit, useTerminal } from "@/lib/store";
import { useT } from "@kalks/i18n/react";
import { LanguageMenu } from "@/components/shell/language-menu";
import { useMarketOpen } from "@/lib/market-hours";
import { PENDING_LABEL, TIMEFRAMES, accCcy, accMoney, fmtPrice, fmtServer, fmtVol, marginState } from "@/lib/trading";
import { Badge, LiveMoney, MiniSwitch, Pnl, Stepper } from "@/components/ui/primitives";
import { ChartView } from "@/components/chart/chart-view";
import { CLIENT_AREA } from "@/components/shell/title-bar";
import { GuestActions, GuestNotice } from "@/components/shell/guest";
import { SegmentChips, inSegment } from "@/components/market/segments";
import { NotificationBell } from "@/components/shell/notifications";
import { REGISTER_URL } from "@/lib/guest";
import { DomLadder } from "@/components/order/dom-ladder";
import { ModeSwitch, useAccountProducts } from "@/components/shell/mode-switch";
import { groupAccounts, productOf } from "@/lib/options/product";
import type { EngineTradingAccount } from "@/lib/engine/map";
import { useTradeMode } from "@/lib/options/mode";

// Options mode: its own chunk, downloaded the first time a trader switches to Options
const OptionsMobile = dynamic(() => import("@/components/options/mobile").then((m) => m.OptionsMobile), { ssr: false, loading: () => <div className="h-full animate-pulse bg-panel" /> });

type MTab = "watch" | "chart" | "trade" | "history" | "account";

/** cTrader-mobile-like layout for < 1024px: content + bottom tab bar. */
export function MobileTerminal() {
  const T = useTerminal();
  const t = useT();
  const [tab, setTab] = React.useState<MTab>("chart");
  // toasts: below the header and the chart's symbol/timeframe strip (see providers.tsx)
  React.useEffect(() => {
    const root = document.documentElement.style;
    root.setProperty("--t-toast-top-m", "92px");
    return () => void root.removeProperty("--t-toast-top-m");
  }, []);
  const a = T.account;
  const m = useMetrics();
  const options = useTradeMode() === "options";
  const tabs: { id: MTab; label: string; icon: React.ReactNode }[] = [
    { id: "watch", label: t("trader.mobile.tab.watch"), icon: <List /> },
    { id: "chart", label: t("trader.mobile.tab.chart"), icon: <CandlestickChart /> },
    { id: "trade", label: t("trader.mobile.tab.trade"), icon: <BarChart2 /> },
    { id: "history", label: t("trader.mobile.tab.history"), icon: <History /> },
    { id: "account", label: t("trader.mobile.tab.account"), icon: <UserRound /> },
  ];
  // dir="ltr": same as the desktop workspace (see desktop.tsx). Bid/ask, Sell/Buy and the chart keep
  // their positions in Arabic/Urdu/Persian; only the text is translated.
  return (
    <div dir="ltr" className="flex h-dvh flex-col overflow-hidden bg-page">
      <header className="flex h-12 shrink-0 items-center gap-2 border-b border-line bg-panel px-3">
        <span className="hidden size-7 shrink-0 place-items-center rounded-[7px] border border-line-top bg-surface-3 min-[420px]:grid">
          <LogoMark size={12} className="text-fg" />
        </span>
        <ModeSwitch size="sm" />
        {T.guest ? (
          <>
            <div className="min-w-0 leading-tight">
              <div className="flex items-center gap-1.5 text-[12.5px] font-semibold">
                Kalks Trader <Badge>{t("trader.guest.badge")}</Badge>
              </div>
              <div className="truncate text-[10.5px] text-fg-3">{t("trader.mobile.guestSubtitle")}</div>
            </div>
            <NotificationBell className="ml-auto" size="sm" />
            <a href={REGISTER_URL} target="_blank" rel="noreferrer" className="flex h-8 shrink-0 items-center rounded-[7px] bg-ember px-3 text-[12px] font-semibold text-white">
              {t("trader.guest.openAccount")}
            </a>
          </>
        ) : (
          <>
        <div className="min-w-0 leading-tight">
          <div className="flex items-center gap-1.5 text-[12.5px] font-semibold">
            <span className="font-mono">{a.login}</span>
            <Badge tone={a.type === "live" ? "ember" : "gold"}>{t.dyn(`trader.accountType.${a.type}`, a.type)}</Badge>
            {T.readOnly && <Badge tone="warn">{t("trader.badge.readOnly")}</Badge>}
          </div>
          <div className="truncate text-[10.5px] text-fg-3">
            {a.group} · {a.mode} · {a.server}
          </div>
        </div>
        <div className="ml-auto text-right leading-tight">
          <div className="font-mono text-[13px] font-semibold"><LiveMoney value={m.equity} format={(v) => accMoney(a, v)} /></div>
          <div className="font-mono text-[10.5px]">
            <Pnl value={m.floating} text={accMoney(a, m.floating, { signed: true })} format={(v) => accMoney(a, v, { signed: true })} /> <span className="text-fg-3">{accCcy(a)}</span>
          </div>
        </div>
        <NotificationBell size="sm" className="-mr-1" />
          </>
        )}
      </header>
      <main className="min-h-0 flex-1 overflow-hidden">
        {options ? (
          <OptionsMobile />
        ) : (
          <>
        {tab === "watch" && <MWatch onPick={() => setTab("chart")} />}
        {tab === "chart" && <MChart />}
        {tab === "trade" && (T.guest ? <GuestNotice icon={<BarChart2 />} text={t("trader.mobile.guestTrade")} /> : <MTrade />)}
        {tab === "history" && (T.guest ? <GuestNotice icon={<History />} text={t("trader.mobile.guestHistory")} /> : <MHistory />)}
        {tab === "account" && (T.guest ? <MGuestAccount /> : <MAccount />)}
          </>
        )}
      </main>
      <nav className={cn("h-[58px] shrink-0 grid-cols-5 border-t border-line bg-panel pb-[env(safe-area-inset-bottom)]", options ? "hidden" : "grid")}>
        {tabs.map((x) => (
          <button key={x.id} onClick={() => setTab(x.id)} className={cn("relative flex flex-col items-center justify-center gap-0.5 text-[10.5px] [&_svg]:size-[18px]", tab === x.id ? "text-ember" : "text-fg-3")}>
            {tab === x.id && <span className="absolute inset-x-5 top-0 h-[2px] rounded-full bg-ember" />}
            {x.icon}
            {x.label}
            {x.id === "trade" && T.positions.length > 0 && <span className="absolute right-[22%] top-1.5 grid h-3.5 min-w-3.5 place-items-center rounded-full bg-ember px-1 font-mono text-[9px] text-white">{T.positions.length}</span>}
          </button>
        ))}
      </nav>
    </div>
  );
}

function MWatch({ onPick }: { onPick: () => void }) {
  const T = useTerminal();
  const t = useT();
  const [q, setQ] = React.useState("");
  const seg = T.ws.mwSegment;
  // browsing: the core markets (and favourites); a search looks through the whole catalogue (1,400+), first 60 matches
  const scope = useMarketScope();
  const list = q
    ? scope.list.filter((i) => inSegment(i, seg, T.ws.favourites)).filter((i) => i.symbol.toLowerCase().includes(q.toLowerCase()) || i.name.toLowerCase().includes(q.toLowerCase())).slice(0, 60)
    : (seg === "favourites" ? scope.list : INSTRUMENTS).filter((i) => inSegment(i, seg, T.ws.favourites));
  return (
    <div className="flex h-full flex-col">
      <div className="space-y-1.5 p-2">
        <SegmentChips instruments={INSTRUMENTS} value={seg} onChange={(s) => T.setWs({ mwSegment: s })} favourites={T.ws.favourites} size="md" label={t("trader.mobile.watchSegment")} />
        <label className="flex h-9 items-center gap-2 rounded-[8px] border border-line bg-surface-2 px-3">
          <Search className="size-4 text-fg-3" />
          <input value={q} onChange={(e) => setQ(e.target.value)} placeholder={t("trader.searchSymbols")} className="min-w-0 flex-1 bg-transparent text-[13px] outline-none placeholder:text-fg-3" />
        </label>
      </div>
      <div className="t-scroll min-h-0 flex-1 overflow-y-auto">
        {list.map((i) => (
          <MWatchRow key={i.symbol} symbol={i.symbol} active={T.activeSymbol === i.symbol} onPick={() => (T.openSymbol(i.symbol), onPick())} />
        ))}
        {!list.length && <div className="p-6 text-center text-[12.5px] text-fg-3">{seg === "favourites" && !q ? t("trader.mobile.noFavourites") : t("trader.mobile.noSymbols")}</div>}
      </div>
    </div>
  );
}

function MWatchRow({ symbol, active, onPick }: { symbol: string; active: boolean; onPick: () => void }) {
  const q = useQuote(symbol);
  const inst = getInstrument(symbol);
  return (
    <button onClick={onPick} className={cn("flex w-full items-center gap-2.5 border-b border-line/60 px-3 py-2 text-left", active && "bg-ember-soft/50")}>
      <SymbolAvatar symbol={symbol} size={22} />
      <div className="min-w-0 flex-1">
        <div className="text-[13px] font-medium">{symbol}</div>
        <div className={cn("k-num font-mono text-[10.5px]", q.change >= 0 ? "text-up" : "text-down")}>
          {q.change >= 0 ? "+" : ""}
          {q.change.toFixed(2)}% <span className="font-sans text-fg-3">· {inst.name}</span>
        </div>
      </div>
      <div className="grid grid-cols-2 gap-1">
        <span className="min-w-[78px] rounded-[5px] bg-down-soft px-1.5 py-1 text-right">
          <PriceText symbol={symbol} value={q.bid} dir={q.dir} pulse className="justify-end px-0.5 text-[12px]" />
        </span>
        <span className="min-w-[78px] rounded-[5px] bg-up-soft px-1.5 py-1 text-right">
          <PriceText symbol={symbol} value={q.ask} dir={q.dir} pulse className="justify-end px-0.5 text-[12px]" />
        </span>
      </div>
    </button>
  );
}

function MChart() {
  const T = useTerminal();
  const t = useT();
  const tab = T.activeTab;
  const q = useQuote(tab.symbol);
  const [vol, setVol] = React.useState(T.ws.lot.toFixed(2));
  const v = Math.max(0.01, parseFloat(vol) || 0.01);
  const marketOpen = useMarketOpen(tab.symbol);
  const trade = (side: "buy" | "sell") => {
    if (!marketOpen) return;
    if (T.guest) return void T.quickTrade(tab.symbol, side, v); // explains: no trading account yet
    if (T.ws.oneClick) T.quickTrade(tab.symbol, side, v);
    else T.openNewOrder({ symbol: tab.symbol, side, type: "market" });
  };
  return (
    <div className="flex h-full flex-col">
      <div className="flex h-9 shrink-0 items-center gap-1 overflow-x-auto border-b border-line px-2 [scrollbar-width:none]">
        <button onClick={() => T.setUi({ search: true })} className="flex h-7 shrink-0 items-center gap-1.5 rounded-[6px] px-2 text-[12.5px] font-semibold">
          <SymbolAvatar symbol={tab.symbol} size={14} /> {tab.symbol}
        </button>
        <span className="h-4 w-px shrink-0 bg-line" />
        {TIMEFRAMES.map((tf) => (
          <button key={tf} onClick={() => T.updateTab(tab.id, { tf })} className={cn("h-7 shrink-0 rounded-[5px] px-2 font-mono text-[11px]", tab.tf === tf ? "bg-ember-soft text-ember" : "text-fg-3")}>
            {tf}
          </button>
        ))}
      </div>
      <div className="min-h-0 flex-1 p-1">
        <ChartView tab={tab} active={false} onActivate={() => {}} compact hideOneClick />
      </div>
      {!T.readOnly && (
        <div className="grid shrink-0 grid-cols-[1fr_110px_1fr] gap-1.5 border-t border-line bg-panel p-2">
          <button onClick={() => trade("sell")} disabled={!marketOpen} title={T.guest ? t("trader.guest.title") : undefined} className={cn("rounded-[8px] bg-down px-2 py-1.5 text-left text-white disabled:bg-surface-3 disabled:text-fg-3 [&:disabled_span]:!text-fg-3", T.guest && "border border-down/40 bg-down/15 text-down")}>
            <div className="text-[9.5px] font-semibold uppercase tracking-[0.1em] opacity-85">{t("common.sell")}</div>
            <PriceText symbol={tab.symbol} value={q.bid} dir={q.dir} className={cn("text-[15px]", !T.guest && "[&_span]:!text-white")} />
          </button>
          <div className="flex flex-col justify-center gap-1">
            <Stepper ariaLabel={t("trader.mobile.volume")} value={vol} onChange={setVol} step={0.01} min={0.01} decimals={2} className="h-8" />
            <div className={cn("text-center font-mono text-[9.5px]", marketOpen ? "text-fg-3" : "text-warn")}>{!marketOpen ? t("trader.mobile.marketClosed") : T.guest ? t("trader.mobile.needsAccount") : T.ws.oneClick ? t("trader.mobile.oneClick") : t("trader.mobile.confirm")}</div>
          </div>
          <button onClick={() => trade("buy")} disabled={!marketOpen} title={T.guest ? t("trader.guest.title") : undefined} className={cn("rounded-[8px] bg-up px-2 py-1.5 text-right text-white disabled:bg-surface-3 disabled:text-fg-3 [&:disabled_span]:!text-fg-3", T.guest && "border border-up/40 bg-up/15 text-up")}>
            <div className="text-[9.5px] font-semibold uppercase tracking-[0.1em] opacity-85">{t("common.buy")}</div>
            <PriceText symbol={tab.symbol} value={q.ask} dir={q.dir} className={cn("justify-end text-[15px]", !T.guest && "[&_span]:!text-white")} />
          </button>
        </div>
      )}
    </div>
  );
}

function MTrade() {
  const T = useTerminal();
  const t = useT();
  const m = useMetrics();
  const a = T.account;
  const [depth, setDepth] = React.useState(false);
  return (
    <div className="t-scroll h-full overflow-y-auto">
      <div className="grid grid-cols-3 gap-px border-b border-line bg-line">
        {[
          ["balance", t("common.balance"), accMoney(a, m.balance)],
          ["equity", t("common.equity"), accMoney(a, m.equity)],
          ["free", t("trader.mobile.freeMargin"), accMoney(a, m.free)],
          ["margin", t("trader.mobile.margin"), accMoney(a, m.margin)],
          ["level", t("trader.mobile.level"), Number.isFinite(m.level) ? `${m.level.toFixed(0)}%` : "—"],
          ["pnl", t("trader.status.pnl"), accMoney(a, m.floating, { signed: true })],
        ].map(([k, label, v]) => (
          <div key={k} className="bg-panel px-3 py-2">
            <div className="text-[10px] uppercase tracking-[0.06em] text-fg-3">{label}</div>
            <div className={cn("k-num font-mono text-[12.5px]", k === "pnl" && (m.floating >= 0 ? "text-up" : "text-down"), k === "level" && { ok: "", low: "text-warn", call: "text-down", stopout: "text-down" }[marginState(m.level, a)], k === "free" && m.free < 0 && "text-down")}>{v}</div>
          </div>
        ))}
      </div>
      {/* depth of market with one-click trading (D97), same ladder as the desktop panel */}
      <div className="border-b border-line">
        <button onClick={() => setDepth((v) => !v)} aria-expanded={depth} className="flex w-full items-center justify-between px-3 py-2.5 text-[10.5px] font-semibold uppercase tracking-[0.08em] text-fg-3">
          <span>
            {t("order.panel.tabDepth")} · <span className="normal-case tracking-normal text-fg-2">{T.activeSymbol}</span>
          </span>
          <ChevronDown className={cn("size-4 transition-transform", depth && "rotate-180")} />
        </button>
        {depth && (
          <div className="h-[560px]">
            <DomLadder symbol={T.activeSymbol} />
          </div>
        )}
      </div>
      <div className="flex items-center justify-between px-3 pb-1 pt-3 text-[10.5px] font-semibold uppercase tracking-[0.08em] text-fg-3">
        {t("trader.mobile.positions", { count: T.positions.length })}
        {!T.readOnly && T.positions.length > 0 && (
          <button onClick={() => T.bulkClose("all")} className="rounded-[5px] border border-line px-2 py-0.5 text-[10.5px] normal-case tracking-normal text-down">
            {t("trader.mobile.closeAll")}
          </button>
        )}
      </div>
      {T.positions.map((p) => (
        <MPosition key={p.ticket} ticket={p.ticket} />
      ))}
      {!T.positions.length && <div className="px-3 py-4 text-center text-[12px] text-fg-3">{t("trader.mobile.noPositions")}</div>}
      <div className="px-3 pb-1 pt-3 text-[10.5px] font-semibold uppercase tracking-[0.08em] text-fg-3">{t("trader.mobile.pendingOrders", { count: T.pendings.length })}</div>
      {T.pendings.map((o) => (
        <div key={o.ticket} className="flex items-center gap-2.5 border-b border-line/60 px-3 py-2">
          <SymbolAvatar symbol={o.symbol} size={18} />
          <div className="min-w-0 flex-1">
            <div className="text-[12.5px] font-medium">
              {o.symbol} <span className={o.side === "buy" ? "text-up" : "text-down"}>{PENDING_LABEL(o)}</span>
            </div>
            <div className="font-mono text-[10.5px] text-fg-3">
              {t("trader.mobile.volumeAt", { volume: fmtVol(o.volume), price: fmtPrice(o.symbol, o.price) })} · {o.expiry}
            </div>
          </div>
          {!T.readOnly && (
            <button onClick={() => T.cancelPending(o.ticket)} className="grid size-7 place-items-center rounded-[6px] border border-line text-fg-3" aria-label={t("trader.mobile.cancelOrder")}>
              <X className="size-3.5" />
            </button>
          )}
        </div>
      ))}
    </div>
  );
}

function MPosition({ ticket }: { ticket: string }) {
  const T = useTerminal();
  const t = useT();
  const p = T.positions.find((x) => x.ticket === ticket)!;
  const q = useQuote(p.symbol);
  const pr = usePositionProfit(p);
  return (
    <div className="flex items-center gap-2.5 border-b border-line/60 px-3 py-2" onClick={() => !T.readOnly && T.setUi({ positionDialog: p.ticket })}>
      <SymbolAvatar symbol={p.symbol} size={20} />
      <div className="min-w-0 flex-1">
        <div className="text-[12.5px] font-medium">
          {p.symbol} <span className={p.side === "buy" ? "text-up" : "text-down"}>{t.dyn(`trader.side.${p.side}`, p.side)}</span> <span className="font-mono text-fg-2">{fmtVol(p.volume)}</span>
        </div>
        <div className="font-mono text-[10.5px] text-fg-3">
          {fmtPrice(p.symbol, p.openPrice)} → {fmtPrice(p.symbol, p.side === "buy" ? q.bid : q.ask)}
        </div>
      </div>
      <Pnl value={pr} text={accMoney(T.account, pr, { signed: true })} format={(v) => accMoney(T.account, v, { signed: true })} arrow className="text-[13px] font-semibold" />
      {!T.readOnly && (
        <button onClick={(e) => (e.stopPropagation(), T.closePosition(p.ticket))} className="grid size-7 place-items-center rounded-[6px] border border-line text-fg-3" aria-label={t("trader.mobile.closePosition")}>
          <X className="size-3.5" />
        </button>
      )}
    </div>
  );
}

function MHistory() {
  const T = useTerminal();
  const t = useT();
  const rows = T.history.slice(0, 80);
  return (
    <div className="t-scroll h-full overflow-y-auto">
      {rows.map((h) => (
        <div key={`${h.ticket}-${h.closeTime}`} className="flex items-center gap-2.5 border-b border-line/60 px-3 py-2">
          <SymbolAvatar symbol={h.symbol} size={18} />
          <div className="min-w-0 flex-1">
            <div className="text-[12.5px] font-medium">
              {h.symbol} <span className={h.side === "buy" ? "text-up" : "text-down"}>{t.dyn(`trader.side.${h.side}`, h.side)}</span> <span className="font-mono text-fg-2">{fmtVol(h.volume)}</span>
            </div>
            <div className="truncate font-mono text-[10.5px] text-fg-3">
              {fmtServer(h.closeTime, false)} · {fmtPrice(h.symbol, h.openPrice)} → {fmtPrice(h.symbol, h.closePrice)}
            </div>
          </div>
          <Pnl value={h.profit} text={accMoney(T.account, h.profit, { signed: true })} className="text-[12.5px] font-semibold" />
        </div>
      ))}
    </div>
  );
}

function MAccount() {
  const T = useTerminal();
  const t = useT();
  const { resolvedTheme, setTheme } = useTheme();
  const products = useAccountProducts();
  const groups = groupAccounts(T.accounts.map((x, i) => ({ ...x, product: products[i]?.product })));
  return (
    <div className="t-scroll h-full space-y-3 overflow-y-auto p-3">
      {/* CFD / Options account split: the accounts by product once this terminal holds both */}
      {groups.map((g) => (
        <React.Fragment key={g.product}>
          <div className="text-[10.5px] font-semibold uppercase tracking-[0.08em] text-fg-3">{groups.length > 1 ? t(g.product === "options" ? "accounts.product.groupOptions" : "accounts.product.groupCfd") : t("common.accounts")}</div>
          <div className="overflow-hidden rounded-[8px] border border-line bg-panel">
            {g.accounts.map((x) => (
              <MAccountRow key={x.login} login={x.login} />
            ))}
          </div>
        </React.Fragment>
      ))}
      <div className="overflow-hidden rounded-[8px] border border-line bg-panel">
        <Row label={t("trader.oneClick.name")} icon={<Zap />}>
          <MiniSwitch checked={T.ws.oneClick} onChange={(v) => T.setWs({ oneClick: v })} label={t("trader.oneClick.name")} />
        </Row>
        <Row label={t("trader.mobile.soundOnFills")}>
          <MiniSwitch checked={T.ws.sound} onChange={(v) => T.setWs({ sound: v })} label={t("trader.mobile.sound")} />
        </Row>
        <Row label={t("trader.mobile.darkTheme")} icon={resolvedTheme === "light" ? <Sun /> : <Moon />}>
          <MiniSwitch checked={resolvedTheme !== "light"} onChange={(v) => setTheme(v ? "dark" : "light")} label={t("trader.menu.theme")} />
        </Row>
        <Row label={t("common.language")} icon={<Languages />}>
          <LanguageMenu size="sm" className="-me-1.5" />
        </Row>
        {T.account.type === "demo" && (
          <Row label={t("trader.account.refillDemo", { count: T.refillsLeft })} icon={<RefreshCw />}>
            <button onClick={T.refillDemo} className="rounded-[5px] border border-gold/40 px-2 py-0.5 text-[11px] text-gold">
              {t("trader.mobile.refill")}
            </button>
          </Row>
        )}
      </div>
      <a href={CLIENT_AREA} target="_blank" rel="noreferrer" className="flex h-10 items-center justify-center gap-1.5 rounded-[8px] border border-line bg-panel text-[13px]">
        <Wallet className="size-4" /> {t("trader.clientArea")} <ArrowUpRight className="size-3.5" />
      </a>
      <button onClick={() => (toast(t("trader.toast.loggedOut")), T.logout())} className="flex h-10 w-full items-center justify-center gap-1.5 rounded-[8px] border border-down/30 bg-down-soft text-[13px] text-down">
        <LogOut className="size-4" /> {t("trader.menu.logOut")}
      </button>
    </div>
  );
}

function MGuestAccount() {
  const t = useT();
  const { resolvedTheme, setTheme } = useTheme();
  return (
    <div className="t-scroll h-full space-y-3 overflow-y-auto p-3">
      <div className="rounded-[8px] border border-line bg-panel px-4 py-4 text-center">
        <Badge>{t("trader.guest.badge")}</Badge>
        <div className="mt-2 text-[13.5px] font-semibold">{t("trader.guest.title")}</div>
        <p className="mt-1 text-[12px] leading-relaxed text-fg-3">{t("trader.mobile.guestAccountText")}</p>
        <GuestActions size="md" className="mt-3" />
      </div>
      <div className="overflow-hidden rounded-[8px] border border-line bg-panel">
        <Row label={t("trader.mobile.darkTheme")} icon={resolvedTheme === "light" ? <Sun /> : <Moon />}>
          <MiniSwitch checked={resolvedTheme !== "light"} onChange={(v) => setTheme(v ? "dark" : "light")} label={t("trader.menu.theme")} />
        </Row>
        <Row label={t("common.language")} icon={<Languages />}>
          <LanguageMenu size="sm" className="-me-1.5" />
        </Row>
      </div>
      <a href={CLIENT_AREA} target="_blank" rel="noreferrer" className="flex h-10 items-center justify-center gap-1.5 rounded-[8px] border border-line bg-panel text-[13px]">
        <Wallet className="size-4" /> {t("trader.clientArea")} <ArrowUpRight className="size-3.5" />
      </a>
    </div>
  );
}

function MAccountRow({ login }: { login: string }) {
  const T = useTerminal();
  const m = useMetrics(login);
  const t = useT();
  const a = m.account;
  const on = login === T.account.login;
  return (
    <button onClick={() => T.switchAccount(login)} className={cn("flex w-full items-center gap-2.5 border-b border-line/60 px-3 py-2.5 text-left last:border-b-0", on && "bg-ember-soft/50")}>
      <Badge tone={a.type === "live" ? "ember" : "gold"} className="w-11 justify-center">
        {t.dyn(`trader.accountType.${a.type}`, a.type)}
      </Badge>
      <div className="min-w-0 flex-1">
        <div className="font-mono text-[12.5px]">{a.login}</div>
        <div className="text-[10.5px] text-fg-3">{productOf((a as Partial<EngineTradingAccount>).engine) === "options" ? `${t("accounts.product.chipOptions")} · ${a.group}` : `${a.group} · ${a.mode} · 1:${a.leverage}`}</div>
      </div>
      <div className="text-right font-mono text-[12px]">
        {accMoney(a, m.equity)} <span className="text-[10px] text-fg-3">{accCcy(a)}</span>
      </div>
    </button>
  );
}

function Row({ label, icon, children }: { label: string; icon?: React.ReactNode; children: React.ReactNode }) {
  return (
    <div className="flex h-11 items-center gap-2.5 border-b border-line/60 px-3 last:border-b-0">
      <span className="text-fg-3 [&>svg]:size-4">{icon}</span>
      <span className="flex-1 text-[13px]">{label}</span>
      {children}
    </div>
  );
}
