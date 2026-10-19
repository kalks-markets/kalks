"use client";

import * as React from "react";
import { useRouter } from "next/navigation";
import { toast, useSonner } from "sonner";
import { ArrowDownToLine, Bell, CandlestickChart, CheckCircle2, Coins, IdCard, Info, LifeBuoy, Megaphone, ShieldCheck, Trophy, TriangleAlert, Users, Wallet, XCircle } from "lucide-react";
import { EmptyState, IconButton, Popover, cn } from "@/components/kit";
import type { TileTone } from "@/components/kit";
import { IS_DEMO, NOTIFICATIONS } from "@kalks/mock";
import { realtime, type Frame } from "@/lib/realtime";
import { useFormat, useT } from "@kalks/i18n/react";

/* ------------------------------------------------------------------ */
/* In-app event log: every toast is kept as a notification             */
/* ------------------------------------------------------------------ */

type Kind = "success" | "error" | "warning" | "info" | "default";
export type LoggedEvent = { id: string; kind: Kind; title: string; description?: string; at: number; unread: boolean };

const CAP = 100;
const EMPTY: LoggedEvent[] = [];
let storeKey = "";
let cache: LoggedEvent[] = EMPTY;
const listeners = new Set<() => void>();

function read(key: string): LoggedEvent[] {
  try {
    const raw = window.localStorage.getItem(key);
    const v = raw ? (JSON.parse(raw) as LoggedEvent[]) : [];
    return Array.isArray(v) ? v.slice(0, CAP) : [];
  } catch {
    return [];
  }
}

function write(next: LoggedEvent[]) {
  cache = next.slice(0, CAP);
  try {
    window.localStorage.setItem(storeKey, JSON.stringify(cache));
  } catch {
    /* private mode / storage full: keep the in-memory log */
  }
  listeners.forEach((l) => l());
}

