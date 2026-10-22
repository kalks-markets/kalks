"use client";

/**
 * Live-mode building blocks: fetching from the Back Office BFF (/api/admin/*), formatting of real timestamps,
 * CSV export, pagination and loading / error states.
 */
import * as React from "react";
import { ChevronLeft, ChevronRight, RefreshCw } from "lucide-react";
import { toast } from "sonner";
import { Button, Chip, EmptyState, IconButton, Skeleton, cn, formatDateTime, type ChipTone } from "@kalks/ui";
import { readCached, writeCached } from "@kalks/ui/swr-cache";

export type ApiErr = { code: string; message: string; field?: string };

function expired() {
  const next = window.location.pathname + window.location.search;
  window.location.assign(`/api/auth/expired?next=${encodeURIComponent(next)}`);
}

/** GET a BFF endpoint. `url = null` skips the request. Re-fetches when `url` changes; `reload()` forces it.
 *  A page opened again starts from the last answer of this tab (dimmed as `loading`, like a poll) while it refetches
 *  (@kalks/ui/swr-cache: per signed-in staff member, cleared by any write). */
export function useApi<T>(url: string | null, opts: { refreshMs?: number } = {}) {
  const [data, setData] = React.useState<T | null>(() => (url ? (readCached<T>(url) ?? null) : null));
  const [error, setError] = React.useState<ApiErr | null>(null);
  const [loading, setLoading] = React.useState(!!url);
  const [tick, setTick] = React.useState(0);
  React.useEffect(() => {
    if (!url) return;
    let alive = true;
    setLoading(true);
    // a different URL (filter, page, record) starts from its own last answer when this tab has one
    const cached = readCached<T>(url);
    if (cached !== undefined) setData(cached);
    fetch(url, { cache: "no-store", credentials: "same-origin" })
      .then(async (r) => {
        const body = await r.json().catch(() => ({}));
        if (!alive) return;
        if (r.status === 401) return expired();
        if (!r.ok) {
          setError(body?.error ?? { code: "unknown", message: "Something went wrong." });
          return;
        }
        setError(null);
        setData(body as T);
        writeCached(url, body);
      })
      .catch(() => alive && setError({ code: "network", message: "Can't reach the Back Office server." }))
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
    };
  }, [url, tick]);
  React.useEffect(() => {
    if (!url || !opts.refreshMs) return;
    const t = setInterval(() => setTick((n) => n + 1), opts.refreshMs);
    return () => clearInterval(t);
  }, [url, opts.refreshMs]);
  const reload = React.useCallback(() => setTick((n) => n + 1), []);
  return { data, error, loading, reload };
}

/** POST/PUT/PATCH/DELETE JSON to a BFF endpoint. */
export async function sendJson<T>(url: string, body: unknown, method: "POST" | "PUT" | "PATCH" | "DELETE" = "POST"): Promise<{ ok: true; data: T } | { ok: false; error: ApiErr }> {
  try {
    const r = await fetch(url, { method, headers: { "content-type": "application/json" }, body: JSON.stringify(body), credentials: "same-origin" });
    const data = await r.json().catch(() => ({}));
    if (r.status === 401) {
      expired();
      return { ok: false, error: { code: "unauthorized", message: "Your session has ended." } };
    }
    if (r.ok) return { ok: true, data: data as T };
    return { ok: false, error: data?.error ?? { code: "unknown", message: "Something went wrong." } };
  } catch {
    return { ok: false, error: { code: "network", message: "Can't reach the Back Office server." } };
  }
}

export function qs(params: Record<string, string | number | boolean | null | undefined>) {
  const u = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) if (v !== undefined && v !== null && v !== "" && v !== "all") u.set(k, String(v));
  const s = u.toString();
  return s ? `?${s}` : "";
}

export function useDebounced<T>(value: T, ms = 300) {
  const [v, setV] = React.useState(value);
  React.useEffect(() => {
    const t = setTimeout(() => setV(value), ms);
    return () => clearTimeout(t);
  }, [value, ms]);
  return v;
}

