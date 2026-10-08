"use client";

// Live Options page: suitability from the gateway (/api/suitability/options) and the client's trading accounts from
// the engine; "Start trading options" records the acceptance of the options terms and opens Kalks Trader through
// the usual one-time SSO link, in options mode. Only the client's Options accounts are offered (CFD / Options account
// split); without one the page offers "Open an Options account".

import * as React from "react";
import { toast } from "sonner";
import { tr, useT } from "@kalks/i18n/react";
import { useReadOnly, useSession } from "@/components/session";
import { errorToast, tradingApi, useAccounts } from "@/components/trading/api";
import { accountFlavor } from "@/components/trading/archive";
import { isPropAccount } from "@/components/trading/ui";
import { TERMINAL_URL } from "@/lib/live";
import { productOf } from "@/lib/products";
import { SuitabilityError, suitabilityApi, useSuitability, type Suitability } from "./api";
import { OptionsPage, type OptionsController, type TradeAccount } from "./ui";

/** The terminal opens straight in options mode with `?mode=options` next to the SSO token. */
export function withOptionsMode(url: string): string {
  try {
    const u = new URL(url, window.location.href);
    u.searchParams.set("mode", "options");
    return u.toString();
  } catch {
    return url;
  }
}

/** Opens Kalks Trader signed in to `login`, in options mode. The tab opens inside the click (popup blockers), then
 *  `before` runs (e.g. recording the acceptance): false closes the tab again. */
async function openOptionsTerminal(login: number, before?: () => Promise<boolean>) {
  const w = window.open("about:blank", "_blank");
  try {
    if (before && !(await before())) {
      w?.close();
      return;
    }
    const r = await tradingApi<{ url: string }>(`accounts/${login}/sso`, { body: {} });
    const url = withOptionsMode(r.url);
    if (w && !w.closed) {
      w.opener = null;
      w.location.replace(url);
    } else {
      window.location.assign(url);
    }
  } catch (e) {
    w?.close();
    errorToast(tr("accounts.toast.openTraderFailed"), e);
  }
}

export function LiveOptions() {
  const t = useT();
  const user = useSession() as ReturnType<typeof useSession> & { impersonation?: unknown };
  // view-only logins and staff sessions (even full access) can't accept the terms for the client
  const readOnly = useReadOnly() || !!user.impersonation;
  const s = useSuitability();
  const { data: acc, error: accError } = useAccounts(0);

  const accounts = React.useMemo<TradeAccount[] | null>(() => {
    if (!acc) return null;
    // CFD / Options account split: options trade on Options accounts only
    return acc.accounts
      .filter((a) => a.status === "active" && productOf(a) === "options" && !isPropAccount(a) && !accountFlavor(a))
      .sort((a, b) => Number(!!b.isDefault) - Number(!!a.isDefault) || (a.type === b.type ? 0 : a.type === "live" ? -1 : 1) || a.login - b.login)
      .map((a) => ({ login: a.login, type: a.type, name: a.name || a.groupName }));
  }, [acc]);

  const accept = React.useCallback(
    async (version: number) => {
      try {
        const d = await suitabilityApi<Suitability>("options/accept", { version });
        s.set(d);
        toast.success(t("options.intro.toastStarted"));
        return true;
      } catch (e) {
        if (e instanceof SuitabilityError && e.code === "disclosure_outdated") {
          toast.warning(t("options.intro.toastUpdated"));
          s.reload();
        } else toast.error(t("options.intro.toastFailed"), { description: e instanceof Error ? e.message : undefined });
        return false;
      }
    },
    [s, t],
  );

  const ctl: OptionsController = {
    data: s.data,
    error: s.error ? `${t("options.error.load")} ${s.error.message}` : null,
    reload: s.reload,
    accept,
    accounts,
    traderHref: !acc && accError ? `${TERMINAL_URL}/?mode=options` : null,
    openTrader: (a, before) => openOptionsTerminal(a.login, before),
    readOnly,
  };
  return <OptionsPage ctl={ctl} />;
}
