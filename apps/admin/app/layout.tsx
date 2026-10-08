import { preconnect } from "react-dom";
import type { Metadata } from "next";
// subpath imports: a server layout importing the "@kalks/ui" barrel ships every client module of it on every page
import { BrandProvider } from "@kalks/ui/brand";
import { KALKS_ICONS, brandCss, isCustomBrand } from "@kalks/ui/brand-vars";
import { fontVariables } from "@kalks/ui/fonts";
import { Providers } from "@kalks/ui/providers";
import { NotificationRecorder } from "@/components/notifications";
import { tenantBrand } from "@/lib/tenant-brand";
import "./globals.css";

// The broker brand of the host (gateway tenant_domains); Kalks keeps its stock look.
export async function generateMetadata(): Promise<Metadata> {
  const b = await tenantBrand();
  const name = isCustomBrand(b) ? b.name : "Kalks";
  return {
    title: { default: `${name} — Back Office`, template: `%s · ${name} Back Office` },
    icons: isCustomBrand(b) ? (b.logo_url ? { icon: b.logo_url } : undefined) : KALKS_ICONS,
    robots: { index: false, follow: false },
  };
}

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
  const brand = await tenantBrand();
  const css = brandCss(brand);
  return (
    <html lang="en" dir="ltr" suppressHydrationWarning className={fontVariables}>
      <body>
        {css && <style dangerouslySetInnerHTML={{ __html: css }} />}
        <BrandProvider brand={brand}>
          <Providers>
            {children}
            <NotificationRecorder />
          </Providers>
        </BrandProvider>
      </body>
    </html>
  );
}
