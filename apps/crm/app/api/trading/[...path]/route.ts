import { NextResponse, type NextRequest } from "next/server";
import { SESSION_COOKIE, consumeStepup, stepupTokenOf, type GatewayUser, type StepupAction } from "@/lib/gateway";
import { TERMINAL_BASE, clientAccount, clientDeal, clientOrder, clientPosition, engine, sameOrigin, sessionUser } from "@/lib/trading";
import { viewerHasAccount } from "@/lib/viewer";
import { tenantConfig } from "@/lib/tenant-config";
import { Memo } from "@/lib/memo";
import { wallet } from "@/lib/wallet";
import { reportsFetch } from "@/lib/reports";
import { dealCommission, dealPremiumsUsd, isOptionTrade, matchesInstrument, optionTerms, usdFactorOf, type DealOption } from "@/components/trading/option-deal";

// Client Area trading BFF. Browser -> /api/trading/<route> (same origin) -> trading engine /v1/…
// The client is resolved from the HttpOnly gateway session cookie (gateway /v1/auth/me); the engine gets
// that user id in X-Kalks-User-Id and returns 404 for accounts the user doesn't own. A user id sent by the
// browser is never used. CSRF: cookies are SameSite=Lax, POSTs must be JSON with a same-origin Origin.
//
//   GET  groups                              open-account groups and their specs
//   GET  accounts                            the client's accounts (live metrics)
//   POST accounts                            {type, group, leverage?, name?, password?, initialBalance?}
//   GET  accounts/{login}                    {account, positions[], orders[]}
//   GET  accounts/{login}/history?from&to&page&limit[&instrument=option|cfd]
//                                            instrument: only Kalks FX Options deals (or only CFD deals); the BFF pages
//                                            through the period itself so the paging and totals match the filter
//   GET  accounts/{login}/ledger?from&to&page&limit
//   GET  accounts/{login}/export?kind=history|ledger&from&to[&instrument=option|cfd]   CSV download (times in UTC)
//   POST accounts/{login}/demo-refill
//   POST accounts/{login}/passwords          {kind: trading|investor, password, stepup_token}
//   POST accounts/{login}/leverage           {leverage, stepup_token}
//
// Passwords and leverage need an emailed-code confirmation (D20): the browser gets a step-up token from
// /api/auth/stepup-verify (action trading_password | investor_password | leverage, target = login) and sends it
// as `stepup_token` (or X-Kalks-Stepup). It is checked against the account first, then redeemed once with the
// gateway, and only then does the engine make the change.
//   POST accounts/{login}/sso                {url, expiresAt}: url = NEXT_PUBLIC_TERMINAL_URL + "/?sso=<token>"
//   GET  accounts/{login}/archive-check      {canArchive, needsEmpty, positions, orders, balance, credit, bonus, blockers[]}
//   POST accounts/{login}/archive            {empty, ackForfeit, stepup_token?}: live accounts need step-up action
//                                            account_archive (target = login); demo accounts don't
//   POST accounts/{login}/restore
//   PATCH accounts/{login}                   {name} (≤ 32 characters, empty clears it)
//   GET  accounts/{login}/closure            {canRequest, needsEmpty, blockers[], surveyReasons[], request}
//   POST accounts/{login}/closure            {reasonCode, survey: {reasons[], comment}, empty, ackForfeit, stepup_token}:
//                                            close permanently (live only, step-up action account_close, target = login)
//   POST accounts/{login}/closure/cancel     withdraw the pending request
//   GET  accounts/{login}/group-options      account types the account can move to (with why not)
//   POST accounts/{login}/group              {group}: self-service type change (flat, minimum deposit met)
//   POST accounts/{login}/demo-balance       {amount}: demo refill to a chosen balance
//   GET  accounts/{login}/health             health card
//   GET  accounts/{login}/history-zip        full history ZIP (reports service: statement PDF + CSV + Excel)
//   GET  prefs  /  POST prefs                {defaultLogin}: the starred default account
//   POST transfers/between                   {fromLogin, toLogin, amount, idempotency_key, stepup_token}: live -> live
//                                            between own accounts (wallet trading-to-trading; step-up internal_transfer,
//                                            target = fromLogin)
// Archive / restore / rename / close / prefs / transfers are the owner's own: view-only logins (D90) and staff
// impersonation sessions are refused.

type Obj = Record<string, unknown>;
type Ctx = { params: Promise<{ path: string[] }> };

