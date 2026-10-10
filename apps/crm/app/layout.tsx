import { preconnect } from "react-dom";
import type { Metadata, Viewport } from "next";
// subpath imports: a server layout importing the "@kalks/ui" barrel ships every client module of it on every page
import { BrandProvider } from "@kalks/ui/brand";
import { brandCss, isCustomBrand } from "@kalks/ui/brand-vars";
import { Providers } from "@kalks/ui/providers";
import { fontVariables } from "@kalks/ui/fonts";
import { getI18n } from "@kalks/i18n/server";
import { tenantBrand } from "@/lib/tenant-config";
import "./globals.css";

// The broker brand of the visitor's host (gateway tenant_domains); Kalks keeps its stock look.
export async function generateMetadata(): Promise<Metadata> {
  const b = await tenantBrand();
  const name = isCustomBrand(b) ? b.name : "Kalks";
  return {
    title: { default: `${name} — Client Area`, template: `%s · ${name}` },
    description: `Trade Forex, Metals, Indices, Crypto and Stocks with ${name}.`,
    icons: isCustomBrand(b) ? (b.logo_url ? { icon: b.logo_url } : undefined) : CLIENT_AREA_ICONS,
  };
}

// the real Kalks K (the original mark, white on black) for the browser tab and the phone home screen
const CLIENT_AREA_ICONS = {
  icon: [
    { url: "/brand/kalks-icon.svg", type: "image/svg+xml" },
    { url: "/brand/favicon-32.png", sizes: "32x32", type: "image/png" },
    { url: "/brand/favicon-16.png", sizes: "16x16", type: "image/png" },
  ],
  apple: [{ url: "/brand/apple-touch-icon.png", sizes: "180x180", type: "image/png" }],
};

// the Client Area is dark only (founder 2026-10-10: Vantablack + orange); the browser chrome takes the canvas colour
export const viewport: Viewport = {
  themeColor: "#000000",
  width: "device-width",
  initialScale: 1,
};

/** Browser-side market-data origin (another host in production), when it is one. */
const MARKET_DATA_ORIGIN = (() => {
  try {
    return new URL(process.env.NEXT_PUBLIC_MARKET_DATA_URL ?? "").origin;
  } catch {
    return null;
  }
})();

export default async function RootLayout({ children }: { children: React.ReactNode }) {
  // quotes, candles and the stream come from the market-data origin: start its DNS + TCP + TLS handshake while the
  // page loads instead of after hydration (the browser fetches it without credentials, hence "anonymous")
  if (MARKET_DATA_ORIGIN) preconnect(MARKET_DATA_ORIGIN, { crossOrigin: "anonymous" });
  // language from the kalks_locale cookie (set by the switcher) or the browser's Accept-Language
  const [{ locale, dir, messages }, brand] = await Promise.all([getI18n(), tenantBrand()]);
  const css = brandCss(brand);
  return (
    <html lang={locale} dir={dir} suppressHydrationWarning className={fontVariables}>
      <body>
        {css && <style dangerouslySetInnerHTML={{ __html: css }} />}
        <BrandProvider brand={brand}>
          <Providers forcedTheme="dark" i18n={{ locale, messages }}>{children}</Providers>
        </BrandProvider>
      </body>
    </html>
  );
}
