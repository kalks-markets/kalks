"use client";

import * as React from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { ACCOUNTS, INSTRUMENT_MAP, priceFeed } from "@kalks/mock";
import { LogoMark } from "@kalks/ui";
import { tr, useT } from "@kalks/i18n/react";
import { toast } from "@/lib/notify";
import { TerminalProvider, engineSession, guestSession, readActive, readSession, savedCharts, useTerminal, writeActive, writeSession, type Session } from "@/lib/store";
import { prefetchHistory } from "@/components/chart/engine";
import { GUEST_MODE } from "@/lib/guest";
import { engineApi } from "@/lib/engine/client";
import type { SessionInfo } from "@/lib/engine/types";
import { LoginDialog } from "./dialogs/login-dialog";
import { startMarket } from "@/lib/market";
import { DesktopTerminal } from "./shell/desktop";
import { useHotkeys } from "./shell/hotkeys";
import { MobileTerminal } from "./mobile/mobile-terminal";
import { NewOrderDialog } from "./order/new-order-dialog";
import { PendingDialog, PositionDialog } from "./dialogs/position-dialog";
import { AboutDialog, GlossaryDialog, OptionsDialog, ShortcutsDialog, SpecDialog, SymbolSearch } from "./dialogs/misc-dialogs";
import { IndicatorDialogs } from "./chart/indicators/dialogs";
import { ShareLayer } from "./share/share-dialogs";
import { ConfirmLayer } from "./dialogs/confirm";
import { ControlsBanner } from "./shell/controls-banner";
import { CopyBanner } from "./shell/copy-banner";
import { applyLinkMode } from "@/lib/options/mode";
import { openAccountPath, parseMode, pickSession, productOf } from "@/lib/options/product";
import type { EngineTradingAccount } from "@/lib/engine/map";
import { CLIENT_AREA } from "@/lib/guest";

function useIsMobile() {
  const [m, setM] = React.useState(() => typeof window !== "undefined" && window.matchMedia("(max-width: 1023px)").matches);
  React.useEffect(() => {
    const mq = window.matchMedia("(max-width: 1023px)");
    const f = () => setM(mq.matches);
    f();
    mq.addEventListener("change", f);
    return () => mq.removeEventListener("change", f);
  }, []);
  return m;
}

export function Splash({ text }: { text?: string }) {
  const t = useT();
  const label = text ?? (GUEST_MODE ? t("trader.splash.connecting", { server: "Kalks" }) : t("trader.splash.connecting", { server: "Kalks-Live01" }));
  return (
    <div className="grid h-dvh place-items-center bg-page">
      <div className="flex flex-col items-center gap-3">
        <span className="grid size-12 place-items-center rounded-[12px] border border-line-top bg-surface-3">
          <LogoMark size={22} className="text-fg" />
        </span>
        <div className="text-[13px] font-semibold">
          Kalks <span className="font-normal text-fg-2">Trader</span>
        </div>
        <div className="flex items-center gap-2 font-mono text-[11px] text-fg-3">
          <span className="size-1.5 rounded-full bg-ember" />
          {label}
        </div>
      </div>
    </div>
  );
}

/**
 * Live builds pick the session from the trading engine: `?sso=<token>` (Client Area Trade button) is
 * redeemed by the BFF first; then every login this browser holds (HttpOnly cookie) is listed and the
 * active one is `?account=`, else (with `?mode=options|cfd`) an account of that product, else the last one shown,
 * else the newest. The active account's product decides the workspace. No login → guest chart mode.
 */
async function liveEntry(sp: URLSearchParams): Promise<{ session: Session; sessions: SessionInfo[] }> {
  const sso = sp.get("sso");
  let prefer = sp.get("account");
  let via: Session["via"] = "login";
  if (sso) {
    // drop the one-time token from the address bar (and history) before anything else
    window.history.replaceState(null, "", "/");
    const r = await engineApi.sso(sso);
    if (r.ok) {
      prefer = r.data.login;
      via = "sso";
    } else toast.error(tr("trader.toast.ssoRejected"), { description: r.err.message });
  }
  const list = await engineApi.sessions();
  const sessions = list.ok ? list.data.sessions : [];
  if (!list.ok) toast.error(tr("trader.toast.serverUnavailable"), { description: tr("trader.toast.serverUnavailableHint") });
  // CFD / Options account split: `?mode=options` (or `cfd`) opens the client's account of that product
  const pick = pickSession(sessions, { prefer, active: readActive(), want: parseMode(sp.get("mode")) });
  if (!pick) {
    if (prefer && !sso) window.location.replace(`/login?login=${encodeURIComponent(prefer)}`);
    return { session: guestSession(), sessions: [] };
  }
  writeActive(pick.login);
  return { session: engineSession(pick, via), sessions };
}

/**
 * Entry. Live builds: see liveEntry(). Demo builds: SSO via `?account=` (from the Client Area), else a
 * saved session, else /login.
 * `?symbol=` opens that symbol in the active chart; `?side=buy|sell` opens a prefilled order.
 */
