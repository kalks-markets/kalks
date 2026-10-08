"use client";

import { Check } from "lucide-react";
import { Chip, cn } from "@/components/kit";
import { useT } from "@kalks/i18n/react";
import type { T } from "@kalks/i18n";
import { modeLabel, type EngineGroup } from "./api";
import { productOf } from "@/lib/products";

const PHOTO: Record<string, string> = {
  standard: "finance",
  pro: "trading-screen",
  "pro-netting": "charts",
  ecn: "analytics",
  cent: "money",
  vip: "skyscrapers",
  prop: "trader",
  "options-standard": "charts",
  "options-pro": "analytics",
};

export const groupPhoto = (g: Pick<EngineGroup, "code">) => `/assets/photos/${PHOTO[g.code] ?? "stock-market"}.jpg`;

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
  return (
    <Comp
      type={onSelect ? "button" : undefined}
      onClick={full ? undefined : onSelect}
      disabled={onSelect ? full : undefined}
      aria-pressed={onSelect ? !!selected : undefined}
      className={cn(
        "group relative flex h-full w-full flex-col overflow-hidden rounded-[20px] border text-start transition-colors duration-200",
        selected ? "border-ember/60 bg-surface shadow-[0_0_0_4px_color-mix(in_oklab,var(--k-ember)_12%,transparent)]" : "k-card hover:border-[var(--k-border-top)]",
        full && "cursor-not-allowed opacity-55",
      )}
    >
      <div className={cn("relative overflow-hidden", compact ? "h-20" : "h-28")}>
        {/* eslint-disable-next-line @next/next/no-img-element */}
        <img src={groupPhoto(g)} alt="" className="absolute inset-0 size-full object-cover" />
        <div className="absolute inset-0 bg-gradient-to-t from-[var(--k-surface)] via-black/40 to-black/10" />
        <div className="absolute bottom-3 start-4 end-4 flex items-end justify-between gap-2">
          <div className="min-w-0">
            <div className="truncate text-[19px] font-semibold tracking-tight text-white">{g.name}</div>
            <div className="text-[11px] font-medium uppercase tracking-wider text-white/75">
              {options ? t("accounts.product.chipOptions") : t.dyn(`accounts.mode.${g.mode}`, modeLabel(g.mode))}
              {g.cent && <span className="text-gold"> · {t("accounts.groupCard.uscCentAlt")}</span>}
            </div>
          </div>
          {selected && (
            <span className="grid size-7 shrink-0 place-items-center rounded-full bg-ember text-white">
              <Check className="size-4" />
            </span>
          )}
        </div>
      </div>
      <div className="flex flex-1 flex-col px-4 pb-4 pt-3">
        <p className="text-[12.5px] leading-snug text-fg-2">{options ? t("accounts.product.optionsText") : spreadType(g, t)}</p>
        <div className="mt-3 grid grid-cols-2 gap-2">
          {!options && <Spec label={t("accounts.label.commission")} value={commissionText(g, t)} />}
          <Spec label={t("accounts.label.minDeposit")} value={g.minDeposit > 0 ? `$${g.minDeposit.toLocaleString("en-US")}` : t("common.none")} />
          {!options && <Spec label={t("accounts.label.maxLeverage")} value={`1:${maxLeverage(g).toLocaleString("en-US")}`} />}
          <Spec label={t("accounts.label.stopOut")} value={`${g.stopOutPct}%`} />
        </div>
        {!compact && !options && (
          <ul className="mt-3 space-y-1.5">
            {groupFeatures(g, t).slice(1).map((f) => (
              <li key={f} className="flex items-start gap-2 text-[12px] leading-snug text-fg-2">
                <Check className="mt-0.5 size-3 shrink-0 text-gold" /> {f}
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
    <div className="rounded-xl border border-line bg-surface-2 px-2.5 py-2">
      <div className="text-[11px] text-fg-3">{label}</div>
      <div className="k-num mt-0.5 truncate text-[13px] font-semibold text-fg">{value}</div>
    </div>
  );
}
