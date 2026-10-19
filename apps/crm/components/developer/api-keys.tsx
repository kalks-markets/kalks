"use client";

import * as React from "react";
import Link from "next/link";
import { AnimatePresence, motion } from "motion/react";
import { toast } from "sonner";
import {
  AlertTriangle,
  BookOpen,
  Check,
  Download,
  Globe,
  KeyRound,
  Lock,
  MoreHorizontal,
  OctagonX,
  Pencil,
  Play,
  Plus,
  RefreshCw,
  ShieldAlert,
  ShieldCheck,
  Trash2,
  X,
  Zap,
} from "lucide-react";
import {
  Button,
  Card,
  CardHeader,
  Chip,
  CopyButton,
  DataTable,
  Dialog,
  Donut,
  EquityChart,
  Field,
  Icon3D,
  IconButton,
  Input,
  Menu,
  Segmented,
  Toggle,
  Tooltip,
  cn,
  formatCompact,
  formatNumber,
  type Column,
} from "@/components/kit";
import { ACCOUNTS } from "@kalks/mock";
import { API_STATS, ENDPOINT_USAGE, ORDER_SOURCES, apiUsage, type ApiKey, type ApiScope } from "@kalks/mock/developer";
import { CodeBlock, type CodeLang } from "./code-block";

/** Fixed "now" for mock data so SSR and client agree. */
export const NOW = Date.parse("2026-09-24T15:00:00Z");

export function ago(iso: string | null) {
  if (!iso) return "Never";
  const m = Math.max(0, Math.round((NOW - Date.parse(iso)) / 60000));
  if (m < 1) return "just now";
  if (m < 60) return `${m}m ago`;
  const h = Math.round(m / 60);
  if (h < 24) return `${h}h ago`;
  const d = Math.round(h / 24);
  if (d < 45) return `${d}d ago`;
  return `${Math.round(d / 30)}mo ago`;
}

export function shortDate(iso: string) {
  return new Date(iso).toLocaleDateString("en-GB", { day: "2-digit", month: "short", year: "numeric", timeZone: "Europe/Istanbul" });
}

function accountOf(login: string) {
  return ACCOUNTS.find((a) => a.login === login);
}

export function AccountTag({ login, className }: { login: string; className?: string }) {
  const a = accountOf(login);
  return (
    <span className={cn("inline-flex items-center gap-2", className)}>
      <span className="font-mono text-[12.5px] text-fg">{login}</span>
      {a && (
        <Chip size="sm" tone={a.type === "live" ? "ember" : "gold"}>
          {a.type === "live" ? a.group : `Demo ${a.group}`}
        </Chip>
      )}
    </span>
  );
}

export function ScopeChips({ scopes }: { scopes: ApiScope[] }) {
  return (
    <span className="inline-flex flex-nowrap gap-1">
      <Chip size="sm" tone={scopes.includes("read") ? "info" : "neutral"}>
        read
      </Chip>
      {scopes.includes("trade") && (
        <Chip size="sm" tone="ember">
          trade
        </Chip>
      )}
    </span>
  );
}

/* ------------------------------------------------------------------ */
/* Keys table                                                          */
/* ------------------------------------------------------------------ */

type Filter = "all" | "active" | "paused" | "expired";

