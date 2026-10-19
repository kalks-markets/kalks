"use client";

import { IS_DEMO } from "@kalks/mock/mode";
import { ComingSoon } from "@kalks/ui";
import { LiveMediaLibrary } from "@/components/content-live/media-library";

export default function ContentMediaPage() {
  // live builds: the real library on services/content; demo builds have no content service
  return IS_DEMO ? <ComingSoon title="Media library" text="Images with dark-mode files, resized versions and alt text in every language. Shown on live builds." /> : <LiveMediaLibrary />;
}
