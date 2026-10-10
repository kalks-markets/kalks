"use client";

import * as React from "react";
import Link from "next/link";
import { ArrowRight, CalendarDays, Gift, Lock, Mail, UserRound, Eye, EyeOff } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { toast } from "sonner";
import { Button, Field, Input, Stepper, Flag, Icon3D, Illustration } from "@/components/kit";
import { IS_DEMO } from "@kalks/mock/mode";
import { useT, Trans } from "@kalks/i18n/react";
import { DemoEntry, DevCodeHint, FormError, GOOGLE_LOGIN, GoogleButton, useGoogleError, OrDivider, OtpInput, PasswordStrength, ResendLink } from "@/components/auth";
import { authPost, type ApiError, type OtpChallenge } from "@/lib/auth-client";
import { COUNTRIES, maxDob } from "@/lib/countries";

type Form = { first_name: string; last_name: string; email: string; country: string; phone: string; date_of_birth: string; referral_code: string; password: string };


export default function RegisterPage() {
  const t = useT();
  const [step, setStep] = React.useState(0);
  const [form, setForm] = React.useState<Form>({ first_name: "", last_name: "", email: "", country: "in", phone: "", date_of_birth: "", referral_code: "", password: "" });
  const [agree, setAgree] = React.useState(false);
  const [marketing, setMarketing] = React.useState(true);
  const [show, setShow] = React.useState(false);
  const [loading, setLoading] = React.useState(false);
  const [err, setErr] = React.useState<ApiError | null>(null);
  const googleErr = useGoogleError();
  const [otp, setOtp] = React.useState<OtpChallenge | null>(null);
  const [otpKey, setOtpKey] = React.useState(0);
  const [code, setCode] = React.useState("");
  const [dobMax, setDobMax] = React.useState<string>();
  const dial = COUNTRIES.find((c) => c[0] === form.country)?.[2] ?? "+91";

  React.useEffect(() => {
    setDobMax(maxDob());
    // partner link: ?ref= on the page, else the referral cookie set when the visitor arrived (IB programme)
    const cookieRef = document.cookie.split("; ").find((c) => c.startsWith("kalks_ref="))?.slice(10);
    const ref = new URLSearchParams(window.location.search).get("ref") ?? (cookieRef ? decodeURIComponent(cookieRef).split(":")[0] : null);
    if (ref) setForm((f) => ({ ...f, referral_code: ref.slice(0, 24) }));
  }, []);

  const set = (k: keyof Form) => (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) => setForm((f) => ({ ...f, [k]: e.target.value }));
  const fieldErr = (f: string) => (err?.field === f ? err.message : undefined);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setErr(null);
    setLoading(true);
    const r = await authPost<OtpChallenge>("register", { ...form, phone_dial: dial, referral_code: form.referral_code || null, accept_terms: agree, marketing_consent: marketing });
    setLoading(false);
    if (!r.ok) return setErr(r.error);
    setOtp(r.data);
    setOtpKey((k) => k + 1);
    setStep(1);
  }

  async function verify(value = code) {
    if (!otp || value.length !== 6) return;
    setErr(null);
    setLoading(true);
    const r = await authPost("verify-email", { challenge: otp.challenge, code: value });
    setLoading(false);
    if (r.ok) return setStep(2);
    setErr(r.error);
    setCode("");
    setOtpKey((k) => k + 1);
  }

  return (
    <div>
      <Stepper steps={[t("auth.register.stepDetails"), t("auth.register.stepVerify"), t("auth.register.stepDone")]} current={step} className="mb-8" />
      <AnimatePresence mode="wait">
        {step === 0 && (
          <motion.div key="s0" initial={{ opacity: 0, x: 10 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -10 }}>
            <h1 className="text-3xl font-medium tracking-tight sm:text-4xl">{t("auth.register.title")}</h1>
            <p className="mt-2 text-[14px] text-fg-2">{IS_DEMO ? t("auth.register.subtitleDemo") : t("auth.register.subtitle")}</p>
            {IS_DEMO && <DemoEntry />}
            {GOOGLE_LOGIN && (
              <>
                <div className="mt-6 space-y-4">
                  <FormError>{googleErr}</FormError>
                  <GoogleButton mode="register" label={t("auth.google.signUp")} />
                </div>
                <OrDivider />
              </>
            )}
            <form method="post" className={GOOGLE_LOGIN ? "space-y-3.5" : "mt-6 space-y-3.5"} onSubmit={submit} noValidate>
              <FormError>{err && !err.field ? err.message : null}</FormError>
              <div className="grid grid-cols-2 gap-3">
                <Field label={t("auth.field.firstName")} error={fieldErr("first_name")}>
                  <Input leading={<UserRound />} name="given-name" autoComplete="given-name" value={form.first_name} onChange={set("first_name")} placeholder="Arjun" required />
                </Field>
                <Field label={t("auth.field.lastName")} error={fieldErr("last_name")}>
                  <Input name="family-name" autoComplete="family-name" value={form.last_name} onChange={set("last_name")} placeholder="Mehta" required />
                </Field>
              </div>
              <Field label={t("auth.field.email")} error={fieldErr("email")}>
                <Input leading={<Mail />} type="email" name="email" autoComplete="email" value={form.email} onChange={set("email")} placeholder={t("auth.placeholder.email")} required />
              </Field>
              {err?.code === "email_taken" && (
                <p className="-mt-1 text-[12.5px] text-fg-3">
                  <Trans
                    k="auth.register.emailTaken"
                    tags={{
                      signin: (c) => <Link href="/login" className="text-ember hover:underline">{c}</Link>,
                      reset: (c) => <Link href="/forgot" className="text-ember hover:underline">{c}</Link>,
                    }}
                  />
                </p>
              )}
              <div className="grid grid-cols-[1fr_1.2fr] gap-3">
                <Field label={t("auth.field.country")} error={fieldErr("country")}>
                  <div className="relative">
                    <select value={form.country} onChange={set("country")} aria-label={t("auth.field.country")} className="h-11 w-full appearance-none rounded-[14px] border border-line bg-surface-2 ps-10 pe-3 text-sm outline-none focus:border-ember/50">
                      {COUNTRIES.map(([c, n]) => (
                        <option key={c} value={c}>
                          {n}
                        </option>
                      ))}
                    </select>
                    <Flag country={form.country} className="pointer-events-none absolute start-3 top-1/2 size-5 -translate-y-1/2" />
                  </div>
                </Field>
                <Field label={t("auth.field.phone")} error={fieldErr("phone")}>
                  <Input leading={<span className="k-num text-[13px] text-fg-2">{dial}</span>} inputMode="tel" dir="ltr" name="tel-national" autoComplete="tel-national" value={form.phone} onChange={set("phone")} placeholder="98201 44721" required />
                </Field>
              </div>
              <div className="grid grid-cols-2 gap-3">
                <Field label={t("auth.field.dateOfBirth")} error={fieldErr("date_of_birth")}>
                  <Input leading={<CalendarDays />} type="date" name="bday" autoComplete="bday" max={dobMax} value={form.date_of_birth} onChange={set("date_of_birth")} required />
                </Field>
                <Field label={t("auth.field.referralCode")} hint={t("auth.field.optionalHint")} error={fieldErr("referral_code")}>
                  <Input leading={<Gift />} name="referral" value={form.referral_code} onChange={set("referral_code")} placeholder="ABC1234" />
                </Field>
              </div>
              <Field label={t("auth.field.password")} error={fieldErr("password")}>
                <Input
                  leading={<Lock />}
                  type={show ? "text" : "password"}
                  name="new-password"
                  autoComplete="new-password"
                  value={form.password}
                  onChange={set("password")}
                  placeholder={t("auth.placeholder.createPassword")}
                  required
                  trailing={
                    <button type="button" onClick={() => setShow((s) => !s)} className="hover:text-fg" aria-label={t("auth.togglePassword")}>
                      {show ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
                    </button>
                  }
                />
                <PasswordStrength value={form.password} />
              </Field>
              <label className="flex items-start gap-3 pt-1 text-[12.5px] leading-relaxed text-fg-2">
                <input type="checkbox" name="accept" checked={agree} onChange={(e) => setAgree(e.target.checked)} className="mt-0.5 size-4 accent-[var(--k-ember)]" />
                <span>
                  <Trans
                    k="auth.register.terms"
                    tags={{
                      agreement: (c) => <a className="text-fg underline-offset-2 hover:underline">{c}</a>,
                      risk: (c) => <a className="text-fg hover:underline">{c}</a>,
                      privacy: (c) => <a className="text-fg hover:underline">{c}</a>,
                    }}
                  />
                </span>
              </label>
              {fieldErr("accept_terms") && <p className="text-xs text-down">{fieldErr("accept_terms")}</p>}
              <label className="flex items-start gap-3 text-[12.5px] leading-relaxed text-fg-2">
                <input type="checkbox" name="marketing" checked={marketing} onChange={(e) => setMarketing(e.target.checked)} className="mt-0.5 size-4 accent-[var(--k-ember)]" data-testid="register-marketing" />
                <span>{t.dyn("auth.register.marketing", "Email me trading tips, product news and offers. Unsubscribe any time.")}</span>
              </label>
              <Button type="submit" variant="ember" size="xl" className="w-full" disabled={!agree || loading} shimmer>
                {loading ? t("auth.register.creating") : t("auth.register.create")} <ArrowRight className="rtl:-scale-x-100" />
              </Button>
            </form>
            <p className="mt-5 text-center text-[13.5px] text-fg-3">
              <Trans k="auth.register.haveAccount" tags={{ link: (c) => <Link href="/login" className="font-medium text-fg hover:text-ember">{c}</Link> }} />
            </p>
          </motion.div>
        )}
        {step === 1 && (
          <motion.div key="s1" initial={{ opacity: 0, x: 10 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -10 }}>
            <Icon3D name="bell" size={64} />
            <h1 className="mt-4 text-3xl font-medium tracking-tight sm:text-4xl">{t("auth.register.checkInbox")}</h1>
            <p className="mt-2 text-[14px] text-fg-2">
              <Trans k="auth.register.enterCode" vars={{ email: otp?.email_masked }} tags={{ b: (c) => <span className="text-fg">{c}</span> }} />
            </p>
            <div className="mt-8 space-y-4">
              <FormError>{err?.message}</FormError>
              <OtpInput
                key={otpKey}
                onComplete={(c) => {
                  setCode(c);
                  void verify(c);
                }}
              />
            </div>
            <DevCodeHint code={otp?.dev_code} />
            <Button variant="ember" size="xl" className="mt-6 w-full" disabled={loading || code.length !== 6} onClick={() => verify()}>
              {loading ? t("auth.otp.verifying") : t("auth.register.verifyEmail")}
            </Button>
            <div className="mt-5 text-end text-[13px] text-fg-3">
              {t("auth.otp.didntGetIt")}{" "}
              <ResendLink
                key={otp?.challenge}
                seconds={otp?.resend_in ?? 30}
                onResend={async () => {
                  if (!otp) return;
                  const r = await authPost<OtpChallenge>("resend", { challenge: otp.challenge });
                  if (!r.ok) {
                    setErr(r.error);
                    return r.error.retry_after;
                  }
                  setErr(null);
                  setOtp(r.data);
                  setOtpKey((k) => k + 1);
                  toast.success(t("auth.toast.newCodeSent"), { description: t("auth.toast.checkEmail", { email: r.data.email_masked }) });
                }}
              />
            </div>
          </motion.div>
        )}
        {step === 2 && (
          <motion.div key="s2" initial={{ opacity: 0, x: 10 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -10 }}>
            <Illustration name="welcome" width={150} maxHeight={176} priority />
            <h1 className="mt-6 text-3xl font-medium tracking-tight sm:text-4xl">{t("auth.register.welcome", { name: form.first_name.trim() })}</h1>
            <p className="mt-2 text-[14px] text-fg-2">{IS_DEMO ? t("auth.register.readyDemo") : t("auth.register.ready")}</p>
            <Button variant="ember" size="xl" className="mt-8 w-full" onClick={() => window.location.assign("/")}>
              {t("auth.register.openClientArea")} <ArrowRight className="rtl:-scale-x-100" />
            </Button>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
