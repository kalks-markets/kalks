#!/usr/bin/env bash
# Deploy the website (repo kalks-markets/kalks-website, cloned at ~/kalks-website). Run as `kalks` on the VPS.
set -euo pipefail
cd ~/kalks-website
git pull --ff-only
npm ci --no-audit --no-fund
cd apps/trader
# the live market-news ticker reads the platform's news service server-side (same internal token as the apps)
f=.env.production.local; touch "$f"
grep -q '^NEWS_URL=' "$f" || printf 'NEWS_URL=http://127.0.0.1:8103\n' >> "$f"
grep -q '^NEWS_INTERNAL_TOKEN=' "$f" || printf 'NEWS_INTERNAL_TOKEN=%s\n' "$(grep '^NEWS_INTERNAL_TOKEN=' ~/kalks/.env.local | cut -d= -f2-)" >> "$f"
# editable copy and pictures from the content service's public read (no token: published content only); media URLs
# point at the API host (CONTENT_PUBLIC_URL in ~/kalks/.env.local)
grep -q '^CONTENT_URL=' "$f" || printf 'CONTENT_URL=http://127.0.0.1:8105\n' >> "$f"
grep -q '^CONTENT_TENANT=' "$f" || printf 'CONTENT_TENANT=kalks\n' >> "$f"
npm ci --no-audit --no-fund
NODE_OPTIONS=--max-old-space-size=6144 npm run build
sudo cp ~/kalks/deploy/systemd/kalks-website.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable kalks-website >/dev/null
sudo systemctl restart kalks-website
sleep 5
printf "website %s\n" "$(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:3010/)"
