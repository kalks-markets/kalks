// Engine / options-service rejections → what the client reads. The codes come from the engine's options API
// (services/trading) and the options service (services/options); unknown codes fall back to the server's message.
import { tr } from "@kalks/i18n/react";
import type { EngineErr } from "@/lib/engine/map";
import { CLIENT_AREA } from "@/lib/guest";
import type { Reason } from "./types";

export const OPTION_ERROR_CODES = [
  "options_disabled",
  // CFD / Options account split: options on a CFD account
  "product_mismatch",
  "not_eligible",
  "market_closed",
  "cutoff",
  "closed",
  "series_halted",
  "close_only",
  "limit_contracts",
  "insufficient_cash",
  "insufficient_margin",
  "stale_prices",
  "no_price",
  "read_only",
  "unavailable",
  "session_expired",
  // order book (docs/OPTIONS-EXCHANGE.md §2, §5): rejections and why an order stopped working
  "price_out_of_band",
  "would_take",
  "reduce_only",
  "fok_not_filled",
  "self_trade",
  "settling",
  "rate_limited",
  "limit_orders",
  "invalid_price",
  "invalid_trigger",
  "post_only_gtc",
  "bad_expiry",
  "no_liquidity",
  "book_closed",
  "quote_expired",
  "rfq_expired",
  "price_moved",
  "ioc_remainder",
  "expiry",
  "session_reset",
  "not_found",
  // combo RFQ and strategy closes (docs/OPTIONS-EXCHANGE.md §5, §14)
  "series_cancel_only",
  "series_closed",
  "rfq_underlyings",
  "kalks_quoted",
  "mixed_venue",
  "book_venue",
] as const;

/**
 * Rejections of a strategy request for quote (or of closing a strategy on the order book) that read better in that
 * context than the order-book wording: the price was a firm quote from the market maker, not a resting order.
 */
const RFQ_ERROR_CODES = ["quote_expired", "price_moved", "rfq_expired", "reduce_only", "self_trade", "no_price", "no_liquidity", "rejected"] as const;

/** A strategy (RFQ / strategy close) rejection in plain words. */
export function rfqErrorText(code: string, message?: string): string {
  if ((RFQ_ERROR_CODES as readonly string[]).includes(code)) return tr.dyn(`trader.opt.rfq.err.${code}`, message ?? code);
  return optionErrorText(code, message);
}

/** The price only went stale: a fresh quote fixes it (offer "Get a new price"). */
export const rfqRequotable = (code: string) => code === "quote_expired" || code === "price_moved" || code === "no_price" || code === "no_liquidity";

/** Where a client takes the one quick options step: the 1-minute options intro in the Client Area ("I understand"). */
export const ONBOARDING_URL = `${CLIENT_AREA}/options`;

export function reasonCode(r: Reason): string {
  return typeof r === "string" ? r : (r.code ?? "");
}

/** Translated text of a rejection code (or the server's message when the code is unknown). */
export function optionErrorText(code: string, message?: string): string {
  if ((OPTION_ERROR_CODES as readonly string[]).includes(code)) return tr.dyn(`trader.opt.err.${code}`, message ?? code);
  return message || code.replace(/_/g, " ");
}

export function reasonText(r: Reason): string {
  return typeof r === "string" ? optionErrorText(r) : optionErrorText(r.code ?? "", r.message);
}

export function errText(e: EngineErr): string {
  return optionErrorText(e.code, e.message);
}

/** The rejection asks the client to read the options intro in the Client Area first (a friendly note, not an error). */
export const needsOnboarding = (code: string) => code === "not_eligible";
