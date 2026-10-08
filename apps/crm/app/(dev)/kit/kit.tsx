"use client";

// Kalks 2 kit: every @kalks/ui component in one place (docs/design/KALKS2.md; compare with docs/design/kalks2-sample.html).

import * as React from "react";
import {
  ArrowRight,
  ArrowUpRight,
  Bell,
  CandlestickChart,
  Download,
  GraduationCap,
  Home,
  Layers,
  MoreHorizontal,
  Plus,
  Search,
  Settings,
  Sparkles,
  Trophy,
  Upload,
  Users,
  Wallet,
} from "lucide-react";
import {
  AccountCard,
  AccountPill,
  AddAccountCard,
  Button,
  Card,
  CardHeader,
  Chip,
  ChoiceCards,
  Delta,
  Dialog,
  Field,
  IconButton,
  IconRail,
  Input,
  Logo,
  LogoMark,
  Money,
  PhotoHero,
  Progress,
  RailAvatar,
  SearchButton,
  Segmented,
  Select,
  Sheet,
  showToast,
  Slider,
  Sparkline,
  StatTile,
  StatusChip,
  Stepper,
  TabBar,
  Table,
  Tabs,
  Tag,
  Td,
  Textarea,
  Th,
  ThemeSwitch,
  TitleCell,
  Toast,
  Toggle,
  TopBar,
  useThemeChoice,
} from "@kalks/ui";
import { BRAND, ROLES, tokenValue } from "@kalks/ui/tokens";

const SWATCHES: (keyof typeof ROLES | "red" | "yellow")[] = [
  "bg",
  "bg-2",
  "surface",
  "surface-2",
  "surface-3",
  "surface-4",
  "fg",
  "fg-2",
  "fg-3",
  "red",
  "red-tx",
  "red-soft",
  "yellow",
  "yellow-tx",
  "yellow-soft",
  "ink",
  "up",
  "up-tx",
  "up-soft",
  "down",
  "down-tx",
  "down-soft",
  "ok",
  "warn-tx",
  "focus",
];

const SPARK = [12, 14, 13, 15, 18, 17, 19, 22, 21, 24, 23, 26, 28, 27, 30];
const SPARK_DN = [30, 28, 29, 26, 25, 27, 24, 22, 23, 20, 21, 19, 17, 18, 16];

function Section({ id, n, title, note, children }: { id: string; n: string; title: string; note?: string; children: React.ReactNode }) {
  return (
    <section id={id} className="pt-12">
      <div className="mb-5 flex flex-col gap-2 md:flex-row md:items-end md:justify-between">
        <h2 className="k-d-wide text-[26px] md:text-[34px]">
          <span className="k-kicker mb-2.5 block text-red">{n}</span>
          {title}
        </h2>
        {note && <p className="max-w-[56ch] text-[14.5px] text-fg-2">{note}</p>}
      </div>
      {children}
    </section>
  );
}

function Panel({ title, caption, className, children }: { title: string; caption?: string; className?: string; children: React.ReactNode }) {
  return (
    <div className={`k-card p-5 md:p-6 ${className ?? ""}`}>
      <h3 className="mb-[18px] flex items-baseline justify-between gap-3 font-sans text-[16px] font-bold tracking-normal">
        {title}
        {caption && <span className="font-mono text-[12px] font-medium text-fg-3">{caption}</span>}
      </h3>
      {children}
    </div>
  );
}

const Row = ({ children, className }: { children: React.ReactNode; className?: string }) => <div className={`flex flex-wrap items-start gap-x-2.5 gap-y-3.5 ${className ?? ""}`}>{children}</div>;
const Cap = ({ children }: { children: React.ReactNode }) => <span className="mt-2 block text-center font-mono text-[11.5px] font-medium text-fg-3">{children}</span>;