/* ---------- formatting ---------- */

/** "28 Sep 2026, 14:27" in server time (GMT+3). */
export function when(iso: string | null | undefined, withSeconds = false) {
  if (!iso) return "—";
  return formatDateTime(iso, { day: "2-digit", month: "short", year: "numeric", hour: "2-digit", minute: "2-digit", ...(withSeconds ? { second: "2-digit" } : {}), hour12: false });
}

export function day(iso: string | null | undefined) {
  if (!iso) return "—";
  return formatDateTime(iso, { day: "2-digit", month: "short", year: "numeric" });
}

/** Relative time against the real clock; re-renders every 30s via useNow(). */
export function ago(iso: string | null | undefined, now: number) {
  if (!iso) return "never";
  const s = Math.round((now - Date.parse(iso)) / 1000);
  // `now` ticks every 30 s: something that just happened is never "in 1m"
  if (s < 0 && s > -90) return "just now";
  if (s < 0) {
    const f = -s;
    if (f < 3600) return `in ${Math.max(1, Math.round(f / 60))}m`;
    if (f < 86400) return `in ${Math.round(f / 3600)}h`;
    return `in ${Math.round(f / 86400)}d`;
  }
  if (s < 45) return "just now";
  if (s < 3600) return `${Math.round(s / 60)}m ago`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
  if (s < 86400 * 30) return `${Math.floor(s / 86400)}d ago`;
  return day(iso);
}

export function useNow(ms = 30_000) {
  const [now, setNow] = React.useState(() => Date.now());
  React.useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), ms);
    return () => clearInterval(t);
  }, [ms]);
  return now;
}

let regionNames: Intl.DisplayNames | null = null;
export function countryName(code: string) {
  try {
    regionNames ??= new Intl.DisplayNames(["en"], { type: "region" });
    return regionNames.of(code.toUpperCase()) ?? code.toUpperCase();
  } catch {
    return code.toUpperCase();
  }
}

/** "Chrome 131 · macOS" from a user-agent string. */
export function device(ua: string | null | undefined) {
  if (!ua) return "Unknown device";
  const os = /iPhone|iPad/.test(ua) ? "iOS" : /Android/.test(ua) ? "Android" : /Mac OS X|Macintosh/.test(ua) ? "macOS" : /Windows/.test(ua) ? "Windows" : /Linux/.test(ua) ? "Linux" : "";
  const m =
    ua.match(/Edg\/(\d+)/) ? ["Edge", ua.match(/Edg\/(\d+)/)![1]] :
    ua.match(/OPR\/(\d+)/) ? ["Opera", ua.match(/OPR\/(\d+)/)![1]] :
    ua.match(/Firefox\/(\d+)/) ? ["Firefox", ua.match(/Firefox\/(\d+)/)![1]] :
    ua.match(/HeadlessChrome\/(\d+)/) ? ["Headless Chrome", ua.match(/HeadlessChrome\/(\d+)/)![1]] :
    ua.match(/Chrome\/(\d+)/) ? ["Chrome", ua.match(/Chrome\/(\d+)/)![1]] :
    ua.match(/Version\/(\d+).*Safari/) ? ["Safari", ua.match(/Version\/(\d+)/)![1]] :
    ua.startsWith("node") || ua.startsWith("undici") ? ["Server", ""] :
    ua.startsWith("curl") ? ["curl", ""] : null;
  const browser = m ? `${m[0]}${m[1] ? ` ${m[1]}` : ""}` : ua.slice(0, 24);
  return os ? `${browser} · ${os}` : browser;
}

/** Tone for an audit action. */
export function actionTone(action: string): ChipTone {
  if (/failed|locked|rejected|blocked/.test(action)) return "down";
  if (/revoked|reset|logout/.test(action)) return "warn";
  if (/login|verified|register|seeded|approved/.test(action)) return "up";
  if (/more_info/.test(action)) return "warn";
  if (/^spreads\./.test(action)) return "ember";
  return "neutral";
}

