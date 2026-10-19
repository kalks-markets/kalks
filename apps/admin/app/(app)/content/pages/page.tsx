"use client";

import { IS_DEMO } from "@kalks/mock/mode";
import { ComingSoon } from "@kalks/ui";
import { LiveContentPages } from "@/components/content-live/pages-editor";

export default function ContentPagesPage() {
  // live builds: the real editor on services/content; demo builds have no content service
  return IS_DEMO ? <ComingSoon title="Pages" text="Edit texts, images and links of the website, the Client Area and Kalks Trader in 22 languages. Shown on live builds." /> : <LiveContentPages />;
}
