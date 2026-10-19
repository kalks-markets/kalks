"use client";

import Link from "next/link";
import { ArrowUpRight } from "lucide-react";
import { ThemedImage, useContentImage, useContentLink, useContentText } from "@kalks/ui/content";
import { Button } from "@/components/kit";

/**
 * The dashboard's editable banner (content slots app/dashboard/banner.*, Back Office → Content → Pages): a card
 * like the growth banners, shown only once staff publish a title or an image. Every client sees the same one;
 * targeted campaigns stay in Marketing → Banners (BannerSlot).
 */
export function ContentBanner() {
  const title = useContentText("dashboard", "banner.title", "");
  const text = useContentText("dashboard", "banner.text", "");
  const img = useContentImage("dashboard", "banner.image");
  const cta = useContentLink("dashboard", "banner.cta");
  if (!title && !img) return null;
  const external = cta && /^https?:\/\//i.test(cta.href);
  const button = cta && (
    <Button size="sm" variant="ember">
      {cta.label} <ArrowUpRight />
    </Button>
  );
  return (
    <div className="k-card relative mb-4 flex overflow-hidden" data-testid="content-banner">
      <span aria-hidden className="w-1 shrink-0 bg-ember" />
      <div className="flex min-w-0 flex-1 flex-col gap-4 px-5 py-4 sm:flex-row sm:items-center sm:gap-5 sm:px-6">
        {img && (
          <ThemedImage
            src={img.url}
            srcSet={img.srcset}
            darkSrc={img.dark?.url}
            darkSrcSet={img.dark?.srcset}
            sizes="(min-width: 640px) 160px, 100vw"
            alt={img.alt}
            className="h-24 w-full shrink-0 rounded-[12px] border border-line object-cover sm:h-16 sm:w-40"
          />
        )}
        <div className="min-w-0 flex-1">
          {title && <div className="text-[15px] font-medium tracking-tight text-fg">{title}</div>}
          {text && <p className="mt-0.5 whitespace-pre-line text-[13px] leading-snug text-fg-2">{text}</p>}
        </div>
        {cta && (
          <div className="shrink-0">
            {external ? (
              <a href={cta.href} target="_blank" rel="noopener noreferrer">
                {button}
              </a>
            ) : (
              <Link href={cta.href}>{button}</Link>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
