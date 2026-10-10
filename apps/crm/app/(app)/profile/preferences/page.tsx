"use client";

import * as React from "react";
import Link from "next/link";
import { Bell, ChevronRight, Clock, Languages, Mail } from "lucide-react";
import { toast } from "sonner";
import { Card, CardHeader, Flag, LANGUAGES, PageHeader, Reveal, Segmented, Toggle, cn } from "@/components/kit";
import { useLocale, useT } from "@kalks/i18n/react";
import { IS_DEMO } from "@kalks/mock/mode";

const NOTIFS = [
  ["profile.prefs.notif.fills", true, true],
  ["profile.prefs.notif.margin", true, true],
  ["profile.prefs.notif.wallet", true, true],
  ["profile.prefs.notif.partner", true, false],
  ["profile.prefs.notif.copy", true, false],
  ["profile.prefs.notif.priceAlerts", true, false],
  ["profile.prefs.notif.news", false, true],
  ["profile.prefs.notif.promotions", false, false],
] as const;

export default function PreferencesPage() {
  const t = useT();
  const { locale, setLocale } = useLocale();
  const [tz, setTz] = React.useState<"server" | "local">("server");
  const [prefs, setPrefs] = React.useState(() => NOTIFS.map(([, e, i]) => ({ email: e, app: i })));
  return (
    <div>
      <PageHeader title={t("profile.prefs.title")} subtitle={IS_DEMO ? t("profile.prefs.subtitle") : t("profile.prefs.subtitleLive")} />
      <div className={cn("grid grid-cols-1 gap-4", IS_DEMO ? "xl:grid-cols-3" : "lg:grid-cols-2")}>
        {IS_DEMO && <Reveal delay={0.05}>
          <Card className="h-full">
            <CardHeader title={t("profile.prefs.timeDisplay")} icon={<Clock />} />
            <div className="p-6">
              <Segmented value={tz} onChange={(v) => { setTz(v); toast.success(t("profile.prefs.timeUpdated")); }} options={[{ value: "server", label: t("profile.prefs.serverTime") }, { value: "local", label: t("profile.prefs.localTime") }]} />
              <p className="mt-4 text-[13px] leading-relaxed text-fg-3">{t("profile.prefs.timeHint")}</p>
            </div>
          </Card>
        </Reveal>}

        <Reveal delay={0.1}>
          <Card className="h-full">
            <CardHeader title={t("common.language")} subtitle={t("profile.prefs.languageCount", { count: LANGUAGES.length })} icon={<Languages />} />
            <div className="grid max-h-56 grid-cols-2 gap-1.5 overflow-y-auto p-6 pt-4">
              {LANGUAGES.map((l) => (
                <button
                  key={l.code}
                  onClick={() => void setLocale(l.code)}
                  className={cn("flex items-center gap-2 rounded-xl px-2.5 py-2 text-start text-[13px]", locale === l.code ? "bg-ember-soft text-ember" : "text-fg-2 hover:bg-surface-3")}
                >
                  <Flag country={l.flag} className="size-4" />
                  <span className="truncate">{l.name}</span>
                </button>
              ))}
            </div>
          </Card>
        </Reveal>
      </div>

      {!IS_DEMO && (
        <Reveal delay={0.1}>
          <Link href="/profile/notifications" className="mt-4 block">
            <Card className="flex items-center gap-4 px-6 py-5 transition-colors hover:border-[var(--k-border-top)]">
              <span className="grid size-10 shrink-0 place-items-center rounded-full border border-line bg-surface-3 text-fg-2">
                <Bell className="size-[18px]" />
              </span>
              <div className="min-w-0 flex-1">
                <div className="text-[14px] font-medium">{t("profile.notifCard.title")}</div>
                <div className="mt-0.5 text-[12.5px] text-fg-3">{t("profile.notifCard.hint")}</div>
              </div>
              <ChevronRight className="size-4 text-fg-3 rtl:-scale-x-100" />
            </Card>
          </Link>
        </Reveal>
      )}

      {IS_DEMO && <Reveal delay={0.15}>
        <Card className="mt-4">
          <CardHeader title={t("profile.prefs.notifications")} subtitle={t("profile.prefs.notificationsHint")} icon={<Bell />} />
          <div className="p-6 pt-4">
            <div className="grid grid-cols-[1fr_80px_80px] items-center gap-y-1 text-[13.5px]">
              <span className="text-xs uppercase tracking-wider text-fg-3">{t("profile.prefs.event")}</span>
              <span className="flex items-center justify-center gap-1 text-xs uppercase tracking-wider text-fg-3"><Mail className="size-3" /> {t("common.email")}</span>
              <span className="flex items-center justify-center gap-1 text-xs uppercase tracking-wider text-fg-3"><Bell className="size-3" /> {t("profile.notifications.inApp")}</span>
              {NOTIFS.map(([key], i) => {
                const label = t(key);
                return (
                <React.Fragment key={key}>
                  <span className="border-t border-line py-3 text-fg-2">{label}</span>
                  <span className="flex justify-center border-t border-line py-3">
                    <Toggle checked={prefs[i]!.email} onChange={(v) => setPrefs((p) => p.map((x, k) => (k === i ? { ...x, email: v } : x)))} label={t("profile.notifications.toggleEmail", { label })} />
                  </span>
                  <span className="flex justify-center border-t border-line py-3">
                    <Toggle checked={prefs[i]!.app} onChange={(v) => setPrefs((p) => p.map((x, k) => (k === i ? { ...x, app: v } : x)))} label={t("profile.notifications.toggleInApp", { label })} />
                  </span>
                </React.Fragment>
                );
              })}
            </div>
          </div>
        </Card>
      </Reveal>}
    </div>
  );
}
