// Browser-side calls to this app's /api/auth route handlers (same origin, HttpOnly cookies).

import { tr } from "@kalks/i18n/react";

export type ApiError = { code: string; message: string; field?: string; retry_after?: number; attempts_left?: number };

export type OtpChallenge = {
  status: "otp_required";
  challenge: string;
  purpose: "verify_email" | "login" | "reset_password" | "confirm";
  email_masked: string;
  expires_in: number;
  resend_in: number;
  /** Step-up codes (purpose "confirm") only: the change being confirmed. */
  action?: string;
  target?: string;
  /** Development only: SMTP is not configured, so the gateway returns the code for the dev hint. */
  dev_code?: string;
};

/** Gateway validation texts (services/gateway/src/validate.rs and the auth flows) -> message keys. */
const MESSAGE_KEYS: Record<string, string> = {
  "Enter your email address.": "auth.apiError.emailRequired",
  "Email address is too long.": "auth.apiError.emailTooLong",
  "Enter a valid email address.": "auth.apiError.emailInvalid",
  "Use at least 8 characters.": "auth.apiError.pwMin",
  "Use at most 128 characters.": "auth.apiError.pwMax",
  "Add an uppercase letter.": "auth.apiError.pwUpper",
  "Add a lowercase letter.": "auth.apiError.pwLower",
  "Add a number.": "auth.apiError.pwNumber",
  "Add a symbol such as ! # @ or %.": "auth.apiError.pwSymbol",
  "Choose a country dial code.": "auth.apiError.dialCode",
  "Choose your country of residence.": "auth.apiError.country",
  "Enter a valid date of birth.": "auth.apiError.dob",
  "You must be at least 18 years old to open an account.": "auth.apiError.age",
  "Enter a valid phone number.": "auth.apiError.phoneInvalid",
  "Enter your phone number.": "auth.apiError.phoneRequired",
  "Name is too long.": "auth.apiError.nameTooLong",
  "Use letters only.": "auth.apiError.nameLetters",
  "Referral code looks wrong. Leave it empty if you don't have one.": "auth.apiError.referral",
  "This referral code doesn't exist. Check it or leave it empty.": "auth.apiError.referralUnknown",
  "Enter your password.": "auth.apiError.passwordRequired",
  "Please confirm you are over 18 and accept the terms.": "auth.apiError.terms",
  "This code was sent for a different change. Request a new code.": "auth.apiError.codeOtherChange",
  "Use the reset form for this code.": "auth.apiError.codeUseReset",
};

/**
 * The gateway answers in English; show its errors in the reader's language. Known codes and validation
 * texts map to catalog keys; anything unknown keeps the server's wording.
 */
export function localizeError(e: ApiError): ApiError {
  const n = e.attempts_left ?? 0;
  const secs = Math.max(1, Math.round(e.retry_after ?? 0));
  let message: string | undefined;
  switch (e.code) {
    case "invalid_credentials":
    case "account_disabled":
    case "account_suspended":
    case "email_taken":
    case "code_expired":
    case "unauthorized":
    case "internal":
    case "network":
    case "unknown":
      message = tr.dyn(`auth.apiError.${e.code}`, e.message);
      break;
    case "invalid_code":
      message = n > 0 ? tr.dyn("auth.apiError.invalidCode", e.message, { count: n }) : tr.dyn("auth.apiError.tooManyCodes", e.message);
      break;
    case "locked":
      {
        const minutes = Math.max(1, Math.ceil(secs / 60));
        message = tr.dyn("auth.apiError.locked", e.message, { count: minutes, minutes });
      }
      break;
    case "rate_limited":
      message = tr.dyn("auth.apiError.rateLimited", e.message, { seconds: secs });
      break;
    default: {
      const key = MESSAGE_KEYS[e.message];
      if (key) message = tr.dyn(key, e.message);
    }
  }
  return message ? { ...e, message } : e;
}

export type AuthResult<T> = { ok: true; data: T } | { ok: false; error: ApiError };

export async function authPost<T = Record<string, unknown>>(action: string, body: unknown = {}): Promise<AuthResult<T>> {
  try {
    const res = await fetch(`/api/auth/${action}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
      credentials: "same-origin",
    });
    const data = await res.json().catch(() => ({}));
    if (res.ok) return { ok: true, data: data as T };
    return { ok: false, error: localizeError((data?.error as ApiError) ?? { code: "unknown", message: "Something went wrong. Please try again." }) };
  } catch {
    return { ok: false, error: localizeError({ code: "network", message: "Can't reach Kalks. Check your connection and try again." }) };
  }
}

export async function authGet<T = Record<string, unknown>>(action: string): Promise<AuthResult<T>> {
  try {
    const res = await fetch(`/api/auth/${action}`, { credentials: "same-origin", cache: "no-store" });
    const data = await res.json().catch(() => ({}));
    if (res.ok) return { ok: true, data: data as T };
    return { ok: false, error: localizeError((data?.error as ApiError) ?? { code: "unknown", message: "Something went wrong. Please try again." }) };
  } catch {
    return { ok: false, error: localizeError({ code: "network", message: "Can't reach Kalks. Check your connection and try again." }) };
  }
}

/** Where to go after sign-in: the ?next= page if it is a safe same-app path, else the dashboard. */
export function nextPath(): string {
  if (typeof window === "undefined") return "/";
  const n = new URLSearchParams(window.location.search).get("next");
  return n && n.startsWith("/") && !n.startsWith("//") && !n.startsWith("/\\") && !n.startsWith("/api/") ? n : "/";
}
