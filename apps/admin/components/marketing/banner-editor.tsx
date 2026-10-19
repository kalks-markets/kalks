"use client";

import * as React from "react";
import { Check, Link2, Monitor, Smartphone } from "lucide-react";
import { toast } from "sonner";
import { Button, Dialog, DialogClose, Field, Input, Segmented, cn } from "@kalks/ui";
import { MKT_BANNER_PHOTOS, MKT_SEGMENTS, type MktBanner, type MktPlacement, type MktSegment } from "@kalks/mock/admin-growth-marketing";
import { BannerPreview, ChipPicker, SectionLabel } from "./kit";

interface Draft {
  eyebrow: string;
  headline: string;
  sub: string;
  cta: string;
  link: string;
  photo: string;
  placement: MktPlacement;
  segments: MktSegment[];
  start: string;
  end: string;
  locales: string[];
}

const EMPTY: Draft = {
  eyebrow: "Limited offer",
  headline: "Trade gold with spreads from 0.0 pips",
  sub: "Open a Pro account this week and get a free VPS for 3 months.",
  cta: "Open Pro account",
  link: "/accounts/new?group=pro",
  photo: "gold",
  placement: "dashboard",
  segments: ["All clients"],
  start: "2026-09-25",
  end: "2026-10-25",
  locales: ["EN"],
};
const LOCALES = ["EN", "AR", "HI", "VI", "TH", "ID", "PT", "ES"] as const;

