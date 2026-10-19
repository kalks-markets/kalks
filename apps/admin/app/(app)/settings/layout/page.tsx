"use client";

import { IS_DEMO } from "@kalks/mock/mode";
import { ComingSoon } from "@kalks/ui";
import { LiveClientLayout } from "@/components/rbac/client-layout";

export default function Page() {
  return IS_DEMO ? <ComingSoon title="Client Area layout" text="Client Area pages and dashboard blocks, switched for everyone or by country, group or client, are shown on live builds." /> : <LiveClientLayout />;
}
