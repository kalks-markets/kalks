# Kalks Trader: terminal design

The design of Kalks Trader (`apps/terminal`) on desktop, CFD and Options modes. It has four parts: what the terminal can do (the feature inventory) with the problems found in the old shell, the design spec the redesign follows, the result of the first redesign (v2, exchange columns), and the current chart-first layout (v3, MT5 web clean + Delta Exchange page scroll, Part 4), which is what the code does now. The phone layout (`components/mobile`, `components/options/mobile.tsx`) follows the same principles and tokens; see 3.4 "Notes for the phone layout".

Screens referenced below: `scratchpad/redesign/before/*` (old shell) and `scratchpad/redesign/after/*` (new shell), captured at 1600×950, 1470×900 (the founder's MacBook width) and 1366×768, dark and light, on the local demo build (`NEXT_PUBLIC_KALKS_MODE=demo`, account 80412337).

---

## Part 1. Audit

### 1.1 Feature inventory

Every control, menu item, shortcut, setting and action the old desktop terminal offered. Each one must survive the redesign and be reachable in at most two clicks (a keyboard shortcut also counts, but never as the only way). "New place" and "Clicks" are filled in by Part 3; the IDs are used there.

**A. Title bar**

| ID | Feature | Old place |
|---|---|---|
| A1 | Brand (logo, "Kalks Trader") | title bar |
| A2 | CFD / Options mode switch, "new" dot on Options | title bar |
| A3 | Cent (USC) badge, Read-only badge | title bar |
| A4 | Account switcher: every login with type, group, mode, leverage, server, live equity; switch account | title bar chip ▾ |
| A5 | Log in to another account (engine), log out of this account (engine) | switcher ▾ |
| A6 | Refill demo (refills left) / Deposit (live), Manage accounts ↗ | switcher ▾ |
| A7 | Guest chip: guest card, Log in, Open account, Client Area | title bar (guest) |
| A8 | Symbol search button (⌘K) | title bar |
| A9 | New Order button (F9); guest: locked New Order | title bar |
| A10 | One-click trading toggle (F10) | title bar |
| A11 | Chart layout 1 / 2 side by side / 2 stacked / 4 (Alt+1…4) | title bar icons |
| A12 | Notifications bell: account inbox + terminal notifications, unread badge, mark all read, clear | title bar |
| A13 | Interface language (22 languages) | title bar flag |
| A14 | Dark / light theme | title bar icon |
| A15 | Full screen (F11) | title bar icon |
| A16 | Client Area ↗ | title bar link |
| A17 | User menu: connected server, Client Area, Profile & security, Keyboard shortcuts, Log out; guest: Log in to trade, Open account, Shortcuts | avatar ▾ |

**B. Menu bar** (File · View · Insert · Charts · Tools · Help)

| ID | Item | Old place |
|---|---|---|
| B1 | New chart ▸ 16 symbols | File |
| B2 | Close chart | File |
| B3 | Profiles ▸ Default / Scalping / Analysis | File |
| B4 | Save as picture | File |
| B5 | Login to trade (engine: log in dialog; demo: back to login) | File |
| B6 | Open an account ↗ | File |
| B7 | Refill demo (n left) | File |
| B8 | Client Area ↗, Log out | File |
| B9 | Guest: Login to trade, Open account, Sign in to Client Area | File |
| B10 | Show/hide Market Watch (Ctrl+M), Navigator, Order·DOM (Ctrl+D), Toolbox (Ctrl+T) | View |
| B11 | Layout presets ▸ Trading / Chart focus / Analysis / Scalper | View |
| B12 | Theme ▸, Language ▸ | View |
| B13 | Full screen (F11) | View |
| B14 | Reset workspace | View |
| B15 | Indicators list (Ctrl+I) | Insert |
| B16 | Indicator categories ▸ every indicator | Insert |
| B17 | Objects ▸ Horizontal line / Trend line / Fibonacci / Rectangle | Insert |
| B18 | Price alert | Insert |
| B19 | Chart type: Candles / Bars / Line / Area | Charts |
| B20 | Timeframes ▸ M1 … MN | Charts |
| B21 | Templates ▸ built-in and saved, Save template | Charts |
| B22 | Layout ▸ 1 / 2h / 2v / 4 | Charts |
| B23 | New chart tab | Charts |
| B24 | Zoom in (+), Zoom out (−) | Charts |
| B25 | Delete all objects | Charts |
| B26 | New order (F9) | Tools |
| B27 | One-click trading (F10) | Tools |
| B28 | Sound on fills | Tools |
| B29 | Max deviation ▸ any price / 0 / 3 / 5 / 10 / 20 / 50 / 100 points | Tools |
| B30 | Price alerts, History, Journal (open the toolbox tab) | Tools |
| B31 | Options… (settings dialog) | Tools |
| B32 | Keyboard shortcuts (F1) | Help |
| B33 | Help topics ↗ (Academy), Contact support ↗ | Help |
| B34 | About | Help |

**C. Market Watch**

| ID | Feature |
|---|---|
| C1 | Tabs Symbols / Details (cards) / Favourites (count) |
| C2 | Server clock in the header |
| C3 | Asset-class chips with counts (All, Forex, Metals, Indices, Energies, Crypto, Stocks, ★) |
| C4 | Search box |
| C5 | Table: symbol, bid, ask, spread, daily change (spread/change dropped when narrow) |
| C6 | Double-click a row: open in the active chart |
| C7 | Hover card: day low/high bar, spread, range, change |
| C8 | Context menu: New order, Chart window (new tab), Open in active chart, Depth of market, Specification, Add/remove favourite, Hide, Show all |
| C9 | Footer: shown / total |
| C10 | Collapse to an edge rail |

**D. Navigator** (second left panel)

| ID | Feature |
|---|---|
| D1 | Accounts (click to switch) |
| D2 | Indicators by category (double-click adds to the active chart; count of active) |
| D3 | Strategies (attach / running, today's P&L) |
| D4 | Scripts: Close all, Close profitable, Close losing, Delete all pendings, Breakeven all |

**E. Charts**

| ID | Feature |
|---|---|
| E1 | Chart tabs: activate, close (× or middle-click), "visible in grid" dot, New tab (+) |
| E2 | Symbol picker (popular symbols + "Search all") |
| E3 | Timeframes M1 … MN |
| E4 | Chart types (4) |
| E5 | Indicators button with count → indicator dialogs (list, settings, templates) |
| E6 | Templates ▾: built-in, my templates, save, delete ▸, apply to all charts |
| E7 | Crosshair (Ctrl+F), zoom in, zoom out, reset view, screenshot |
| E8 | Active indicators summary |
| E9 | Drawing rail: cursor, crosshair, horizontal line, trend, Fibonacci, rectangle, text (placeholder), ruler (placeholder), delete all objects (count) |
| E10 | Grid layouts 1 / 2h / 2v / 4 |
| E11 | In-chart legend (OHLC, indicators, show/hide, "+N more") and quote line (bid, ask, spread, last, market closed) |
| E12 | Chart quick-trade box: Sell / Buy, lot stepper, spread, collapse |
| E13 | Trade lines: position, SL, TP, pending order, alert; drag to modify; × to close/remove; double-click position → dialog; drag a position line to project SL/TP |
| E14 | Chart context menu: buy/sell limit/stop at the clicked price, New order, alert at price, horizontal line, timeframes ▸, chart type ▸, indicators ▸ (incl. remove all), crosshair, zoom, save as picture, delete all objects |
| E15 | Select a drawing, Delete removes it, Esc cancels a tool |

**F. Order panel** (right)

| ID | Feature |
|---|---|
| F1 | Header with symbol; collapse |
| F2 | Tabs Order / Depth / Info |
| F3 | Symbol select |
| F4 | Order type: Market / Limit / Stop / Stop limit |
| F5 | Volume stepper (lots), presets 0.01 / 0.1 / 0.5 / 1 / 2, units, pip value |
| F6 | Pending price, stop-limit price, expiry GTC / Today / Date (+ date) |
| F7 | OCO: opposite twin order at a price |
| F8 | SL / TP in pips or price, money at SL/TP |
| F9 | Trailing stop (pips) |
| F10 | Risk calculator: risk % or $, SL distance, lots / pip value / margin, Apply |
| F11 | Comment, max deviation (order window) |
| F12 | Summary: margin, free margin, leverage · spread, deviation |
| F13 | Sell / Buy buttons with live prices and spread |
| F14 | Market closed note |
| F15 | One-click trading switch |
| F16 | Guest ticket (prices, locked buttons, Log in / Open account); read-only notice |
| F17 | Depth of market: click a level = limit order, market buttons, volume, my orders, imbalance bar |
| F18 | Info: price, range, spread, tick sparkline, contract specification, swaps, sessions |

**G. Toolbox** (bottom)

| ID | Feature |
|---|---|
| G1 | Tabs: Trade (positions + pending), History, Exposure, News, Calendar, Alerts, Journal, AI Trader, MAM (when the account has one); Options mode: Options, Orders (book), Closed, Settlements |
| G2 | Share controls (select trades, share link, my links) |
| G3 | Bulk close ▾: all, profitable, losing, buys, sells, by symbol ▸, delete all pendings |
| G4 | Maximise / restore, hide (edge rail) |
| G5 | Position row: symbol, ticket, time, side, volume, open price, SL, TP, current, swap, commission, profit, source/comment; close × |
| G6 | Position context menu: close, close partially, close 50%, modify, SL to breakeven, close by ▸ (hedging), share, bulk close ▸, show on chart, copy ticket |
| G7 | Double-click a position/order: modify dialog |
| G8 | Pending row: type, OCO tag, price (→ stop-limit), SL, TP, current, expiry, placed; delete ×; context menu (modify, delete, delete all, share, show on chart) |
| G9 | Account summary row: balance, equity, margin, free margin, margin level, margin call / stop out tag, credit, floating P&L |
| G10 | History: period, symbol filter, All/CFD/Options, trades / win rate / gross / PF, Report, rows, option rows with share, row menu (share, copy ticket), totals (profit, credit, deposit, withdrawal, balance) |
| G11 | Exposure by currency with bar graph |
| G12 | News (live headlines or demo cards) |
| G13 | Economic calendar |
| G14 | Price alerts: create / edit (symbol, condition, price, note), enable/disable, delete |
| G15 | Journal: source filter, text filter, copy, clear |
| G16 | AI Trader: describe a strategy, examples, build, strategy cards, stop all AI |
| G17 | MAM tab |
| G18 | Options positions cards, option orders, closed options, settlements |
| G19 | Guest notices in account-only tabs |

**H. Status bar**

| ID | Feature |
|---|---|
| H1 | Price feed state (connected / simulated / connecting), server, latency (p95) |
| H2 | Trade-server stream state (engine builds) |
| H3 | Layout profile name |
| H4 | Quotes per second |
| H5 | Guest: no account · Log in |
| H6 | Currency · leverage |
| H7 | Floating P&L, margin level |
| H8 | "One-click ON" |
| H9 | Server time GMT+3 |
| H10 | Terminal load meter (demo builds; simulated) |
| H11 | Help (F1) |

**I. Dialogs and layers**

| ID | Feature |
|---|---|
| I1 | New order window (F9): tick chart, bid/ask, low/high/spread/change, full ticket incl. comment and max deviation |
| I2 | Position dialog: profit, pips, details; partial close 25/50/100%; SL/TP with ±10/25/50 pip nudges and clear; trailing stop; breakeven; close by |
| I3 | Pending order dialog: price, SL, TP, modify, delete |
| I4 | Symbol search (⌘K): asset chips, ↑↓, Enter = chart, Alt+Enter = new chart, Shift+Enter = new order |
| I5 | Keyboard shortcuts (F1) |
| I6 | Symbol specification |
| I7 | About |
| I8 | Settings ("Options…"): one-click, default lot, max deviation, sounds, theme, language, connection |
| I9 | Indicator list / settings / save template dialogs |
| I10 | Share layer (create link, my links) |
| I11 | Log in to another account (engine builds) |

**J. Global**

| ID | Feature |
|---|---|
| J1 | Shortcuts: F9, F10, F1, F11, Ctrl/⌘+K, Ctrl+I, Alt+1…4, Ctrl+M, Ctrl+T, Ctrl+D, Ctrl+F, Esc, Delete/Backspace, + / − |
| J2 | Staff-session and restrictions banner; copy-trading banner |
| J3 | Toasts placed clear of the chart toolbar and order panel |
| J4 | Links: `?symbol=`, `?side=`, `?mode=options&u=`, `?sso=`, `?account=` |
| J5 | Guest mode (live build without an account), read-only (investor) mode |
| J6 | 22 languages, right-to-left text (ar, fa, ur) inside the LTR workspace |
| J7 | Resizable panels with saved sizes; small laptops start with Market Watch tucked |

**K. Options mode**

| ID | Feature |
|---|---|
| K1 | Instruments list (search, spot, change, ATM IV in Pro) |
| K2 | Centre tabs: Option chain, Chart (selected series tag), Book (when live), Analytics |
| K3 | Underlying stats, stream dot, Strategy builder button |
| K4 | Expiry bar (every expiry with time left, cut time) |
| K5 | Chain: Calls / Both / Puts, Columns Simple / Standard / Pro, book badge, hint, Market-maker rules, How options work, strikes range, put/call ratio, in-the-money shading |
| K6 | Right panel: Quick trade (guided) and Order (ticket) with leg count |
| K7 | Strategy builder drawer |
| K8 | Option toasts (settled, knocked out, fills) |

### 1.2 UX problems found

Grouped by what they cost a first-time trader. The screenshot names refer to `scratchpad/redesign/before/`.

**Confusing**

1. The MT5 menu bar (File / View / Insert / Charts / Tools / Help) holds unrelated things together: one-click trading, sounds and max deviation sit under "Tools", language under "View", log out under "File". Nobody new looks for trading settings in a menu called Tools (`02-menu-5-tools`).
2. "Options" means two different things: the CFD | Options product switch and "Tools › Options…", the settings window.
3. Clicking SELL or BUY in the order panel sends the order at once, with no step that says what is about to happen. The words on the buttons are "SELL 2,662.56", not "Sell 0.50 lot XAUUSD at market".
4. "One-click ON" sits in the status bar in red, which reads like an error, and the title-bar toggle wraps to two lines ("One- / click ON").
5. Market Watch needs a double-click to open a market. The only hint is "dbl-click: chart" in 10 px text in the footer.
6. Jargon without explanation: "Pip 5.00", "SP", "CHG%", "Stop-Lmt", "Deviation", "Margin level 2862.90%", "PF". Nothing explains margin, free margin, lot, pip, swap, hedging or netting.
7. Favourites appear twice in Market Watch: a Favourites tab and a ★ chip.
8. In Options mode the toolbox shows 13 tabs mixing option tabs (Options, Orders, Closed, Settlements) with CFD tabs (Trade, History, Exposure…) in one row (`20-options-main`).

**Cluttered**

9. The title bar has about 17 controls, ten of them icon-only (layouts ×4, bell, flag, theme, full screen, avatar, search on small screens).
10. The status bar has 11 cells in 10.5 px mono: profile, quotes per second, currency/leverage, P&L, margin level, "One-click ON" and a simulated CPU-load meter.
11. The Trade tab is a 14-column table (min-width 1120 px) with ticket, time, commission and comment columns at the same weight as profit.
12. The Navigator panel takes a third of the left column at all sizes with things available elsewhere (accounts, indicators, bulk-close scripts). At 1366×768 only five markets are visible (`1366-dark-01-cfd-main`).

**Hidden**

13. At 1366×768 the Sell and Buy buttons are below the fold of the order panel: the panel must be scrolled to trade (`1366-dark-01-cfd-main`).
14. Account health (equity, free margin, margin level) is only in the Trade tab's footer row in 12 px mono, and disappears on every other tab.
15. Close partially, close 50 %, breakeven, close by, share, show on chart, hide symbol, depth, specification: right-click only.
16. Inline SL/TP editing does not exist: a double-click opens a dialog.
17. The close × on a position row is at 60 % opacity until hover.

**Inconsistent / hard to read**

18. Text sizes of 9.5 – 11 px everywhere (chip counts, table headers, labels, prices in Market Watch at 10.5 px).
19. Hit targets of 20 – 24 px (panel icons, row ×, timeframe pills, volume presets at 10 px).
20. Uppercase micro-labels on every panel (MARKET WATCH, NAVIGATOR, ORDER, TOOLBOX) add noise and compete with data.
21. Contrast: the tertiary text colour is 3.3:1 in dark and 2.8:1 in light (AA needs 4.5:1); light-theme green text is 3.3:1; white on the green Buy button is 2.3:1.
22. Symbol names are truncated in Market Watch at 1366 ("EURU…") and the asset chips are cut off ("Ene").
23. Two placeholder drawing tools (text, ruler) look like working tools and only show "coming soon".
24. The symbol is chosen in three places that do not look related (chart toolbar dropdown, order-panel select, Market Watch).

**Founder review of the first pass (button sizes)**

25. Button sizes were inconsistent: some far too big (the option ticket's SELL / BUY blocks about 85 px tall with a wrapping "you get, per contract · 3,449.6p" line; 48 px green / red place-order buttons; a large New Order pill), others too small or not recognisable as buttons (contract presets "1 2 5 10", strike range "±6 ±10 ±20 All", the Call / Put switch, "+ Add leg" and the trash icon, volume presets, timeframe pills).
26. Clickable Buy / Sell prices in the option chain's Standard / Pro columns were plain coloured text.
27. The layout gave too little room to the market and read as an MT5 tool rather than a modern exchange: no symbol header, no order book next to the order form, tall rows, hard dividers.
28. The direction: compact and consistent (sm 24 / md 28 / lg 32, trade buttons ≤ 44 px, rows 30–32 px), cards with frosted materials, the Exness / Delta Exchange layout, and one primary action per panel.

---

## Part 2. Design spec

The target is an exchange-grade terminal in the style of the Exness Terminal and Delta Exchange: the chart is the hero, every area is a calm frosted card, controls are compact and consistent, colour carries meaning only, and every money-moving action names itself.

### 2.1 Principles

1. **Say what will happen.** Every action that moves money names itself in full ("Buy 0.50 lot XAUUSD at market", "Place buy limit 0.50 lot XAUUSD at 2,640.00"). Numbers that matter come with their consequence: margin, pip value, free margin after, risk and reward in money.
2. **One place per job, where you expect it.** The chart fills the first screen; Instruments and the Order book share the right-hand column; Buy / Sell sit on the chart and the order form opens as a popup in the middle; your positions are one scroll below; account health stays in the bar at the foot of the first screen; settings and help are in the ☰ menu.
3. **One primary action per panel.** The order form has one accent button (confirm); the top bar has one (Deposit). Everything else is secondary, ghost or a pill.
4. **Compact, consistent, obviously clickable.** One size scale for every control (§2.4). Small controls stay small (24–28 px) but always look like controls: a subtle fill or border, hover, pressed, selected and disabled states. Nothing oversized; no tall saturated blocks.
5. **Calm colour.** Blue = buy / profit / up, red = sell / loss / down (the founder's decision of 2026-10-09: candles, Buy / Sell, P&L, change %, the buy position line, the ask line and TP are blue; the sell position line and SL red), ember (the brand accent, replaceable per broker) = the selected item and the primary action, amber = warning. No red unless something is wrong or it is a sell. Blue is Kalks Trader's alone: the Client Area keeps the @kalks/ui green for success states.
6. **Plain words, expert depth.** Labels a first-time trader understands with a (?) that explains the term; pros keep every column, shortcut and tool (folded into More, ⋯ menus and the command palette, never removed).
7. **Same product in both modes.** CFD and Options share the shell, the columns, the cards, the order-book component and the order-form pattern.

### 2.2 Layout and information architecture (desktop ≥ 1024 px)

The chart-first layout (v3, the founder's MT5 web / Delta Exchange directions of 2026-10-07). The v2 column layout it replaced is described in Part 3.

```
┌ Top bar 48 (sticky, frosted over the scrolling page) ─────────────────────────────────────────────────┐
│ ☰  K Kalks Trader  [CFD|Options]  🔍 Search markets and actions ⌘K     LIVE 80412337 ▣ 26,308 USD ▾ [Deposit] 🔔 ◯ │
├───────────────────────────────────────────────────────────────────────────────┬──────────────────────┤
│ XAUUSD M15 ▾ | ▥▾ M1 M5 M15 M30 H1 H4 D1 W1 MN | 🛒 New order | ⊕ ⊖ ⛶ | ∿2 ⧉ ▦ 🔔 📷   ⤢ ⛶ │ Instruments | Order book | Ticks » │
│┌─┐ XAUUSD, M15  Gold vs US Dollar  O H L C                                    │ 🔍 Search symbol  ≣ ▦ │
││✎│ EMA 50 · SMA 20                                                            │ ☆ All Forex Metals …→ │
││ │ [Sell 2,662.31 | − 0.50 + | Buy 2,662.49]   (the only Buy / Sell)          │ SYMBOL  BID  ASK DAILY│
││ │                                                                             │ virtualised rows 32px │
││ │                         chart (hero)                                         │ (1,000+ markets)      │
││ │ K                                                                            │ 28 of 28 markets      │
├┴─┴─────────────────────────────────────────────────────────────────────────────┴──────────────────────┤
│ Balance ⓘ · Equity ⓘ · Floating P&L ⓘ · Margin ⓘ · Free margin ⓘ · Margin level ⓘ ▬ Safe   ● Connected 12:46 GMT+3 [Positions 3 ↓] │
└───────────────────────────────────────────────────────────────────────────────────────────────────────┘
   ── the page scrolls ──
┌ [Positions 3] Orders 2 History │ Alerts 2 News 3 Calendar More ▾          Share ⛓ Close positions ▾ ↑ Back to chart ┐
│ MARKET  OPENED  SIDE  VOLUME  OPEN → CURRENT  STOP LOSS  TAKE PROFIT  SWAP  COMMISSION  P&L   [× Close] ⋯        │
│ … full width, one viewport tall …                                                                                 │
│ Balance · Equity · Floating P&L · Margin · Free margin · Margin level                                             │
└────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

- **Top bar (48 px, sticky, frosted).** ☰ menu · brand · a small CFD | Options switch (28 px, sliding frosted thumb) · search (⌘K) · account pill (badges, login, equity; group, mode, leverage in its tooltip and dropdown) · Deposit / Top up demo (the bar's only accent button) · notifications · profile. The ☰ menu (MT5 web) holds: trading accounts ▸, chart settings ▸ (type, indicators, template, picture, grid, layouts, panels), one-click trading (F10), dark / light theme, language ▸, sounds, max price change ▸, full screen, all settings, keyboard shortcuts, trading terms explained, the tour, help center, contact support, about. Guest: the guest chip, Open account, Log in.
- **Chart card (the first screen, hero).** ONE toolbar row: open charts (every chart as a tab when the card is ≥ 1240 px wide, otherwise the active chart as a chip with the others in its menu) · chart type ▾ · timeframes M1…MN (a menu below 860 px) · **New order** (F9) · zoom in / out / fit · Indicators (n) · Templates ▾ · Layout ▾ (grid, presets, panels, reset) · price alert · picture · **Full chart** (Shift+F) · browser full screen (F11). A thin drawing rail on the left. On the plot only: the legend (symbol, timeframe, name, OHLC, indicators) top-left, the Buy / Sell box (sell price · volume − + · spread · buy price) under it, trade lines (a position without a stop loss / take profit shows two small square handles after its P&L, **S** in a red outline and **T** in a blue one: drag one out to place that stop, held on the side of the price the server accepts and the stops level away; a click puts it at the order tickets' starting distance, max(10 pips, 2 × spread) for SL and twice that for TP, from the price the position closes at; the handle goes once its line exists — `chart/trade-handles.ts`, same rules in the app's `assets/chart/chart.html`), and the small Kalks K in the bottom-left corner (as TradingView shows its logo; drawn on the canvas, never on the scales). No watermark, no quote strip.
- **Right-hand column (~344 px, resizable 288–480, collapsible to an edge tab, Ctrl+M).** Tabs: **Instruments** | **Order book** | **Ticks** (Options: the underlyings | the selected option's book or the spot depth | Trades / Ticks); the Navigator joins as a tab when chosen in Layout ▾. Instruments (MT5 web "Search symbol"): search (symbol first, then name; ↑ ↓ Enter), a one-row asset-class filter that scrolls sideways (☆ Favourites, All, then every class the catalogue has, new ones included), list or cards, rows Symbol · Bid · Ask · Daily %, virtualised for 1,000+ markets. A row opens its market on the chart; Bid / Ask open the order form with that side; double-click = New order; hover = ☆ and ⋯; right-click = the row menu; hover card = day range.
- **Order form = a centred popup.** Opened by Buy / Sell on the chart (one-click off), New order, F9 / Ctrl+D, a Bid / Ask in Instruments, a level in the order book (limit at that price), empty states. 440 px wide; the MT5 tick chart can be shown beside it (header button, remembered). The form itself is the Exness flow of §2.5.
- **Bar at the foot of the first screen (40 px).** Account health (balance, equity, floating P&L, margin, free margin, margin level with its meter and state, each with (?)) · connection (prices: connected / simulated with the delay; the trade-server stream on live builds; server name and quotes per second in the tooltip) · server time · **Positions (n)**: ↓ scrolls the page down (full page) or ⌄ / ⌃ collapses and expands the panel (split).
- **Positions: two layouts** (a toggle in the positions header, ☰ › Chart settings › Positions, remembered). **Full page** (default): the section sits below the first screen, full width and one viewport tall, reached by scrolling, "Positions (n) ↓" or Ctrl+T; "↑ Back to chart" returns. **Split**: the section is a resizable panel under the chart on the first screen (40 % by default, 18–65 %), no page scroll; "Positions (n) ⌄ / ⌃" and Ctrl+T collapse and expand it. Either way: pill tabs Positions · Orders · History (Options: Positions · Orders · Closed · Settlements), then Alerts · News · Calendar, then More ▾; Share, links, Close positions ▾ (confirmed); sticky table headers and a visible Close on every row; the chart never remounts when switching.
- **Toasts.** A compact stack right under the 🔔 (aligned to its right edge, 320 px wide), at most 3, newest on top, in and out in ~150 ms; info and success leave after 3 s, warnings 5 s, errors 6 s, paused while hovered; every one stays in the bell's list with the unread count. Full chart: the window's top-right corner. Frosted like the other overlays.
- **Full chart (Shift+F, toolbar button).** The chart covers the window (top bar, column and bar hidden); an arrow tab on the right edge slides the column in over the chart (opaque, below the toolbar row so Exit stays reachable); Esc closes the column, then Full chart. Browser full screen is a separate button (F11).
- **Options mode, same frame.** The main card: one row (underlying ▾ → the Instruments tab, spot and change, Expiry (?) · Time to the cut (?) · ATM IV (?) · state, **Quick trade** (accent), Strategy builder, Full chart, full screen), then ONE tab row: **Option chain · Underlying · Option · Both · Analytics · Book** (keys 1–6, Book while the book is live). Each view fills the card at full size; the expiry chips and the selected option ("EURUSD 1.1250 Call · Thu, Oct 08") stay on screen in every view; one toolbar row under the tabs carries only what that view needs (timeframes for the chart views, plus side-by-side / stacked for Both; view, columns and hints for the chain). Option chain is the first view on a first visit, then the last one is remembered. Underlying: the underlying's chart with strike, breakeven and position lines. Option: the selected strike's premium chart, or "Pick a strike in the Option chain…" with [Open option chain]. Both: the two charts side by side or stacked, with one timeframe and a shared crosshair time. Quick trade and Order open as a popup (Quick trade | Order), by themselves when a price in the chain, a level in the book or "Send to ticket" picks an option. A wide desktop chain starts on Standard columns.
- **Command palette (⌘K).** One search for markets and every action by name, as before.

### 2.3 Tokens and materials

Defined in `apps/terminal/app/globals.css` on top of `@kalks/ui/styles.css` (packages/ui is unchanged). Brand colours are never hard-coded: stronger variants are mixed from the brand colour.

**Spacing** (4 px base): 4 · 8 · 12 · 16 · 24. Panel gaps and gutters 8; card padding 8–12; between groups 8–12.

**Type** (Geist Sans for UI, Geist Mono tabular numerals for numbers): 13 px UI text (buttons, tabs, menus, fields) · 12–12.5 px dense tables, labels and hints · 10.5 px uppercase grey caps for column headers and stat labels only · 14 px prices in controls, 15 px book mid, 20 px the symbol header price. Sentence case everywhere else; figures keep `dir="ltr"` inside RTL text.

**Radii:** 6 small controls · 7–8 buttons, inputs, pills · 10–11 field rows and grouped controls · 12 menus and popovers · 14 cards · 16 dialogs and the tour.

**Colours** (dark / light):

| Token | Dark | Light | Use |
|---|---|---|---|
| backdrop | gradient #0a0a0e → #050507 with a faint ember glow | #ecebe7 → #e3e1dc | app background behind the cards |
| panel (card) | #0e0e12 at 90 % | #ffffff at 86 % | cards |
| panel-2 | #15151a | #f5f3f0 | fields, grouped controls, sticky rows |
| surface-3 | #1e1e24 | #f1eee9 | hover, active pill |
| fg / fg-2 / fg-3 | #f5f5f7 / #a1a1aa / **#8b8b96** | #0e0e12 / #55555f / **#6b6b75** | text; fg-3 now ≥ 4.5:1 (5.8:1 / 5.3:1) |
| up / down | **#2f7bff** / #f04438 | **#1f5fe0** / #dc2626 | buy, profit, up, TP / sell, loss, down, SL (blue 4.9:1 on the dark panel, 5.6:1 on white); up-soft = the blue at 12 % |
| buy-fill / sell-fill | **#1f5fe0** / #dc2626 | same | filled blue / red with white text (5.6:1 / 4.8:1) |
| accent / accent-strong / accent-text | brand ember; ember mixed 80 % with black; ember | same; same; ember mixed 78 % with black | selection; the primary button (white text ≈ 4.8:1); accent text |
| warn / info | #f59e0b / #38bdf8 | **#b45309** / **#0369a1** | warnings / neutral notices |

**Materials ("frosted, never see-through").** `.t-glass` (cards): ~90 % opaque surface over the backdrop gradient, 1 px light edge, soft shadow, 14 px radius. `.t-bar` (the order form's sticky footer): the stronger 92 % surface, no blur. `.t-glass-strong` (the sticky top bar, which the page scrolls under; the mode-switch thumb; menus, popovers, tooltips, dialogs, the palette, the tour card; the options panel arrow): the same plus `backdrop-filter: blur(24px) saturate(160%)`, because they float over content that moves. Solid fallback when `backdrop-filter` is unsupported or `prefers-reduced-transparency: reduce` is set. **Performance rule:** no backdrop blur on anything that stays on screen while prices stream — cards and bars sit over a static gradient (blur shows nothing there) or hold ticking values (the blur re-runs every tick). Measured at 1470×900 with prices streaming and the order book open (headless Chrome, software rendering): blur on cards 23 fps; blur on the bars 26–37 fps; blur only on overlays (as shipped: nothing blurred on the main screen) 57–58 fps, the same as no blur at all.

**States:** hover = surface-3 (or 50 % of it for ghost controls) · pressed = 95 % brightness · selected = filled grey pill, soft tint (side buttons) or accent-soft · focus = 2 px accent outline at 65 %, 1 px offset, on every control (base layer) · disabled = 45 % opacity · motion 150–200 ms; live ticks keep the digit flash; changed order-book levels flash at 12 % in their side colour; depth bars are 24 % (dark) / 15 % (light) at the price end.

### 2.4 Component rules and the size scale

Shared pieces live in `apps/terminal/components/ui/` (`kit.tsx` new, `panel.tsx`, `primitives.tsx`, `menu.tsx` restyled); the order book is `components/order/order-book.tsx`. All sizes are props, never breakpoints, so the phone layout can ask for larger ones.

**One size scale (desktop).** Every control uses it; no ad-hoc heights.

| Size | Height | Text | Use |
|---|---|---|---|
| sm | 24 px | 12 px | presets, chips, strike range, inline row actions (Close, Edit, Cancel, ⋯), tiny toggles |
| md (default) | 28 px | 13 px | toolbar buttons, tabs / pill tabs, menu rows, inputs, selects, segmented controls, icon buttons |
| lg | 32 px | 13 px semibold | top-bar controls (search, account pill, Deposit, mode switch), field rows' outer size 36 |
| trade | 44 px | 12.5 px label + 14 px price, 11.5 px hint | Sell / Buy, Call / Put, Up / Down outcome pairs (one line label + price, one short line under) |
| xl | 40 px | 14 px semibold | the one primary action of a panel (confirm, place order) |

Rows: Markets and chain 32 px · tables 36 px · order book 22 px · menus 28 px.

- **Buttons** (`Button`): variants primary (accent-strong fill), secondary (panel-2 + line), ghost, outline, soft (accent-soft), buy, sell, danger. Label first; icon 14–16 px.
- **Icon buttons** (`IconButton`): sm 24, md 28, lg 32; `label` is required (aria-label + tooltip) and `shortcut` is shown in the tooltip.
- **Pill tabs** (`PanelTabs`): every card header; the active tab is a filled grey pill, the others plain text; count badges; "More ▾" holds overflow and shows the active one.
- **Segmented control** (`Segmented`): one rounded strip, equal cells, a filled thumb for the selection (Call / Put, list / cards, risk % / $, theme). The CFD | Options switch is the animated variant.
- **Field rows** (`FieldRow` + `InlineNumber`): full-width rounded row, grey label left, value or control right (− value unit +; arrow keys and the wheel step too).
- **Quick-adjust strip** (`QuickStrip`): one rounded strip of equal cells (volume 0.01 0.1 0.5 1 2, contracts 1 2 5 10).
- **Summary rows** (`SummaryRow`): grey label, mono value right.
- **Stats** (`Stat`): 10.5 px grey caps label over a 13 px mono value (symbol header).
- **Chips** (`Chip`, segment chips): 24–26 px pills with a border, filled when selected.
- **Price pills:** clickable prices are always pills (tinted red for sell / bid, green for buy / ask, border on hover) — Markets rows, the chain's Buy / Sell columns in every preset.
- **Inputs, selects, steppers:** md 28 px, `Stepper` sizes sm (legacy dense) / md 28 / lg 32.
- **Tables:** sticky 10.5 px grey caps headers, 36 px rows, numbers right-aligned in mono, the row's main action a visible small labelled button (Close / Cancel), the rest in ⋯ = right-click menu.
- **Menus** (12 px radius, frosted): 28 px rows, 13 px text, shortcut hints right-aligned; small caps headers.
- **Dialogs** (16 px radius, frosted): 44 px header, Esc closes, primary action right. Destructive bulk actions go through the confirm dialog.
- **Tooltips** (`Tip`): frosted, 12.5 px, 350 ms delay (120 ms for (?) help), never the only place for information a beginner needs; `HelpTip` explains a term from the glossary.
- **Empty states:** icon, one sentence, one action.

### 2.5 Behaviour rules

- **Trading flow (Exness style).** Buy / Sell live on the chart. One-click off (the default for new workspaces): the click opens the order popup with that side chosen; set the size, optionally SL / TP, then the one confirm button names the trade. One-click on (an explicit switch in the form, the ☰ menu and F10): the chart's Sell / Buy and order-book levels send at once.
- **SL / TP.** Switching a stop on fills a starting distance (2 × spread, at least 10 pips; TP twice the SL). Values can be typed as price, pips or money; the form shows the resulting price, distance and money; stops are converted to prices at the moment of sending.
- **Order book clicks** open the order popup with a limit order at the level (bid level → buy limit, ask level → sell limit, as the old DOM; one-click on: sent at once); the options book keeps its convention (offer → buy, bid → sell) and opens the options popup.
- **Selection** anywhere (Instruments, palette, chart tab, position "Show on chart") updates the chart, the legend, the Buy / Sell box and the order book.
- **Destructive bulk actions** (close all / profitable / losing / buys / sells / by symbol, cancel all orders) always ask for confirmation.
- **First run.** CFD: find a market → Sell or Buy on the chart → the order form → positions below → the ☰ menu (four steps for guests). Options (the first time Options mode opens): the underlying → Quick trade → the view tabs (one at a time, keys 1–6) → positions and settlement → the words explained. Dismissible, shown once each, restartable from ☰ › Take the tour. Not on phones or read-only sessions.
- **Glossary.** CFD: balance, equity, floating P&L, margin, free margin, margin level, lot, pip, spread, leverage, swap, stop loss, take profit, hedging, netting, commission, credit. Options: call, put, strike, expiry, the cut, premium, breakeven, in / at / out of the money, implied volatility, the Greeks (delta, gamma, theta, vega), settlement, selling an option. In (?) tips and ☰ › Trading terms explained (CFD | Options tabs; the options panel links to it).

---

## Part 3. Result

### 3.1 What changed, screen by screen

- **Top bar** (`shell/title-bar.tsx`, `shell/mode-switch.tsx`, `shell/commands.tsx`). The MT5 menu bar is gone. 48 px bar: brand, an iOS-style CFD | Options switch with a sliding thumb, search (⌘K), a single-line account pill (badges, login, equity), Deposit / Top up demo (the bar's only accent button, 32 px), notifications, Settings ⚙, Help ?, profile. Every old menu command moved to the command palette, the Settings and Help menus, the chart's Layout menu or a labelled button (3.2).
- **Command palette** (`dialogs/misc-dialogs.tsx`). ⌘K now finds markets *and* actions (≈ 100 commands: trading, chart, view, account, help), with the old keyboard actions (↵ / Alt ↵ / ⇧ ↵).
- **Markets** (`market/market-watch.tsx`, `market/segments.tsx`). Single-line 32 px rows with ★, avatar, symbol, change (when wide) and tinted **Sell / Buy price pills**; one click opens a market (no more double-click), a price click also picks the side; wrapping chips with ★ Favourites first (the Favourites tab merged into the chip); list / cards toggle; collapses to an icon rail of favourites; helpful empty states.
- **Symbol header** (`shell/symbol-header.tsx`, `OptionsSymbolHeader` in `options/desktop.tsx`). New: market ▾, big live price and change, bid / ask / spread / day high / low / swaps / session (Options: expiry, time to cut, contract, ATM IV, state), favourite, alert, specification.
- **Chart card** (`chart/workspace.tsx`). Open charts as pill tabs with Layout ▾ (grid, layouts, panels, reset) and full screen; one toolbar with plain-text timeframes, chart type ▾, Indicators, Templates ▾, alert, picture, zoom and fit; a thin drawing rail with tooltips and shortcuts (the two "coming soon" placeholder tools are removed). Trade lines show a grip on draggable chips; the in-chart quick-trade box shows ⚡ when one-click is on; the accent frame marking the active chart only appears with two or more charts on screen (`chart/chart-view.tsx`).
- **Order book** (`order/order-book.tsx`, `order/book-card.tsx`, `options/book-card.tsx`). New column: asks / spread row / bids with gradient depth bars, cumulative toggle, view switch, price grouping, level flashes, B/S ratio bar, Ticks / Trades tab; one `BookView` for CFD depth and the options book. Clicks fill the order form (one-click: send). Replaces the old Depth tab.
- **Order form** (`order/order-ticket.tsx`, `order/right-panel.tsx`). Rebuilt: Sell | Buy (44 px, live prices), order type ▾, label-value rows, quick-adjust strip, SL / TP switches with price / pips / money and risk / reward in money, More options, summary rows, one accent confirm button naming the trade, the one-click switch with its state. The Info tab moved to the symbol header (ⓘ → specification).
- **Positions card** (`toolbox/toolbox.tsx`, `toolbox/trade-tab.tsx`, `shell/account-health.tsx`). Pill tabs with counts (Positions · Orders · History · Alerts · News · Calendar · More ▾; the option tabs first in Options mode); single-line 36 px rows; positions and pending orders are separate tabs; SL / TP editable inline; Close / Cancel always visible; ⋯ = the right-click menu; "Close positions ▾" with a confirm step; the account health strip (with (?) on every figure and a Safe / Low / Margin call / Stop out meter) at the foot, also when collapsed.
- **Status bar** (`shell/status-bar.tsx`). Connection pill, server, price delay (quotes / s in the tooltip), trade-server stream; server time, theme switch, sound, Shortcuts, Support. Profile name, q/s, currency, P&L, margin level, "One-click ON" and the simulated CPU meter are gone from it.
- **Dialogs** (`ui/primitives.tsx`, `dialogs/*`, `order/new-order-dialog.tsx`). Frosted, compact; Settings (renamed from "Options…": trading, appearance, workspace incl. layouts, reset and the tour); grouped Keyboard shortcuts; new Glossary ("Trading terms explained"); new confirm dialog for bulk actions; position / pending / new-order windows restyled to the scale.
- **Onboarding** (`shell/tour.tsx`). Five-step first-run tour (three for guests), restartable from Help and Settings; plain-language (?) help on margin, free margin, margin level, lot, pip, swap, SL / TP, hedging / netting (account pill tooltip), commission, credit.
- **Options desktop** (`options/desktop.tsx`, `ticket.tsx`, `simple.tsx`, `chain.tsx`, `book-ticket.tsx`, `rfq.tsx`, `instruments.tsx`, `positions-tab.tsx`). Same shell and columns as CFD; pill-tab centre and ticket headers; the ticket's SELL / BUY blocks became balanced 44 px outcome buttons (label + price on one line, one short line; the "3,449.6p" points figure removed); Call / Put is a proper segmented control; contracts = field row + quick strip (1 2 5 10); "+ Add leg" and Clear are bordered buttons with labels / tooltips; place-order and Quick-trade confirm are 40 px accent buttons (no green / red 48 px blocks); Up / Down are an outcome pair; the chain's Standard / Pro Buy / Sell prices are price pills, rows 32 px, the strike range a segmented control; secondary hints hide by container width instead of overflowing.
- **Tokens and materials** (`app/globals.css`, `ui/kit.tsx`, `ui/panel.tsx`, `ui/menu.tsx`). AA text colours, buy / sell fills, accent-strong, the backdrop gradient, card / bar / frosted-overlay materials, the compact size scale (§2.4), visible focus on every control.

### 3.2 Feature inventory: where everything lives now

Every item of 1.1 is present. "Clicks" = clicks to reach the control from the default screen (a keyboard shortcut, where one exists, is in addition).

| ID | Now | Clicks |
|---|---|---|
| A1 | Top bar, left | 0 ✓ |
| A2 | Top bar CFD \| Options switch (still with the dot on Options) | 1 ✓ |
| A3 | Badges in the account pill (LIVE/DEMO, CENT, READ-ONLY) | 0 ✓ |
| A4 | Account pill ▾: every login with type, group, mode, leverage, server, live equity; balance in the header | 2 ✓ |
| A5 | Account pill ▾: Log in to another account, Log out of {login} (engine) | 2 ✓ |
| A6 | Top bar Deposit / Top up demo; account pill ▾: Deposit / Refill demo, Open a new account, Manage accounts | 1–2 ✓ |
| A7 | Top bar guest chip, Open account, Log in; guest profile menu | 1 ✓ |
| A8 | Top bar search (⌘K) → command palette | 1 ✓ |
| A9 | The order form is always on screen; Order window (F9) button in its header; palette; Markets row menu; chart menu | 1 ✓ |
| A10 | Order form "One-click trading" switch (with state and help); Settings menu (F10) | 1 ✓ |
| A11 | Chart card Layout ▾ › grid (Alt+1…4) | 2 ✓ |
| A12 | Top bar bell | 1 ✓ |
| A13 | Settings ⚙ › Language ▸; Settings dialog; palette | 2 ✓ |
| A14 | Status bar theme switch; Settings ⚙ › Theme ▸; Settings dialog | 1 ✓ |
| A15 | Chart card ⛶; Settings ⚙ › Full screen (F11) | 1 ✓ |
| A16 | Profile menu › Client Area | 2 ✓ |
| A17 | Profile menu (connected server, Client Area, Profile & security, Log out); shortcuts in Help ? and the status bar | 1–2 ✓ |
| B1 | Chart card + (new chart), then pick the market in the symbol header ▾ / palette | 2 ✓ |
| B2 | × on the chart's pill tab (or middle-click) | 1 ✓ |
| B3 | Layout ▾ › Layouts (presets set the profile) | 2 ✓ |
| B4 | Chart toolbar 📷; chart right-click | 1 ✓ |
| B5 | Account pill ▾ › Log in to another account (engine); guest: top bar Log in | 1–2 ✓ |
| B6 | Account pill ▾ › Open a new account | 2 ✓ |
| B7 | Top bar Top up demo (demo accounts) | 1 ✓ |
| B8 | Profile menu | 2 ✓ |
| B9 | Top bar (guest) | 1 ✓ |
| B10 | Collapse buttons and rails on each panel; Layout ▾ › Panels (incl. Navigator, Order book); Ctrl+M / D / T / B | 1–2 ✓ |
| B11 | Layout ▾ › Layouts; Settings dialog › Workspace | 2 ✓ |
| B12 | See A13 / A14 | ✓ |
| B13 | See A15 | ✓ |
| B14 | Layout ▾ › Reset workspace; Settings dialog | 2 ✓ |
| B15 | Chart toolbar Indicators (Ctrl+I) | 1 ✓ |
| B16 | Indicators dialog (categories); chart right-click ▸ Indicators | 2 ✓ |
| B17 | Drawing rail (horizontal line, trend, Fibonacci, rectangle) | 1 ✓ |
| B18 | Symbol header 🔔, chart toolbar 🔔 (open Alerts); chart right-click "Alert at price" | 1 ✓ |
| B19 | Chart toolbar type ▾ | 2 ✓ |
| B20 | Chart toolbar timeframes | 1 ✓ |
| B21 | Chart toolbar Templates ▾ (built-in, mine, save, delete, apply to all) | 2 ✓ |
| B22 | Layout ▾ › Charts on screen | 2 ✓ |
| B23 | Chart card + | 1 ✓ |
| B24 | Chart toolbar zoom −/+ (and +/− keys, chart menu) | 1 ✓ |
| B25 | Drawing rail 🗑 (with count) | 1 ✓ |
| B26 | See A9 | ✓ |
| B27 | See A10 | ✓ |
| B28 | Status bar 🔊; Settings ⚙ › Sounds on fills | 1 ✓ |
| B29 | Settings ⚙ › Max price change ▸; order form › More options › Max price change | 2 ✓ |
| B30 | Positions card Alerts / History tabs (1); More ▾ › Journal (2) | 1–2 ✓ |
| B31 | Settings ⚙ › All settings… | 2 ✓ |
| B32 | Status bar Shortcuts; Help ? › Keyboard shortcuts (F1) | 1 ✓ |
| B33 | Help ? › Help center / Contact support; status bar Support | 1–2 ✓ |
| B34 | Help ? › About Kalks Trader | 2 ✓ |
| C1 | Markets list / cards toggle; ★ Favourites chip | 1 ✓ |
| C2 | Status bar server time (GMT+3) | 0 ✓ |
| C3 | Markets chips (with counts in tooltips; ★ shows its count) | 1 ✓ |
| C4 | Markets search | 1 ✓ |
| C5 | Markets rows: Sell, Buy, change; spread in the symbol header, order form and hover card | 0 ✓ |
| C6 | One click on a row (double-click still works) | 1 ✓ |
| C7 | Hover card (after 450 ms) | 0 ✓ |
| C8 | Row ⋯ or right-click: New order, chart window, open in chart, order book, specification, favourite, hide, show all | 2 ✓ |
| C9 | Markets footer "N of M markets", Show hidden (n) | 0 ✓ |
| C10 | Markets « collapses to the icon rail (favourites, search, expand) | 1 ✓ |
| D1–D4 | Navigator is an optional panel: Layout ▾ › Panels › Navigator (all four groups unchanged inside). Also: accounts in the account pill, indicators in the Indicators dialog, scripts in Close positions ▾ (incl. "Move all stop losses to breakeven") | 2 ✓ |
| E1 | Chart card pill tabs (activate, ×, middle-click, grid dot, +) | 1 ✓ |
| E2 | Symbol header ▾ → market search | 1 ✓ |
| E3–E6 | Chart toolbar | 1–2 ✓ |
| E7 | Crosshair in the rail (Ctrl+F); zoom, fit, picture in the toolbar | 1 ✓ |
| E8 | Toolbar indicator summary (wide charts) and Indicators count | 0 ✓ |
| E9 | Drawing rail; text / ruler placeholders removed (they only said "coming soon") | 1 ✓ |
| E10 | Layout ▾ › Charts on screen | 2 ✓ |
| E11 | Unchanged in-chart legend and quote line | 0 ✓ |
| E12 | In-chart quick-trade box (⚡ when one-click is on) | 1 ✓ |
| E13 | Trade lines with grips; drag, ×, double-click, projected SL / TP; S / T handles on a position without SL / TP (2026-10-09) | 1 ✓ |
| E14 | Chart right-click menu (unchanged) | 2 ✓ |
| E15 | Delete / Esc (unchanged) | ✓ |
| F1 | Order card header: title, market, Order window (F9), collapse | 0 ✓ |
| F2 | Order (the card), Depth → order-book column (Ctrl+B, rail), Info → symbol header ⓘ / specification | 1 ✓ |
| F3 | Symbol header ▾, Markets; the order window keeps its own market select | 1 ✓ |
| F4 | Order type ▾ (Market, Limit, Stop, Stop limit) | 2 ✓ |
| F5 | Volume row (− value lots +), quick strip 0.01 / 0.1 / 0.5 / 1 / 2, Position size and Pip value rows | 1 ✓ |
| F6 | Price, stop-limit price and expiry (GTC / Today / date) rows | 1 ✓ |
| F7 | More options › OCO | 2 ✓ |
| F8 | Stop loss / Take profit rows with price / pips / money and money at the stop | 1 ✓ |
| F9 | More options › Trailing stop | 2 ✓ |
| F10 | Calculator button next to the quick strip (risk % or $, SL distance, apply) | 1 ✓ |
| F11 | More options › Comment, Max price change | 2 ✓ |
| F12 | Summary rows (margin and % of free margin, pip value, size, free margin after); spread chip; leverage in the account pill ▾ | 0 ✓ |
| F13 | Sell \| Buy with live prices; spread chip | 1 ✓ |
| F14 | "Market closed" note and confirm label | 0 ✓ |
| F15 | One-click switch at the foot of the order form | 1 ✓ |
| F16 | Guest form (explains, Log in / Open account); read-only notice | 0 ✓ |
| F17 | Order-book column: click a level = limit order (one-click: sent); market orders are the form's Sell / Buy (instant with one-click) and the chart box; my orders = ember dot; imbalance = B / S bar | 1 ✓ |
| F18 | Symbol header stats; specification dialog (contract, swaps, sessions) via ⓘ | 1 ✓ |
| G1 | Positions card pill tabs + More ▾ | 1–2 ✓ |
| G2 | Share and My links in the card header (Positions, Orders, History) | 1 ✓ |
| G3 | Close positions ▾ (all, profitable, losing, buys, sells, by symbol ▸, breakeven all, cancel all orders) — with a confirm step | 2 ✓ |
| G4 | Enlarge / restore, hide (Ctrl+T) | 1 ✓ |
| G5 | Positions rows (ticket, opened, side, volume, open → current, SL, TP, swap, commission, P&L, source / comment) with Close | 0 ✓ |
| G6 | Row ⋯ = right-click menu (all items kept) | 2 ✓ |
| G7 | Double-click a row → dialog (kept) | ✓ |
| G8 | Orders tab (type, OCO, price, distance, SL, TP, expiry, Edit, Cancel, ⋯) | 1 ✓ |
| G9 | Account health strip (always visible) | 0 ✓ |
| G10 | History tab (unchanged features, larger filters) | 1 ✓ |
| G11 | More ▾ › Exposure | 2 ✓ |
| G12–G14 | News, Calendar, Alerts tabs | 1 ✓ |
| G15–G17 | More ▾ › Journal, AI Trader, MAM | 2 ✓ |
| G18 | Options mode: Positions, Orders, Closed, Settlements first; CFD mode: option tabs shown while there are option positions / book orders | 1 ✓ |
| G19 | Guest notices (unchanged) | ✓ |
| H1 | Status bar connection pill, server, price delay | 0 ✓ |
| H2 | Status bar trade-server cell (engine builds) | 0 ✓ |
| H3 | Profile = the layout preset (Layout ▾ › Layouts); the name is no longer printed | 2 ✓ |
| H4 | Status bar tooltip (quotes per second) | 0 ✓ |
| H5 | Top bar guest chip | 0 ✓ |
| H6 | Account pill ▾ (currency, 1:leverage); Settings › Connection | 2 ✓ |
| H7 | Health strip (Floating P&L, Margin level + meter) | 0 ✓ |
| H8 | One-click switch state (ON / OFF) and ⚡ on Sell / Buy and the chart box | 0 ✓ |
| H9 | Status bar server time | 0 ✓ |
| H10 | Removed: the demo build's simulated "terminal load" meter (not a real measurement) | — |
| H11 | Status bar Shortcuts / Support; Help ? | 1 ✓ |
| I1 | Order window (F9): tick chart + the new form (comment and max price change open by default) | 1 ✓ |
| I2–I3 | Position and pending dialogs (all actions kept) | 1 ✓ |
| I4 | Command palette | 1 ✓ |
| I5 | Keyboard shortcuts (grouped) | 1 ✓ |
| I6 | Specification (symbol header ⓘ, Markets menu) | 1 ✓ |
| I7 | About (Help ?) | 2 ✓ |
| I8 | Settings dialog | 2 ✓ |
| I9–I11 | Indicator, share and log-in dialogs (unchanged) | ✓ |
| J1 | All shortcuts kept (F9, F10, F1, F11, ⌘K, Ctrl+I, Alt+1…4, Ctrl+M / T / D / F, Esc, Delete, + / −) plus Ctrl+B (order book); shown in tooltips and the sheet | ✓ |
| J2 | Staff / restrictions / copy banners (unchanged) | ✓ |
| J3 | Toasts placed below the symbol header and chart rows, left of the order form | ✓ |
| J4 | Links `?symbol` `?side` `?mode&u` `?sso` `?account` (unchanged) | ✓ |
| J5 | Guest and read-only modes (checked) | ✓ |
| J6 | 22 languages (parity OK), RTL text inside the LTR workspace (checked in Arabic) | ✓ |
| J7 | Resizable panels with saved sizes (new storage keys so old sizes don't squeeze the new columns); < 1440 px starts with Markets as a rail | ✓ |
| K1–K8 | Options: instruments, centre tabs, header stats (now the symbol header), expiries, chain (all presets and hints), Quick trade / Order, Strategy builder, toasts — all kept, restyled | ✓ |

### 3.3 Checks

- `npx tsc --noEmit` in apps/terminal: no errors.
- `pnpm turbo run build --filter=@kalks/terminal`: success.
- `node packages/i18n/scripts/check-parity.mjs`: OK for all 21 languages (9,017 / 9,017 keys; the new `desk` namespace has 345 keys).
- Rendering performance while prices stream (1470×900, order book open, headless Chrome): 56.7–57.7 fps as shipped vs 57.4–57.9 fps with every blur and mask disabled; no element is blurred on the main screen. Blur on cards (23 fps) and on the bars (26–37 fps) was measured and removed.
- Checked live: demo build (accounts, orders, SL / TP inline edits, pending, bulk confirm), guest mode (live build without an account), read-only (investor) session, Options mode with the book on, the public option chain page, the phone layout (unchanged files, still renders), Arabic RTL, dark and light.

### 3.4 Notes for the phone layout

- Use the same tokens (`globals.css`), the `t-glass` / `t-bar` / `t-glass-strong` materials (blur only on overlays) and the kit (`components/ui/kit.tsx`). Ask for larger sizes through props, not breakpoints: `Button size="lg"|"xl"`, `IconButton size="lg"`, `Stepper size="lg"`, `Segmented size="lg"`; phones need ≥ 40 px touch targets.
- Reuse: `BookView` / `CfdOrderBook` / `TickTape` (order book), `OrderTicket` (the order form; already has a `variant` prop), `AccountHealth`, `PositionsTab` / `PendingTab`, `askConfirm` + `ConfirmLayer`, `useCommands` (the palette), `HelpTip` + the `desk.g.*` glossary, `SymbolHeader` stats, `ModeSwitch size="sm"` (unchanged look on phones).
- Shared components already changed and visible on phones: segment chips (now outlined pills), the options ticket / Quick trade (44 px outcome pairs, 40 px accent confirm, quick strips), the chain price pills and 32 px rows on desktop (the phone keeps its 46 px rows), the depth ladder (`order/dom-ladder.tsx`, slightly larger text and 36 px market buttons; the desktop now uses `order-book.tsx` instead).
- Strings: everything new is in `desk.*` (all 22 languages).
- Behaviour to mirror: one-click off by default with the confirm button naming the trade; confirm for bulk closes; SL / TP by price, pips or money.

### 3.5 Open items

- The demo build has no depth stream, so its order book shows made-up sizes fixed per price level, labelled "Indicative · demo"; live builds show the market-data depth (or prices only when none).
- The chart's own canvas, legend and trade-line chips (`chart-view.tsx`, shared with the phone) were only touched lightly (grips, the quick-trade box); a deeper restyle of the chart internals is left for a follow-up.
- AI Trader, MAM, News and Calendar tab contents were not redesigned beyond the shared primitives.
- Some translations keep English trading terms where the existing catalogs do (Bid / Ask, P&L); a native review of the new `desk` strings is advisable before a marketing push.

---

## Part 4. v3: the chart-first layout (2026-10-07)

The founder's directions after testing v2: make it clean like the MetaTrader 5 web terminal, the chart as the hero; instruments and the order book on the right; Buy / Sell on the chart with the order form as a popup; a small CFD | Options switch; positions below the chart with a full-page scroll (Delta Exchange); a Full chart mode; an instruments list ready for 1,000+ markets; the same idea in Options; plain-language help for options; and a security fix for the AI routes. §2.2 and §2.5 describe the result; this part records what changed, the numbers and the checks.

### 4.1 What changed

- **Shell** (`shell/desktop.tsx`). The first screen is the chart card plus the right-hand column (`shell/side-column.tsx`) and the bar at its foot (`shell/status-bar.tsx` → `ScreenBar`); the positions section (`toolbox/toolbox.tsx`, one viewport tall) sits below it and the page scrolls (`ACTIVITY_ID`, `scrollToActivity`, "Positions (n) ↓", "↑ Back to chart", Ctrl+T). One component tree for every state, so the chart never remounts when the column opens, closes or Full chart toggles. The v2 Markets column, symbol header card, order-book column, permanent order form and status bar are gone (files removed: `market/market-watch.tsx`, `shell/symbol-header.tsx`, `order/book-card.tsx`; `RightPanel` removed from `order/right-panel.tsx`, which keeps the specification and tick sparkline).
- **Top bar** (`shell/title-bar.tsx`, `shell/mode-switch.tsx`, `shell/commands.tsx`). ☰ menu (`useMainMenuItems`) replaces the Settings and Help buttons; the CFD | Options switch is 28 px, text only, with a frosted thumb; the bar is sticky and frosted over the scrolling page.
- **Chart** (`chart/workspace.tsx`, `chart/chart-view.tsx`, `chart/brand-watermark.ts`). One toolbar row (tabs or the compact chart chip, type, M1…MN, New order, zoom, indicators, templates, layout, alert, picture, Full chart, full screen). The centred "SYMBOL, TF / name · Kalks" watermark became the small K in the bottom-left corner (also in the options premium chart and on phones); the bottom-left Bid / Ask / Spread strip was removed (the Buy / Sell box carries the prices and spread).
- **Instruments** (`market/instruments.tsx`, `lib/stress-instruments.ts`). Virtualised list (32 px rows, 8 rows overscan; cards view 124 px), fast ranked search with ↑ ↓ Enter, a one-row asset-class filter built from the catalogue, ☆ and ⋯ on hover, hover range card, row menu, hidden markets. `?stress=1500` (or `localStorage["kalks.stress"]`) adds generated test markets with their own simulated prices.
- **Order popup** (`order/new-order-dialog.tsx`). 440 px form, optional tick chart (840 px), More options folded. Everything that used to focus the side panel opens it: chart Buy / Sell (one-click off), New order, F9 / Ctrl+D, Instruments Bid / Ask / double-click, order-book levels, empty states. The prefilled side no longer resets under React's dev double-run (`order/order-ticket.tsx`).
- **Full chart.** `ui.fullChart` in the store, Shift+F, toolbar buttons in both modes; Esc closes the slid-in column, then Full chart.
- **Options** (`options/desktop.tsx` → `OptionsMain`, `OptionsTicketPopup`; `options/side.tsx`; `options/book-card.tsx` → `OptionsBookBody`). First built as a big chart with one arrow opening an options panel under it; after the founder's feedback the card has one tab row of full-size views instead (Part 5.2, §2.2). Quick trade and Order as a popup that opens by itself when a price, a book level or "Send to ticket" picks an option; a wide desktop chain starts on Standard columns once (`kalks.options.deskStd`). The "Options in 30 seconds" card is a one-line banner until opened (`options/explain.tsx`).
- **Options help.** An Options tour (5 steps, first time Options mode opens), "(?)" on Expiry, Time to the cut and ATM IV, an Options tab in Trading terms explained with 18 terms (`desk.og.*`), "Words explained" in the options panel.
- **AI routes** (`lib/ai-guard.ts`, `lib/ai-client.ts`, `app/api/ai-trader/route.ts`, `app/api/options/explain/route.ts`). A paid model call needs a same-origin request and a terminal session the trading engine confirms (the `kalks_trade` session cookie of the engine BFF; the engine check is cached for a minute), and stays within 10 calls a minute and 200 a day per login. Refusals: 403 `forbidden`, 401 `signin` ("Sign in to use AI"), 429 `rate_minute` / `rate_day` (with Retry-After), 503 `unavailable` when the engine cannot confirm the session (fail closed). Demo builds have no sign-in: there the routes answer only on localhost, so the public demo showcase cannot spend credits. The terminal shows the refusals in the reader's language; guests see "Sign in to use AI" (the AI Trader falls back to its local parser). The GET "configured?" probes are unchanged. The budget is per server process (in memory).

### 4.2 Inventory changes since 3.2

Everything in 3.2 is still present; these items moved again:

| Item | v3 location |
|---|---|
| A1 Brand, A13–A17 settings / help / about | ☰ menu (theme toggle, language ▸, shortcuts, glossary, tour, help center, support, about); profile menu unchanged |
| A9, F1–F17 order form | Order popup (New order, F9 / Ctrl+D, chart Buy / Sell, Instruments prices, book levels) |
| A10, F15 one-click | Order popup switch, ☰ menu, F10 |
| B10 panels, D1–D4 Navigator | Layout ▾ / ☰ › Chart settings › Panels: Instruments (Ctrl+M), Order book (Ctrl+B), Navigator (a column tab), Options panel, Full chart (Shift+F), Positions (Ctrl+T, scrolls) |
| C1–C10 Markets | Instruments tab of the right column (rail replaced by the edge tab; favourites via ☆ filter) |
| E2, F18 symbol header | Chart legend (symbol, timeframe, name, OHLC) and the compact chart chip; stats via the row hover card and Specification (row menu, chart menu) |
| E11 quote strip | Removed from the plot; the Buy / Sell box shows sell, buy and spread |
| F2 Depth | Order book and Ticks tabs of the right column |
| G4 hide / enlarge positions | The section is below the chart, full width and one viewport tall; ↑ Back to chart |
| H1–H11 status bar | The bar at the foot of the first screen (health, connection with delay and quotes / s, trade server, server time); theme, sound, shortcuts, support in ☰ |
| K1–K8 options | Underlyings in the right column; chain / analytics / book in the options panel; Quick trade / Order popup; header stats in the options bar |

### 4.3 Measurements

Chart area at the same window sizes (demo build, account 80412337, default workspace, measured from the DOM):

| | before (v1, the old shell) | after (v3) | change |
|---|---|---|---|
| CFD chart, 1470×900 | 856 × 504 = 431,424 px² | 1,052 × 738 = 776,376 px² | +80 % |
| CFD chart, 1600×950 | 952 × 540 = 514,080 px² | 1,182 × 788 = 931,416 px² | +81 % |
| Options, 1470×900 | chain 899 × 419 = 376,681 px² (Simple columns, 760 px table) | chart 1,100 × 706 = 776,600 px²; with the panel open the chain is 1,102 px wide, Standard columns | |
| Options, 1600×950 | chain 996 × 454 = 452,429 px² | chart 1,230 × 756 = 929,880 px²; chain 1,232 px wide | |

Instruments list with `?stress=1500` (1,528 markets): 28 rows rendered, continuous scroll top → bottom in 4 s at 58.6 fps (headless), search and filters immediate.

Rendering: headless Chrome on this machine is now capped at 30 fps (about:blank measures 30.3), and the terminal holds that cap with and without every blur (30.0 / 30.1); the main thread is busy about 17 % of the time with prices streaming. A real-GPU measurement through the Chrome extension was not possible (the extension was not connected), so the frosted materials stay where §2.3 puts them: overlays and the sticky top bar blur; cards with live prices do not.

### 4.4 Checks

- `npx tsc --noEmit` (apps/terminal): passes.
- `pnpm turbo run build --filter=@kalks/terminal`: passes.
- `node packages/i18n/scripts/check-parity.mjs`: OK for all 21 languages in every namespace (desk 403 keys, 32 unused desk keys removed, 90 added and translated).
- Fresh browser profile on the dev server: log in, the CFD tour runs to the end, switch to Options (its tour runs), Quick trade popup, back to CFD, Buy on the chart opens the order popup with Buy chosen; no console errors. (Fixed on the way: an effect in the tour returned `window.scrollTo(...)`'s Promise, which crashed React; every one-line effect that could return a value now has a block body.)
- AI gate on a production build (live mode) with a dummy key: no session → 401 `signin`; another origin → 403; a forged session cookie → 503 (the engine can't confirm it; no model call); GET still reports `configured`.

### 4.5 Phone layout and open items

- The phone layout (`components/mobile`, `options/mobile.tsx`) was not redesigned in v3; it picks up the shared changes (the corner K on charts, the options intro banner, the glossary's Options tab, the AI sign-in message).
- The price feed's network subscription (packages/mock, `op: "subscribe"` with the whole catalogue) is unchanged: the Instruments panel only listens to the rows on screen, but the socket still asks for every symbol. With 1,000+ instruments the feed should subscribe to what is visible plus favourites and open positions, and unsubscribe the rest (the protocol already has `subscribe`; an `unsubscribe` / replace op would be needed on the service).
- The AI budget lives in the Node process; several instances behind a load balancer would each keep their own.

---

## Part 5. The 1,409-instrument catalogue and the founder's v3 feedback (2026-10-07)

### 5.1 Instruments and prices (packages/mock)

- **Catalogue.** `packages/mock/scripts/gen-catalogue.mjs` turns `config/instruments.json` (1,381 `"tier": "catalogue"` rows) into `src/catalogue.generated.ts`, priced from the provider snapshot's last daily close (`config/provider/infoway-snapshot.json`); `--check` fails when the file is out of date. Each row: symbol (HK `00700.HK`, Tokyo `7203.JP`, US one-letter `A.US`, crypto `XXXUSD`), name, asset class, digits, base spread, reference price, contract size, session, quote / base currency, exchange, avatar. `symbols.ts` exports `CATALOGUE`, `ALL_INSTRUMENTS` (core 28 + catalogue) and `liveTradable(symbol)`; `INSTRUMENT_MAP` covers all 1,409; `INSTRUMENTS` stays the 28 core (crm and admin lists unchanged). Market hours add Hong Kong and Tokyo sessions.
- **Streaming within the plan** (`prices.ts`, protocol of services/market-data `api.rs` / `demand.rs`). The socket subscribes to every instrument with `"passive": true` (prices if they stream, delayed snapshots otherwise) and actively only to the demand: symbols with a live listener (the Instruments rows on screen, positions tables, the order popup…) plus `priceFeed().want(owner, symbols)` (the terminal declares favourites, charts on screen, positions and orders, the order popup's market); charts and depth ask through `bars` / `depth` (focus). Changes go out as diffs (`subscribe` for new, `subscribe` + `passive` for dropped) after 700 ms of quiet, at most every 3 s, so a scrolled list doesn't churn the plan.
- **Delayed prices.** `"d":1` frames and `"d":true` REST quotes set `Quote.delayed`. Live builds show catalogue markets the service hasn't priced yet as delayed (the snapshot close); demo builds simulate them from that close. Delayed rows show a clock; their Bid / Ask, the chart's Sell / Buy and the order popup's confirm are off with "Price is delayed; open the chart to stream live prices".
- **Demo only.** On live accounts, catalogue markets (`liveTrading: false`) show a "Demo only" chip in Instruments and on the chart box; opening a trade is blocked with "Live trading for this market isn't enabled yet" (client-side, and the engine's `symbol_demo_only` maps to the same toast). Closing and modifying stay allowed. Demo accounts trade everything.
- **Phones.** The watchlist browses the core markets and searches the whole catalogue (first 60 matches).

### 5.2 Founder feedback on v3

- **Toasts under the bell** (§2.2): `shell/notifications.tsx` anchors them, `app/providers.tsx` and `lib/notify.ts` set the stack, durations and styles. A toast that sat over the chart could stay for minutes (hovering the chart under it paused it); in the corner it can't.
- **Options: one tab row** (§2.2): Option chain · Underlying · Option · Both · Analytics · Book replace the chart + panel split, the bottom-edge arrow and the "Select a strike…" banner (which stopped responding after its first click). `lib/options/crosshair.ts` shares the crosshair time between the two charts of Both.
- **Positions: Full page or Split** (§2.2): `ws.posLayout`, `setPositionsLayout()`.

### 5.3 Checks

Recorded in the final report of this round (tsc, terminal / crm / admin builds, parity).
