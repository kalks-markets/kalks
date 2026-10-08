#!/usr/bin/env bash
# Deploy the current main branch on the Kalks VPS. Run as the `kalks` user:  ~/kalks/deploy/deploy.sh
set -euo pipefail
cd "$(dirname "$0")/.."
source ~/.cargo/env
export COREPACK_ENABLE_DOWNLOAD_PROMPT=0 NODE_OPTIONS=--max-old-space-size=6144

# Pull first, then run the freshly pulled copy of this script (bash reads scripts while running, so
# steps added by the pull would otherwise be skipped).
if [ -z "${KALKS_DEPLOY_PULLED:-}" ]; then
  git pull --ff-only
  KALKS_DEPLOY_PULLED=1 exec "$0" "$@"
fi
pnpm install --frozen-lockfile
cargo build --release -p market-data -p gateway -p trading -p prop -p ib
cargo build --release -p academy
cargo build --release -p algo
cargo build --release -p wallet
cargo build --release -p support
cargo build --release -p growth
cargo build --release -p reports
cargo build --release -p news
cargo build --release -p options
cargo build --release -p circle

# trading engine secrets are generated on the server on first deploy (never committed, never printed)
touch .env.local
grep -q '^TRADING_SESSION_SECRET=' .env.local || printf '\nTRADING_SESSION_SECRET=%s\n' "$(openssl rand -hex 32)" >> .env.local
grep -q '^TRADING_INTERNAL_TOKEN=' .env.local || printf 'TRADING_INTERNAL_TOKEN=%s\n' "$(openssl rand -hex 32)" >> .env.local
if ! grep -q '^TRADING_DATABASE_URL=' .env.local && grep -q '^GATEWAY_DATABASE_URL=' .env.local; then
  # same server and credentials as the gateway, database kalks_trading (created on first start)
  printf 'TRADING_DATABASE_URL=%s\n' "$(grep '^GATEWAY_DATABASE_URL=' .env.local | cut -d= -f2- | sed -E 's#/[^/?]+([?].*)?$#/kalks_trading\1#')" >> .env.local
fi
# KYC documents (gateway): AES-256-GCM data key generated once and never printed. BACK IT UP: without it the
# stored documents can't be decrypted. Files live outside every web root, 0700 / 0600.
grep -q '^KYC_ENCRYPTION_KEY=' .env.local || printf 'KYC_ENCRYPTION_KEY=%s\n' "$(openssl rand -hex 32)" >> .env.local
grep -q '^KYC_STORAGE_DIR=' .env.local || printf 'KYC_STORAGE_DIR=%s\n' "$HOME/.kalks-data/kyc" >> .env.local
install -d -m 700 "$(grep '^KYC_STORAGE_DIR=' .env.local | cut -d= -f2-)"
# IB service secrets (same rules as the engine): internal token generated once, database kalks_ib
grep -q '^IB_INTERNAL_TOKEN=' .env.local || printf 'IB_INTERNAL_TOKEN=%s\n' "$(openssl rand -hex 32)" >> .env.local
if ! grep -q '^IB_DATABASE_URL=' .env.local && grep -q '^GATEWAY_DATABASE_URL=' .env.local; then
  printf 'IB_DATABASE_URL=%s\n' "$(grep '^GATEWAY_DATABASE_URL=' .env.local | cut -d= -f2- | sed -E 's#/[^/?]+([?].*)?$#/kalks_ib\1#')" >> .env.local
fi
# the Client Area and Back Office BFFs reach the IB service with the same token
for app in apps/crm apps/admin apps/terminal; do
  f="$app/.env.production.local"; touch "$f"
  grep -q '^IB_URL=' "$f" || printf 'IB_URL=http://127.0.0.1:8096\n' >> "$f"
  grep -q '^IB_INTERNAL_TOKEN=' "$f" || printf 'IB_INTERNAL_TOKEN=%s\n' "$(grep '^IB_INTERNAL_TOKEN=' .env.local | cut -d= -f2-)" >> "$f"
