"use client";

import * as React from "react";
import { ArrowRight } from "lucide-react";
import { Button, cn, useBrand } from "@/components/kit";
import { useT, Trans } from "@kalks/i18n/react";

/** "Sign in with Google" is shown only once Google OAuth is configured (NEXT_PUBLIC_GOOGLE_LOGIN=1). */
export const GOOGLE_LOGIN = process.env.NEXT_PUBLIC_GOOGLE_LOGIN === "1";

/** Friendly messages for /login?google_error=... and /register?google_error=... (set by /api/auth/google/callback). Text: auth.google.error.<code>. */
export const GOOGLE_ERRORS = new Set(["cancelled", "expired", "unverified", "conflict", "disabled", "suspended", "rate_limited", "unavailable", "failed"]);

/** Reads ?google_error= once on mount (client only, so the server render stays identical). */
export function useGoogleError(): string | null {
  const t = useT();
  const [code, setCode] = React.useState<string | null>(null);
  React.useEffect(() => {
    const c = new URLSearchParams(window.location.search).get("google_error");
    if (c) setCode(GOOGLE_ERRORS.has(c) ? c : "failed");
  }, []);
  return code ? t.dyn(`auth.google.error.${code}`) : null;
}

/** Google "G" mark (brand colours are Google's, required for the sign-in button). */
export function GoogleMark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 48 48" className={className} aria-hidden="true">
      <path fill="#FFC107" d="M43.6 20.5H42V20H24v8h11.3C33.7 32.7 29.2 36 24 36c-6.6 0-12-5.4-12-12s5.4-12 12-12c3.1 0 5.9 1.2 8 3.1l5.7-5.7C34.1 6.1 29.3 4 24 4 12.9 4 4 12.9 4 24s8.9 20 20 20 20-8.9 20-20c0-1.2-.1-2.3-.4-3.5z" />
      <path fill="#FF3D00" d="M6.3 14.7l6.6 4.8C14.7 15.1 19 12 24 12c3.1 0 5.9 1.2 8 3.1l5.7-5.7C34.1 6.1 29.3 4 24 4 16.3 4 9.7 8.3 6.3 14.7z" />
      <path fill="#4CAF50" d="M24 44c5.2 0 9.9-2 13.4-5.2l-6.2-5.2C29.2 35.1 26.7 36 24 36c-5.2 0-9.6-3.3-11.3-7.9l-6.5 5C9.5 39.6 16.2 44 24 44z" />
      <path fill="#1976D2" d="M43.6 20.5H42V20H24v8h11.3c-.8 2.2-2.2 4.2-4.1 5.6l6.2 5.2C37 39.2 44 34 44 24c0-1.2-.1-2.3-.4-3.5z" />
    </svg>
  );
}

/**
 * Starts the Google OAuth flow (/api/auth/google/start). Carries ?next= (sign-in) and ?ref= (sign-up)
 * from the current page so they survive the round trip to Google.
 */
export function GoogleButton({ label, mode = "login" }: { label?: string; mode?: "login" | "register" }) {
  const t = useT();
  const [href, setHref] = React.useState(`/api/auth/google/start?mode=${mode}`);
  const [busy, setBusy] = React.useState(false);
  React.useEffect(() => {
    const sp = new URLSearchParams(window.location.search);
    const q = new URLSearchParams({ mode });
    const next = sp.get("next");
    const ref = sp.get("ref");
    if (next) q.set("next", next);
    if (ref) q.set("ref", ref);
    setHref(`/api/auth/google/start?${q.toString()}`);
    // back/forward cache: re-enable the button when the page is restored
    const reset = () => setBusy(false);
    window.addEventListener("pageshow", reset);
    return () => window.removeEventListener("pageshow", reset);
  }, [mode]);
  return (
    <a
      href={href}
      onClick={() => setBusy(true)}
      aria-disabled={busy}
      className={cn(
        "flex h-12 w-full items-center justify-center gap-3 rounded-full border border-line bg-surface-2 text-[14px] font-medium text-fg shadow-[inset_0_1px_0_var(--k-border-top)] transition-colors hover:bg-surface-3",
        busy && "pointer-events-none opacity-70",
      )}
    >
      <GoogleMark className="size-5" />
      {busy ? t("auth.google.opening") : (label ?? t("auth.google.continue"))}
    </a>
  );
}

export function OrDivider() {
  const t = useT();
  return (
    <div className="my-6 flex items-center gap-3 text-[12px] text-fg-3">
      <span className="h-px flex-1 bg-line" />
      {t("auth.google.orWithEmail")}
      <span className="h-px flex-1 bg-line" />
    </div>
  );
}

