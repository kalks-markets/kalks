"use client";

import * as React from "react";
import { AnimatePresence, motion } from "motion/react";
import { ArrowRight, Check, Eye, EyeOff, Lock, Mail, MailCheck, ShieldCheck } from "lucide-react";
import { toast } from "sonner";
import { Button, Field, Input, cn } from "@kalks/ui";
import { IS_DEMO } from "@kalks/mock/mode";

function OtpInput({ length = 6, onComplete }: { length?: number; onComplete?: (code: string) => void }) {
  const [vals, setVals] = React.useState<string[]>(Array(length).fill(""));
  const refs = React.useRef<(HTMLInputElement | null)[]>([]);
  React.useEffect(() => refs.current[0]?.focus(), []);
  function set(i: number, v: string) {
    const digits = v.replace(/\D/g, "");
    const next = [...vals];
    if (digits.length > 1) {
      digits.split("").slice(0, length - i).forEach((d, k) => (next[i + k] = d));
      refs.current[Math.min(length - 1, i + digits.length)]?.focus();
    } else {
      next[i] = digits;
      if (digits && i < length - 1) refs.current[i + 1]?.focus();
    }
    setVals(next);
    if (next.every(Boolean)) onComplete?.(next.join(""));
  }
  return (
    <div className="flex justify-between gap-2" dir="ltr">
      {vals.map((v, i) => (
        <input
          key={i}
          ref={(el) => {
            refs.current[i] = el;
          }}
          value={v}
          inputMode="numeric"
          autoComplete="one-time-code"
          aria-label={`Digit ${i + 1}`}
          onChange={(e) => set(i, e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Backspace" && !vals[i] && i > 0) refs.current[i - 1]?.focus();
          }}
          className={cn(
            "k-num h-14 w-full rounded-[14px] border bg-surface-2 text-center font-mono text-xl font-semibold outline-none transition-all focus:border-ember/60 focus:ring-4 focus:ring-ember/10",
            v ? "border-[var(--k-border-top)]" : "border-line",
          )}
        />
      ))}
    </div>
  );
}

type ApiError = { code: string; message: string; field?: string; retry_after?: number };
type Challenge = { status: "otp_required"; challenge: string; email_masked: string; expires_in: number; resend_in: number; dev_code?: string };

async function post<T>(action: string, body: unknown): Promise<{ ok: true; data: T } | { ok: false; error: ApiError }> {
  try {
    const res = await fetch(`/api/auth/${action}`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body), credentials: "same-origin" });
    const data = await res.json().catch(() => ({}));
    if (res.ok) return { ok: true, data: data as T };
    return { ok: false, error: data?.error ?? { code: "unknown", message: "Something went wrong. Please try again." } };
  } catch {
    return { ok: false, error: { code: "network", message: "Can't reach the Back Office. Check your connection." } };
  }
}

function nextPath(): string {
  const n = new URLSearchParams(window.location.search).get("next");
  return n && n.startsWith("/") && !n.startsWith("//") && !n.startsWith("/\\") && !n.startsWith("/api/") ? n : "/";
}

function FormError({ children }: { children?: React.ReactNode }) {
  if (!children) return null;
  return (
    <div role="alert" className="rounded-[14px] border border-down/25 bg-down-soft px-4 py-3 text-[13px] text-down">
      {children}
    </div>
  );
}

function AuditNotice() {
  return (
    <div className="flex items-center gap-3 rounded-[14px] border border-up/25 bg-up-soft px-4 py-3">
      <span className="grid size-7 shrink-0 place-items-center rounded-full bg-up text-white">
        <Check className="size-4" strokeWidth={2.6} />
      </span>
      <div className="min-w-0 text-[13px]">
        <div className="font-medium text-fg">Every sign-in is audited</div>
        <div className="text-[12px] text-fg-3">Emailed code at every sign-in · 12h sessions</div>
      </div>
    </div>
  );
}

