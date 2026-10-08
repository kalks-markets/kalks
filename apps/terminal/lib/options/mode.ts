"use client";

// CFD or Options workspace (CFD / Options account split): the ACTIVE ACCOUNT's product decides — a CFD account opens
// the CFD workspace, an Options account the options workspace (TerminalProvider provides it through
// AccountProductContext). Without an engine account (guest chart mode, demo builds with mock accounts) a CFD | Options
// switch held in memory decides; nothing is stored in the browser any more. `?mode=options` (and `?u=EURUSD` for the
// underlying) from the Client Area's Options page or the public option chain picks the client's Options account on
// start (components/terminal.tsx liveEntry) or, without accounts, the Options side of the switch.
import * as React from "react";
import { parseMode, workspaceOf, type Product } from "./product";

export type TradeMode = Product;

/** The browser key the old per-browser mode lived under (removed once). */
const OLD_KEY = "kalks.terminal.mode";
/** Underlying asked for by a link (`?u=`), read once by the options workspace. */
export const LINK_UNDERLYING_KEY = "kalks.options.link-u";

function rememberUnderlying(u: string | null | undefined) {
  const sym = u?.toUpperCase();
  if (!sym || !/^[A-Z]{5,7}$/.test(sym)) return;
  try {
    sessionStorage.setItem(LINK_UNDERLYING_KEY, sym);
  } catch {
    /* storage blocked */
  }
}

function initial(): TradeMode {
  if (typeof window === "undefined") return "cfd";
  try {
    localStorage.removeItem(OLD_KEY);
  } catch {
    /* storage blocked */
  }
  const sp = new URLSearchParams(window.location.search);
  const m = parseMode(sp.get("mode"));
  if (m === "options") rememberUnderlying(sp.get("u"));
  return m ?? "cfd";
}

/** The switch without an engine account (guest, demo builds). */
let manual: TradeMode = initial();
const listeners = new Set<() => void>();

export function getManualMode() {
  return manual;
}

/** Guest and demo builds: show the CFD or the Options workspace. With an engine account the account decides (switch
 *  accounts instead, shell/mode-switch.tsx), so this only matters when there is none. */
export function setTradeMode(next: TradeMode) {
  if (next === manual) return;
  manual = next;
  listeners.forEach((l) => l());
}

/**
 * A link asked for a product (Client Area `/options` SSO hand-off `…&mode=options`, the public chain's "Trade" button,
 * `?mode=options` on a normal or guest load): remember `u` for the options workspace and, without an engine account,
 * show that side. True when the link named a product.
 */
export function applyLinkMode(m: string | null | undefined, u?: string | null) {
  const want = parseMode(m);
  if (!want) return false;
  if (want === "options") rememberUnderlying(u);
  setTradeMode(want);
  return true;
}

function subscribe(l: () => void) {
  listeners.add(l);
  return () => void listeners.delete(l);
}

/** The active engine account's product (null: no engine account, the switch decides). */
export const AccountProductContext = React.createContext<TradeMode | null>(null);

/** The workspace on screen: the active account's product, else the in-memory switch. */
export function useTradeMode(): TradeMode {
  const m = React.useSyncExternalStore(subscribe, getManualMode, () => "cfd" as TradeMode);
  return workspaceOf(React.useContext(AccountProductContext), m);
}