done
# Academy secrets: internal token generated once, database kalks_academy, public URL printed on certificates;
# the Client Area and Back Office BFFs reach the service with the same token
grep -q '^ACADEMY_INTERNAL_TOKEN=' .env.local || printf 'ACADEMY_INTERNAL_TOKEN=%s\n' "$(openssl rand -hex 32)" >> .env.local
if ! grep -q '^ACADEMY_DATABASE_URL=' .env.local && grep -q '^GATEWAY_DATABASE_URL=' .env.local; then
  printf 'ACADEMY_DATABASE_URL=%s\n' "$(grep '^GATEWAY_DATABASE_URL=' .env.local | cut -d= -f2- | sed -E 's#/[^/?]+([?].*)?$#/kalks_academy\1#')" >> .env.local
fi
grep -q '^ACADEMY_VERIFY_URL=' .env.local || printf 'ACADEMY_VERIFY_URL=https://app.kalkstrade.com\n' >> .env.local
for f in apps/crm/.env.production.local apps/admin/.env.production.local; do
  touch "$f"
  grep -q '^ACADEMY_URL=' "$f" || printf 'ACADEMY_URL=http://127.0.0.1:8098\n' >> "$f"
  grep -q '^ACADEMY_INTERNAL_TOKEN=' "$f" || printf 'ACADEMY_INTERNAL_TOKEN=%s\n' "$(grep '^ACADEMY_INTERNAL_TOKEN=' .env.local | cut -d= -f2-)" >> "$f"
done
# prop service secrets (same rules as the engine): internal token generated once, database kalks_prop
grep -q '^PROP_INTERNAL_TOKEN=' .env.local || printf 'PROP_INTERNAL_TOKEN=%s\n' "$(openssl rand -hex 32)" >> .env.local
if ! grep -q '^PROP_DATABASE_URL=' .env.local && grep -q '^GATEWAY_DATABASE_URL=' .env.local; then
  printf 'PROP_DATABASE_URL=%s\n' "$(grep '^GATEWAY_DATABASE_URL=' .env.local | cut -d= -f2- | sed -E 's#/[^/?]+([?].*)?$#/kalks_prop\1#')" >> .env.local
fi
# the Client Area and Back Office BFFs reach the prop service with the same token
for app in apps/crm apps/admin apps/terminal; do
  f="$app/.env.production.local"; touch "$f"
  grep -q '^PROP_URL=' "$f" || printf 'PROP_URL=http://127.0.0.1:8097\n' >> "$f"
  grep -q '^PROP_INTERNAL_TOKEN=' "$f" || printf 'PROP_INTERNAL_TOKEN=%s\n' "$(grep '^PROP_INTERNAL_TOKEN=' .env.local | cut -d= -f2-)" >> "$f"
done
# ALGO service secrets: internal token and API-key HMAC master generated once (never printed), database
# kalks_algo. The AI assistant's Claude key is read from .env.claude (copied from the terminal's env if missing).
grep -q '^ALGO_INTERNAL_TOKEN=' .env.local || printf 'ALGO_INTERNAL_TOKEN=%s\n' "$(openssl rand -hex 32)" >> .env.local
grep -q '^ALGO_KEY_SECRET=' .env.local || printf 'ALGO_KEY_SECRET=%s\n' "$(openssl rand -hex 32)" >> .env.local
if ! grep -q '^ALGO_DATABASE_URL=' .env.local && grep -q '^GATEWAY_DATABASE_URL=' .env.local; then
  printf 'ALGO_DATABASE_URL=%s\n' "$(grep '^GATEWAY_DATABASE_URL=' .env.local | cut -d= -f2- | sed -E 's#/[^/?]+([?].*)?$#/kalks_algo\1#')" >> .env.local
fi
if ! grep -q '^ANTHROPIC_API_KEY=' .env.claude 2>/dev/null && grep -q '^ANTHROPIC_API_KEY=' apps/terminal/.env.production.local 2>/dev/null; then
  (umask 077; grep '^ANTHROPIC_API_KEY=' apps/terminal/.env.production.local > .env.claude)
fi
# the Client Area and Back Office BFFs reach the ALGO service with the same token
for app in apps/crm apps/admin apps/terminal; do
  f="$app/.env.production.local"; touch "$f"
  grep -q '^ALGO_URL=' "$f" || printf 'ALGO_URL=http://127.0.0.1:8099\n' >> "$f"
  grep -q '^ALGO_INTERNAL_TOKEN=' "$f" || printf 'ALGO_INTERNAL_TOKEN=%s\n' "$(grep '^ALGO_INTERNAL_TOKEN=' .env.local | cut -d= -f2-)" >> "$f"
