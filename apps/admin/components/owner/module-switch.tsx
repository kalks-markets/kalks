"use client";

import * as React from "react";
import { Button, Dialog, Field, Input } from "@kalks/ui";
import { act } from "@/components/rbac/kit";

/**
 * What a broker's clients lose when the Platform Owner switches a module off (gateway tenancy.rs keys). Shown in the
 * confirm dialog of the Brokers › Modules grid and the tenant page; English only, like the rest of the Back Office.
 */
export const MODULE_LOSES: Record<string, string> = {
  copy_trading: "Copy trading pages and new copy subscriptions (running copies keep mirroring).",
  pamm: "PAMM fund pages, new funds and new investments (existing investments stay).",
  mam: "MAM manager and managed-account pages and new MAM links (existing links keep running).",
  prop: "Prop challenges: buying, dashboards and payout requests (running challenges are still evaluated).",
  ib: "The partner dashboard, referral links pages, commissions and payouts pages (tracking and accrual continue).",
  algo: "The strategy builder, backtests and marketplace; running strategies pause (kept, nothing is deleted).",
  api: "API keys, webhooks and the public API: every key and webhook is refused until switched back on.",
  academy: "Courses, quizzes, exams and the AI coach (progress and certificates are kept; certificates stay verifiable).",
  wallet: "The USDT wallet: deposits, withdrawals and transfers pages.",
  rewards: "Contests, loyalty points, cashback, promotions and bonus claims (banners and share cards stay).",
  options: "The Options page, Kalks Trader's options workspace and new option orders (open positions stay until closed by the desk or expiry).",
  news: "Market news, the news map and the daily brief (dashboard cards and Kalks Trader's News tab too).",
  calendar: "The economic calendar, event reminders and alerts (dashboard card and Kalks Trader's Calendar tab too).",
  markets: "The Markets page, movers and heat map on the dashboard.",
  ai: "Ask Kalks AI on the dashboard and AI Trader in Kalks Trader and the app.",
  support_chat: "The live support chat with the bot and agents (the Support page keeps email).",
  circle: "Kalks Circle for this broker's clients: feed, profiles, stories, chat and rooms (their content stays; the shared community keeps running).",
};

type Pending = { tenantId: number; tenantName: string; key: string; name: string; value: boolean | null; effective: boolean };

/**
 * Switching a module asks to confirm with a reason (audited with the switch, `settings.module_toggled`). Returns
 * `ask` to open the dialog and the dialog element to render once.
 */
export function useModuleSwitch(onDone: () => void) {
  const [p, setP] = React.useState<Pending | null>(null);
  const [reason, setReason] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const ask = React.useCallback((x: Pending) => {
    setReason("");
    setP(x);
  }, []);
  const save = async () => {
    if (!p) return;
    setBusy(true);
    const ok = await act("PUT", `/api/owner/tenants/${p.tenantId}/features`, { [p.key]: p.value, reason: reason.trim() }, p.value === null ? `${p.name}: back to default` : `${p.name} ${p.value ? "on" : "off"} for ${p.tenantName}`);
    setBusy(false);
    if (ok) {
      setP(null);
      onDone();
    }
  };
  const off = p ? p.effective === false : false;
  const title = !p ? "" : p.value === null ? `Reset ${p.name} for ${p.tenantName}?` : `Switch ${p.name} ${p.value ? "on" : "off"} for ${p.tenantName}?`;
  const dialog = (
    <Dialog open={!!p} onOpenChange={(o) => !o && setP(null)} title={title} description={p ? (off ? "Clients lose:" : "Clients get back:") : undefined}>
      {p && (
        <div data-testid="module-switch-dialog">
          <p className="rounded-[12px] bg-surface-2 px-3.5 py-2.5 text-[13px] leading-relaxed text-fg-2">{MODULE_LOSES[p.key] ?? "Every page and API of this module."}</p>
          <p className="mt-2 text-[12px] text-fg-3">Takes effect within about 30 seconds in the Client Area, Kalks Trader, the app and the services.</p>
          <Field label="Reason (audit log)" className="mt-4">
            <Input value={reason} onChange={(e) => setReason(e.target.value)} placeholder="e.g. not in this broker's plan" name="module-reason" autoFocus />
          </Field>
          <div className="mt-5 flex justify-end gap-2">
            <Button variant="ghost" onClick={() => setP(null)}>
              Cancel
            </Button>
            <Button variant={off ? "ember" : "surface"} disabled={busy || reason.trim().length < 3} onClick={() => void save()}>
              {p.value === null ? "Reset to default" : off ? `Switch ${p.name} off` : `Switch ${p.name} on`}
            </Button>
          </div>
        </div>
      )}
    </Dialog>
  );
  return { ask, dialog };
}
