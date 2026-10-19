"use client";

/**
 * Settings › Client Area layout (gateway sections.rs, docs/SECTIONS.md): every Client Area page and dashboard block,
 * grouped like the Client Area nav, switched for everyone or by rule (country, account group, client), with a preview
 * of what a given client sees. Staff with `settings.write` switch sections (modules stay with the Platform Owner);
 * every change asks for a reason and is audited. Hiding never deletes anything.
 */
import * as React from "react";
import Link from "next/link";
import { Eye, EyeOff, LayoutGrid, ListFilter, Plus, RotateCcw, Search, SlidersHorizontal, Trash2, UserRound, X } from "lucide-react";
import { Button, Card, CardHeader, Chip, Dialog, EmptyState, Field, Flag, IconButton, Input, KpiCard, PageHeader, Segmented, Select, Toggle, cn } from "@kalks/ui";
import { SECTIONS, SECTION_GROUPS, type SectionDef } from "@kalks/ui/sections";
import { useCan } from "@/components/staff-session";
import { ErrorState, TableSkeleton, ago, countryName, useApi, useDebounced, useNow } from "@/components/live/kit";
import { act } from "./kit";

type Scope = "all" | "country" | "group" | "user";
type Rule = { id: number; scope: Scope; value: string; enabled: boolean; updated_at: string; updated_by: string | null; label: string | null };
type Section = { key: string; name: string | null; description: string | null; module: string | null; module_on: boolean; default: boolean; enabled: boolean; rules: Rule[] };
type ListResp = { sections: Section[]; modules: Record<string, boolean>; can_edit: boolean; version: number };
type Preview = { who: { user_id: number | null; country: string | null; groups: string[]; client: { id: number; email: string; name: string } | null }; sections: Record<string, boolean>; sources: Record<string, string> };
type Client = { id: number; email: string; name: string; country?: string };
type Group = { code: string; name: string; enabled?: boolean };

/** ISO 3166 country codes for the country picker (names from the browser, in English). */
const COUNTRY_CODES =
  "ad ae af ag ai al am ao aq ar as at au aw ax az ba bb bd be bf bg bh bi bj bl bm bn bo bq br bs bt bv bw by bz ca cc cd cf cg ch ci ck cl cm cn co cr cu cv cw cx cy cz de dj dk dm do dz ec ee eg eh er es et fi fj fk fm fo fr ga gb gd ge gf gg gh gi gl gm gn gp gq gr gs gt gu gw gy hk hm hn hr ht hu id ie il im in io iq ir is it je jm jo jp ke kg kh ki km kn kp kr kw ky kz la lb lc li lk lr ls lt lu lv ly ma mc md me mf mg mh mk ml mm mn mo mp mq mr ms mt mu mv mw mx my mz na nc ne nf ng ni nl no np nr nu nz om pa pe pf pg ph pk pl pm pn pr ps pt pw py qa re ro rs ru rw sa sb sc sd se sg sh si sj sk sl sm sn so sr ss st sv sx sy sz tc td tf tg th tj tk tl tm tn to tr tt tv tw tz ua ug um us uy uz va vc ve vg vi vn vu wf ws ye yt za zm zw".split(" ");
let countryOptions: { value: string; label: string }[] | null = null;
function countries() {
  countryOptions ??= COUNTRY_CODES.map((c) => ({ value: c, label: countryName(c) })).sort((a, b) => a.label.localeCompare(b.label));
  return countryOptions;
}

const SCOPE_LABEL: Record<Scope, string> = { all: "Everyone", country: "Country", group: "Account group", user: "Client" };
const SOURCE_LABEL: Record<string, string> = { user: "Client rule", group: "Group rule", country: "Country rule", all: "Everyone", default: "Default", module: "Module off" };

/* ------------------------------------------------------------------ */
/* Confirm with a reason (the module-switch pattern)                   */
/* ------------------------------------------------------------------ */

type Ask = { title: string; text: React.ReactNode; confirm: string; danger?: boolean; run: (reason: string) => Promise<boolean> };

