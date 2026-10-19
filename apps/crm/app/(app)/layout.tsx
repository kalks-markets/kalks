import { cookies, headers } from "next/headers";
import { redirect } from "next/navigation";
import { IS_DEMO } from "@kalks/mock/mode";
import { SESSION_COOKIE, fetchMe, safeNext } from "@/lib/gateway";
import { hostOf } from "@/lib/tenant-host";
import { SessionProvider } from "@/components/session";
import { ClientShell } from "@/components/shell";
import { FeaturesProvider } from "@/components/tenant-config";
import { tenantConfig } from "@/lib/tenant-config";
import { forEveryone, userFeatures } from "@/lib/features";

// Server-side session gate for every Client Area page: validates the HttpOnly session cookie with the
// gateway and hands the real client to the (client-side) shell. Demo builds skip it and browse as the
// sample client, so prospects can look around without an account. The shell gates live builds to the
// pages backed by real data (components/live-gate.tsx, lib/live.ts).
export default async function ClientAreaLayout({ children }: { children: React.ReactNode }) {
  if (IS_DEMO) {
    return (
      <SessionProvider>
        <ClientShell>{children}</ClientShell>
      </SessionProvider>
    );
  }

  const h = await headers();
  const next = safeNext(h.get("x-kalks-path"), "");
  const token = (await cookies()).get(SESSION_COOKIE)?.value;
  if (!token) redirect(next ? `/login?next=${encodeURIComponent(next)}` : "/login");

  // session, broker config and the client's own switches (sections after targeting rules) are independent: fetch them
  // together
  const [user, cfg, mine] = await Promise.all([fetchMe(token, h), tenantConfig(), userFeatures(token, h, hostOf(h))]);
  if (user === "unavailable") throw new Error("Sign-in service is unavailable. Please try again shortly.");
  if (!user) redirect(next ? `/api/auth/expired?next=${encodeURIComponent(next)}` : "/api/auth/expired");

  return (
    <SessionProvider user={user}>
      <FeaturesProvider value={mine ?? forEveryone(cfg)}>
        <ClientShell>{children}</ClientShell>
      </FeaturesProvider>
    </SessionProvider>
  );
}
