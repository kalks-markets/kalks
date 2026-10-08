"use client";

// CFD | Options in the terminal header (desktop title bar and mobile header). CFD / Options account split: an account
// trades one product and the ACTIVE ACCOUNT decides the workspace, so on a live terminal this control switches to the
// client's account of that product (the current one if it already trades it, else the last one shown, else the first
// listed), and offers "Open a CFD / Options account" in the Client Area when there is none on this terminal. Without
// an engine account (guest chart mode, demo builds) it switches the workspace itself, held in memory only. Options
// puts the Options / Settlements tabs first in the toolbox; the control is hidden while the broker has Options off.
import * as React from "react";
import { Plus } from "lucide-react";
import { cn } from "@kalks/ui";
import { Tip } from "@/components/ui/kit";
import { useT } from "@kalks/i18n/react";
import { useTerminal, type ToolboxTab } from "@/lib/store";
import { AccountProductContext, setTradeMode, useTradeMode, type TradeMode } from "@/lib/options/mode";
import { openAccountPath, productOf, switchTarget } from "@/lib/options/product";
import type { EngineTradingAccount } from "@/lib/engine/map";
import { CLIENT_AREA } from "@/lib/guest";
import { useModuleOn } from "@/components/modules";

const CFD_ONLY: ToolboxTab[] = ["positions", "pending", "trade", "history", "exposure"];

/** Logins shown in this tab, newest first (which account of a product the control opens). */
let recent: string[] = [];

/** The product of each account this terminal holds (engine accounts carry it; others count as CFD). */
export function useAccountProducts() {
  const T = useTerminal();
  return React.useMemo(() => T.accounts.map((a) => ({ login: a.login, product: productOf((a as Partial<EngineTradingAccount>).engine) })), [T.accounts]);
}

/** Switches to the CFD or the Options workspace: by account on a live terminal, else in memory. */
export function useSwitchMode() {
  const T = useTerminal();
  const byAccount = React.useContext(AccountProductContext) !== null;
  const products = useAccountProducts();
  const optionsOn = useModuleOn()("options");
  return React.useCallback(
    (m: TradeMode) => {
      if (m === "options" && !optionsOn) return;
      if (byAccount) {
        const login = switchTarget(products, T.account.login, m, recent);
        if (!login) {
          window.open(`${CLIENT_AREA}${openAccountPath(m)}`, "_blank", "noopener");
          return;
        }
        if (login !== T.account.login) T.switchAccount(login);
      } else setTradeMode(m);
      const tab = T.ws.toolboxTab;
      if (m === "options" && CFD_ONLY.includes(tab)) T.setWs({ toolboxTab: "options" });
      if (m === "cfd" && (tab === "settlements" || tab === "closed")) T.setWs({ toolboxTab: "positions" });
    },
    [T, byAccount, products, optionsOn],
  );
}

export function ModeSwitch({ size = "md", className }: { size?: "sm" | "md"; className?: string }) {
  const t = useT();
  const T = useTerminal();
  const mode = useTradeMode();
  const sw = useSwitchMode();
  const byAccount = React.useContext(AccountProductContext) !== null;
  const products = useAccountProducts();
  const optionsOn = useModuleOn()("options");
  React.useEffect(() => {
    recent = [T.account.login, ...recent.filter((l) => l !== T.account.login)].slice(0, 20);
  }, [T.account.login]);
  if (!optionsOn) return null;
  // a product the client has no account of here: the segment opens the Client Area's open-account wizard
  const missing = (m: TradeMode) => byAccount && !products.some((a) => a.product === m);
  const hint = (m: TradeMode) => (missing(m) ? t(m === "cfd" ? "trader.acct.openCfd" : "trader.acct.openOptions") : m === "cfd" ? t("trader.opt.mode.cfdHint") : t("trader.opt.mode.optionsHint"));
  const label = (m: TradeMode) => (
    <>
      {m === "cfd" ? t("trader.opt.mode.cfd") : t("trader.opt.mode.options")}
      {missing(m) ? <Plus className="size-3 opacity-70" aria-hidden /> : m === "options" && mode !== "options" && <span className="size-1.5 rounded-full bg-ember" aria-hidden />}
    </>
  );
  const md = size === "md";
  if (md)
    // iOS-style segmented control: a frosted thumb slides under the active workspace
    return (
      <div role="tablist" aria-label={t("trader.opt.mode.label")} data-tour="mode" className={cn("relative grid h-7 shrink-0 grid-cols-2 rounded-[9px] border border-line bg-panel-2/80 p-[2px]", className)}>
        <span
          aria-hidden
          className="t-glass-strong absolute inset-y-[2px] start-[2px] w-[calc(50%-2px)] rounded-[7px] border border-[var(--t-glass-edge)] shadow-[0_1px_3px_rgba(0,0,0,0.25)] transition-transform duration-200 ease-out"
          style={{ transform: mode === "options" ? "translateX(100%)" : "translateX(0)" }}
        />
        {(["cfd", "options"] as const).map((m) => (
          <Tip key={m} content={hint(m)} side="bottom">
            <button
              role="tab"
              aria-selected={mode === m}
              data-testid={`mode-${m}`}
              onClick={() => sw(m)}
              className={cn("relative z-[1] flex items-center justify-center gap-1 px-2.5 text-[12px] font-semibold transition-colors duration-200", mode === m ? "text-fg" : "text-fg-3 hover:text-fg-2")}
            >
              {label(m)}
            </button>
          </Tip>
        ))}
      </div>
    );
  return (
    <div role="tablist" aria-label={t("trader.opt.mode.label")} className={cn("flex shrink-0 items-center rounded-[7px] border border-line bg-surface-2 p-0.5", className)}>
      {(["cfd", "options"] as const).map((m) => (
        <button
          key={m}
          role="tab"
          aria-selected={mode === m}
          onClick={() => sw(m)}
          title={hint(m)}
          className={cn("flex h-6 items-center gap-1 rounded-[5px] px-2 text-[11px] font-semibold transition-colors", mode === m ? "bg-surface-3 text-fg shadow-[inset_0_1px_0_var(--k-border-top)]" : "text-fg-3 hover:text-fg-2")}
        >
          {label(m)}
        </button>
      ))}
    </div>
  );
}