/** Demo builds: no staff sign-in, one click into the mock showcase. */
function DemoEntry() {
  return (
    <div>
      <span className="inline-flex items-center gap-2 rounded-full border border-ember/30 bg-ember-soft px-3 py-1 text-[11px] font-semibold tracking-[0.14em] text-ember">
        <ShieldCheck className="size-3.5" /> DEMO
      </span>
      <h1 className="mt-5 text-[30px] font-medium tracking-[-0.02em]">Back Office demo</h1>
      <p className="mt-2 text-[14.5px] text-fg-2">Explore the full Kalks Back Office on sample data: dealing, risk, compliance, finance, partners and more.</p>
      <div className="mt-7 flex items-center gap-3 rounded-[14px] border border-line bg-surface-2 px-4 py-3 text-[13px] text-fg-2">
        <Lock className="size-4 shrink-0 text-fg-3" />
        No sign-in needed. Clients, balances and trades are sample data; changes you make stay in your browser.
      </div>
      <Button variant="ember" size="xl" className="mt-6 w-full" shimmer onClick={() => window.location.assign(nextPath())}>
        Enter demo <ArrowRight />
      </Button>
    </div>
  );
}

export default function AdminLoginPage() {
  return IS_DEMO ? <DemoEntry /> : <StaffLogin />;
}