export function BannerEditor({ open, onOpenChange, banner }: { open: boolean; onOpenChange: (o: boolean) => void; banner: MktBanner | null }) {
  const [d, setD] = React.useState<Draft>(EMPTY);
  const [device, setDevice] = React.useState<"desktop" | "mobile">("desktop");
  React.useEffect(() => {
    if (!open) return;
    setD(
      banner
        ? {
            eyebrow: banner.eyebrow,
            headline: banner.headline,
            sub: banner.sub,
            cta: banner.cta,
            link: banner.link,
            photo: banner.photo.replace("/assets/photos/", "").replace(".jpg", ""),
            placement: banner.placement,
            segments: [...banner.segments],
            start: banner.start.slice(0, 10),
            end: banner.end.slice(0, 10),
            locales: [...banner.locales],
          }
        : EMPTY,
    );
  }, [open, banner]);
  const set = <K extends keyof Draft>(k: K, v: Draft[K]) => setD((x) => ({ ...x, [k]: v }));
  const reach = Math.round(62410 * Math.min(1, d.segments.includes("All clients") ? 1 : d.segments.length * 0.14));

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      width={1040}
      title={banner ? "Edit banner" : "New banner"}
      description="What you see on the right is exactly what targeted clients will see."
      footer={
        <>
          <span className="mr-auto hidden text-[12.5px] text-fg-3 sm:block">
            Estimated reach <span className="k-num text-fg">{reach.toLocaleString("en-US")}</span> clients
          </span>
          <DialogClose asChild>
            <Button size="sm" variant="ghost">
              Cancel
            </Button>
          </DialogClose>
          <Button size="sm" variant="surface" onClick={() => toast.success("Test banner sent", { description: "Shown to priya.nair@kalkstrade.com on next login" })}>
            Send test
          </Button>
          <Button
            size="sm"
            variant="ember"
            onClick={() => {
              toast.success(banner ? "Banner updated" : "Banner scheduled", { description: `${d.placement} · ${d.segments.join(", ")} · ${d.start} → ${d.end}` });
              onOpenChange(false);
            }}
          >
            {banner ? "Save changes" : "Schedule banner"}
          </Button>
        </>
      }
    >
      <div className="grid grid-cols-1 gap-6 lg:grid-cols-[1fr_420px]">
        <div className="space-y-5">
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <Field label="Eyebrow">
              <Input value={d.eyebrow} onChange={(e) => set("eyebrow", e.target.value)} />
            </Field>
            <Field label="CTA label">
              <Input value={d.cta} onChange={(e) => set("cta", e.target.value)} />
            </Field>
            <Field label="Headline" hint={`${d.headline.length}/60`} className="sm:col-span-2">
              <Input value={d.headline} maxLength={60} onChange={(e) => set("headline", e.target.value)} />
            </Field>
            <Field label="Supporting text" className="sm:col-span-2">
              <Input value={d.sub} onChange={(e) => set("sub", e.target.value)} />
            </Field>
            <Field label="Link" className="sm:col-span-2">
              <Input value={d.link} onChange={(e) => set("link", e.target.value)} leading={<Link2 />} inputClassName="font-mono text-[13px]" />
            </Field>
          </div>

          <div>
            <SectionLabel>Image</SectionLabel>
            <div className="grid grid-cols-6 gap-2">
              {MKT_BANNER_PHOTOS.slice(0, 12).map((p) => (
                <button
                  key={p}
                  type="button"
                  onClick={() => set("photo", p)}
                  className={cn("relative aspect-square overflow-hidden rounded-xl border transition-all", d.photo === p ? "border-ember ring-2 ring-ember/40" : "border-line opacity-70 hover:opacity-100")}
                >
                  {/* eslint-disable-next-line @next/next/no-img-element */}
                  <img src={`/assets/photos/${p}.jpg`} alt={p} className="size-full object-cover" />
                  {d.photo === p && (
                    <span className="absolute right-1 top-1 grid size-4 place-items-center rounded-full bg-ember text-white">
                      <Check className="size-3" />
                    </span>
                  )}
                </button>
              ))}
            </div>
          </div>

          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <div>
              <SectionLabel>Placement</SectionLabel>
              <Segmented size="sm" value={d.placement} onChange={(v) => set("placement", v)} options={[{ value: "dashboard", label: "Dashboard" }, { value: "wallet", label: "Wallet" }, { value: "login", label: "Login" }]} />
            </div>
            <div>
              <SectionLabel>Schedule (GMT+3)</SectionLabel>
              <div className="grid grid-cols-2 gap-2">
                <Input type="date" value={d.start} onChange={(e) => set("start", e.target.value)} className="h-10 px-2.5 text-[12.5px]" />
                <Input type="date" value={d.end} onChange={(e) => set("end", e.target.value)} className="h-10 px-2.5 text-[12.5px]" />
              </div>
            </div>
          </div>

          <div>
            <SectionLabel>Target segments</SectionLabel>
            <ChipPicker options={MKT_SEGMENTS} value={d.segments} onChange={(v) => set("segments", v)} />
          </div>
          <div>
            <SectionLabel>Languages</SectionLabel>
            <ChipPicker options={LOCALES} value={d.locales as (typeof LOCALES)[number][]} onChange={(v) => set("locales", v)} />
          </div>
        </div>

        <div className="lg:sticky lg:top-0 lg:self-start">
          <div className="mb-3 flex items-center justify-between">
            <span className="k-label">Live preview</span>
            <Segmented
              size="xs"
              value={device}
              onChange={setDevice}
              options={[
                { value: "desktop", label: <Monitor className="size-3.5" /> },
                { value: "mobile", label: <Smartphone className="size-3.5" /> },
              ]}
            />
          </div>
          <div className="k-dotgrid rounded-[22px] border border-line bg-bg p-4">
            <div className={cn("mx-auto transition-all duration-500", device === "mobile" ? "max-w-[260px]" : "max-w-full")}>
              {d.placement !== "login" && (
                <div className="mb-2.5 flex items-center justify-between">
                  <div className="h-2.5 w-24 rounded-full bg-surface-3" />
                  <div className="flex gap-1.5">
                    <div className="size-5 rounded-full bg-surface-3" />
                    <div className="h-5 w-14 rounded-full bg-ember/60" />
                  </div>
                </div>
              )}
              <BannerPreview photo={`/assets/photos/${d.photo}.jpg`} eyebrow={d.eyebrow} headline={d.headline} sub={d.sub} cta={d.cta} placement={device === "mobile" && d.placement !== "login" ? "login" : d.placement} className={cn(d.placement === "login" && device === "desktop" && "mx-auto max-w-[240px]")} />
              {d.placement !== "login" && (
                <div className="mt-2.5 grid grid-cols-3 gap-2">
                  {[0, 1, 2].map((i) => (
                    <div key={i} className="h-12 rounded-xl border border-line bg-surface" />
                  ))}
                </div>
              )}
            </div>
          </div>
          <p className="mt-3 text-[12px] text-fg-3">
            Shown on <span className="text-fg-2">{d.placement}</span> to <span className="text-fg-2">{d.segments.join(", ") || "no one"}</span> in {d.locales.join(" / ")}.
          </p>
        </div>
      </div>
    </Dialog>
  );
}
