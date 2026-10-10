"use client";

// Chart card (docs/TERMINAL-DESIGN.md §2.2), MT5 web style: ONE slim toolbar row (chart tabs · type · timeframes M1…MN ·
// New order · Sell / lot / Buy · zoom · indicators · templates · layout · alert · picture · full chart · full screen), a
// thin drawing rail on the left and the chart grid. The plot itself carries only the legend and the small K mark: the
// Buy / Sell box moved into the toolbar row so it never covers candles (founder 2026-10-10).
import * as React from "react";
import { toast } from "@/lib/notify";
import { AreaChart, BarChart3, Bell, Camera, CandlestickChart, ChevronDown, Crosshair, Expand, FileStack, LayoutPanelLeft, LineChart, Maximize2, Minimize2, Minus, MousePointer2, Plus, Scan, ShoppingCart, Shrink, Spline, Square, Trash2, TrendingUp, X, ZoomIn, ZoomOut } from "lucide-react";
import { SymbolAvatar, cn } from "@kalks/ui";
import { useTerminal, type DrawTool } from "@/lib/store";
import { useT } from "@kalks/i18n/react";
import type { MessageKey } from "@kalks/i18n";
import { CHART_TYPES, TIMEFRAMES, type ChartType } from "@/lib/trading";
import { DropMenu } from "@/components/ui/menu";
import { CountBadge, IconButton, Tip } from "@/components/ui/kit";
import { useLayoutItems, openActivity, toggleFullChart, toggleFullscreen } from "@/components/shell/commands";
import { ChartView, OneClickBar } from "./chart-view";
import { chartRegistry } from "./engine";
import { BUILTIN_TEMPLATES, applyTemplate, deleteTemplate, openIndicatorList, openSaveTemplate, shortList, templateMatches, useUserTemplates } from "./indicators/state";

const TYPE_ICON: Record<ChartType, React.ReactNode> = {
  candles: <CandlestickChart />,
  bars: <BarChart3 />,
  line: <LineChart />,
  area: <AreaChart />,
};