done
# ALGO service secrets: internal token and API-key HMAC master generated once (never printed), database
# kalks_algo. The AI assistant's Claude key is read from .env.claude (copied from the terminal's env if missing).
grep -q '^ALGO_INTERNAL_TOKEN=' .env.local || printf 'ALGO_INTERNAL_TOKEN=%s\n' "$(openssl rand -hex 32)" >> .env.local
grep -q '^ALGO_KEY_SECRET=' .env.local || printf 'ALGO_KEY_SECRET=%s\n' "$(openssl rand -hex 32)" >> .env.local
if ! grep -q '^ALGO_DATABASE_URL=' .env.local && grep -q '^GATEWAY_DATABASE_URL=' .env.local; then
  printf 'ALGO_DATABASE_URL=%s\n' "$(grep '^GATEWAY_DATABASE_URL=' .env.local | cut -d= -f2- | sed -E 's#/[^/?]+([?].*)?$#/kalks_algo\1#')" >> .env.local
fi
if ! grep -q '^ANTHROPIC_API_KEY=' .env.claude 2>/dev/null && grep -q '^ANTHROPIC_API_KEY=' apps/terminal/.env.production.local 2>/dev/null; then
  (umask 077; grep '^ANTHROPIC_API_KEY=' apps/terminal/.env.production.local > .env.claude)
fi
# the Client Area and Back Office BFFs reach the ALGO service with the same token
for app in apps/crm apps/admin apps/terminal; do
  f="$app/.env.production.local"; touch "$f"
  grep -q '^ALGO_URL=' "$f" || printf 'ALGO_URL=http://127.0.0.1:8099\n' >> "$f"
  grep -q '^ALGO_INTERNAL_TOKEN=' "$f" || printf 'ALGO_INTERNAL_TOKEN=%s\n' "$(grep '^ALGO_INTERNAL_TOKEN=' .env.local | cut -d= -f2-)" >> "$f"
done
# wallet secrets (never printed): internal token generated once, database kalks_wallet next to the gateway's.
# Receiving addresses are seeded from WALLET_BSC_ADDRESS / WALLET_TRON_ADDRESS on the first start only; after
# that they are an audited Back Office setting. TRONGRID_API_KEY comes from .env.tron when present.
grep -q '^WALLET_INTERNAL_TOKEN=' .env.local || printf 'WALLET_INTERNAL_TOKEN=%s\n' "$(openssl rand -hex 32)" >> .env.local
if ! grep -q '^WALLET_DATABASE_URL=' .env.local && grep -q '^GATEWAY_DATABASE_URL=' .env.local; then
  printf 'WALLET_DATABASE_URL=%s\n' "$(grep '^GATEWAY_DATABASE_URL=' .env.local | cut -d= -f2- | sed -E 's#/[^/?]+([?].*)?$#/kalks_wallet\1#')" >> .env.local
fi
grep -q '^WALLET_BSC_ADDRESS=' .env.local || printf 'WALLET_BSC_ADDRESS=0x11e9373d598703F83582e34378E086EbEEC5da11\n' >> .env.local
grep -q '^WALLET_TRON_ADDRESS=' .env.local || printf 'WALLET_TRON_ADDRESS=TU7PHUS22Hw632YsnAyjxNh4gu3u8PzcHZ\n' >> .env.local
if ! grep -q '^TRONGRID_API_KEY=' .env.local && [ -f .env.tron ] && grep -q '^TRONGRID_API_KEY=' .env.tron; then
  grep '^TRONGRID_API_KEY=' .env.tron >> .env.local
fi
# the Client Area and Back Office BFFs reach the wallet with the same token
for app in apps/crm apps/admin apps/terminal; do
  f="$app/.env.production.local"; touch "$f"
  grep -q '^WALLET_URL=' "$f" || printf 'WALLET_URL=http://127.0.0.1:8095\n' >> "$f"
  grep -q '^WALLET_INTERNAL_TOKEN=' "$f" || printf 'WALLET_INTERNAL_TOKEN=%s\n' "$(grep '^WALLET_INTERNAL_TOKEN=' .env.local | cut -d= -f2-)" >> "$f"
