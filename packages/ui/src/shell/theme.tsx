"use client";

// Theme: Auto (follows the device, the default) · Light · Dark — KALKS2 §11. next-themes writes the resolved theme
// before first paint as data-theme="light|dark" (tokens.css) and as the .light / .dark class (older selectors and the
// Tailwind light:/dark: variants), and keeps following the OS while the choice is Auto.

import * as React from "react";
import { ThemeProvider, useTheme } from "next-themes";
import { Monitor, Moon, Sun } from "lucide-react";
import { useT } from "@kalks/i18n/react";
import { Segmented } from "../components/navigation";

export type ThemeChoice = "system" | "light" | "dark";

/** The one theme configuration of every Kalks app. */
export function KalksThemeProvider({ children, defaultTheme = "system", forcedTheme }: { children: React.ReactNode; defaultTheme?: ThemeChoice; forcedTheme?: "light" | "dark" }) {
  return (
    <ThemeProvider attribute={["class", "data-theme"]} defaultTheme={defaultTheme} forcedTheme={forcedTheme} enableSystem themes={["light", "dark"]} disableTransitionOnChange>
      {children}
    </ThemeProvider>
  );
}

/** The stored choice (Auto = "system") and the theme it resolves to. `mounted` is false during the first render. */
export function useThemeChoice(): { choice: ThemeChoice; resolved: "light" | "dark"; setChoice: (c: ThemeChoice) => void; mounted: boolean } {
  const { theme, resolvedTheme, setTheme } = useTheme();
  const [mounted, setMounted] = React.useState(false);
  React.useEffect(() => setMounted(true), []);
  const choice = (theme === "light" || theme === "dark" ? theme : "system") as ThemeChoice;
  return { choice, resolved: resolvedTheme === "dark" ? "dark" : "light", setChoice: setTheme, mounted };
}

/** Auto | Light | Dark segmented switch (three states, like the R0 sample). */
export function ThemeSwitch({ size = 26, iconsOnly, labels, className }: { size?: 26 | 30; iconsOnly?: boolean; labels?: Partial<Record<ThemeChoice, string>>; className?: string }) {
  const t = useT();
  const { choice, setChoice, mounted } = useThemeChoice();
  const name: Record<ThemeChoice, string> = {
    system: labels?.system ?? "Auto",
    light: labels?.light ?? t("shell.themeLight"),
    dark: labels?.dark ?? t("shell.themeDark"),
  };
  const icon = { system: <Monitor className="size-3.5" />, light: <Sun className="size-3.5" />, dark: <Moon className="size-3.5" /> };
  return (
    <Segmented<ThemeChoice>
      aria-label={t("shell.toggleTheme")}
      size={size}
      className={className}
      value={mounted ? choice : "system"}
      onChange={setChoice}
      options={(["system", "light", "dark"] as const).map((v) => ({
        value: v,
        title: name[v],
        label: iconsOnly ? (
          <>
            {icon[v]}
            <span className="sr-only">{name[v]}</span>
          </>
        ) : (
          name[v]
        ),
      }))}
    />
  );
}
