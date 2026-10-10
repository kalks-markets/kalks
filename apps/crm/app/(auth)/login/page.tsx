"use client";

import * as React from "react";
import Link from "next/link";
import { Eye, EyeOff, Lock, Mail, ArrowRight, ShieldCheck } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { toast } from "sonner";
import { Button, Field, Input } from "@/components/kit";
import { IS_DEMO } from "@kalks/mock/mode";
import { useT, Trans } from "@kalks/i18n/react";
import { DemoEntry, DevCodeHint, FormError, GOOGLE_LOGIN, GoogleButton, useGoogleError, OrDivider, OtpInput, ResendLink } from "@/components/auth";
import { authPost, nextPath, type ApiError, type OtpChallenge } from "@/lib/auth-client";

export default function LoginPage() {
  const t = useT();
  const [show, setShow] = React.useState(false);
  const [step, setStep] = React.useState<"creds" | "otp">("creds");
  const [loading, setLoading] = React.useState(false);
  const [email, setEmail] = React.useState("");
  const [password, setPassword] = React.useState("");
  const [err, setErr] = React.useState<ApiError | null>(null);
  const googleErr = useGoogleError();
  const [otp, setOtp] = React.useState<OtpChallenge | null>(null);
  const [otpKey, setOtpKey] = React.useState(0);
  const [code, setCode] = React.useState("");

  const done = () => window.location.assign(nextPath());

  async function signIn(e: React.FormEvent) {
    e.preventDefault();
    setErr(null);
    setLoading(true);
    const r = await authPost<{ status: "ok" } | OtpChallenge>("login", { email, password });
    setLoading(false);
    if (!r.ok) {
      setErr(r.error);
      if (r.error.code === "invalid_credentials") setPassword("");
      return;
    }
    if (r.data.status === "ok") return done();
    setOtp(r.data);
    setCode("");
    setOtpKey((k) => k + 1);
    setStep("otp");
  }

  async function verify(value = code) {
    if (!otp || value.length !== 6) return;
    setErr(null);
    setLoading(true);
    const r = await authPost("verify-email", { challenge: otp.challenge, code: value });
    if (r.ok) return done();
    setLoading(false);
    setErr(r.error);
    setCode("");
    setOtpKey((k) => k + 1);
  }

  const fieldErr = (f: string) => (err?.field === f ? err.message : undefined);
  const formErr = err && !err.field ? err.message : null;

  return (
    <AnimatePresence mode="wait">
      {step === "creds" ? (
        <motion.div key="creds" initial={{ opacity: 0, x: 10 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -10 }}>
          <h1 className="text-3xl font-medium tracking-tight sm:text-4xl">{t("auth.login.title")}</h1>
          <p className="mt-2 text-[14.5px] text-fg-2">{t("auth.login.subtitle")}</p>
          {IS_DEMO && <DemoEntry />}
          {GOOGLE_LOGIN && (
            <>
              <div className="mt-8 space-y-4">
                <FormError>{googleErr}</FormError>
                <GoogleButton mode="login" />
              </div>
              <OrDivider />
            </>
          )}
          <form method="post" className={GOOGLE_LOGIN ? "space-y-4" : "mt-8 space-y-4"} onSubmit={signIn} noValidate>
            <FormError>{formErr}</FormError>
            <Field label={t("auth.field.emailOrViewer")} error={fieldErr("email")}>
              <Input leading={<Mail />} type="text" inputMode="email" autoCapitalize="none" spellCheck={false} name="email" autoComplete="username" value={email} onChange={(e) => setEmail(e.target.value)} placeholder={t("auth.placeholder.email")} required />
            </Field>
            <Field label={t("auth.field.password")} error={fieldErr("password")} hint={<Link href="/forgot" className="text-ember hover:underline">{t("auth.login.forgot")}</Link>}>
              <Input
                leading={<Lock />}
                type={show ? "text" : "password"}
                name="password"
                autoComplete="current-password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                required
                trailing={
                  <button type="button" onClick={() => setShow((s) => !s)} className="hover:text-fg" aria-label={t("auth.togglePassword")}>
                    {show ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
                  </button>
                }
              />
            </Field>
            <Button type="submit" variant="ember" size="xl" className="w-full" shimmer disabled={loading}>
              {loading ? t("auth.login.signingIn") : t("auth.login.signIn")} <ArrowRight className="rtl:-scale-x-100" />
            </Button>
          </form>
          <p className="mt-6 text-center text-[13.5px] text-fg-3">
            <Trans k="auth.login.newToKalks" tags={{ link: (c) => <Link href="/register" className="font-medium text-fg hover:text-ember">{c}</Link> }} />
          </p>
        </motion.div>
      ) : (
        <motion.div key="otp" initial={{ opacity: 0, x: 10 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -10 }}>
          <span className="grid size-12 place-items-center rounded-2xl border border-ember/30 bg-ember-soft text-ember">
            <ShieldCheck className="size-6" />
          </span>
          <h1 className="mt-5 text-3xl font-medium tracking-tight sm:text-4xl">{otp?.purpose === "verify_email" ? t("auth.login.verifyEmailTitle") : t("auth.login.verifyDeviceTitle")}</h1>
          <p className="mt-2 text-[14.5px] text-fg-2">
            {otp?.purpose === "verify_email" ? t("auth.login.emailNotVerified") : t("auth.login.newDevice")}{" "}
            <Trans k="auth.login.codeSent" vars={{ email: otp?.email_masked }} tags={{ b: (c) => <span className="text-fg">{c}</span> }} />
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
            {loading ? t("auth.otp.verifying") : t("auth.login.verifyContinue")}
          </Button>
          <div className="mt-5 flex items-center justify-between text-[13px] text-fg-3">
            <button
              onClick={() => {
                setErr(null);
                setStep("creds");
              }}
              className="hover:text-fg"
            >
              {t("auth.login.back")}
            </button>
            <span>
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
            </span>
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