done
# support + notifications service: internal token generated once (never printed), database kalks_support,
# chat attachments stored privately under ~/.kalks-data/support. The AI help bot's Claude key is read from
# .env.claude (see ALGO above); emails go through the same SMTP relay settings as the gateway (SMTP_*).
grep -q '^SUPPORT_INTERNAL_TOKEN=' .env.local || printf 'SUPPORT_INTERNAL_TOKEN=%s\n' "$(openssl rand -hex 32)" >> .env.local
if ! grep -q '^SUPPORT_DATABASE_URL=' .env.local && grep -q '^GATEWAY_DATABASE_URL=' .env.local; then
  printf 'SUPPORT_DATABASE_URL=%s\n' "$(grep '^GATEWAY_DATABASE_URL=' .env.local | cut -d= -f2- | sed -E 's#/[^/?]+([?].*)?$#/kalks_support\1#')" >> .env.local
fi
grep -q '^SUPPORT_STORAGE_DIR=' .env.local || printf 'SUPPORT_STORAGE_DIR=%s\n' "$HOME/.kalks-data/support" >> .env.local
grep -q '^SUPPORT_APP_URL=' .env.local || printf 'SUPPORT_APP_URL=https://app.kalkstrade.com\n' >> .env.local
install -d -m 700 "$(grep '^SUPPORT_STORAGE_DIR=' .env.local | cut -d= -f2-)"
# the Client Area, Back Office and Kalks Trader BFFs reach the support service with the same token; browsers
# open the realtime stream at wss://<host>/support/stream (Caddy). Wallet, prop and IB push notifications with it too.
for app in apps/crm apps/admin apps/terminal; do
  f="$app/.env.production.local"; touch "$f"
  grep -q '^SUPPORT_URL=' "$f" || printf 'SUPPORT_URL=http://127.0.0.1:8100\n' >> "$f"
  grep -q '^SUPPORT_INTERNAL_TOKEN=' "$f" || printf 'SUPPORT_INTERNAL_TOKEN=%s\n' "$(grep '^SUPPORT_INTERNAL_TOKEN=' .env.local | cut -d= -f2-)" >> "$f"
done
# the Client Area's mobile AI routes (/api/mobile/trade/ai-trader, /options/explain) use the same Claude key
if ! grep -q '^ANTHROPIC_API_KEY=' apps/crm/.env.production.local 2>/dev/null && grep -q '^ANTHROPIC_API_KEY=' .env.claude 2>/dev/null; then
  (umask 077; touch apps/crm/.env.production.local; grep '^ANTHROPIC_API_KEY=' .env.claude >> apps/crm/.env.production.local)
fi
# growth (rewards + marketing) secrets: internal token generated once, database kalks_growth next to the gateway's;
# the Client Area, Back Office and Kalks Trader BFFs reach the service with the same token (Kalks Trader: option share cards)
grep -q '^GROWTH_INTERNAL_TOKEN=' .env.local || printf 'GROWTH_INTERNAL_TOKEN=%s\n' "$(openssl rand -hex 32)" >> .env.local
if ! grep -q '^GROWTH_DATABASE_URL=' .env.local && grep -q '^GATEWAY_DATABASE_URL=' .env.local; then
  printf 'GROWTH_DATABASE_URL=%s\n' "$(grep '^GATEWAY_DATABASE_URL=' .env.local | cut -d= -f2- | sed -E 's#/[^/?]+([?].*)?$#/kalks_growth\1#')" >> .env.local
fi
for app in apps/crm apps/admin apps/terminal; do
  f="$app/.env.production.local"; touch "$f"
  grep -q '^GROWTH_URL=' "$f" || printf 'GROWTH_URL=http://127.0.0.1:8101\n' >> "$f"
  grep -q '^GROWTH_INTERNAL_TOKEN=' "$f" || printf 'GROWTH_INTERNAL_TOKEN=%s\n' "$(grep '^GROWTH_INTERNAL_TOKEN=' .env.local | cut -d= -f2-)" >> "$f"
