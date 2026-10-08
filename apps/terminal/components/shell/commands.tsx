"use client";

/*
 * Every action the old MT5 menu bar offered, in one registry: the command palette (⌘K) lists them, the Settings menu
 * and the chart's Layout menu build their items from the same helpers. docs/TERMINAL-DESIGN.md §2.2.
 */
import * as React from "react";
import { useTheme } from "next-themes";
import {
  ArrowUpRight,
  BarChart2,
  Bell,
  BookOpen,
  Camera,
  CircleHelp,
  Columns2,
  Expand,
  Grid2x2,
  Maximize2,
  PanelBottom,
  PanelRight,
  Keyboard,
  Languages,
  LayoutTemplate,
  LogIn,
  LogOut,
  Moon,
  RefreshCw,
  Rows2,
  Settings2,
  ShoppingCart,
  Square,
  Sun,
  UserPlus,
  Wallet,
  Volume2,
  X,
  Zap,
} from "lucide-react";
import { Flag } from "@kalks/ui";
import { tr, useLocale, useT } from "@kalks/i18n/react";
import { LOCALES } from "@kalks/i18n/locales";
import type { MessageKey } from "@kalks/i18n";
import { toast } from "@/lib/notify";
import { useTerminal, type Layout, type SideTab, type ToolboxTab, type Workspace } from "@/lib/store";
import { CHART_TYPES, TIMEFRAMES } from "@/lib/trading";
import { chartRegistry } from "@/components/chart/engine";
import { openIndicatorList, openSaveTemplate } from "@/components/chart/indicators/state";
import { guestNotice, openRegister, CLIENT_AREA } from "@/lib/guest";
import type { MenuItem } from "@/components/ui/menu";
import { askConfirm } from "@/components/dialogs/confirm";
import { useSwitchMode } from "./mode-switch";
import { useTradeMode } from "@/lib/options/mode";
import { useModules } from "@/components/modules";
import { tabOn } from "@/lib/modules";

export function toggleFullscreen() {
  try {
    if (document.fullscreenElement) void document.exitFullscreen();
    else void document.documentElement.requestFullscreen();
  } catch {
    toast.error(tr("trader.toast.fullscreenUnavailable"));
  }
}

// name/hint are the English copies; the UI renders nameKey/hintKey.
export const PRESETS: { id: string; name: string; hint: string; nameKey: MessageKey; hintKey: MessageKey; profile: string; patch: (w: Workspace) => Partial<Workspace> }[] = [
  { id: "trading", name: "Trading", hint: "Default", nameKey: "trader.preset.trading", hintKey: "trader.preset.tradingHint", profile: "Default", patch: (w) => ({ layout: "1", panels: { ...w.panels, watch: true, toolbox: true }, side: "instruments" }) },
  { id: "focus", name: "Chart focus", hint: "Charts only", nameKey: "trader.preset.chartFocus", hintKey: "trader.preset.chartFocusHint", profile: "Default", patch: (w) => ({ layout: "1", panels: { ...w.panels, watch: false } }) },
  { id: "analysis", name: "Analysis", hint: "4 charts", nameKey: "trader.preset.analysis", hintKey: "trader.preset.analysisHint", profile: "Analysis", patch: (w) => ({ layout: "4", panels: { ...w.panels, watch: true, toolbox: true }, side: "instruments" }) },
  { id: "scalper", name: "Scalper", hint: "Order book + 2 charts", nameKey: "trader.preset.scalper", hintKey: "trader.preset.scalperHint", profile: "Scalping", patch: (w) => ({ layout: "2v", panels: { ...w.panels, watch: true, toolbox: true }, side: "book" }) },
];

export const LAYOUTS: { id: Layout; label: MessageKey; hint: string; icon: React.ReactNode }[] = [
  { id: "1", label: "trader.layout.one", hint: "Alt+1", icon: <Square /> },
  { id: "2h", label: "trader.layout.twoH", hint: "Alt+2", icon: <Columns2 /> },
  { id: "2v", label: "trader.layout.twoV", hint: "Alt+3", icon: <Rows2 /> },
  { id: "4", label: "trader.layout.four", hint: "Alt+4", icon: <Grid2x2 /> },
];

