/**
 * Known content slots: what each surface reads from services/content, so the Pages editor lists them (with the
 * built-in default the app shows today) before anything has been saved. The apps read exactly these keys; a slot
 * saved in the service that isn't listed here still shows in the editor (as "custom").
 *
 * `default`: the built-in English text, or for images the built-in image path on that surface's site
 * (e.g. "/heroes/dashboard.jpg"); empty = nothing shown / the built-in illustration.
 * English defaults come from packages/i18n/src/catalog/en (auth.brand.*, trader.login.*; trader side.text = brandTextLive).
 */

import { WEBSITE_PAGE_LABELS, WEBSITE_PAGE_PATHS, WEBSITE_SLOTS } from "./content-catalogue-website";

export type CatalogueSurface = "website" | "app" | "trader" | "email";
export type CatalogueKind = "text" | "rich_text" | "image" | "link" | "placeholder";

export type CatalogueSlot = {
  surface: CatalogueSurface;
  page: string;
  slot: string;
  kind: CatalogueKind;
  label: string;
  help?: string;
  /** Default text, or for images the default image path relative to the surface's site, e.g. "/heroes/dashboard.jpg". */
  default?: string;
};

/** Preview path of each page, keyed `${surface}/${page}`; pages not listed preview at "/". */
export const PAGE_PATHS: Record<string, string> = {
  // website (generated from the website repo, lib/content-catalogue-website.ts)
  ...Object.fromEntries(Object.entries(WEBSITE_PAGE_PATHS).map(([p, path]) => [`website/${p}`, path])),
  // Client Area
  "app/dashboard": "/",
  "app/accounts": "/accounts",
  "app/wallet": "/wallet",
  "app/portfolio": "/portfolio",
  "app/copy": "/social",
  "app/prop": "/prop",
  "app/markets": "/markets",
  "app/options": "/options",
  "app/partner": "/partner",
  "app/rewards": "/rewards",
  "app/academy": "/academy",
  "app/profile": "/profile",
  "app/support": "/support",
  "app/news": "/news",
  "app/auth": "/login",
  "app/empty": "/accounts",
  // Kalks Trader
  "trader/login": "/login",
};

/** Human page names for the tree (falls back to the key). */
export const PAGE_LABELS: Record<string, string> = {
  ...Object.fromEntries(Object.entries(WEBSITE_PAGE_LABELS).map(([p, label]) => [`website/${p}`, label])),
  "app/dashboard": "Dashboard",
  "app/accounts": "Accounts",
  "app/wallet": "Wallet",
  "app/portfolio": "Portfolio",
  "app/copy": "Copy trading",
  "app/prop": "Prop challenges",
  "app/markets": "Markets",
  "app/options": "Options",
  "app/partner": "Partner",
  "app/rewards": "Rewards",
  "app/academy": "Academy",
  "app/profile": "Profile",
  "app/support": "Support",
  "app/news": "News",
  "app/auth": "Sign in / sign up",
  "app/empty": "Empty states",
  "trader/login": "Sign in",
};

const HERO_PAGES: [page: string, name: string][] = [
  ["accounts", "Accounts"],
  ["wallet", "Wallet"],
  ["portfolio", "Portfolio"],
  ["copy", "Copy trading"],
  ["prop", "Prop challenges"],
  ["markets", "Markets"],
  ["options", "Options"],
  ["partner", "Partner"],
  ["rewards", "Rewards"],
  ["academy", "Academy"],
  ["profile", "Profile"],
  ["support", "Support"],
  ["news", "News"],
];

const EMPTY_STATES: [slot: string, where: string][] = [
  ["accounts.image", "no trading accounts yet"],
  ["positions.image", "no open positions"],
  ["history.image", "no trade history"],
  ["referrals.image", "no referrals yet"],
  ["notifications.image", "no notifications"],
];

