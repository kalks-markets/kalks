# Production deploy (VPS)

One Ubuntu VPS runs everything. Only Caddy (ports 80/443) and SSH are reachable from the internet.

| Host | Serves | Local port |
|---|---|---|
| app.kalkstrade.com | Client Area | 3000 |
| admin.kalkstrade.com | Back Office | 3001 |
| trade.kalkstrade.com | Kalks Trader | 3002 |
| api.kalkstrade.com | market-data (public quotes, candles, stream; `/v1/admin*` blocked); `/v1/public/leaderboard` and `/v1/public/contests` go to growth (8101, GET only, cached 15 s, nicknames only: docs/CHALLENGES.md) | 8081 |
| kalkstrade.com | website (repo kalks-markets/kalks-website, `deploy-website.sh`) | 3010 |
| (internal only) | gateway / auth | 8080 |
| (internal only) | trading engine (`/engine/stream` on trade./admin. proxies its ticket-authenticated WebSockets) | 8090 |
| (internal only) | wallet (USDT deposits / withdrawals, wallet ↔ trading; reached only by the app BFFs and internal services) | 8095 |
| (internal only) | IB / referral programme (referral tree, commissions, payout batches → wallet; reached only by the app BFFs) | 8096 |
| (internal only) | prop firm service (rule evaluator, challenges, payouts; reached only by the app BFFs) | 8097 |
| (internal only) | Kalks FX Options (reference data, listings, TWAP fixings, chain REST + WS, engine snapshot; `/options/stream` on trade. and `/v1/public/options/*` on api. are its only public routes; module OFF per broker until switched on) | 8104 |
| (internal only) | content + media library (editable pages, images with light / dark files; `/v1/public/content` and `/v1/public/media/*` on api. are its only public routes, GET only) | 8105 |
| (internal only) | PostgreSQL | 5432 |

- `deploy.sh` pulls `main`, builds, installs the units in `systemd/` and the `Caddyfile`, restarts, and checks health.
- Secrets live only on the server: `~/kalks/.env.local` (services) and `apps/*/.env.production.local` (apps). They are generated there and never committed.
- KYC documents: `deploy.sh` generates `KYC_ENCRYPTION_KEY` (AES-256-GCM, `openssl rand -hex 32`) and sets `KYC_STORAGE_DIR=~/.kalks-data/kyc` (0700) in `~/kalks/.env.local` on first deploy. Back up the key together with the directory: files can't be decrypted without it. Never serve that directory; staff read documents only through the Back Office (`/api/admin/kyc/documents/{id}/file`). Caddy caps `/api/kyc/documents` uploads at 12 MB. Optional: `KYC_SLA_HOURS` (review target, default 24).
- Content and media: `deploy.sh` generates `CONTENT_INTERNAL_TOKEN`, sets `CONTENT_DATABASE_URL` (kalks_content), `CONTENT_STORAGE_DIR=~/.kalks-data/content` (public images, back it up with the database) and `CONTENT_PUBLIC_URL=https://api.kalkstrade.com`, and gives the apps `CONTENT_URL` (the Back Office also the token and the `CONTENT_PREVIEW_*_URL` bases). Caddy caps Back Office media uploads (`/api/content/media*`) at 11 MB. First fill of the website copy: `node scripts/seed-website-content.mjs --publish` (docs/CONTENT.md).
- Caddy reads `SITE_LOCK_USER` / `SITE_LOCK_HASH` from `/etc/caddy/kalks.env` for the pre-launch lock.
- Logs: `journalctl -u kalks-<name> -f`.

## White-label broker domains and tenant isolation

**Domains.** Each broker (tenant) has its own hosts in `tenant_domains` (gateway), one app per host: `website`, `app` (Client Area), `trade` (Trader) and `admin` (Back Office). The Platform Owner manages them in Back Office → Brokers → a broker → Domains (add, change app, disable, remove, check DNS). `tenants.domains` is kept as a mirror of the active rows. Optional: `GATEWAY_PUBLIC_IPS=203.0.113.10,2001:db8::10` in `~/kalks/.env.local` makes "Check DNS" verify that a domain points at this server (otherwise any DNS answer counts).

**How a request finds its broker.** The apps' server-side BFFs forward the visitor's host to the gateway in `X-Kalks-Host`. The gateway (services/gateway/src/domains.rs) picks the tenant in this order:

1. the host is an active `tenant_domains` row → that broker (it wins over any `X-Kalks-Tenant` header);
2. an explicit `X-Kalks-Tenant` slug from the BFF (`kalks` today);
3. the default tenant `kalks`. Unknown hosts (localhost in development, the bare server IP) therefore get Kalks.

Lookups are cached for 30 s in the gateway (cleared on every Owner change) and the apps cache the broker's config / branding per host (5 s Client Area, 30 s Back Office and Trader). The branding (name, logo, primary / accent colours, support email, domains by app) comes from `GET /v1/public/tenant-config` (`branding`) or `GET /v1/public/tenant-by-host?host=`; the apps render it on sign-in pages and in the app shell and swap the `--k-ember` / `--k-gold` tokens for the broker's colours. Kalks keeps its stock look.

**TLS for new broker domains (Caddy on-demand TLS).** Prepared but not enabled: see the commented block at the end of `deploy/Caddyfile`. A catch-all `https://` site with `tls { on_demand }` issues a certificate on the first visit to a broker host, after asking `http://127.0.0.1:8080/v1/public/domain-check?domain=<host>` (global `on_demand_tls { ask … }`). The gateway answers `200` only for an active domain of an active broker, `404` otherwise, and `403` to anything that isn't a direct loopback connection (no internal token is needed because Caddy can't send one; the endpoint reveals nothing but yes / no). Suspending a broker or disabling a domain stops new certificates. The existing kalkstrade.com blocks are unchanged and keep precedence. To enable: move the `on_demand_tls` option into the global block, uncomment the site block, `caddy validate --config /etc/caddy/Caddyfile`, `systemctl reload caddy`, then point the broker's DNS (A/AAAA) at the server and add its hosts in the Owner panel. Broker hosts are routed to an app by their first label (`app.`/`my.`/`client.`/`portal.` → Client Area, `trade.`/`trader.`/`webtrader.` → Trader, `admin.`/`backoffice.`/`bo.` → Back Office; apex / www → redirect to `app.`).

**Row-level security.** The gateway's tenant-scoped tables (users, staff, sessions, email_otps, trusted_devices, audit_log, roles, staff_invites, tenant_ip_allowlist, tenant_features, tenant_domains, kyc_*, stepup_tokens, trade_shares, client_viewers, client_requests) have RLS enabled and forced (migration `20260929180000_tenant_domains_rls.sql`). The pool's own login is unaffected while `app.tenant_id` is unset, so existing queries behave as before. Tenant-scoped request paths (today: the Back Office client list and client detail) run in a transaction that calls `kalks_enter_tenant(<tenant id>)`: it sets `app.tenant_id` and switches to the NOLOGIN, NOBYPASSRLS role `kalks_tenant`, which can only read and write that tenant's rows (and sees nothing when no tenant is set). The migration creates the role when the database login may (`CREATEROLE` or superuser); without it, `app.tenant_id` alone still scopes a non-superuser table owner because RLS is forced. New tenant tables in later migrations need their own `ENABLE / FORCE ROW LEVEL SECURITY` + `tenant_isolation` policy (`USING (kalks_tenant_row(tenant_id))`); the role's table grants follow via default privileges.