export const MAX_DEVIATIONS = [null, 0, 3, 5, 10, 20, 50, 100] as const;

type Terminal = ReturnType<typeof useTerminal>;

export function applyPreset(T: Terminal, p: (typeof PRESETS)[number]) {
  T.setWs((w) => ({ ...p.patch(w), profile: p.profile }));
  const next = p.patch(T.ws);
  if (next.layout) T.setLayout(next.layout);
  toast(tr("trader.toast.layoutApplied", { name: tr(p.nameKey) }));
}

/** The positions / orders / history section below the chart (the page scrolls down to it). */
export const ACTIVITY_ID = "k-activity";

/** Scroll the page down to the positions section (Delta-style full-page scroll). */
export function scrollToActivity() {
  document.getElementById(ACTIVITY_ID)?.scrollIntoView({ behavior: "smooth", block: "start" });
}

/** Scroll back up to the chart. */
export function scrollToChart() {
  window.scrollTo({ top: 0, behavior: "smooth" });
}

/** Open a tab of the positions section: scroll down to it (full page), or show the panel under the chart (split). */
export function openActivity(T: Terminal, tab: ToolboxTab) {
  T.setWs({ toolboxTab: tab });
  T.togglePanel("toolbox", true);
  if (T.ws.posLayout !== "split") requestAnimationFrame(scrollToActivity);
}

/** Positions below the chart with a page scroll, or in a panel under the chart on the first screen. */
export function setPositionsLayout(T: Terminal, v: "page" | "split") {
  if (T.ws.posLayout === v) return;
  T.setWs((w) => ({ posLayout: v, panels: { ...w.panels, toolbox: true } }));
  if (v === "split") window.scrollTo({ top: 0 });
}

/** Show a tab of the right-hand column (Instruments | Order book | Navigator), opening the column. With `toggle`, the
 *  same tab again closes the column (Ctrl+M, Ctrl+B). */
export function showSide(T: Terminal, tab: SideTab, toggle = false) {
  if (toggle && T.ws.panels.watch && T.ws.side === tab) return T.togglePanel("watch", false);
  T.setWs((w) => ({ side: tab, panels: { ...w.panels, watch: true } }));
}

/** "Full chart": the chart covers the window; an edge arrow slides the instruments back in. */
export function toggleFullChart(T: Terminal, v?: boolean) {
  T.setUi({ fullChart: v ?? !T.ui.fullChart });
}

export function toggleOneClick(T: Terminal) {
  if (T.readOnly) return;
  if (T.guest) return void guestNotice(tr("trader.oneClick.name"));
  const v = !T.ws.oneClick;
  T.setWs({ oneClick: v });
  toast(v ? tr("trader.oneClick.enabled") : tr("trader.oneClick.disabled"), { description: v ? tr("desk.op.oneClickOn") : tr("desk.op.oneClickOff") });
}

/** Grid, presets, panels and reset: the chart's Layout menu and Settings › Layout and panels. */
export function useLayoutItems(): MenuItem[] {
  const T = useTerminal();
  const t = useT();
  const options = useTradeMode() === "options";
  return [
    ...(options
      ? []
      : ([
          { header: t("desk.ch.grid") },
          ...LAYOUTS.map((l) => ({ label: t(l.label), icon: l.icon, hint: l.hint, checked: T.ws.layout === l.id, onSelect: () => T.setLayout(l.id) })),
          "sep",
        ] as MenuItem[])),
    { header: t("desk.set.presets") },
    ...PRESETS.map((p) => ({ label: t(p.nameKey), hint: t(p.hintKey), icon: <LayoutTemplate />, onSelect: () => applyPreset(T, p) })),
    "sep",
    { header: t("desk.set.panels") },
    { label: options ? t("trader.opt.inst.title") : t("desk.side.instruments"), icon: <PanelRight />, checked: T.ws.panels.watch && T.ws.side === "instruments", hint: "Ctrl+M", keepOpen: true, onSelect: () => showSide(T, "instruments", true) },
    { label: t("desk.ob.title"), checked: T.ws.panels.watch && T.ws.side === "book", hint: "Ctrl+B", keepOpen: true, onSelect: () => showSide(T, "book", true) },
    ...(options ? [] : [{ label: t("desk.panel.navigator"), checked: T.ws.panels.watch && T.ws.side === "navigator", keepOpen: true, onSelect: () => showSide(T, "navigator", true) } as MenuItem]),
    { label: t("desk.ch.fullChart"), icon: <Maximize2 />, checked: T.ui.fullChart, hint: "Shift+F", onSelect: () => toggleFullChart(T) },
    { header: t("desk.pl.title") },
    { label: t("desk.pl.page"), icon: <Rows2 />, checked: T.ws.posLayout !== "split", onSelect: () => setPositionsLayout(T, "page") },
    { label: t("desk.pl.split"), icon: <PanelBottom />, checked: T.ws.posLayout === "split", onSelect: () => setPositionsLayout(T, "split") },
    { label: t("desk.panel.activity"), hint: "Ctrl+T", onSelect: () => openActivity(T, T.ws.toolboxTab) },
    "sep",
    { label: t("desk.set.reset"), icon: <RefreshCw />, onSelect: () => T.resetWorkspace() },
  ];
}

