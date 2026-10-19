"use client";

import * as React from "react";
import Link from "next/link";
import { toast } from "sonner";
import { AlertOctagon, ArrowUpRight, Cable, Download, Gauge, KeyRound, Landmark, ListOrdered, Radio, ShieldCheck, Webhook, Layers } from "lucide-react";
import { Button, Card, Chip, CopyButton, PageHeader, Reveal, cn } from "@/components/kit";
import { API_BASE, API_ENDPOINTS, API_ERRORS, FIX_SESSION, RATE_LIMITS } from "@kalks/mock/developer";
import { CodeBlock, toJson } from "@/components/developer/code-block";
import { DocSection, DocTable, DocsLangContext, DocsNav, EndpointCard, MethodBadge, SampleBlock, useScrollSpy, type SampleLang } from "@/components/developer/docs";
import { IS_DEMO as DEMO_BUILD } from "@kalks/mock/mode";
import { LiveDocsPage } from "@/components/algo/docs-page";

const NAV = [
  { id: "authentication", label: "Authentication", icon: <KeyRound /> },
  { id: "accounts", label: "Accounts", icon: <Landmark />, count: API_ENDPOINTS.accounts.length },
  { id: "orders", label: "Orders", icon: <ListOrdered />, count: API_ENDPOINTS.orders.length },
  { id: "positions", label: "Positions", icon: <Layers />, count: API_ENDPOINTS.positions.length },
  { id: "market-data", label: "Market data WS", icon: <Radio /> },
  { id: "webhooks", label: "Webhooks", icon: <Webhook /> },
  { id: "fix", label: "FIX 4.4", icon: <Cable /> },
  { id: "rate-limits", label: "Rate limits", icon: <Gauge /> },
  { id: "errors", label: "Errors", icon: <AlertOctagon /> },
];
const IDS = NAV.map((n) => n.id);

const AUTH_SAMPLE: Record<SampleLang, string> = {
  curl: `# Signature = HMAC_SHA256(secret, timestamp + method + path + body)
TS=$(date +%s%3N)
SIG=$(printf "%s" "$TS""GET""/v1/accounts" \\
  | openssl dgst -sha256 -hmac "$KALKS_SECRET" | cut -d' ' -f2)

curl "${API_BASE}/accounts" \\
  -H "X-KALKS-KEY: kk_live_EXAMPLE1" \\
  -H "X-KALKS-TIMESTAMP: $TS" \\
  -H "X-KALKS-SIGNATURE: $SIG"`,
  python: `import hmac, hashlib, time, requests

KEY, SECRET = "kk_live_EXAMPLE1", os.environ["KALKS_SECRET"]

def sign(method, path, body=""):
    ts = str(int(time.time() * 1000))
    msg = ts + method + path + body
    sig = hmac.new(SECRET.encode(), msg.encode(), hashlib.sha256).hexdigest()
    return {"X-KALKS-KEY": KEY, "X-KALKS-TIMESTAMP": ts, "X-KALKS-SIGNATURE": sig}

r = requests.get("${API_BASE}/accounts", headers=sign("GET", "/v1/accounts"))
print(r.json())  # the SDK does all of this for you`,
  js: `import { createHmac } from "node:crypto";

const KEY = "kk_live_EXAMPLE1";
const SECRET = process.env.KALKS_SECRET;

function sign(method, path, body = "") {
  const ts = Date.now().toString();
  const sig = createHmac("sha256", SECRET).update(ts + method + path + body).digest("hex");
  return { "X-KALKS-KEY": KEY, "X-KALKS-TIMESTAMP": ts, "X-KALKS-SIGNATURE": sig };
}

const res = await fetch("${API_BASE}/accounts", { headers: sign("GET", "/v1/accounts") });
console.log(await res.json()); // the SDK does all of this for you`,
};

