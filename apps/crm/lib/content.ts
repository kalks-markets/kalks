// Server-only: the Client Area's editable content (surface "app") from the content service, for the root layout's
// <ContentProvider>. The broker comes from the visitor's host (tenant-config), the language from the request
// (cookie / Accept-Language). A Back Office preview link sets the preview cookie (app/api/content-preview) and then
// drafts are shown. Unreachable service = every slot shows the built-in default. Slot keys: docs/CONTENT.md.

import { cookies } from "next/headers";
import type { ContentBundle } from "@kalks/ui/content/types";
import { CONTENT_PREVIEW_COOKIE, fetchContent, isPreviewToken } from "@kalks/ui/content/server";
import { tenantConfig } from "@/lib/tenant-config";

export async function appContent(locale: string): Promise<ContentBundle> {
  const [cfg, jar] = await Promise.all([tenantConfig(), cookies()]);
  const preview = jar.get(CONTENT_PREVIEW_COOKIE)?.value;
  return fetchContent({ surface: "app", tenant: cfg?.tenant.slug, locale, preview: isPreviewToken(preview) ? preview : null });
}