const NO_STORE = { "cache-control": "no-store" };
const LOGIN_RE = /^\d{8}$/;
/** Open-account groups per broker (the same for every client; Back Office edits show within the TTL). */
const groupsCache = new Memo<{ status: number; data: { groups?: Obj[] } }>(15_000, 500);
const DATE_RE = /^\d{4}-\d{2}-\d{2}(T[\d:.]+(Z|[+-]\d{2}:\d{2})?)?$/;

/** Engine groups reserved for prop-challenge accounts (same rule as the prop service and the wallet). */
const isPropGroup = (code: unknown) => typeof code === "string" && code.toLowerCase().startsWith("prop");

function error(status: number, code: string, message: string) {
  return NextResponse.json({ error: { code, message } }, { status, headers: NO_STORE });
}

function reply(status: number, data: unknown) {
  return NextResponse.json(data, { status, headers: NO_STORE });
}

async function auth(req: NextRequest): Promise<GatewayUser | NextResponse> {
  const user = await sessionUser(req);
  if (user === "unavailable") return error(503, "unavailable", "Sign-in service is unavailable. Please try again shortly.");
  if (!user) return error(401, "unauthorized", "Please sign in.");
  return user;
}

/** from / to / page / limit, validated before they reach the engine. */
function pageQuery(req: NextRequest, maxLimit = 200): string | NextResponse {
  const sp = req.nextUrl.searchParams;
  const out = new URLSearchParams();
  for (const k of ["from", "to"] as const) {
    const v = sp.get(k);
    if (!v) continue;
    if (!DATE_RE.test(v)) return error(400, "bad_request", `Invalid ${k} date.`);
    out.set(k, v);
  }
  for (const [k, max] of [["page", 100000], ["limit", maxLimit]] as const) {
    const v = sp.get(k);
    if (!v) continue;
    const n = Number(v);
    if (!Number.isInteger(n) || n < 1 || n > max) return error(400, "bad_request", `Invalid ${k}.`);
    out.set(k, String(n));
  }
  const s = out.toString();
  return s ? `?${s}` : "";
}

export async function GET(req: NextRequest, { params }: Ctx) {
  const path = (await params).path;
  const user = await auth(req);
  if (user instanceof NextResponse) return user;

  if (path.length === 1 && path[0] === "groups") {
    // the broker's group catalogue is the same for all its clients: shared briefly per broker
    const r = await groupsCache.get(user.tenant?.slug ?? "", () => engine<{ groups?: Obj[] }>("/v1/groups", { user, req }), (x) => x.status === 200);
    if (r.status !== 200) return reply(r.status, r.data);
    // spread group / route are dealing details; the client sees the commercial terms only
    // prop* groups hold prop-challenge accounts only (opened by the prop service; the wallet refuses transfers to them)
    const groups = (r.data.groups ?? []).filter((g) => !isPropGroup(g.code)).map(({ route: _r, tenantId: _t, ...g }) => g);
    return reply(200, { groups });
  }

  if (path.length === 1 && path[0] === "prefs") {
    const r = await engine<Obj>("/v1/accounts/prefs", { user, req });
    return reply(r.status, r.data);
  }

  if (path.length === 1 && path[0] === "accounts") {
    const r = await engine<{ accounts?: unknown[] }>("/v1/accounts", { user, req });
    if (r.status !== 200) return reply(r.status, r.data);
    let accounts = (r.data.accounts ?? []).map(clientAccount).filter(Boolean) as Obj[];
    // a view-only login (D90) sees only the accounts it was given
    if (user.viewer) accounts = accounts.filter((a) => viewerHasAccount(user.viewer!, String(a.login)));
    return reply(200, { accounts });
  }

  const login = path[1];
  if (path[0] !== "accounts" || !login || !LOGIN_RE.test(login)) return error(404, "not_found", "Not found.");
  if (user.viewer && !viewerHasAccount(user.viewer, login)) return error(404, "not_found", "Account not found.");

  if (path.length === 2) {
    const r = await engine<{ account?: unknown; positions?: unknown[]; orders?: unknown[] }>(`/v1/accounts/${login}`, { user, req });
    if (r.status !== 200) return reply(r.status, r.data);
    if (!r.data.account) return error(404, "not_found", "Account not found.");
    return reply(200, {
      account: clientAccount(r.data.account),
      positions: (r.data.positions ?? []).map(clientPosition),
      orders: (r.data.orders ?? []).map(clientOrder),
    });
  }

  if (path.length === 3 && (path[2] === "history" || path[2] === "ledger")) {
    const q = pageQuery(req);
    if (q instanceof NextResponse) return q;
    const inst = req.nextUrl.searchParams.get("instrument");
    if (path[2] === "history" && inst && inst !== "all") {
      if (inst !== "option" && inst !== "cfd") return error(400, "bad_request", "instrument must be option or cfd.");
      return filteredHistory(req, user, login, inst);
    }
    const r = await engine<Obj>(`/v1/accounts/${login}/${path[2]}${q}`, { user, req });
    if (r.status !== 200 || path[2] === "ledger") return reply(r.status, r.data);
    const d = r.data as { deals?: unknown[]; orders?: unknown[] };
    return reply(200, { ...r.data, deals: (d.deals ?? []).map(clientDeal), orders: (d.orders ?? []).map(clientOrder) });
  }

  if (path.length === 3 && path[2] === "export") return exportCsv(req, user, login);

  // B10: the full history as one ZIP (PDF statement + CSV + Excel) from the reports service
  if (path.length === 3 && path[2] === "history-zip") {
    const r = await reportsFetch(`/v1/me/accounts/${login}/history.zip`, user, 120_000);
    if (!r) return error(503, "unavailable", "Statements are unavailable right now. Please try again shortly.");
    if (!r.ok) return reply(r.status, await r.json().catch(() => ({ error: { code: "unavailable", message: "The history could not be prepared." } })));
    return new NextResponse(r.body, {
      status: 200,
      headers: { ...NO_STORE, "content-type": "application/zip", "content-disposition": r.headers.get("content-disposition") ?? `attachment; filename="kalks-${login}-history.zip"`, "x-content-type-options": "nosniff" },
    });
  }

  if (path.length === 3 && ["archive-check", "closure", "group-options", "health"].includes(path[2]!)) {
    const r = await engine<Obj>(`/v1/accounts/${login}/${path[2]}`, { user, req });
    return reply(r.status, r.data);
  }

  return error(404, "not_found", "Not found.");
}

