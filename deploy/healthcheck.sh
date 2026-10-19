#!/usr/bin/env bash
# Kalks production watchdog (systemd timer, every minute).
# A service that fails its health check 3 runs in a row is restarted and support is alerted (email, and Telegram
# when TELEGRAM_BOT_TOKEN + TELEGRAM_CHAT_ID are set in /etc/kalks/alerts.env). Units that are not enabled
# (a module switched off at deploy, e.g. KALKS_DISABLE_PROP=1) are skipped.
set -uo pipefail
STATE=/var/lib/kalks-health; mkdir -p "$STATE"
ALERT_TO="${KALKS_ALERT_TO:-support@kalkstrade.com}"

check() { # url python-condition-on-d (empty = HTTP 200 is enough)
  local url=$1 cond=$2 body code
  if [ -z "$cond" ]; then
    code=$(curl -s -o /dev/null -m 15 -w '%{http_code}' "$url"); [ "$code" = 200 ]; return
  fi
  body=$(curl -s -m 8 "$url") && python3 -c "import json,sys; d=json.loads(sys.argv[1]); sys.exit(0 if ($cond) else 1)" "$body" 2>/dev/null
}

alert() { # subject body
  python3 - "$ALERT_TO" "$1" "$2" <<'PY'
import smtplib, ssl, sys
from email.message import EmailMessage
to, subject, body = sys.argv[1:4]
m = EmailMessage(); m["From"] = "Kalks Monitor <no-reply@kalkstrade.com>"; m["To"] = to; m["Subject"] = subject; m.set_content(body)
try:
    with smtplib.SMTP("smtp-relay.gmail.com", 587, local_hostname="kalkstrade.com", timeout=20) as s:
        s.starttls(context=ssl.create_default_context()); s.ehlo("kalkstrade.com"); s.send_message(m)
except Exception as e:
    print("alert email failed:", e)
PY
  if [ -n "${TELEGRAM_BOT_TOKEN:-}" ] && [ -n "${TELEGRAM_CHAT_ID:-}" ]; then
    curl -s -m 15 -o /dev/null "https://api.telegram.org/bot${TELEGRAM_BOT_TOKEN}/sendMessage" \
      --data-urlencode "chat_id=${TELEGRAM_CHAT_ID}" --data-urlencode "text=$1"$'\n'"$2" || echo "alert telegram failed"
  fi
}

run() { # unit name url condition
  local svc=$1 name=$2 url=$3 cond=$4 f="$STATE/$1"
  systemctl is-enabled --quiet "$svc" 2>/dev/null || return 0
  if check "$url" "$cond"; then
    [ -f "$f" ] && [ "$(cat "$f")" -ge 3 ] && alert "Kalks: $name recovered" "$name is healthy again ($(date -u))."
    echo 0 > "$f"; return
  fi
  local n=$(( $( [ -f "$f" ] && cat "$f" || echo 0 ) + 1 )); echo $n > "$f"
  if [ $n -eq 3 ]; then
    systemctl restart "$svc"
    alert "Kalks: $name was down, restarted" "$name failed its health check 3 minutes in a row and was restarted at $(date -u). Check: journalctl -u $svc"
  fi
}

OK="d.get('status')=='ok'"
run kalks-market-data "Price feed"        http://127.0.0.1:8081/health "d.get('ok') and len(d.get('provider_streams',[]))>0"
run kalks-gateway     "Sign-in service"   http://127.0.0.1:8080/health "$OK and d.get('db')"
run kalks-trading     "Trading engine"    http://127.0.0.1:8090/health "$OK and d.get('feedConnected')"
run kalks-wallet      "Wallet"            http://127.0.0.1:8095/health "$OK and d.get('db') and d.get('workers')"
run kalks-ib          "IB / referrals"    http://127.0.0.1:8096/health "$OK and d.get('db')"
run kalks-prop        "Prop service"      http://127.0.0.1:8097/health "$OK and d.get('db')"
run kalks-academy     "Academy"           http://127.0.0.1:8098/health "$OK"
run kalks-algo        "ALGO service"      http://127.0.0.1:8099/health "$OK and d.get('db')"
run kalks-support     "Support + notifications" http://127.0.0.1:8100/health "$OK and d.get('db')"
run kalks-growth      "Rewards + marketing" http://127.0.0.1:8101/health "$OK and d.get('db')"
run kalks-reports     "Reports"           http://127.0.0.1:8102/health "$OK and d.get('db')"
run kalks-news        "News + calendar"   http://127.0.0.1:8103/health "$OK and d.get('db')"
run kalks-options     "Options service"   http://127.0.0.1:8104/health "$OK and d.get('db')"
run kalks-crm         "Client Area"       http://127.0.0.1:3000/login ""
run kalks-admin       "Back Office"       http://127.0.0.1:3001/login ""
run kalks-terminal    "Kalks Trader"      http://127.0.0.1:3002/login ""
run kalks-website     "Website"           http://127.0.0.1:3010/ ""
