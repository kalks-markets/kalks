# Client Area sections

A broker decides which Client Area pages and dashboard blocks its clients see: for everyone, or by the client's
country, the account group of any of their trading accounts, or the client. Switching a section off only hides
it (menu entry, page, card, palette entry); nothing is deleted and switching it back shows everything again.

- Modules (copy trading, wallet, prop, …) are still switched per broker by the Platform Owner only
  (`services/gateway/src/tenancy.rs`). A section inside a switched-off module stays off whatever its own switch or
  rules say.
- Sections are switched by the broker's staff with `settings.write` in the Back Office: Settings › Client Area
  layout (`/settings/layout`). Every change asks for a reason and is audited.

## Where things are

| What | Where |
|---|---|
| The shared map (paths → modules and sections, the section catalogue, Kalks Trader tabs) | `packages/ui/src/lib/sections.ts` (`@kalks/ui/sections`) |
| Catalogue, rules, evaluation, endpoints | `services/gateway/src/sections.rs`, migration `20261019100000_client_sections.sql` |
| Client Area: the client's own switches (cached) | `apps/crm/lib/features.ts`, `GET /api/me/features` |
| Client Area: pages and single-page APIs | `apps/crm/proxy.ts` (`brokerGate`) |
| Client Area: nav, palette, account menu, dashboard blocks | `apps/crm/components/tenant-config.tsx`, `chrome/client-shell.tsx`, `dashboard/live-dashboard.tsx` |
| Mobile app | `GET /api/mobile/me/features`, `config.sections` (`docs/MOBILE-API.md`) |
| Kalks Trader | `apps/terminal/lib/modules.ts` (`TAB_MODULES` = `TRADER_TABS`) |
| Back Office | `apps/admin/components/rbac/client-layout.tsx` |

`BUILTIN_SECTIONS` (gateway) and `SECTIONS` (shared map) list the same keys, names and modules in the same order; the
gateway test `catalogue_matches_the_apps` reads the TypeScript file and fails when they drift apart.

## Keys

Pages are `page.<path with dots>` (the page and everything under it, unless a longer section path claims it).
Dashboard blocks are `dashboard.<block>`. The dashboard itself (`/`) is the landing page and has no switch: its
blocks do. Module in brackets.

**Dashboard blocks** (`apps/crm/components/dashboard/live-dashboard.tsx`, top to bottom)

| Key | Block |
|---|---|
| `dashboard.ask_ai` | Ask Kalks AI bar in the hero (ai) |
| `dashboard.balance_total` | Total balance |
| `dashboard.balance_equity` | Equity |
| `dashboard.wallet_card` | Wallet balance (wallet) |
| `dashboard.today_pnl` | Today's P&L |
| `dashboard.open_positions` | Open positions |
| `dashboard.rewards_card` | Reward points (rewards) |
| `dashboard.banners` | Promotion banners |
| `dashboard.shortcuts` | Shortcuts (each shortcut also follows the page it opens) |
| `dashboard.accounts` | Your accounts |

**Pages**

| Nav group | Keys |
|---|---|
| Dashboard | `page.markets` (markets), `page.news` (news), `page.calendar` (calendar) |
| Accounts | `page.accounts`, `page.accounts.new` |
| Wallet (wallet) | `page.wallet`, `page.wallet.deposit`, `page.wallet.withdraw`, `page.wallet.transfer`, `page.wallet.history` |
| Portfolio | `page.portfolio`, `page.portfolio.analytics`, `page.portfolio.history`, `page.portfolio.ledger`, `page.portfolio.statements` |
| Options | `page.options` (options) |
| Partner (ib) | `page.partner`, `page.partner.clients`, `page.partner.network`, `page.partner.commissions`, `page.partner.links`, `page.partner.payouts` |
| Copy & PAMM | `page.social`, `page.social.copy`, `page.social.master` (copy_trading); `page.social.pamm`, `page.social.investments` (pamm); `page.social.managed`, `page.social.mam` (mam) |
| Prop (prop) | `page.prop`, `page.prop.mine`, `page.prop.payouts`, `page.prop.certificates` |
| Rewards (rewards) | `page.rewards`, `page.rewards.loyalty`, `page.rewards.cashback`, `page.rewards.promotions` |
| API & Algo | `page.developer`, `page.developer.webhooks`, `page.developer.docs` (api); `page.developer.strategies`, `page.developer.deployments`, `page.developer.backtests`, `page.developer.marketplace` (algo) |
| Academy (academy) | `page.academy`, `page.academy.glossary`, `page.academy.progress`, `page.academy.coach` |
| Profile & Security | `page.profile`, `page.profile.security`, `page.profile.verification`, `page.profile.viewers`, `page.profile.notifications`, `page.profile.preferences` |
| Support | `page.support` |

