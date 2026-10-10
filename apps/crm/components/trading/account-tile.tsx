"use client";

// An account on the Accounts page as a debit card (founder 2026-10-10: "all account cards like the debit card"), with
// its actions under it. Opening the card opens the account's page.

import * as React from "react";
import Link from "next/link";
import { useT } from "@kalks/i18n/react";
import { AccountVisual, type CardAccount } from "@/components/dashboard/home/accounts-panel";
import { useReadOnly } from "@/components/session";
import { curOf, fmtLevel, serverOf, type EngineAccount } from "@/components/trading/api";
import { AccountActions, FundButton, RefillButton, TradeButton, isPropAccount } from "@/components/trading/ui";

export function cardOf(a: EngineAccount, t: ReturnType<typeof useT>): CardAccount {
  return {
    login: String(a.login),
    type: a.type,
    prop: isPropAccount(a),
    title: `${a.groupName} · ${t.dyn(`accounts.mode.${a.mode}`, a.mode)}`,
    name: a.name || null,
    currency: curOf(a),
    cent: a.cent,
    balance: a.balance,
    equity: a.equity,
    freeMargin: a.freeMargin,
    marginLevel: a.marginLevel,
    leverage: a.leverage,
    server: serverOf(a),
    positions: a.positions,
    product: a.product === "options" ? "options" : "cfd",
  };
}

export function AccountTile({ a, onChanged }: { a: EngineAccount; onChanged: () => void }) {
  const t = useT();
  const readOnly = useReadOnly();
  return (
    <div className="min-w-0">
      <Link href={`/accounts/${a.login}`} className="block rounded-[22px] transition-transform hover:-translate-y-0.5" aria-label={`#${a.login} · ${a.groupName}`}>
        <AccountVisual a={cardOf(a, t)} />
      </Link>
      <div className="mt-2.5 flex items-center justify-between gap-2 px-1 text-[12px] text-fg-3">
        <span className="truncate">
          {t.dyn("accounts.card.positions", "{count} open positions", { count: a.positions ?? 0 })} · {t.dyn("accounts.card.marginLevel", "Margin level")} {fmtLevel(a.marginLevel)}
        </span>
        <Link href={`/accounts/${a.login}`} className="shrink-0 font-semibold text-fg-2 hover:text-ember">
          {t.dyn("common.details", "Details")}
        </Link>
      </div>
      {!readOnly && (
        <div className="mt-3 flex items-center gap-2">
          <TradeButton a={a} size="md" className="flex-1" />
          {a.type === "live" ? !isPropAccount(a) && <FundButton a={a} size="md" /> : <RefillButton a={a} onDone={onChanged} size="md" />}
          <AccountActions a={a} onChanged={onChanged} />
        </div>
      )}
    </div>
  );
}