function StaffLogin() {
  const [show, setShow] = React.useState(false);
  const [step, setStep] = React.useState<"creds" | "otp">("creds");
  const [loading, setLoading] = React.useState(false);
  const [left, setLeft] = React.useState(59);
  const [email, setEmail] = React.useState("");
  const [password, setPassword] = React.useState("");
  const [err, setErr] = React.useState<ApiError | null>(null);
  const [otp, setOtp] = React.useState<Challenge | null>(null);
  const [otpKey, setOtpKey] = React.useState(0);
  const [code, setCode] = React.useState("");

  React.useEffect(() => {
    if (step !== "otp") return;
    const t = setInterval(() => setLeft((s) => Math.max(0, s - 1)), 1000);
    return () => clearInterval(t);
  }, [step]);

  const enter = (name?: string) => {
    toast.success("Signed in to Back Office", { description: name ? `Welcome, ${name} · session 12h` : "Session bound to this device · 12h" });
    setTimeout(() => window.location.assign(nextPath()), 300);
  };

  async function signIn(e: React.FormEvent) {
    e.preventDefault();
    setErr(null);
    setLoading(true);
    const r = await post<{ status: "ok"; staff: { name: string } } | Challenge>("login", { email, password });
    setLoading(false);
    if (!r.ok) {
      setErr(r.error);
      if (r.error.code === "invalid_credentials") setPassword("");
      return;
    }
    if (r.data.status === "ok") return enter(r.data.staff.name);
    setOtp(r.data);
    setLeft(r.data.resend_in);
    setCode("");
    setOtpKey((k) => k + 1);
    setStep("otp");
  }

  async function verify(value = code) {
    if (!otp || value.length !== 6) return;
    setErr(null);
    setLoading(true);
    const r = await post<{ staff: { name: string } }>("verify-otp", { challenge: otp.challenge, code: value });
    if (r.ok) return enter(r.data.staff.name);
    setLoading(false);
    setErr(r.error);
    setCode("");
    setOtpKey((k) => k + 1);
  }

  const fieldErr = (f: string) => (err?.field === f ? err.message : undefined);

  return (
    <AnimatePresence mode="wait">
      {step === "creds" ? (
        <motion.div key="creds" initial={{ opacity: 0, x: 10 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -10 }}>
          <span className="inline-flex items-center gap-2 rounded-full border border-ember/30 bg-ember-soft px-3 py-1 text-[11px] font-semibold tracking-[0.14em] text-ember">
            <ShieldCheck className="size-3.5" /> STAFF SIGN-IN
          </span>
          <h1 className="mt-5 text-[30px] font-medium tracking-[-0.02em]">Back Office</h1>
          <p className="mt-2 text-[14.5px] text-fg-2">Sign in with your Kalks staff account.</p>
          <div className="mt-7">
            <AuditNotice />
          </div>
          <form method="post" className="mt-6 space-y-4" onSubmit={signIn} noValidate>
            <FormError>{err && !err.field ? err.message : null}</FormError>
            <Field label="Work email" error={fieldErr("email")}>
              <Input leading={<Mail />} type="email" name="email" autoComplete="username" value={email} onChange={(e) => setEmail(e.target.value)} placeholder="name@kalkstrade.com" required />
            </Field>
            <Field label="Password" error={fieldErr("password")} hint={<button type="button" onClick={() => toast("Ask your Super Admin to reset it", { description: "Staff password resets are done from Organisation › Staff." })} className="text-ember hover:underline">Forgot?</button>}>
              <Input
                leading={<Lock />}
                type={show ? "text" : "password"}
                name="password"
                autoComplete="current-password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                required
                trailing={
                  <button type="button" onClick={() => setShow((s) => !s)} className="hover:text-fg" aria-label="Toggle password">
                    {show ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
                  </button>
                }
              />
            </Field>
            <Button type="submit" variant="ember" size="xl" className="w-full" shimmer disabled={loading}>
              {loading ? "Checking credentials…" : "Continue"} <ArrowRight />
            </Button>
          </form>
          <div className="mt-6 flex items-center justify-center gap-4 text-[12px] text-fg-3">
            <span className="flex items-center gap-1.5">
              <span className="size-1.5 rounded-full bg-up" /> SSO disabled for this tenant
            </span>
            <span>·</span>
            <span>Need access? Ask your team lead</span>
          </div>
        </motion.div>
      ) : (
        <motion.div key="otp" initial={{ opacity: 0, x: 10 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -10 }}>
          <span className="grid size-12 place-items-center rounded-2xl border border-ember/30 bg-ember-soft text-ember">
            <MailCheck className="size-6" />
          </span>
          <h1 className="mt-5 text-[28px] font-medium tracking-[-0.02em]">Check your email</h1>
          <p className="mt-2 text-[14.5px] text-fg-2">
            To finish signing in, enter the 6-digit code we sent to <span className="text-fg">{otp?.email_masked}</span>. It expires in {Math.round((otp?.expires_in ?? 300) / 60)} minutes.
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
          {otp?.dev_code && (
            <p className="mt-4 rounded-[14px] border border-dashed border-line px-4 py-2.5 text-[12px] text-fg-3">
              Dev mode: email delivery isn&apos;t configured yet. Your code is <span className="k-num font-mono text-fg">{otp.dev_code}</span> (also in the gateway log).
            </p>
          )}
          <Button variant="ember" size="xl" className="mt-6 w-full" disabled={loading || code.length !== 6} onClick={() => verify()}>
            {loading ? "Verifying…" : "Verify & enter Back Office"}
          </Button>
          <div className="mt-5 flex items-center justify-between text-[13px] text-fg-3">
            <button
              onClick={() => {
                setErr(null);
                setStep("creds");
              }}
              className="hover:text-fg"
            >
              ← Back
            </button>
            <span>
              Didn&apos;t get it?{" "}
              <button
                disabled={left > 0}
                onClick={async () => {
                  if (!otp) return;
                  const r = await post<Challenge>("resend", { challenge: otp.challenge });
                  if (!r.ok) {
                    setErr(r.error);
                    setLeft(r.error.retry_after ?? 30);
                    return;
                  }
                  setErr(null);
                  setOtp(r.data);
                  setOtpKey((k) => k + 1);
                  setLeft(r.data.resend_in);
                  toast.success("New code sent");
                }}
                className="text-ember hover:underline disabled:text-fg-3 disabled:no-underline"
              >
                {left > 0 ? `Resend in 0:${String(left).padStart(2, "0")}` : "Resend code"}
              </button>
            </span>
          </div>
          <div className="mt-8">
            <AuditNotice />
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