export function KeysTable({
  keys,
  setKeys,
  halted,
  onCreate,
}: {
  keys: ApiKey[];
  setKeys: React.Dispatch<React.SetStateAction<ApiKey[]>>;
  halted: boolean;
  onCreate: () => void;
}) {
  const [filter, setFilter] = React.useState<Filter>("all");
  const rows = filter === "all" ? keys : keys.filter((k) => k.status === filter);
  const count = (f: Filter) => (f === "all" ? keys.length : keys.filter((k) => k.status === f).length);

  const toggleKey = React.useCallback(
    (k: ApiKey, on: boolean) => {
      setKeys((ks) => ks.map((x) => (x.id === k.id ? { ...x, status: on ? "active" : "paused" } : x)));
      if (on) toast.success(`${k.name} re-enabled`, { description: `Key ${k.prefix}… can place requests again.` });
      else toast.warning(`Kill switch: ${k.name}`, { description: "Key paused instantly. Pending API orders on this key were cancelled." });
    },
    [setKeys],
  );

  const columns: Column<ApiKey>[] = [
    {
      key: "name",
      header: "Key",
      sort: (k) => k.name,
      cell: (k) => (
        <div className="flex min-w-[190px] max-w-[230px] items-center gap-3">
          <span className={cn("grid size-9 shrink-0 place-items-center rounded-full border", k.status === "active" && !halted ? "border-up/25 bg-up-soft text-up" : k.status === "paused" || halted ? "border-warn/25 bg-warn-soft text-warn" : "border-line bg-surface-3 text-fg-3")}>
            <KeyRound className="size-4" />
          </span>
          <div className="min-w-0">
            <div className="truncate font-medium">{k.name}</div>
            <div className="mt-0.5 flex items-center gap-1 font-mono text-[11px] text-fg-3">
              {k.prefix}••••
              <CopyButton value={k.prefix} label="Key id" className="size-5" />
            </div>
          </div>
        </div>
      ),
    },
    { key: "acct", header: "Account", sort: (k) => k.login, cell: (k) => {
        const a = accountOf(k.login);
        return (
          <div className="whitespace-nowrap">
            <div className="font-mono text-[12.5px]">{k.login}</div>
            <div className={cn("text-[10.5px]", a?.type === "live" ? "text-ember" : "text-gold")}>{a ? `${a.type === "live" ? "Live" : "Demo"} · ${a.group}` : "—"}</div>
          </div>
        );
      } },
    { key: "scopes", header: "Scopes", cell: (k) => <ScopeChips scopes={k.scopes} /> },
    {
      key: "ips",
      header: "IP whitelist",
      hideOn: "md",
      cell: (k) =>
        k.ips.length ? (
          <Tooltip content={<span className="font-mono">{k.ips.join(", ")}</span>}>
            <span className="inline-flex items-center gap-1.5 font-mono text-[12px] text-fg-2">
              {k.ips[0]}
              {k.ips.length > 1 && <Chip size="sm">+{k.ips.length - 1}</Chip>}
            </span>
          </Tooltip>
        ) : (
          <Chip size="sm" tone="warn">
            <Globe className="size-3" /> Any IP
          </Chip>
        ),
    },
    { key: "rate", header: "Limit", align: "right", sort: (k) => k.rateLimit, cell: (k) => <span className="k-num whitespace-nowrap text-fg-2"><span className="text-fg">{k.rateLimit}</span> req/s</span> },
    {
      key: "used",
      header: "Last used",
      sort: (k) => k.lastUsed ?? "",
      cell: (k) => (
        <div className="whitespace-nowrap">
          <div className="k-num text-fg-2">{ago(k.lastUsed)}</div>
          {k.lastIp && <div className="font-mono text-[10.5px] text-fg-3">{k.lastIp}</div>}
        </div>
      ),
    },
    {
      key: "exp",
      header: "Expiry",
      sort: (k) => k.expiresAt ?? "9999",
      cell: (k) => {
        if (!k.expiresAt) return <span className="text-fg-3">No expiry</span>;
        const days = Math.round((Date.parse(k.expiresAt) - NOW) / 86400000);
        return (
          <div className="whitespace-nowrap">
            <div className="k-num text-fg-2">{shortDate(k.expiresAt)}</div>
            <div className={cn("k-num text-[10.5px]", days < 0 ? "text-down" : days < 30 ? "text-warn" : "text-fg-3")}>{days < 0 ? `${-days}d ago` : `in ${days}d`}</div>
          </div>
        );
      },
    },
    {
      key: "status",
      header: "Status · kill switch",
      cell: (k) => (
        <div className="flex items-center justify-between gap-3" onClick={(e) => e.stopPropagation()}>
          {halted && k.status === "active" ? (
            <Chip tone="down" dot>
              Halted
            </Chip>
          ) : k.status === "active" ? (
            <Chip tone="up" dot>
              Active
            </Chip>
          ) : k.status === "paused" ? (
            <Chip tone="warn" dot>
              Paused
            </Chip>
          ) : (
            <Chip dot>Expired</Chip>
          )}
          {k.status === "expired" ? (
            <Tooltip content="Expired keys can't be re-enabled — rotate instead">
              <span className="grid w-10 place-items-center">
                <Lock className="size-4 text-fg-3" />
              </span>
            </Tooltip>
          ) : (
            <Toggle checked={k.status === "active"} onChange={(v) => toggleKey(k, v)} label={`Enable ${k.name}`} />
          )}
        </div>
      ),
    },
    {
      key: "menu",
      header: "",
      align: "right",
      width: "48px",
      cell: (k) => (
        <Menu
          width={200}
          trigger={
            <IconButton size="sm" aria-label="Key actions">
              <MoreHorizontal />
            </IconButton>
          }
          items={[
            { label: "Edit scopes & IPs", icon: <Pencil />, onSelect: () => toast.info(`Editing ${k.name}`, { description: "Scope and IP changes apply within 5 seconds." }) },
            { label: "Rotate secret", icon: <RefreshCw />, onSelect: () => toast.success("Secret rotated", { description: `Old secret for ${k.prefix}… stays valid for 24h.` }) },
            { label: "Usage logs", icon: <BookOpen />, onSelect: () => toast.info(`${formatNumber(k.requests24h, 0)} requests in the last 24h`, { description: `${k.errors24h} errors · p50 ${API_STATS.p50}ms` }) },
            "sep",
            {
              label: "Revoke key",
              icon: <Trash2 />,
              danger: true,
              onSelect: () => {
                setKeys((ks) => ks.filter((x) => x.id !== k.id));
                toast.error(`${k.name} revoked`, { description: "All sessions using this key were disconnected." });
              },
            },
          ]}
        />
      ),
    },
  ];

  return (
    <Card>
      <CardHeader
        title="API keys"
        subtitle="Scoped per trading account · withdrawals are never possible via API"
        icon={<KeyRound />}
        action={
          <Button size="sm" variant="ember" onClick={onCreate}>
            <Plus /> <span className="hidden sm:inline">Create API key</span>
            <span className="sm:hidden">New</span>
          </Button>
        }
      />
      <div className="px-4 pb-5 pt-4 sm:px-6">
        <DataTable
          columns={columns}
          rows={rows}
          rowKey={(k) => k.id}
          pageSize={8}
          search={(k) => `${k.name} ${k.login} ${k.prefix} ${k.ips.join(" ")}`}
          searchPlaceholder="Search keys, logins, IPs…"
          exportName="kalks-api-keys"
          toolbar={
            <Segmented
              size="xs"
              value={filter}
              onChange={setFilter}
              options={(["all", "active", "paused", "expired"] as const).map((f) => ({
                value: f,
                label: (
                  <>
                    <span className="capitalize">{f}</span> <span className="k-num text-fg-3">{count(f)}</span>
                  </>
                ),
              }))}
            />
          }
        />
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Create key dialog                                                   */
/* ------------------------------------------------------------------ */

const EXPIRY = [
  { value: "30", label: "30 days" },
  { value: "90", label: "90 days" },
  { value: "365", label: "1 year" },
  { value: "never", label: "Never" },
] as const;
const RATES = ["5", "10", "20", "50"] as const;

function randomSecret(len = 40) {
  const chars = "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
  let s = "";
  const arr = new Uint32Array(len);
  crypto.getRandomValues(arr);
  for (let i = 0; i < len; i++) s += chars[arr[i]! % chars.length];
  return s;
}

export function CreateKeyDialog({ open, onOpenChange, onCreated }: { open: boolean; onOpenChange: (o: boolean) => void; onCreated: (k: ApiKey) => void }) {
  const [step, setStep] = React.useState<"form" | "secret">("form");
  const [name, setName] = React.useState("XAUUSD breakout bot");
  const [login, setLogin] = React.useState("80412337");
  const [read, setRead] = React.useState(true);
  const [trade, setTrade] = React.useState(true);
  const [ips, setIps] = React.useState<string[]>(["185.212.44.19"]);
  const [ipDraft, setIpDraft] = React.useState("");
  const [expiry, setExpiry] = React.useState<(typeof EXPIRY)[number]["value"]>("90");
  const [rate, setRate] = React.useState<(typeof RATES)[number]>("20");
  const [secret, setSecret] = React.useState<{ id: string; secret: string } | null>(null);
  const [ack, setAck] = React.useState(false);

  React.useEffect(() => {
    if (open) {
      setStep("form");
      setAck(false);
    }
  }, [open]);

  const addIp = () => {
    const v = ipDraft.trim();
    if (!v) return;
    if (!/^(\d{1,3}\.){3}\d{1,3}(\/\d{1,2})?$/.test(v)) {
      toast.error("Invalid IPv4 address or CIDR", { description: "e.g. 185.212.44.19 or 10.0.0.0/24" });
      return;
    }
    if (!ips.includes(v)) setIps([...ips, v]);
    setIpDraft("");
  };

  const create = () => {
    if (!name.trim()) return toast.error("Give your key a name");
    if (!read && !trade) return toast.error("Select at least one scope");
    const acct = accountOf(login)!;
    const id = `kk_${acct.type === "live" ? "live" : "demo"}_${randomSecret(6)}`;
    const s = `sk_${randomSecret(40)}`;
    setSecret({ id, secret: s });
    setStep("secret");
    const exp = expiry === "never" ? null : new Date(NOW + Number(expiry) * 86400000).toISOString().slice(0, 10);
    onCreated({
      id: `key_${id}`,
      name: name.trim(),
      login,
      prefix: id,
      scopes: [...(read ? (["read"] as const) : []), ...(trade ? (["trade"] as const) : [])],
      ips,
      createdAt: new Date(NOW).toISOString(),
      lastUsed: null,
      expiresAt: exp,
      status: "active",
      rateLimit: Number(rate),
      requests24h: 0,
      errors24h: 0,
    });
    toast.success("API key created", { description: `${name} · ${login}` });
  };

  const envText = secret ? `KALKS_API_KEY=${secret.id}\nKALKS_API_SECRET=${secret.secret}\nKALKS_ACCOUNT=${login}\nKALKS_BASE_URL=https://api.kalkstrade.com/algo/public/v1\n` : "";

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      width={600}
      title={step === "form" ? "Create API key" : "Your new API secret"}
      description={step === "form" ? "Keys are bound to one trading account. Every order placed with it is tagged source=api." : "Store it in a password manager or your server's secret store."}
      footer={
        step === "form" ? (
          <>
            <Button variant="ghost" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button variant="ember" onClick={create}>
              <KeyRound /> Create key
            </Button>
          </>
        ) : (
          <>
            <Button variant="surface" onClick={() => toast.success(".env downloaded", { description: "kalks-api.env" })}>
              <Download /> Download .env
            </Button>
            <Button variant="ember" disabled={!ack} onClick={() => onOpenChange(false)}>
              <Check /> I&apos;ve saved it
            </Button>
          </>
        )
      }
    >
      <AnimatePresence mode="wait" initial={false}>
        {step === "form" ? (
          <motion.div key="form" initial={{ opacity: 0, x: -12 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -12 }} className="space-y-5">
            <Field label="Key name">
              <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="e.g. Gold scalper bot" leading={<Pencil />} />
            </Field>

            <div>
              <div className="mb-1.5 text-[12.5px] font-medium text-fg-2">Trading account</div>
              <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
                {ACCOUNTS.map((a) => {
                  const on = a.login === login;
                  return (
                    <button
                      key={a.login}
                      type="button"
                      onClick={() => setLogin(a.login)}
                      className={cn("k-row flex items-center gap-3 px-3.5 py-2.5 text-left transition-colors", on ? "border-ember/50 bg-ember-soft" : "hover:bg-surface-3")}
                    >
                      <span className={cn("grid size-4 shrink-0 place-items-center rounded-full border", on ? "border-ember bg-ember" : "border-fg-3")}>{on && <span className="size-1.5 rounded-full bg-white" />}</span>
                      <div className="min-w-0 flex-1">
                        <div className="font-mono text-[13px]">{a.login}</div>
                        <div className="text-[11px] text-fg-3">
                          {a.group} · {a.mode} · {a.server}
                        </div>
                      </div>
                      <Chip size="sm" tone={a.type === "live" ? "ember" : "gold"}>
                        {a.type.toUpperCase()}
                      </Chip>
                    </button>
                  );
                })}
              </div>
            </div>

            <div>
              <div className="mb-1.5 text-[12.5px] font-medium text-fg-2">Scopes</div>
              <div className="divide-y divide-line overflow-hidden rounded-[14px] border border-line bg-surface-2">
                <ScopeRow title="Read" desc="Accounts, positions, orders, history, market data" checked={read} onChange={setRead} tone="info" />
                <ScopeRow title="Trade" desc="Place, modify, cancel orders · close positions" checked={trade} onChange={setTrade} tone="ember" />
                <div className="flex items-center gap-3 bg-down/[0.04] px-4 py-3">
                  <span className="grid size-8 place-items-center rounded-full border border-down/25 bg-down-soft text-down">
                    <Lock className="size-3.5" />
                  </span>
                  <div className="min-w-0 flex-1">
                    <div className="text-[13.5px] font-medium text-fg-2 line-through decoration-fg-3">Withdraw</div>
                    <div className="text-[11.5px] text-fg-3">Never available — funds can only leave via the Client Area with 2FA</div>
                  </div>
                  <Chip size="sm" tone="down">
                    Locked
                  </Chip>
                </div>
              </div>
            </div>

            <Field label="IP whitelist" hint={ips.length ? `${ips.length} address${ips.length > 1 ? "es" : ""}` : "Empty = any IP (not recommended)"}>
              <div className="rounded-[14px] border border-line bg-surface-2 p-2 focus-within:border-ember/50">
                <div className="flex flex-wrap gap-1.5">
                  {ips.map((ip) => (
                    <span key={ip} className="inline-flex items-center gap-1 rounded-full border border-line bg-surface-3 py-1 pl-2.5 pr-1 font-mono text-[11.5px]">
                      {ip}
                      <button type="button" onClick={() => setIps(ips.filter((x) => x !== ip))} className="grid size-4 place-items-center rounded-full text-fg-3 hover:bg-surface hover:text-fg" aria-label={`Remove ${ip}`}>
                        <X className="size-3" />
                      </button>
                    </span>
                  ))}
                  <input
                    value={ipDraft}
                    onChange={(e) => setIpDraft(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" || e.key === "," || e.key === " ") {
                        e.preventDefault();
                        addIp();
                      }
                    }}
                    onBlur={addIp}
                    placeholder="Add IP or CIDR, press Enter"
                    className="h-7 min-w-[160px] flex-1 bg-transparent px-1.5 font-mono text-[12.5px] outline-none placeholder:font-sans placeholder:text-fg-3"
                  />
                </div>
              </div>
            </Field>
            <button
              type="button"
              onClick={() => {
                if (!ips.includes("103.21.58.4")) setIps([...ips, "103.21.58.4"]);
                toast.success("Added your current IP", { description: "103.21.58.4 · Mumbai, IN" });
              }}
              className="-mt-3 text-[12px] font-medium text-ember hover:underline"
            >
              + Add my current IP (103.21.58.4)
            </button>

            <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
              <div>
                <div className="mb-1.5 text-[12.5px] font-medium text-fg-2">Expiry</div>
                <Segmented size="xs" value={expiry} onChange={setExpiry} options={EXPIRY} className="flex w-full [&>button]:flex-1" />
              </div>
              <div>
                <div className="mb-1.5 text-[12.5px] font-medium text-fg-2">Rate limit</div>
                <Segmented size="xs" value={rate} onChange={setRate} options={RATES.map((r) => ({ value: r, label: `${r}/s` }))} className="flex w-full [&>button]:flex-1" />
              </div>
            </div>
            {expiry === "never" && (
              <div className="flex items-start gap-2 rounded-[12px] border border-warn/25 bg-warn-soft px-3 py-2 text-[12px] text-warn">
                <AlertTriangle className="mt-0.5 size-3.5 shrink-0" /> Non-expiring keys should always have an IP whitelist.
              </div>
            )}
          </motion.div>
        ) : (
          <motion.div key="secret" initial={{ opacity: 0, x: 12 }} animate={{ opacity: 1, x: 0 }} className="space-y-4">
            <div className="flex items-start gap-3 rounded-[14px] border border-warn/30 bg-warn-soft p-4">
              <ShieldAlert className="mt-0.5 size-5 shrink-0 text-warn" />
              <div>
                <div className="text-[14px] font-medium text-warn">You won&apos;t see this again</div>
                <div className="mt-0.5 text-[12.5px] text-fg-2">Kalks stores only a hash of the secret. If you lose it, rotate the key to issue a new one.</div>
              </div>
            </div>
            <div className="space-y-2">
              <SecretRow label="API key id" value={secret?.id ?? ""} />
              <SecretRow label="Secret" value={secret?.secret ?? ""} strong />
            </div>
            <CodeBlock lang="bash" title="kalks-api.env" code={envText} showLang={false} />
            <div className="grid grid-cols-3 gap-2 text-center">
              {[
                ["Account", login],
                ["Scopes", [read && "read", trade && "trade"].filter(Boolean).join(" + ")],
                ["Limit", `${rate} req/s`],
              ].map(([k, v]) => (
                <div key={k} className="k-row px-3 py-2">
                  <div className="text-[11.5px] text-fg-3">{k}</div>
                  <div className="mt-0.5 truncate font-mono text-[12.5px]">{v}</div>
                </div>
              ))}
            </div>
            <label className="flex cursor-pointer items-center gap-2.5 text-[13px] text-fg-2">
              <input type="checkbox" checked={ack} onChange={(e) => setAck(e.target.checked)} className="size-4 accent-[var(--k-ember)]" />
              I have copied the secret and stored it somewhere safe
            </label>
          </motion.div>
        )}
      </AnimatePresence>
    </Dialog>
  );
}

