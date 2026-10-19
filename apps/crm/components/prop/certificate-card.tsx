"use client";

import * as React from "react";
import { Download, Link2 } from "lucide-react";
import { toast } from "sonner";
import { Logo, Starfield, Tooltip, cn, formatMoney } from "@/components/kit";
import type { PropCertificate } from "@kalks/mock/prop";

function XIcon({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" className={className} fill="currentColor" aria-hidden>
      <path d="M17.75 3h3.07l-6.7 7.66L22 21h-6.17l-4.83-6.32L5.47 21H2.4l7.17-8.2L2 3h6.33l4.37 5.78L17.75 3zm-1.08 16.2h1.7L7.4 4.73H5.57L16.67 19.2z" />
    </svg>
  );
}

function LinkedinIcon({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" className={className} fill="currentColor" aria-hidden>
      <path d="M4.98 3.5a2.5 2.5 0 1 1 0 5 2.5 2.5 0 0 1 0-5zM3 9.75h4v11H3v-11zm6.5 0h3.8v1.5h.06c.53-1 1.83-2.05 3.76-2.05 4.02 0 4.76 2.64 4.76 6.08v5.47h-4v-4.85c0-1.16-.02-2.65-1.62-2.65-1.62 0-1.86 1.26-1.86 2.57v4.93h-4v-11z" />
    </svg>
  );
}

const HEAD: Record<PropCertificate["kind"], string> = {
  funded: "Certificate of funding",
  payout: "Certificate of payout",
  passed: "Certificate of achievement",
};

/** Dark premium certificate (always dark, regardless of theme) with share actions. */
export function CertificateCard({ cert, name, className }: { cert: PropCertificate; name: string; className?: string }) {
  const url = `https://app.kalkstrade.com/verify/${cert.id}`;
  const date = new Date(cert.date).toLocaleDateString("en-GB", { day: "2-digit", month: "long", year: "numeric" });
  const amountText = cert.kind === "payout" ? formatMoney(cert.amount) : `$${cert.amount.toLocaleString("en-US")}`;
  const share = (where: string) => toast.success(`Shared to ${where}`, { description: `${cert.title} · ${amountText}` });

  return (
    <div className={cn("group min-w-0", className)}>
      <div
        className="relative aspect-[1.42] overflow-hidden rounded-[20px] border border-[#e9b949]/30 text-white shadow-[0_30px_60px_-30px_rgba(0,0,0,0.8),inset_0_1px_0_rgba(243,207,107,0.35)] transition-transform duration-500 group-hover:-translate-y-1"
        style={{
          background:
            cert.kind === "funded"
              ? "radial-gradient(120% 90% at 100% 0%, color-mix(in oklab, var(--k-ember) 45%, transparent), transparent 55%), radial-gradient(90% 70% at 0% 100%, rgba(233,185,73,0.18), transparent 60%), linear-gradient(160deg, #2a130b, #0d0a09 60%, #07070a)"
              : "radial-gradient(110% 80% at 100% 0%, rgba(233,185,73,0.28), transparent 55%), radial-gradient(90% 70% at 0% 100%, color-mix(in oklab, var(--k-ember) 18%, transparent), transparent 60%), linear-gradient(160deg, #1a1508, #0c0b09 60%, #07070a)",
        }}
      >
        <Starfield density={36} />
        {/* guilloche frame */}
        <div className="pointer-events-none absolute inset-2.5 rounded-[14px] border border-[#e9b949]/25" />
        <div className="pointer-events-none absolute inset-3.5 rounded-[11px] border border-dashed border-[#e9b949]/15" />
        <div className="relative flex h-full flex-col p-5 sm:p-6">
          <div className="flex items-start justify-between">
            <div>
              <Logo height={16} className="text-white" />
              <div className="mt-1.5 font-mono text-[9px] tracking-wider text-white/40">{cert.id}</div>
            </div>
            <div className="grid size-11 shrink-0 place-items-center rounded-full border border-[#e9b949]/40 bg-[radial-gradient(circle_at_35%_30%,#f3cf6b55,#c9971f22_60%,transparent)]">
              <div className="grid size-8 place-items-center rounded-full border border-dashed border-[#e9b949]/50 text-[7px] font-bold uppercase tracking-widest text-[#e9b949]">KLKS</div>
            </div>
          </div>
          <div className="mt-auto">
            <div className="text-[9.5px] font-semibold uppercase tracking-[0.22em] text-[#e9b949]">{HEAD[cert.kind]}</div>
            <div className="mt-1 text-[11px] text-white/55">Proudly presented to</div>
            <div className="mt-0.5 text-[19px] font-semibold tracking-tight sm:text-[21px]">{name}</div>
          </div>
          <div className="mt-3 flex items-end justify-between gap-3">
            <div>
              <div className="text-[10px] uppercase tracking-wider text-white/45">{cert.kind === "payout" ? "Payout" : cert.kind === "funded" ? "Funded capital" : "Account size"}</div>
              <div
                className="k-num bg-clip-text text-[26px] font-semibold leading-tight tracking-tight text-transparent sm:text-[28px]"
                style={{ backgroundImage: "linear-gradient(180deg, #fbe7a8, #e9b949 55%, #c9971f)" }}
              >
                {amountText}
              </div>
            </div>
            <div className="text-right">
              <div className="text-[10px] uppercase tracking-wider text-white/45">Date</div>
              <div className="text-[12px] font-medium text-white/85">{date}</div>
            </div>
          </div>
        </div>
      </div>
      <div className="mt-3 flex items-center justify-between gap-2 px-1">
        <div className="min-w-0">
          <div className="truncate text-[13.5px] font-medium">{cert.title}</div>
          <div className="font-mono text-[11px] text-fg-3">#{cert.account}</div>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {[
            { icon: <Link2 className="size-3.5" />, label: "Copy link", on: () => { navigator.clipboard?.writeText(url).catch(() => {}); toast.success("Certificate link copied", { description: url }); } },
            { icon: <XIcon className="size-3.5" />, label: "Share on X", on: () => share("X") },
            { icon: <LinkedinIcon className="size-3.5" />, label: "Share on LinkedIn", on: () => share("LinkedIn") },
            { icon: <Download className="size-3.5" />, label: "Download PNG", on: () => toast.success("Certificate downloaded", { description: `${cert.id}.png · 2400×1690` }) },
          ].map((b) => (
            <Tooltip key={b.label} content={b.label}>
              <button type="button" onClick={b.on} aria-label={b.label} className="grid size-8 place-items-center rounded-full border border-line bg-surface-2 text-fg-2 transition-colors hover:border-gold/40 hover:text-gold">
                {b.icon}
              </button>
            </Tooltip>
          ))}
        </div>
      </div>
    </div>
  );
}