export async function POST(req: NextRequest, { params }: Ctx) {
  const path = (await params).path;
  if (!sameOrigin(req)) return error(403, "forbidden", "Cross-site request blocked.");
  if (!req.headers.get("content-type")?.includes("application/json")) return error(415, "bad_request", "Expected JSON.");
  const body = (await req.json().catch(() => null)) as Obj | null;
  if (body === null || typeof body !== "object" || Array.isArray(body)) return error(400, "bad_request", "Invalid request body.");
  const user = await auth(req);
  if (user instanceof NextResponse) return user;

  if (path.length === 1 && path[0] === "accounts") {
    const type = body.type;
    if (type !== "live" && type !== "demo") return error(422, "validation", "Choose a live or demo account.");
    if (typeof body.group !== "string" || !/^[a-z0-9_-]{1,40}$/i.test(body.group)) return error(422, "validation", "Choose an account type.");
    if (isPropGroup(body.group)) return error(422, "validation", "Prop accounts are opened by buying a prop challenge.");
    // Back Office › Settings › Features: "Demo accounts" off stops new demo accounts (existing ones keep working)
    if (type === "demo" && (await tenantConfig())?.flags.demo_accounts === false) return error(403, "feature_disabled", "Demo accounts aren't available right now.");
    // CFD / Options account split: no new Options account while the broker has the Options module off
    if ((await tenantConfig())?.modules?.options === false) {
      const gs = await groupsCache.get(user.tenant?.slug ?? "", () => engine<{ groups?: Obj[] }>("/v1/groups", { user, req }), (x) => x.status === 200);
      if ((gs.data.groups ?? []).some((g) => g.code === body.group && g.product === "options")) return error(403, "module_disabled", "Options aren't available right now.");
    }
    const open: Obj = { type, group: body.group };
    if (body.leverage !== undefined) {
      if (!Number.isInteger(body.leverage)) return error(422, "validation", "Invalid leverage.");
      open.leverage = body.leverage;
    }
    if (typeof body.name === "string" && body.name.trim()) open.name = body.name.trim().slice(0, 32);
    if (typeof body.password === "string" && body.password) open.password = body.password.slice(0, 64);
    if (type === "demo" && body.initialBalance !== undefined) {
      if (typeof body.initialBalance !== "number" || !Number.isFinite(body.initialBalance)) return error(422, "validation", "Invalid demo balance.");
      open.initialBalance = body.initialBalance;
    }
    const r = await engine<{ account?: unknown; credentials?: Obj }>("/v1/accounts", { user, req, body: open });
    if (r.status !== 200) return reply(r.status, r.data);
    // Generated passwords are returned exactly once, here; they are not stored anywhere by the Client Area.
    const creds = { ...(r.data.credentials ?? {}) };
    if (open.password && !creds.password) creds.password = open.password;
    return reply(200, { account: clientAccount(r.data.account), credentials: creds });
  }

  if (path.length === 1 && path[0] === "prefs") {
    const refused = ownerOnly(req, user);
    if (refused) return refused;
    const d = body.defaultLogin;
    if (d !== null && !(Number.isInteger(d) && LOGIN_RE.test(String(d)))) return error(422, "validation", "Choose an account.");
    const r = await engine(`/v1/accounts/prefs`, { user, req, body: { defaultLogin: d } });
    return reply(r.status, r.data);
  }

  if (path.length === 2 && path[0] === "transfers" && path[1] === "between") return transferBetween(req, user, body);

  const login = path[1];
  if (path[0] !== "accounts" || !login || !LOGIN_RE.test(login)) return error(404, "not_found", "Not found.");
  if (path.length === 4 && path[2] === "closure" && path[3] === "cancel") {
    const refused = ownerOnly(req, user);
    if (refused) return refused;
    const r = await engine(`/v1/accounts/${login}/closure/cancel`, { user, req, body: {} });
    return reply(r.status, r.data);
  }
  if (path.length !== 3) return error(404, "not_found", "Not found.");

  switch (path[2]) {
    case "closure": {
      const refused = ownerOnly(req, user);
      if (refused) return refused;
      if (typeof body.reasonCode !== "string" || typeof body.empty !== "boolean" || typeof body.ackForfeit !== "boolean") return error(422, "validation", "Invalid request body.");
      const sv = (body.survey ?? {}) as Obj;
      const reasons = Array.isArray(sv.reasons) ? sv.reasons.filter((x): x is string => typeof x === "string").slice(0, 10) : [];
      const comment = typeof sv.comment === "string" ? sv.comment.slice(0, 1000) : "";
      // checked before the one-time confirmation is spent: ownership (404), blockers, open request, emptying, forfeit
      const chk = await engine<{ kind?: string; canRequest?: boolean; needsEmpty?: boolean; credit?: number; bonus?: number; blockers?: { code: string; message: string }[]; surveyReasons?: string[] }>(`/v1/accounts/${login}/closure`, { user, req });
      if (chk.status !== 200) return reply(chk.status, chk.data);
      const c = chk.data;
      const blocker = c.blockers?.[0];
      if (blocker) return error(409, blocker.code, blocker.message);
      if (c.canRequest === false) return error(409, "request_pending", "A closure request for this account is already waiting for review.");
      if (c.surveyReasons && !c.surveyReasons.includes(body.reasonCode)) return error(422, "validation", "Choose a reason.");
      if (c.needsEmpty && !body.empty) return error(409, "needs_empty", "Close the open trades and move the money out first.");
      if ((Number(c.credit) || 0) + (Number(c.bonus) || 0) > 0 && !body.ackForfeit) return error(422, "ack_forfeit", "Confirm that the bonus and credit will be lost.");
      const denied = await stepup(req, user, body, "account_close", login);
      if (denied) return denied;
      const r = await engine(`/v1/accounts/${login}/closure`, { user, req, body: { reasonCode: body.reasonCode, survey: { reasons, comment }, empty: body.empty, ackForfeit: body.ackForfeit } });
      return reply(r.status, r.data);
    }
    case "group": {
      const refused = ownerOnly(req, user);
      if (refused) return refused;
      if (typeof body.group !== "string" || !/^[a-z0-9_-]{1,40}$/i.test(body.group) || isPropGroup(body.group)) return error(422, "validation", "Choose an account type.");
      const r = await engine(`/v1/accounts/${login}/group`, { user, req, body: { group: body.group } });
      return reply(r.status, r.data);
    }
    case "demo-balance": {
      const amount = body.amount;
      if (typeof amount !== "number" || !Number.isFinite(amount) || amount < 100 || amount > 1_000_000) return error(422, "validation", "Choose an amount between 100 and 1,000,000.");
      const r = await engine(`/v1/accounts/${login}/demo-balance`, { user, req, body: { amount: Math.round(amount * 100) / 100 } });
      return reply(r.status, r.data);
    }
    case "demo-refill": {
      const r = await engine(`/v1/accounts/${login}/demo-refill`, { user, req, body: {} });
      return reply(r.status, r.data);
    }
    case "passwords": {
      if (body.kind !== "trading" && body.kind !== "investor") return error(422, "validation", "Choose the trading or investor password.");
      const pw = body.password;
      // same rules as the engine, checked before the one-time confirmation is spent
      if (typeof pw !== "string" || pw.length < 8 || pw.length > 64 || !/\p{L}/u.test(pw) || !/\d/.test(pw)) {
        return error(422, "validation", "Use 8 to 64 characters with letters and digits.");
      }
      const own = await ownAccount(req, user, login);
      if (own) return own;
      const denied = await stepup(req, user, body, body.kind === "trading" ? "trading_password" : "investor_password", login);
      if (denied) return denied;
      const r = await engine(`/v1/accounts/${login}/passwords`, { user, req, body: { kind: body.kind, password: pw } });
      return reply(r.status, r.data);
    }
    case "leverage": {
      if (!Number.isInteger(body.leverage)) return error(422, "validation", "Invalid leverage.");
      const acct = await engine<{ account?: { leverage?: number; leverages?: number[]; positions?: number } }>(`/v1/accounts/${login}`, { user, req });
      if (acct.status !== 200 || !acct.data.account) return reply(acct.status === 200 ? 404 : acct.status, acct.data);
      const a = acct.data.account;
      if (Array.isArray(a.leverages) && !a.leverages.includes(body.leverage as number)) return error(422, "invalid_leverage", "This leverage isn't available for the account's group.");
      if ((a.positions ?? 0) > 0) return error(409, "positions_open", "Close all open positions before changing leverage.");
      if (a.leverage === body.leverage) return error(422, "validation", "The account already uses this leverage.");
      const denied = await stepup(req, user, body, "leverage", login);
      if (denied) return denied;
      const r = await engine(`/v1/accounts/${login}/leverage`, { user, req, body: { leverage: body.leverage } });
      return reply(r.status, r.data);
    }
    case "archive": {
      const refused = ownerOnly(req, user);
      if (refused) return refused;
      if (typeof body.empty !== "boolean" || typeof body.ackForfeit !== "boolean") return error(422, "validation", "Invalid request body.");
      // checked before the one-time confirmation is spent: ownership (404), blockers, emptying, forfeit
      const chk = await engine<{ kind?: string; canArchive?: boolean; needsEmpty?: boolean; credit?: number; bonus?: number; blockers?: { code: string; message: string }[] }>(`/v1/accounts/${login}/archive-check`, { user, req });
      if (chk.status !== 200) return reply(chk.status, chk.data);
      const c = chk.data;
      const blocker = c.blockers?.[0];
      if (blocker || c.canArchive === false) return error(409, blocker?.code ?? "cannot_archive", blocker?.message ?? "This account can't be deleted right now.");
      if (c.needsEmpty && !body.empty) return error(409, "needs_empty", "Close the open trades and move the money out first.");
      if ((Number(c.credit) || 0) + (Number(c.bonus) || 0) > 0 && !body.ackForfeit) return error(422, "ack_forfeit", "Confirm that the bonus and credit will be lost.");
      // deleting a live account (it may move money and close trades) needs the emailed code (D20); demo doesn't
      if (c.kind !== "demo") {
        const denied = await stepup(req, user, body, "account_archive", login);
        if (denied) return denied;
      }
      const r = await engine(`/v1/accounts/${login}/archive`, { user, req, body: { empty: body.empty, ackForfeit: body.ackForfeit } });
      return reply(r.status, r.data);
    }
    case "restore": {
      const refused = ownerOnly(req, user);
      if (refused) return refused;
      const r = await engine(`/v1/accounts/${login}/restore`, { user, req, body: {} });
      return reply(r.status, r.data);
    }
    case "sso": {
      const r = await engine<{ token?: string; expiresAt?: string }>(`/v1/accounts/${login}/sso`, { user, req, body: {} });
      if (r.status !== 200 || !r.data.token) return reply(r.status === 200 ? 502 : r.status, r.data);
      return reply(200, { url: `${TERMINAL_BASE}/?sso=${encodeURIComponent(r.data.token)}`, expiresAt: r.data.expiresAt });
    }
  }
  return error(404, "not_found", "Not found.");
}

