"use client";

import * as React from "react";
import { Toaster } from "sonner";
import { TooltipProvider } from "@kalks/ui";
import { KalksThemeProvider } from "@kalks/ui/theme";
import type { PartialCatalog } from "@kalks/i18n";
import { I18nProvider, useLocale } from "@kalks/i18n/react";
import { ModulesProvider } from "@/components/modules";
import type { Modules } from "@/lib/modules";

/** Phones get top-center toasts (full width), everything else top-right. */
function useNarrow() {
  const [n, setN] = React.useState(false);
  React.useEffect(() => {
    const mq = window.matchMedia("(max-width: 639px)");
    const f = () => setN(mq.matches);
    f();
    mq.addEventListener("change", f);
    return () => mq.removeEventListener("change", f);
  }, []);
  return n;
}

/**
 * Terminal providers: same theme/tooltip stack as the CRM, with compact toasts at the top.
 * The terminal shell moves them clear of its chrome through --t-toast-top / --t-toast-right
 * (below the chart toolbar, left of the order panel); other pages use the defaults.
 */
export function TerminalProviders({ children, locale, messages, modules = null }: { children: React.ReactNode; locale: string; messages: PartialCatalog; /** the broker's module switches (lib/tenant-brand.ts) */ modules?: Modules | null }) {
  return (
    <KalksThemeProvider>
      <I18nProvider locale={locale} messages={messages}>
        <ModulesProvider modules={modules}>
          <TooltipProvider>
            {children}
            <TerminalToaster />
          </TooltipProvider>
        </ModulesProvider>
      </I18nProvider>
    </KalksThemeProvider>
  );
}

// Desktop toasts: a compact stack right under the 🔔 (shell/notifications.tsx sets --t-toast-top / --t-toast-right
// from the bell; Full chart: the window's top-right corner), at most 3, newest on top, 320 px wide; info and success go
// after 3 s, warnings 5 s, errors 6 s (lib/notify.ts), paused while hovered; every one stays in the bell's list.
// In right-to-left layouts they stay top-right too and only their text direction follows the language.
function TerminalToaster() {
  const narrow = useNarrow();
  const { dir } = useLocale();
  return (
    <Toaster
      dir={dir}
      position={narrow ? "top-center" : "top-right"}
      offset={{ top: "var(--t-toast-top, 58px)", right: "var(--t-toast-right, 12px)" }}
      mobileOffset={{ top: "var(--t-toast-top-m, 12px)", left: 10, right: 10 }}
      visibleToasts={3}
      expand
      gap={6}
      style={{ "--width": "320px" } as React.CSSProperties}
      toastOptions={{
        classNames: {
          toast: "t-toast !rounded-[11px] !border !border-line-top !text-fg !py-2 !px-3 !gap-2 !shadow-[var(--t-shadow-pop)] !text-[12.5px]",
          title: "!font-medium",
          description: "!text-fg-3 !font-mono !text-[10.5px] !leading-[14px]",
          actionButton: "!bg-accent-strong !text-white !text-[11px] !h-6 !rounded-[6px]",
        },
      }}
    />
  );
}
