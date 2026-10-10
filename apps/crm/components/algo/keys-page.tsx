"use client";

// Live API keys (/developer): per trading account keys with read / trade scopes, IP whitelist and expiry (D78),
// usage, activity, and the quickstart for the REST API.

import * as React from "react";
import Link from "next/link";
import { Activity, AlertCircle, BookOpen, Gauge as GaugeIcon, KeyRound, Loader2, Plus, ShieldCheck } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, CardHeader, Chip, CopyButton, DataTable, Dialog, KpiCard, MiniBars, PageHeader, Reveal, type Column } from "@/components/kit";
import { Trans, useFormat, useT } from "@kalks/i18n/react";
import { algoApi, algoError, ago, fmtDateTime, useAlgo, type TradingAccount } from "./api";
import { PageHero } from "@/components/page-hero";

interface Key {
  id: number;
  name: string;
  keyId: string;
  login: number;
  accountType: string;
  scopes: string[];
  ipWhitelist: string[];
  ratePerMin: number | null;
  expiresAt: string | null;
  status: "active" | "revoked" | "expired";
  createdAt: string;
  lastUsedAt: string | null;
  lastIp: string | null;
}
interface KeysResp {
  items: Key[];
  usage: { requests24h: number; errors24h: number; rateLimited24h: number; p50: number; p99: number; writes24h: number; hourly: { t: string; n: number }[] };
  baseUrl: string;
}