// ---- website -------------------------------------------------------------------------------------------------
// Generated from the website's own slot list (kalks-website apps/trader/src/content/manifest.ts) by
// scripts/gen-website-catalogue.mjs: re-run it after the website adds or renames a slot.
const WEBSITE: CatalogueSlot[] = WEBSITE_SLOTS;

// ---- Client Area (surface "app") -----------------------------------------------------------------------------
const APP: CatalogueSlot[] = [
  { surface: "app", page: "dashboard", slot: "hero.image", kind: "image", label: "Dashboard hero photo", default: "/heroes/dashboard.jpg" },
  { surface: "app", page: "dashboard", slot: "hero.title", kind: "text", label: "Hero headline", help: "Optional headline over the hero photo (empty = none, as today)", default: "" },
  { surface: "app", page: "dashboard", slot: "hero.lead", kind: "text", label: "Hero lead", help: "Optional line under the hero headline", default: "" },
  { surface: "app", page: "dashboard", slot: "banner.title", kind: "text", label: "Banner headline", help: "Dashboard banner headline; the banner shows only when a title or image is published", default: "" },
  { surface: "app", page: "dashboard", slot: "banner.text", kind: "text", label: "Banner text", default: "" },
  { surface: "app", page: "dashboard", slot: "banner.image", kind: "image", label: "Banner image", default: "" },
  { surface: "app", page: "dashboard", slot: "banner.cta", kind: "link", label: "Banner button", help: "Label and link of the banner's button", default: "" },
  ...HERO_PAGES.map(([page, name]): CatalogueSlot => ({ surface: "app", page, slot: "hero.image", kind: "image", label: `Hero photo of ${name}`, default: `/heroes/${page}.jpg` })),
  { surface: "app", page: "auth", slot: "side.image", kind: "image", label: "Sign-in / sign-up side photo", default: "/heroes/dashboard.jpg" },
  { surface: "app", page: "auth", slot: "side.headline", kind: "text", label: "Side headline", default: "Trade global markets with institutional precision." },
  {
    surface: "app",
    page: "auth",
    slot: "side.body",
    kind: "text",
    label: "Side text",
    default: "Forex, metals, indices, energies, crypto and stocks — instant USDT funding, one account for trading, copying and partnering.",
  },
  ...EMPTY_STATES.map(([slot, where]): CatalogueSlot => ({ surface: "app", page: "empty", slot, kind: "image", label: `Empty-state illustration: ${where}`, help: "Empty = the built-in illustration", default: "" })),
];

// ---- Kalks Trader (surface "trader") -------------------------------------------------------------------------
const TRADER: CatalogueSlot[] = [
  { surface: "trader", page: "login", slot: "side.image", kind: "image", label: "Sign-in side photo", default: "/assets/photos/trading-screen.jpg" },
  { surface: "trader", page: "login", slot: "side.eyebrow", kind: "text", label: "Side eyebrow", default: "Professional trading room" },
  { surface: "trader", page: "login", slot: "side.title", kind: "text", label: "Side headline", default: "Every market. One workstation." },
  {
    surface: "trader",
    page: "login",
    slot: "side.text",
    kind: "text",
    label: "Side text",
    help: "Built-in: the live build's text (trader.login.brandTextLive); demo builds show trader.login.brandTextDemo instead when nothing is published",
    default: "Multi-chart layouts, one-click execution, server-side SL/TP and trailing stops, depth of market and a full toolbox. Hedging, netting, cent and demo accounts.",
  },
];

export const CATALOGUE: CatalogueSlot[] = [...WEBSITE, ...APP, ...TRADER];

export const pagePath = (surface: string, page: string) => PAGE_PATHS[`${surface}/${page}`] ?? "/";
export const pageLabel = (surface: string, page: string) => PAGE_LABELS[`${surface}/${page}`] ?? page;
export const catalogueSlot = (surface: string, page: string, slot: string) => CATALOGUE.find((c) => c.surface === surface && c.page === page && c.slot === slot);