const WS_SAMPLE: Record<SampleLang, string> = {
  curl: `wscat -c "wss://api.kalkstrade.com/v1/stream?key=kk_live_EXAMPLE1&ts=1790261892184&sig=$SIG"

> {"op": "subscribe", "channels": ["quotes"], "symbols": ["XAUUSD", "EURUSD"]}
> {"op": "subscribe", "channels": ["positions", "orders"], "login": "80412337"}`,
  python: `from kalks import Client

client = Client(key="kk_live_EXAMPLE1", secret=os.environ["KALKS_SECRET"])

async for msg in client.ws.subscribe(
    channels=["quotes", "positions"],
    symbols=["XAUUSD", "EURUSD"],
    login="80412337",
):
    print(msg.channel, msg.data)`,
  js: `import { Kalks } from "@kalks/sdk";

const kalks = new Kalks({ key: "kk_live_EXAMPLE1", secret: process.env.KALKS_SECRET });
const ws = kalks.ws();

ws.subscribe({ channels: ["quotes"], symbols: ["XAUUSD", "EURUSD"] });
ws.subscribe({ channels: ["positions", "orders"], login: "80412337" });

ws.on("quote", (q) => console.log(q.symbol, q.bid, q.ask));`,
};

const WS_STREAM = `{"channel":"subscribed","symbols":["XAUUSD","EURUSD"],"id":1}
{"channel":"quote","symbol":"XAUUSD","bid":2654.30,"ask":2654.48,"ts":1790261892301}
{"channel":"quote","symbol":"EURUSD","bid":1.08456,"ask":1.08464,"ts":1790261892318}
{"channel":"quote","symbol":"XAUUSD","bid":2654.36,"ask":2654.54,"ts":1790261892402}
{"channel":"position","login":"80412337","ticket":51298844,"profit":142.60,"source":"api"}
{"channel":"order","id":"ord_8f22a1","status":"filled","price":1.08461,"source":"webhook"}
{"channel":"heartbeat","ts":1790261897000}`;

const WEBHOOK_VERIFY: Record<SampleLang, string> = {
  curl: `curl -X POST "https://api.kalkstrade.com/algo/hooks/wh_9f3a1c7e2b" \\
  -H "Content-Type: application/json" \\
  -d '{"secret":"whsec_EXAMPLE0…","action":"buy","symbol":"XAUUSD","volume":1.0,"sl":2638.0,"tp":2690.0,"comment":"gold-bo"}'`,
  python: `import requests

requests.post(
    "https://api.kalkstrade.com/algo/hooks/wh_9f3a1c7e2b",
    json={"secret": "whsec_EXAMPLE0…", "action": "buy", "symbol": "XAUUSD",
          "volume": 1.0, "sl": 2638.0, "tp": 2690.0, "comment": "gold-bo"},
    timeout=5,
)`,
  js: `await fetch("https://api.kalkstrade.com/algo/hooks/wh_9f3a1c7e2b", {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({ secret: "whsec_EXAMPLE0…", action: "buy", symbol: "XAUUSD",
    volume: 1.0, sl: 2638.0, tp: 2690.0, comment: "gold-bo" }),
});`,
};

const FIX_LOGON = `# Logon (35=A) — password = API secret
8=FIX.4.4|9=126|35=A|49=${FIX_SESSION.senderCompId}|56=${FIX_SESSION.targetCompId}|34=1|52=20260924-14:58:12.184|98=0|108=${FIX_SESSION.heartbeat}|141=Y|553=kk_live_EXAMPLE1|554=••••••|10=087|
# New Order Single (35=D) — buy 0.50 lot XAUUSD at market
8=FIX.4.4|9=152|35=D|49=${FIX_SESSION.senderCompId}|56=${FIX_SESSION.targetCompId}|34=2|11=gold-bo-0924-01|1=80412337|55=XAUUSD|54=1|38=50|40=1|59=3|60=20260924-14:58:12.201|10=164|
# Execution Report (35=8) — filled
8=FIX.4.4|9=198|35=8|49=${FIX_SESSION.targetCompId}|56=${FIX_SESSION.senderCompId}|34=2|37=51298844|11=gold-bo-0924-01|17=E0924-88121|150=F|39=2|55=XAUUSD|54=1|38=50|32=50|31=2654.48|14=50|6=2654.48|10=231|`;

