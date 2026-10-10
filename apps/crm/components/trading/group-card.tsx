"use client";

import * as React from "react";
import { Check } from "lucide-react";
import { Chip, LogoMark, cn } from "@/components/kit";
import { useT } from "@kalks/i18n/react";
import type { T } from "@kalks/i18n";
import { modeLabel, type EngineGroup } from "./api";
import { productOf } from "@/lib/products";

/** Pricing model from the group's commercial terms. */
export const spreadType = (g: Pick<EngineGroup, "commissionPerLot">, t: T) => (g.commissionPerLot > 0 ? t("accounts.pricing.rawPlusCommission") : t("accounts.pricing.allIn"));

export const commissionText = (g: Pick<EngineGroup, "commissionPerLot">, t: T) => (g.commissionPerLot > 0 ? t("accounts.unit.perLot", { amount: `$${g.commissionPerLot.toFixed(2).replace(/\.00$/, "")}` }) : t("common.none"));

export const maxLeverage = (g: Pick<EngineGroup, "leverages">) => (g.leverages.length ? Math.max(...g.leverages) : 0);

export function groupFeatures(g: EngineGroup, t: T) {
  return [
    spreadType(g, t),
    g.mode === "hedging" ? t("accounts.feature.hedging") : t("accounts.feature.netting"),
    t("accounts.feature.marginCall", { marginCall: g.marginCallPct, stopOut: g.stopOutPct }),
    g.cent ? t("accounts.feature.cent") : g.swapFree ? t("accounts.feature.swapFree") : t("accounts.feature.nbp"),
  ];
}

/** The finish of each account type's card (founder 2026-10-10: every account card styled like a debit card). */
export function faceOf(code: string, options?: boolean) {
  if (options) return "k-face-options";
  if (code === "cent") return "k-face-cent";
  if (code === "pro") return "k-face-pro";
  if (code === "pro-netting" || code.endsWith("netting")) return "k-face-graphite";
  if (code === "ecn") return "k-face-ecn";
  if (code === "vip") return "k-face-vip";
  if (code.startsWith("prop")) return "k-face-vip";
  return "k-face-standard";
}

/** A debit-card face: logo, chip, name, a badge, and two figures at the bottom. Used for account types, live / demo
 *  and CFD / Options choices. */
