// Server-only: Kalks Trader's editable content (surface "trader") from the content service, for the root layout's
// <ContentProvider>. The broker comes from the visitor's host (tenant-brand), the language from the request. A Back
// Office preview link sets the preview cookie (app/api/content-preview) and then drafts are shown. Unreachable
// service = the built-in defaults. Slot keys: docs/CONTENT.md.

import { cookies } from "next/headers";
import type { ContentBundle } from "@kalks/ui/content/types";
import { CONTENT_PREVIEW_COOKIE, fetchContent, isPreviewToken } from "@kalks/ui/content/server";
import { tenantBrand } from "@/lib/tenant-brand";

export async function traderContent(locale: string): Promise<ContentBundle> {
  const [brand, jar] = await Promise.all([tenantBrand(), cookies()]);
  const preview = jar.get(CONTENT_PREVIEW_COOKIE)?.value;
  return fetchContent({ surface: "trader", tenant: brand?.slug, locale, preview: isPreviewToken(preview) ? preview : null });
}
