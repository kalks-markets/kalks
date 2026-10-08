// CFD / Options account split (Kalks 2): an account trades ONE product, its group's (`product` on the engine's account
// view: "cfd" | "options"; absent on older engines = CFD). In Kalks Trader the active account decides the workspace —
// a CFD account opens the CFD workspace, an Options account the options workspace — and the CFD | Options control in
// the title bar switches to the client's account of that product. Pure helpers (no React, no browser APIs), tested
// in tests/product.test.mjs.

export type Product = "cfd" | "options";

/** The product of an account view (engine JSON or the store's account extra): anything but "options" is CFD. */
export const productOf = (a: { product?: string | null } | null | undefined): Product => (a?.product === "options" ? "options" : "cfd");

/** `?mode=` of a link (`cfd` | `options`, any case), else null. */
export function parseMode(v: string | null | undefined): Product | null {
  const m = v?.trim().toLowerCase();
  return m === "options" || m === "cfd" ? m : null;
}

/**
 * The workspace on screen: an engine account's product decides; without one (guest chart mode, demo builds with mock
 * accounts) the CFD | Options switch held in memory does.
 */
export const workspaceOf = (accountProduct: Product | null, manual: Product): Product => accountProduct ?? manual;

type SessionLike = { login: string; account?: { product?: string | null } | null };

/**
 * The login Kalks Trader opens on start: the SSO account (the Client Area's Trade button), else — when the link asks
 * for a product (`?mode=options` from the Client Area's Options page or the public option chain) — the last shown
 * account of that product, else any account of that product, else the last shown account, else the newest.
 */
export function pickSession<S extends SessionLike>(sessions: readonly S[], o: { prefer?: string | null; active?: string | null; want?: Product | null }): S | undefined {
  const by = (l: string | null | undefined) => (l ? sessions.find((x) => x.login === l) : undefined);
  const preferred = by(o.prefer);
  if (preferred) return preferred;
  if (o.want) {
    const of = (x: S | undefined) => (x && x.account && productOf(x.account) === o.want ? x : undefined);
    const hit = of(by(o.active)) ?? sessions.find((x) => !!of(x));
    if (hit) return hit;
  }
  return by(o.active) ?? sessions[0];
}

type AccountLike = { login: string; product?: string | null };

/**
 * The account the CFD | Options control opens for `product`: the current one when it already trades it, else the
 * last one shown of that product (`recent`, newest first), else the first listed; null when the client has none here
 * (then the control offers "Open a CFD / Options account" in the Client Area).
 */
export function switchTarget(accounts: readonly AccountLike[], current: string | null, product: Product, recent: readonly string[] = []): string | null {
  const own = accounts.filter((a) => productOf(a) === product);
  if (current && own.some((a) => a.login === current)) return current;
  for (const l of recent) if (own.some((a) => a.login === l)) return l;
  return own[0]?.login ?? null;
}

/** Accounts split by product for the account switcher (each list in its original order). */
export function groupAccounts<A extends AccountLike>(accounts: readonly A[]): { product: Product; accounts: A[] }[] {
  return (["cfd", "options"] as const).map((p) => ({ product: p, accounts: accounts.filter((a) => productOf(a) === p) })).filter((g) => g.accounts.length > 0);
}

/** The Client Area's open-account wizard on a product's path. */
export const openAccountPath = (product: Product) => `/accounts/new?product=${product}`;
