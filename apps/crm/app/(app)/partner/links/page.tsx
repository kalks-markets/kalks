"use client";

import * as React from "react";
import { QRCodeSVG } from "qrcode.react";
import { ChevronDown, Code2, Copy, Download, ExternalLink, Eye, Globe, Link2, MoreHorizontal, MousePointerClick, Pause, Play, Plus, QrCode, UserPlus, Wallet } from "lucide-react";
import { toast } from "sonner";
import {
  Button,
  Card,
  CardHeader,
  Chip,
  CopyButton,
  DataTable,
  Dialog,
  Field,
  IconButton,
  Input,
  KpiCard,
  Logo,
  Menu,
  PageHeader,
  Reveal,
  Segmented,
  Sparkline,
  Toggle,
  cn,
  formatCompact,
  formatMoney,
  type Column,
} from "@/components/kit";
import { ME } from "@kalks/mock";
import { BANNERS, BANNER_SIZES, CAMPAIGNS, LANDING_PAGES, type Campaign } from "@kalks/mock/partner";
import { IS_DEMO } from "@kalks/mock/mode";
import { LivePartnerLinks } from "@/components/partner/live/links";

const BASE = ME.referralLink; // https://app.kalkstrade.com/r/ARJUN24
const linkFor = (c: Pick<Campaign, "slug">) => `${BASE}/${c.slug}`;
const short = (u: string) => u.replace("https://", "");

/* ------------------------------------------------------------------ */

function Funnel({ c }: { c: Campaign }) {
  const steps = [
    { label: "Clicks", v: c.clicks, cls: "bg-fg-3" },
    { label: "Sign-ups", v: c.signups, cls: "bg-gold" },
    { label: "FTDs", v: c.ftds, cls: "bg-ember" },
  ];
  return (
    <div className="w-[190px]">
      <div className="space-y-1">
        {steps.map((s) => (
          <div key={s.label} className="flex items-center gap-2">
            <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-surface-3">
              <div className={cn("h-full rounded-full", s.cls)} style={{ width: `${Math.max(4, Math.sqrt(s.v / c.clicks) * 100)}%` }} />
            </div>
            <span className="k-num w-11 text-right text-[11px] text-fg-2">{formatCompact(s.v)}</span>
          </div>
        ))}
      </div>
      <div className="k-num mt-1 flex gap-2 text-[10.5px] text-fg-3">
        <span>
          CR <span className="text-gold">{((c.signups / c.clicks) * 100).toFixed(1)}%</span>
        </span>
        <span>
          FTD <span className="text-ember">{((c.ftds / c.signups) * 100).toFixed(1)}%</span>
        </span>
      </div>
    </div>
  );
}

