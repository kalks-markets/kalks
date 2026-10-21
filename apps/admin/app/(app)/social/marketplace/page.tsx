"use client";

import * as React from "react";
import { BookOpen, Download, EyeOff, OctagonAlert } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, Chip, DataTable, Money, PageHeader, Reveal, StatusChip, Tabs, type Column } from "@kalks/ui";
import { LISTINGS, type Listing } from "@kalks/mock/admin-partners";
import { IS_DEMO } from "@kalks/mock/mode";
import { AlgoMarketplacePage } from "@/components/algo/admin";
import { MiniStat, PersonCell, auditToast, useReason } from "@/components/config/kit";
import { fmtInt, fmtUsdK } from "@/components/partners/common";
import { EmergencyStopDialog, type StopTarget } from "@/components/social/common";
import { CAT_TONE, ListingCard, Rating, priceLabel } from "@/components/social/listing-card";

function DemoMarketplace() {
  const [rows, setRows] = React.useState(LISTINGS);
  const [tab, setTab] = React.useState<"queue" | "live" | "rejected">("queue");
  const [stop, setStop] = React.useState<StopTarget | null>(null);
  const reason = useReason();
  const queue = rows.filter((l) => l.status === "pending" || l.status === "changes").sort((a, b) => b.flags.length - a.flags.length);
  const live = rows.filter((l) => l.status === "live");
  const rejected = rows.filter((l) => l.status === "rejected");
  const set = (l: Listing, status: Listing["status"]) => setRows((rs) => rs.map((r) => (r.id === l.id ? { ...r, status } : r)));

  const approve = (l: Listing) => {
    if (!l.flags.length) {
      set(l, "live");
      auditToast(`${l.title} published`, "Automated checks passed");
      return;
    }
    reason.ask({
      title: `Approve flagged listing`,
      description: `${l.title} has ${l.flags.length} open flag(s): ${l.flags.join(", ")}. Approving overrides them.`,
      reasons: ["Flags reviewed — false positive", "Author provided audited results", "Claims amended offline"],
      confirmLabel: "Approve anyway",
      onConfirm: (r) => { set(l, "live"); auditToast(`${l.title} published with override`, r); },
    });
  };
  const changes = (l: Listing) =>
    reason.ask({
      title: `Request changes · ${l.title}`,
      description: "The author receives the reason code and note; the listing returns to the queue on resubmission.",
      reasons: ["Remove guaranteed-return claims", "Add risk disclosure", "Provide out-of-sample backtest", "Disclose martingale / grid logic", "Fix pricing description"],
      confirmLabel: "Send to author",
      onConfirm: (r) => { set(l, "changes"); auditToast(`Changes requested on ${l.title}`, r); },
    });
  const reject = (l: Listing) =>
    reason.ask({
      title: `Reject ${l.title}`,
      reasons: ["Misleading performance claims", "Prohibited strategy (martingale without stop)", "Unlicensed investment advice", "Latency arbitrage / toxic flow", "Duplicate or plagiarised"],
      confirmLabel: "Reject listing",
      tone: "sell",
      onConfirm: (r) => { set(l, "rejected"); auditToast(`${l.title} rejected`, r); },
    });

  const cols: Column<Listing>[] = [
    { key: "t", header: "Listing", sort: (l) => l.title, cell: (l) => <PersonCell name={l.title} photo={l.photo} sub={<>{l.author} · <span className="font-mono">{l.id}</span></>} /> },
    { key: "c", header: "Category", cell: (l) => <span className="flex gap-1"><Chip size="sm" tone={CAT_TONE[l.category]}>{l.category}</Chip><Chip size="sm">{l.platform}</Chip></span> },
    { key: "p", header: "Price", cell: (l) => <span className="k-num text-[12.5px] text-fg-2">{priceLabel(l)}</span> },
    { key: "s", header: "Subscribers", align: "right", sort: (l) => l.subscribers, cell: (l) => <span className="k-num">{fmtInt(l.subscribers)}</span> },
    { key: "r", header: "Rating", align: "right", sort: (l) => l.rating, hideOn: "md", cell: (l) => <Rating v={l.rating} /> },
    { key: "dd", header: "Max DD", align: "right", hideOn: "lg", cell: (l) => <span className="k-num text-fg-2">{l.backtest.maxDD}%</span> },
    { key: "rev", header: "Revenue 30d", align: "right", sort: (l) => l.revenue30d, cell: (l) => <Money value={l.revenue30d} decimals={0} countUp={false} className="font-medium" /> },
    { key: "st", header: "Status", cell: (l) => <StatusChip status={l.status === "live" ? "active" : "rejected"} label={l.status === "live" ? "Live" : "Rejected"} /> },
    {
      key: "a",
      header: "",
      align: "right",
      cell: (l) =>
        l.status === "live" ? (
          <span className="flex justify-end gap-1.5">
            <Button size="xs" variant="ghost" onClick={() => reason.ask({ title: `Unlist ${l.title}`, description: "Existing subscribers keep access until period end; no new sales.", reasons: ["User complaints", "Performance misrepresentation", "Author request", "Compliance review"], confirmLabel: "Unlist", tone: "sell", onConfirm: (r) => { set(l, "rejected"); auditToast(`${l.title} unlisted`, r); } })}>
              <EyeOff /> Unlist
            </Button>
            {l.category !== "Indicator" && (
              <Button size="xs" variant="down-outline" onClick={() => setStop({ kind: "strategy", id: l.id, name: l.title, followers: l.subscribers, aum: l.subscribers * 1850, openPositions: Math.round(l.subscribers * 0.6) })}>
                <OctagonAlert /> Stop
              </Button>
            )}
          </span>
        ) : (
          <Button size="xs" variant="ghost" onClick={() => { set(l, "pending"); toast.success(`${l.title} moved back to queue`); }}>Re-open</Button>
        ),
    },
  ];

  return (
    <div className="pb-16">
      <PageHeader
        title="Marketplace moderation"
        subtitle="Strategies, EAs, signals and indicators sold to clients"
        actions={
          <>
            <Button variant="surface" onClick={() => toast.success("Moderation policy v3.2", { description: "No guaranteed returns · martingale must disclose max exposure · out-of-sample ≥ 12 months" })}>
              <BookOpen /> Policy
            </Button>
            <Button variant="surface" onClick={() => toast.success("marketplace.csv exported", { description: `${rows.length} listings` })}>
              <Download /> Export
            </Button>
          </>
        }
      />
      <Reveal>
        <div className="mb-5 grid grid-cols-2 gap-3 lg:grid-cols-4">
          <MiniStat label="In review queue" value={queue.length} sub={`${queue.filter((l) => l.status === "changes").length} awaiting author changes`} />
          <MiniStat label="Auto-flagged" value={queue.filter((l) => l.flags.length).length} sub="Claims / martingale / arbitrage" tone="down" />
          <MiniStat label="Live listings" value={live.length} sub={`${fmtInt(live.reduce((s, l) => s + l.subscribers, 0))} subscribers`} />
          <MiniStat label="GMV · 30d" value={fmtUsdK(live.reduce((s, l) => s + l.revenue30d, 0))} sub="Platform take 30%" tone="gold" />
        </div>
      </Reveal>
      <Tabs
        className="mb-5"
        value={tab}
        onChange={setTab}
        tabs={[
          { value: "queue", label: "Review queue", count: queue.length },
          { value: "live", label: "Live", count: live.length },
          { value: "rejected", label: "Rejected & unlisted", count: rejected.length },
        ]}
      />
      {tab === "queue" ? (
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-2 2xl:grid-cols-3">
          {queue.map((l, i) => (
            <Reveal key={l.id} delay={i * 0.04}>
              <ListingCard l={l} onApprove={() => approve(l)} onChanges={() => changes(l)} onReject={() => reject(l)} />
            </Reveal>
          ))}
          {queue.length === 0 && <Card className="p-10 text-center text-fg-3 lg:col-span-2">Queue is clear.</Card>}
        </div>
      ) : (
        <Reveal>
          <Card className="p-4 sm:p-6">
            <DataTable columns={cols} rows={tab === "live" ? live : rejected} dense rowKey={(l) => l.id} search={(l) => `${l.title} ${l.author} ${l.id}`} searchPlaceholder="Listing, author…" />
          </Card>
        </Reveal>
      )}
      <EmergencyStopDialog target={stop} open={!!stop} onOpenChange={(o) => !o && setStop(null)} />
      {reason.node}
    </div>
  );
}

// Live builds: strategy listings on the algo service (/api/algo/listings), the same queue as Algo & API → Marketplace moderation.
export default function MarketplacePage() {
  return IS_DEMO ? <DemoMarketplace /> : <AlgoMarketplacePage />;
}
