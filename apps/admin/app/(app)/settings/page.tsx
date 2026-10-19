"use client";

import * as React from "react";
import { toast } from "sonner";
import { Bell, Building2, Clock, ExternalLink, Globe2, Headset, Languages, Mail, MessageCircle, Palette, Phone, Plus, RefreshCw, Upload, Wallet } from "lucide-react";
import { Button, Card, CardHeader, Chip, Dialog, Field, Flag, Input, PageHeader, Reveal, cn } from "@kalks/ui";
import { SET_DOMAINS, SET_GENERAL, SET_LANGUAGES, SET_TIMEZONES, type SetDomain } from "@kalks/mock/admin-platform-settings";
import { BrandImg, SaveBar, SelectInput } from "@/components/settings/kit";

const PALETTES = [
  { name: "Kalks Ember", primary: "#FF5A1F", accent: "#E9B949" },
  { name: "Aurum Gold", primary: "#E9B949", accent: "#FF8A3D" },
  { name: "Dunes Green", primary: "#22C55E", accent: "#E9B949" },
  { name: "Nova Sky", primary: "#38BDF8", accent: "#E9B949" },
];

function hexOk(h: string) {
  return /^#[0-9a-f]{6}$/i.test(h);
}

function shade(hex: string, amt: number) {
  if (!hexOk(hex)) return hex;
  const n = parseInt(hex.slice(1), 16);
  const f = (c: number) => Math.max(0, Math.min(255, Math.round(c + amt)));
  const r = f(n >> 16),
    g = f((n >> 8) & 0xff),
    b = f(n & 0xff);
  return `#${((r << 16) | (g << 8) | b).toString(16).padStart(6, "0")}`;
}

/* ------------------------------------------------------------------ */

function LogoSlot({ label, hint, src, bg, onFile, invert, size = "wide" }: { label: string; hint: string; src: string; bg: "dark" | "light"; onFile: (url: string, name: string) => void; invert?: boolean; size?: "wide" | "square" }) {
  const ref = React.useRef<HTMLInputElement>(null);
  return (
    <div className="flex flex-col gap-2">
      <button
        type="button"
        onClick={() => ref.current?.click()}
        className={cn(
          "group relative grid place-items-center overflow-hidden rounded-[16px] border border-line transition-colors hover:border-ember/40",
          size === "wide" ? "h-28" : "h-28",
          bg === "dark" ? "bg-[radial-gradient(120%_120%_at_50%_0%,#2a1a12,#0b0b0e_60%)]" : "bg-[#F6F4F1]",
        )}
      >
        <BrandImg src={src} color={bg === "dark" ? "#F5F5F7" : "#0E0E12"} className={cn("max-w-[70%]", size === "wide" ? "h-8" : "h-12", invert && !src.startsWith("/assets/") && "invert")} />
        <span className="absolute inset-0 grid place-items-center bg-black/60 opacity-0 backdrop-blur-sm transition-opacity group-hover:opacity-100">
          <span className="flex items-center gap-1.5 text-[12.5px] font-medium text-white">
            <Upload className="size-3.5" /> Replace
          </span>
        </span>
      </button>
      <input
        ref={ref}
        type="file"
        accept="image/svg+xml,image/png"
        className="hidden"
        onChange={(e) => {
          const f = e.target.files?.[0];
          if (f) onFile(URL.createObjectURL(f), f.name);
        }}
      />
      <div>
        <div className="text-[12.5px] font-medium text-fg-2">{label}</div>
        <div className="text-[11.5px] text-fg-3">{hint}</div>
      </div>
    </div>
  );
}

function ColorField({ label, value, onChange }: { label: string; value: string; onChange: (v: string) => void }) {
  return (
    <Field label={label} hint={hexOk(value) ? undefined : "Invalid hex"}>
      <div className="flex h-11 items-center gap-2 rounded-[14px] border border-line bg-surface-2 pl-1.5 pr-3.5 focus-within:border-ember/50 focus-within:ring-4 focus-within:ring-ember/10">
        <label className="relative size-8 shrink-0 cursor-pointer overflow-hidden rounded-[10px] ring-1 ring-white/15" style={{ background: hexOk(value) ? value : "transparent" }}>
          <input type="color" value={hexOk(value) ? value : "#000000"} onChange={(e) => onChange(e.target.value.toUpperCase())} className="absolute inset-0 cursor-pointer opacity-0" />
        </label>
        <input value={value} onChange={(e) => onChange(e.target.value)} className="min-w-0 flex-1 bg-transparent font-mono text-[13px] uppercase text-fg outline-none" />
      </div>
    </Field>
  );
}