/** 6-digit OTP input with auto-advance and paste support. */
export function OtpInput({ length = 6, onComplete }: { length?: number; onComplete?: (code: string) => void }) {
  const t = useT();
  const [vals, setVals] = React.useState<string[]>(Array(length).fill(""));
  const refs = React.useRef<(HTMLInputElement | null)[]>([]);
  function set(i: number, v: string) {
    const digits = v.replace(/\D/g, "");
    if (digits.length > 1) {
      const next = [...vals];
      digits.split("").slice(0, length - i).forEach((d, k) => (next[i + k] = d));
      setVals(next);
      refs.current[Math.min(length - 1, i + digits.length)]?.focus();
      if (next.every(Boolean)) onComplete?.(next.join(""));
      return;
    }
    const next = [...vals];
    next[i] = digits;
    setVals(next);
    if (digits && i < length - 1) refs.current[i + 1]?.focus();
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
          // the code step opens with the cursor in the first box, so the client can type or paste at once
          autoFocus={i === 0}
          aria-label={t("auth.otp.digit", { n: i + 1, total: length })}
          inputMode="numeric"
          autoComplete="one-time-code"
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

export function PasswordStrength({ value }: { value: string }) {
  const checks = [value.length >= 8, /[A-Z]/.test(value), /[0-9]/.test(value), /[^A-Za-z0-9]/.test(value)];
  const score = checks.filter(Boolean).length;
  const t = useT();
  const label = [t("auth.strength.tooWeak"), t("auth.strength.weak"), t("auth.strength.fair"), t("auth.strength.good"), t("auth.strength.strong")][score];
  return (
    <div className="mt-2">
      <div className="flex gap-1.5">
        {[0, 1, 2, 3].map((i) => (
          <span key={i} className={cn("h-1 flex-1 rounded-full transition-colors", i < score ? (score < 2 ? "bg-down" : score < 4 ? "bg-warn" : "bg-up") : "bg-surface-3")} />
        ))}
      </div>
      <div className="mt-1.5 flex justify-between text-[11.5px] text-fg-3">
        <span>{t("auth.strength.rule")}</span>
        <span className={score >= 4 ? "text-up" : ""}>{value ? label : ""}</span>
      </div>
    </div>
  );
}

/** Inline error banner for auth forms. */
export function FormError({ children }: { children?: React.ReactNode }) {
  if (!children) return null;
  return (
    <div role="alert" className="rounded-[14px] border border-down/25 bg-down-soft px-4 py-3 text-[13px] text-down">
      {children}
    </div>
  );
}

/** Development only: SMTP isn't configured yet, so the gateway hands back the code it logged. */
export function DevCodeHint({ code }: { code?: string }) {
  if (!code) return null;
  return (
    <p className="mt-4 rounded-[14px] border border-dashed border-line px-4 py-2.5 text-[12px] text-fg-3">
      <Trans k="auth.otp.devHint" vars={{ code }} tags={{ code: (c) => <span dir="ltr" className="k-num font-mono text-fg">{c}</span> }} />
    </p>
  );
}

/** "Resend in 0:30" countdown that turns into a resend link. */
export function ResendLink({ seconds, onResend }: { seconds: number; onResend: () => Promise<number | void> }) {
  const t = useT();
  const [left, setLeft] = React.useState(seconds);
  const [busy, setBusy] = React.useState(false);
  React.useEffect(() => {
    if (left <= 0) return;
    const id = setTimeout(() => setLeft((s) => s - 1), 1000);
    return () => clearTimeout(id);
  }, [left]);
  return (
    <button
      type="button"
      disabled={left > 0 || busy}
      onClick={async () => {
        setBusy(true);
        const wait = await onResend();
        setBusy(false);
        setLeft(typeof wait === "number" ? wait : seconds);
      }}
      className="text-ember hover:underline disabled:text-fg-3 disabled:no-underline"
    >
      {left > 0 ? t("auth.otp.resendIn", { seconds: String(left).padStart(2, "0") }) : busy ? t("auth.otp.sending") : t("auth.otp.resendCode")}
    </button>
  );
}

/** Demo builds only: skip sign-in and browse the Client Area as the sample client. */
export function DemoEntry() {
  const t = useT();
  return (
    <div className="mt-8 rounded-[18px] border border-ember/30 bg-ember-soft px-5 py-4">
      <div className="text-[14px] font-medium text-fg">{t("auth.demo.title")}</div>
      <p className="mt-1 text-[13px] text-fg-2">{t("auth.demo.body")}</p>
      <a href="/" className="mt-3 block">
        <Button type="button" variant="ember" size="lg" className="w-full">
          {t("auth.demo.enter")} <ArrowRight className="rtl:-scale-x-100" />
        </Button>
      </a>
    </div>
  );
}
