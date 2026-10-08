import { preconnect } from "react-dom";
import type { Metadata, Viewport } from "next";
// subpath imports: a server layout importing the "@kalks/ui" barrel ships every client module of it on every page
import { BrandProvider } from "@kalks/ui/brand";
import { KALKS_ICONS, brandCss, isCustomBrand } from "@kalks/ui/brand-vars";
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
    icons: isCustomBrand(b) ? (b.logo_url ? { icon: b.logo_url } : undefined) : KALKS_ICONS,
  };
}

// the theme follows the device by default (Auto · Light · Dark, Kalks 2); the browser chrome takes the canvas colour
export const viewport: Viewport = {
  themeColor: [
    { media: "(prefers-color-scheme: dark)", color: "#0b0809" },
    { media: "(prefers-color-scheme: light)", color: "#f6f1ee" },
  ],
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
          <Providers i18n={{ locale, messages }}>{children}</Providers>
        </BrandProvider>
      </body>
    </html>
  );
}
