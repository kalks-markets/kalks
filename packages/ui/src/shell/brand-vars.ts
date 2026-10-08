// Tenant (broker) branding shared by the apps. Server-safe: no React here, so layouts can call it.
// The gateway serves the brand of the request's host (GET /v1/public/tenant-config → `branding`);
// Kalks itself (`default: true`) keeps the built-in look.

export type TenantBrand = {
  slug: string;
  name: string;
  /** True for the platform's own tenant (Kalks): the stock logo and colours are used. */
  default?: boolean;
  logo_url?: string | null;
  primary?: string | null;
  accent?: string | null;
  support_email?: string | null;
  /** First active domain per app, e.g. { app: "https://app.broker.com", trade: "https://trade.broker.com" }. */
  urls?: Partial<Record<"website" | "app" | "trade" | "admin", string | null>>;
};

const HEX = /^#[0-9a-f]{6}$/i;

/** Only hex colours ever reach CSS. */
export function brandColor(v: string | null | undefined): string | null {
  return v && HEX.test(v) ? v.toLowerCase() : null;
}

/** A branded tenant (anything but Kalks' own look). */
export function isCustomBrand(b: TenantBrand | null | undefined): b is TenantBrand {
  return !!b && !b.default;
}

/** Relative luminance of a #rrggbb colour (WCAG). */
function luminance(hex: string): number {
  const c = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * c[0]! + 0.7152 * c[1]! + 0.0722 * c[2]!;
}

/** Label colour on a face: white or the Kalks warm black, whichever contrasts more. */
function labelOn(hex: string): string {
  const l = luminance(hex);
  return 1.05 / (l + 0.05) >= (l + 0.05) / (0.0027 + 0.05) ? "#ffffff" : "#0b0809";
}

/** The accent-family constants of tokens.ts for one broker colour (`red` = primary, `yellow` = highlight). */
function family(name: "red" | "yellow", c: string): string[] {
  return [
    `--k-${name}:${c}`,
    `--k-on-${name}:${labelOn(c)}`,
    // NeoPOP edge: the colour mixed 55 % with black (KALKS2 §9)
    `--k-${name}-edge-light:color-mix(in oklab,${c} 45%,#000)`,
    `--k-${name}-edge-dark:color-mix(in oklab,${c} 45%,#000)`,
    // text tone: darker on light surfaces, lighter on dark ones
    `--k-${name}-tx-light:color-mix(in oklab,${c} 82%,#000)`,
    `--k-${name}-tx-dark:color-mix(in oklab,${c} 62%,#fff)`,
    `--k-${name}-soft-light:color-mix(in oklab,${c} 14%,#fff)`,
    `--k-${name}-soft-dark:color-mix(in oklab,${c} 18%,transparent)`,
    ...(name === "red" ? [`--k-red-pastel-light:color-mix(in oklab,${c} 26%,#fff)`, `--k-red-pastel-dark:color-mix(in oklab,${c} 30%,#0b0809)`] : []),
  ];
}

/**
 * CSS replacing the Kalks accents with the broker's colours: primary → the red family (primary buttons, links,
 * active states; the legacy `ember` follows it), accent → the yellow family (highlight buttons, active markers).
 * A broker without an accent gets its primary there too: Kalks yellow never appears for a white-label broker.
 * Empty for Kalks. Rendered as a <style> tag by the root layouts (values are validated hex colours). The theme roles
 * in tokens.css point at these constants, so one :root rule covers light, dark and Auto.
 */
export function brandCss(b: TenantBrand | null | undefined): string {
  if (!isCustomBrand(b)) return "";
  const p = brandColor(b.primary);
  const a = brandColor(b.accent) ?? p;
  const vars: string[] = [];
  if (p) vars.push(...family("red", p), `--k-brand-primary:${p}`);
  if (a) vars.push(...family("yellow", a), `--k-brand-accent:${a}`);
  if (!vars.length) return "";
  return `html:root{${vars.join(";")}}`;
}

/**
 * Kalks favicons (32 px: the K with its edge on a black rounded square; 16 px: the flat red K on black) and the Apple
 * touch icon (the app icon). Files in assets/brand, synced into every app's public/assets. Brokers never get these.
 */
export const KALKS_ICONS = {
  icon: [
    { url: "/assets/brand/favicon-32.png", sizes: "32x32", type: "image/png" },
    { url: "/assets/brand/favicon-16.png", sizes: "16x16", type: "image/png" },
  ],
  apple: [{ url: "/assets/brand/apple-touch-icon.png", sizes: "180x180", type: "image/png" }],
};

/** Host part of a brand URL (`https://app.broker.com` → `app.broker.com`). */
export function brandHost(url: string | null | undefined): string | null {
  if (!url) return null;
  try {
    return new URL(url).host;
  } catch {
    return null;
  }
}