export async function PATCH(req: NextRequest, { params }: Ctx) {
  const path = (await params).path;
  if (!sameOrigin(req)) return error(403, "forbidden", "Cross-site request blocked.");
  if (!req.headers.get("content-type")?.includes("application/json")) return error(415, "bad_request", "Expected JSON.");
  const body = (await req.json().catch(() => null)) as Obj | null;
  if (body === null || typeof body !== "object" || Array.isArray(body)) return error(400, "bad_request", "Invalid request body.");
  const user = await auth(req);
  if (user instanceof NextResponse) return user;
  const login = path[1];
  if (path.length !== 2 || path[0] !== "accounts" || !login || !LOGIN_RE.test(login)) return error(404, "not_found", "Not found.");
  const refused = ownerOnly(req, user);
  if (refused) return refused;
  if (typeof body.name !== "string") return error(422, "validation", "Enter a name.");
  const name = body.name.trim();
  if ([...name].length > 32) return error(422, "validation", "Use up to 32 characters.");
  if (/[\u0000-\u001f\u007f]/.test(name)) return error(422, "validation", "The name contains characters that aren't allowed.");
  const r = await engine(`/v1/accounts/${login}`, { method: "PATCH", user, req, body: { name } });
  return reply(r.status, r.data);
}

/** Archive / restore / rename are for the account owner only: not a view-only login, not a staff session. */
function ownerOnly(req: NextRequest, user: GatewayUser): NextResponse | null {
  if (user.viewer) return error(403, "viewer_read_only", "This is a view-only login. Changes are not allowed.");
  // staff sign-in-as sessions ("i." read-only, "s." full) may not delete, restore or rename a client's accounts
  const token = req.cookies.get(SESSION_COOKIE)?.value ?? "";
  if (token.startsWith("i.") || token.startsWith("s.")) return error(403, "staff_read_only", "Staff sessions can't archive, restore or rename a client's accounts.");
  return null;
}

