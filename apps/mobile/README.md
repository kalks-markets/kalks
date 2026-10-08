# Kalks app (Flutter, Android first)

One app, **Kalks** (`com.kalkstrade.app`, Android 7.0+ / API 24): the Client Area and Kalks Trader with the **same
pages, order, buttons, texts and API calls as the phone web** (`apps/crm`, `apps/terminal`), in a native iOS-style
look. It talks to one server, the Client Area's mobile API (`https://app.kalkstrade.com/api/mobile/*`, contract in
[`docs/MOBILE-API.md`](../../docs/MOBILE-API.md)), plus the WebSocket streams whose URLs come from `GET /config`.

This folder has no `package.json`: pnpm and turbo ignore it. Flutter's own `.gitignore` is inside.

**Status (Step 3, foundation):** design system, core (API, session, biometric lock, config, streams, notifications,
step-up), i18n (all 22 languages, the web's keys), assets, routes for every Client Area page, the shell (header, bottom
bar, More, bell, profile menu, search), sign-in / sign-up / reset / unlock, and the **Dashboard** as the reference
screen on real data. Every other page is a **route stub** (same path and label as the web) waiting for its screen;
Kalks Trader is a full-screen **placeholder** with its final frame.

## Run

Toolchain: `export PATH=$HOME/dev/flutter/bin:$PATH` (Flutter 3.47, JDK 17 in `~/dev/jdk17`).

| What | Command (in `apps/mobile`) |
|---|---|
| Phone over USB (production API) | `flutter run` (or `flutter run --release`) |
| Phone against a local stack | `adb reverse tcp:3000 tcp:3000` (+ 8081, 8090, 8104, 8100), then `flutter run --dart-define=KALKS_API_BASE=http://127.0.0.1:3000/api/mobile` |
| Design preview in Chrome, sample data, no network | `flutter run -d chrome --dart-define=KALKS_PREVIEW=true` |
| The same as a static build | `flutter build web --dart-define=KALKS_PREVIEW=true --no-web-resources-cdn`, serve `build/web` |
| Release APK (debug-signed for USB testing) | `flutter build apk --release` |
| Checks | `flutter analyze` (must stay clean) · `flutter test` |

Preview URL switches (preview builds only): `?signedIn=1` (open the Client Area as the sample client), `?locked=1`
(the biometric lock screen), `?theme=dark|light`, `?lang=ar` (any of the 22). The design system on one page:
**More › Design system** or `#/more/gallery`, with `?open=banner|sheet|actions|alert|stepup|bell|profile|palette|language`
to open an overlay. Example: `http://localhost:PORT/?signedIn=1&lang=ar#/`. In preview, sign-in accepts any password
and code (`wrong@example.com` shows the error; code `000000` is wrong).

The web target exists **only** for these previews; Android is the product. Plugins without web support (biometrics,
secure storage) fall back quietly there.