/** Readable label for an audit action code. */
export function actionLabel(action: string) {
  const map: Record<string, string> = {
    "user.register": "Client registered",
    "user.login": "Client signed in",
    "user.login_failed": "Client sign-in failed",
    "user.locked": "Client locked out",
    "user.logout": "Client signed out",
    "user.email_verified": "Client email verified",
    "user.password_reset_requested": "Client password reset requested",
    "user.password_reset": "Client password reset",
    "staff.login": "Staff signed in",
    "staff.login_failed": "Staff sign-in failed",
    "staff.locked": "Staff locked out",
    "staff.logout": "Staff signed out",
    "staff.seeded": "Staff account created",
    "admin.session_revoked": "Session revoked",
    "admin.sessions_revoked": "Signed out everywhere by staff",
    "admin.client_request_processed": "Closure / export request updated",
    "user.login_challenge": "Client sign-in code sent (new device)",
    "user.session_revoked": "Client signed a device out",
    "user.sessions_revoked": "Client signed out other devices",
    "user.password_changed": "Client changed password",
    "user.stepup_requested": "Client requested a confirmation code",
    "user.stepup_verified": "Client confirmed a change by email code",
    "user.stepup_failed": "Client entered a wrong confirmation code",
    "user.stepup_consumed": "Confirmed change applied",
    "user.closure_requested": "Client requested account closure",
    "user.data_export_requested": "Client requested a data export",
    "user.request_cancelled": "Client cancelled a request",
    "user.data_exported": "Client downloaded a data export",
    "viewer.created": "View-only login created",
    "viewer.updated": "View-only login changed",
    "viewer.password_reset": "View-only login password reset",
    "viewer.revoked": "View-only login revoked",
    "viewer.login": "Viewer signed in",
    "viewer.logout": "Viewer signed out",
    "viewer.login_failed": "Viewer sign-in failed",
    "viewer.locked": "Viewer locked out",
    "viewer.login_blocked": "Viewer sign-in blocked",
    "viewer.page_view": "Viewer opened a page",
    "settings.client_idle_updated": "Client idle sign-out changed",
    "spreads.update": "Spread markup changed",
    "clients.exported": "Client list exported",
    "audit.exported": "Audit log exported",
    "share.created": "Trade share link created",
    "share.revoked": "Trade share link revoked",
    "kyc.started": "KYC started",
    "kyc.document_uploaded": "KYC document uploaded",
    "kyc.submitted": "KYC submitted",
    "kyc.resubmitted": "KYC documents resubmitted",
    "kyc.review_started": "KYC review started",
    "kyc.more_info_requested": "KYC: more information requested",
    "kyc.approved": "KYC approved",
    "kyc.rejected": "KYC rejected",
    "kyc.note_added": "KYC note added",
    "kyc.document_viewed": "KYC document viewed",
    "user.identity_corrected": "Client corrected name / date of birth",
    "client.restriction_set": "Restriction set",
    "client.restriction_lifted": "Restriction lifted",
    "client.restriction_expired": "Restriction expired",
    "client.impersonation_started": "Staff session as client started",
    "client.impersonation_ended": "Staff session as client ended",
    "client.impersonation_action": "Staff action as client",
    "client.impersonation_write_refused": "Staff change refused (read-only)",
    "client.impersonation_page_view": "Staff opened a page as client",
  };
  return map[action] ?? action;
}

/* ---------- CSV ---------- */

export function downloadCsv(name: string, header: string[], rows: (string | number | null | undefined)[][]) {
  const esc = (v: unknown) => {
    const s = String(v ?? "");
    return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
  };
  const text = [header.map(esc).join(","), ...rows.map((r) => r.map(esc).join(","))].join("\n");
  const url = URL.createObjectURL(new Blob([text], { type: "text/csv;charset=utf-8" }));
  const a = document.createElement("a");
  a.href = url;
  a.download = `${name}.csv`;
  a.click();
  URL.revokeObjectURL(url);
  toast.success(`${name}.csv downloaded`, { description: `${rows.length} rows` });
}

