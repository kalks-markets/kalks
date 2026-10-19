"use client";

// Content slots in the apps: <ContentProvider> holds the surface's published content (fetched on the server by
// content/server.ts and passed down from the root layout); `useContentText` / <ContentText> / `useContentImage` read
// one slot and fall back to the app's built-in default (the current hard-coded or i18n string) when the slot is empty,
// so nothing changes until staff publish something. <ThemedImage> picks the light or dark file from data-theme.
// Slot catalogue: docs/CONTENT.md.

import * as React from "react";
import type { ContentBundle, ContentImage, ContentLink, ContentSlot } from "./types";

export type { ContentBundle, ContentImage, ContentImageFile, ContentLink, ContentPages, ContentSlot, ContentSurface } from "./types";
export { safeHref, sanitizeContent } from "./types";

const Ctx = React.createContext<ContentBundle | null>(null);

/** In preview (drafts) a small badge shows it, with a way out (`exitHref`: the app's /api/content-preview?exit=1). */
export function ContentProvider({ value, children, exitHref = "/api/content-preview?exit=1" }: { value: ContentBundle | null; children: React.ReactNode; exitHref?: string }) {
  return (
    <Ctx.Provider value={value}>
      {children}
      {value?.preview && (
        <div className="fixed bottom-4 start-4 z-[200] flex items-center gap-2 rounded-full border border-line bg-surface-2/95 py-1.5 ps-3 pe-1.5 text-[12px] text-fg shadow-[var(--k-shadow-pop)] backdrop-blur-xl" role="status">
          <span className="size-2 rounded-full bg-warn" aria-hidden />
          Preview: unpublished drafts are shown
          <a href={exitHref} className="rounded-full bg-surface-3 px-2.5 py-1 font-medium hover:bg-surface">
            Exit
          </a>
        </div>
      )}
    </Ctx.Provider>
  );
}

/** The raw slot value, or undefined. */
export function useContentSlot(page: string, slot: string): ContentSlot | undefined {
  return React.useContext(Ctx)?.pages[page]?.[slot];
}

/** True while the page shows drafts (a preview link from the Back Office). */
export function useContentPreview(): boolean {
  return React.useContext(Ctx)?.preview ?? false;
}

/** A text / placeholder slot, else `fallback`. An explicitly published empty text stays empty. */
export function useContentText(page: string, slot: string, fallback: string): string;
export function useContentText(page: string, slot: string, fallback?: string): string | undefined;
export function useContentText(page: string, slot: string, fallback?: string): string | undefined {
  const s = useContentSlot(page, slot);
  return s && (s.kind === "text" || s.kind === "placeholder") ? s.text : fallback;
}

/** A rich-text slot's markdown, else `fallback`. */
export function useContentMarkdown(page: string, slot: string, fallback?: string): string | undefined {
  const s = useContentSlot(page, slot);
  return s?.kind === "rich_text" ? s.markdown : fallback;
}

export function useContentLink(page: string, slot: string, fallback?: ContentLink): ContentLink | undefined {
  const s = useContentSlot(page, slot);
  return s?.kind === "link" ? s.link : fallback;
}

/** An image slot (resolved URLs, srcset, dark file, alt in the reader's language), else null. */
export function useContentImage(page: string, slot: string): ContentImage | null {
  const s = useContentSlot(page, slot);
  return s?.kind === "image" ? s.image : null;
}

/** Inline text from a slot: `<ContentText page="auth" slot="side.headline">{t("auth.brand.headline")}</ContentText>`. */
export function ContentText({ page, slot, children }: { page: string; slot: string; children?: string }) {
  return <>{useContentText(page, slot, children ?? "")}</>;
}

type ImgProps = Omit<React.ImgHTMLAttributes<HTMLImageElement>, "src" | "srcSet"> & {
  /** the light (default) image */
  src: string;
  srcSet?: string;
  /** the dark-theme image; without one the light image shows in both themes */
  darkSrc?: string | null;
  darkSrcSet?: string;
  /** eager loading (heroes above the fold); otherwise both images are lazy, so only the visible one loads */
  priority?: boolean;
};

/**
 * An image with a light and an optional dark file. Both are rendered and the theme decides which one shows
 * (CSS on html[data-theme], styles.css `[data-k-theme-img]`), so the first paint is right in either theme without
 * waiting for hydration and without a layout shift. Falls back to the light file when there is no dark one.
 */
export function ThemedImage({ src, srcSet, darkSrc, darkSrcSet, priority, loading, alt = "", ...rest }: ImgProps) {
  const load = loading ?? (priority ? "eager" : "lazy");
  if (!darkSrc || darkSrc === src) {
    // eslint-disable-next-line @next/next/no-img-element
    return <img src={src} srcSet={srcSet || undefined} alt={alt} loading={load} {...rest} />;
  }
  return (
    <>
      {/* eslint-disable-next-line @next/next/no-img-element */}
      <img src={src} srcSet={srcSet || undefined} alt={alt} loading={load} data-k-theme-img="light" {...rest} />
      {/* eslint-disable-next-line @next/next/no-img-element */}
      <img src={darkSrc} srcSet={darkSrcSet || undefined} alt={alt} loading={load} data-k-theme-img="dark" {...rest} />
    </>
  );
}

/** <ThemedImage> from an image slot, else the built-in `fallback` image (light and optional dark path). */
export function ContentImageView({ page, slot, fallback, fallbackDark, alt, ...rest }: Omit<ImgProps, "src" | "darkSrc" | "srcSet" | "darkSrcSet"> & { page: string; slot: string; fallback: string; fallbackDark?: string }) {
  const img = useContentImage(page, slot);
  if (!img) return <ThemedImage src={fallback} darkSrc={fallbackDark} alt={alt} {...rest} />;
  return <ThemedImage src={img.url} srcSet={img.srcset} darkSrc={img.dark?.url} darkSrcSet={img.dark?.srcset} alt={img.alt || alt} {...rest} />;
}
