/**
 * POST /api/ai-trader  { prompt, symbol?, timeframe? }
 *
 * Converts a plain-language trading instruction into a StrategySpec with Claude (structured output),
 * validated server-side. Without ANTHROPIC_API_KEY it answers { configured: false } and the terminal
 * falls back to its local parser. The key is read from the server environment only.
 * Each call costs money: it needs a signed-in terminal session and stays within the per-user budget
 * (lib/ai-guard.ts: 401 "signin", 429 "rate_minute" / "rate_day").
 */
import Anthropic from "@anthropic-ai/sdk";
import type { NextRequest } from "next/server";
import { INSTRUMENTS } from "@kalks/mock";
import { aiGate } from "@/lib/ai-guard";
import { moduleOn } from "@/lib/tenant-brand";
import { hostOf } from "@/lib/tenant-host";
import { PARSE_RESULT_JSON_SCHEMA, validateSpec, type ParseResult } from "@/lib/ai-trader/schema";
import { TIMEFRAMES } from "@/lib/trading";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

const MODEL = "claude-opus-5-5";

const SYSTEM = `You convert a trader's plain-language instructions into a strategy for the Kalks Trader terminal's rule engine.
The strategy is shown to the trader as a card that they review, edit and explicitly activate; it trades real or demo money, so never invent aggressive settings.

Engine semantics:
- Rules are evaluated on each CLOSED bar of the strategy timeframe. A condition compares two operands with gt, lt, gte, lte, crosses_above (was <= on the previous bar and is > now) or crosses_below.
- Operands: price field of the bar; an indicator (sma, ema, rsi, macd line, macd_signal, macd_hist, bb_upper/bb_middle/bb_lower, atr, stoch_k, stoch_d, highest = highest high of the N bars before the current bar, lowest = lowest low of the N bars before); a constant value; or a candle pattern (value 1 when present: write it as candle >= 1).
- "Price above EMA 200" means close gt ema(200). "RSI(14) crosses above 30" means rsi(14) crosses_above 30. "MACD crosses above signal" means macd crosses_above macd_signal with the same periods. "Breaks the 20-bar high" means close crosses_above highest(20).
- A condition's timeframe is "same" unless the trader explicitly asks for another timeframe for that condition (e.g. "EMA 50 on H1" inside an M5 strategy).
- long = buy entry rules, short = sell entry rules, exitLong/exitShort = rule-based exits (separate from SL/TP). Leave a rule set with an empty groups array when it does not apply. Use one group with logic "all" for simple AND lists; use several groups joined with logic "any" for OR.
- Points are MT5 points: 1 point = 1 / 10^digits of the symbol (XAUUSD 2 digits: 150 points = 1.50; EURUSD 5 digits: 10 points = 1 pip). Keep the unit the trader used (points, pips, ATR multiples, price distance, percent, absolute level, or R multiple for TP).
- Trailing uses a price distance that starts moving the stop once the trade is that far in profit. Breakeven moves the stop to entry (+ offset points) once profit reaches breakevenTrigger points.
- sessions are server-time windows (HH:MM, broker server time GMT+3). days are server weekdays 0=Sunday..6=Saturday.
- sizing: fixed lots, or risk % of balance per trade (needs a stop loss). maxLots is a hard cap per order: set it equal to the lot size unless the trader gave a cap; for risk sizing use the trader's cap or 1.
- maxTradesPerDay 0 means unlimited; maxDailyLoss 0 means off; oneAtATime defaults to true.

Rules for you:
- Use only symbols from the provided list (map "gold" to XAUUSD, "bitcoin" to BTCUSD, "nasdaq" to NAS100, and so on).
- If the symbol, the side/entry condition, or the size is missing or genuinely ambiguous, set status "needs_clarification" and ask short, specific questions. Still return your best draft (use the chart context for a missing symbol/timeframe) so the trader can edit it.
- A missing stop loss is allowed but must be raised as a question.
- List every default you filled in and every interpretation you made in assumptions.
- Never add conditions, limits or indicators the trader did not ask for.`;

/** GET: whether Claude is configured (never exposes the key); not while the broker has AI switched off. */
export async function GET(req: NextRequest) {
  const on = await moduleOn("ai", hostOf(req.headers));
  return Response.json({ configured: on && !!process.env.ANTHROPIC_API_KEY, model: MODEL, ...(on ? {} : { code: "module_disabled" }) });
}