/** The ☰ menu at the left of the top bar (MT5 web style): accounts, chart, trading, appearance, help. */
export function useMainMenuItems(): MenuItem[] {
  const T = useTerminal();
  const t = useT();
  const lang = useLocale();
  const { resolvedTheme, setTheme } = useTheme();
  const layout = useLayoutItems();
  const tab = T.activeTab;
  const ro = T.readOnly || T.guest;
  const accounts: MenuItem[] = T.guest
    ? [
        { label: t("trader.guest.logInToTrade"), icon: <LogIn />, onSelect: () => T.openLogin() },
        { label: t("trader.guest.openAccount"), icon: <UserPlus />, onSelect: openRegister },
      ]
    : [
        ...T.accounts.map((a) => ({ label: `${a.login}${a.nickname ? ` · ${a.nickname}` : ""}`, hint: t.dyn(`trader.accountType.${a.type}`, a.type), checked: a.login === T.account.login, onSelect: () => a.login !== T.account.login && T.switchAccount(a.login) }) as MenuItem),
        "sep",
        ...(T.engine ? [{ label: t("desk.acc.logInAnother"), icon: <LogIn />, onSelect: () => T.openLogin() } as MenuItem] : []),
        { label: t("desk.acc.openNew"), icon: <UserPlus />, onSelect: () => window.open(`${CLIENT_AREA}/accounts`, "_blank") },
        { label: t("desk.acc.manage"), icon: <ArrowUpRight />, onSelect: () => window.open(`${CLIENT_AREA}/accounts`, "_blank") },
      ];
  const chart: MenuItem[] = [
    { header: t("desk.ch.type") },
    ...CHART_TYPES.map((ct) => ({ label: t(`trader.chartType.${ct}`), checked: tab.type === ct, onSelect: () => T.updateTab(tab.id, { type: ct }) }) as MenuItem),
    "sep",
    { label: t("trader.menu.indicatorsList"), hint: "Ctrl+I", icon: <BarChart2 />, onSelect: () => openIndicatorList(tab.id) },
    { label: t("trader.menu.saveTemplate"), onSelect: () => openSaveTemplate(tab.id) },
    { label: t("trader.menu.saveAsPicture"), icon: <Camera />, onSelect: () => chartRegistry.get(tab.id)?.screenshot() },
    "sep",
    ...layout,
  ];
  return [
    { header: T.guest ? t("desk.menu.guest") : `${T.account.login} · ${T.account.server}` },
    { label: t("desk.menu.accounts"), icon: <Wallet />, items: accounts },
    { label: t("desk.menu.chart"), icon: <BarChart2 />, items: chart },
    { label: t("desk.set.oneClick"), icon: <Zap />, hint: "F10", checked: T.ws.oneClick && !ro, disabled: ro, onSelect: () => toggleOneClick(T) },
    "sep",
    { label: resolvedTheme === "light" ? t("desk.menu.darkTheme") : t("desk.menu.lightTheme"), icon: resolvedTheme === "light" ? <Moon /> : <Sun />, onSelect: () => setTheme(resolvedTheme === "light" ? "dark" : "light") },
    { label: t("desk.set.language"), icon: <Languages />, hint: lang.info.name, items: LOCALES.map((l) => ({ label: l.name, icon: <Flag country={l.flag} className="size-3.5" />, hint: l.code === "en" ? undefined : l.english, checked: lang.locale === l.code, onSelect: () => void lang.setLocale(l.code) })) },
    { label: t("desk.set.sounds"), icon: <Volume2 />, checked: T.ws.sound, onSelect: () => (T.setWs({ sound: !T.ws.sound }), toast(T.ws.sound ? t("trader.toast.soundsOff") : t("trader.toast.soundsOn"))) },
    ...(T.guest
      ? []
      : ([
          {
            label: t("desk.set.maxDeviation"),
            hint: T.ws.maxDeviation === null ? t("trader.menu.anyPrice") : t("trader.menu.points", { count: T.ws.maxDeviation }),
            items: MAX_DEVIATIONS.map((d) => ({ label: d === null ? t("trader.menu.anyPrice") : t("trader.menu.points", { count: d }), checked: T.ws.maxDeviation === d, onSelect: () => T.setWs({ maxDeviation: d }) })),
          },
        ] as MenuItem[])),
    { label: t("desk.set.fullScreen"), icon: <Expand />, hint: "F11", onSelect: toggleFullscreen },
    { label: t("desk.set.all"), icon: <Settings2 />, onSelect: () => T.setUi({ options: true }) },
    "sep",
    { label: t("desk.set.shortcuts"), icon: <Keyboard />, hint: "F1", onSelect: () => T.setUi({ shortcuts: true }) },
    { label: t("desk.help.glossary"), icon: <BookOpen />, onSelect: () => T.setUi({ glossary: true }) },
    { label: t("desk.help.tour"), icon: <CircleHelp />, onSelect: () => T.setUi({ tour: true }) },
    { label: t("desk.help.topics"), icon: <ArrowUpRight />, onSelect: () => window.open(`${CLIENT_AREA}/academy`, "_blank") },
    { label: t("desk.help.support"), icon: <ArrowUpRight />, onSelect: () => window.open(`${CLIENT_AREA}/support`, "_blank") },
    { label: t("desk.help.about"), onSelect: () => T.setUi({ about: true }) },
  ];
}

