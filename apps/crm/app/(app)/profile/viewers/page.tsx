"use client";

import * as React from "react";
import { IS_DEMO as DEMO_BUILD } from "@kalks/mock/mode";
import { LiveViewers } from "@/components/security/live-viewers";
import { Eye, EyeOff, Plus, ShieldOff, UserRound, CalendarDays } from "lucide-react";
import { toast } from "sonner";
import { Avatar, Button, Card, CardHeader, Chip, CopyButton, Dialog, Field, Icon3D, Input, PageHeader, Reveal, StatusChip, Toggle } from "@/components/kit";
import { ACCOUNTS, PEOPLE } from "@kalks/mock";
import { useT } from "@kalks/i18n/react";

const SECTIONS = ["profile.viewers.section.dashboard", "profile.viewers.section.accounts", "profile.viewers.section.history", "profile.viewers.section.portfolio", "profile.viewers.section.wallet", "profile.viewers.section.partner"] as const;

const VIEWERS = [
  { id: "v1", name: "Rahul Verma", note: "Accountant", login: "view-arjun-rv", person: PEOPLE[13]!, accounts: ["80412337", "80412512"], sections: 4, expires: "31 Dec 2026", lastSeen: "2h ago", status: "active" },
  { id: "v2", name: "Investor preview", note: "Shared with prospective investors", login: "view-arjun-inv", person: undefined, accounts: ["80412337"], sections: 2, expires: "15 Oct 2026", lastSeen: "5d ago", status: "active" },
  { id: "v3", name: "Mentor access", note: "Trading mentor review", login: "view-arjun-mnt", person: PEOPLE[9]!, accounts: ["80412512"], sections: 3, expires: "Expired 01 Sep", lastSeen: "28d ago", status: "expired" },
];

const ACTIVITY = [
  { who: "Rahul Verma", what: "Viewed trade history · #80412337", when: "Today 19:02" },
  { who: "Rahul Verma", what: "Downloaded statement · Aug 2026", when: "Today 18:57" },
  { who: "Investor preview", what: "Viewed dashboard", when: "19 Sep 11:20" },
  { who: "Mentor access", what: "Login blocked — access expired", when: "02 Sep 09:14" },
];