done
# reports (statements, analytics, broker reports) secrets: internal token generated once, database kalks_reports next
# to the gateway's; scheduled report emails use the gateway's SMTP_* relay settings. The BFFs reach it with the same token.
grep -q '^REPORTS_INTERNAL_TOKEN=' .env.local || printf 'REPORTS_INTERNAL_TOKEN=%s\n' "$(openssl rand -hex 32)" >> .env.local
if ! grep -q '^REPORTS_DATABASE_URL=' .env.local && grep -q '^GATEWAY_DATABASE_URL=' .env.local; then
  printf 'REPORTS_DATABASE_URL=%s\n' "$(grep '^GATEWAY_DATABASE_URL=' .env.local | cut -d= -f2- | sed -E 's#/[^/?]+([?].*)?$#/kalks_reports\1#')" >> .env.local
fi
for app in apps/crm apps/admin apps/terminal; do
  f="$app/.env.production.local"; touch "$f"
  grep -q '^REPORTS_URL=' "$f" || printf 'REPORTS_URL=http://127.0.0.1:8102\n' >> "$f"
  grep -q '^REPORTS_INTERNAL_TOKEN=' "$f" || printf 'REPORTS_INTERNAL_TOKEN=%s\n' "$(grep '^REPORTS_INTERNAL_TOKEN=' .env.local | cut -d= -f2-)" >> "$f"
done
# news + economic calendar service: internal token generated once (never printed), database kalks_news next to
# the gateway's. Calendar reminders go through the support service (SUPPORT_INTERNAL_TOKEN); the daily AI brief
# reads the Claude key from .env.claude. The Client Area, Back Office and Kalks Trader BFFs use the same token.
grep -q '^NEWS_INTERNAL_TOKEN=' .env.local || printf 'NEWS_INTERNAL_TOKEN=%s\n' "$(openssl rand -hex 32)" >> .env.local
if ! grep -q '^NEWS_DATABASE_URL=' .env.local && grep -q '^GATEWAY_DATABASE_URL=' .env.local; then
  printf 'NEWS_DATABASE_URL=%s\n' "$(grep '^GATEWAY_DATABASE_URL=' .env.local | cut -d= -f2- | sed -E 's#/[^/?]+([?].*)?$#/kalks_news\1#')" >> .env.local
fi
for app in apps/crm apps/admin apps/terminal; do
  f="$app/.env.production.local"; touch "$f"
  grep -q '^NEWS_URL=' "$f" || printf 'NEWS_URL=http://127.0.0.1:8103\n' >> "$f"
  grep -q '^NEWS_INTERNAL_TOKEN=' "$f" || printf 'NEWS_INTERNAL_TOKEN=%s\n' "$(grep '^NEWS_INTERNAL_TOKEN=' .env.local | cut -d= -f2-)" >> "$f"
done
# Kalks FX Options service: internal token generated once (never printed), database kalks_options next to the
# gateway's. The trading engine (reads .env.local) and the Client Area, Back Office and Kalks Trader BFFs use the same
# token. The module itself stays OFF per broker until switched on in the Back Office (tenant kalks: demo only).
grep -q '^OPTIONS_INTERNAL_TOKEN=' .env.local || printf 'OPTIONS_INTERNAL_TOKEN=%s\n' "$(openssl rand -hex 32)" >> .env.local
if ! grep -q '^OPTIONS_DATABASE_URL=' .env.local && grep -q '^GATEWAY_DATABASE_URL=' .env.local; then
  printf 'OPTIONS_DATABASE_URL=%s\n' "$(grep '^GATEWAY_DATABASE_URL=' .env.local | cut -d= -f2- | sed -E 's#/[^/?]+([?].*)?$#/kalks_options\1#')" >> .env.local
fi
grep -q '^OPTIONS_URL=' .env.local || printf 'OPTIONS_URL=http://127.0.0.1:8104\n' >> .env.local
for app in apps/crm apps/admin apps/terminal; do
  f="$app/.env.production.local"; touch "$f"
  grep -q '^OPTIONS_URL=' "$f" || printf 'OPTIONS_URL=http://127.0.0.1:8104\n' >> "$f"
  grep -q '^OPTIONS_INTERNAL_TOKEN=' "$f" || printf 'OPTIONS_INTERNAL_TOKEN=%s\n' "$(grep '^OPTIONS_INTERNAL_TOKEN=' .env.local | cut -d= -f2-)" >> "$f"