/** 404 unless the account exists and belongs to the client (the engine checks ownership). */
async function ownAccount(req: NextRequest, user: GatewayUser, login: string): Promise<NextResponse | null> {
  const r = await engine<{ account?: unknown }>(`/v1/accounts/${login}`, { user, req });
  if (r.status === 200 && r.data.account) return null;
  return reply(r.status === 200 ? 404 : r.status, r.data);
}

/** Live -> live between the client's own accounts (B9): both accounts checked first, then the emailed code
 *  (step-up internal_transfer, target = the source login), then the wallet's two idempotent legs. */
async function transferBetween(req: NextRequest, user: GatewayUser, body: Obj) {
  const refused = ownerOnly(req, user);
  if (refused) return refused;
  const from = String(body.fromLogin ?? "");
  const to = String(body.toLogin ?? "");
  if (!LOGIN_RE.test(from) || !LOGIN_RE.test(to)) return error(422, "validation", "Choose both accounts.");
  if (from === to) return error(422, "validation", "Choose two different accounts.");
  const amount = typeof body.amount === "string" || typeof body.amount === "number" ? String(body.amount).trim() : "";
  if (!/^\d{1,12}(\.\d{1,2})?$/.test(amount) || Number(amount) <= 0) return error(422, "validation", "Enter an amount with up to 2 decimals.");
  const key = typeof body.idempotency_key === "string" && /^[A-Za-z0-9_-]{8,64}$/.test(body.idempotency_key) ? body.idempotency_key : null;
  if (!key) return error(422, "validation", "Missing request id.");
  const list = await engine<{ accounts?: { login: number; type: string; group: string; status: string; withdrawable?: number; currency?: string }[] }>("/v1/accounts", { user, req });
  if (list.status !== 200) return reply(list.status, list.data);
  const mine = list.data.accounts ?? [];
  const src = mine.find((a) => String(a.login) === from);
  const dst = mine.find((a) => String(a.login) === to);
  if (!src || !dst) return error(404, "not_found", "Account not found.");
  if (src.type !== "live" || dst.type !== "live") return error(422, "live_only", "Transfers between accounts are for live accounts only.");
  if (isPropGroup(src.group) || isPropGroup(dst.group)) return error(422, "prop_account", "Prop challenge accounts can't send or receive transfers.");
  if (["archived", "closed"].includes(dst.status)) return error(409, "account_status", "The receiving account is archived or closed.");
  const usd = (Number(src.withdrawable) || 0) / (src.currency === "USC" ? 100 : 1);
  if (Number(amount) > usd + 1e-9) return error(422, "insufficient_funds", `Not enough free funds on #${from}: ${usd.toFixed(2)} USD available.`);
  const denied = await stepup(req, user, body, "internal_transfer", from);
  if (denied) return denied;
  const r = await wallet(`/v1/wallets/${user.id}/trading-to-trading`, { user, req, body: { from_login: Number(from), to_login: Number(to), amount, idempotency_key: `crm:${user.id}:${key}` } });
  return reply(r.status, r.data);
}

