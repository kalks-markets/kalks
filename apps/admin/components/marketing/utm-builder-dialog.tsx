"use client";

import * as React from "react";
import { Link2, QrCode, Rocket } from "lucide-react";
import { toast } from "sonner";
import { Button, CopyButton, Dialog, DialogClose, Field, Input } from "@kalks/ui";
import { MKT_UTM_MEDIUMS, MKT_UTM_SOURCES } from "@kalks/mock/admin-growth-marketing";
import { ChipPicker, SectionLabel } from "./kit";

const PAGES = [
  { value: "https://app.kalkstrade.com/register", label: "Open account" },
  { value: "https://kalkstrade.com/promo/welcome-30", label: "Welcome 30% landing" },
  { value: "https://kalkstrade.com/prop", label: "Prop challenges" },
  { value: "https://kalkstrade.com/copy-trading", label: "Copy trading" },
];

export function UtmBuilderDialog({ open, onOpenChange, onCreate }: { open: boolean; onOpenChange: (o: boolean) => void; onCreate: (v: { source: string; medium: string; campaign: string; budget: number }) => void }) {
  const [page, setPage] = React.useState(PAGES[0]!.value);
  const [source, setSource] = React.useState(["google"]);
  const [medium, setMedium] = React.useState(["cpc"]);
  const [campaign, setCampaign] = React.useState("xauusd_q4_search_ae");
  const [content, setContent] = React.useState("");
  const [budget, setBudget] = React.useState("15000");

  const s = source[source.length - 1] ?? "";
  const m = medium[medium.length - 1] ?? "";
  const url = `${page}?utm_source=${s}&utm_medium=${m}&utm_campaign=${campaign}${content ? `&utm_content=${encodeURIComponent(content)}` : ""}`;
  const valid = !!s && !!m && /^[a-z0-9_]{3,48}$/.test(campaign);

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title="New UTM campaign"
      description="Tracked links attribute clicks, sign-ups, FTDs and deposits back to the channel."
      width={680}
      footer={
        <>
          <DialogClose asChild>
            <Button size="sm" variant="ghost">
              Cancel
            </Button>
          </DialogClose>
          <Button size="sm" variant="surface" onClick={() => toast.success("QR code generated", { description: `${campaign}.png · 1024×1024` })}>
            <QrCode /> QR code
          </Button>
          <Button
            size="sm"
            variant="ember"
            disabled={!valid}
            onClick={() => {
              onCreate({ source: s, medium: m, campaign, budget: Number(budget) || 0 });
              toast.success("Campaign created", { description: `${s} / ${m} / ${campaign}` });
              onOpenChange(false);
            }}
          >
            <Rocket /> Create campaign
          </Button>
        </>
      }
    >
      <div className="space-y-5">
        <Field label="Landing page">
          <div className="relative flex h-11 items-center rounded-[14px] border border-line bg-surface-2 px-3.5 text-sm">
            <select value={page} onChange={(e) => setPage(e.target.value)} className="h-full w-full cursor-pointer appearance-none bg-transparent text-fg outline-none [&>option]:bg-surface">
              {PAGES.map((p) => (
                <option key={p.value} value={p.value}>
                  {p.label} · {p.value.replace("https://", "")}
                </option>
              ))}
            </select>
          </div>
        </Field>
        <div>
          <SectionLabel>utm_source</SectionLabel>
          <ChipPicker options={MKT_UTM_SOURCES} value={source} onChange={(v) => setSource(v.slice(-1))} />
        </div>
        <div>
          <SectionLabel>utm_medium</SectionLabel>
          <ChipPicker options={MKT_UTM_MEDIUMS} value={medium} onChange={(v) => setMedium(v.slice(-1))} />
        </div>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          <Field label="utm_campaign" hint="a–z, 0–9, _" className="sm:col-span-2" error={valid || !campaign ? undefined : "Use lowercase letters, digits and underscores"}>
            <Input value={campaign} onChange={(e) => setCampaign(e.target.value.toLowerCase())} inputClassName="font-mono text-[13px]" />
          </Field>
          <Field label="Monthly budget">
            <Input value={budget} onChange={(e) => setBudget(e.target.value.replace(/[^0-9]/g, ""))} leading={<span className="text-[13px]">$</span>} inputClassName="k-num" />
          </Field>
          <Field label="utm_content (optional)" className="sm:col-span-3">
            <Input value={content} onChange={(e) => setContent(e.target.value)} placeholder="e.g. gold_video_15s" inputClassName="font-mono text-[13px]" />
          </Field>
        </div>
        <div className="k-row px-4 py-3">
          <div className="mb-1.5 flex items-center justify-between">
            <span className="k-label flex items-center gap-1.5">
              <Link2 className="size-3.5" /> Tracked link
            </span>
            <CopyButton value={url} label="Tracked link" />
          </div>
          <div className="break-all font-mono text-[12px] leading-relaxed text-fg-2">{url}</div>
        </div>
      </div>
    </Dialog>
  );
}
