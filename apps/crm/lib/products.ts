// CFD / Options account split (Kalks 2): an account trades ONE product, its group's (`product` on the engine's account
// and group views: "cfd" | "options"; absent on older engines = CFD). A CFD account opens CFDs only, an Options
// account Kalks FX Options only; a client opens either or both. Pure helpers shared by the open-account wizard, the
// account lists and the Options page (tested in tests/products.test.mjs).

export type Product = "cfd" | "options";
export const PRODUCTS: readonly Product[] = ["cfd", "options"];

type HasProduct = { product?: string | null };

/** The product of an account or a group: anything but "options" is a CFD one. */
export const productOf = (x: HasProduct | null | undefined): Product => (x?.product === "options" ? "options" : "cfd");

/** `?product=` of a link (the Options page's "Open an Options account"), else null. */
export const parseProduct = (v: string | null | undefined): Product | null => (v === "options" || v === "cfd" ? v : null);

/** Options are offered unless the broker switched the module off (`modules.options === false`; missing = on). */
export const optionsModuleOn = (modules: Record<string, boolean | undefined> | null | undefined): boolean => modules?.options !== false;

/** Groups the platform manages (prop challenges, copy / PAMM / MAM accounts, the options market maker): never opened
 *  by the client in the wizard. */
export const systemGroup = (code: string): boolean => {
  const g = code.toLowerCase();
  return g.startsWith("prop") || g === "copy" || g.startsWith("copy-") || g === "pamm" || g.startsWith("pamm-") || g === "mam" || g.startsWith("mam-") || g === "options-mm";
};

type WizardGroup = HasProduct & { code: string; enabled: boolean; accountTypes: string };

/** The groups the open-account wizard offers for a live / demo account of `product`. */
export function offeredGroups<G extends WizardGroup>(groups: readonly G[], kind: "live" | "demo", product: Product): G[] {
  return groups.filter((g) => g.enabled && !systemGroup(g.code) && (g.accountTypes === "both" || g.accountTypes === kind) && productOf(g) === product);
}

type CountedAccount = HasProduct & { type: string; group: string; status: string };

/** The client's accounts of `kind` that count against the account limit of a `product` group: the engine counts per
 *  (live / demo, product), never archived or closed accounts nor platform-managed ones. */
export function usedFor(accounts: readonly CountedAccount[], kind: "live" | "demo", product: Product): number {
  return accounts.filter((a) => a.type === kind && productOf(a) === product && a.status !== "archived" && a.status !== "closed" && !systemGroup(a.group)).length;
}

/** Accounts split by product, each list in its original order. */
export function byProduct<A extends HasProduct>(accounts: readonly A[]): Record<Product, A[]> {
  return { cfd: accounts.filter((a) => productOf(a) === "cfd"), options: accounts.filter((a) => productOf(a) === "options") };
}

/** CFD accounts first, then Options accounts (stable within each). */
export const productOrder = <A extends HasProduct>(accounts: readonly A[]): A[] => [...byProduct(accounts).cfd, ...byProduct(accounts).options];