/** Redeems the client's step-up token (D20) for this change; null = go ahead. */
async function stepup(req: NextRequest, user: GatewayUser, body: Obj, action: StepupAction, login: string): Promise<NextResponse | null> {
  const denied = await consumeStepup(user.id, req.headers, action, login, stepupTokenOf(req.headers, body));
  return denied ? reply(denied.status, denied.data) : null;
}

/* ------------------------------------------------------------------ */
/* Trade history narrowed to options / CFDs                            */
/* ------------------------------------------------------------------ */

const FILTER_PAGE = 1000;
const FILTER_MAX_PAGES = 20;

type DealRow = { symbol: string; entry: string; commission: number; instrument?: string | null; option?: DealOption | null };
const num = (v: unknown) => (typeof v === "number" && Number.isFinite(v) ? v : Number(v) || 0);
const r2 = (v: number) => Math.round(v * 100) / 100;

/** The engine has no instrument filter: the newest FILTER_PAGE × FILTER_MAX_PAGES deals of the period are read and
 *  filtered here, then paged; totals follow the engine's (non-reversed deals; commission once per charge). */
async function filteredHistory(req: NextRequest, user: GatewayUser, login: string, inst: "option" | "cfd") {
  const sp = req.nextUrl.searchParams;
  const range = new URLSearchParams();
  for (const k of ["from", "to"] as const) {
    const v = sp.get(k);
    if (v) range.set(k, v); // validated by pageQuery
  }
  const page = Number(sp.get("page") ?? 1) || 1;
  const limit = Number(sp.get("limit") ?? 100) || 100;
  const matched: Obj[] = [];
  let seen = 0;
  let truncated = false;
  for (let p = 1; p <= FILTER_MAX_PAGES; p++) {
    const q = new URLSearchParams(range);
    q.set("page", String(p));
    q.set("limit", String(FILTER_PAGE));
    const r = await engine<{ deals?: Obj[]; total?: number }>(`/v1/accounts/${login}/history?${q}`, { user, req });
    if (r.status !== 200) return reply(r.status, r.data);
    const batch = r.data.deals ?? [];
    seen += batch.length;
    for (const d of batch) if (matchesInstrument(d as unknown as DealRow, inst)) matched.push(clientDeal(d));
    if (batch.length < FILTER_PAGE || seen >= (r.data.total ?? 0)) break;
    if (p === FILTER_MAX_PAGES) truncated = true;
  }
  const totals = { profit: 0, swap: 0, commission: 0 };
  for (const d of matched) {
    if (d.reversed) continue;
    totals.profit += num(d.profit);
    totals.swap += num(d.swap);
    totals.commission += dealCommission(d as unknown as DealRow);
  }
  return reply(200, {
    deals: matched.slice((page - 1) * limit, page * limit),
    orders: [],
    page,
    limit,
    total: matched.length,
    totals: { profit: r2(totals.profit), swap: r2(totals.swap), commission: r2(totals.commission) },
    truncated,
  });
}