To add a page or block: add it to `SECTIONS` in the shared map and to `BUILTIN_SECTIONS` in the gateway (same order),
then use `useSections()("dashboard.<block>")` for a block. Pages need nothing else: the proxy, nav and palette find
them by path. The gateway seeds new keys on start and drops built-in keys that are gone (with their rules).

## Precedence

For one client and one section, the first rule that matches wins:

1. its module is off → hidden
2. a rule for the client (user id)
3. a rule for an account group of any of the client's trading accounts (two group rules that disagree: shown wins,
   one of their accounts is in a group that has it)
4. a rule for the client's country (their registered country, `users.country`)
5. the rule for everyone (the on / off switch of the Back Office page)
6. the catalogue default (shown)

Signed-out pages, and the client's own switches when the gateway can't be reached, use 1, 5 and 6 only.

### Account groups

Account groups live in the trading engine; the gateway never calls it. The Client Area BFF (`lib/features.ts`) asks
`GET /v1/me/features` first; the answer says whether the broker has any group rule (`group_rules`). Only then does
the BFF read the client's accounts from the engine (`GET /v1/accounts`, internal token, `X-Kalks-User-Id` from the
session) and ask again with `?groups=pro,std`. The groups always come from the BFF (internal token), never from the
browser. The Back Office preview does the same with the chosen client's accounts (`/api/trading/admin/accounts`).

## API

Gateway (internal token; only the apps' BFFs call it):

| Method | Path | Who | What |
|---|---|---|---|
| GET | `/v1/me/features?groups=a,b` | client session (also view-only and staff sessions) | `{ user_id, country, groups, group_rules, modules, flags, sections, version }` |
| GET | `/v1/public/tenant-config` | anyone | now also `sections` (for everyone) and `features_version` |
| GET | `/v1/admin/settings/sections` | `settings.read` | every section with its default, module, switch for everyone and rules |
| PUT | `/v1/admin/settings/sections/{key}` | `settings.write` | `{ enabled: true \| false \| null, reason }`: the switch for everyone (null = default) |
| POST | `/v1/admin/settings/sections/{key}/rules` | `settings.write` | `{ scope: country \| group \| user, value, enabled, reason }`: add or change a rule (`enabled: null` removes it) |
| DELETE | `/v1/admin/settings/sections/rules/{id}` | `settings.write` | `{ reason }` |
| GET | `/v1/admin/settings/sections/preview?country=&groups=&user_id=` | `settings.read` | `{ who, modules, sections, sources }`: `sources` says which rule decided each section |

A reason (3+ characters) is required for every change. Audit actions: `settings.section_toggled` (switch for
everyone), `settings.section_rule_set`, `settings.section_rule_removed`, with `key`, `scope`, `value`, `reason`,
`before`, `after`. The Platform Owner's `PUT /v1/owner/tenants/{id}/features` accepts section keys too (the switch for
everyone).

Client Area:

- `GET /api/me/features` (and `/api/mobile/me/features` for the app): `{ modules, flags, sections, personal }`.
- A page whose section is off redirects to the dashboard. BFF paths that serve one page only (`API_SECTIONS` in the
  shared map: view-only access, API keys, webhooks, contests, promotions, loyalty vouchers / redemptions) answer
  `403 section_disabled`; shared APIs (wallet, accounts, cashback, …) stay open, so hiding a page never breaks another
  one. The mobile app's rewrites are judged on the same cookie-route path.

Kalks Trader has no client session with the gateway: its News, Calendar and MAM tabs follow `page.news`,
`page.calendar` and `page.social.mam` for everyone (`TRADER_TABS`), not country / group / client rules.

## Storage and caching

- `feature_flags.kind = 'section'` (keys may contain dots), `feature_rules (tenant_id, key, scope, value, enabled,
  updated_by, updated_at)`, unique per `(tenant_id, key, scope, value)`, with row-level security
  (`tenant_isolation`, like the other tenant tables); the gateway reads and writes rules inside `tenant_tx`.
- Every switch of a module, flag, section or rule bumps `tenants.features_version`.
- The Client Area caches a client's switches for 30 s per session; the cache key carries `features_version` from the
  tenant config (itself cached 5 s), so a change shows within about 5 s. Changes the gateway doesn't see (a new
  account in another group) show within 30 s. Kalks Trader caches the tenant config for 30 s.
