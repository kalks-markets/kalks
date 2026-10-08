"use client";

import * as React from "react";
import { Eye, Globe, Lock, Pencil, RefreshCcw, TriangleAlert, Users } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, CardHeader, Chip, Dialog, Field, KeyValue, Money, Reveal, cn } from "@/components/kit";
import { PasswordInput } from "@/components/accounts/security";
import { FormError } from "@/components/auth";
import { STEPUP_CODES, StepUpCode, StepUpDialog, useStepUp } from "@/components/stepup";
import { ApiError, STATUS_LABEL, curOf, errorToast, fmtDate, modeLabel, serverOf, tradingApi, type EngineAccount } from "./api";
import { PasswordRules, SecretField, TradeButton, demoTarget, livePasswordOk, refillsLeft, useRefill } from "./ui";
import { Trans, useT } from "@kalks/i18n/react";

/* ------------------------------------------------------------------ */
/* Change password (D4, D90)                                           */
/* ------------------------------------------------------------------ */

function ChangePasswordDialog({ a, kind, open, onOpenChange }: { a: EngineAccount; kind: "trading" | "investor"; open: boolean; onOpenChange: (o: boolean) => void }) {
  const t = useT();
  const [pw, setPw] = React.useState("");
  const [confirm, setConfirm] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [phase, setPhase] = React.useState<"form" | "code" | "done">("form");
  const [formErr, setFormErr] = React.useState<string | null>(null);
  const [done, setDone] = React.useState<{ password: string; revoked: number } | null>(null);
  const s = useStepUp(kind === "trading" ? "trading_password" : "investor_password", String(a.login));
  const { reset } = s;
  React.useEffect(() => {
    if (!open) {
      setPw("");
      setConfirm("");
      setDone(null);
      setPhase("form");
      setFormErr(null);
      reset();
    }
  }, [open, reset]);
  const ok = livePasswordOk(pw) && pw === confirm;

  // step 1: email a confirmation code (D20)
  const requestCode = async () => {
    setFormErr(null);
    if (await s.start()) setPhase("code");
  };

  // step 2: code -> step-up token -> change
  const submit = async (c?: string) => {
    if (busy) return;
    const token = await s.verify(c);
    if (!token) return;
    setBusy(true);
    try {
      const r = await tradingApi<{ sessionsRevoked: number }>(`accounts/${a.login}/passwords`, { body: { kind, password: pw, stepup_token: token } });
      setDone({ password: pw, revoked: r.sessionsRevoked ?? 0 });
      setPhase("done");
      setPw("");
      setConfirm("");
      toast.success(t(kind === "trading" ? "accountDetail.pw.changed.trading" : "accountDetail.pw.changed.investor"), { description: `#${a.login}${r.sessionsRevoked ? ` · ${t("accountDetail.pw.sessionsSignedOut", { count: r.sessionsRevoked })}` : ""}` });
    } catch (e) {
      // the confirmation is spent either way: back to the form, a new code is sent on the next try
      s.reset();
      setPhase("form");
      if (e instanceof ApiError && (e.status === 422 || STEPUP_CODES.has(e.code))) setFormErr(e.message);
      else errorToast(t(kind === "trading" ? "accountDetail.pw.changeError.trading" : "accountDetail.pw.changeError.investor"), e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={kind === "trading" ? t("accountDetail.pw.changeTitle.trading") : t("accountDetail.pw.changeTitle.investor")}
      description={
        phase === "code"
          ? t("accountDetail.pw.confirmWithCode")
          : kind === "trading"
            ? t("accountDetail.pw.descTrading", { login: a.login })
            : t("accountDetail.pw.descInvestor", { login: a.login })
      }
      width={520}
      footer={
        phase === "done" ? (
          <Button variant="ember" onClick={() => onOpenChange(false)}>
            {t("common.done")}
          </Button>
        ) : phase === "code" ? (
          <>
            <Button
              variant="ghost"
              onClick={() => {
                s.reset();
                setPhase("form");
              }}
            >
              {t("common.back")}
            </Button>
            <Button variant="ember" disabled={s.code.length !== 6 || s.verifying || busy} onClick={() => void submit()}>
              {s.verifying ? t("accountDetail.pw.checking") : busy ? t("accountDetail.pw.saving") : t("accountDetail.pw.confirmSet")}
            </Button>
          </>
        ) : (
          <>
            <Button variant="ghost" onClick={() => onOpenChange(false)}>
              {t("common.cancel")}
            </Button>
            <Button variant="ember" disabled={!ok || s.sending} onClick={requestCode}>
              {s.sending ? t("accountDetail.pw.sendingCode") : t("common.continue")}
            </Button>
          </>
        )
      }
    >
      {phase === "done" && done ? (
        <div className="space-y-4">
          <SecretField label={t(kind === "trading" ? "accountDetail.pw.newLabel.trading" : "accountDetail.pw.newLabel.investor")} value={done.password} secret />
          <div className="flex items-start gap-2 rounded-[14px] border border-warn/25 bg-warn-soft px-3.5 py-3 text-[12.5px] text-fg-2">
            <TriangleAlert className="mt-0.5 size-4 shrink-0 text-warn" />
            {t("accountDetail.pw.shownOnce")}
          </div>
        </div>
      ) : phase === "code" ? (
        <StepUpCode s={s} what={t(kind === "trading" ? "accountDetail.pw.stepUpWhat.trading" : "accountDetail.pw.stepUpWhat.investor", { login: a.login })} onSubmit={(c) => void submit(c)} />
      ) : (
        <div className="space-y-4">
          <FormError>{formErr ?? (s.err && !s.challenge ? s.err.message : null)}</FormError>
          <Field label={t("accountDetail.pw.new")}>
            <PasswordInput value={pw} onChange={setPw} generate />
          </Field>
          <PasswordRules password={pw} />
          <Field label={t("accountDetail.pw.confirmNew")} error={confirm && confirm !== pw ? t("accountDetail.pw.mismatch") : undefined}>
            <PasswordInput value={confirm} onChange={setConfirm} placeholder={t("accountDetail.pw.repeat")} />
          </Field>
          <p className="text-[12px] text-fg-3">{t("accountDetail.pw.differentHint")}</p>
        </div>
      )}
    </Dialog>
  );
}

export function CredentialsPanel({ a }: { a: EngineAccount }) {
  const t = useT();
  const [dlg, setDlg] = React.useState<"trading" | "investor" | null>(null);
  return (
    <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
      <Reveal className="xl:col-span-7">
        <Card className="h-full">
          <CardHeader title={t("accountDetail.creds.title")} subtitle={t("accountDetail.creds.subtitle")} icon={<Lock />} />
          <div className="grid grid-cols-1 gap-4 px-4 pb-6 pt-5 sm:grid-cols-2 sm:px-6">
            <SecretField label={t("accountDetail.info.login")} value={String(a.login)} />
            <SecretField label={t("accountDetail.info.server")} value={serverOf(a)} hint="GMT+3 / GMT+2" />
            <div className="sm:col-span-2">
              <div className="k-row flex flex-col gap-4 p-4 sm:flex-row sm:items-center">
                <span className="grid size-10 shrink-0 place-items-center rounded-full border border-ember/30 bg-ember-soft text-ember">
                  <Lock className="size-4" />
                </span>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2 text-[14px] font-medium">
                    {t("accountDetail.pw.trading")} <Chip size="sm">{t("accountDetail.creds.fullAccess")}</Chip>
                  </div>
                  <div className="text-[12.5px] text-fg-3">{t("accountDetail.creds.tradingDesc")}</div>
                </div>
                <Button size="sm" variant="surface" onClick={() => setDlg("trading")}>
                  <Pencil /> {t("accountDetail.creds.change")}
                </Button>
              </div>
            </div>
            <div className="sm:col-span-2">
              <div className="k-row flex flex-col gap-4 p-4 sm:flex-row sm:items-center">
                <span className="grid size-10 shrink-0 place-items-center rounded-full border border-line bg-surface-3 text-fg-2">
                  <Eye className="size-4" />
                </span>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2 text-[14px] font-medium">
                    {t("accountDetail.pw.investor")} <Chip size="sm" tone="info">{t("accountDetail.creds.readOnly")}</Chip>
                  </div>
                  <div className="text-[12.5px] text-fg-3">{t("accountDetail.creds.investorDesc")}</div>
                </div>
                <Button size="sm" variant="surface" onClick={() => setDlg("investor")}>
                  <Pencil /> {t("accountDetail.creds.change")}
                </Button>
              </div>
            </div>
            <div className="flex items-start gap-2 text-[12px] text-fg-3 sm:col-span-2">
              <TriangleAlert className="mt-0.5 size-3.5 shrink-0 text-warn" />
              {t("accountDetail.creds.securityNote")}
            </div>
          </div>
        </Card>
      </Reveal>
      <Reveal delay={0.05} className="xl:col-span-5">
        <Card className="h-full">
          <CardHeader title={t("accountDetail.investor.title")} subtitle={t("accountDetail.investor.subtitle")} icon={<Users />} />
          <div className="space-y-3 px-4 pb-6 pt-4 sm:px-6">
            <ol className="space-y-2.5 text-[13px] text-fg-2">
              {[
                t("accountDetail.investor.step1"),
                t("accountDetail.investor.step2", { login: a.login, server: serverOf(a) }),
                t("accountDetail.investor.step3"),
                t("accountDetail.investor.step4"),
              ].map((step, i) => (
                <li key={step} className="flex items-start gap-3">
                  <span className="k-num grid size-6 shrink-0 place-items-center rounded-full border border-line bg-surface-3 text-[11.5px] text-fg-2">{i + 1}</span>
                  <span className="pt-0.5">{step}</span>
                </li>
              ))}
            </ol>
            <div className="k-row flex items-center gap-3 px-4 py-3">
              <span className="grid size-9 shrink-0 place-items-center rounded-full border border-line bg-surface-3 text-fg-2">
                <Globe className="size-4" />
              </span>
              <div className="min-w-0 flex-1">
                <div className="text-[13.5px] font-medium">Kalks Trader</div>
                <div className="text-[11.5px] text-fg-3">{t("accountDetail.investor.webTerminal")}</div>
              </div>
              <TradeButton a={a} label={t("common.open")} />
            </div>
          </div>
        </Card>
      </Reveal>
      {dlg && <ChangePasswordDialog a={a} kind={dlg} open={!!dlg} onOpenChange={(o) => !o && setDlg(null)} />}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Settings: leverage (D15), demo funds (D8), details                  */
/* ------------------------------------------------------------------ */

function LeverageCard({ a, onChanged }: { a: EngineAccount; onChanged: () => void }) {
  const t = useT();
  const [lev, setLev] = React.useState(a.leverage);
  const [confirming, setConfirming] = React.useState(false);
  React.useEffect(() => setLev(a.leverage), [a.leverage]);
  const locked = a.positions > 0;
  const fmt = (l: number) => `1:${l.toLocaleString("en-US")}`;
  // runs after the emailed code is confirmed (D20)
  const apply = async (token: string) => {
    try {
      const r = await tradingApi<{ from: number; leverage: number }>(`accounts/${a.login}/leverage`, { body: { leverage: lev, stepup_token: token } });
      toast.success(t("accountDetail.leverage.changed"), { description: `#${a.login}: ${fmt(r.from)} → ${fmt(r.leverage)}` });
      onChanged();
    } catch (e) {
      errorToast(t("accountDetail.leverage.changeError"), e);
    }
  };
  return (
    <Card>
      <CardHeader title={t("accountDetail.leverage.title")} subtitle={t("accountDetail.leverage.available", { group: a.groupName, list: a.leverages.map((l) => `1:${l.toLocaleString("en-US")}`).join(" · ") })} action={<Chip tone="ember">{t("accountDetail.leverage.current", { value: `1:${a.leverage.toLocaleString("en-US")}` })}</Chip>} />
      <div className="px-4 pb-6 pt-4 sm:px-6">
        {locked && (
          <div className="mb-4 flex items-start gap-3 rounded-[14px] border border-warn/25 bg-warn-soft px-4 py-3 text-[13px]">
            <Lock className="mt-0.5 size-4 shrink-0 text-warn" />
            <div>
              <div className="font-medium text-warn">{t("accountDetail.leverage.lockedTitle")}</div>
              <div className="mt-0.5 text-fg-2">
                {t("accountDetail.leverage.lockedTextTrader", { count: a.positions })}
              </div>
            </div>
          </div>
        )}
        <div className={cn("flex flex-wrap gap-2", locked && "pointer-events-none opacity-45")}>
          {a.leverages.map((l) => (
            <button
              key={l}
              type="button"
              disabled={locked}
              aria-pressed={lev === l}
              onClick={() => setLev(l)}
              className={cn("k-num h-10 min-w-20 rounded-full border px-4 text-[13.5px] font-semibold transition-colors", lev === l ? "border-ember/60 bg-ember-soft text-ember" : "border-line bg-surface-2 text-fg-2 hover:text-fg")}
            >
              1:{l.toLocaleString("en-US")}
            </button>
          ))}
        </div>
        <div className="mt-4 flex items-center justify-between gap-3">
          <span className="text-[12.5px] text-fg-3">{t("accountDetail.leverage.hint")}</span>
          <Button size="sm" variant="ember" disabled={locked || lev === a.leverage || confirming} onClick={() => setConfirming(true)}>
            {t("common.apply")}
          </Button>
        </div>
      </div>
      {confirming && (
        <StepUpDialog
          open={confirming}
          onOpenChange={setConfirming}
          action="leverage"
          target={String(a.login)}
          title={t("accountDetail.leverage.confirmTitle")}
          description={`#${a.login}: ${fmt(a.leverage)} → ${fmt(lev)}`}
          what={t("accountDetail.leverage.stepUpWhat", { login: a.login, value: fmt(lev) })}
          confirmLabel={t("accountDetail.leverage.confirmApply")}
          onConfirmed={apply}
        />
      )}
    </Card>
  );
}

function DemoFundsCard({ a, onChanged }: { a: EngineAccount; onChanged: () => void }) {
  const t = useT();
  const cur = curOf(a);
  const { busy, run } = useRefill(a, onChanged);
  const left = refillsLeft(a);
  const target = demoTarget(a) ?? 0;
  const full = a.balance >= target;
  if (!a.demo) return null;
  return (
    <Card>
      <CardHeader title={t("accountDetail.demoFunds.title")} subtitle={t("accountDetail.demoFunds.subtitle", { amount: `${cur}${target.toLocaleString("en-US")}` })} />
      <div className="px-6 pb-6 pt-3">
        <Money value={a.balance} currency={cur} countUp={false} className="block text-[28px] font-semibold" />
        <div className="mt-5 flex items-end justify-between gap-3">
          <div className="text-[12.5px] text-fg-2">
            <Trans k="accountDetail.demoFunds.refillsLeft" vars={{ left, total: a.demo.refillsPerDay }} tags={{ n: (c) => <span className="k-num font-semibold text-fg">{c}</span> }} />
            <div className="mt-1.5 flex gap-1">
              {Array.from({ length: a.demo.refillsPerDay }, (_, i) => (
                <span key={i} className={cn("h-1.5 w-8 rounded-full", i < left ? "bg-gold" : "bg-surface-3")} />
              ))}
            </div>
          </div>
          <Button variant="gold" disabled={busy || left === 0 || full || a.status === "expired"} onClick={run}>
            <RefreshCcw /> {t("accountDetail.header.refill")}
          </Button>
        </div>
        <p className="mt-4 text-[12px] text-fg-3">
          {full ? `${t("accountDetail.demoFunds.full")} ` : ""}
          {t("accountDetail.demoFunds.resetNote", { days: a.demo.expiryDays })}
        </p>
      </div>
    </Card>
  );
}

export function SettingsPanel({ a, onChanged }: { a: EngineAccount; onChanged: () => void }) {
  const t = useT();
  const st = STATUS_LABEL[a.status];
  return (
    <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
      <div className="space-y-4 xl:col-span-7">
        {/* leverage is a CFD setting: an Options account has none to change (CFD / Options account split) */}
        {a.product !== "options" && (
          <Reveal>
            <LeverageCard a={a} onChanged={onChanged} />
          </Reveal>
        )}
      </div>
      <div className="space-y-4 xl:col-span-5">
        {a.type === "demo" && (
          <Reveal>
            <DemoFundsCard a={a} onChanged={onChanged} />
          </Reveal>
        )}
        <Reveal delay={0.05}>
          <Card>
            <CardHeader title={t("accountDetail.details.title")} />
            <div className="px-6 pb-4 pt-1">
              <KeyValue
                rows={[
                  [t("accountDetail.info.login"), <span key="l" className="font-mono">{a.login}</span>],
                  [t("common.type"), `${a.type === "live" ? t("common.live") : t("common.demo")} · ${a.groupName}`],
                  [t("accountDetail.info.positionMode"), modeLabel(a.mode)],
                  [t("common.currency"), a.cent ? t("accountDetail.info.uscCents") : a.currency],
                  [t("accountDetail.info.marginCallStopOut"), `${a.marginCallLevel}% / ${a.stopOutLevel}%`],
                  [t("common.status"), <Chip key="s" size="sm" tone={st.tone}>{st.label}</Chip>],
                  [t("accountDetail.info.opened"), fmtDate(a.createdAt)],
                ]}
              />
            </div>
          </Card>
        </Reveal>
      </div>
    </div>
  );
}
