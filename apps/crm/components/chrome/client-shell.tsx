"use client";

// Client Area chrome for the signed-in client: the pastel backdrop, the slim rail (labelled when expanded), the top
// area (module pages as text tabs, search, language, theme, notifications, profile pill) and, on phones and tablets,
// a bottom bar with a "More" drawer.

import * as React from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { ArrowDownToLine, ArrowUpRight, Eye, IdCard, LogOut, Settings, ShieldCheck, UserRound } from "lucide-react";
import { Avatar, Button, Chip, CommandPalette, LanguageMenu, MarketBoundary, ThemeToggle, TooltipProvider, cn } from "@/components/kit";
import { IS_DEMO } from "@kalks/mock";
import { useT } from "@kalks/i18n/react";
import { CRM_COMMANDS, NAV, localizeCommands, localizeNav } from "@/lib/nav";
import { navForFeatures, pageModule, useFeatures } from "@/components/tenant-config";
import { TERMINAL_URL } from "@/lib/live";
import { LiveGate } from "@/components/live-gate";
import { NotificationsBell, NotificationsProvider } from "@/components/notifications";
import { SupportLauncher } from "@/components/support/launcher";
import { KYC_CHIP, logout, useSession } from "@/components/session";
import { SessionGuard, ViewerBar, navForViewer } from "@/components/security/session-guard";
import { viewerPageAllowed } from "@/lib/viewer";
import { AccountNotices } from "@/components/account-notices";
import { BrandAvatar, Rail, useRailExpanded } from "./rail";
import { ProfilePill, SubNav, type MenuItem } from "./topbar";
import { MobileBar } from "./mobile-nav";
import { isActive } from "./nav-utils";

const ACCOUNT_MENU_DEMO = [
  { label: "shell.profile", icon: <UserRound />, href: "/profile" },
  { label: "shell.security", icon: <ShieldCheck />, href: "/profile/security" },
  { label: "shell.verification", icon: <IdCard />, href: "/profile/verification" },
  { label: "shell.preferences", icon: <Settings />, href: "/profile/preferences" },
] as const;

const ACCOUNT_MENU_LIVE = [
  { label: "shell.profile", icon: <UserRound />, href: "/profile" },
  { label: "shell.security", icon: <ShieldCheck />, href: "/profile/security" },
  { label: "shell.nav.viewers", icon: <Eye />, href: "/profile/viewers" },
  { label: "shell.verification", icon: <IdCard />, href: "/profile/verification" },
  { label: "shell.preferences", icon: <Settings />, href: "/profile/preferences" },
] as const;

/** True once the page has scrolled (the sticky top area then gets its frosted backdrop). */
function useScrolled() {
  const [s, setS] = React.useState(false);
  React.useEffect(() => {
    const on = () => setS(window.scrollY > 8);
    on();
    window.addEventListener("scroll", on, { passive: true });
    return () => window.removeEventListener("scroll", on);
  }, []);
  return s;
}