/* ------------------------------------------------------------------ */
/* CSV statements (D48)                                                */
/* ------------------------------------------------------------------ */

const EXPORT_PAGE = 1000;
const EXPORT_MAX_PAGES = 50;

function cell(v: unknown, text = false): string {
  let s = v === null || v === undefined ? "" : String(v);
  // spreadsheet formula injection: free-text cells never start with = + - @
  if (text && /^[=+\-@\t\r]/.test(s)) s = `'${s}`;
  return /[",\n\r]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

async function exportCsv(req: NextRequest, user: GatewayUser, login: string) {
  const kind = req.nextUrl.searchParams.get("kind");
  if (kind !== "history" && kind !== "ledger") return error(400, "bad_request", "kind must be history or ledger.");
  const sp = req.nextUrl.searchParams;
  const inst = sp.get("instrument") ?? "all";
  if (inst !== "all" && inst !== "option" && inst !== "cfd") return error(400, "bad_request", "instrument must be option or cfd.");
  const range = new URLSearchParams();
  for (const k of ["from", "to"] as const) {
    const v = sp.get(k);
    if (!v) continue;
    if (!DATE_RE.test(v)) return error(400, "bad_request", `Invalid ${k} date.`);
    range.set(k, v);
  }

  const rows: Obj[] = [];
  let seen = 0;
  for (let page = 1; page <= EXPORT_MAX_PAGES; page++) {
    const q = new URLSearchParams(range);
    q.set("page", String(page));
    q.set("limit", String(EXPORT_PAGE));
    const r = await engine<{ deals?: Obj[]; items?: Obj[]; total?: number }>(`/v1/accounts/${login}/${kind}?${q}`, { user, req });
    if (r.status !== 200) return reply(r.status, r.data);
    const batch = (kind === "history" ? r.data.deals : r.data.items) ?? [];
    rows.push(...(kind === "history" && inst !== "all" ? batch.filter((d) => matchesInstrument(d as unknown as DealRow, inst)) : batch));
    seen += batch.length;
    if (batch.length < EXPORT_PAGE || seen >= (r.data.total ?? 0)) break;
  }

  let csv: string;
  if (kind === "history") {
    // Kalks FX Options deals: Volume = contracts, Price / Open price = premium per unit of the underlying (quote
    // currency); the option columns give the terms and the premiums in USD per contract
    const usdFactor = rows.some((d) => isOptionTrade(d as unknown as DealRow)) ? await accountUsdFactor(req, user, login) : 1;
    const head = ["Time (UTC)", "Deal", "Position", "Order", "Symbol", "Type", "Direction", "Volume", "Price", "Open price", "Open time (UTC)", "Commission", "Swap", "Profit", "Reason", "Comment", "Instrument", "Underlying", "Call/Put", "Strike", "Expiry", "Quote currency", "Premium per contract (USD)", "Open premium per contract (USD)"];
    csv = [
      head.join(","),
      ...rows.map((d) => {
        const row = d as unknown as DealRow & { volume: number; price: number; openPrice: number | null; profit: number };
        const isOpt = isOptionTrade(row);
        const o = isOpt ? optionTerms(row.symbol, row.option) : null;
        const prem = isOpt ? dealPremiumsUsd(row, usdFactor) : null;
        const usd = (v: number | null | undefined) => (v === null || v === undefined ? "" : v.toFixed(2));
        const opt = isOpt
          ? ["Option", o?.underlying ?? "", o ? (o.right === "call" ? "Call" : "Put") : "", o?.strikeLabel ?? "", o?.expiry ?? "", o?.quoteCurrency ?? "", usd(prem?.own), row.entry === "in" ? "" : usd(prem?.open)]
          : ["CFD", "", "", "", "", "", "", ""];
        return [d.time, d.id, d.positionTicket, d.orderTicket, d.symbol, d.side, d.entry, d.volume, d.price, d.openPrice, d.openTime, d.commission, d.swap, d.profit, d.reason, cell(d.comment, true), ...opt]
          .map((v, i) => (i === 15 ? v : cell(v)))
          .join(",");
      }),
    ].join("\r\n");
  } else {
    const head = ["Time (UTC)", "Transaction", "Type", "Sub-ledger", "Amount", "Currency", "Reference", "Note"];
    csv = [head.join(","), ...rows.map((e) => [cell(e.at), cell(e.txn), cell(e.kind), cell(e.subLedger), cell(e.amount), cell(e.currency), cell(e.reference, true), cell(e.note, true)].join(","))].join("\r\n");
  }

  const span = [range.get("from"), range.get("to")].filter(Boolean).map((s) => s!.slice(0, 10)).join("_");
  const name = `kalks-${login}-${kind === "history" ? (inst === "option" ? "options" : inst === "cfd" ? "cfd-trades" : "trades") : "ledger"}${span ? `-${span}` : ""}.csv`;
  return new NextResponse("﻿" + csv + "\r\n", {
    status: 200,
    headers: { ...NO_STORE, "content-type": "text/csv; charset=utf-8", "content-disposition": `attachment; filename="${name}"`, "x-content-type-options": "nosniff" },
  });
}

/** Account-currency units per USD of an account (100 on cent accounts): option premiums in the CSV are USD. */
async function accountUsdFactor(req: NextRequest, user: GatewayUser, login: string) {
  const r = await engine<{ account?: { cent?: boolean; currency?: string } }>(`/v1/accounts/${login}`, { user, req });
  return r.status === 200 ? usdFactorOf(r.data.account) : 1;
}