/* ---------- states ---------- */

export function ErrorState({ error, onRetry, className }: { error: ApiErr; onRetry?: () => void; className?: string }) {
  const forbidden = error.code === "forbidden";
  return (
    <EmptyState
      className={className}
      illustration={forbidden ? "locked" : "warning"}
      title={forbidden ? "Not available for your role" : "Couldn't load this"}
      text={forbidden ? "Ask a Super Admin if you need access." : error.message}
      action={
        !forbidden && onRetry ? (
          <Button variant="surface" size="sm" onClick={onRetry}>
            <RefreshCw /> Try again
          </Button>
        ) : undefined
      }
    />
  );
}

export function TableSkeleton({ rows = 6, className }: { rows?: number; className?: string }) {
  return (
    <div className={cn("space-y-2", className)}>
      <Skeleton className="h-11 w-full rounded-[14px]" />
      {Array.from({ length: rows }).map((_, i) => (
        <Skeleton key={i} className="h-12 w-full" />
      ))}
    </div>
  );
}

export function Pager({ page, perPage, total, onPage }: { page: number; perPage: number; total: number; onPage: (p: number) => void }) {
  const pages = Math.max(1, Math.ceil(total / perPage));
  if (total === 0) return null;
  return (
    <div className="mt-4 flex items-center justify-between text-[12.5px] text-fg-3">
      <span className="k-num">
        {(page - 1) * perPage + 1}–{Math.min(total, page * perPage)} of {total.toLocaleString("en-US")}
      </span>
      {pages > 1 && (
        <div className="flex items-center gap-1.5">
          <IconButton size="sm" disabled={page <= 1} onClick={() => onPage(page - 1)} aria-label="Previous page">
            <ChevronLeft />
          </IconButton>
          <span className="k-num px-2">
            {page} / {pages}
          </span>
          <IconButton size="sm" disabled={page >= pages} onClick={() => onPage(page + 1)} aria-label="Next page">
            <ChevronRight />
          </IconButton>
        </div>
      )}
    </div>
  );
}

export function KycChip({ status }: { status: string }) {
  const map: Record<string, { tone: ChipTone; label: string }> = {
    verified: { tone: "up", label: "KYC verified" },
    pending: { tone: "warn", label: "KYC pending" },
    rejected: { tone: "down", label: "KYC rejected" },
    unverified: { tone: "neutral", label: "No KYC" },
  };
  const s = map[status] ?? { tone: "neutral" as ChipTone, label: status };
  return (
    <Chip size="sm" tone={s.tone} dot>
      {s.label}
    </Chip>
  );
}

export function EmailChip({ verified }: { verified: boolean }) {
  return (
    <Chip size="sm" tone={verified ? "up" : "warn"} dot>
      {verified ? "Email verified" : "Email unverified"}
    </Chip>
  );
}

export function Mono({ children, className }: { children: React.ReactNode; className?: string }) {
  return <span className={cn("font-mono text-[12.5px]", className)}>{children}</span>;
}

/** Filter pill select (native select styled like the kit). */
export function FilterSelect({ value, onChange, options, label }: { value: string; onChange: (v: string) => void; options: { value: string; label: string }[]; label: string }) {
  return (
    <label className="flex h-9 items-center gap-2 rounded-full border border-line bg-surface-2 pl-3.5 pr-2 text-[12.5px] text-fg-3">
      {label}
      <select value={value} onChange={(e) => onChange(e.target.value)} className="h-full max-w-48 bg-transparent pr-1 text-fg outline-none">
        {options.map((o) => (
          <option key={o.value} value={o.value} className="bg-surface text-fg">
            {o.label}
          </option>
        ))}
      </select>
    </label>
  );
}