function CreateLinkDialog({ open, onOpenChange, onCreate }: { open: boolean; onOpenChange: (o: boolean) => void; onCreate: (c: Campaign) => void }) {
  const [name, setName] = React.useState("Diwali gold promo");
  const [landing, setLanding] = React.useState(LANDING_PAGES[1]!.id);
  const [src, setSrc] = React.useState("instagram");
  const [medium, setMedium] = React.useState("social");
  const [camp, setCamp] = React.useState("diwali26");
  const slug = name.toLowerCase().trim().replace(/[^a-z0-9]+/g, "-").replace(/(^-|-$)/g, "") || "campaign";
  const lp = LANDING_PAGES.find((l) => l.id === landing)!;
  const url = `${BASE}/${slug}?utm_source=${encodeURIComponent(src)}&utm_medium=${encodeURIComponent(medium)}&utm_campaign=${encodeURIComponent(camp)}`;
  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      width={620}
      title="Create campaign link"
      description="Track clicks, sign-ups and first deposits per campaign."
      footer={
        <>
          <Button variant="ghost" size="sm" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            variant="ember"
            disabled={!name.trim()}
            onClick={() => {
              onCreate({ id: `cp${Date.now()}`, name, slug, landing: lp.name, utmSource: src, utmMedium: medium, utmCampaign: camp, clicks: 0, signups: 0, ftds: 0, deposits: 0, lots: 0, createdAt: "2026-09-24", active: true, trend: [0, 0] });
              navigator.clipboard?.writeText(url).catch(() => {});
              toast.success("Campaign link created & copied", { description: short(`${BASE}/${slug}`) });
              onOpenChange(false);
            }}
          >
            <Plus /> Create link
          </Button>
        </>
      }
    >
      <div className="space-y-5">
        <Field label="Campaign name">
          <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="e.g. YouTube gold webinar" />
        </Field>
        <div>
          <div className="mb-2 text-[12.5px] font-medium text-fg-2">Landing page</div>
          <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
            {LANDING_PAGES.map((l) => (
              <button
                key={l.id}
                type="button"
                onClick={() => setLanding(l.id)}
                className={cn("flex items-center gap-3 rounded-[14px] border p-2 text-left transition-colors", landing === l.id ? "border-ember/50 bg-ember-soft" : "border-line bg-surface-2 hover:bg-surface-3/60")}
              >
                <img src={l.photo} alt="" className="h-10 w-14 shrink-0 rounded-lg object-cover" />
                <span className="min-w-0">
                  <span className="block truncate text-[13px] font-medium">{l.name}</span>
                  <span className="block truncate font-mono text-[11px] text-fg-3">kalkstrade.com{l.path}</span>
                </span>
              </button>
            ))}
          </div>
        </div>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          <Field label="utm_source">
            <Input value={src} onChange={(e) => setSrc(e.target.value)} />
          </Field>
          <Field label="utm_medium">
            <Input value={medium} onChange={(e) => setMedium(e.target.value)} />
          </Field>
          <Field label="utm_campaign">
            <Input value={camp} onChange={(e) => setCamp(e.target.value)} />
          </Field>
        </div>
        <div className="rounded-[14px] border border-gold/25 bg-gold-soft px-4 py-3">
          <div className="text-[11px] uppercase tracking-wider text-gold">Preview</div>
          <div className="mt-1 flex items-start gap-2">
            <span className="min-w-0 flex-1 break-all font-mono text-[12.5px] text-fg">{short(url)}</span>
            <CopyButton value={url} label="Link" />
          </div>
          <div className="mt-1 text-[11.5px] text-fg-3">Opens {lp.name} · attribution is permanent once the visitor signs up</div>
        </div>
      </div>
    </Dialog>
  );
}

/* ------------------------------------------------------------------ */

function BannerArt({ b, size }: { b: (typeof BANNERS)[number]; size: (typeof BANNER_SIZES)[number]["key"] }) {
  const wide = size === "728x90";
  const square = size === "1080x1080";
  return (
    <div className={cn("relative w-full overflow-hidden rounded-[12px] ring-1 ring-white/10", wide ? "aspect-[728/90]" : square ? "aspect-square" : "aspect-[300/250]")}>
      <img src={b.photo} alt="" className="absolute inset-0 size-full object-cover" />
      <div className={cn("absolute inset-0", wide ? "bg-gradient-to-r from-black via-black/75 to-[color-mix(in_oklab,var(--k-ember)_78%,#000)]/30" : "bg-gradient-to-t from-black via-black/60 to-[color-mix(in_oklab,var(--k-ember)_78%,#000)]/20")} />
      <div className={cn("absolute inset-0 flex", wide ? "items-center justify-between gap-3 px-4" : "flex-col justify-between p-4")}>
        <Logo height={wide ? 13 : square ? 20 : 16} className={cn("shrink-0 text-white", !wide && "self-start")} />
        <div className={cn(wide ? "flex flex-1 items-center justify-between gap-3" : "")}>
          <div className={cn("font-semibold leading-tight text-white", wide ? "truncate text-[13px] sm:text-[15px]" : square ? "text-[20px]" : "text-[16px]")}>{b.title}</div>
          <span className={cn("k-ember-btn inline-flex shrink-0 items-center rounded-full font-medium", wide ? "h-6 px-3 text-[11px]" : "mt-3 h-8 px-4 text-[12px]")}>{b.cta}</span>
        </div>
      </div>
    </div>
  );
}