export function ChartWorkspace() {
  const T = useTerminal();
  const slots = T.ws.slots.map((id) => T.ws.tabs.find((t) => t.id === id)!).filter(Boolean);
  const layout = T.ws.layout;
  const grid = layout === "1" ? "grid-cols-1 grid-rows-1" : layout === "2h" ? "grid-cols-2 grid-rows-1" : layout === "2v" ? "grid-cols-1 grid-rows-2" : "grid-cols-2 grid-rows-2";
  return (
    <section data-tour="chart" className="t-glass flex h-full min-h-0 min-w-0 flex-col overflow-hidden rounded-[14px] border border-line">
      <ChartBar />
      <div className="flex min-h-0 flex-1">
        <DrawingBar />
        <div className={cn("grid min-h-0 min-w-0 flex-1 gap-1.5 pb-1.5 pe-1.5", grid)}>
          {slots.map((tab) => (
            <ChartView key={tab.id} tab={tab} active={tab.id === T.ws.activeId && slots.length > 0} highlight={tab.id === T.ws.activeId && slots.length > 1} onActivate={() => T.ws.activeId !== tab.id && T.activateTab(tab.id)} compact={layout === "4"} hideOneClick />
          ))}
        </div>
      </div>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/* The toolbar row                                                     */
/* ------------------------------------------------------------------ */

function Sep() {
  return <span className="mx-0.5 h-5 w-px shrink-0 bg-line" aria-hidden />;
}

/** Open charts as compact tabs: the market (and timeframe of the active one); × closes, middle-click too. */
function ChartTabs() {
  const T = useTerminal();
  const t = useT();
  return (
    <div role="tablist" aria-label={t("desk.panel.chart")} className="flex min-w-0 shrink items-center gap-0.5 overflow-x-auto [mask-image:linear-gradient(to_right,#000_calc(100%-20px),transparent)] [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
      {T.ws.tabs.map((ct) => {
        const on = ct.id === T.ws.activeId;
        const shown = T.ws.slots.includes(ct.id);
        return (
          <div
            key={ct.id}
            role="tab"
            tabIndex={on ? 0 : -1}
            aria-selected={on}
            onClick={() => T.activateTab(ct.id)}
            onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && T.activateTab(ct.id)}
            onAuxClick={(e) => e.button === 1 && T.closeTab(ct.id)}
            title={`${ct.symbol}, ${ct.tf}`}
            className={cn("group flex h-7 shrink-0 cursor-pointer items-center gap-1.5 rounded-[8px] ps-1.5 pe-0.5 text-[12.5px] transition-colors", on ? "bg-surface-3 text-fg" : "text-fg-2 hover:bg-surface-3/50 hover:text-fg")}
          >
            <SymbolAvatar symbol={ct.symbol} size={14} />
            <span className="font-medium">{ct.symbol}</span>
            {on && <span className="font-mono text-[11px] text-fg-3">{ct.tf}</span>}
            {shown && !on && <span className="size-1.5 rounded-full bg-fg-3" title={t("chart.tab.visibleInGrid")} />}
            <button
              aria-label={t("chart.tab.close", { symbol: ct.symbol })}
              title={t("chart.tab.close", { symbol: ct.symbol })}
              onClick={(e) => {
                e.stopPropagation();
                T.closeTab(ct.id);
              }}
              className={cn("grid size-5 place-items-center rounded-[5px] text-fg-3 hover:bg-panel hover:text-fg", on ? "opacity-100" : "w-0 opacity-0 focus-visible:w-5 focus-visible:opacity-100 group-hover:w-5 group-hover:opacity-100")}
            >
              <X className="size-3" />
            </button>
          </div>
        );
      })}
    </div>
  );
}

/** Narrow chart cards: the active chart as one chip, the other open charts in its menu. */
function ChartTabsCompact() {
  const T = useTerminal();
  const t = useT();
  const tab = T.activeTab;
  return (
    <DropMenu
      width={240}
      items={[
        { header: t("desk.ch.openCharts") },
        ...T.ws.tabs.map((ct) => ({ label: `${ct.symbol}, ${ct.tf}`, icon: <SymbolAvatar symbol={ct.symbol} size={14} />, checked: ct.id === T.ws.activeId, onSelect: () => T.activateTab(ct.id) })),
        "sep",
        { label: t("desk.ch.newChart"), icon: <Plus />, onSelect: () => T.addTab() },
        { label: t("trader.menu.closeChart"), icon: <X />, disabled: T.ws.tabs.length <= 1, onSelect: () => T.closeTab(tab.id) },
      ]}
      trigger={({ toggle, open }) => (
        <button onClick={toggle} aria-expanded={open} aria-label={t("desk.ch.openCharts")} className={cn("flex h-7 shrink-0 items-center gap-1.5 rounded-[8px] ps-1.5 pe-1 text-[12.5px] transition-colors", open ? "bg-surface-3 text-fg" : "bg-surface-3/70 text-fg hover:bg-surface-3")}>
          <SymbolAvatar symbol={tab.symbol} size={14} />
          <span className="font-medium">{tab.symbol}</span>
          <span className="font-mono text-[11px] text-fg-3">{tab.tf}</span>
          {T.ws.tabs.length > 1 && <span className="rounded-[4px] bg-panel px-1 font-mono text-[10.5px] text-fg-3">{T.ws.tabs.length}</span>}
          <ChevronDown className="size-3.5 text-fg-3" />
        </button>
      )}
    />
  );
}

function NewTabButton() {
  const T = useTerminal();
  const t = useT();
  return (
    <IconButton label={t("desk.ch.newChart")} size="sm" onClick={() => T.addTab()}>
      <Plus />
    </IconButton>
  );
}

function ChartBar() {
  const T = useTerminal();
  const t = useT();
  const tab = T.activeTab;
  const userTpl = useUserTemplates();
  const layoutItems = useLayoutItems();
  const reg = () => chartRegistry.get(tab.id);
  const full = T.ui.fullChart;
  const btn = (open?: boolean) => cn("flex h-7 shrink-0 items-center gap-1.5 rounded-[7px] px-1.5 text-[12.5px] font-medium transition-colors [&>svg]:size-4", open ? "bg-surface-3 text-fg" : "text-fg-2 hover:bg-surface-3 hover:text-fg");
  return (
    <div role="toolbar" aria-label={t("desk.ch.toolbar")} className="@container flex h-10 shrink-0 items-center gap-1 border-b border-line px-1.5">
      {/* every chart as a tab when there is room; the active chart and a menu otherwise */}
      <div className="hidden min-w-0 shrink items-center @[1240px]:flex">
        <ChartTabs />
        <NewTabButton />
      </div>
      <div className="flex shrink-0 items-center @[1240px]:hidden">
        <ChartTabsCompact />
      </div>
      <Sep />
      <DropMenu
        width={220}
        items={[{ header: t("desk.ch.type") }, ...CHART_TYPES.map((ct) => ({ label: t(`trader.chartType.${ct}`), icon: TYPE_ICON[ct], checked: tab.type === ct, onSelect: () => T.updateTab(tab.id, { type: ct }) }))]}
        trigger={({ toggle, open }) => (
          <Tip content={t("desk.ch.type")} side="bottom">
            <button onClick={toggle} aria-label={`${t("desk.ch.type")}: ${t(`trader.chartType.${tab.type}`)}`} className={cn(btn(open), "gap-0.5 px-1")}>
              {TYPE_ICON[tab.type]}
              <ChevronDown className="!size-3 text-fg-3" />
            </button>
          </Tip>
        )}
      />
      {/* timeframes: all of them when there is room, a menu otherwise */}
      <div role="radiogroup" aria-label={t("trader.menu.timeframes")} className="hidden shrink-0 items-center @[1000px]:flex">
        {TIMEFRAMES.map((tf) => (
          <button
            key={tf}
            role="radio"
            aria-checked={tab.tf === tf}
            onClick={() => T.updateTab(tab.id, { tf, drawings: tab.tf === tf ? tab.drawings : [] })}
            className={cn("h-7 min-w-[30px] rounded-[6px] px-1 font-mono text-[12px] transition-colors", tab.tf === tf ? "bg-ember-soft font-semibold text-accent-text" : "text-fg-2 hover:bg-surface-3/60 hover:text-fg")}
          >
            {tf}
          </button>
        ))}
      </div>
      <span className="@[1000px]:hidden">
        <DropMenu
          width={160}
          items={[{ header: t("trader.menu.timeframes") }, ...TIMEFRAMES.map((tf) => ({ label: tf, checked: tab.tf === tf, onSelect: () => T.updateTab(tab.id, { tf, drawings: tab.tf === tf ? tab.drawings : [] }) }))]}
          trigger={({ toggle, open }) => (
            <button onClick={toggle} aria-label={`${t("trader.menu.timeframes")}: ${tab.tf}`} className={cn(btn(open), "font-mono")}>
              {tab.tf}
              <ChevronDown className="!size-3 text-fg-3" />
            </button>
          )}
        />
      </span>
      <Sep />
      <Tip content={t("trader.newOrder")} shortcut="F9" side="bottom">
        <button onClick={() => T.openNewOrder({ symbol: tab.symbol })} disabled={T.readOnly} data-tour="new-order" className="flex h-7 shrink-0 items-center gap-1.5 rounded-[7px] border border-line px-2 text-[12.5px] font-semibold text-fg transition-colors hover:border-ember/50 hover:bg-ember-soft/40 disabled:opacity-45 [&>svg]:size-3.5">
          <ShoppingCart className="text-accent-text" />
          <span className="hidden @[1320px]:inline">{t("trader.newOrder")}</span>
        </button>
      </Tip>
      {/* one-click Sell · lot · Buy for the active chart, slim in the row instead of a box on the candles */}
      {!T.readOnly && (
        <span className="ms-1 hidden @[560px]:flex">
          <OneClickBar symbol={tab.symbol} />
        </span>
      )}
      <Sep />
      {/* with the Sell / Buy bar in the row, the zoom buttons show only on wide charts (wheel and + / − zoom anyway) */}
      <span className="hidden shrink-0 items-center gap-1 @[1300px]:flex">
        <IconButton label={t("desk.ch.zoomIn")} shortcut="+" onClick={() => reg()?.zoom(1)}>
          <ZoomIn />
        </IconButton>
        <IconButton label={t("desk.ch.zoomOut")} shortcut="−" onClick={() => reg()?.zoom(-1)}>
          <ZoomOut />
        </IconButton>
        <IconButton label={t("desk.ch.fit")} onClick={() => reg()?.fit()}>
          <Scan />
        </IconButton>
        <Sep />
      </span>
      <Tip content={t("chart.toolbar.indicatorsTitle")} shortcut="Ctrl+I" side="bottom">
        <button onClick={() => openIndicatorList(tab.id)} aria-haspopup="dialog" aria-label={t("chart.toolbar.indicators")} className={btn()}>
          <Spline />
          <span className="hidden @[1180px]:inline">{t("chart.toolbar.indicators")}</span>
          {tab.indicators.length > 0 && <CountBadge n={tab.indicators.length} tone="accent" />}
        </button>
      </Tip>
      <span className="hidden @[1180px]:contents">
      <DropMenu
        width={292}
        items={[
          { header: t("chart.toolbar.builtIn") },
          ...BUILTIN_TEMPLATES.map((tp) => ({ label: t.dyn(`chart.template.${tp.id}`, tp.name), checked: templateMatches(tp, tab), onSelect: () => applyTemplate(T, [tab.id], tp) })),
          { header: t("chart.toolbar.myTemplates") },
          ...(userTpl.length ? userTpl.map((tp) => ({ label: tp.name, hint: `${tp.indicators.length}`, checked: templateMatches(tp, tab), onSelect: () => applyTemplate(T, [tab.id], tp) })) : [{ label: t("chart.toolbar.noTemplates"), disabled: true }]),
          "sep",
          { label: t("chart.toolbar.saveTemplate"), onSelect: () => openSaveTemplate(tab.id) },
          { label: t("chart.toolbar.deleteTemplate"), disabled: !userTpl.length, items: userTpl.map((tp) => ({ label: tp.name, danger: true, onSelect: () => deleteTemplate(tp.id) })) },
          { label: t("chart.toolbar.applyToAll"), onSelect: () => applyTemplate(T, T.ws.tabs.map((x) => x.id), { id: "cur", name: `${tab.symbol} ${tab.tf}`, type: tab.type, indicators: tab.indicators }) },
        ]}
        trigger={({ toggle, open }) => (
          <Tip content={t("chart.toolbar.templates")} side="bottom">
            <button onClick={toggle} aria-label={t("chart.toolbar.templates")} className={btn(open)}>
              <FileStack />
            </button>
          </Tip>
        )}
      />
      </span>
      <DropMenu
        align="end"
        width={272}
        items={layoutItems}
        trigger={({ toggle, open }) => (
          <Tip content={t("desk.ch.layout")} side="bottom">
            <button onClick={toggle} aria-expanded={open} aria-label={t("desk.ch.layout")} className={btn(open)}>
              <LayoutPanelLeft />
            </button>
          </Tip>
        )}
      />
      <IconButton label={t("desk.ch.alertTip", { symbol: tab.symbol })} onClick={() => openActivity(T, "alerts")} className="hidden @[1100px]:inline-grid">
        <Bell />
      </IconButton>
      <IconButton label={t("desk.ch.screenshot")} onClick={() => reg()?.screenshot()} className="hidden @[1180px]:inline-grid">
        <Camera />
      </IconButton>
      <div className="ms-auto hidden min-w-0 shrink items-center px-2 font-mono text-[11px] text-fg-3 @[1400px]:flex">{tab.indicators.length > 0 && <span className="truncate">{shortList(tab).join(" · ")}</span>}</div>
      <div className="ms-auto flex shrink-0 items-center gap-0.5 @[1400px]:ms-0">
        <IconButton label={full ? t("desk.ch.exitFullChart") : t("desk.ch.fullChart")} shortcut="Shift+F" active={full} onClick={() => toggleFullChart(T)} data-tour="full-chart">
          {full ? <Minimize2 /> : <Maximize2 />}
        </IconButton>
        <IconButton label={t("desk.set.fullScreen")} shortcut="F11" onClick={toggleFullscreen}>
          <FullscreenIcon />
        </IconButton>
      </div>
    </div>
  );
}

/** Browser full screen: Expand, or Shrink while the page is full screen. */
function FullscreenIcon() {
  const [on, setOn] = React.useState(false);
  React.useEffect(() => {
    const f = () => setOn(!!document.fullscreenElement);
    f();
    document.addEventListener("fullscreenchange", f);
    return () => document.removeEventListener("fullscreenchange", f);
  }, []);
  return on ? <Shrink /> : <Expand />;
}

/* ------------------------------------------------------------------ */
/* Left vertical drawing rail                                          */
/* ------------------------------------------------------------------ */

const TOOLS: { id: DrawTool; label: MessageKey; icon: React.ReactNode; shortcut?: string }[] = [
  { id: "cursor", label: "chart.tool.cursor", icon: <MousePointer2 />, shortcut: "Esc" },
  { id: "crosshair", label: "chart.tool.crosshair", icon: <Crosshair />, shortcut: "Ctrl+F" },
  { id: "hline", label: "chart.tool.hline", icon: <Minus /> },
  { id: "trend", label: "chart.tool.trend", icon: <TrendingUp /> },
  { id: "fib", label: "chart.tool.fib", icon: <FibIcon /> },
  { id: "rect", label: "chart.tool.rect", icon: <Square /> },
];

function FibIcon() {
  return (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.3">
      <path d="M2 3h12M2 6.5h12M2 9.5h12M2 13h12" />
      <path d="M3 13L13 3" strokeDasharray="1.6 1.6" />
    </svg>
  );
}

function DrawingBar() {
  const T = useTerminal();
  const t = useT();
  const n = T.activeTab.drawings.length;
  return (
    <div role="toolbar" aria-label={t("desk.ch.drawings")} aria-orientation="vertical" className="flex w-10 shrink-0 flex-col items-center gap-0.5 py-1.5">
      {TOOLS.map((tl) => (
        <IconButton key={tl.id} label={t(tl.label)} shortcut={tl.shortcut} tipSide="right" active={T.drawTool === tl.id} onClick={() => T.setDrawTool(T.drawTool === tl.id && tl.id !== "cursor" ? "cursor" : tl.id)} className="[&_svg]:size-4">
          {tl.icon}
        </IconButton>
      ))}
      <span className="my-1 h-px w-6 bg-line" />
      <IconButton
        label={t("desk.ch.deleteDrawings")}
        tipSide="right"
        onClick={() => {
          if (!n) return void toast(t("chart.tool.noObjects"));
          T.updateTab(T.activeTab.id, { drawings: [] });
          T.selectDrawing(null);
          toast(t("chart.tool.deleted", { count: n }), { description: `${T.activeTab.symbol}, ${T.activeTab.tf}` });
        }}
        className="hover:bg-down-soft hover:text-down [&_svg]:size-4"
        badge={n > 0 ? <span className="absolute -end-0.5 -top-0.5 grid h-4 min-w-4 place-items-center rounded-full bg-ember px-1 font-mono text-[10px] text-white">{n}</span> : undefined}
      >
        <Trash2 />
      </IconButton>
    </div>
  );
}