function bad(status: number, error: string) {
  return Response.json({ error }, { status });
}

export async function POST(req: NextRequest) {
  // module switches: the broker switched AI off
  if (!(await moduleOn("ai", hostOf(req.headers)))) return Response.json({ configured: false, error: "AI isn't available on your account.", code: "module_disabled" }, { status: 403 });
  let body: { prompt?: unknown; symbol?: unknown; timeframe?: unknown };
  try {
    body = await req.json();
  } catch {
    return bad(400, "Invalid JSON body");
  }
  const prompt = typeof body.prompt === "string" ? body.prompt.trim() : "";
  if (!prompt) return bad(400, "Prompt is empty");
  if (prompt.length > 4000) return bad(400, "Prompt is too long (max 4000 characters)");
  const symbol = typeof body.symbol === "string" ? body.symbol : "EURUSD";
  const timeframe = typeof body.timeframe === "string" && (TIMEFRAMES as readonly string[]).includes(body.timeframe) ? body.timeframe : "H1";

  if (!process.env.ANTHROPIC_API_KEY) {
    return Response.json({ configured: false, error: "AI not configured: set ANTHROPIC_API_KEY in apps/terminal/.env.local and restart the terminal." });
  }
  const refused = await aiGate(req);
  if (refused) return refused;

  const client = new Anthropic({ apiKey: process.env.ANTHROPIC_API_KEY });
  const symbols = INSTRUMENTS.map((i) => `${i.symbol} (${i.name}, ${i.digits} digits)`).join("; ");
  try {
    const msg = await client.beta.messages.create({
      model: MODEL,
      max_tokens: 16000,
      thinking: { type: "adaptive" },
      betas: ["server-side-fallback-2026-07-01"],
      fallbacks: "default",
      // The strategy schema is too large for constrained decoding, so the model is given the schema and
      // asked for plain JSON; the reply is then checked by validateSpec below like any other input.
      system: `${SYSTEM}\n\nReply with a single JSON object only (no prose, no code fences) that matches this JSON Schema:\n${JSON.stringify(PARSE_RESULT_JSON_SCHEMA)}`,
      messages: [
        {
          role: "user",
          content: `Available symbols: ${symbols}\nActive chart: ${symbol} ${timeframe}\n\nTrader's instructions:\n${prompt}`,
        },
      ],
    });
    if (msg.stop_reason === "refusal") return Response.json({ configured: true, error: "The model declined this request." }, { status: 422 });
    if (msg.stop_reason === "max_tokens") return Response.json({ configured: true, error: "The model response was cut off. Shorten the prompt and try again." }, { status: 502 });
    const text = msg.content.map((b) => (b.type === "text" ? b.text : "")).join("");
    const json = text.slice(text.indexOf("{"), text.lastIndexOf("}") + 1);
    let raw: Partial<ParseResult>;
    try {
      raw = JSON.parse(json) as Partial<ParseResult>;
    } catch {
      return Response.json({ configured: true, error: "The model returned invalid JSON." }, { status: 502 });
    }
    const { spec, errors, warnings } = validateSpec(raw.strategy);
    const questions = [...(Array.isArray(raw.questions) ? raw.questions.map(String) : []), ...errors.filter((e) => !(raw.questions ?? []).some((q) => String(q).includes(e)))];
    const result: ParseResult = {
      status: raw.status === "ok" && !errors.length ? "ok" : "needs_clarification",
      questions,
      assumptions: Array.isArray(raw.assumptions) ? raw.assumptions.map(String) : [],
      strategy: spec,
    };
    return Response.json({ configured: true, model: msg.model, result, warnings });
  } catch (e) {
    if (e instanceof Anthropic.AuthenticationError) return Response.json({ configured: false, error: "ANTHROPIC_API_KEY was rejected (401). Check the key." }, { status: 401 });
    if (e instanceof Anthropic.RateLimitError) return Response.json({ configured: true, error: "Claude is rate limited. Try again shortly." }, { status: 429 });
    if (e instanceof Anthropic.BadRequestError) return Response.json({ configured: true, error: `Request rejected: ${e.message}` }, { status: 400 });
    if (e instanceof Anthropic.APIError) return Response.json({ configured: true, error: `Claude API error ${e.status ?? ""}: ${e.message}` }, { status: 502 });
    return Response.json({ configured: true, error: "Could not reach the Claude API." }, { status: 502 });
  }
}