function BrandPreview({ primary, accent, logo }: { primary: string; accent: string; logo: string }) {
  const p = hexOk(primary) ? primary : "#FF5A1F";
  const a = hexOk(accent) ? accent : "#E9B949";
  return (
    <div className="relative overflow-hidden rounded-[18px] border border-line bg-bg">
      <div className="pointer-events-none absolute inset-x-0 -top-24 h-56 opacity-70" style={{ background: `radial-gradient(420px 160px at 50% 0%, ${p}88, transparent 70%)` }} />
      <div className="relative flex items-center justify-between gap-3 px-4 pt-4">
        <BrandImg src={logo} className="h-4" />
        <div className="hidden items-center gap-0.5 rounded-full border border-line bg-surface/70 p-0.5 sm:flex">
          {["Dashboard", "Accounts", "Wallet"].map((t, i) => (
            <span key={t} className={cn("rounded-full px-2.5 py-1 text-[10.5px]", i === 0 ? "bg-surface-3 text-fg" : "text-fg-3")}>
              {t}
            </span>
          ))}
        </div>
        <span className="rounded-full px-3 py-1.5 text-[10.5px] font-semibold text-white" style={{ background: `linear-gradient(135deg, ${shade(p, 30)}, ${shade(p, -25)})`, boxShadow: `0 6px 16px -6px ${p}` }}>
          Deposit
        </span>
      </div>
      <div className="relative px-4 pb-4 pt-5">
        <div className="text-[15px] font-medium">Good evening, Arjun</div>
        <div className="mt-3 grid grid-cols-2 gap-2">
          <div className="rounded-[14px] border border-white/10 p-3" style={{ background: `linear-gradient(160deg, ${p}55, #111114 70%)` }}>
            <div className="text-[9.5px] font-semibold uppercase tracking-wider text-fg-2">Total equity</div>
            <div className="k-num mt-1.5 text-[18px] font-semibold">
              $48,915<span className="opacity-40">.60</span>
            </div>
            <span className="mt-2 inline-flex rounded-full bg-up-soft px-1.5 py-0.5 text-[9.5px] font-medium text-up">+1.84% today</span>
          </div>
          <div className="k-card rounded-[14px] p-3">
            <div className="text-[9.5px] font-semibold uppercase tracking-wider text-fg-2">IB level</div>
            <div className="mt-1.5 text-[18px] font-semibold" style={{ color: a }}>
              Gold
            </div>
            <div className="mt-2 h-1 overflow-hidden rounded-full bg-surface-3">
              <div className="h-full w-2/3 rounded-full" style={{ background: a }} />
            </div>
          </div>
        </div>
        <svg viewBox="0 0 300 60" className="mt-3 h-14 w-full">
          <defs>
            <linearGradient id="bp-fill" x1="0" x2="0" y1="0" y2="1">
              <stop offset="0" stopColor={a} stopOpacity="0.3" />
              <stop offset="1" stopColor={a} stopOpacity="0" />
            </linearGradient>
          </defs>
          <path d="M0,48 C30,44 50,30 80,34 S130,18 160,24 S220,6 250,14 S290,4 300,6 L300,60 L0,60Z" fill="url(#bp-fill)" />
          <path d="M0,48 C30,44 50,30 80,34 S130,18 160,24 S220,6 250,14 S290,4 300,6" fill="none" stroke={a} strokeWidth="1.8" />
        </svg>
        <div className="mt-2 flex gap-2">
          <span className="flex-1 rounded-full py-2 text-center text-[11px] font-semibold text-white" style={{ background: `linear-gradient(135deg, ${shade(p, 30)}, ${shade(p, -25)})` }}>
            Open terminal
          </span>
          <span className="flex-1 rounded-full border border-line bg-surface-2 py-2 text-center text-[11px] text-fg">Withdraw</span>
        </div>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */

type Form = typeof SET_GENERAL;

export default function GeneralSettingsPage() {
  const [form, setForm] = React.useState<Form>(SET_GENERAL);
  const [saved, setSaved] = React.useState<Form>(SET_GENERAL);
  const [logo, setLogo] = React.useState("/assets/brand/kalks-logo.svg");
  const [mark, setMark] = React.useState("/assets/brand/kalks-mark.svg");
  const [domains, setDomains] = React.useState<SetDomain[]>(SET_DOMAINS);
  const [addOpen, setAddOpen] = React.useState(false);
  const [newHost, setNewHost] = React.useState("");
  const dirty = JSON.stringify(form) !== JSON.stringify(saved);
  const set = <K extends keyof Form>(k: K, v: Form[K]) => setForm((f) => ({ ...f, [k]: v }));

  const save = () => {
    setSaved(form);
    toast.success("Settings saved", { description: "Changes are live for new sessions · logged to the audit trail" });
  };

  const verify = (host: string) => {
    toast.loading(`Checking DNS for ${host}…`, { id: host });
    setTimeout(() => {
      setDomains((d) => d.map((x) => (x.host === host ? { ...x, dns: "verified", ssl: "active", expires: "2026-12-23" } : x)));
      toast.success(`${host} verified`, { id: host, description: "CNAME found · Let's Encrypt certificate issued" });
    }, 1200);
  };

  return (
    <div className="pb-24">
      <PageHeader
        title="General & branding"
        subtitle="Brand identity, domains and regional defaults for Kalks Markets. White-label tenants override these in Brokers."
        actions={
          <>
            <Button variant="surface" onClick={() => toast("Opening app.kalkstrade.com preview in a new tab")}>
              <ExternalLink /> Preview client area
            </Button>
            <Button variant="ember" onClick={save} disabled={!dirty}>
              Save changes
            </Button>
          </>
        }
      />

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
        {/* Branding */}
        <Reveal className="xl:col-span-7">
          <Card className="h-full">
            <CardHeader icon={<Palette />} title="Brand identity" subtitle="Logo, mark and colours used across the Client Area, terminal and emails" />
            <div className="space-y-6 px-4 pb-6 pt-5 sm:px-6">
              <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
                <LogoSlot
                  label="Logo · dark"
                  hint="SVG/PNG · 400×96 · white wordmark"
                  src={logo}
                  bg="dark"
                  onFile={(u, n) => {
                    setLogo(u);
                    toast.success("Logo uploaded", { description: `${n} · preview updated` });
                  }}
                />
                <LogoSlot label="Logo · light" hint="Auto-generated from dark logo" src={logo} bg="light" invert onFile={(u, n) => { setLogo(u); toast.success("Logo uploaded", { description: n }); }} />
                <LogoSlot
                  label="Mark & favicon"
                  hint="Square · 512×512"
                  src={mark}
                  bg="dark"
                  size="square"
                  onFile={(u, n) => {
                    setMark(u);
                    toast.success("Mark uploaded", { description: `${n} · favicons regenerated (16–512px)` });
                  }}
                />
              </div>

              <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                <ColorField label="Primary colour" value={form.primary} onChange={(v) => set("primary", v)} />
                <ColorField label="Accent colour" value={form.accent} onChange={(v) => set("accent", v)} />
              </div>

              <div>
                <div className="mb-2 text-[12.5px] font-medium text-fg-2">Presets</div>
                <div className="flex flex-wrap gap-2">
                  {PALETTES.map((p) => {
                    const on = form.primary.toUpperCase() === p.primary && form.accent.toUpperCase() === p.accent;
                    return (
                      <button
                        key={p.name}
                        type="button"
                        onClick={() => setForm((f) => ({ ...f, primary: p.primary, accent: p.accent }))}
                        className={cn("flex items-center gap-2 rounded-full border py-1.5 pl-1.5 pr-3 text-[12.5px] transition-colors", on ? "border-ember/40 bg-ember-soft text-fg" : "border-line bg-surface-2 text-fg-2 hover:bg-surface-3")}
                      >
                        <span className="flex -space-x-1.5">
                          <span className="size-5 rounded-full ring-2 ring-surface" style={{ background: p.primary }} />
                          <span className="size-5 rounded-full ring-2 ring-surface" style={{ background: p.accent }} />
                        </span>
                        {p.name}
                      </button>
                    );
                  })}
                </div>
              </div>

              <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                <Field label="Brand name">
                  <Input value={form.brandName} onChange={(e) => set("brandName", e.target.value)} />
                </Field>
                <Field label="Base currency" hint="Reporting & wallet">
                  <SelectInput value={form.baseCurrency} onChange={(v) => set("baseCurrency", v)} options={["USD", "USDT", "EUR"]} leading={<Wallet />} />
                </Field>
              </div>
            </div>
          </Card>
        </Reveal>

        <Reveal delay={0.05} className="xl:col-span-5">
          <Card className="flex h-full flex-col">
            <CardHeader title="Live preview" subtitle="How clients see the brand on app.kalkstrade.com" action={<Chip tone="up" dot>Live</Chip>} />
            <div className="flex-1 px-4 pb-6 pt-4 sm:px-6">
              <BrandPreview primary={form.primary} accent={form.accent} logo={logo} />
              <div className="mt-4 grid grid-cols-3 gap-2">
                {[
                  ["Primary", form.primary],
                  ["Accent", form.accent],
                  ["Canvas", "#07070A"],
                ].map(([k, v]) => (
                  <div key={k} className="k-row flex items-center gap-2.5 px-3 py-2.5">
                    <span className="size-6 shrink-0 rounded-lg ring-1 ring-white/15" style={{ background: hexOk(v!) ? v : "transparent" }} />
                    <div className="min-w-0">
                      <div className="text-[11px] text-fg-3">{k}</div>
                      <div className="truncate font-mono text-[11.5px] uppercase">{v}</div>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </Card>
        </Reveal>

        {/* Domains */}
        <Reveal delay={0.05} className="xl:col-span-7">
          <Card className="h-full">
            <CardHeader
              icon={<Globe2 />}
              title="Domains"
              subtitle="Point each host to edge.kalkstrade.com with a CNAME. SSL is issued automatically."
              action={
                <Button size="sm" variant="surface" onClick={() => setAddOpen(true)}>
                  <Plus /> Add domain
                </Button>
              }
            />
            <div className="mt-4 space-y-2 px-4 pb-6 sm:px-6">
              {domains.map((d) => (
                <div key={d.host} className="k-row flex flex-wrap items-center gap-3 px-4 py-3">
                  <span className={cn("size-2 shrink-0 rounded-full", d.dns === "verified" ? "bg-up shadow-[0_0_10px_var(--k-up)]" : "bg-warn")} />
                  <div className="min-w-0 flex-1">
                    <div className="truncate font-mono text-[13px]">{d.host}</div>
                    <div className="text-[11.5px] text-fg-3">{d.purpose}</div>
                  </div>
                  <Chip size="sm" tone={d.dns === "verified" ? "up" : "warn"}>
                    DNS {d.dns}
                  </Chip>
                  <Chip size="sm" tone={d.ssl === "active" ? "up" : "warn"}>
                    SSL {d.ssl}
                  </Chip>
                  <span className="k-num hidden w-24 text-right text-[11.5px] text-fg-3 sm:block">{d.expires === "—" ? "—" : `exp. ${d.expires.slice(5)}`}</span>
                  {d.dns === "pending" ? (
                    <Button size="xs" variant="ember" onClick={() => verify(d.host)}>
                      <RefreshCw /> Verify
                    </Button>
                  ) : (
                    <Button size="xs" variant="ghost" onClick={() => toast.success(`Certificate renewed for ${d.host}`, { description: "Valid for 90 days" })}>
                      Renew
                    </Button>
                  )}
                </div>
              ))}
            </div>
          </Card>
        </Reveal>

        {/* Regional */}
        <Reveal delay={0.1} className="xl:col-span-5">
          <Card className="h-full">
            <CardHeader icon={<Clock />} title="Regional defaults" subtitle="Server time drives statements, swaps and daily candles" />
            <div className="space-y-4 px-4 pb-6 pt-5 sm:px-6">
              <Field label="Server timezone" hint="Applies to all tenants">
                <SelectInput value={form.timezone} onChange={(v) => set("timezone", v)} options={SET_TIMEZONES} leading={<Clock />} />
              </Field>
              <Field label="Default language" hint="20+ available in Translations">
                <SelectInput value={form.defaultLanguage} onChange={(v) => set("defaultLanguage", v)} options={SET_LANGUAGES.map((l) => ({ value: l.code, label: `${l.label}${l.rtl ? "  · RTL" : ""}` }))} leading={<Languages />} />
              </Field>
              <div>
                <div className="mb-2 text-[12.5px] font-medium text-fg-2">Enabled client languages</div>
                <div className="flex flex-wrap gap-1.5">
                  {SET_LANGUAGES.map((l) => (
                    <Chip key={l.code} tone={l.code === form.defaultLanguage ? "ember" : "neutral"}>
                      <Flag country={l.flag} className="size-3.5" />
                      {l.code.toUpperCase()}
                    </Chip>
                  ))}
                  <Chip tone="neutral">+12 more</Chip>
                </div>
              </div>
              <div className="k-row flex items-center justify-between px-4 py-3">
                <span className="text-[12.5px] text-fg-3">Server clock now</span>
                <span className="font-mono text-[13px]">24 Sep 2026 · 18:45:12 {form.timezone}</span>
              </div>
            </div>
          </Card>
        </Reveal>

        {/* Company */}
        <Reveal delay={0.05} className="xl:col-span-7">
          <Card className="h-full">
            <CardHeader icon={<Building2 />} title="Company & regulation" subtitle="Shown in the footer, emails, statements and legal documents" />
            <div className="grid grid-cols-1 gap-4 px-4 pb-6 pt-5 sm:grid-cols-2 sm:px-6">
              <Field label="Legal entity name">
                <Input value={form.legalName} onChange={(e) => set("legalName", e.target.value)} />
              </Field>
              <Field label="Registration number">
                <Input value={form.regNumber} onChange={(e) => set("regNumber", e.target.value)} inputClassName="font-mono" />
              </Field>
              <Field label="Regulator & licence">
                <Input value={form.regulator} onChange={(e) => set("regulator", e.target.value)} />
              </Field>
              <Field label="Registered address">
                <Input value={form.address} onChange={(e) => set("address", e.target.value)} />
              </Field>
              <Field label="Risk warning (footer & emails)" className="sm:col-span-2" hint={`${form.riskWarning.length} / 400`}>
                <textarea
                  value={form.riskWarning}
                  onChange={(e) => set("riskWarning", e.target.value.slice(0, 400))}
                  rows={3}
                  className="resize-none rounded-[14px] border border-line bg-surface-2 px-3.5 py-3 text-[13px] leading-relaxed text-fg outline-none focus:border-ember/50 focus:ring-4 focus:ring-ember/10"
                />
              </Field>
            </div>
          </Card>
        </Reveal>

        {/* Support contacts */}
        <Reveal delay={0.1} className="xl:col-span-5">
          <Card className="h-full">
            <CardHeader icon={<Headset />} title="Support contacts" subtitle="Displayed in the Client Area help menu and emails" />
            <div className="space-y-4 px-4 pb-6 pt-5 sm:px-6">
              <Field label="Support email">
                <Input value={form.supportEmail} onChange={(e) => set("supportEmail", e.target.value)} leading={<Mail />} />
              </Field>
              <Field label="Compliance email">
                <Input value={form.complianceEmail} onChange={(e) => set("complianceEmail", e.target.value)} leading={<Bell />} />
              </Field>
              <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                <Field label="Phone">
                  <Input value={form.supportPhone} onChange={(e) => set("supportPhone", e.target.value)} leading={<Phone />} />
                </Field>
                <Field label="WhatsApp">
                  <Input value={form.whatsapp} onChange={(e) => set("whatsapp", e.target.value)} leading={<MessageCircle />} />
                </Field>
              </div>
              <Field label="Live chat hours">
                <Input value={form.liveChatHours} onChange={(e) => set("liveChatHours", e.target.value)} leading={<Clock />} />
              </Field>
            </div>
          </Card>
        </Reveal>
      </div>

      <Dialog
        open={addOpen}
        onOpenChange={setAddOpen}
        title="Add domain"
        description="Create a CNAME record pointing to edge.kalkstrade.com, then verify."
        footer={
          <>
            <Button variant="ghost" onClick={() => setAddOpen(false)}>
              Cancel
            </Button>
            <Button
              variant="ember"
              onClick={() => {
                if (!/^[a-z0-9-]+(\.[a-z0-9-]+)+$/i.test(newHost)) {
                  toast.error("Enter a valid hostname", { description: "e.g. partners.kalkstrade.com" });
                  return;
                }
                setDomains((d) => [...d, { host: newHost.toLowerCase(), purpose: "Custom", dns: "pending", ssl: "pending", expires: "—" }]);
                setAddOpen(false);
                setNewHost("");
                toast.success("Domain added", { description: "Add the DNS record, then click Verify" });
              }}
            >
              Add domain
            </Button>
          </>
        }
      >
        <div className="space-y-4">
          <Field label="Hostname">
            <Input value={newHost} onChange={(e) => setNewHost(e.target.value)} placeholder="partners.kalkstrade.com" inputClassName="font-mono" leading={<Globe2 />} />
          </Field>
          <div className="k-row overflow-x-auto p-4 font-mono text-[12px]">
            <div className="grid min-w-[380px] grid-cols-[70px_1fr_1fr] gap-y-2 text-fg-2">
              <span className="text-fg-3">TYPE</span>
              <span className="text-fg-3">NAME</span>
              <span className="text-fg-3">VALUE</span>
              <span>CNAME</span>
              <span className="truncate">{newHost || "partners.kalkstrade.com"}</span>
              <span>edge.kalkstrade.com</span>
              <span>TXT</span>
              <span className="truncate">_kalks-verify</span>
              <span className="truncate">kv=8f2a91c4e7</span>
            </div>
          </div>
        </div>
      </Dialog>

      <SaveBar dirty={dirty} onSave={save} onReset={() => setForm(saved)} />
    </div>
  );
}
