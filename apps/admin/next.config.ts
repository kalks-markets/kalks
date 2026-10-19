import type { NextConfig } from "next";

const config: NextConfig = {
  transpilePackages: ["@kalks/ui", "@kalks/mock", "@kalks/i18n"],
  devIndicators: false,
  // development only: broker domains resolved from the Host (tenant_domains) can be tried locally on
  // *.test / *.localhost hosts (plus KALKS_DEV_ORIGINS, comma-separated) without Next blocking its dev assets
  allowedDevOrigins: ["*.localhost", "*.*.localhost", "*.test", "*.*.test", ...(process.env.KALKS_DEV_ORIGINS?.split(",").map((s) => s.trim()).filter(Boolean) ?? [])],
  agentRules: false,
  images: { unoptimized: true },
  // Client router cache: pages here are client components whose server payload carries no user data, so a page
  // visited in the last minute opens without a server round trip (data still refetches on mount).
  // proxyClientMaxBodySize: lets a 10 MB media library upload (/api/content/media) through the proxy untruncated
  experimental: { staleTimes: { dynamic: 60 }, proxyClientMaxBodySize: "11mb" },
  // Static images (public/assets: logos, coins, flags, photos) have no content hash: a day fresh, then served from
  // cache while revalidating for a week, instead of a revalidation round trip on every page load (max-age=0).
  async headers() {
    return [{ source: "/assets/:file*", headers: [{ key: "Cache-Control", value: "public, max-age=86400, stale-while-revalidate=604800" }] }];
  },
};

export default config;
