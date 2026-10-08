import { preconnect } from "react-dom";
import type { Metadata, Viewport } from "next";
import { getI18n } from "@kalks/i18n/server";
// subpath imports: a server layout importing the "@kalks/ui" barrel ships every client module of it on every page
import { BrandProvider } from "@kalks/ui/brand";
import { KALKS_ICONS, brandCss, isCustomBrand } from "@kalks/ui/brand-vars";
import { fontVariables } from "@kalks/ui/fonts";
import { tenantBrand, tenantModules } from "@/lib/tenant-brand";
import { TerminalProviders } from "./providers";
import "./globals.css";

// The broker brand of the host (gateway tenant_domains); Kalks keeps its stock look.
export async function generateMetadata(): Promise<Metadata> {
  const b = await tenantBrand();
  const name = isCustomBrand(b) ? b.name : "Kalks";
  return {
    title: { default: `${name} Trader`, template: `%s · ${name} Trader` },
    description: `${name} professional trading room`,
    icons: isCustomBrand(b) ? (b.logo_url ? { icon: b.logo_url } : undefined) : KALKS_ICONS,
  };
}

export const viewport: Viewport = {
  themeColor: [
    { media: "(prefers-color-scheme: dark)", color: "#070505" },
    { media: "(prefers-color-scheme: light)", color: "#eee7e3" },
  ],
  width: "device-width",
  initialScale: 1,
  maximumScale: 1,
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
  // same language cookie as the Client Area (kalks_locale), else the browser's Accept-Language
  const [{ locale, dir, messages }, brand, modules] = await Promise.all([getI18n(), tenantBrand(), tenantModules()]);
  const css = brandCss(brand);
  return (
    <html lang={locale} dir={dir} suppressHydrationWarning className={fontVariables}>
      <body className="overflow-hidden">
        {css && <style dangerouslySetInnerHTML={{ __html: css }} />}
        <BrandProvider brand={brand}>
          <TerminalProviders locale={locale} messages={messages} modules={modules}>
            {children}
          </TerminalProviders>
        </BrandProvider>
      </body>
    </html>
  );
}
