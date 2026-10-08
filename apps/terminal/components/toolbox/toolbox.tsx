"use client";

// Activity panel (the old Toolbox), docs/TERMINAL-DESIGN.md §2.2: the account health strip, then primary tabs
// (Positions · Orders · History, or the option tabs in Options mode) with counts, the secondary tabs (Alerts · News ·
// Calendar) and "More ▾" (Exposure, Journal, AI Trader, MAM, the other mode's tabs). Tab actions sit on the right.
import * as React from "react";
import dynamic from "next/dynamic";
import { ArrowUp, BarChart3, ChevronDown, ChevronsDown, History, Layers, Maximize2, Minimize2, PanelBottom, PieChart, Rows2 } from "lucide-react";
import { cn } from "@kalks/ui";
import { useTerminal, type ToolboxTab } from "@/lib/store";
import { PanelTabs } from "@/components/ui/panel";
import { CountBadge, IconButton, Segmented } from "@/components/ui/kit";
import { setPositionsLayout } from "@/components/shell/commands";
import { DropMenu, type MenuItem } from "@/components/ui/menu";
import { PendingTab, PositionsTab, bulkMenu } from "./trade-tab";
import { AlertsTab, CalendarTab, ExposureTab, HistoryTab, JournalTab, NewsTab } from "./tabs";
import { AiTraderTab, useAi } from "./ai-trader";
import { MamTab, useMam } from "./mam-tab";
import { LiveCalendarTab, LiveNewsTab } from "./news-live";
import { ShareControls } from "@/components/share/share-dialogs";
import { GuestNotice } from "@/components/shell/guest";
import { AccountHealth } from "@/components/shell/account-health";
import { useT } from "@kalks/i18n/react";
import { AccountProductContext, useTradeMode } from "@/lib/options/mode";
import { useOptionBook } from "@/lib/options/book";
import { useBookFlag } from "@/lib/options/book-flag";
import { useModules } from "@/components/modules";
import { tabOn } from "@/lib/modules";

// Kalks FX Options tabs: their own chunk (loaded when the tab first shows)
const OptionsPositionsTab = dynamic(() => import("@/components/options/positions-tab").then((m) => m.OptionsPositionsTab), { ssr: false });
const SettlementsTab = dynamic(() => import("@/components/options/settlements-tab").then((m) => m.SettlementsTab), { ssr: false });
const OrdersTab = dynamic(() => import("@/components/options/orders-tab").then((m) => m.OrdersTab), { ssr: false });
const ClosedTab = dynamic(() => import("@/components/options/closed-tab").then((m) => m.ClosedTab), { ssr: false });

/** Guest mode: account-only tabs explain what they show once a trading account is logged in. */
const GUEST_TABS: Partial<Record<ToolboxTab, { icon: React.ReactNode; textKey: "toolbox.guest.trade" | "toolbox.guest.history" | "toolbox.guest.exposure" }>> = {
  positions: { icon: <BarChart3 />, textKey: "toolbox.guest.trade" },
  pending: { icon: <BarChart3 />, textKey: "toolbox.guest.trade" },
  history: { icon: <History />, textKey: "toolbox.guest.history" },
  exposure: { icon: <PieChart />, textKey: "toolbox.guest.exposure" },
};

type TabDef = { value: ToolboxTab; label: string; count?: number; tone?: "warn" };