function ScopeRow({ title, desc, checked, onChange, tone }: { title: string; desc: string; checked: boolean; onChange: (v: boolean) => void; tone: "info" | "ember" }) {
  return (
    <div className="flex items-center gap-3 px-4 py-3">
      <span className={cn("grid size-8 place-items-center rounded-full border", tone === "info" ? "border-info/25 bg-info-soft text-info" : "border-ember/30 bg-ember-soft text-ember")}>
        {tone === "info" ? <BookOpen className="size-3.5" /> : <Zap className="size-3.5" />}
      </span>
      <div className="min-w-0 flex-1">
        <div className="text-[13.5px] font-medium">{title}</div>
        <div className="text-[11.5px] text-fg-3">{desc}</div>
      </div>
      <Toggle checked={checked} onChange={onChange} label={title} />
    </div>
  );
}

function SecretRow({ label, value, strong }: { label: string; value: string; strong?: boolean }) {
  return (
    <div className={cn("k-row flex items-center gap-3 px-4 py-3", strong && "border-ember/30")}>
      <div className="w-20 shrink-0 text-[11px] uppercase tracking-wider text-fg-3">{label}</div>
      <div className={cn("min-w-0 flex-1 break-all font-mono text-[12.5px]", strong ? "text-ember" : "text-fg")}>{value}</div>
      <CopyButton value={value} label={label} />
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Usage                                                               */
/* ------------------------------------------------------------------ */

export function UsageCard() {
  const [mode, setMode] = React.useState<"hour" | "day">("hour");
  const data = React.useMemo(() => apiUsage(mode), [mode]);
  const total = data.reduce((s, d) => s + d.value, 0);
  const peak = Math.max(...data.map((d) => d.value));
  const errs = data.reduce((s, d) => s + d.volume, 0);
  return (
    <Card className="flex h-full flex-col">
      <CardHeader
        title="API usage"
        subtitle={mode === "hour" ? "Requests per hour · last 48h" : "Requests per day · last 30 days"}
        action={<Segmented size="xs" value={mode} onChange={setMode} options={[{ value: "hour", label: "Hourly" }, { value: "day", label: "Daily" }]} />}
      />
      <div className="grid grid-cols-3 gap-2 px-4 pt-4 sm:px-6">
        {[
          ["Total", formatCompact(total), "text-fg"],
          ["Peak", `${formatCompact(peak)}/${mode === "hour" ? "h" : "d"}`, "text-gold"],
          ["Errors (bars)", formatCompact(errs), "text-fg-2"],
        ].map(([k, v, c]) => (
          <div key={k} className="k-row px-3 py-2.5 sm:px-4">
            <div className="truncate text-[11.5px] text-fg-3">{k}</div>
            <div className={cn("k-num mt-0.5 text-[15px] font-medium", c)}>{v}</div>
          </div>
        ))}
      </div>
      <div className="px-3 pb-2 pt-3">
        <EquityChart data={data} height={230} color="gold" />
      </div>
      <div className="mt-auto border-t border-line px-4 pb-5 pt-4 sm:px-6">
        <div className="k-label mb-3">Top endpoints</div>
        <div className="grid grid-cols-1 gap-x-6 gap-y-2.5 sm:grid-cols-2">
          {ENDPOINT_USAGE.map((e, i) => (
            <div key={e.path} className="flex items-center gap-3 text-[12.5px]">
              <span className="w-[150px] shrink-0 truncate font-mono text-fg-2">{e.path}</span>
              <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-surface-3">
                <motion.div className={cn("h-full rounded-full", i === 0 ? "bg-gold" : "bg-fg-3")} initial={{ width: 0 }} animate={{ width: `${(e.share / 34) * 100}%` }} transition={{ duration: 0.8, delay: i * 0.05 }} />
              </div>
              <span className="k-num w-9 text-right text-fg">{e.share}%</span>
              <span className="k-num hidden w-12 text-right text-fg-3 sm:inline">{e.p50}ms</span>
            </div>
          ))}
        </div>
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Global kill switch                                                  */
/* ------------------------------------------------------------------ */

export function KillSwitchCard({ halted, setHalted }: { halted: boolean; setHalted: (v: boolean) => void }) {
  const [closeTrades, setCloseTrades] = React.useState(true);
  const [cancelPending, setCancelPending] = React.useState(true);
  const [disableWebhooks, setDisableWebhooks] = React.useState(true);
  const [confirm, setConfirm] = React.useState(false);
  const [typed, setTyped] = React.useState("");
  const [since, setSince] = React.useState<string | null>(null);

  const activate = () => {
    setHalted(true);
    setConfirm(false);
    setTyped("");
    setSince(new Date().toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit", second: "2-digit", timeZone: "Europe/Istanbul" }));
    toast.error("Global kill switch activated", {
      description: [closeTrades && "7 API/strategy positions closed", cancelPending && "3 pending orders cancelled", disableWebhooks && "webhooks paused"].filter(Boolean).join(" · ") || "All API trading halted",
    });
  };

  return (
    <Card className={cn("relative flex h-full flex-col overflow-hidden border-down/25 transition-colors", halted && "border-down/50")}>
      <div className={cn("pointer-events-none absolute inset-0 bg-[radial-gradient(420px_220px_at_100%_0%,var(--k-down-soft),transparent_70%)]", halted && "bg-[radial-gradient(520px_300px_at_100%_0%,rgba(240,68,56,0.22),transparent_70%)]")} />
      <div className="relative flex items-start justify-between gap-3 px-6 pt-5">
        <div>
          <div className="flex items-center gap-2">
            <h3 className="text-[17px] font-medium tracking-tight">Global kill switch</h3>
            {halted ? (
              <Chip tone="down" dot>
                <span className="animate-pulse">Halted</span>
              </Chip>
            ) : (
              <Chip tone="up" dot>
                Armed
              </Chip>
            )}
          </div>
          <p className="mt-0.5 text-[13px] text-fg-3">Stops every API key, webhook and strategy in one click.</p>
        </div>
        <Icon3D name="warning" size={52} className="-mr-1 -mt-1" />
      </div>

      <div className="relative mt-4 grid grid-cols-3 gap-2 px-4 sm:px-6">
        {[
          ["Auto positions", halted ? "0" : "7", halted ? "text-fg-3" : "text-fg"],
          ["Notional", halted ? "$0" : "$184.2K", halted ? "text-fg-3" : "text-fg"],
          ["Floating P/L", halted ? "$0.00" : "+$412.60", halted ? "text-fg-3" : "text-up"],
        ].map(([k, v, c]) => (
          <div key={k} className="k-row px-3 py-2.5">
            <div className="truncate text-[11.5px] text-fg-3">{k}</div>
            <div className={cn("k-num mt-0.5 text-[15px] font-medium", c)}>{v}</div>
          </div>
        ))}
      </div>

      <div className="relative mt-3 flex-1 space-y-2 px-4 sm:px-6">
        <div className="k-label pb-1 pt-1">When triggered</div>
        {[
          { l: "Close API, webhook & strategy positions", d: "Manual and copy trades are not touched", v: closeTrades, s: setCloseTrades },
          { l: "Cancel pending API orders", d: "Limit and stop orders with source ≠ manual", v: cancelPending, s: setCancelPending },
          { l: "Pause signal webhooks", d: "Incoming signals return 423", v: disableWebhooks, s: setDisableWebhooks },
        ].map((o) => (
          <div key={o.l} className="k-row flex items-center gap-3 px-4 py-2.5">
            <div className="min-w-0 flex-1">
              <div className="text-[13px] font-medium">{o.l}</div>
              <div className="text-[11.5px] text-fg-3">{o.d}</div>
            </div>
            <Toggle checked={o.v} onChange={o.s} label={o.l} />
          </div>
        ))}
      </div>

      <div className="relative px-4 pb-5 pt-4 sm:px-6">
        {!halted && <div className="mb-3 flex items-center justify-between text-[11.5px] text-fg-3"><span>Last triggered · 12 Aug 2026, 16:04</span><span>Scope · all 5 accounts</span></div>}
        {halted ? (
          <div className="rounded-[14px] border border-down/30 bg-down-soft p-4">
            <div className="flex items-center gap-2 text-[13px] font-medium text-down">
              <OctagonX className="size-4" /> All automated trading halted{since ? ` since ${since}` : ""}
            </div>
            <p className="mt-1 text-[12px] text-fg-2">API calls return 423 kill_switch_active. Manual trading in the terminal still works.</p>
            <Button
              variant="up-outline"
              size="sm"
              className="mt-3 w-full"
              onClick={() => {
                setHalted(false);
                toast.success("Automated trading resumed", { description: "API keys and webhooks are live again." });
              }}
            >
              <Play /> Resume automated trading
            </Button>
          </div>
        ) : (
          <Button variant="sell" size="lg" className="w-full" onClick={() => setConfirm(true)}>
            <OctagonX /> Activate kill switch
          </Button>
        )}
      </div>

      <Dialog
        open={confirm}
        onOpenChange={setConfirm}
        width={460}
        title="Stop all automated trading?"
        description="This takes effect immediately on every account."
        footer={
          <>
            <Button variant="ghost" onClick={() => setConfirm(false)}>
              Cancel
            </Button>
            <Button variant="sell" disabled={typed.trim().toUpperCase() !== "STOP"} onClick={activate}>
              <OctagonX /> Halt everything
            </Button>
          </>
        }
      >
        <div className="space-y-4">
          <div className="space-y-2">
            {[
              ["API keys paused", "4 active keys"],
              ["Webhooks", disableWebhooks ? "2 enabled → paused" : "left running"],
              ["Open automated positions", closeTrades ? "7 will be closed at market" : "kept open"],
              ["Pending API orders", cancelPending ? "3 will be cancelled" : "kept"],
            ].map(([k, v]) => (
              <div key={k} className="flex items-center justify-between text-[13px]">
                <span className="text-fg-3">{k}</span>
                <span className="font-medium">{v}</span>
              </div>
            ))}
          </div>
          <Field label='Type "STOP" to confirm'>
            <Input value={typed} onChange={(e) => setTyped(e.target.value)} placeholder="STOP" inputClassName="font-mono uppercase" autoFocus />
          </Field>
        </div>
      </Dialog>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Order sources                                                       */
/* ------------------------------------------------------------------ */

const SOURCE_COLORS = ["var(--k-fg-3)", "var(--k-gold)", "var(--k-ember)", "var(--k-up)", "var(--k-info)"];

export function OrderSourcesCard() {
  const total = ORDER_SOURCES.reduce((s, o) => s + o.count, 0);
  return (
    <Card className="flex h-full flex-col">
      <CardHeader title="Orders by source" subtitle="Every order carries a source tag · last 30 days" />
      <div className="flex flex-1 flex-col items-center gap-5 px-6 pb-6 pt-5 sm:flex-row xl:flex-col 2xl:flex-row">
        <Donut
          size={168}
          thickness={18}
          data={ORDER_SOURCES.map((o, i) => ({ label: o.label, value: o.count, color: SOURCE_COLORS[i] }))}
          center={
            <div>
              <div className="k-num text-[22px] font-semibold">{formatNumber(total, 0)}</div>
              <div className="text-[11px] text-fg-3">orders</div>
            </div>
          }
        />
        <div className="w-full flex-1 space-y-1.5">
          {ORDER_SOURCES.map((o, i) => (
            <div key={o.source} className="flex items-center gap-2.5 rounded-[10px] px-2 py-1.5 text-[13px] hover:bg-surface-2">
              <span className="size-2.5 rounded-full" style={{ background: SOURCE_COLORS[i] }} />
              <span className="flex-1 text-fg-2">{o.label}</span>
              <span className="font-mono text-[10.5px] text-fg-3">source={o.source}</span>
              <span className="k-num w-12 text-right font-medium">{((o.count / total) * 100).toFixed(1)}%</span>
            </div>
          ))}
        </div>
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* SDK quick start                                                     */
/* ------------------------------------------------------------------ */

const SDK: Record<"python" | "js" | "fix", { label: string; install: string; lang: CodeLang; file: string; code: string; badge: string }> = {
  python: {
    label: "Python",
    badge: "kalks 2.4.1",
    install: "pip install kalks",
    lang: "python",
    file: "bot.py",
    code: `from kalks import Client

client = Client(key="kk_live_EXAMPLE1", secret=os.environ["KALKS_SECRET"])

# Buy 0.5 lot gold with SL/TP — tagged source=api
order = client.orders.create(
    login="80412337", symbol="XAUUSD", side="buy",
    volume=0.5, sl=2638.0, tp=2690.0,
)
print(order.ticket, order.price)  # 51298844 2654.48

async for tick in client.stream(["XAUUSD", "EURUSD"]):
    print(tick.symbol, tick.bid, tick.ask)`,
  },
  js: {
    label: "JavaScript",
    badge: "@kalks/sdk 2.4.0",
    install: "npm install @kalks/sdk",
    lang: "js",
    file: "bot.ts",
    code: `import { Kalks } from "@kalks/sdk";

const kalks = new Kalks({ key: "kk_live_EXAMPLE1", secret: process.env.KALKS_SECRET });

// Market order on NAS100 — tagged source=api
const order = await kalks.orders.create({
  login: "80412337", symbol: "NAS100", side: "sell",
  volume: 1.2, sl: 20168.0, tp: 19980.0,
});

kalks.stream(["NAS100"], (t) => console.log(t.bid, t.ask));`,
  },
  fix: {
    label: "FIX 4.4",
    badge: "fix.kalkstrade.com:9880",
    install: "openssl s_client -connect fix.kalkstrade.com:9880",
    lang: "fix",
    file: "logon.fix",
    code: `8=FIX.4.4|9=112|35=A|49=KLK_80412337|56=KALKS|34=1|52=20260924-14:58:12.184|98=0|108=30|141=Y|553=kk_live_EXAMPLE1|554=••••••|10=087|
8=FIX.4.4|9=148|35=D|49=KLK_80412337|56=KALKS|34=2|11=gold-bo-0924-01|55=XAUUSD|54=1|38=50|40=1|59=0|60=20260924-14:58:12.201|10=164|`,
  },
};

export function SdkQuickstart() {
  const [sdk, setSdk] = React.useState<keyof typeof SDK>("python");
  const s = SDK[sdk];
  return (
    <Card className="flex h-full flex-col">
      <CardHeader
        title="Quick start"
        subtitle="Official SDKs for Python & JavaScript, plus a raw FIX 4.4 gateway"
        action={
          <Link href="/developer/docs">
            <Button size="sm" variant="surface">
              <BookOpen /> <span className="hidden sm:inline">Full docs</span>
            </Button>
          </Link>
        }
      />
      <div className="flex flex-wrap items-center gap-2 px-4 pt-4 sm:px-6">
        {(Object.keys(SDK) as (keyof typeof SDK)[]).map((k) => (
          <button
            key={k}
            type="button"
            onClick={() => setSdk(k)}
            className={cn(
              "inline-flex h-9 items-center gap-2 rounded-full border px-3.5 text-[13px] font-medium transition-colors",
              sdk === k ? "border-ember/40 bg-ember-soft text-ember" : "border-line bg-surface-2 text-fg-2 hover:bg-surface-3 hover:text-fg",
            )}
          >
            <span className={cn("size-1.5 rounded-full", sdk === k ? "bg-ember" : "bg-fg-3")} />
            {SDK[k].label}
            <span className="font-mono text-[10.5px] text-fg-3">{SDK[k].badge}</span>
          </button>
        ))}
      </div>
      <div className="mx-4 mt-3 flex items-center gap-2 rounded-full border border-line bg-surface-2 py-1.5 pl-4 pr-1.5 sm:mx-6">
        <span className="font-mono text-[12px] text-fg-3">$</span>
        <span className="min-w-0 flex-1 truncate font-mono text-[12.5px]">{s.install}</span>
        <CopyButton value={s.install} label="Install command" className="size-7 rounded-full" />
      </div>
      <div className="flex-1 px-4 pb-5 pt-3 sm:px-6">
        <CodeBlock key={sdk} code={s.code} lang={s.lang} title={s.file} wrap={sdk === "fix"} />
      </div>
    </Card>
  );
}

export function SecurityChecklist({ keys }: { keys: ApiKey[] }) {
  const anyIp = keys.filter((k) => k.status !== "expired" && k.ips.length === 0).length;
  const noExp = keys.filter((k) => k.status !== "expired" && !k.expiresAt).length;
  const items = [
    { ok: true, t: "Withdraw scope is never granted", d: "Enforced at platform level" },
    { ok: true, t: "2FA required to create keys", d: "Google Authenticator · enabled" },
    { ok: anyIp === 0, t: anyIp ? `${anyIp} key${anyIp > 1 ? "s" : ""} accept any IP` : "All keys IP-restricted", d: "Add a whitelist for production bots" },
    { ok: noExp === 0, t: noExp ? `${noExp} key without expiry` : "All keys expire", d: "Rotate at least every 12 months" },
  ];
  return (
    <div className="grid grid-cols-1 gap-2 sm:grid-cols-2 xl:grid-cols-4">
      {items.map((i) => (
        <div key={i.t} className="k-row flex items-center gap-3 px-4 py-3">
          <span className={cn("grid size-8 shrink-0 place-items-center rounded-full border", i.ok ? "border-up/25 bg-up-soft text-up" : "border-warn/25 bg-warn-soft text-warn")}>
            {i.ok ? <ShieldCheck className="size-4" /> : <AlertTriangle className="size-4" />}
          </span>
          <div className="min-w-0 flex-1">
            <div className="truncate text-[13px] font-medium">{i.t}</div>
            <div className="truncate text-[11.5px] text-fg-3">{i.d}</div>
          </div>
          {!i.ok && (
            <Button size="xs" variant="surface" onClick={() => toast.info("Open the key menu → Edit scopes & IPs")}>
              Fix
            </Button>
          )}
        </div>
      ))}
    </div>
  );
}