/** Every change asks to confirm with a reason (audited with it). `ask` opens the dialog; render `dialog` once. */
function useReasonConfirm() {
  const [p, setP] = React.useState<Ask | null>(null);
  const [reason, setReason] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const ask = React.useCallback((a: Ask) => {
    setReason("");
    setP(a);
  }, []);
  const save = async () => {
    if (!p) return;
    setBusy(true);
    const ok = await p.run(reason.trim());
    setBusy(false);
    if (ok) setP(null);
  };
  const dialog = (
    <Dialog open={!!p} onOpenChange={(o) => !o && setP(null)} title={p?.title ?? ""}>
      {p && (
        <div data-testid="section-confirm">
          <div className="rounded-[12px] bg-surface-2 px-3.5 py-2.5 text-[13px] leading-relaxed text-fg-2">{p.text}</div>
          <p className="mt-2 text-[12px] text-fg-3">Takes effect within about 30 seconds in the Client Area and the app. Nothing is deleted; switching back shows it again.</p>
          <Field label="Reason (audit log)" className="mt-4">
            <Input value={reason} onChange={(e) => setReason(e.target.value)} placeholder="e.g. not licensed in this country yet" name="section-reason" autoFocus />
          </Field>
          <div className="mt-5 flex justify-end gap-2">
            <Button variant="ghost" onClick={() => setP(null)}>
              Cancel
            </Button>
            <Button variant={p.danger ? "ember" : "surface"} disabled={busy || reason.trim().length < 3} onClick={() => void save()} data-testid="section-confirm-ok">
              {p.confirm}
            </Button>
          </div>
        </div>
      )}
    </Dialog>
  );
  return { ask, dialog };
}

/* ------------------------------------------------------------------ */
/* Pickers                                                             */
/* ------------------------------------------------------------------ */

function CountryPicker({ value, onChange, any }: { value: string; onChange: (v: string) => void; any?: string }) {
  return (
    <Select
      value={value}
      onChange={(e) => onChange(e.target.value)}
      leading={value ? <Flag country={value} className="size-4" /> : undefined}
      options={[...(any !== undefined ? [{ value: "", label: any }] : [{ value: "", label: "Pick a country" }]), ...countries()]}
      aria-label="Country"
    />
  );
}

/** The broker's account groups (trading engine, `accounts.read`); free text when they can't be listed. */
function useGroups(): Group[] | null {
  const { data } = useApi<{ groups: Group[] }>("/api/trading/admin/groups");
  return data?.groups ?? null;
}

function GroupPicker({ value, onChange, groups }: { value: string; onChange: (v: string) => void; groups: Group[] | null }) {
  if (!groups?.length) return <Input value={value} onChange={(e) => onChange(e.target.value.toLowerCase())} placeholder="Group code, e.g. pro" aria-label="Account group" className="font-mono" />;
  return <Select value={value} onChange={(e) => onChange(e.target.value)} options={[{ value: "", label: "Pick an account group" }, ...groups.map((g) => ({ value: g.code, label: `${g.name} (${g.code})` }))]} aria-label="Account group" />;
}