function BannersCard() {
  const [size, setSize] = React.useState<(typeof BANNER_SIZES)[number]["key"]>("300x250");
  const sz = BANNER_SIZES.find((s) => s.key === size)!;
  const html = (b: (typeof BANNERS)[number]) =>
    `<a href="${BASE}/banner-${b.id}?utm_source=banner&utm_medium=${size}" target="_blank" rel="noopener"><img src="https://cdn.kalkstrade.com/partners/${b.id}-${size}.jpg" width="${sz.w}" height="${sz.h}" alt="${b.title}" /></a>`;
  return (
    <Card className="h-full">
      <CardHeader
        title="Banners"
        subtitle="Tracked with your code · updated monthly"
        action={<Segmented size="xs" value={size} onChange={setSize} options={BANNER_SIZES.map((s) => ({ value: s.key, label: s.key.replace("x", "×") }))} />}
      />
      <div className={cn("grid gap-4 px-4 pb-6 pt-5 sm:px-6", size === "728x90" ? "grid-cols-1" : "grid-cols-1 sm:grid-cols-2 lg:grid-cols-3")}>
        {BANNERS.map((b) => (
          <div key={b.id} className="group">
            <BannerArt b={b} size={size} />
            <div className="mt-2 flex items-center justify-between gap-2">
              <div className="min-w-0">
                <div className="truncate text-[12.5px] font-medium">{b.theme}</div>
                <div className="k-num text-[11px] text-fg-3">
                  {sz.w}×{sz.h}
                </div>
              </div>
              <div className="flex shrink-0 gap-1.5">
                <Button
                  size="xs"
                  variant="surface"
                  onClick={() => {
                    navigator.clipboard?.writeText(html(b)).catch(() => {});
                    toast.success("Banner HTML copied", { description: "Paste it into your website or blog." });
                  }}
                >
                  <Code2 /> Copy HTML
                </Button>
                <IconButton size="sm" aria-label="Download banner" onClick={() => toast.success("Banner downloaded", { description: `kalks-${b.id}-${size}.jpg` })}>
                  <Download />
                </IconButton>
              </div>
            </div>
          </div>
        ))}
      </div>
    </Card>
  );
}