export interface Command {
  id: string;
  group: "trade" | "chart" | "view" | "account" | "help";
  label: string;
  hint?: string;
  icon?: React.ReactNode;
  /** extra words to match (English names too, so "options" or "file" still finds things in any language) */
  keywords?: string;
  checked?: boolean;
  disabled?: boolean;
  run: () => void;
}

/** Every action, for the command palette. */
export function useCommands(): Command[] {
  const T = useTerminal();
  const t = useT();
  const modules = useModules();
  const lang = useLocale();
  const { resolvedTheme, setTheme } = useTheme();
  const switchMode = useSwitchMode();
  const mode = useTradeMode();
  const tab = T.activeTab;
  const reg = () => chartRegistry.get(tab.id);
  const ro = T.readOnly || T.guest;
  const c: Command[] = [];
  const add = (x: Command) => c.push(x);

  // trading
  add({ id: "new-order", group: "trade", label: t("trader.newOrder"), hint: "F9", icon: <ShoppingCart />, keywords: "new order buy sell trade ticket", disabled: T.readOnly, run: () => T.openNewOrder() });
  add({ id: "one-click", group: "trade", label: t("desk.set.oneClick"), hint: "F10", icon: <Zap />, keywords: "one click instant", checked: T.ws.oneClick && !ro, disabled: ro, run: () => toggleOneClick(T) });
  if (!T.guest && !T.readOnly) {
    const bulk = (label: string, all: boolean, run: () => void) => () => askConfirm({ title: `${label}?`, text: all ? t("desk.cf.closeAllText", { count: T.positions.length }) : t("desk.cf.closeSomeText"), confirmLabel: label, run });
    add({ id: "close-all", group: "trade", label: t("toolbox.bulk.closeAll"), icon: <X />, keywords: "close all positions bulk", disabled: !T.positions.length, run: bulk(t("toolbox.bulk.closeAll"), true, () => T.bulkClose("all")) });
    add({ id: "close-profit", group: "trade", label: t("toolbox.bulk.closeProfitable"), keywords: "close profitable winners", disabled: !T.positions.length, run: bulk(t("toolbox.bulk.closeProfitable"), false, () => T.bulkClose("profit")) });
    add({ id: "close-loss", group: "trade", label: t("toolbox.bulk.closeLosing"), keywords: "close losing losers", disabled: !T.positions.length, run: bulk(t("toolbox.bulk.closeLosing"), false, () => T.bulkClose("loss")) });
    add({ id: "cancel-pend", group: "trade", label: t("toolbox.bulk.cancelPendings"), keywords: "cancel delete pending orders", disabled: !T.pendings.length, run: () => askConfirm({ title: `${t("toolbox.bulk.cancelPendings")}?`, text: t("desk.cf.cancelText", { count: T.pendings.length }), confirmLabel: t("toolbox.bulk.cancelPendings"), run: () => T.cancelAllPendings() }) });
    add({ id: "breakeven", group: "trade", label: t("desk.act.breakevenAll"), keywords: "breakeven stop loss entry script", disabled: !T.positions.length, run: () => T.positions.forEach((p) => T.modifyPosition(p.ticket, { sl: p.openPrice })) });
  }
  add({ id: "alerts", group: "trade", label: t("trader.menu.priceAlerts"), icon: <Bell />, keywords: "alert alarm notify price", run: () => openActivity(T, "alerts") });
  add({ id: "settings", group: "trade", label: t("desk.set.all"), icon: <Settings2 />, keywords: "settings options preferences max deviation slippage default lot sound tools", run: () => T.setUi({ options: true }) });

  // chart
  add({ id: "indicators", group: "chart", label: t("trader.menu.indicatorsList"), hint: "Ctrl+I", icon: <BarChart2 />, keywords: "indicators insert rsi macd ema", run: () => openIndicatorList(tab.id) });
  add({ id: "new-tab", group: "chart", label: t("trader.menu.newChartTab"), keywords: "new chart tab file", run: () => T.addTab() });
  add({ id: "close-tab", group: "chart", label: t("trader.menu.closeChart"), keywords: "close chart tab file", disabled: T.ws.tabs.length <= 1, run: () => T.closeTab(tab.id) });
  for (const tf of TIMEFRAMES) add({ id: `tf-${tf}`, group: "chart", label: `${t("trader.menu.timeframes")}: ${tf}`, keywords: `timeframe ${tf}`, checked: tab.tf === tf, run: () => T.updateTab(tab.id, { tf, drawings: tab.tf === tf ? tab.drawings : [] }) });
  for (const ct of CHART_TYPES) add({ id: `type-${ct}`, group: "chart", label: `${t("desk.ch.type")}: ${t(`trader.chartType.${ct}`)}`, keywords: `chart type ${ct}`, checked: tab.type === ct, run: () => T.updateTab(tab.id, { type: ct }) });
  for (const l of LAYOUTS) add({ id: `layout-${l.id}`, group: "chart", label: `${t("desk.ch.layout")}: ${t(l.label)}`, hint: l.hint, icon: l.icon, keywords: "layout grid charts split", checked: T.ws.layout === l.id, run: () => T.setLayout(l.id) });
  add({ id: "crosshair", group: "chart", label: t("chart.toolbar.crosshair"), hint: "Ctrl+F", keywords: "crosshair cursor", run: () => T.setDrawTool(T.drawTool === "crosshair" ? "cursor" : "crosshair") });
  add({ id: "hline", group: "chart", label: t("trader.menu.horizontalLine"), keywords: "draw object insert line", run: () => T.setDrawTool("hline") });
  add({ id: "trend", group: "chart", label: t("trader.menu.trendLine"), keywords: "draw object insert trend", run: () => T.setDrawTool("trend") });
  add({ id: "fib", group: "chart", label: t("trader.menu.fibonacci"), keywords: "draw object insert fibonacci", run: () => T.setDrawTool("fib") });
  add({ id: "rect", group: "chart", label: t("trader.menu.rectangle"), keywords: "draw object insert rectangle box", run: () => T.setDrawTool("rect") });
  add({ id: "del-objects", group: "chart", label: t("trader.menu.deleteAllObjects"), keywords: "delete drawings objects clear", disabled: !tab.drawings.length, run: () => (T.updateTab(tab.id, { drawings: [] }), T.selectDrawing(null)) });
  add({ id: "zoom-in", group: "chart", label: t("trader.menu.zoomIn"), hint: "+", keywords: "zoom in", run: () => reg()?.zoom(1) });
  add({ id: "zoom-out", group: "chart", label: t("trader.menu.zoomOut"), hint: "−", keywords: "zoom out", run: () => reg()?.zoom(-1) });
  add({ id: "fit", group: "chart", label: t("desk.ch.fit"), keywords: "reset view fit", run: () => reg()?.fit() });
  add({ id: "shot", group: "chart", label: t("trader.menu.saveAsPicture"), icon: <Camera />, keywords: "screenshot picture image save", run: () => reg()?.screenshot() });
  add({ id: "save-tpl", group: "chart", label: t("trader.menu.saveTemplate"), keywords: "template save", run: () => openSaveTemplate(tab.id) });

  // view
  add({ id: "mode-cfd", group: "view", label: `${t("trader.opt.mode.label")}: ${t("trader.opt.mode.cfd")}`, keywords: "cfd mode", checked: mode === "cfd", run: () => switchMode("cfd") });
  add({ id: "mode-opt", group: "view", label: `${t("trader.opt.mode.label")}: ${t("trader.opt.mode.options")}`, keywords: "options mode fx options", checked: mode === "options", run: () => switchMode("options") });
  add({ id: "p-watch", group: "view", label: mode === "options" ? t("trader.opt.inst.title") : t("desk.side.instruments"), hint: "Ctrl+M", keywords: "market watch instruments symbols panel toggle view", checked: T.ws.panels.watch && T.ws.side === "instruments", run: () => showSide(T, "instruments", true) });
  add({ id: "p-book", group: "view", label: t("desk.ob.title"), hint: "Ctrl+B", keywords: "order book depth dom ladder", checked: T.ws.panels.watch && T.ws.side === "book", run: () => showSide(T, "book", true) });
  add({ id: "p-box", group: "view", label: t("desk.panel.activity"), hint: "Ctrl+T", keywords: "toolbox positions orders panel scroll", run: () => openActivity(T, T.ws.toolboxTab) });
  if (mode !== "options") add({ id: "p-nav", group: "view", label: t("desk.panel.navigator"), keywords: "navigator scripts strategies", checked: T.ws.panels.watch && T.ws.side === "navigator", run: () => showSide(T, "navigator", true) });
  add({ id: "full-chart", group: "view", label: t("desk.ch.fullChart"), hint: "Shift+F", icon: <Maximize2 />, keywords: "full chart maximise chart hide panels", checked: T.ui.fullChart, run: () => toggleFullChart(T) });
  for (const p of PRESETS) add({ id: `preset-${p.id}`, group: "view", label: `${t("desk.set.presets")}: ${t(p.nameKey)}`, hint: t(p.hintKey), keywords: `layout preset profile ${p.name}`, run: () => applyPreset(T, p) });
  for (const [k, label] of [
    ["history", t("toolbox.tab.history")],
    ["exposure", t("toolbox.tab.exposure")],
    ["news", t("toolbox.tab.news")],
    ["calendar", t("toolbox.tab.calendar")],
    ["journal", t("toolbox.tab.journal")],
    ["ai", t("toolbox.tab.ai")],
  ] as const)
    // tabs of modules the broker switched off are not offered (module switches)
    if (tabOn(modules, k)) add({ id: `tab-${k}`, group: "view", label, keywords: `${k} tab toolbox`, run: () => openActivity(T, k) });
  add({ id: "dark", group: "view", label: `${t("desk.set.theme")}: ${t("desk.set.dark")}`, icon: <Moon />, keywords: "dark theme night", checked: resolvedTheme !== "light", run: () => setTheme("dark") });
  add({ id: "light", group: "view", label: `${t("desk.set.theme")}: ${t("desk.set.light")}`, icon: <Sun />, keywords: "light theme day", checked: resolvedTheme === "light", run: () => setTheme("light") });
  for (const l of LOCALES) add({ id: `lang-${l.code}`, group: "view", label: `${t("desk.set.language")}: ${l.name}`, icon: <Flag country={l.flag} className="size-3.5" />, keywords: `language ${l.english} ${l.code}`, checked: lang.locale === l.code, run: () => void lang.setLocale(l.code) });
  add({ id: "full", group: "view", label: t("desk.set.fullScreen"), hint: "F11", icon: <Expand />, keywords: "fullscreen full screen", run: toggleFullscreen });
  add({ id: "reset", group: "view", label: t("desk.set.reset"), keywords: "reset workspace default", run: () => T.resetWorkspace() });
  add({ id: "sounds", group: "view", label: t("desk.set.sounds"), icon: <Volume2 />, keywords: "sound audio beep", checked: T.ws.sound, run: () => T.setWs({ sound: !T.ws.sound }) });

  // account
  if (T.guest) {
    add({ id: "login", group: "account", label: t("trader.guest.logInToTrade"), icon: <LogIn />, keywords: "log in login sign in", run: () => T.openLogin() });
    add({ id: "register", group: "account", label: t("trader.guest.openAccount"), icon: <UserPlus />, keywords: "register open account sign up", run: openRegister });
  } else {
    for (const a of T.accounts)
      if (a.login !== T.account.login) add({ id: `acc-${a.login}`, group: "account", label: `${t("trader.account.switch")}: ${a.login}`, keywords: `account switch ${a.type} ${a.group} ${a.nickname ?? ""}`, run: () => T.switchAccount(a.login) });
    if (T.engine) add({ id: "login-another", group: "account", label: t("desk.acc.logInAnother"), icon: <LogIn />, keywords: "log in login another account", run: () => T.openLogin() });
    add({ id: "open-acc", group: "account", label: t("desk.acc.openNew"), icon: <UserPlus />, keywords: "open new account register", run: () => window.open(`${CLIENT_AREA}/accounts`, "_blank") });
    if (T.account.type === "demo") add({ id: "refill", group: "account", label: t("trader.menu.refillDemo", { count: T.refillsLeft }), icon: <RefreshCw />, keywords: "refill top up demo balance reset", run: () => T.refillDemo() });
    else add({ id: "deposit", group: "account", label: t("desk.top.deposit"), icon: <Wallet />, keywords: "deposit fund wallet money", run: () => window.open(`${CLIENT_AREA}/wallet`, "_blank") });
    add({ id: "client-area", group: "account", label: t("trader.clientArea"), icon: <ArrowUpRight />, keywords: "client area portal", run: () => window.open(CLIENT_AREA, "_blank") });
    add({ id: "profile", group: "account", label: t("trader.account.profileSecurity"), keywords: "profile security password", run: () => window.open(`${CLIENT_AREA}/profile`, "_blank") });
    add({ id: "logout", group: "account", label: t("trader.menu.logOut"), icon: <LogOut />, keywords: "log out logout sign out exit", run: () => T.logout() });
  }

  // help
  add({ id: "tour", group: "help", label: t("desk.help.tour"), icon: <CircleHelp />, keywords: "tour welcome onboarding help", run: () => T.setUi({ tour: true }) });
  add({ id: "glossary", group: "help", label: t("desk.help.glossary"), icon: <BookOpen />, keywords: "glossary terms margin pip lot explain", run: () => T.setUi({ glossary: true }) });
  add({ id: "keys", group: "help", label: t("desk.set.shortcuts"), hint: "F1", icon: <Keyboard />, keywords: "keyboard shortcuts hotkeys keys", run: () => T.setUi({ shortcuts: true }) });
  add({ id: "topics", group: "help", label: t("desk.help.topics"), keywords: "help academy topics learn", run: () => window.open(`${CLIENT_AREA}/academy`, "_blank") });
  add({ id: "support", group: "help", label: t("desk.help.support"), keywords: "support contact chat", run: () => window.open(`${CLIENT_AREA}/support`, "_blank") });
  add({ id: "about", group: "help", label: t("desk.help.about"), keywords: "about version", run: () => T.setUi({ about: true }) });
  return c;
}

export const COMMAND_GROUPS: Command["group"][] = ["trade", "chart", "view", "account", "help"];