function DemoDocsPage() {
  const [lang, setLang] = React.useState<SampleLang>("curl");
  const active = useScrollSpy(IDS);
  const ctx = React.useMemo(() => ({ lang, setLang }), [lang]);

  return (
    <DocsLangContext.Provider value={ctx}>
      <div className="pb-24">
        <PageHeader
          title="API reference"
          subtitle={
            <span>
              REST + WebSocket core, FIX 4.4 gateway and signal webhooks · v1 ·{" "}
              <span className="font-mono text-fg-2">{API_BASE}</span>
            </span>
          }
          actions={
            <>
              <Button variant="surface" size="lg" onClick={() => toast.success("openapi.yaml downloaded", { description: "Kalks API v1 · OpenAPI 3.1" })}>
                <Download /> OpenAPI spec
              </Button>
              <Link href="/developer">
                <Button variant="ember" size="lg" shimmer>
                  <KeyRound /> Get API key
                </Button>
              </Link>
            </>
          }
        />

        <Reveal>
          <div className="mb-6 grid grid-cols-2 gap-3 lg:grid-cols-4">
            {[
              { k: "REST", v: API_BASE.replace("https://", ""), sub: "JSON · HMAC-SHA256" },
              { k: "WebSocket", v: "api.kalkstrade.com/v1/stream", sub: "Quotes, orders, positions" },
              { k: "FIX 4.4", v: `${FIX_SESSION.host}:${FIX_SESSION.port}`, sub: "TLS 1.3 · LD4" },
              { k: "Webhooks", v: "api.kalkstrade.com/algo/hooks", sub: "TradingView-ready" },
            ].map((x) => (
              <div key={x.k} className="k-card flex min-w-0 items-center gap-3 px-4 py-3.5">
                <div className="min-w-0 flex-1">
                  <div className="k-label">{x.k}</div>
                  <div className="mt-1 truncate font-mono text-[12.5px] text-fg">{x.v}</div>
                  <div className="mt-0.5 truncate text-[11px] text-fg-3">{x.sub}</div>
                </div>
                <CopyButton value={x.v} label={`${x.k} endpoint`} className="hidden sm:inline-grid" />
              </div>
            ))}
          </div>
        </Reveal>

        <div className="grid grid-cols-1 gap-8 lg:grid-cols-[220px_minmax(0,1fr)]">
          <div className="lg:self-stretch">
            <DocsNav items={NAV} active={active} />
          </div>

          <div className="min-w-0 space-y-14">
            {/* Authentication */}
            <DocSection
              id="authentication"
              kicker="Getting started"
              title="Authentication"
              intro={
                <>
                  Every request is signed with your API key&apos;s secret. Keys are bound to one trading account, carry <span className="font-mono text-fg">read</span> and/or{" "}
                  <span className="font-mono text-fg">trade</span> scopes and are IP-whitelisted. There is no withdraw scope — funds can never leave via the API.
                </>
              }
            >
              <Card className="p-5">
                <DocTable
                  head={["Header", "Value", "Notes"]}
                  mono={[0, 1]}
                  rows={[
                    ["X-KALKS-KEY", "kk_live_EXAMPLE1", "Public key id from API keys page"],
                    ["X-KALKS-TIMESTAMP", "1790261892184", "Unix ms; rejected if more than 5s from server time"],
                    ["X-KALKS-SIGNATURE", "hex(HMAC_SHA256(secret, ts+method+path+body))", "Path includes /v1 and the query string"],
                  ]}
                />
                <div className="mt-4">
                  <SampleBlock code={AUTH_SAMPLE} title="Signing a request" />
                </div>
                <div className="mt-4 flex flex-wrap items-center gap-2 text-[12px] text-fg-3">
                  <ShieldCheck className="size-4 text-up" /> Order responses always include <span className="font-mono text-fg-2">source</span>:
                  {["manual", "api", "webhook", "strategy", "copy"].map((s) => (
                    <Chip key={s} size="sm" tone={s === "api" ? "ember" : "neutral"}>
                      {s}
                    </Chip>
                  ))}
                </div>
              </Card>
            </DocSection>

            <DocSection id="accounts" kicker="REST" title="Accounts" intro="Read balances, equity and margin for the account bound to your key.">
              {API_ENDPOINTS.accounts.map((e) => (
                <EndpointCard key={e.id} e={e} />
              ))}
            </DocSection>

            <DocSection id="orders" kicker="REST" title="Orders" intro="Requires the trade scope. Use client_id for idempotent retries — a duplicate returns 409 with the original order.">
              {API_ENDPOINTS.orders.map((e) => (
                <EndpointCard key={e.id} e={e} />
              ))}
            </DocSection>

            <DocSection id="positions" kicker="REST" title="Positions" intro="Open positions update on every tick. Closing requires the trade scope.">
              {API_ENDPOINTS.positions.map((e) => (
                <EndpointCard key={e.id} e={e} />
              ))}
            </DocSection>

            {/* WS */}
            <DocSection
              id="market-data"
              kicker="Streaming"
              title="Market data WebSocket"
              intro="One connection streams quotes plus your own orders and positions. Up to 200 symbols per connection, 5 connections per key. Heartbeat every 5s; reconnect with exponential backoff."
            >
              <Card className="overflow-hidden">
                <div className="flex flex-wrap items-center gap-3 border-b border-line bg-surface-2/40 px-5 py-3.5">
                  <MethodBadge method="WS" />
                  <span className="font-mono text-[13.5px]">wss://api.kalkstrade.com/v1/stream</span>
                  <CopyButton value="wss://api.kalkstrade.com/v1/stream" label="WebSocket URL" />
                  <div className="ml-auto flex gap-1.5">
                    {["quotes", "orders", "positions", "account"].map((c) => (
                      <Chip key={c} size="sm">
                        {c}
                      </Chip>
                    ))}
                  </div>
                </div>
                <div className="grid grid-cols-1 gap-3 p-5 xl:grid-cols-2">
                  <div>
                    <div className="k-label mb-2">Subscribe</div>
                    <SampleBlock code={WS_SAMPLE} title="subscribe" />
                  </div>
                  <div>
                    <div className="k-label mb-2 flex items-center gap-2">
                      Streamed messages <span className="size-1.5 animate-pulse rounded-full bg-up" />
                    </div>
                    <CodeBlock code={WS_STREAM} lang="json" title="← server · newline-delimited JSON" highlight={[5, 6]} />
                  </div>
                </div>
              </Card>
            </DocSection>

            {/* Webhooks */}
            <DocSection
              id="webhooks"
              kicker="Signals"
              title="Signal webhooks"
              intro={
                <>
                  POST a JSON signal to your webhook URL (from TradingView or any system). Kalks validates the secret, then fans the order out to every target account using its sizing rule — fixed lot,
                  multiplier or risk %. Orders are tagged <span className="font-mono text-fg">source=webhook</span>.
                </>
              }
            >
              <Card className="p-5">
                <div className="flex flex-wrap items-center gap-3">
                  <MethodBadge method="POST" />
                  <span className="font-mono text-[13.5px]">
                    https://api.kalkstrade.com/algo/hooks/<span className="text-gold">{"{webhook_id}"}</span>
                  </span>
                </div>
                <div className="mt-4">
                  <DocTable
                    head={["Field", "Type", "Description"]}
                    mono={[0, 1]}
                    rows={[
                      [<>secret <span className="text-[10px] uppercase text-ember">required</span></>, "string", "Webhook secret (or send X-Kalks-Signature header instead)"],
                      [<>action <span className="text-[10px] uppercase text-ember">required</span></>, "enum", "buy | sell | close | close_all"],
                      [<>symbol <span className="text-[10px] uppercase text-ember">required</span></>, "string", "Must be in the webhook's allowed symbols"],
                      ["volume", "number", "Signal volume; scaled per account by its sizing rule"],
                      ["sl / tp", "number", "Absolute prices; required for risk % sizing"],
                      ["comment", "string", "Dedup key within 10s, max 32 chars"],
                    ]}
                  />
                </div>
                <div className="mt-4 grid grid-cols-1 gap-3 xl:grid-cols-2">
                  <SampleBlock code={WEBHOOK_VERIFY} title="Send a signal" />
                  <CodeBlock
                    lang="json"
                    title="200 OK"
                    code={toJson({ status: "accepted", signal_id: "dlv_4a91c2", fanout: [{ login: "80412512", volume: 0.5, ticket: 51298861 }, { login: "80412337", volume: 0.16, ticket: 51298862 }], latency_ms: 42 })}
                  />
                </div>
                <Link href="/developer/webhooks" className="mt-4 inline-flex items-center gap-1.5 text-[13px] font-medium text-ember hover:underline">
                  Manage webhooks <ArrowUpRight className="size-3.5" />
                </Link>
              </Card>
            </DocSection>

            {/* FIX */}
            <DocSection id="fix" kicker="Institutional" title="FIX 4.4" intro="Low-latency order entry and market data for professional systems. Sessions are provisioned per trading account; the API key id and secret are used as Username (553) and Password (554).">
              <Card className="p-5">
                <div className="grid grid-cols-1 gap-2 sm:grid-cols-2 xl:grid-cols-4">
                  {[
                    ["Host", `${FIX_SESSION.host}:${FIX_SESSION.port}`],
                    ["SenderCompID (49)", FIX_SESSION.senderCompId],
                    ["TargetCompID (56)", FIX_SESSION.targetCompId],
                    ["BeginString (8)", FIX_SESSION.version],
                    ["HeartBtInt (108)", `${FIX_SESSION.heartbeat}s`],
                    ["Transport", FIX_SESSION.tls],
                    ["ResetSeqNumFlag (141)", FIX_SESSION.resetOnLogon],
                    ["Session hours", "Sun 23:05 – Fri 23:55 GMT+3"],
                  ].map(([k, v]) => (
                    <div key={k} className="k-row flex min-w-0 items-center gap-2 px-3.5 py-2.5">
                      <div className="min-w-0 flex-1">
                        <div className="truncate text-[11.5px] text-fg-3">{k}</div>
                        <div className="mt-0.5 truncate font-mono text-[12.5px]">{v}</div>
                      </div>
                      <CopyButton value={v!} label={k} />
                    </div>
                  ))}
                </div>
                <div className="mt-5">
                  <div className="k-label mb-2">Supported message types</div>
                  <div className="flex flex-wrap gap-1.5">
                    {FIX_SESSION.msgTypes.map(([t, n]) => (
                      <span key={t} className="inline-flex items-center gap-2 rounded-full border border-line bg-surface-2 py-1 pl-1 pr-3 text-[12px]">
                        <span className="grid h-5 min-w-7 place-items-center rounded-full bg-gold-soft px-1.5 font-mono text-[10.5px] font-semibold text-gold">{t}</span>
                        <span className="text-fg-2">{n}</span>
                      </span>
                    ))}
                  </div>
                </div>
                <div className="mt-5">
                  <CodeBlock code={FIX_LOGON} lang="fix" title="session.log · | = SOH (0x01)" wrap />
                </div>
                <div className="mt-4 flex flex-wrap gap-2">
                  <Button variant="surface" size="sm" onClick={() => toast.success("FIX session requested", { description: "Credentials for 80412337 will arrive in ~5 minutes." })}>
                    Request FIX session
                  </Button>
                  <Button variant="ghost" size="sm" onClick={() => toast.success("KALKS-FIX44.xml downloaded", { description: "QuickFIX data dictionary" })}>
                    <Download /> Data dictionary
                  </Button>
                </div>
              </Card>
            </DocSection>

            {/* Rate limits */}
            <DocSection id="rate-limits" kicker="Operations" title="Rate limits" intro="Limits are per key unless stated. Every response includes X-RateLimit-Limit, X-RateLimit-Remaining and X-RateLimit-Reset headers. Exceeding a limit returns 429 with Retry-After.">
              <Card className="p-5">
                <DocTable
                  head={["Scope", "Limit", "Burst", "Window"]}
                  mono={[1, 2, 3]}
                  rows={RATE_LIMITS.map((r) => [r.scope, r.limit, r.burst, r.window])}
                />
                <div className="mt-4 rounded-[14px] border border-info/25 bg-info-soft px-4 py-3 text-[12.5px] text-fg-2">
                  Need more? Pro and ECN accounts with $50k+ equity can request up to <span className="font-mono text-fg">100 req/s</span> and a co-located FIX session in LD4.
                </div>
              </Card>
            </DocSection>

            {/* Errors */}
            <DocSection id="errors" kicker="Operations" title="Errors" intro="Errors use standard HTTP codes and a stable machine-readable name.">
              <Card className="p-5">
                <DocTable
                  head={["HTTP", "Error", "Meaning"]}
                  mono={[0, 1]}
                  rows={API_ERRORS.map((e) => [
                    <span key="c" className={cn("font-semibold", e.code >= 500 ? "text-down" : e.code === 429 || e.code === 423 ? "text-warn" : "text-ember")}>
                      {e.code}
                    </span>,
                    e.name,
                    e.desc,
                  ])}
                />
                <div className="mt-4">
                  <CodeBlock
                    lang="json"
                    title="422 Unprocessable Entity"
                    code={toJson({ error: { code: 422, name: "invalid_order", message: "Volume 0.001 is below the minimum lot 0.01 for XAUUSD", field: "volume", request_id: "req_7f1c20a9e4" } })}
                  />
                </div>
              </Card>
            </DocSection>
          </div>
        </div>
      </div>
    </DocsLangContext.Provider>
  );
}

export default function DocsPage() {
  return DEMO_BUILD ? <DemoDocsPage /> : <React.Suspense fallback={null}><LiveDocsPage /></React.Suspense>;
}