/** The tabs of the panel for the current mode: primary, secondary and the "More" group. */
export function useActivityTabs() {
  const T = useTerminal();
  const t = useT();
  const ai = useAi();
  const mam = useMam();
  const mode = useTradeMode();
  const book = useOptionBook(T.guest ? null : T.account.login);
  // the options order book: an Orders tab while it is live (in CFD mode only while book orders are working)
  const bookFlag = useBookFlag(T.guest ? null : T.account.login);
  const optCount = book.positions.length + book.orders.length;
  const options = mode === "options";
  // an Options account holds no CFD positions or orders (CFD / Options account split): no CFD tabs at all
  const optionsAccount = React.useContext(AccountProductContext) === "options";
  // tabs of modules the broker switched off (News, Calendar, AI Trader, MAM) are left out
  const modules = useModules();
  const cfdPositions: TabDef = { value: "positions", label: options ? t("desk.act.cfdPositions") : t("desk.act.positions"), count: T.positions.length };
  const cfdOrders: TabDef = { value: "pending", label: options ? t("desk.act.cfdOrders") : t("desk.act.orders"), count: T.pendings.length };
  const history: TabDef = { value: "history", label: t("desk.act.history") };
  const optPositions: TabDef = { value: "options", label: options ? t("desk.act.positions") : t("desk.act.optPositions"), count: optCount };
  const optOrders: TabDef = { value: "orders", label: options ? t("desk.act.orders") : t("desk.act.optOrders"), count: bookFlag.open };
  const primary: TabDef[] = options
    ? [optPositions, ...(bookFlag.live || bookFlag.open ? [optOrders] : []), { value: "closed", label: t("desk.act.closed") }, { value: "settlements", label: t("desk.act.settlements") }]
    : [cfdPositions, cfdOrders, history];
  const secondaryAll: TabDef[] = [
    // CFD mode: option positions / working book orders stay in view while there are any
    ...(!options && optCount ? [optPositions] : []),
    ...(!options && bookFlag.open ? [optOrders] : []),
    { value: "alerts", label: t("desk.act.alerts"), count: T.alerts.filter((a) => a.active).length },
    // live builds: real headlines and calendar (services/news); demo builds: sample content
    { value: "news", label: t("desk.act.news"), count: T.live ? undefined : 3 },
    { value: "calendar", label: t("desk.act.calendar") },
  ];
  const moreAll: TabDef[] = [
    ...(options && !optionsAccount ? [cfdPositions, cfdOrders, history] : []),
    { value: "exposure", label: t("desk.act.exposure") },
    { value: "journal", label: t("desk.act.journal") },
    { value: "ai", label: t("desk.act.ai"), count: ai.records.filter((r) => r.status === "active").length },
    // MAM master account or linked client account (live engine only)
    ...(mam?.role ? [{ value: "mam" as const, label: "MAM", count: mam.role === "manager" ? mam.accounts : undefined }] : []),
  ];
  const secondary = secondaryAll.filter((x) => tabOn(modules, x.value));
  const more = moreAll.filter((x) => tabOn(modules, x.value));
  const all = [...primary, ...secondary, ...more];
  // a tab that isn't offered here (Settlements after switching back to CFD…) shows the first primary tab
  const want = T.ws.toolboxTab === "trade" ? "positions" : T.ws.toolboxTab;
  const tab: ToolboxTab = all.some((x) => x.value === want) ? want : primary[0]!.value;
  return { primary, secondary, more, tab };
}