function QrCard({ campaigns }: { campaigns: Campaign[] }) {
  const [cid, setCid] = React.useState(campaigns[0]!.id);
  const [theme, setTheme] = React.useState<"light" | "dark">("light");
  const [size, setSize] = React.useState<"S" | "M" | "L">("M");
  const [logo, setLogo] = React.useState(true);
  const c = campaigns.find((x) => x.id === cid) ?? campaigns[0]!;
  const px = { S: 150, M: 190, L: 230 }[size];
  const fg = theme === "light" ? "#0b0b0e" : "#ffffff";
  const bg = theme === "light" ? "#ffffff" : "#111114";
  return (
    <Card className="flex h-full flex-col">
      <CardHeader title="QR generator" subtitle="For flyers, events and screens" icon={<QrCode />} />
      <div className="flex flex-1 flex-col gap-4 px-4 pb-5 pt-4 sm:px-6">
        <Menu
          align="start"
          width={300}
          trigger={
            <button className="flex h-11 w-full items-center gap-2 rounded-[14px] border border-line bg-surface-2 px-3.5 text-left text-[13px] hover:bg-surface-3/60">
              <Link2 className="size-4 text-fg-3" />
              <span className="min-w-0 flex-1 truncate">{c.name}</span>
              <ChevronDown className="size-4 text-fg-3" />
            </button>
          }
          items={campaigns.map((x) => ({ label: x.name, onSelect: () => setCid(x.id), hint: x.id === cid ? "Selected" : undefined }))}
        />
        <div className="grid place-items-center rounded-[18px] border border-line py-6" style={{ background: theme === "light" ? "radial-gradient(circle at 50% 30%, color-mix(in oklab, var(--k-ember) 18%, transparent), transparent 70%)" : undefined }}>
          <div className="rounded-[16px] p-3.5 shadow-[0_20px_50px_-20px_color-mix(in_oklab,var(--k-ember)_60%,transparent)] transition-all" style={{ background: bg }}>
            <QRCodeSVG value={linkFor(c)} size={px} level="H" fgColor={fg} bgColor={bg} imageSettings={logo ? { src: "/assets/brand/kalks-mark.svg", width: px * 0.2, height: px * 0.2, excavate: true } : undefined} />
          </div>
          <div className="mt-3 max-w-full truncate px-4 font-mono text-[11.5px] text-fg-2">{short(linkFor(c))}</div>
        </div>
        <div className="grid grid-cols-2 gap-3">
          <Segmented
            size="xs"
            value={theme}
            onChange={setTheme}
            options={[
              { value: "light", label: "Light" },
              { value: "dark", label: "Dark" },
            ]}
            className="w-full justify-between"
          />
          <Segmented size="xs" value={size} onChange={setSize} options={["S", "M", "L"] as const} className="w-full justify-between" />
        </div>
        <div className="flex items-center justify-between text-[13px] text-fg-2">
          Kalks mark in centre
          <Toggle checked={logo} onChange={setLogo} label="Logo" />
        </div>
        <div className="mt-auto grid grid-cols-2 gap-2">
          <Button variant="surface" size="sm" onClick={() => toast.success("QR code saved", { description: `qr-${c.slug}.svg · vector` })}>
            <Download /> SVG
          </Button>
          <Button variant="ember" size="sm" onClick={() => toast.success("QR code saved", { description: `qr-${c.slug}.png · ${px * 4}×${px * 4}` })}>
            <Download /> PNG
          </Button>
        </div>
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */

function DemoPartnerLinksPage() {
  const [campaigns, setCampaigns] = React.useState<Campaign[]>(CAMPAIGNS);
  const [create, setCreate] = React.useState(false);
  const [paused, setPaused] = React.useState<Record<string, boolean>>({});
  const clicks = campaigns.reduce((s, c) => s + c.clicks, 0);
  const signups = campaigns.reduce((s, c) => s + c.signups, 0);
  const ftds = campaigns.reduce((s, c) => s + c.ftds, 0);
  const deposits = campaigns.reduce((s, c) => s + c.deposits, 0);

  const columns: Column<Campaign>[] = [
    {
      key: "name",
      header: "Campaign",
      cell: (c) => (
        <span className="block min-w-0">
          <span className="flex items-center gap-2 text-[13.5px] font-medium">
            {c.name}
            {(!c.active || paused[c.id]) && <Chip size="sm">Paused</Chip>}
            {c.clicks === 0 && <Chip size="sm" tone="ember">New</Chip>}
          </span>
          <span className="mt-0.5 flex items-center gap-1 font-mono text-[11.5px] text-fg-3">
            {short(linkFor(c))}
            <CopyButton value={linkFor(c)} label="Campaign link" className="size-5" />
          </span>
        </span>
      ),
      sort: (c) => c.name,
      width: "280px",
    },
    {
      key: "landing",
      header: "Landing · UTM",
      cell: (c) => (
        <span className="block">
          <span className="text-[13px]">{c.landing}</span>
          <span className="block font-mono text-[11px] text-fg-3">
            {c.utmSource}/{c.utmMedium}
          </span>
        </span>
      ),
      hideOn: "lg",
    },
    { key: "funnel", header: "Clicks → sign-ups → FTDs", cell: (c) => (c.clicks ? <Funnel c={c} /> : <span className="text-[12px] text-fg-3">Waiting for first click</span>), sort: (c) => c.ftds },
    { key: "dep", header: "Deposits", align: "right", cell: (c) => <span className="k-num">{c.deposits ? formatMoney(c.deposits, "USD", 0) : "—"}</span>, sort: (c) => c.deposits },
    { key: "lots", header: "Lots", align: "right", cell: (c) => <span className="k-num">{c.lots ? c.lots.toLocaleString("en-US", { maximumFractionDigits: 1 }) : "—"}</span>, sort: (c) => c.lots, hideOn: "md" },
    { key: "trend", header: "30d", align: "right", cell: (c) => (c.trend.length > 2 ? <Sparkline data={c.trend} width={72} height={24} className="ml-auto" /> : <span className="text-fg-3">—</span>), hideOn: "md" },
    {
      key: "act",
      header: "",
      align: "right",
      cell: (c) => {
        const isPaused = !c.active || paused[c.id];
        return (
          <Menu
            trigger={
              <IconButton size="sm" aria-label="Campaign actions">
                <MoreHorizontal />
              </IconButton>
            }
            items={[
              {
                label: "Copy link",
                icon: <Copy />,
                onSelect: () => {
                  navigator.clipboard?.writeText(linkFor(c)).catch(() => {});
                  toast.success("Campaign link copied", { description: short(linkFor(c)) });
                },
              },
              { label: "Open landing page", icon: <ExternalLink />, onSelect: () => toast("Opening landing page", { description: `${c.landing} with your tracking code` }) },
              "sep",
              {
                label: isPaused ? "Resume tracking" : "Pause link",
                icon: isPaused ? <Play /> : <Pause />,
                onSelect: () => {
                  setPaused((p) => ({ ...p, [c.id]: !isPaused }));
                  toast.success(isPaused ? "Campaign resumed" : "Campaign paused", { description: isPaused ? "New clicks are tracked again." : "Visitors land on the homepage without campaign tracking. Existing clients stay attributed to you." });
                },
              },
            ]}
          />
        );
      },
    },
  ];

  return (
    <div className="pb-24">
      <PageHeader
        title="Links & materials"
        subtitle="Tracked campaign links, ready-made banners, QR codes and landing pages."
        actions={
          <Button variant="ember" size="lg" shimmer onClick={() => setCreate(true)}>
            <Plus /> Create link
          </Button>
        }
      />

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <KpiCard label="Clicks · all time" icon={<MousePointerClick />} value={<span className="k-num">{clicks.toLocaleString()}</span>} chip={`${campaigns.length} campaigns`} />
        <KpiCard label="Sign-ups" icon={<UserPlus />} value={<span className="k-num">{signups.toLocaleString()}</span>} chip={`${((signups / clicks) * 100).toFixed(1)}% of clicks`} chipTone="gold" delay={0.04} />
        <KpiCard label="First deposits" icon={<Wallet />} value={<span className="k-num">{ftds}</span>} chip={`${((ftds / signups) * 100).toFixed(1)}% of sign-ups`} chipTone="ember" delay={0.08} />
        <KpiCard label="Deposits generated" icon={<Globe />} value={<span className="k-num">{formatMoney(deposits, "USD", 0)}</span>} chip={`${formatMoney(deposits / ftds, "USD", 0)} avg FTD`} chipTone="up" delay={0.12} />
      </div>

      <Reveal delay={0.08} className="mt-4 block">
        <Card>
          <CardHeader title="Campaign links" subtitle="Each link carries your code — ARJUN24 — plus its own tracking" icon={<Link2 />} />
          <div className="px-4 pb-5 pt-4 sm:px-6">
            <DataTable columns={columns} rows={campaigns} rowKey={(c) => c.id} pageSize={8} search={(c) => `${c.name} ${c.slug} ${c.utmSource}`} searchPlaceholder="Search campaigns…" exportName="kalks-campaigns" />
          </div>
        </Card>
      </Reveal>

      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal delay={0.05} className="xl:col-span-8">
          <BannersCard />
        </Reveal>
        <Reveal delay={0.1} className="xl:col-span-4">
          <QrCard campaigns={campaigns} />
        </Reveal>
      </div>

      <Reveal delay={0.1} className="mt-4 block">
        <Card>
          <CardHeader title="Landing pages" subtitle="Localised pages that convert — pick one per campaign" icon={<Globe />} />
          <div className="grid grid-cols-1 gap-4 px-4 pb-6 pt-5 sm:grid-cols-2 sm:px-6 xl:grid-cols-3">
            {LANDING_PAGES.map((l) => (
              <div key={l.id} className="k-row overflow-hidden transition-colors hover:border-[var(--k-border-top)]">
                <div className="relative h-32">
                  <img src={l.photo} alt="" className="absolute inset-0 size-full object-cover" />
                  <div className="absolute inset-0 bg-gradient-to-t from-surface-2 via-black/40 to-transparent" />
                  <Chip size="sm" tone="up" className="absolute right-3 top-3 backdrop-blur">
                    {l.conv}% conversion
                  </Chip>
                </div>
                <div className="px-4 pb-4 pt-1">
                  <div className="flex items-center justify-between gap-2">
                    <div className="min-w-0">
                      <div className="truncate text-[14px] font-medium">{l.name}</div>
                      <div className="truncate font-mono text-[11.5px] text-fg-3">kalkstrade.com{l.path}?ref=ARJUN24</div>
                    </div>
                    <span className="k-num shrink-0 text-right text-[12px] text-fg-2">{l.visits.toLocaleString()} visits</span>
                  </div>
                  <div className="mt-3 flex flex-wrap gap-1">
                    {l.lang.map((x) => (
                      <span key={x} className="rounded-md bg-surface-3 px-1.5 py-0.5 font-mono text-[10.5px] text-fg-2">
                        {x}
                      </span>
                    ))}
                  </div>
                  <div className="mt-3 grid grid-cols-2 gap-2">
                    <Button size="xs" variant="surface" onClick={() => toast("Opening preview", { description: `kalkstrade.com${l.path} in a new tab` })}>
                      <Eye /> Preview
                    </Button>
                    <Button
                      size="xs"
                      variant="surface"
                      onClick={() => {
                        navigator.clipboard?.writeText(`https://kalkstrade.com${l.path}?ref=ARJUN24`).catch(() => {});
                        toast.success("Landing link copied");
                      }}
                    >
                      <Copy /> Copy link
                    </Button>
                  </div>
                </div>
              </div>
            ))}
          </div>
        </Card>
      </Reveal>

      <CreateLinkDialog open={create} onOpenChange={setCreate} onCreate={(c) => setCampaigns((x) => [c, ...x])} />
    </div>
  );
}

export default function Page() {
  return IS_DEMO ? <DemoPartnerLinksPage /> : <LivePartnerLinks />;
}