done
# Kalks Circle (community): internal token and trade-card signing key generated once (never printed), database
# kalks_circle next to the gateway's. Media on local disk at /srv/kalks/circle-media (served by Caddy at
# /circle/media/*) until Cloudflare R2 is configured (CIRCLE_STORAGE=s3 + CIRCLE_S3_*); private upload / transcode
# space under ~/.kalks-data/circle. Video needs ffmpeg (installed here when sudo allows it). The AI checks use the
# Claude key from .env.claude; push stays off until FCM_SERVICE_ACCOUNT_FILE points at the Firebase service account.
grep -q '^CIRCLE_INTERNAL_TOKEN=' .env.local || printf 'CIRCLE_INTERNAL_TOKEN=%s\n' "$(openssl rand -hex 32)" >> .env.local
grep -q '^CIRCLE_CARD_SECRET=' .env.local || printf 'CIRCLE_CARD_SECRET=%s\n' "$(openssl rand -hex 32)" >> .env.local
if ! grep -q '^CIRCLE_DATABASE_URL=' .env.local && grep -q '^GATEWAY_DATABASE_URL=' .env.local; then
  printf 'CIRCLE_DATABASE_URL=%s\n' "$(grep '^GATEWAY_DATABASE_URL=' .env.local | cut -d= -f2- | sed -E 's#/[^/?]+([?].*)?$#/kalks_circle\1#')" >> .env.local
fi
grep -q '^CIRCLE_MEDIA_DIR=' .env.local || printf 'CIRCLE_MEDIA_DIR=/srv/kalks/circle-media\n' >> .env.local
grep -q '^CIRCLE_WORK_DIR=' .env.local || printf 'CIRCLE_WORK_DIR=%s\n' "$HOME/.kalks-data/circle/work" >> .env.local
sudo install -d -o "$(id -un)" -g "$(id -gn)" -m 755 /srv/kalks/circle-media
install -d -m 700 "$(grep '^CIRCLE_WORK_DIR=' .env.local | cut -d= -f2-)"
if ! command -v ffmpeg >/dev/null 2>&1; then
  sudo -n env DEBIAN_FRONTEND=noninteractive apt-get install -y -qq ffmpeg >/dev/null 2>&1 \
    || echo "WARNING: ffmpeg is missing and could not be installed: run 'sudo apt install ffmpeg' (Circle video uploads fail until then)"
fi
# the Client Area BFF (and the Back Office, for Back Office > Circle) reach the service with the same token
for app in apps/crm apps/admin; do
  f="$app/.env.production.local"; touch "$f"
  grep -q '^CIRCLE_URL=' "$f" || printf 'CIRCLE_URL=http://127.0.0.1:8105\n' >> "$f"
  grep -q '^CIRCLE_INTERNAL_TOKEN=' "$f" || printf 'CIRCLE_INTERNAL_TOKEN=%s\n' "$(grep '^CIRCLE_INTERNAL_TOKEN=' .env.local | cut -d= -f2-)" >> "$f"
done
# the Client Area's public URLs, inlined at build time: market-data at the public edge (live quotes in the browser)
# and Kalks Trader (Trade links and sign-in hand-off)
f=apps/crm/.env.production.local; touch "$f"
grep -q '^NEXT_PUBLIC_MARKET_DATA_URL=' "$f" || printf 'NEXT_PUBLIC_MARKET_DATA_URL=https://api.kalkstrade.com\n' >> "$f"
grep -q '^NEXT_PUBLIC_TERMINAL_URL=' "$f" || printf 'NEXT_PUBLIC_TERMINAL_URL=https://trade.kalkstrade.com\n' >> "$f"
pnpm turbo run build --filter=@kalks/crm --filter=@kalks/admin --filter=@kalks/terminal --concurrency=1

