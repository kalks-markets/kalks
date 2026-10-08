#!/usr/bin/env bash
# Start the whole Kalks stack locally (after a reboot or crash). Safe to re-run: anything already
# listening is left alone. Logs go to ~/.kalks-local/<name>.log.
#
#   scripts/dev-services.sh            # Postgres + all services + the three apps
#   scripts/dev-services.sh services   # Postgres + services only
#
# Local market-data runs in relay mode (MARKET_DATA_UPSTREAM in .env.local): the provider allows one
# connection per key, which belongs to production.
set -uo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD
LOGS=~/.kalks-local
mkdir -p "$LOGS"
export PATH="$HOME/.cargo/bin:$PATH"

up() { lsof -iTCP:"$1" -sTCP:LISTEN -n -P >/dev/null 2>&1; }

# ---- Postgres (dedicated Kalks cluster on :5433) ----
PG_BIN=${KALKS_PG_BIN:-$HOME/.swisscresta-local/postgres/bin}
PG_DATA=${KALKS_PG_DATA:-$HOME/.kalks-local/pgdata}
if up 5433; then
  echo "postgres   :5433 already up"
else
  "$PG_BIN/pg_ctl" -D "$PG_DATA" -o "-p 5433" -l "$LOGS/pg.log" start >/dev/null && echo "postgres   :5433 started"
  for _ in $(seq 1 20); do up 5433 && break; sleep 1; done
fi

# ---- Rust services (name:port) — market-data and gateway first, others depend on them ----
SERVICES="market-data:8081 gateway:8080 trading:8090 wallet:8095 ib:8096 prop:8097 academy:8098 algo:8099 support:8100 growth:8101 reports:8102 news:8103 options:8104 circle:8105"
for entry in $SERVICES; do
  name=${entry%%:*}; port=${entry##*:}
  if up "$port"; then echo "$(printf '%-10s' "$name") :$port already up"; continue; fi
  if ! cargo build -q -p "$name" >"$LOGS/$name.build.log" 2>&1; then
    echo "$(printf '%-10s' "$name") :$port BUILD FAILED (see $LOGS/$name.build.log)"; continue
  fi
  (cd "$ROOT/services/$name" && RUST_LOG=${RUST_LOG:-info} nohup "$ROOT/target/debug/$name" >"$LOGS/$name.log" 2>&1 &)
  for _ in $(seq 1 30); do up "$port" && break; sleep 1; done
  if up "$port"; then echo "$(printf '%-10s' "$name") :$port started"; else echo "$(printf '%-10s' "$name") :$port FAILED TO START (see $LOGS/$name.log)"; fi
done

[ "${1:-all}" = "services" ] && exit 0

# ---- Next.js apps ----
for entry in crm:3000 admin:3001 terminal:3002; do
  app=${entry%%:*}; port=${entry##*:}
  if up "$port"; then echo "$(printf '%-10s' "$app") :$port already up"; continue; fi
  (cd "$ROOT/apps/$app" && nohup pnpm dev >"$LOGS/$app-dev.log" 2>&1 &)
  echo "$(printf '%-10s' "$app") :$port starting (first page compile takes a minute)"
done