function CreateKeyDialog({ open, onOpenChange, accounts, onCreated }: { open: boolean; onOpenChange: (o: boolean) => void; accounts: TradingAccount[]; onCreated: (k: { keyId: string; secret: string; name: string }) => void }) {
  const t = useT();
  const [name, setName] = React.useState(() => t("developer.keys.defaultName"));
  const [login, setLogin] = React.useState<number | null>(null);
  const [trade, setTrade] = React.useState(true);
  const [ips, setIps] = React.useState("");
  const [days, setDays] = React.useState(90);
  const [busy, setBusy] = React.useState(false);
  React.useEffect(() => {
    if (open && login === null && accounts.length) setLogin((accounts.find((a) => a.type === "demo") ?? accounts[0]!).login);
  }, [open, accounts, login]);
  const acct = accounts.find((a) => a.login === login);
  const create = async () => {
    setBusy(true);
    try {
      const r = await algoApi<{ keyId: string; secret: string; name: string }>("keys", {
        body: { name, login, scopes: trade ? ["read", "trade"] : ["read"], ipWhitelist: ips.split(/[\s,]+/).filter(Boolean), expiresInDays: days || undefined },
      });
      onCreated(r);
      onOpenChange(false);
    } catch (e) {
      algoError(t("developer.keys.createFailed"), e);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={t("developer.keys.create")}
      description={t("developer.keys.createText")}
      width={560}
      footer={
        <>
          <Button variant="surface" onClick={() => onOpenChange(false)}>
            {t("common.cancel")}
          </Button>
          <Button variant="ember" disabled={busy || !login} onClick={create}>
            {busy ? <Loader2 className="animate-spin" /> : <KeyRound />} {t("developer.keys.createKey")}
          </Button>
        </>
      }
    >
      <div className="space-y-4 text-[13px]">
        <label className="block">
          <span className="text-fg-3">{t("common.name")}</span>
          <input value={name} onChange={(e) => setName(e.target.value)} className="mt-1 h-10 w-full rounded-[12px] border border-line bg-surface-2 px-3 text-fg outline-none focus:border-ember/50" />
        </label>
        <div>
          <div className="text-fg-3">{t("developer.tradingAccount")}</div>
          <div className="mt-1 flex flex-wrap gap-1.5">
            {accounts.map((a) => (
              <button key={a.login} type="button" onClick={() => setLogin(a.login)} className={`h-8 rounded-full border px-3 text-[12px] ${login === a.login ? "border-ember/40 bg-ember-soft text-ember" : "border-line text-fg-2"}`}>
                {a.type === "live" ? t("common.live") : t("common.demo")} #{a.login}
              </button>
            ))}
          </div>
        </div>
        <div>
          <div className="text-fg-3">{t("developer.keys.scopes")}</div>
          <div className="mt-1 flex gap-2">
            <Chip tone="up">{t("developer.scope.read")}</Chip>
            <button type="button" onClick={() => setTrade(!trade)} aria-pressed={trade}>
              <Chip tone={trade ? "ember" : "neutral"}>{trade ? t("developer.scope.trade") : `+ ${t("developer.scope.trade")}`}</Chip>
            </button>
          </div>
        </div>
        <label className="block">
          <span className="text-fg-3">{t("developer.keys.ipWhitelist")} {acct?.type === "live" && trade ? <span className="text-warn">{t("developer.keys.ipRequired")}</span> : t("developer.keys.ipOptional")}</span>
          <textarea value={ips} onChange={(e) => setIps(e.target.value)} rows={2} dir="ltr" placeholder="203.0.113.10, 198.51.100.0/24" className="mt-1 w-full rounded-[12px] border border-line bg-surface-2 px-3 py-2 font-mono text-[12.5px] text-fg outline-none focus:border-ember/50" />
        </label>
        <div>
          <div className="text-fg-3">{t("developer.keys.expires")}</div>
          <div className="mt-1 flex flex-wrap gap-1.5">
            {[
              [30, t("developer.keys.days30")],
              [90, t("developer.keys.days90")],
              [365, t("developer.keys.year1")],
              [0, t("developer.keys.never")],
            ].map(([d, l]) => (
              <button key={l} type="button" onClick={() => setDays(d as number)} className={`h-8 rounded-full border px-3 text-[12px] ${days === d ? "border-ember/40 bg-ember-soft text-ember" : "border-line text-fg-2"}`}>
                {l}
              </button>
            ))}
          </div>
        </div>
      </div>
    </Dialog>
  );
}

function Activity24({ keyId, open, onOpenChange }: { keyId: number | null; open: boolean; onOpenChange: (o: boolean) => void }) {
  const t = useT();
  const a = useAlgo<{ items: { at: string; method: string; path: string; status: number; ip: string | null; ms: number }[] }>(open && keyId ? `keys/${keyId}/activity` : null);
  return (
    <Dialog open={open} onOpenChange={onOpenChange} title={t("developer.keys.activityTitle")} description={t("developer.keys.activityText")} side="right">
      <div className="space-y-1 font-mono text-[11.5px]" dir="ltr">
        {(a.data?.items ?? []).map((r, i) => (
          <div key={i} className="flex gap-3 border-b border-line/60 py-1.5">
            <span className="text-fg-3">{fmtDateTime(r.at)}</span>
            <span className={r.status >= 400 ? "text-down" : "text-up"}>{r.status}</span>
            <span className="text-fg-2">{r.method}</span>
            <span className="min-w-0 flex-1 truncate text-fg">{r.path}</span>
            <span className="text-fg-3">{r.ms}ms</span>
          </div>
        ))}
        {a.data && a.data.items.length === 0 && <div className="py-8 text-center text-fg-3" dir="auto">{t("developer.keys.noRequests")}</div>}
      </div>
    </Dialog>
  );
}

export function LiveKeysPage() {
  const t = useT();
  const f = useFormat();
  const keys = useAlgo<KeysResp>("keys", 10000);
  const accounts = useAlgo<{ items: TradingAccount[] }>("accounts");
  const [creating, setCreating] = React.useState(false);
  const [secret, setSecret] = React.useState<{ keyId: string; secret: string; name: string } | null>(null);
  const [activity, setActivity] = React.useState<number | null>(null);
  const items = keys.data?.items ?? [];
  const u = keys.data?.usage;
  const base = keys.data?.baseUrl ?? "";
  const active = items.filter((k) => k.status === "active").length;

  const revoke = async (k: Key) => {
    if (!window.confirm(t("developer.keys.revokeConfirm", { name: k.name }))) return;
    try {
      await algoApi(`keys/${k.id}/revoke`, { body: {} });
      toast.success(t("developer.keys.revoked"));
      keys.reload();
    } catch (e) {
      algoError(t("developer.keys.revokeFailed"), e);
    }
  };

  const cols: Column<Key>[] = [
    { key: "name", header: t("developer.keys.colKey"), cell: (k) => <div><div className="font-medium text-fg">{k.name}</div><div className="font-mono text-[11.5px] text-fg-3">{k.keyId}</div></div>, sort: (k) => k.name },
    { key: "acct", header: t("common.account"), cell: (k) => <span className="font-mono text-fg-2">{k.accountType === "live" ? t("common.live") : t("common.demo")} #{k.login}</span>, sort: (k) => k.login },
    { key: "scopes", header: t("developer.keys.scopes"), cell: (k) => <div className="flex gap-1">{k.scopes.map((s) => <Chip key={s} size="sm" tone={s === "trade" ? "ember" : "up"}>{t.dyn(`developer.scope.${s}`, s)}</Chip>)}</div> },
    { key: "ips", header: t("developer.keys.ipWhitelist"), cell: (k) => <span className="text-[12px] text-fg-3">{k.ipWhitelist.length ? <span dir="ltr">{k.ipWhitelist.join(", ")}</span> : t("developer.keys.anyIp")}</span>, hideOn: "lg" },
    { key: "exp", header: t("developer.keys.expires"), cell: (k) => <span className="text-[12px] text-fg-3">{k.expiresAt ? fmtDateTime(k.expiresAt).slice(0, 10) : t("developer.ago.never")}</span>, hideOn: "md" },
    { key: "used", header: t("developer.keys.lastUsed"), cell: (k) => <span className="text-[12px] text-fg-3">{ago(k.lastUsedAt)}{k.lastIp ? ` · ${k.lastIp}` : ""}</span>, sort: (k) => k.lastUsedAt ?? "" },
    { key: "status", header: t("common.status"), cell: (k) => <Chip size="sm" tone={k.status === "active" ? "up" : "neutral"} dot={k.status === "active"}>{t.dyn(`developer.keyStatus.${k.status}`, k.status)}</Chip>, sort: (k) => k.status },
    {
      key: "act",
      header: "",
      align: "right",
      cell: (k) => (
        <div className="flex justify-end gap-1.5">
          <Button size="xs" variant="ghost" onClick={() => setActivity(k.id)}>
            {t("developer.keys.activity")}
          </Button>
          {k.status === "active" && (
            <Button size="xs" variant="down-outline" onClick={() => revoke(k)}>
              {t("developer.keys.revoke")}
            </Button>
          )}
        </div>
      ),
    },
  ];

  const curl = secret
    ? `curl ${base}/account \\\n  -H "Authorization: Bearer ${secret.keyId}:${secret.secret}"\n\ncurl -X POST ${base}/orders \\\n  -H "Authorization: Bearer ${secret.keyId}:${secret.secret}" \\\n  -H "content-type: application/json" \\\n  -d '{"symbol":"EURUSD","side":"buy","volume":0.01,"sl":null}'`
    : "";

  return (
    <div className="pb-24">
      <PageHero page="options"
        title={t("developer.keys.pageTitle")}
        lead={t("developer.keys.pageSubtitle")}
        actions={
          <>
            <Link href="/developer/docs">
              <Button variant="surface" size="lg">
                <BookOpen /> {t("developer.docs.title")}
              </Button>
            </Link>
            <Button variant="ember" size="lg" onClick={() => setCreating(true)} disabled={!accounts.data?.items.length}>
              <Plus /> {t("developer.keys.create")}
            </Button>
          </>
        }
      />
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <KpiCard label={t("developer.keys.requests24h")} icon={<Activity />} value={<span className="k-num">{f.number(u?.requests24h ?? 0, 0)}</span>} footer={<div className="flex w-full items-center justify-between gap-3"><Chip>{t("developer.keys.ordersCloses", { n: u?.writes24h ?? 0 })}</Chip><MiniBars data={(u?.hourly ?? []).map((h) => h.n)} className="h-6" /></div>} />
        <KpiCard label={t("developer.keys.errors24h")} icon={<AlertCircle />} value={<span className="k-num">{u?.errors24h ?? 0}</span>} chip={t("developer.keys.rateLimited", { n: u?.rateLimited24h ?? 0 })} delay={0.05} />
        <KpiCard label={t("developer.keys.latencyP50")} icon={<GaugeIcon />} value={<span className="k-num">{Math.round(u?.p50 ?? 0)}<span className="text-[20px] text-fg-3"> {t("developer.unit.ms")}</span></span>} chip={`p99 ${Math.round(u?.p99 ?? 0)} ${t("developer.unit.ms")}`} delay={0.1} />
        <KpiCard label={t("developer.keys.activeKeys")} icon={<KeyRound />} hot value={<span className="k-num">{active}<span className="text-fg-3">/{items.length}</span></span>} chip={t("developer.keys.max", { n: 20 })} delay={0.15} />
      </div>
      {secret && (
        <Reveal className="mt-4">
          <Card className="border-ember/40">
            <CardHeader icon={<ShieldCheck />} title={t("developer.keys.created", { name: secret.name })} subtitle={t("developer.keys.createdText")} />
            <div className="space-y-3 px-6 pb-5 pt-4">
              <div className="grid grid-cols-1 gap-2 md:grid-cols-2">
                <div className="flex items-center gap-2 rounded-[12px] bg-black/30 light:bg-surface-2 px-3 py-2">
                  <span className="text-[12px] text-fg-3">{t("developer.keys.keyId")}</span>
                  <code className="min-w-0 flex-1 truncate font-mono text-[12.5px] text-fg" dir="ltr" data-testid="key-id">{secret.keyId}</code>
                  <CopyButton value={secret.keyId} label={t("developer.keys.keyId")} />
                </div>
                <div className="flex items-center gap-2 rounded-[12px] bg-black/30 light:bg-surface-2 px-3 py-2">
                  <span className="text-[12px] text-fg-3">{t("developer.keys.secret")}</span>
                  <code className="min-w-0 flex-1 truncate font-mono text-[12.5px] text-ember" dir="ltr" data-testid="key-secret">{secret.secret}</code>
                  <CopyButton value={secret.secret} label={t("developer.keys.secret")} />
                </div>
              </div>
              <pre dir="ltr" className="overflow-x-auto rounded-[12px] bg-black/30 light:bg-surface-2 p-3 font-mono text-[11.5px] leading-[17px] text-fg-2">{curl}</pre>
              <Button size="sm" variant="surface" onClick={() => setSecret(null)}>
                {t("developer.keys.stored")}
              </Button>
            </div>
          </Card>
        </Reveal>
      )}
      <Reveal delay={0.05} className="mt-4">
        <Card>
          <CardHeader title={t("developer.keys.title")} subtitle={t("developer.keys.subtitle")} />
          <div className="px-6 pb-6 pt-4">
            <DataTable columns={cols} rows={items} pageSize={10} rowKey={(k) => String(k.id)} empty={<div className="py-8 text-center text-fg-3">{t("developer.keys.none")}</div>} />
          </div>
        </Card>
      </Reveal>
      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-2">
        <Card>
          <CardHeader title={t("developer.keys.quickstart")} subtitle={base} />
          <pre dir="ltr" className="mx-6 mb-6 mt-4 overflow-x-auto rounded-[12px] bg-black/30 light:bg-surface-2 p-3 font-mono text-[11.5px] leading-[17px] text-fg-2">{`# read the account
curl ${base}/account -H "Authorization: Bearer $KEY_ID:$SECRET"

# market order with stop and target (scope: trade)
curl -X POST ${base}/orders -H "Authorization: Bearer $KEY_ID:$SECRET" \\
  -H "content-type: application/json" \\
  -d '{"symbol":"XAUUSD","side":"sell","volume":0.1,"sl":2710,"tp":2650}'

# close a position
curl -X POST ${base}/positions/1000123/close -H "Authorization: Bearer $KEY_ID:$SECRET"`}</pre>
        </Card>
        <Card>
          <CardHeader title={t("developer.keys.safety")} subtitle={t("developer.keys.safetySub")} />
          <div className="space-y-2 px-6 pb-6 pt-4 text-[13px] text-fg-2">
            {[
              t("developer.keys.safety1"),
              t("developer.keys.safety2"),
              t("developer.keys.safety3"),
              t("developer.keys.safety4"),
              t("developer.keys.safety5"),
            ].map((line) => (
              <div key={line} className="flex gap-2">
                <ShieldCheck className="mt-0.5 size-4 shrink-0 text-up" /> {line}
              </div>
            ))}
            <p className="pt-2 text-[12.5px] text-fg-3">
              <Trans k="developer.keys.killHint" tags={{ link: (c) => <Link href="/developer/deployments" className="text-ember hover:underline">{c}</Link> }} />
            </p>
          </div>
        </Card>
      </div>
      <CreateKeyDialog open={creating} onOpenChange={setCreating} accounts={(accounts.data?.items ?? []).filter((a) => a.status === "active")} onCreated={(k) => (setSecret(k), keys.reload())} />
      <Activity24 keyId={activity} open={activity !== null} onOpenChange={(o) => !o && setActivity(null)} />
    </div>
  );
}