The same sample data is the app's **demo**: "Try the demo" on the sign-in / sign-up pages (Kalks only, never white-label
brokers) switches `demoModeProvider` on (kept in prefs as `kalks.demo`, so a restart stays in the demo), seeds the
sample client's session and opens the Dashboard in place, with a "Demo · Sample data · Exit demo" strip over the header.
Log out (strip, profile menu, More, the terminal's Account tab) ends it and returns to the live transport.

## Regenerating generated files

| Files | Command (repo root) | Source |
|---|---|---|
| `assets/i18n/<locale>.json` (22) | `node apps/mobile/tool/export_i18n.mjs` | `packages/i18n/src/catalog` + `tool/i18n_app.json` |
| `assets/illustrations/` (1x/2x/3x PNG) + `lib/ui/illustrations.g.dart` | `node scripts/process-illustrations.mjs --only flutter` (`pnpm illustrations` does web + app) | `illustrator/*.png` |
| brand images, notification icon, coins, stocks, people, flags, Geist fonts | `node apps/mobile/tool/sync_assets.mjs` | `assets/`, `brand/`, node_modules `geist`, `flag-icons` |
| launcher + adaptive icon, splash | `cd apps/mobile && dart run flutter_launcher_icons && dart run flutter_native_splash:create` | `assets/brand/*.png` (config in `pubspec.yaml`) |

Re-run the i18n export whenever the web's catalogs change (new keys appear in the app at once). Plus Jakarta Sans
(static weights instanced from google/fonts `ofl/plusjakartasans`) is committed in `assets/fonts` with its OFL licence.

## Structure

```
lib/
  main.dart, app.dart        start-up (prefs, i18n, app info) and the MaterialApp (themes, locale, RTL, banners)
  env.dart                   --dart-define switches: API base, preview
  router/router.dart         every route (web paths), redirects (sign-in, lock, maintenance, update, view-only)
  shell/                     the Client Area chrome: app_shell (header + sub-page tabs + bottom bar), module_pager
                             (a module's sub-pages side by side: swipe or tap a tab, the URL follows, visited pages
                             keep their state), chrome (what the shell and the pager share), menus (bell, profile
                             menu, search), more_screen, nav (the web's nav: modules, pages, keys, icons, module
                             switches, view-only rules), session_keeper (heartbeat 45 s, me 30 s)
  features/                  screens, one folder per web module
    auth/                    login (the signed-out welcome page: picture, headline, Log in / Open account pills),
                             sign_in_sheet (the sign-in form as a sheet), register, forgot, unlock (+ auth_widgets)
    dashboard/               the reference screen and its section widgets
    terminal/                Kalks Trader (see "Kalks Trader" below): core/ (sessions, the account stream, orders,
                             contract maths, market feed), cfd/ (the five tabs and the sheets), chart/ (the chart page
                             bridge), options/ (Kalks FX Options mode), preview/ (the preview trade server)
    common/                  stub_screen (route stubs), system screens (maintenance, update), pickers (language)
  data/client_data.dart      shared Client Area data providers (accounts, wallet, rewards, equity curve)
  core/
    api/                     ApiClient (Dio, headers, error mapping), ApiException + localizeError, providers
    auth/                    secure store (Keystore), session + device id + trade tokens, AuthApi, AuthController
                             (booting / signedOut / locked / signedIn), biometrics
    config/app_config.dart   GET /config (cached), branding, module switches, maintenance, min version
    realtime/                Backoff, ReconnectingSocket, MarketStream (passive/active demand), SupportStream,
                             EngineStream + OptionsStream (for the terminal)
    notifications/           the inbox (bell + dashboard + banners), local toasts, engine pushes
    models/                  user (me), account (EngineAccount, totals), wallet
    format/format.dart       money / numbers / dates (Latin digits, server time GMT+3)
    prefs.dart, lifecycle.dart, theme_controller.dart, app_info.dart
  i18n/                      t.dart (createT port), i18n.dart (catalog loading, providers, context.t), locales
  ui/                        "Kalks iOS" design system (import lib/ui/ui.dart)
  preview/                   sample data, the preview HTTP adapter, the design-system gallery (previews only)
tool/                        export_i18n.mjs, sync_assets.mjs, i18n_app.json (app-only texts)
test/                        unit tests, widget tests, goldens (test/goldens, generated on macOS)
```

## Design system rules ("Kalks iOS", `lib/ui`)

- **Tokens, never raw colours.** `context.k` (KTokens) carries the web's palette: Client Area pastel light by default
  (`apps/crm/app/globals.css`), dark as an option; Kalks Trader dark by default (`apps/terminal/app/globals.css`), via
  `KTheme.client()` / `KTheme.trader()`. Every tint is mixed from the tenant brand colour with the web's
  `color-mix(in oklab)` maths (`ui/color_mix.dart`), so white-label brokers re-tint automatically (`config.tenant`).
- **Type** from `context.text` (Plus Jakarta Sans in the Client Area, Geist in the terminal, Geist Mono for prices,
  logins, codes). Figures are tabular and stay left-to-right inside RTL text (`KMoney`, `textDirection: ltr`).
- **Compact sizes.** Buttons sm 32 / md 40 / lg 44 (the full-width primary is lg; nothing taller); every control keeps
  a ≥ 44 pt touch target (`KPressable`); card radius 24, rows 16 (terminal 14 / 10). **One saturated primary action
  per screen**; Deposit / Withdraw are `ink`.
- **iOS materials.** Bars, sheets, banners: `KFrosted` (blur + saturate under a ~90 % fill: blurred, never
  see-through). Sheets: `showKSheet` (grabber, drag to close); choices: `showKActionSheet`; confirmations:
  `showKAlert`; lists: `KListSection` + `KListRow` (swipe actions); `KSegmented` (sliding thumb), `KSwitch`,
  `KStepper`, `KOtpField`, `KTextField`, `KChip`, `KIconTile`, `KCard` (a `KCardTheme` above a page sets its cards'
  radius / padding / border / shadow), `KKpiCard`, `KEmptyState` (the founder's illustrations, `KIllustrationName`),
  `KSkeleton`, pull to refresh via `KPageScroll(onRefresh:)`, a full-bleed picture with the page in a sheet over it
  via `KPageScroll(hero: KPageHero(...))` (the Dashboard; the shell floats its controls over it), `KPillNav` (pages as
  pill chips), haptics via `KHaptics` (taps are automatic).
- **Feedback.** Never a Material snackbar or dialog: `notificationsProvider.notifier.toast(kind, title)` (a short
  top banner, kept in the bell like the web's toasts); engine events: `.push(...)`.
- **Icons:** Lucide (`lucide_icons_flutter`, the same set and names as the web's lucide-react). Use the web's icon for
  the same thing. Mirror direction icons in RTL (`arrowLeft` ↔ `arrowRight`, chevrons).
- **Texts:** only `t('<ns>.<key>')` with the **web's own key** for the same label (grep the web component), so every
  word matches the web in all 22 languages. Placeholders `{name}`, plurals with `{'count': n}`, markup with
  `KRichText(text, tags: {'link': KTag.link(onTap)})`. App-only texts (no web equivalent) go in `tool/i18n_app.json`
  (namespace `app`, all 22 languages) and need the export re-run.
- **RTL** (ar, ur, fa) is automatic through `Directionality`; use `EdgeInsetsDirectional`, `AlignmentDirectional`,
  `PositionedDirectional`.

## How later agents add a screen

The web is the source of truth: **same sections, same order, same buttons, same texts, same API calls**.

1. Open the web page (`apps/crm/app/(app)/<path>/page.tsx` and the components it renders; for the terminal
   `apps/terminal/components/mobile/mobile-terminal.tsx` and friends). List its sections in the phone order (the web's
   `max-md:order-*` / phone layout), every button and what it calls.
2. Create `lib/features/<module>/<page>_screen.dart`. Body: `KPageScroll(onRefresh: …, children: [...])` with a
   `KPageHeader` (the web's PageHeader title / subtitle keys) and the sections in that order, built from `lib/ui`.
3. Data: one provider per web hook (`FutureProvider.autoDispose`, `ref.pollEvery(...)` with the web's interval)
   calling the same path under `/api/mobile/<family>/…` (`ref.watch(apiProvider).get/post`). Parse into small models
   in `lib/core/models` or next to the feature. Show `KSkeleton` while loading and the web's empty / error states.
4. Replace the stub in `lib/router/router.dart`: pass the screen in `_moduleRoutes(<module>, screens: {...})`; detail
   pages (`/accounts/:login`) are child routes (iOS push). Keep the web path. A module's sub-pages share one page,
   the module pager (`lib/shell/module_pager.dart`): the reader swipes between them or taps the tabs, and a page
   visited stays alive (its providers keep polling), so always `go` to a sub-page, never `push` it.
5. Actions: errors through `localizeError(e, t)`; step-up protected writes through `withStepUp(...)` /
   `showStepUpSheet(...)` (actions and targets as in `lib/ui/components/stepup_sheet.dart`); hide account actions when
   `me.readOnly` (view-only / read-only staff), and modules the broker switched off (`config.moduleOn`). Never retry a
   money write automatically.
6. Kalks Trader: trade tokens per login (`SessionStore.setTradeToken`, `X-Kalks-Trade` via `tradeToken:` on the API
   calls), `trade/sessions` to open an own account, `EngineStream` / `OptionsStream` / `MarketStream` from
   `lib/core/realtime` (only visible rows subscribe actively: `MarketStream.subscribe` / `want`). Theme: the trader
   route wraps itself in `KTheme.trader`.
7. Tests: unit-test parsing and rules; add the screen to a widget test on the preview adapter (add its sample answers
   to `lib/preview/preview_data.dart` + `preview_adapter.dart`), golden in light / dark / Arabic if it is a key screen
   (`flutter test --update-goldens` on macOS). `flutter analyze` must stay clean (`dart format .`, width 160).

## Notes for the next agents

- **Agent C1** (Dashboard rest, Accounts, Wallet, Portfolio, Profile & Security, Support): the Dashboard still lacks
  Ask Kalks AI, Statistics, the activity tabs, Getting started, the Markets cards and More for you (marked in
  `dashboard_screen.dart`); the account ⋯ menu, Fund dialog and demo refill go through the Accounts work.
- **Agent C2** (Markets / News / Calendar, Copy & PAMM, Partner, Prop, Rewards, Academy, Developer, Options intro).
- **Agent D** (Kalks Trader CFD + Options): built in `features/terminal/` (see "Kalks Trader").
- Push notifications, Google sign-in and refresh tokens are not part of this app version (docs/MOBILE-API.md).
- Not verified on a device yet: the Android SDK wasn't installed when the foundation was built, so no APK was built.
  The first `flutter build apk` will tell whether Gradle needs anything (AGP 9.1, Kotlin 2.4, appcompat for
  local_auth's BiometricPrompt theme).

## Kalks Trader (`lib/features/terminal`)

The web terminal's PHONE layout (`apps/terminal/components/mobile/mobile-terminal.tsx`, `components/options/mobile.tsx`)
on the trade API (`trade/*`, docs/MOBILE-API.md §6) and the streams in `lib/core/realtime`:

- `/trader?login=` opens `TerminalScreen`: header back · CFD | Options · account pill (switcher) · bell; CFD bottom bar
  Watchlist · Chart · Trade · History · Account; Options has its own (Markets · Chart · Chain · Trade · Positions).
  Like the web's mobile terminal the body stays left-to-right in Arabic, Urdu and Persian (sheets follow the language).
- Sessions: `core/sessions.dart` (`trade/sessions` for own accounts, `trade/login` MT5-style, `sessions/check`, trade
  tokens in the Keystore); the account on screen: `core/terminal_controller.dart` (`trade/state` + the engine stream,
  engine notifications to the bell); actions: `core/trade_actions.dart` (banners, haptics, the web's reject texts).
- The chart: `assets/chart/chart.html` with **lightweight-charts 5.2.1 bundled offline** (from node_modules, Apache 2.0,
  licence next to it) in a WebView on Android; `chart/chart_bridge.dart` is the JSON codec; the web preview and widget
  tests draw a native stand-in (`chart/chart_native.dart`, `TerminalChart.forceNative`). Indicators are the web's own
  code: `node apps/mobile/tool/build_chart_indicators.mjs` (from the repo root, after the web files change) bundles
  `apps/terminal/lib/indicators.ts` + `components/chart/indicators/{layer,band-fill}.ts` into
  `assets/chart/indicators.bundle.js` and writes the registry `assets/chart/indicators.json` for the menus
  (`cfd/chart_menu.dart`: chart type, indicators list, settings, templates; saved per symbol in the workspace).
  Trade lines like the web: a position without SL / TP shows **S** / **T** handles after its P&L (drag one out to
  place that stop, tap for the order tickets' starting distance; `ChartLine.handles`, `lineForHandle`, `clampStop`,
  `defaultStop` in `chart_bridge.dart`, the same rules as the web's `components/chart/trade-handles.ts`). Kalks
  Trader is blue for up / buy / profit / TP and red for down / sell / loss / SL (`KTokens.traderDark/Light`).
- Previews (`KALKS_PREVIEW=true`): `preview/preview_server.dart` answers `trade/*` and plays the market-data and engine
  sockets (moving quotes, fills, pending triggers, SL / TP), so `?signedIn=1#/trader` works offline.
- Tests: `test/terminal` (maths incl. cent / JPY, order rules, engine shapes, the stream, the chart codec, the S / T
  handles, the order and position sheets).