export function CardFace({
  face,
  name,
  badge,
  left,
  right,
  selected,
}: {
  face: string;
  name: string;
  badge?: React.ReactNode;
  left?: { label: string; value: string };
  right?: { label: string; value: string };
  selected?: boolean;
}) {
  return (
    <div className={cn("k-face relative aspect-[1.586/1] w-full overflow-hidden rounded-[18px] p-4 text-white [container-type:inline-size] sm:p-5", face)}>
      <svg viewBox="0 0 400 250" preserveAspectRatio="none" className="pointer-events-none absolute inset-0 size-full opacity-60" aria-hidden>
        <path d="M-20 175 C 70 120, 120 215, 190 150 S 300 40, 340 120 S 410 190, 430 95" fill="none" stroke="rgba(255,255,255,.22)" strokeWidth="22" strokeLinecap="round" />
        <path d="M-30 205 C 60 165, 130 240, 210 185 S 310 85, 360 150" fill="none" stroke="rgba(255,255,255,.12)" strokeWidth="10" strokeLinecap="round" />
      </svg>
      <div className="relative flex h-full flex-col">
        <div className="flex items-start justify-between gap-2">
          <LogoMark size={22} className="text-white" />
          <span className="flex items-center gap-1.5">
            {badge && <span className="rounded-full bg-white/20 px-2.5 py-1 text-[10px] font-bold uppercase tracking-[0.08em] backdrop-blur-sm">{badge}</span>}
            {selected && (
              <span className="grid size-6 place-items-center rounded-full bg-white text-[#141416]">
                <Check className="size-3.5" strokeWidth={3} />
              </span>
            )}
          </span>
        </div>
        <svg viewBox="0 0 40 30" className="mt-[4cqw] h-[22px] w-[30px]" aria-hidden>
          <rect x="0.5" y="0.5" width="39" height="29" rx="6" fill="#d9b768" stroke="rgba(0,0,0,.2)" />
          <path d="M0 10h13M0 20h13M27 10h13M27 20h13M13 0v30M27 0v30M13 15h14" stroke="rgba(80,55,15,.45)" strokeWidth="1" fill="none" />
        </svg>
        <div className="mt-auto truncate text-[clamp(17px,7.5cqw,26px)] font-semibold uppercase tracking-[0.06em] [text-shadow:0_1px_10px_rgba(0,0,0,.2)]">{name}</div>
        {(left || right) && (
          <div className="mt-[2.5cqw] flex items-end justify-between gap-3">
            {left ? (
              <div className="min-w-0">
                <div className="truncate text-[9px] font-semibold uppercase tracking-[0.12em] text-white/70">{left.label}</div>
                <div className="k-num truncate text-[clamp(11.5px,4cqw,14px)] font-semibold">{left.value}</div>
              </div>
            ) : (
              <span />
            )}
            {right && (
              <div className="shrink-0 text-end">
                <div className="text-[9px] font-semibold uppercase tracking-[0.12em] text-white/70">{right.label}</div>
                <div className="k-num text-[clamp(11.5px,4cqw,14px)] font-semibold">{right.value}</div>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

export function EngineGroupCard({
  g,
  selected,
  onSelect,
  used,
  kind,
  compact,
}: {
  g: EngineGroup;
  selected?: boolean;
  onSelect?: () => void;
  /** accounts of this kind the client already holds in the group (D23) */
  used?: number;
  kind?: "live" | "demo";
  compact?: boolean;
}) {
  const t = useT();
  // an Options group (CFD / Options account split): leverage, per-lot commission and position mode don't apply
  const options = productOf(g) === "options";
  const full = used !== undefined && used >= g.maxAccountsPerUser;
  const Comp = onSelect ? "button" : "div";
  const mode = options ? t("accounts.product.chipOptions") : t.dyn(`accounts.mode.${g.mode}`, modeLabel(g.mode));
  const specs: [string, string][] = [
    ...(!options ? [[t("accounts.label.commission"), commissionText(g, t)] as [string, string]] : []),
    [t("accounts.label.minDeposit"), g.minDeposit > 0 ? `$${g.minDeposit.toLocaleString("en-US")}` : t("common.none")],
    ...(!options ? [[t("accounts.label.maxLeverage"), `1:${maxLeverage(g).toLocaleString("en-US")}`] as [string, string]] : []),
    [t("accounts.label.stopOut"), `${g.stopOutPct}%`],
  ];
  return (
    <Comp
      type={onSelect ? "button" : undefined}
      onClick={full ? undefined : onSelect}
      disabled={onSelect ? full : undefined}
      aria-pressed={onSelect ? !!selected : undefined}
      className={cn(
        "k-card group relative flex h-full w-full flex-col overflow-hidden rounded-[24px] p-3 text-start transition-[transform,box-shadow,border-color] duration-200 hover:-translate-y-0.5",
        selected && "!border-ember/70 shadow-[0_0_0_4px_color-mix(in_oklab,var(--k-ember)_18%,transparent)]",
        full && "cursor-not-allowed opacity-55",
      )}
    >
      <CardFace
        face={faceOf(g.code, options)}
        name={g.name}
        badge={g.cent ? `${mode} · USC` : mode}
        left={{ label: t("accounts.label.minDeposit"), value: g.minDeposit > 0 ? `$${g.minDeposit.toLocaleString("en-US")}` : t("common.none") }}
        right={options ? { label: t("accounts.label.stopOut"), value: `${g.stopOutPct}%` } : { label: t("accounts.label.maxLeverage"), value: `1:${maxLeverage(g).toLocaleString("en-US")}` }}
        selected={selected}
      />
      <div className="flex flex-1 flex-col px-2 pb-1 pt-3">
        <p className="text-[12.5px] leading-snug text-fg-2">{options ? t("accounts.product.optionsText") : spreadType(g, t)}</p>
        <div className="mt-3 grid grid-cols-2 gap-x-3 gap-y-2">
          {specs.map(([k, v]) => (
            <Spec key={k} label={k} value={v} />
          ))}
        </div>
        {!compact && !options && (
          <ul className="mt-3 space-y-1.5">
            {groupFeatures(g, t).slice(1).map((f) => (
              <li key={f} className="flex items-start gap-2 text-[12px] leading-snug text-fg-2">
                <Check className="mt-0.5 size-3 shrink-0 text-ember" /> {f}
              </li>
            ))}
          </ul>
        )}
        {used !== undefined && (
          <div className="mt-auto pt-3">
            <Chip size="sm" tone={full ? "warn" : "neutral"}>
              {full ? t.dyn(`accounts.groupCard.limitReached.${kind ?? "any"}`, undefined, { max: g.maxAccountsPerUser }) : t.dyn(`accounts.groupCard.used.${kind ?? "any"}`, undefined, { used, max: g.maxAccountsPerUser })}
            </Chip>
          </div>
        )}
      </div>
    </Comp>
  );
}

function Spec({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0">
      <div className="truncate text-[10.5px] font-semibold uppercase tracking-[0.08em] text-fg-3">{label}</div>
      <div className="k-num mt-0.5 truncate text-[13.5px] font-semibold text-fg">{value}</div>
    </div>
  );
}
