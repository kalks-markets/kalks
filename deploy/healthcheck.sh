#!/usr/bin/env bash
# Kalks production watchdog (systemd timer, every minute).
# A service that fails its health check 3 runs in a row is restarted and support is emailed.
set -uo pipefail
STATE=/var/lib/kalks-health; mkdir -p "$STATE"
ALERT_TO="${KALKS_ALERT_TO:-support@kalkstrade.com}"

check() { # name url jq-condition
  local name=$1 url=$2 cond=$3 body
  body=$(curl -s -m 8 "$url") && python3 -c "import json,sys; d=json.loads(sys.argv[1]); sys.exit(0 if ($cond) else 1)" "$body" 2>/dev/null
}

alert() {
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
}

run() { # service name url condition
  local svc=$1 name=$2 url=$3 cond=$4 f="$STATE/$1"
  if check "$name" "$url" "$cond"; then
    [ -f "$f" ] && [ "$(cat "$f")" -ge 3 ] && alert "Kalks: $name recovered" "$name is healthy again ($(date -u))."
    echo 0 > "$f"; return
  fi
  local n=$(( $( [ -f "$f" ] && cat "$f" || echo 0 ) + 1 )); echo $n > "$f"
  if [ $n -eq 3 ]; then
    systemctl restart "$svc"
    alert "Kalks: $name was down, restarted" "$name failed its health check 3 minutes in a row and was restarted at $(date -u). Check: journalctl -u $svc"
  fi
}

run kalks-market-data "Price feed" http://127.0.0.1:8081/health "d.get('ok') and len(d.get('provider_streams',[]))>0"
run kalks-gateway     "Sign-in service" http://127.0.0.1:8080/health "d.get('status')=='ok' and d.get('db')"
run kalks-trading     "Trading engine" http://127.0.0.1:8090/health "d.get('status')=='ok' and d.get('feedConnected')"
run kalks-options     "Options service" http://127.0.0.1:8104/health "d.get('status')=='ok' and d.get('db')"
run kalks-circle      "Kalks Circle" http://127.0.0.1:8105/health "d.get('status')=='ok' and d.get('db')"