export function Terminal() {
  const sp = useSearchParams();
  const router = useRouter();
  const [session, setSession] = React.useState<Session | null>(null);
  const [sessions, setSessions] = React.useState<SessionInfo[]>([]);
  // read before liveEntry() / the demo SSO wipe the query string (`?sso=…&mode=options` from the Client Area's
  // Options page, `?mode=options&u=EURUSD` from the public option chain)
  const [intent] = React.useState(() => ({ symbol: sp.get("symbol")?.toUpperCase() ?? null, side: sp.get("side"), mode: sp.get("mode"), u: sp.get("u") }));

  React.useEffect(() => {
    const acc = sp.get("account");
    let s: Session | null = null;
    if (GUEST_MODE) {
      let alive = true;
      const feed = priceFeed();
      feed.markHydrated();
      // chart history doesn't depend on the session: request the saved layout's charts now, not after sign-in
      for (const c of savedCharts()) prefetchHistory(c.symbol, c.tf);
      void liveEntry(new URLSearchParams(sp.toString())).then(async (r) => {
        if (!alive) return;
        if (window.location.search) window.history.replaceState(null, "", "/");
        const g = r.sessions.find((x) => x.login === r.session.login)?.account?.spreadGroup;
        if (g) feed.setGroup(g); // quotes carry the account group's spread (what the engine fills at)
        await feed.ready;
        if (!alive) return;
        startMarket();
        setSessions(r.sessions);
        setSession(r.session);
      });
      return () => {
        alive = false;
      };
    } else if (acc) {
      const a = ACCOUNTS.find((x) => x.login === acc);
      if (!a) {
        router.replace(`/login?error=unknown&login=${encodeURIComponent(acc)}`);
        return;
      }
      s = { login: a.login, investor: false, server: a.server, via: "sso", at: Date.now() };
      writeSession(s);
    } else {
      s = readSession();
      if (!s || !ACCOUNTS.some((x) => x.login === s!.login)) {
        router.replace("/login");
        return;
      }
    }
    if (sp.toString()) window.history.replaceState(null, "", "/");
    // live prices (with this account group's spread) before the terminal mounts
    const feed = priceFeed();
    feed.markHydrated();
    const group = ACCOUNTS.find((x) => x.login === s!.login)?.group;
    if (group) feed.setGroup(group); // guest: the default (standard) spread
    let alive = true;
    void feed.ready.then(() => {
      if (!alive) return;
      startMarket();
      setSession(s);
    });
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!session) return <Splash />;
  return (
    <TerminalProvider initialSession={session} engineSessions={sessions} onLogout={(to) => (GUEST_MODE ? window.location.replace(to ?? "/login?logout=1") : router.replace("/login?logout=1"))}>
      <Shell intent={intent} />
    </TerminalProvider>
  );
}

function Shell({ intent }: { intent: { symbol: string | null; side: string | null; mode: string | null; u: string | null } }) {
  const T = useTerminal();
  const mobile = useIsMobile();
  useHotkeys();
  React.useEffect(() => {
    // CFD | Options from the link (the account was already picked by product, liveEntry); Options opens with its
    // toolbox tab in front. A live terminal without an Options account says how to get one.
    const want = parseMode(intent.mode);
    if (applyLinkMode(intent.mode, intent.u) && want === "options") {
      const onOptions = !T.engine || T.guest || productOf((T.account as Partial<EngineTradingAccount>).engine) === "options";
      if (onOptions && ["positions", "pending", "trade", "history", "exposure"].includes(T.ws.toolboxTab)) T.setWs({ toolboxTab: "options" });
      if (!onOptions)
        toast.info(tr("options.account.noneTitle"), {
          description: tr("options.account.noneText"),
          duration: 10_000,
          action: { label: tr("options.account.open"), onClick: () => window.open(`${CLIENT_AREA}${openAccountPath("options")}`, "_blank", "noopener") },
        });
    }
    const sym = intent.symbol && INSTRUMENT_MAP[intent.symbol] ? intent.symbol : null;
    if (sym) T.openSymbol(sym);
    if (intent.side === "buy" || intent.side === "sell") T.openNewOrder({ symbol: sym ?? T.activeSymbol, side: intent.side, type: "market" });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  return (
    <>
      {/* staff session / account restrictions banner above the terminal (shell/controls-banner.tsx) */}
      <div className="flex h-dvh flex-col">
        <ControlsBanner />
        <CopyBanner />
        <div className="min-h-0 flex-1 [&>div]:h-full">{mobile ? <MobileTerminal /> : <DesktopTerminal />}</div>
      </div>
      <NewOrderDialog />
      <PositionDialog />
      <PendingDialog />
      <SymbolSearch />
      <ShortcutsDialog />
      <SpecDialog />
      <AboutDialog />
      <GlossaryDialog />
      <OptionsDialog />
      <IndicatorDialogs />
      <ShareLayer />
      <ConfirmLayer />
      {T.live && <LoginDialog />}
    </>
  );
}