function DemoViewersPage() {
  const t = useT();
  const [show, setShow] = React.useState<Record<string, boolean>>({});
  const live = ACCOUNTS.filter((a) => a.type === "live");
  return (
    <div>
      <PageHeader
        title={t("profile.viewers.title")}
        subtitle={t("profile.viewers.subtitle")}
        actions={
          <Dialog
            title={t("profile.viewers.createTitle")}
            description={t("profile.viewers.createDescription", { host: "app.kalkstrade.com" })}
            trigger={
              <Button variant="ember">
                <Plus /> {t("profile.viewers.new")}
              </Button>
            }
            footer={<Button variant="ember" onClick={() => toast.success(t("profile.viewers.created"), { description: t("profile.viewers.createdHint") })}>{t("profile.viewers.create")}</Button>}
          >
            <div className="space-y-4">
              <Field label={t("profile.viewers.label")}>
                <Input leading={<UserRound />} placeholder={t("profile.viewers.labelPlaceholder")} />
              </Field>
              <div className="grid grid-cols-2 gap-3">
                <Field label={t("profile.viewers.username")}>
                  <Input defaultValue="view-arjun-04" className="font-mono" />
                </Field>
                <Field label={t("profile.viewers.expiresOn")}>
                  <Input type="date" leading={<CalendarDays />} defaultValue="2026-12-31" />
                </Field>
              </div>
              <Field label={t("profile.viewers.accounts")}>
                <div className="flex flex-wrap gap-2">
                  {live.map((a) => (
                    <label key={a.login} className="k-row flex cursor-pointer items-center gap-2 px-3 py-2 text-[13px]">
                      <input type="checkbox" defaultChecked className="accent-[var(--k-ember)]" />
                      <span className="font-mono">#{a.login}</span>
                      <span className="text-fg-3">{a.group}</span>
                    </label>
                  ))}
                </div>
              </Field>
              <Field label={t("profile.viewers.sections")}>
                <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
                  {SECTIONS.map((s, i) => (
                    <label key={s} className="k-row flex cursor-pointer items-center gap-2 px-3 py-2 text-[13px]">
                      <input type="checkbox" defaultChecked={i < 3} className="accent-[var(--k-ember)]" />
                      {t(s)}
                    </label>
                  ))}
                </div>
              </Field>
            </div>
          </Dialog>
        }
      />

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
        {VIEWERS.map((v, i) => (
          <Reveal key={v.id} delay={i * 0.05}>
            <Card className="h-full p-6">
              <div className="flex items-start justify-between">
                <div className="flex items-center gap-3">
                  {v.person ? <Avatar src={v.person.photo} name={v.name} size={44} /> : <span className="grid size-11 place-items-center rounded-full bg-surface-3"><Eye className="size-5 text-fg-2" /></span>}
                  <div>
                    <div className="font-medium">{v.name}</div>
                    <div className="text-xs text-fg-3">{v.note}</div>
                  </div>
                </div>
                <StatusChip status={v.status} />
              </div>
              <div className="mt-5 space-y-2 text-[13px]">
                <div className="k-row flex items-center justify-between px-3 py-2">
                  <span className="text-fg-3">{t("profile.viewers.username")}</span>
                  <span className="flex items-center gap-1 font-mono">
                    {v.login}
                    <CopyButton value={v.login} />
                  </span>
                </div>
                <div className="k-row flex items-center justify-between px-3 py-2">
                  <span className="text-fg-3">{t("common.password")}</span>
                  <span className="flex items-center gap-1 font-mono">
                    {show[v.id] ? "Vw#7qL2x!9" : "••••••••••"}
                    <button onClick={() => setShow((s) => ({ ...s, [v.id]: !s[v.id] }))} className="grid size-6 place-items-center text-fg-3 hover:text-fg" aria-label={t("profile.viewers.togglePassword")}>
                      {show[v.id] ? <EyeOff className="size-3.5" /> : <Eye className="size-3.5" />}
                    </button>
                  </span>
                </div>
              </div>
              <div className="mt-4 flex flex-wrap gap-1.5">
                {v.accounts.map((a) => (
                  <Chip key={a} size="sm" className="font-mono">
                    #{a}
                  </Chip>
                ))}
                <Chip size="sm">{t("profile.viewers.sectionCount", { count: v.sections })}</Chip>
              </div>
              <div className="mt-4 flex items-center justify-between border-t border-line pt-4 text-xs text-fg-3">
                <span>{v.expires}</span>
                <span>{t("profile.viewers.lastSeen", { when: v.lastSeen })}</span>
              </div>
              <div className="mt-4 flex gap-2">
                <Button size="sm" variant="surface" className="flex-1" onClick={() => toast(t("profile.viewers.editToast"))}>
                  {t("common.edit")}
                </Button>
                <Button size="sm" variant="down-outline" className="flex-1" onClick={() => toast.success(t("profile.viewers.revoked", { name: v.name }))}>
                  <ShieldOff /> {t("profile.viewers.revoke")}
                </Button>
              </div>
            </Card>
          </Reveal>
        ))}
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4 lg:grid-cols-3">
        <Reveal className="lg:col-span-2">
          <Card>
            <CardHeader title={t("profile.viewers.activity")} subtitle={t("profile.viewers.activityHint")} />
            <div className="space-y-2 p-6 pt-4">
              {ACTIVITY.map((a, i) => (
                <div key={i} className="k-row flex items-center gap-3 px-4 py-3 text-[13.5px]">
                  <span className="size-2 rounded-full bg-ember" />
                  <span className="font-medium">{a.who}</span>
                  <span className="flex-1 text-fg-2">{a.what}</span>
                  <span className="k-num text-xs text-fg-3">{a.when}</span>
                </div>
              ))}
            </div>
          </Card>
        </Reveal>
        <Reveal delay={0.05}>
          <Card hot className="h-full p-6">
            <Icon3D name="key" size={64} />
            <h3 className="mt-4 text-lg font-medium">{t("profile.viewers.investorTitle")}</h3>
            <p className="mt-1 text-sm text-fg-2">{t("profile.viewers.investorHint")}</p>
            <div className="mt-4 space-y-2">
              {live.map((a) => (
                <div key={a.login} className="flex items-center justify-between rounded-xl border border-white/10 light:border-line bg-black/20 light:bg-white/70 px-3 py-2 text-[13px]">
                  <span className="font-mono">#{a.login}</span>
                  <Toggle checked={a.login !== "80413001"} onChange={() => toast.success(t("profile.viewers.investorUpdated"))} label={t("profile.viewers.investorToggle", { login: a.login })} />
                </div>
              ))}
            </div>
          </Card>
        </Reveal>
      </div>
    </div>
  );
}

export default function ViewersPage() {
  return DEMO_BUILD ? <DemoViewersPage /> : <LiveViewers />;
}