export function Toolbox({ onCollapse, onMaximize, maximized, onTop, health = true }: { onCollapse?: () => void; onMaximize?: () => void; maximized?: boolean; onTop?: () => void; /** the account health strip at the foot (the split layout shows it in the bar below instead) */ health?: boolean }) {
  const T = useTerminal();
  const t = useT();
  const { primary, secondary, more, tab } = useActivityTabs();
  const set = (v: ToolboxTab) => T.setWs({ toolboxTab: v });
  const mode = useTradeMode();
  // opening straight into Options mode: show the option tabs first, as switching modes does (mode-switch.tsx)
  React.useEffect(() => {
    if (mode === "options" && ["positions", "pending", "trade", "history", "exposure"].includes(T.ws.toolboxTab)) set("options");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const inMore = more.find((x) => x.value === tab);
  const moreItems: MenuItem[] = more.map((x) => ({ label: x.count ? `${x.label} (${x.count})` : x.label, checked: x.value === tab, onSelect: () => set(x.value) }));
  const cfdTab = tab === "positions" || tab === "pending";
  return (
    <section data-tour="activity-panel" className="t-glass flex h-full min-h-0 flex-col overflow-hidden rounded-[14px] border border-line">
      <header className="flex h-10 shrink-0 items-center gap-1 px-1.5">
        <PanelTabs value={tab} onChange={set} tabs={primary} />
        <span className="mx-1 h-4 w-px shrink-0 bg-line" aria-hidden />
        <PanelTabs value={tab} onChange={set} tabs={secondary} className="min-w-0" />
        <DropMenu
          width={240}
          items={moreItems}
          trigger={({ toggle, open }) => (
            <button
              type="button"
              onClick={toggle}
              aria-expanded={open}
              aria-haspopup="menu"
              className={cn("flex h-7 shrink-0 items-center gap-1 rounded-[8px] px-2.5 text-[13px] font-medium transition-colors", inMore ? "bg-surface-3 text-fg" : open ? "text-fg" : "text-fg-2 hover:bg-surface-3/50 hover:text-fg")}
            >
              {inMore ? inMore.label : t("desk.act.more")}
              {inMore?.count ? <CountBadge n={inMore.count} tone="accent" /> : null}
              <ChevronDown className={cn("size-3.5 text-fg-3 transition-transform", open && "rotate-180")} />
            </button>
          )}
        />
        <div className="ms-auto flex shrink-0 items-center gap-1">
          {!T.guest && (cfdTab || tab === "history") && <ShareControls />}
          {!T.readOnly && !T.guest && cfdTab && (
            <DropMenu
              align="end"
              width={280}
              items={bulkMenu(T)}
              trigger={({ toggle, open }) => (
                <button onClick={toggle} aria-expanded={open} className={cn("flex h-7 items-center gap-1 rounded-[7px] border border-line px-2.5 text-[12.5px] font-medium transition-colors", open ? "bg-surface-3 text-fg" : "text-fg-2 hover:bg-surface-3 hover:text-fg")}>
                  {t("desk.act.closeMenu")} <ChevronDown className="size-3.5 text-fg-3" />
                </button>
              )}
            />
          )}
          {onMaximize && (
            <IconButton label={maximized ? t("desk.panel.restore") : t("desk.panel.maximise")} onClick={onMaximize} tipSide="top">
              {maximized ? <Minimize2 /> : <Maximize2 />}
            </IconButton>
          )}
          {onCollapse && (
            <IconButton label={t("desk.panel.hide", { panel: t("desk.panel.activity") })} shortcut="Ctrl+T" onClick={onCollapse} tipSide="top">
              <ChevronsDown />
            </IconButton>
          )}
          {/* where the positions live: below the chart (page scroll) or in a panel under it (split) */}
          <Segmented
            size="sm"
            stretch={false}
            label={t("desk.pl.title")}
            value={T.ws.posLayout === "split" ? "split" : "page"}
            onChange={(v) => setPositionsLayout(T, v as "page" | "split")}
            options={[
              { value: "page", label: <span className="sr-only">{t("desk.pl.pageShort")}</span>, icon: <Rows2 />, tip: t("desk.pl.page") },
              { value: "split", label: <span className="sr-only">{t("desk.pl.splitShort")}</span>, icon: <PanelBottom />, tip: t("desk.pl.split") },
            ]}
          />
          {onTop && (
            <button onClick={onTop} className="flex h-7 items-center gap-1 rounded-[7px] px-2 text-[12.5px] font-medium text-fg-2 hover:bg-surface-3 hover:text-fg">
              <ArrowUp className="size-3.5" />
              {t("desk.act.backToChart")}
            </button>
          )}
          {T.ws.posLayout === "split" && (
            <IconButton label={t("desk.pl.toggleTip")} shortcut="Ctrl+T" onClick={() => T.togglePanel("toolbox", false)} tipSide="top">
              <ChevronsDown />
            </IconButton>
          )}
        </div>
      </header>
      <div className="min-h-0 flex-1 border-t border-line">
        {T.guest && GUEST_TABS[tab] ? (
          <GuestNotice icon={GUEST_TABS[tab]!.icon} text={t(GUEST_TABS[tab]!.textKey)} />
        ) : T.guest && (tab === "options" || tab === "settlements" || tab === "orders" || tab === "closed") ? (
          <GuestNotice icon={<Layers />} text={t("trader.opt.guest.text")} />
        ) : (
          <ToolboxBody tab={tab} live={T.live} />
        )}
      </div>
      {health && <AccountHealth className="border-t border-line" />}
    </section>
  );
}

function ToolboxBody({ tab, live }: { tab: ToolboxTab; live: boolean }) {
  return (
    <>
      {(tab === "positions" || tab === "trade") && <PositionsTab />}
      {tab === "pending" && <PendingTab />}
      {tab === "history" && <HistoryTab />}
      {tab === "exposure" && <ExposureTab />}
      {tab === "news" && (live ? <LiveNewsTab /> : <NewsTab />)}
      {tab === "calendar" && (live ? <LiveCalendarTab /> : <CalendarTab />)}
      {tab === "alerts" && <AlertsTab />}
      {tab === "journal" && <JournalTab />}
      {tab === "ai" && <AiTraderTab />}
      {tab === "mam" && <MamTab />}
      {tab === "options" && <OptionsPositionsTab />}
      {tab === "settlements" && <SettlementsTab />}
      {tab === "orders" && <OrdersTab />}
      {tab === "closed" && <ClosedTab />}
    </>
  );
}