export function Kit({ theme }: { theme?: "light" | "dark" }) {
  const { setChoice } = useThemeChoice();
  React.useEffect(() => {
    if (theme) setChoice(theme);
  }, [theme, setChoice]);

  const [seg, setSeg] = React.useState("market");
  const [tf, setTf] = React.useState("1h");
  const [scope, setScope] = React.useState("all");
  const [tab, setTab] = React.useState("positions");
  const [heroTab, setHeroTab] = React.useState("overview");
  const [tog1, setTog1] = React.useState(true);
  const [tog2, setTog2] = React.useState(false);
  const [risk, setRisk] = React.useState(35);
  const [chips, setChips] = React.useState("all");
  const [modal, setModal] = React.useState(false);
  const [sheet, setSheet] = React.useState(false);
  const [lots, setLots] = React.useState("0.50");
  const [product, setProduct] = React.useState<"cfd" | "options">("cfd");

  return (
    <div className="k-canvas min-h-dvh text-fg">
      {/* masthead */}
      <header className="sticky top-0 z-40 border-b border-line bg-[color-mix(in_srgb,var(--k-page)_82%,transparent)] backdrop-blur-[20px] backdrop-saturate-150">
        <div className="mx-auto flex h-16 max-w-[1440px] items-center gap-4 px-4 md:px-8">
          <LogoMark size={28} />
          <div className="min-w-0">
            <b className="block font-display text-[15px] font-extrabold leading-tight wdth-118">Kalks 2 kit</b>
            <span className="block font-mono text-[12px] text-fg-3">@kalks/ui · R1 design system</span>
          </div>
          <ThemeSwitch className="ms-auto" />
        </div>
      </header>

      <main className="mx-auto max-w-[1440px] px-4 pb-20 md:px-8">
        <div className="pt-10 md:pt-16">
          <h1 className="k-d text-[46px] md:text-[80px]">
            Controls you <span className="text-red">press</span>.
          </h1>
          <p className="mt-5 max-w-[60ch] text-[17px] font-medium leading-[1.45] text-fg-2 md:text-[19px]">
            Tokens, type, NeoPOP buttons and every shared component of the Client Area and Kalks Trader. Auto follows the device; Light and Dark override it.
          </p>
        </div>

        {/* ------------------------------------------------------------ colour */}
        <Section id="colour" n="01 — Colour" title="Black, red, yellow. Blue is up." note="Every role has a face and a text tone: bg-up / text-up, bg-red / text-red. Green appears only in status chips.">
          <div className="grid grid-cols-2 gap-3.5 sm:grid-cols-3 lg:grid-cols-5">
            {SWATCHES.map((k) => {
              const role = k in ROLES ? (k as keyof typeof ROLES) : null;
              return (
                <div key={k} className="flex flex-col gap-1.5">
                  <div className="flex h-14 items-end rounded-[14px] px-2.5 py-2 shadow-[inset_0_0_0_1px_rgba(0,0,0,.06)]" style={{ background: `var(--k-${k})` }} />
                  <b className="text-[12.5px] font-semibold">{k}</b>
                  <span className="truncate font-mono text-[11px] text-fg-3">{role ? `${tokenValue(role, "light")} · ${tokenValue(role, "dark")}` : BRAND[k as "red" | "yellow"]}</span>
                </div>
              );
            })}
          </div>
        </Section>

        {/* ------------------------------------------------------------ type */}
        <Section id="type" n="02 — Type" title="Archivo, Instrument Sans, JetBrains Mono." note="Display wide and heavy; money narrower with tabular figures and smaller cents; prices in mono.">
          <Panel title="Scale" caption="KALKS2 §3">
            <div className="divide-y divide-line">
              {[
                ["Display XL · 80", <span key="a" className="k-d text-[46px] md:text-[80px]">Made simple.</span>],
                ["Display M · 46", <span key="b" className="k-d text-[34px] md:text-[46px]">Get funded.</span>],
                ["Title · 34", <span key="c" className="font-display text-[28px] leading-none tracking-[-0.03em] wdth-112 md:text-[34px]">Good evening, <b className="font-extrabold">Arjun</b></span>],
                ["Card title · 20", <span key="d" className="k-title text-[20px]">Recent activity</span>],
                ["Money L · 30", <Money key="e" value={24318.42} display className="text-[30px] leading-none" />],
                ["Body · 15", <span key="f" className="text-[15px]">Buy an option and the premium is the most you can lose.</span>],
                ["Label · 12.5", <span key="g" className="text-[12.5px] font-semibold text-fg-2">Free margin</span>],
                ["Table head · 11", <span key="h" className="k-th">Amount</span>],
                ["Price · mono", <span key="i" className="font-mono text-[14px] font-semibold tabular-nums">1.16418</span>],
                ["Kicker · mono", <span key="j" className="k-kicker text-red">NEW · KALKS FX OPTIONS</span>],
              ].map(([m, el], i) => (
                <div key={i} className="grid grid-cols-1 items-baseline gap-1.5 py-3 first:pt-0 md:grid-cols-[150px_1fr] md:gap-4">
                  <span className="font-mono text-[11.5px] text-fg-3">{m}</span>
                  {el}
                </div>
              ))}
            </div>
          </Panel>
        </Section>

        {/* ------------------------------------------------------------ buttons */}
        <Section id="buttons" n="03 — Buttons" title="A flat face on a solid edge." note="Press slides the face into the edge (90 ms). Disabled sits half-pressed with no edge. One saturated action per panel.">
          <div className="grid grid-cols-1 gap-4 lg:grid-cols-12">
            <Panel title="Variants" caption="40" className="lg:col-span-7">
              <Row>
                <Button variant="primary">Open account</Button>
                <Button variant="highlight" icon={<Download />}>
                  Deposit
                </Button>
                <Button variant="ink">Trade</Button>
                <Button variant="ghost">Cancel</Button>
                <Button variant="neutral" icon={<Plus />}>
                  Add
                </Button>
              </Row>
              <Row className="mt-5">
                <Button variant="buy">Buy</Button>
                <Button variant="sell">Sell</Button>
                <Button variant="primary" loading>
                  Confirm
                </Button>
                <Button variant="primary" disabled>
                  Disabled
                </Button>
                <Button variant="soft">Soft</Button>
              </Row>
              <div className="mt-5 flex flex-wrap items-center gap-3.5 rounded-[18px] bg-[url(/assets/photos/robot-red.jpg)] bg-cover bg-[70%_30%] p-[18px]">
                <Button variant="white" size={48} iconEnd={<ArrowRight />}>
                  Start trading
                </Button>
                <span className="text-[13px] font-semibold text-white/85">white · on photos only</span>
              </div>
            </Panel>
            <Panel title="Sizes" caption="32 · 40 · 48 · 56" className="lg:col-span-5">
              <Row className="items-end">
                {([32, 40, 48, 56] as const).map((s) => (
                  <div key={s}>
                    <Button variant={s === 56 ? "primary" : "ink"} size={s}>
                      {s === 56 ? "Open account" : `Size ${s}`}
                    </Button>
                    <Cap>{s}</Cap>
                  </div>
                ))}
              </Row>
              <Row className="mt-5 items-center">
                <IconButton variant="primary" size="sm" aria-label="Add">
                  <Plus />
                </IconButton>
                <IconButton variant="highlight" aria-label="Ask AI">
                  <Sparkles />
                </IconButton>
                <IconButton variant="ink" size="lg" aria-label="More">
                  <MoreHorizontal />
                </IconButton>
                <IconButton aria-label="Search">
                  <Search />
                </IconButton>
                <IconButton variant="ghost" aria-label="Settings">
                  <Settings />
                </IconButton>
                <IconButton active aria-label="Active tool">
                  <CandlestickChart />
                </IconButton>
                <IconButton dot aria-label="Alerts">
                  <Bell />
                </IconButton>
                <IconButton variant="outline" size="sm" aria-label="Close position">
                  <span className="text-[15px] leading-none">×</span>
                </IconButton>
              </Row>
              <div className="mt-5">
                <Button variant="buy" block className="justify-between">
                  <span>Buy 0.50 lots</span>
                  <span className="k-btn-sub">at 1.16418</span>
                </Button>
              </div>
            </Panel>
          </div>
        </Section>

        {/* ------------------------------------------------------------ chips */}
        <Section id="chips" n="04 — Chips & controls" title="Tactile, never glowing.">
          <div className="grid grid-cols-1 gap-4 lg:grid-cols-12">
            <Panel title="Chips, status, tags" className="lg:col-span-6">
              <Row>
                {["all", "live", "demo", "cfd", "options"].map((c) => (
                  <Chip key={c} onClick={() => setChips(c)} selected={chips === c}>
                    {{ all: "All", live: "Live", demo: "Demo", cfd: "CFD", options: "Options" }[c]}
                  </Chip>
                ))}
                <Chip onClick={() => {}} variant="outline">
                  Outline
                </Chip>
              </Row>
              <Row className="mt-5">
                <StatusChip status="completed" />
                <StatusChip status="pending" />
                <StatusChip status="rejected" />
                <StatusChip status="processing" label="Open" />
                <StatusChip status="draft" />
              </Row>
              <Row className="mt-5 items-center">
                <Tag tone="live">Live</Tag>
                <Tag tone="demo">Demo</Tag>
                <Tag tone="cfd">CFD</Tag>
                <Tag tone="options">Options</Tag>
                <Tag tone="new">New</Tag>
                <Delta value={0.42} chip />
                <Delta value={-1.18} chip />
              </Row>
            </Panel>
            <Panel title="Segmented, tabs, stepper" className="lg:col-span-6">
              <div className="flex flex-col gap-4">
                <Segmented aria-label="Order type" value={seg} onChange={setSeg} options={[{ value: "market", label: "Market" }, { value: "limit", label: "Limit" }, { value: "stop", label: "Stop" }]} />
                <Segmented aria-label="Timeframe" size={26} value={tf} onChange={setTf} options={["1m", "5m", "15m", "1h", "4h", "D"]} />
                <Segmented aria-label="Accounts" size={36} block value={scope} onChange={setScope} options={[{ value: "all", label: "All" }, { value: "live", label: "Live" }, { value: "demo", label: "Demo" }]} />
                <Tabs aria-label="Positions" variant="pill" value={tab} onChange={setTab} tabs={[{ value: "positions", label: "Positions", count: 3 }, { value: "orders", label: "Orders", count: 1 }, { value: "history", label: "History" }]} />
                <Tabs aria-label="Range" value={tab} onChange={setTab} tabs={[{ value: "positions", label: "Weekly" }, { value: "orders", label: "Monthly" }, { value: "history", label: "Last year" }]} />
                <Stepper steps={["Product", "Account", "Group", "Confirm"]} current={1} />
                <ChoiceCards
                  aria-label="Account product"
                  value={product}
                  onChange={setProduct}
                  options={[
                    { value: "cfd", title: "CFD account", text: "Forex, metals, indices, crypto, stocks" },
                    { value: "options", title: "Options account", text: "Calls and puts, settled in USD" },
                  ]}
                />
              </div>
            </Panel>
            <Panel title="Toggle, slider, meter" className="lg:col-span-6">
              <div className="flex flex-col gap-4">
                <label className="flex items-center justify-between gap-3 text-[14px] font-semibold">
                  Stop loss
                  <Toggle checked={tog1} onChange={setTog1} label="Stop loss" />
                </label>
                <label className="flex items-center justify-between gap-3 text-[14px] font-semibold">
                  Take profit
                  <Toggle checked={tog2} onChange={setTog2} label="Take profit" />
                </label>
                <label className="flex items-center justify-between gap-3 text-[13px] font-semibold text-fg-2">
                  Trader row (sm)
                  <Toggle size="sm" checked={tog1} onChange={setTog1} label="Trader row" />
                </label>
                <Slider label="Risk per trade" value={risk} onChange={setRisk} format={(v) => `${v}%`} ticks={["0%", "25%", "50%", "75%", "100%"]} />
                <div>
                  <div className="flex justify-between text-[12.5px] font-semibold text-fg-2">
                    <span>Profit target</span>
                    <span className="font-mono">6.2% / 8%</span>
                  </div>
                  <Progress value={62} target={80} tone="up" size="md" className="mt-2.5" label="Profit target" />
                </div>
              </div>
            </Panel>
            <Panel title="Fields" caption="44 · radius 12" className="lg:col-span-6">
              <div className="grid gap-4 md:grid-cols-2">
                <Field label="Amount" hint="Min $10 · no fee">
                  <Input placeholder="0.00" inputMode="decimal" trailing="USDT" />
                </Field>
                <Field label="Wallet address" error="This is a TRC20 address. Choose TRON as the network.">
                  <Input defaultValue="TQ7p…9xKe" invalid />
                </Field>
                <Field label="Network">
                  <Select defaultValue="trc20" options={[{ value: "trc20", label: "TRON (TRC20)" }, { value: "erc20", label: "Ethereum (ERC20)" }]} />
                </Field>
                <Field label="Search">
                  <Input placeholder="Symbols, pages, help" leading={<Search />} trailing={<kbd className="k-kbd">⌘K</kbd>} />
                </Field>
                <Field label="Volume" className="md:col-span-2">
                  <div className="flex items-center gap-1.5">
                    <Button variant="neutral" aria-label="Less" onClick={() => setLots((l) => Math.max(0.01, +l - 0.1).toFixed(2))}>
                      −
                    </Button>
                    <Input value={lots} onChange={(e) => setLots(e.target.value)} className="flex-1" inputClassName="text-center font-mono text-[17px] font-semibold" aria-label="Lots" />
                    <Button variant="neutral" aria-label="More" onClick={() => setLots((l) => (+l + 0.1).toFixed(2))}>
                      +
                    </Button>
                  </div>
                </Field>
                <Field label="Message" className="md:col-span-2">
                  <Textarea placeholder="Tell us what happened" rows={3} />
                </Field>
              </div>
            </Panel>
          </div>
        </Section>

        {/* ------------------------------------------------------------ cards */}
        <Section id="cards" n="05 — Cards" title="Solid by default. Glass only over something." note="Photo hero with a glass KPI strip, stat tiles, account cards with their product tag, the activity table.">
          <div className="grid grid-cols-1 gap-4 lg:grid-cols-12">
            <PhotoHero
              className="lg:col-span-8"
              image="/assets/photos/robot-red.jpg"
              kicker={
                <>
                  <Sparkles className="size-3.5" /> NEW · KALKS FX OPTIONS
                </>
              }
              title="Options on forex, made simple."
              text="Buy a call or a put and the premium is the most you can lose."
              cta={
                <Button variant="white" size={48} iconEnd={<ArrowRight />}>
                  Explore options
                </Button>
              }
              link={
                <>
                  Try the demo <ArrowUpRight className="size-4" />
                </>
              }
              tabs={{ items: [{ value: "overview", label: "Overview" }, { value: "options", label: "Options" }, { value: "prop", label: "Prop" }], value: heroTab, onChange: setHeroTab, "aria-label": "Hero" }}
              stats={[
                { label: "Equity", value: <Money value={24318.42} />, color: "var(--k-yellow)" },
                { label: "Today", value: <span className="text-up">+$412.06</span>, color: "var(--k-up)" },
                { label: "Margin level", value: "1,284%", color: "var(--k-fg)", bar: 72 },
                { label: "Open positions", value: "3", color: "var(--k-red)" },
              ]}
              statsAction={
                <IconButton variant="ink" aria-label="More">
                  <ArrowUpRight />
                </IconButton>
              }
            />
            <div className="flex flex-col gap-4 lg:col-span-4">
              <StatTile label="Balance" value={<Money value={18240.5} display />} sub={<Delta value={2.4} />} icon={<Wallet />} spark={SPARK} sparkTone="accent" />
              <StatTile label="Today's P&L" value={<span className="text-down">−$86.10</span>} sub="3 positions" spark={SPARK_DN} />
            </div>

            <div className="grid gap-4 sm:grid-cols-2 lg:col-span-8 lg:grid-cols-3">
              <AccountCard
                product="cfd"
                kind="live"
                login="#7012 4431"
                name="Standard"
                balance={12480.2}
                facts={[
                  { label: "Equity", value: "$12,892.61" },
                  { label: "Free margin", value: "$11,604.00" },
                  { label: "Leverage", value: "1:500" },
                  { label: "P&L", value: <span className="text-up">+$412.41</span> },
                ]}
                actions={
                  <>
                    <Button variant="highlight" size={32}>
                      Deposit
                    </Button>
                    <Button variant="ink" size={32}>
                      Trade
                    </Button>
                    <IconButton variant="ghost" size="sm" aria-label="More">
                      <MoreHorizontal />
                    </IconButton>
                  </>
                }
              />
              <AccountCard
                product="options"
                kind="demo"
                login="#9001 2210"
                name="Options Standard"
                balance={10000}
                facts={[
                  { label: "Premium at risk", value: "$240.00" },
                  { label: "Open options", value: "2" },
                ]}
                actions={
                  <>
                    <Button variant="ink" size={32}>
                      Trade options
                    </Button>
                    <IconButton variant="ghost" size="sm" aria-label="More">
                      <MoreHorizontal />
                    </IconButton>
                  </>
                }
              />
              <AddAccountCard title="Open an Options account" text="Calls and puts get their own account. Free demo with $10,000." action={<Button variant="primary" size={32}>Open account</Button>} />
            </div>
            <Card className="flex flex-col p-5 lg:col-span-4">
              <div className="flex items-center justify-between">
                <h3 className="k-title text-[19px]">Equity</h3>
                <Segmented aria-label="Range" size={26} value={tf} onChange={setTf} options={["1h", "4h", "D"]} />
              </div>
              <Money value={24318.42} display className="mt-4 text-[34px] leading-none" />
              <span className="mt-2 text-[13px] font-medium text-fg-2">
                <span className="text-up">+$1,204.18</span> this month
              </span>
              <Sparkline data={SPARK} tone="accent" width={320} height={120} className="mt-4 h-[120px] w-full" />
            </Card>

            <Card className="px-2 pb-2 pt-5 lg:col-span-8">
              <div className="flex items-center gap-3 px-3.5 pb-3">
                <h3 className="k-title text-[19px]">Recent activity</h3>
                <Segmented aria-label="Activity" size={26} className="ms-auto" value={scope} onChange={setScope} options={[{ value: "all", label: "All" }, { value: "live", label: "Deposits" }, { value: "demo", label: "Trades" }]} />
              </div>
              <Table>
                <thead>
                  <tr>
                    <Th>Event</Th>
                    <Th>Account</Th>
                    <Th>Status</Th>
                    <Th num>Amount</Th>
                    <Th num>Time</Th>
                  </tr>
                </thead>
                <tbody>
                  <tr>
                    <td>
                      <TitleCell icon={<Download />} tone="yellow" title="Deposit · USDT TRC20" sub="TQ7p…9xKe" />
                    </td>
                    <td className="text-fg-2">#7012 4431</td>
                    <td>
                      <StatusChip status="completed" />
                    </td>
                    <Td num>+$2,000.00</Td>
                    <Td num className="!font-medium !text-[12.5px] text-fg-3">
                      14:02
                    </Td>
                  </tr>
                  <tr>
                    <td>
                      <TitleCell icon={<CandlestickChart />} tone="up" title="Buy EURUSD 0.50" sub="Closed at 1.16418" />
                    </td>
                    <td className="text-fg-2">#7012 4431</td>
                    <td>
                      <StatusChip status="processing" label="Closed" />
                    </td>
                    <Td num>
                      <span className="text-up">+$412.41</span>
                    </Td>
                    <Td num className="!font-medium !text-[12.5px] text-fg-3">
                      11:47
                    </Td>
                  </tr>
                  <tr>
                    <td>
                      <TitleCell icon={<Upload />} title="Withdrawal" sub="Bank transfer" />
                    </td>
                    <td className="text-fg-2">#7012 4431</td>
                    <td>
                      <StatusChip status="pending" />
                    </td>
                    <Td num>−$500.00</Td>
                    <Td num className="!font-medium !text-[12.5px] text-fg-3">
                      Yesterday
                    </Td>
                  </tr>
                </tbody>
              </Table>
            </Card>
            <Card variant="glass" className="p-5 lg:col-span-4">
              <CardHeader className="!p-0" title="Glass card" subtitle="Frosted (blur 26, saturate 160 %), only over a photo or the blooms." icon={<Layers />} />
              <div className="mt-5 grid grid-cols-2 gap-3">
                <Card className="p-4">
                  <span className="text-[12.5px] font-semibold text-fg-2">Solid · e1</span>
                </Card>
                <Card hot className="p-4">
                  <span className="text-[12.5px] font-semibold text-fg-2">Tinted</span>
                </Card>
              </div>
            </Card>
          </div>
        </Section>

        {/* ------------------------------------------------------------ overlays */}
        <Section id="overlays" n="06 — Sheets, modals, toasts" title="One line of consequence, then the pair.">
          <div className="grid grid-cols-1 gap-4 lg:grid-cols-12">
            <Panel title="Toasts" caption="iOS banners" className="lg:col-span-7">
              <div className="flex flex-col gap-3">
                <Toast tone="fill" title="Buy EURUSD 0.50 filled" text="At 1.16418 · #7012 4431" time="now" />
                <Toast tone="reject" title="Order rejected" text="Not enough free margin. Lower the volume or deposit." time="2m" />
                <Toast tone="info" title="Price alert" text="XAUUSD crossed 2,410.00" time="9m" />
              </div>
              <Row className="mt-5">
                <Button variant="neutral" size={32} onClick={() => showToast({ tone: "fill", title: "Buy EURUSD 0.50 filled", text: "At 1.16418 · #7012 4431" })}>
                  Show a toast
                </Button>
              </Row>
            </Panel>
            <Panel title="Dialogs" caption="modal · sheet" className="lg:col-span-5">
              <p className="text-[14px] text-fg-2">Radius 28, e2, 320 ms in. On phones the sheet docks to the bottom with a grabber.</p>
              <Row className="mt-5">
                <Button variant="ink" size={32} onClick={() => setModal(true)}>
                  Open modal
                </Button>
                <Button variant="neutral" size={32} onClick={() => setSheet(true)}>
                  Open sheet
                </Button>
              </Row>
              <Dialog
                open={modal}
                onOpenChange={setModal}
                width={420}
                title="Close 3 positions?"
                description="You lock in +$412.41 on EURUSD, XAUUSD and US500."
                footer={
                  <>
                    <Button variant="ghost" onClick={() => setModal(false)}>
                      Keep open
                    </Button>
                    <Button variant="primary" onClick={() => setModal(false)}>
                      Close all
                    </Button>
                  </>
                }
              />
              <Sheet
                open={sheet}
                onOpenChange={setSheet}
                width={440}
                title="Withdraw $500.00"
                description="To your bank account ending 4431. Arrives in 1–2 business days."
                footer={
                  <>
                    <Button variant="ghost" onClick={() => setSheet(false)}>
                      Cancel
                    </Button>
                    <Button variant="ink" onClick={() => setSheet(false)}>
                      Withdraw
                    </Button>
                  </>
                }
              />
            </Panel>
          </div>
        </Section>

        {/* ------------------------------------------------------------ shell */}
        <Section id="shell" n="07 — Shell" title="A black rail and a quiet top bar." note="The rail stays black in both themes; on phones it becomes the ink tab bar.">
          <div className="k-card overflow-hidden p-3.5">
            <div className="flex gap-4">
              <IconRail
                className="hidden md:flex"
                items={[
                  { key: "home", label: "Dashboard", icon: <Home />, active: true },
                  { key: "accounts", label: "Accounts", icon: <Layers /> },
                  { key: "wallet", label: "Wallet", icon: <Wallet /> },
                  { key: "copy", label: "Copy & PAMM", icon: <Users /> },
                  { key: "prop", label: "Prop", icon: <Trophy /> },
                  { key: "academy", label: "Academy", icon: <GraduationCap /> },
                ]}
                bottom={[
                  { key: "alerts", label: "Alerts", icon: <Bell />, dot: true },
                  { key: "settings", label: "Settings", icon: <Settings /> },
                ]}
                footer={<RailAvatar initials="AS" />}
              />
              <div className="min-w-0 flex-1 py-2 md:py-3">
                <TopBar kicker="THU · 9 OCT 2026" title={<>Good evening, <b>Arjun</b></>}>
                  <SearchButton placeholder="Search" className="hidden lg:flex" />
                  <AccountPill tag="CFD" login="#7012 4431" balance="$12,480.20" className="hidden md:flex" />
                  <Button variant="highlight" icon={<Download />}>
                    Deposit
                  </Button>
                </TopBar>
                <div className="mt-6 grid gap-3 sm:grid-cols-3">
                  <StatTile label="Equity" value={<Money value={24318.42} display />} />
                  <StatTile label="Free margin" value={<Money value={11604} display />} />
                  <StatTile label="Open P&L" value={<span className="text-up">+$412.41</span>} />
                </div>
                <TabBar
                  className="mt-6 md:hidden"
                  items={[
                    { key: "home", label: "Dashboard", icon: <Home />, active: true },
                    { key: "accounts", label: "Accounts", icon: <Layers /> },
                    { key: "trade", label: "Trade", icon: <CandlestickChart /> },
                    { key: "wallet", label: "Wallet", icon: <Wallet /> },
                    { key: "more", label: "More", icon: <MoreHorizontal /> },
                  ]}
                />
              </div>
            </div>
          </div>
        </Section>

        {/* ------------------------------------------------------------ brand */}
        <Section id="brand" n="08 — Brand" title="A red K with a yellow edge." note="On yellow the edge turns black; on red the face turns black. Below 20 px the edge is dropped.">
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
            {[
              { bg: "var(--k-black)", tone: "auto" as const, cls: "text-[#F6EEE8]", cap: "on black" },
              { bg: "#FFFFFF", tone: "auto" as const, cls: "text-[#0B0809]", cap: "on white" },
              { bg: "#D4112A", tone: "on-red" as const, cls: "", cap: "on red" },
              { bg: "#FFD21F", tone: "on-yellow" as const, cls: "", cap: "on yellow" },
            ].map((l) => (
              <div key={l.cap} className="relative grid h-[170px] place-items-center rounded-[24px] shadow-[inset_0_0_0_1px_var(--k-border)]" style={{ background: l.bg }}>
                <Logo height={48} tone={l.tone} className={l.cls} />
                <span className="absolute bottom-3.5 start-4 font-mono text-[11.5px] font-medium opacity-70" style={{ color: l.bg === "#FFFFFF" || l.bg === "#FFD21F" ? "#0B0809" : "#fff" }}>
                  {l.cap}
                </span>
              </div>
            ))}
          </div>
          <Panel title="App icon & favicons" caption="assets/brand" className="mt-4">
            <div className="flex flex-wrap items-end gap-6">
              {/* eslint-disable-next-line @next/next/no-img-element */}
              <div><img src="/assets/brand/icon-512.png" alt="App icon" width={112} height={112} className="rounded-[26px]" /><Cap>app · 512</Cap></div>
              {/* eslint-disable-next-line @next/next/no-img-element */}
              <div><img src="/assets/brand/apple-touch-icon.png" alt="Apple touch icon" width={60} height={60} className="rounded-[14px]" /><Cap>apple · 180</Cap></div>
              {/* eslint-disable-next-line @next/next/no-img-element */}
              <div><img src="/assets/brand/favicon-32.png" alt="Favicon 32" width={32} height={32} /><Cap>32</Cap></div>
              {/* eslint-disable-next-line @next/next/no-img-element */}
              <div><img src="/assets/brand/favicon-16.png" alt="Favicon 16" width={16} height={16} /><Cap>16</Cap></div>
              <div className="flex items-end gap-4">
                <div><LogoMark size={40} /><Cap>mark 40</Cap></div>
                <div><LogoMark size={18} /><Cap>18 · flat</Cap></div>
                <div><Logo height={22} /><Cap>wordmark 22</Cap></div>
              </div>
            </div>
          </Panel>
        </Section>
      </main>
    </div>
  );
}