# market-data holds the price provider's WebSocket connections, which the provider limits per key and keeps
# counting for a while after a restart (reconnects are then refused with HTTP 429). So it restarts only when what it
# runs changed: its binary, its config inputs (instruments, holiday calendars, its unit file, its .env.local
# settings). The fingerprint of what it was last started with is kept in ~/.kalks-deploy.
md_fingerprint() {
  {
    if [ -f target/release/market-data ]; then sha256sum target/release/market-data | cut -d' ' -f1; else echo no-binary; fi
    find config/instruments.json config/holidays -type f -print0 | sort -z | xargs -0 sha256sum
    sha256sum deploy/systemd/kalks-market-data.service
    grep -E '^(INFOWAY_|MARKET_DATA_|INSTRUMENTS_FILE=|HOLIDAYS_DIR=|DATABASE_URL=|STORE_TICKS=|TICKS_RETENTION_HOURS=|BACKFILL_|RUST_LOG=)' .env.local 2>/dev/null | sha256sum
  } | sha256sum | cut -d' ' -f1
}
MD_FP_FILE="$HOME/.kalks-deploy/market-data.fingerprint"
mkdir -p "$(dirname "$MD_FP_FILE")"

# service units + edge config (idempotent)
sudo cp deploy/systemd/*.service /etc/systemd/system/
sudo cp deploy/Caddyfile /etc/caddy/Caddyfile
sudo systemctl daemon-reload
sudo systemctl enable kalks-market-data kalks-gateway kalks-trading kalks-ib kalks-prop kalks-crm kalks-admin kalks-terminal >/dev/null
md_now="$(md_fingerprint)"
if [ "$md_now" != "$(cat "$MD_FP_FILE" 2>/dev/null || true)" ]; then
  echo "market-data changed: restarting it (it closes its provider connections first)"
  sudo systemctl restart kalks-market-data
  printf '%s\n' "$md_now" > "$MD_FP_FILE"
elif ! sudo systemctl is-active --quiet kalks-market-data; then
  echo "market-data unchanged but not running: starting it"
  sudo systemctl start kalks-market-data
else
  echo "market-data unchanged: not restarted (provider connections untouched)"
fi
sudo systemctl restart kalks-gateway kalks-trading kalks-ib kalks-prop kalks-crm kalks-admin kalks-terminal
sudo systemctl enable kalks-academy >/dev/null && sudo systemctl restart kalks-academy
sudo systemctl enable kalks-algo >/dev/null && sudo systemctl restart kalks-algo
sudo systemctl enable kalks-wallet >/dev/null && sudo systemctl restart kalks-wallet
sudo systemctl enable kalks-support >/dev/null && sudo systemctl restart kalks-support
sudo systemctl enable kalks-growth >/dev/null && sudo systemctl restart kalks-growth
sudo systemctl enable kalks-reports >/dev/null && sudo systemctl restart kalks-reports
sudo systemctl enable kalks-news >/dev/null && sudo systemctl restart kalks-news
sudo systemctl enable kalks-options >/dev/null && sudo systemctl restart kalks-options
sudo systemctl enable kalks-circle >/dev/null && sudo systemctl restart kalks-circle
sudo systemctl reload caddy || echo "caddy reload timed out (long-lived connections); config is validated, continuing"
sleep 5
for u in 127.0.0.1:8081/health 127.0.0.1:8080/health 127.0.0.1:8090/health 127.0.0.1:8096/health 127.0.0.1:8097/health 127.0.0.1:3000/login 127.0.0.1:3001/login 127.0.0.1:3002/login; do
  printf "%-26s %s\n" "$u" "$(curl -s -o /dev/null -w '%{http_code}' "http://$u")"
done
printf "%-26s %s\n" 127.0.0.1:8098/health "$(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:8098/health)"
printf "%-26s %s\n" 127.0.0.1:8095/health "$(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:8095/health)"
printf "%-26s %s\n" 127.0.0.1:8099/health "$(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:8099/health)"
printf "%-26s %s\n" 127.0.0.1:8100/health "$(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:8100/health)"
printf "%-26s %s\n" 127.0.0.1:8101/health "$(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:8101/health)"
printf "%-26s %s\n" 127.0.0.1:8102/health "$(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:8102/health)"
printf "%-26s %s\n" 127.0.0.1:8103/health "$(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:8103/health)"
printf "%-26s %s\n" 127.0.0.1:8104/health "$(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:8104/health)"
printf "%-26s %s\n" 127.0.0.1:8105/health "$(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:8105/health)"
