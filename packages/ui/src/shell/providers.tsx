"use client";

import * as React from "react";
import { useRouter } from "next/navigation";
import { Toaster } from "sonner";
import type { Locale, PartialCatalog } from "@kalks/i18n";
import { I18nProvider, useLocale } from "@kalks/i18n/react";
import { TooltipProvider } from "../components/overlays";
import { KalksThemeProvider, type ThemeChoice } from "./theme";

export type I18nInit = {
  locale: string;
  messages: PartialCatalog;
  /** Same-origin endpoint that saves the language on the signed-in user's profile (POST { locale }). */
  persistUrl?: string;
};

function I18n({ init, children }: { init?: I18nInit; children: React.ReactNode }) {
  const router = useRouter();
  if (!init) return <>{children}</>;
  const persist = (code: Locale) => {
    if (!init.persistUrl) return;
    void fetch(init.persistUrl, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ locale: code }) }).catch(() => {});
  };
  return (
    <I18nProvider locale={init.locale} messages={init.messages} onChange={persist} refresh={() => router.refresh()}>
      {children}
    </I18nProvider>
  );
}

/** Sonner toasts as Kalks 2 banners: frosted s2, radius 20, bold title + one line. */
export const TOAST_CLASSNAMES = {
  toast:
    "!rounded-[20px] !border-0 !bg-[color-mix(in_oklab,var(--k-surface-2)_84%,transparent)] !text-fg !backdrop-blur-[26px] !backdrop-saturate-[1.6] !shadow-[inset_0_0_0_1px_var(--k-border),var(--k-shadow-pop)] !px-3.5 !py-3 !gap-3",
  title: "!text-[14px] !font-semibold",
  description: "!text-[13px] !text-fg-2",
  success: "[&_[data-icon]]:!text-ok",
  error: "[&_[data-icon]]:!text-down",
  warning: "[&_[data-icon]]:!text-warn",
  info: "[&_[data-icon]]:!text-up",
  actionButton: "!rounded-[10px] !bg-ink !text-ink-fg !font-semibold",
  cancelButton: "!rounded-[10px] !bg-surface-3 !text-fg",
} as const;

function DirToaster() {
  const { dir } = useLocale();
  return <Toaster dir={dir} position={dir === "rtl" ? "top-left" : "top-right"} toastOptions={{ classNames: TOAST_CLASSNAMES }} />;
}

/** Theme (Auto by default: follows the device), language, tooltips and toasts. */
export function Providers({ children, defaultTheme = "system", i18n }: { children: React.ReactNode; defaultTheme?: ThemeChoice; i18n?: I18nInit }) {
  return (
    <KalksThemeProvider defaultTheme={defaultTheme}>
      <I18n init={i18n}>
        <TooltipProvider>
          {children}
          <DirToaster />
        </TooltipProvider>
      </I18n>
    </KalksThemeProvider>
  );
}