/** Client search by email, name or ID (`clients.read`). */
function ClientSearch({ value, onChange }: { value: Client | null; onChange: (c: Client | null) => void }) {
  const [q, setQ] = React.useState("");
  const term = useDebounced(q.trim(), 300);
  const { data, loading } = useApi<{ items: Client[] }>(!value && term.length >= 2 ? `/api/admin/users?q=${encodeURIComponent(term)}&per_page=6` : null);
  if (value)
    return (
      <div className="flex items-center justify-between gap-2 rounded-[14px] border border-line bg-surface-2 px-3 py-2" data-testid="client-picked">
        <div className="min-w-0">
          <div className="truncate text-[13px] font-medium">{value.name}</div>
          <div className="truncate text-[12px] text-fg-3">
            <span className="font-mono">#{value.id}</span> · {value.email}
          </div>
        </div>
        <IconButton aria-label="Clear client" size="sm" onClick={() => onChange(null)}>
          <X />
        </IconButton>
      </div>
    );
  return (
    <div className="relative">
      <Input value={q} onChange={(e) => setQ(e.target.value)} leading={<Search />} placeholder="Email, name or client ID" aria-label="Find a client" name="client-search" />
      {term.length >= 2 && (
        <div className="absolute inset-x-0 top-full z-20 mt-1 max-h-64 overflow-auto rounded-[14px] border border-line bg-surface p-1 shadow-e2">
          {!data && loading && <div className="px-3 py-2 text-[12.5px] text-fg-3">Searching…</div>}
          {data?.items.length === 0 && <div className="px-3 py-2 text-[12.5px] text-fg-3">No client found</div>}
          {data?.items.map((c) => (
            <button
              key={c.id}
              type="button"
              className="flex w-full items-center gap-2 rounded-[10px] px-3 py-2 text-left hover:bg-surface-2"
              onClick={() => {
                onChange(c);
                setQ("");
              }}
            >
              {c.country && <Flag country={c.country} className="size-4 shrink-0" />}
              <span className="min-w-0 flex-1 truncate text-[13px]">{c.name}</span>
              <span className="shrink-0 font-mono text-[11.5px] text-fg-3">#{c.id}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function RuleValue({ r }: { r: Pick<Rule, "scope" | "value" | "label"> }) {
  if (r.scope === "country")
    return (
      <span className="inline-flex items-center gap-1.5">
        <Flag country={r.value} className="size-4" /> {countryName(r.value)}
      </span>
    );
  if (r.scope === "group") return <span className="font-mono">{r.value}</span>;
  if (r.scope === "user")
    return (
      <span>
        <span className="font-mono">#{r.value}</span>
        {r.label ? <span className="text-fg-3"> · {r.label}</span> : null}
      </span>
    );
  return <span>Everyone</span>;
}

/* ------------------------------------------------------------------ */
/* Rules drawer                                                        */
/* ------------------------------------------------------------------ */

function RulesDrawer({ s, def, canEdit, groups, onClose, confirm, reload }: { s: Section | null; def: SectionDef | undefined; canEdit: boolean; groups: Group[] | null; onClose: () => void; confirm: (a: Ask) => void; reload: () => void }) {
  const now = useNow();
  const [scope, setScope] = React.useState<Exclude<Scope, "all">>("country");
  const [country, setCountry] = React.useState("");
  const [group, setGroup] = React.useState("");
  const [client, setClient] = React.useState<Client | null>(null);
  const [show, setShow] = React.useState<"show" | "hide">("hide");
  React.useEffect(() => {
    setCountry("");
    setGroup("");
    setClient(null);
  }, [s?.key]);
  if (!s) return null;
  const name = s.name ?? s.key;
  const targeted = s.rules.filter((r) => r.scope !== "all");
  const value = scope === "country" ? country : scope === "group" ? group : client ? String(client.id) : "";
  const add = () =>
    confirm({
      title: `${show === "show" ? "Show" : "Hide"} ${name} for ${scope === "user" ? client?.email : scope === "country" ? countryName(country) : `group ${group}`}?`,
      text: (
        <>
          A {SCOPE_LABEL[scope].toLowerCase()} rule. Precedence: client &gt; account group &gt; country &gt; everyone.{!s.module_on && " Its module is off, so it stays hidden until the platform owner switches the module on."}
        </>
      ),
      confirm: show === "show" ? "Add rule: show" : "Add rule: hide",
      danger: show === "hide",
      run: async (reason) => {
        const ok = await act("POST", `/api/admin/settings/sections/${s.key}/rules`, { scope, value, enabled: show === "show", reason }, "Rule added");
        if (ok) reload();
        return !!ok;
      },
    });
  const remove = (r: Rule) =>
    confirm({
      title: `Remove this ${SCOPE_LABEL[r.scope].toLowerCase()} rule?`,
      text: (
        <>
          {r.enabled ? "Shown" : "Hidden"} for <RuleValue r={r} />. The next rule down (or the switch for everyone) decides again.
        </>
      ),
      confirm: "Remove rule",
      run: async (reason) => {
        const ok = await act("DELETE", `/api/admin/settings/sections/rules/${r.id}`, { reason }, "Rule removed");
        if (ok) reload();
        return !!ok;
      },
    });
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()} side="right" title={name} description={def?.path ? `Client Area page ${def.path}` : "Dashboard block"}>
      <div className="space-y-5 px-6 pb-6" data-testid="rules-drawer">
        <div className="flex flex-wrap items-center gap-2 text-[12.5px] text-fg-3">
          <code>{s.key}</code>
          {s.module && (
            <Chip size="sm" tone={s.module_on ? "neutral" : "warn"}>
              Module {s.module}
              {s.module_on ? "" : " · off"}
            </Chip>
          )}
          <Chip size="sm" tone={s.enabled ? "up" : "neutral"} dot>
            Everyone: {s.enabled ? "shown" : "hidden"}
          </Chip>
        </div>
        <div>
          <div className="mb-2 text-[12px] font-medium uppercase tracking-[0.08em] text-fg-3">Targeting rules</div>
          {targeted.length === 0 ? (
            <p className="rounded-[14px] bg-surface-2 px-3.5 py-3 text-[13px] text-fg-3">No rules: everyone gets the switch for everyone.</p>
          ) : (
            <div className="divide-y divide-line rounded-[16px] border border-line">
              {targeted.map((r) => (
                <div key={r.id} className="flex items-center gap-3 px-3.5 py-2.5" data-testid={`rule-${r.scope}-${r.value}`}>
                  <Chip size="sm" tone="info">
                    {SCOPE_LABEL[r.scope]}
                  </Chip>
                  <div className="min-w-0 flex-1 text-[13px]">
                    <RuleValue r={r} />
                    <div className="text-[11.5px] text-fg-3">
                      {r.updated_by ?? "Staff"} · {ago(r.updated_at, now)}
                    </div>
                  </div>
                  <Chip size="sm" tone={r.enabled ? "up" : "down"} dot>
                    {r.enabled ? "Shown" : "Hidden"}
                  </Chip>
                  {canEdit && (
                    <IconButton aria-label="Remove rule" size="sm" onClick={() => remove(r)}>
                      <Trash2 />
                    </IconButton>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
        {canEdit && (
          <div className="space-y-3 rounded-[18px] border border-line p-4">
            <div className="text-[13px] font-medium">Add a rule</div>
            <Segmented<Exclude<Scope, "all">> value={scope} onChange={setScope} options={[{ value: "country", label: "Country" }, { value: "group", label: "Account group" }, { value: "user", label: "Client" }]} aria-label="Rule scope" />
            {scope === "country" && <CountryPicker value={country} onChange={setCountry} />}
            {scope === "group" && <GroupPicker value={group} onChange={setGroup} groups={groups} />}
            {scope === "user" && <ClientSearch value={client} onChange={setClient} />}
            <div className="flex flex-wrap items-center justify-between gap-2">
              <Segmented<"show" | "hide"> value={show} onChange={setShow} options={[{ value: "hide", label: "Hide" }, { value: "show", label: "Show" }]} aria-label="Shown or hidden" />
              <Button variant="ember" size="sm" disabled={!value} onClick={add} data-testid="rule-add">
                <Plus /> Add rule
              </Button>
            </div>
          </div>
        )}
      </div>
    </Dialog>
  );
}

/* ------------------------------------------------------------------ */
/* Preview                                                             */
/* ------------------------------------------------------------------ */

/** `version`: the switches' version (a change reloads the preview). */
function PreviewCard({ groups, version }: { groups: Group[] | null; version: number }) {
  const [country, setCountry] = React.useState("");
  const [picked, setPicked] = React.useState<string[]>([]);
  const [client, setClient] = React.useState<Client | null>(null);
  // a chosen client brings the groups of their trading accounts (trading engine, `accounts.read`)
  const accounts = useApi<{ items: { group?: string }[] }>(client ? `/api/trading/admin/accounts?user_id=${client.id}&limit=200` : null);
  React.useEffect(() => {
    if (client && accounts.data) setPicked([...new Set(accounts.data.items.map((a) => a.group ?? "").filter(Boolean))].sort());
  }, [client, accounts.data]);
  const qs = new URLSearchParams();
  if (country) qs.set("country", country);
  if (picked.length) qs.set("groups", picked.join(","));
  if (client) qs.set("user_id", String(client.id));
  qs.set("v", String(version));
  const { data, error, reload } = useApi<Preview>(`/api/admin/settings/sections/preview?${qs}`);
  const on = (key: string) => data?.sections[key] !== false;
  const why = (key: string) => SOURCE_LABEL[data?.sources[key] ?? "default"] ?? "";
  const togglePick = (code: string) => setPicked((p) => (p.includes(code) ? p.filter((x) => x !== code) : [...p, code].sort()));
  return (
    <Card>
      <CardHeader title="Preview" subtitle="What a client sees: pick a country, account groups or a client" icon={<Eye />} />
      <div className="space-y-3 px-4 pb-5 pt-3 sm:px-6" data-testid="layout-preview">
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <Field label="Country">
            <CountryPicker value={country} onChange={setCountry} any={client ? "The client's country" : "Any country"} />
          </Field>
          <Field label="Client">
            <ClientSearch value={client} onChange={setClient} />
          </Field>
        </div>
        <div>
          <div className="mb-1.5 text-[12px] text-fg-3">Account groups</div>
          <div className="flex flex-wrap gap-1.5">
            {(groups ?? picked.map((code) => ({ code, name: code }))).map((g) => (
              <button key={g.code} type="button" onClick={() => togglePick(g.code)} className={cn("rounded-full border px-2.5 py-1 font-mono text-[11.5px]", picked.includes(g.code) ? "border-ember bg-ember-soft text-fg" : "border-line text-fg-3 hover:text-fg")}>
                {g.code}
              </button>
            ))}
            {!groups?.length && !picked.length && <span className="text-[12px] text-fg-3">No account groups to pick</span>}
          </div>
        </div>
        {error ? (
          <ErrorState error={error} onRetry={reload} />
        ) : !data ? (
          <TableSkeleton rows={6} />
        ) : (
          <>
            {data.who.client && (
              <p className="text-[12.5px] text-fg-3">
                {data.who.client.name} · {data.who.country ? countryName(data.who.country) : "no country"} · {data.who.groups.length ? data.who.groups.join(", ") : "no account groups"}
              </p>
            )}
            <div className="rounded-[18px] border border-line bg-surface-2 p-3">
              <div className="mb-2 text-[11.5px] font-medium uppercase tracking-[0.08em] text-fg-3">Navigation</div>
              <ul className="space-y-1.5">
                {SECTION_GROUPS.map((g) => {
                  const pages = SECTIONS.filter((x) => x.kind === "page" && x.group === g.key);
                  const shown = pages.filter((x) => on(x.key));
                  if (g.key !== "dashboard" && !shown.length) return null;
                  return (
                    <li key={g.key} className="text-[13px]">
                      <span className="font-medium">{g.name}</span>
                      <span className="text-fg-3"> · {[...(g.key === "dashboard" ? ["Overview"] : []), ...shown.map((x) => x.name)].join(", ")}</span>
                    </li>
                  );
                })}
              </ul>
              <div className="mb-2 mt-4 text-[11.5px] font-medium uppercase tracking-[0.08em] text-fg-3">Dashboard</div>
              <div className="flex flex-wrap gap-1.5">
                {SECTIONS.filter((x) => x.kind === "block").map((b) => (
                  <Chip key={b.key} size="sm" tone={on(b.key) ? "up" : "neutral"} dot title={why(b.key)}>
                    <span className={cn(!on(b.key) && "line-through")}>{b.name}</span>
                  </Chip>
                ))}
              </div>
            </div>
            {(() => {
              const hidden = SECTIONS.filter((x) => !on(x.key));
              return hidden.length ? (
                <div className="text-[12.5px]">
                  <div className="mb-1 text-fg-3">Hidden ({hidden.length})</div>
                  <ul className="space-y-1">
                    {hidden.map((x) => (
                      <li key={x.key} className="flex items-center justify-between gap-2">
                        <span>{x.name}</span>
                        <Chip size="sm" tone="neutral">
                          {why(x.key)}
                        </Chip>
                      </li>
                    ))}
                  </ul>
                </div>
              ) : (
                <p className="text-[12.5px] text-fg-3">Everything is shown.</p>
              );
            })()}
          </>
        )}
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Page                                                                */
/* ------------------------------------------------------------------ */

function SectionRow({ s, def, canEdit, onSet, onRules }: { s: Section; def: SectionDef | undefined; canEdit: boolean; onSet: (v: boolean | null) => void; onRules: () => void }) {
  const all = s.rules.find((r) => r.scope === "all");
  const targeted = s.rules.filter((r) => r.scope !== "all").length;
  return (
    <div className="flex items-center justify-between gap-3 border-b border-line py-3 last:border-0" data-testid={`section-${s.key}`}>
      <div className="min-w-0">
        <div className="flex flex-wrap items-center gap-2 text-[13.5px] font-medium">
          {def?.kind === "block" ? <LayoutGrid className="size-3.5 text-fg-3" /> : null}
          {s.name ?? s.key}
          {!s.module_on && (
            <Chip size="sm" tone="warn">
              Module off
            </Chip>
          )}
          {targeted > 0 && (
            <Chip size="sm" tone="info">
              {targeted} {targeted === 1 ? "rule" : "rules"}
            </Chip>
          )}
        </div>
        <div className="truncate text-[12px] text-fg-3">{def?.path ?? "Dashboard block"}</div>
      </div>
      <div className="flex shrink-0 items-center gap-2">
        {canEdit && all && (
          <button type="button" className="text-fg-3 hover:text-ember" title="Back to the default (shown)" onClick={() => onSet(null)}>
            <RotateCcw className="size-4" />
          </button>
        )}
        <Button size="xs" variant="ghost" onClick={onRules} data-testid={`rules-${s.key}`}>
          <ListFilter /> Rules
        </Button>
        {canEdit ? (
          <Toggle checked={s.enabled} disabled={!s.module_on} onChange={(v) => onSet(v)} label={`${s.name ?? s.key} for everyone`} />
        ) : (
          <Chip size="sm" tone={s.enabled ? "up" : "neutral"} dot>
            {s.enabled ? "Shown" : "Hidden"}
          </Chip>
        )}
      </div>
    </div>
  );
}

export function LiveClientLayout() {
  const canRead = useCan("settings.read");
  const { data, error, reload } = useApi<ListResp>(canRead ? "/api/admin/settings/sections" : null);
  const groups = useGroups();
  const confirm = useReasonConfirm();
  const [open, setOpen] = React.useState<string | null>(null);
  const [kind, setKind] = React.useState<"all" | "page" | "block">("all");
  if (!canRead) return <EmptyState title="Not available for your role" illustration="locked" />;

  const byKey = new Map(data?.sections.map((s) => [s.key, s]) ?? []);
  const set = (s: Section, v: boolean | null) => {
    const name = s.name ?? s.key;
    confirm.ask({
      title: v === null ? `Reset ${name} to the default?` : `${v ? "Show" : "Hide"} ${name} for everyone?`,
      text: v === false ? <>Clients no longer see {name} in the Client Area or the app (its menu entry, page or card). Targeting rules can still show it to some clients.</> : v ? <>Clients see {name} again, except where a targeting rule hides it.</> : <>Back to the platform default (shown). Targeting rules stay.</>,
      confirm: v === null ? "Reset to default" : v ? `Show ${name}` : `Hide ${name}`,
      danger: v === false,
      run: async (reason) => {
        const ok = await act("PUT", `/api/admin/settings/sections/${s.key}`, { enabled: v, reason }, v === null ? `${name}: default` : `${name} ${v ? "shown" : "hidden"} for everyone`);
        if (ok) reload();
        return !!ok;
      },
    });
  };
  const all = data?.sections ?? [];
  const pages = SECTIONS.filter((x) => x.kind === "page");
  const blocks = SECTIONS.filter((x) => x.kind === "block");
  const shown = (defs: readonly SectionDef[]) => defs.filter((d) => byKey.get(d.key)?.enabled !== false).length;
  const rules = all.reduce((n, s) => n + s.rules.filter((r) => r.scope !== "all").length, 0);
  const openSection = open ? (byKey.get(open) ?? null) : null;

  return (
    <div className="pb-10">
      <PageHeader
        title="Client Area layout"
        subtitle="Choose which pages and dashboard blocks your clients see, for everyone or by country, account group or client. Hiding never deletes anything."
        actions={
          <Link href="/settings/features">
            <Button variant="surface">
              <SlidersHorizontal /> Features & modules
            </Button>
          </Link>
        }
      />
      {error ? (
        <ErrorState error={error} onRetry={reload} />
      ) : !data ? (
        <TableSkeleton rows={8} />
      ) : (
        <>
          <div className="mb-4 grid grid-cols-2 gap-4 lg:grid-cols-4">
            <KpiCard label="Pages shown" value={`${shown(pages)} / ${pages.length}`} icon={<Eye />} />
            <KpiCard label="Dashboard blocks shown" value={`${shown(blocks)} / ${blocks.length}`} icon={<LayoutGrid />} />
            <KpiCard label="Targeting rules" value={String(rules)} icon={<ListFilter />} />
            <KpiCard label="Hidden by a module" value={String(all.filter((s) => !s.module_on).length)} icon={<EyeOff />} />
          </div>
          {!data.can_edit && <p className="mb-4 text-[12.5px] text-fg-3">You can view the layout. Changing it needs the settings permission (Super Admin).</p>}
          <div className="grid grid-cols-1 items-start gap-4 xl:grid-cols-12">
            <div className="space-y-4 xl:col-span-7">
              <Segmented value={kind} onChange={setKind} options={[{ value: "all", label: "Everything" }, { value: "page", label: "Pages" }, { value: "block", label: "Dashboard blocks" }]} aria-label="Show" />
              {SECTION_GROUPS.map((g) => {
                const defs = SECTIONS.filter((x) => x.group === g.key && (kind === "all" || x.kind === kind));
                const rows = defs.map((d) => ({ d, s: byKey.get(d.key) })).filter((x): x is { d: SectionDef; s: Section } => !!x.s);
                if (!rows.length) return null;
                return (
                  <Card key={g.key}>
                    <CardHeader title={g.name} subtitle={`${rows.filter((x) => x.s.enabled).length} of ${rows.length} shown to everyone`} icon={g.key === "profile" ? <UserRound /> : <LayoutGrid />} />
                    <div className="px-4 pb-2 pt-1 sm:px-6">
                      {rows.map(({ d, s }) => (
                        <SectionRow key={s.key} s={s} def={d} canEdit={data.can_edit} onSet={(v) => set(s, v)} onRules={() => setOpen(s.key)} />
                      ))}
                    </div>
                  </Card>
                );
              })}
            </div>
            <div className="xl:sticky xl:top-24 xl:col-span-5">
              <PreviewCard groups={groups} version={data.version} />
            </div>
          </div>
          <RulesDrawer s={openSection} def={SECTIONS.find((x) => x.key === open)} canEdit={data.can_edit} groups={groups} onClose={() => setOpen(null)} confirm={confirm.ask} reload={reload} />
        </>
      )}
      {confirm.dialog}
    </div>
  );
}
