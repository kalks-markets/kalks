/**
 * POST /api/options/explain  { locale, strategy }
 *
 * Kalks FX Options simple mode: Claude explains an option idea (legs, cost, max loss / profit, breakeven) in plain
 * language, in the reader's language. Follows app/api/ai-trader/route.ts: the key is read from the server
 * environment only; without ANTHROPIC_API_KEY it answers { configured: false } and the terminal shows its built-in
 * explanation. Same-origin only, small bodies, a signed-in terminal session and a per-user budget (lib/ai-guard.ts:
 * 401 "signin", 429 "rate_minute" / "rate_day"), and Claude is told to use only the numbers given.
 */
import Anthropic from "@anthropic-ai/sdk";
import type { NextRequest } from "next/server";
import { LOCALES } from "@kalks/i18n/locales";
import { sameOrigin } from "@/lib/engine/server";
import { aiGate } from "@/lib/ai-guard";
import { moduleOn } from "@/lib/tenant-brand";
import { hostOf } from "@/lib/tenant-host";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

const MODEL = "claude-opus-5-5";

const SYSTEM = `You explain one options trading idea to a retail trader of Kalks FX Options, in plain everyday language.
Product facts (apply them, never contradict them):
- European options on forex, gold, silver and oil, cash-settled in USD. At the cut (normally 10:00 New York) an option that is in the money pays automatically; one that is out of the money expires worthless. The settlement price is the average of the mid price over the last 30 minutes before the cut.
- A buyer pays the premium upfront and can never lose more than that. A seller receives the premium, needs margin, and can lose more than the premium.
- The position can be closed at any time before the cut at the current price.
Rules:
- Use only the numbers you are given (cost, max profit, max loss, breakevens, probability). Never invent prices or outcomes, never promise profit, never recommend whether to trade.
- Cover: what the idea bets on, what it costs now, when and how it makes money (breakeven), the most it can lose, how time passing affects it (time decay), and one plain sentence on the main risk.
- 110 to 170 words, short paragraphs, no headings, no markdown, no emoji. Write it in the requested language; keep symbols, numbers and the currency code as given.`;

/** GET: whether Claude is configured (never exposes the key); not while the broker has Options or AI switched off. */
export async function GET(req: NextRequest) {
  const on = await moduleOn("options+ai", hostOf(req.headers));
  return Response.json({ configured: on && !!process.env.ANTHROPIC_API_KEY, model: MODEL, ...(on ? {} : { code: "module_disabled" }) });
}

const num = (v: unknown) => (typeof v === "number" && Number.isFinite(v) ? v : null);
const str = (v: unknown, max: number) => (typeof v === "string" ? v.slice(0, max) : "");

function bad(status: number, error: string) {
  return Response.json({ error }, { status });
}

export async function POST(req: NextRequest) {
  // module switches: the broker switched Options or AI off
  if (!(await moduleOn("options+ai", hostOf(req.headers)))) return Response.json({ configured: false, error: "AI isn't available on your account.", code: "module_disabled" }, { status: 403 });
  if (!sameOrigin(req)) return bad(403, "Cross-site request blocked.");
  const raw = await req.text();
  if (raw.length > 8000) return bad(413, "Request too large.");
  let body: { locale?: unknown; strategy?: Record<string, unknown> };
  try {
    body = JSON.parse(raw);
  } catch {
    return bad(400, "Invalid JSON body");
  }
  const s = body.strategy;
  if (!s || typeof s !== "object") return bad(400, "Missing strategy");
  const underlying = str(s.underlying, 8).toUpperCase();
  if (!/^[A-Z]{3,8}$/.test(underlying)) return bad(400, "Invalid underlying");
  const legsIn = Array.isArray(s.legs) ? s.legs.slice(0, 8) : [];
  if (!legsIn.length) return bad(400, "No legs");
  const legs = legsIn.map((l) => {
    const x = (l ?? {}) as Record<string, unknown>;
    return { side: x.side === "sell" ? "sell" : "buy", right: x.right === "put" ? "put" : "call", strike: num(x.strike), contracts: num(x.contracts), premiumUsdPerContract: num(x.premiumUsd), iv: num(x.iv), delta: num(x.delta) };
  });
  const locale = LOCALES.find((l) => l.code === body.locale) ?? LOCALES[0]!;

  if (!process.env.ANTHROPIC_API_KEY) return Response.json({ configured: false });
  const refused = await aiGate(req);
  if (refused) return refused;

  const facts = {
    idea: str(s.name, 60),
    view: s.view === "down" ? "the price goes down" : "the price goes up",
    underlying,
    expiry: str(s.expiry, 10),
    cut: str(s.cut, 40),
    spot: num(s.spot),
    contractSize: num(s.contractSize),
    contractUnit: str(s.contractUnit, 10),
    legs,
    netPremiumUsd: num(s.netPremium),
    maxProfitUsd: s.maxProfit === null ? "unlimited" : num(s.maxProfit),
    maxLossUsd: s.maxLoss === null ? "unlimited" : num(s.maxLoss),
    breakevens: Array.isArray(s.breakevens) ? s.breakevens.slice(0, 4).map(num) : [],
    probabilityOfProfit: num(s.probProfit),
  };

  const client = new Anthropic({ apiKey: process.env.ANTHROPIC_API_KEY });
  try {
    const msg = await client.beta.messages.create({
      model: MODEL,
      max_tokens: 4000,
      thinking: { type: "adaptive" },
      output_config: { effort: "low" },
      betas: ["server-side-fallback-2026-07-01"],
      fallbacks: "default",
      system: SYSTEM,
      messages: [{ role: "user", content: `Language: ${locale.english} (${locale.code}).\nExplain this idea:\n${JSON.stringify(facts, null, 1)}` }],
    });
    if (msg.stop_reason === "refusal") return Response.json({ configured: true, error: "The model declined this request." }, { status: 422 });
    const text = msg.content
      .map((b) => (b.type === "text" ? b.text : ""))
      .join("")
      .trim();
    if (!text) return Response.json({ configured: true, error: "No explanation came back. Try again." }, { status: 502 });
    return Response.json({ configured: true, model: msg.model, text: text.slice(0, 2400) });
  } catch (e) {
    if (e instanceof Anthropic.AuthenticationError) return Response.json({ configured: false, error: "ANTHROPIC_API_KEY was rejected (401)." }, { status: 401 });
    if (e instanceof Anthropic.RateLimitError) return Response.json({ configured: true, error: "Claude is rate limited. Try again shortly." }, { status: 429 });
    if (e instanceof Anthropic.BadRequestError) return Response.json({ configured: true, error: `Request rejected: ${e.message}` }, { status: 400 });
    if (e instanceof Anthropic.APIError) return Response.json({ configured: true, error: `Claude API error ${e.status ?? ""}: ${e.message}` }, { status: 502 });
    return Response.json({ configured: true, error: "Could not reach the Claude API." }, { status: 502 });
  }
}