/** Per-browser, per-client log (so two clients sharing a browser never see each other's events). */
function useEventLog(userKey: string): LoggedEvent[] {
  const key = `kalks.crm.notifications.${userKey}`;
  if (typeof window !== "undefined" && storeKey !== key) {
    storeKey = key;
    cache = read(key);
  }
  React.useEffect(() => {
    const onStorage = (e: StorageEvent) => {
      if (e.key !== storeKey) return;
      cache = read(storeKey);
      listeners.forEach((l) => l());
    };
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, []);
  return React.useSyncExternalStore(
    (cb) => {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    () => cache,
    () => EMPTY,
  );
}

function text(node: unknown): string {
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (typeof node === "function") return text((node as () => unknown)());
  return "";
}

/** Records every toast shown in the Client Area (sonner's active list) into the event log. */
function ToastRecorder() {
  const { toasts } = useSonner();
  const seen = React.useRef(new Set<string | number>());
  React.useEffect(() => {
    const fresh: LoggedEvent[] = [];
    for (const t of toasts) {
      if (seen.current.has(t.id)) continue;
      seen.current.add(t.id);
      // live notifications from the service are already in the bell
      if (typeof t.id === "string" && t.id.startsWith("srv-")) continue;
      if (t.type === "loading") continue;
      const title = text(t.title);
      if (!title) continue;
      const kind: Kind = t.type === "success" || t.type === "error" || t.type === "warning" || t.type === "info" ? t.type : "default";
      fresh.push({ id: `${Date.now()}-${t.id}`, kind, title, description: text(t.description) || undefined, at: Date.now(), unread: true });
    }
    if (fresh.length) write([...fresh.reverse(), ...cache]);
  }, [toasts]);
  return null;
}

/* ------------------------------------------------------------------ */
/* Service notifications (services/support via /api/notifications)     */
/* ------------------------------------------------------------------ */

export type ServerItem = {
  id: number;
  type: string;
  category: string;
  severity: "info" | "success" | "warning" | "critical";
  title: string;
  body: string;
  link: string | null;
  read: boolean;
  createdAt: string;
};

async function call<T>(path: string, body?: unknown): Promise<T | null> {
  try {
    const r = await fetch(`/api/notifications${path}`, {
      method: body === undefined ? "GET" : "POST",
      headers: body === undefined ? undefined : { "content-type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
      cache: "no-store",
    });
    if (!r.ok) return null;
    return (await r.json()) as T;
  } catch {
    return null;
  }
}

/** Live inbox from the notification service: first page + realtime pushes. */
function useServerNotifications(enabled: boolean) {
  const [items, setItems] = React.useState<ServerItem[]>([]);
  const [unread, setUnread] = React.useState(0);
  const load = React.useCallback(async () => {
    const d = await call<{ items: ServerItem[]; unread: number }>("?limit=40");
    if (d) {
      setItems(d.items);
      setUnread(d.unread);
    }
  }, []);
  React.useEffect(() => {
    if (!enabled) return;
    void load();
    return realtime().subscribe((f: Frame) => {
      if (f.type === "reconnected") return void load();
      if (f.type === "hello") return setUnread(Number(f.unread ?? 0));
      if (f.type === "notification") {
        const it = f.item as ServerItem;
        setItems((xs) => (xs.some((x) => x.id === it.id) ? xs : [it, ...xs].slice(0, 100)));
        setUnread(Number(f.unread ?? 0));
        // a reply in the chat you're looking at needs no toast
        if (it.type === "support.reply" && window.location.pathname.startsWith("/support")) return;
        const fn = it.severity === "critical" || it.severity === "warning" ? toast.warning : it.severity === "success" ? toast.success : toast.info;
        fn(it.title, { id: `srv-${it.id}`, description: it.body || undefined });
      }
      if (f.type === "notifications.read") {
        setUnread(Number(f.unread ?? 0));
        if (f.cleared) setItems([]);
        else if (f.all) setItems((xs) => xs.map((x) => ({ ...x, read: true })));
        else {
          const ids = new Set((f.ids as number[]) ?? []);
          setItems((xs) => xs.map((x) => (ids.has(x.id) ? { ...x, read: true } : x)));
        }
      }
    });
  }, [enabled, load]);
  return {
    items,
    unread,
    markAll: () => {
      setItems((xs) => xs.map((x) => ({ ...x, read: true })));
      setUnread(0);
      void call("/read", { all: true });
    },
    markOne: (id: number) => {
      setItems((xs) => xs.map((x) => (x.id === id ? { ...x, read: true } : x)));
      setUnread((n) => Math.max(0, n - 1));
      void call("/read", { ids: [id] });
    },
    clear: () => {
      setItems([]);
      setUnread(0);
      void call("/clear", {});
    },
  };
}

/* ------------------------------------------------------------------ */
/* Bell                                                                */
/* ------------------------------------------------------------------ */

const KIND_ICON: Record<Kind, { icon: React.ReactNode; cls: string; tone: TileTone }> = {
  success: { icon: <CheckCircle2 />, cls: "text-up", tone: "mint" },
  error: { icon: <XCircle />, cls: "text-down", tone: "coral" },
  warning: { icon: <TriangleAlert />, cls: "text-warn", tone: "amber" },
  info: { icon: <Info />, cls: "text-info", tone: "sky" },
  default: { icon: <Bell />, cls: "text-fg-2", tone: "lavender" },
};

/** Pastel tile colour per notification category (dashboard list). */
const CATEGORY_TONE: Record<string, TileTone> = {
  security: "lavender",
  trading_alerts: "amber",
  trading_fills: "accent",
  wallet: "mint",
  kyc: "sky",
  ib: "amber",
  copy: "pink",
  prop: "amber",
  support: "lavender",
  marketing: "pink",
  system: "neutral",
};

const CATEGORY_ICON: Record<string, React.ReactNode> = {
  security: <ShieldCheck />,
  trading_alerts: <TriangleAlert />,
  trading_fills: <CandlestickChart />,
  wallet: <Wallet />,
  kyc: <IdCard />,
  ib: <Coins />,
  copy: <Users />,
  prop: <Trophy />,
  support: <LifeBuoy />,
  marketing: <Megaphone />,
  system: <Bell />,
};

const SEVERITY_CLS: Record<ServerItem["severity"], string> = { info: "text-info", success: "text-up", warning: "text-warn", critical: "text-down" };

const MOCK_ICON: Record<string, { icon: React.ReactNode; tone: TileTone }> = {
  fill: { icon: <CheckCircle2 />, tone: "accent" },
  deposit: { icon: <ArrowDownToLine />, tone: "mint" },
  kyc: { icon: <IdCard />, tone: "sky" },
  ib: { icon: <Coins />, tone: "amber" },
  margin: { icon: <TriangleAlert />, tone: "coral" },
};

function ago(at: number, now: number, t: ReturnType<typeof useT>, f: ReturnType<typeof useFormat>) {
  const s = Math.max(0, Math.round((now - at) / 1000));
  if (s < 45) return t("dashboard.time.justNow");
  const m = Math.round(s / 60);
  if (m < 60) return t("dashboard.time.minutesAgo", { count: m });
  const h = Math.round(m / 60);
  if (h < 24) return t("dashboard.time.hoursAgo", { count: h });
  const d = Math.round(h / 24);
  if (d < 7) return t("dashboard.time.daysAgo", { count: d });
  return f.date(at, { day: "numeric", month: "short" });
}

export type NotificationRow = { id: string; title: string; description?: string; at: number; time: string; unread: boolean; icon: React.ReactNode; iconCls: string; tone: TileTone; onOpen?: () => void };

type Api = { rows: NotificationRow[]; unread: number; markAll: () => void; clear: () => void; touch: () => void };
const NOOP = () => {};
const NotificationsContext = React.createContext<Api>({ rows: [], unread: 0, markAll: NOOP, clear: NOOP, touch: NOOP });

/** The client's notifications (service inbox + this browser's toast history; sample ones in demo builds). */
export function useNotifications(): Api {
  return React.useContext(NotificationsContext);
}

/**
 * One inbox for the Client Area: the client's notifications from the notification service (deposits, withdrawals,
 * verification, margin calls, support replies, announcements; live over the realtime stream) together with every
 * toast shown in this browser (kept locally as history). Demo builds list the sample notifications. Read by the top
 * bar's bell and the dashboard's Notifications list. Off (`enabled` false) for view-only sessions.
 */
export function NotificationsProvider({ userKey, enabled = true, children }: { userKey: string; enabled?: boolean; children: React.ReactNode }) {
  const router = useRouter();
  const t = useT();
  const f = useFormat();
  const log = useEventLog(userKey);
  const srv = useServerNotifications(enabled && !IS_DEMO);
  const [mock, setMock] = React.useState(() => (IS_DEMO && enabled ? NOTIFICATIONS.map((n) => ({ ...n })) : []));
  // relative times are computed after mount (server and first client render agree) and refreshed every minute
  const [now, setNow] = React.useState(0);
  React.useEffect(() => {
    setNow(Date.now());
    const id = setInterval(() => setNow(Date.now()), 60_000);
    return () => clearInterval(id);
  }, []);

  const rows: NotificationRow[] = [
    ...srv.items.map((n) => {
      const at = new Date(n.createdAt).getTime();
      return {
        id: `srv-${n.id}`,
        title: n.title,
        description: n.body || undefined,
        at,
        time: now ? ago(at, now, t, f) : "",
        unread: !n.read,
        icon: CATEGORY_ICON[n.category] ?? <Bell />,
        iconCls: SEVERITY_CLS[n.severity] ?? "text-fg-2",
        tone: n.severity === "critical" ? ("coral" as const) : (CATEGORY_TONE[n.category] ?? "neutral"),
        onOpen: () => {
          if (!n.read) srv.markOne(n.id);
          if (n.link?.startsWith("/")) router.push(n.link);
          else if (n.link) window.open(n.link, "_blank", "noopener");
        },
      };
    }),
    ...log.map((e) => ({ id: e.id, title: e.title, description: e.description, at: e.at, time: now ? ago(e.at, now, t, f) : "", unread: e.unread, icon: KIND_ICON[e.kind].icon, iconCls: KIND_ICON[e.kind].cls, tone: KIND_ICON[e.kind].tone })),
  ].sort((a, b) => b.at - a.at);
  const mockRows: NotificationRow[] = mock.map((n) => ({ id: `mock-${n.id}`, title: n.title, at: 0, time: t("dashboard.time.ago", { time: n.time }), unread: n.unread, icon: MOCK_ICON[n.kind]?.icon ?? <Bell />, iconCls: "text-fg-2", tone: MOCK_ICON[n.kind]?.tone ?? "neutral" }));
  const all = [...rows, ...mockRows];
  const unread = srv.unread + log.filter((e) => e.unread).length + mock.filter((m) => m.unread).length;

  const markAll = () => {
    if (log.some((e) => e.unread)) write(log.map((e) => ({ ...e, unread: false })));
    if (srv.unread > 0) srv.markAll();
    setMock((m) => m.map((n) => ({ ...n, unread: false })));
  };
  const clear = () => {
    write([]);
    if (srv.items.length) srv.clear();
    setMock([]);
  };
  const api: Api = { rows: enabled ? all : [], unread: enabled ? unread : 0, markAll, clear, touch: () => setNow(Date.now()) };
  return (
    <NotificationsContext.Provider value={api}>
      {enabled && <ToastRecorder />}
      {children}
    </NotificationsContext.Provider>
  );
}

/** Top-bar bell: the inbox in a popover. */
export function NotificationsBell() {
  const t = useT();
  const { rows: all, unread, markAll, clear, touch } = useNotifications();
  return (
    <Popover
      width={380}
      trigger={
        <IconButton aria-label={unread ? t("dashboard.notifications.ariaUnread", { count: unread }) : t("dashboard.notifications.title")} data-testid="notifications-bell" onPointerDown={touch} onKeyDown={touch}>
          <Bell />
          {unread > 0 && (
            <span data-testid="notifications-unread" className="k-num pointer-events-none absolute -end-1 -top-1 grid h-[18px] min-w-[18px] place-items-center rounded-full bg-ember px-1 text-[10px] font-semibold leading-none text-[var(--k-on-ember)] ring-2 ring-[var(--k-bg)]">
              {unread > 99 ? "99+" : unread}
            </span>
          )}
        </IconButton>
      }
    >
      <div className="flex items-center justify-between gap-3 border-b border-line px-4 py-3">
        <div className="text-sm font-bold">
          {t("dashboard.notifications.title")} {unread > 0 && <span className="k-num ms-1 rounded-full bg-ember-soft px-1.5 text-[11px] text-ember">{unread}</span>}
        </div>
        {all.length > 0 && (
          <div className="flex items-center gap-3 text-xs">
            <button onClick={markAll} disabled={!unread} className="text-fg-3 hover:text-fg disabled:opacity-40 disabled:hover:text-fg-3">
              {t("dashboard.notifications.markAll")}
            </button>
            <button onClick={clear} className="text-fg-3 hover:text-down">
              {t("dashboard.notifications.clear")}
            </button>
          </div>
        )}
      </div>
      {all.length === 0 ? (
        <EmptyState illustration="bell" contentSlot={{ page: "empty", slot: "notifications.image" }} title={t("dashboard.notifications.emptyTitle")} text={t("dashboard.notifications.emptyText")} className="py-10" />
      ) : (
        <div className="max-h-[min(24rem,60vh)] overflow-y-auto p-1.5" data-testid="notifications-list">
          {all.map((n) => {
            const inner = (
              <>
                <span className={cn("k-tile mt-0.5 size-9 shrink-0 rounded-[11px] [&_svg]:size-4", `k-tile-${n.tone}`)}>{n.icon}</span>
                <div className="min-w-0 flex-1">
                  <p className={cn("text-[13px] leading-snug", n.unread ? "font-semibold text-fg" : "text-fg-2")}>{n.title}</p>
                  {n.description && <p className="mt-0.5 line-clamp-2 text-[12px] leading-snug text-fg-3">{n.description}</p>}
                  {n.time && <p className="mt-0.5 text-[11.5px] text-fg-3">{n.time}</p>}
                </div>
                {n.unread && <span className="mt-2 size-2 shrink-0 rounded-full bg-ember" />}
              </>
            );
            return n.onOpen ? (
              <button key={n.id} onClick={n.onOpen} className="flex w-full gap-3 rounded-[14px] px-3 py-2.5 text-start hover:bg-surface-2">
                {inner}
              </button>
            ) : (
              <div key={n.id} className="flex gap-3 rounded-[14px] px-3 py-2.5 hover:bg-surface-2">
                {inner}
              </div>
            );
          })}
        </div>
      )}
      <div className="border-t border-line px-4 py-2.5 text-end text-xs">
        <a href="/profile/notifications" className="text-fg-3 hover:text-fg">
          {t("dashboard.notifications.settings")}
        </a>
      </div>
    </Popover>
  );
}