export function ClientShell({ children }: { children: React.ReactNode }) {
  const t = useT();
  const pathname = usePathname();
  const me = useSession();
  const verified = me.kyc_status === "verified";
  const kyc = KYC_CHIP[me.kyc_status];
  // modules the broker switched off (Platform Owner, D112) disappear from the navigation
  const features = useFeatures();
  // a view-only session (D90) only gets the sections it was given, and no account actions
  const viewer = me.viewer ?? null;
  const modules = localizeNav(viewer ? navForViewer(navForFeatures(NAV, features), viewer) : navForFeatures(NAV, features), t);
  const current = modules.find((m) => isActive(pathname, m));
  const subs = current?.sub && current.sub.length > 1 ? current.sub : null;
  const [expanded, setExpanded] = useRailExpanded();
  const scrolled = useScrolled();
  const signOut = () => void logout();

  const commands = localizeCommands(CRM_COMMANDS, t)
    .filter((c) => {
      const m = pageModule(c.href);
      return (!m || features?.modules[m] !== false) && (!viewer || viewerPageAllowed(viewer, c.href));
    })
    .map((c) => ({ group: c.group, label: c.label, href: c.href, icon: <c.Icon /> }));

  const menuHeader = (
    <div className="flex items-center gap-3">
      <Avatar name={me.name} size={42} />
      <div className="min-w-0">
        <div className="truncate text-sm font-bold">{me.name}</div>
        <div className="truncate text-xs text-fg-3">{viewer ? `${t("security.sessions.viewOnly")} · ${viewer.label}` : me.email}</div>
        {viewer ? (
          <Chip tone="info" size="sm" className="mt-1.5" dot>
            Read-only
          </Chip>
        ) : (
          <Chip tone={kyc.tone} size="sm" className="mt-1.5" dot>
            {t(`shell.kyc.${me.kyc_status}`)}
          </Chip>
        )}
      </div>
    </div>
  );
  const menuItems: MenuItem[] = [
    ...(viewer ? [] : IS_DEMO ? ACCOUNT_MENU_DEMO : ACCOUNT_MENU_LIVE).map(({ label, ...m }) => ({ ...m, label: t(label) })),
    ...(viewer ? [] : ["sep" as const]),
    { label: t("shell.logOut"), icon: <LogOut />, onSelect: signOut, danger: true },
  ];

  const cta = viewer ? null : IS_DEMO ? (
    <Link href="/wallet/deposit" className="hidden md:block">
      <Button variant="ember">
        <ArrowDownToLine /> {t("shell.deposit")}
      </Button>
    </Link>
  ) : (
    <a href={TERMINAL_URL} target="_blank" rel="noopener" className="hidden md:block">
      <Button variant="ink">
        {t("shell.kalksTrader")} <ArrowUpRight className="rtl:-scale-x-100" />
      </Button>
    </a>
  );

  return (
    <NotificationsProvider userKey={String(me.id)} enabled={!viewer}>
      <TooltipProvider>
        <div className="relative min-h-dvh overflow-x-clip">
          <div className="k-backdrop" aria-hidden />
          <Rail modules={modules} expanded={expanded} onToggle={() => setExpanded(!expanded)} onSignOut={signOut} />
          <div className={cn("relative transition-[padding] duration-300", expanded ? "lg:ps-[248px]" : "lg:ps-[84px]")}>
            <header data-scrolled={scrolled || undefined} className="k-topbar sticky top-0 z-30">
              <div className="mx-auto flex h-[68px] max-w-[1640px] items-center gap-3 px-4 sm:px-6 lg:h-[76px] lg:px-8">
                <Link href="/" className="lg:hidden" aria-label={t("shell.nav.dashboard")}>
                  <BrandAvatar size={40} />
                </Link>
                <div className="k-display min-w-0 flex-1 truncate text-[17px] font-bold tracking-[-0.01em] lg:hidden">{current?.label}</div>
                <div className="hidden min-w-0 flex-1 lg:flex">{subs ? <SubNav fit items={subs} className="-ms-2.5" /> : <span className="k-display truncate text-[17px] font-bold tracking-[-0.01em]">{current?.label}</span>}</div>
                <div className="flex shrink-0 items-center gap-2">
                  <CommandPalette items={commands} />
                  <span className="hidden sm:contents">
                    <LanguageMenu />
                    <ThemeToggle />
                  </span>
                  {!viewer && <NotificationsBell />}
                  {cta}
                  <span className="hidden xl:block">
                    <ProfilePill name={me.name} email={viewer ? viewer.label : me.email} verified={verified} header={menuHeader} items={menuItems} label={t("shell.accountMenu")} />
                  </span>
                  <span className="xl:hidden">
                    <ProfilePill compact name={me.name} email={me.email} verified={verified} header={menuHeader} items={menuItems} label={t("shell.accountMenu")} />
                  </span>
                </div>
              </div>
              {subs && (
                <div className="px-2 pb-1 sm:px-4 lg:hidden">
                  <SubNav items={subs} />
                </div>
              )}
            </header>
            <main className="relative mx-auto max-w-[1640px] px-4 pb-32 pt-3 sm:px-6 lg:px-8 lg:pb-14">
              {/* demo builds rebuild their sample views from live prices once; live pages follow the feed mode themselves */}
              <MarketBoundary remount={IS_DEMO}>
                {/* staff session banner, account restrictions, presence heartbeat */}
                <AccountNotices />
                {viewer && <ViewerBar viewer={viewer} owner={me.name} />}
                <LiveGate>{children}</LiveGate>
              </MarketBoundary>
            </main>
          </div>
          <MobileBar modules={modules} onSignOut={signOut} />
        </div>
      </TooltipProvider>
      {!IS_DEMO && <SessionGuard />}
      {!viewer && features?.modules.support_chat !== false && <SupportLauncher />}
    </NotificationsProvider>
  );
}
