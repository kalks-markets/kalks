"use client";

// Toolbox → MAM (live builds, shown only for MAM accounts):
// - the manager's MAM master account: linked accounts, how a block of the chosen size on the active symbol is
//   allocated right now, and the latest allocations;
// - a linked client account: which programme manages it (MAM trades are tagged MAM; they are changed and closed
//   by the manager, the engine refuses manual edits on them).

import * as React from "react";
import { Briefcase } from "lucide-react";
import { cn } from "@kalks/ui";
import { useTerminal } from "@/lib/store";
import { engineApi, type MamInfo } from "@/lib/engine/client";
import { fmtServer } from "@/lib/trading";
import { Td, Th } from "@/components/ui/panel";
import { Badge, Empty, KV, TInput } from "@/components/ui/primitives";
import { useModuleOn } from "@/components/modules";

const METHOD: Record<string, string> = { equity: "Equity share", balance: "Balance share", multiplier: "Multiplier", percent: "Percent" };
const lots = (v: number | null | undefined) => (typeof v === "number" ? v.toFixed(2) : "—");
const usd = (v: number | null | undefined) => (typeof v === "number" ? `$${v.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 })}` : "—");
const note = (r: string | null) => (!r ? "" : r === "below_min_lot" ? "below min lot" : r === "no_equity" ? "no equity" : r.replace(/_/g, " "));

/** MAM role of the live account (polled; null while unknown or outside the engine). */
export function useMam(symbol?: string, volume?: number): MamInfo | null {
  const T = useTerminal();
  // the broker switched MAM off (module switches): no polling, no MAM tab or allocation preview
  const on = useModuleOn()("mam");
  const login = on && T.engine && T.live && !T.guest ? T.account.login : null;
  const [info, setInfo] = React.useState<MamInfo | null>(null);
  React.useEffect(() => {
    if (!login) {
      setInfo(null);
      return;
    }
    let stop = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let role: MamInfo["role"] = null;
    const run = async () => {
      if (typeof document === "undefined" || document.visibilityState === "visible") {
        const r = await engineApi.mam(login, symbol, volume);
        if (stop) return;
        if (r.ok) {
          role = r.data.role;
          setInfo(r.data);
        }
      }
      // MAM accounts refresh often; any other account only re-checks now and then (a link can start any time)
      if (!stop) timer = setTimeout(run, role ? 15_000 : 60_000);
    };
    run();
    return () => {
      stop = true;
      if (timer) clearTimeout(timer);
    };
  }, [login, symbol, volume]);
  return login ? info : null;
}

export function MamTab() {
  const T = useTerminal();
  const [raw, setRaw] = React.useState("1");
  const volume = Number(raw) > 0 && Number(raw) <= 1000 ? Math.round(Number(raw) * 100) / 100 : 1;
  const info = useMam(T.activeSymbol, volume);
  if (!info) return <Empty icon={<Briefcase />} title="Loading MAM…" />;
  if (info.role === "client")
    return (
      <div className="t-scroll h-full overflow-auto p-3">
        <div className="max-w-[520px] rounded-[8px] border border-line bg-panel-2 p-3">
          <div className="flex items-center gap-2 text-[12.5px] font-medium">
            <Briefcase className="size-3.5 text-fg-3" /> Managed by {info.manager?.name ?? "a MAM manager"} <Badge tone="ember">MAM</Badge>
          </div>
          <div className="mt-2">
            <KV k="Manager" v={info.manager?.nickname ?? "—"} />
            <KV k="Allocation" v={METHOD[info.manager?.method ?? ""] ?? "—"} />
            <KV k="Linked since" v={info.link ? fmtServer(info.link.since) : "—"} />
            <KV k="Your max lot" v={info.link?.maxLot ?? "no cap"} />
            <KV k="Your equity stop" v={info.link?.equityStop ? usd(info.link.equityStop) : "off"} />
          </div>
          <p className="mt-2 text-[11.5px] leading-relaxed text-fg-3">
            Trades tagged MAM are opened, changed and closed by the manager. You can trade your own positions alongside them. To take over the MAM trades, revoke the link in the Client Area (Social → Managed accounts).
          </p>
        </div>
      </div>
    );
  if (info.role !== "manager") return <Empty icon={<Briefcase />} title="This account is not part of a MAM programme" />;
  const p = info.preview;
  return (
    <div className="t-scroll flex h-full min-h-0 flex-col overflow-auto lg:flex-row">
      <div className="shrink-0 border-line p-3 lg:w-[260px] lg:border-e">
        <div className="flex items-center gap-2 text-[12.5px] font-medium">
          <Briefcase className="size-3.5 text-fg-3" /> {info.manager?.name}
          <Badge tone={info.manager?.status === "active" ? "up" : "down"}>{info.manager?.status === "active" ? "Allocating" : "Frozen"}</Badge>
        </div>
        <div className="mt-2">
          <KV k="Allocation" v={METHOD[info.manager?.method ?? ""] ?? "—"} />
          <KV k="Linked accounts" v={<span data-testid="terminal-mam-accounts">{info.accounts ?? 0}</span>} />
          <KV k="Equity managed" v={usd(info.equity)} />
          <KV k="Fees" v={`${info.manager?.perfFeePct ?? 0}%${info.manager?.mgmtFeePct ? ` + ${info.manager.mgmtFeePct}%/y` : ""}`} />
        </div>
        <p className="mt-2 text-[11.5px] leading-relaxed text-fg-3">Every opening trade on this account is a block: it is allocated to the linked accounts, rounded down to the lot step.</p>
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2 border-b border-line px-3 py-1.5 text-[11.5px] text-fg-3">
          <span>
            Allocation of <span className="font-mono text-fg-2">{T.activeSymbol}</span>
          </span>
          <TInput aria-label="Block lots" className="h-6 w-[70px] font-mono" inputMode="decimal" value={raw} onChange={(e) => setRaw(e.target.value)} />
          <span>lots</span>
          {p && (
            <span className="ms-auto">
              allocated <span className="font-mono text-fg-2">{lots(p.allocated)}</span>
              {p.unallocated > 0 && <> · rounding {lots(p.unallocated)}</>}
            </span>
          )}
        </div>
        {!p || p.rows.length === 0 ? (
          <Empty title="No linked accounts yet" sub="Clients link their accounts in the Client Area (Social → Managed accounts)." />
        ) : (
          <table className="w-full min-w-[520px] border-separate border-spacing-0">
            <thead>
              <tr>
                <Th className="ps-3">Account</Th>
                <Th right>{info.manager?.method === "balance" ? "Balance" : "Equity"}</Th>
                <Th right>{info.manager?.method === "multiplier" || info.manager?.method === "percent" ? "Value" : "Share"}</Th>
                <Th right>Lots</Th>
                <Th className="pe-3">Note</Th>
              </tr>
            </thead>
            <tbody>
              {p.rows.map((r) => (
                <tr key={r.linkId} className="hover:bg-surface-2/70">
                  <Td className="ps-3 font-mono">{r.account}</Td>
                  <Td right mono className="text-fg-2">
                    {usd(info.manager?.method === "balance" ? r.balance : r.equity)}
                  </Td>
                  <Td right mono className="text-fg-2">
                    {info.manager?.method === "multiplier" ? `${r.value}×` : info.manager?.method === "percent" ? `${r.value}%` : `${(r.basis * 100).toFixed(2)}%`}
                  </Td>
                  <Td right mono className={cn(r.volume ? "text-fg" : "text-fg-3")}>
                    {lots(r.volume)}
                  </Td>
                  <Td className="pe-3 text-[11.5px] text-fg-3">{note(r.reason)}</Td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {(info.recent?.length ?? 0) > 0 && (
          <div className="border-t border-line px-3 py-2">
            <div className="mb-1 text-[10.5px] font-semibold uppercase tracking-[0.09em] text-fg-3">Latest allocations</div>
            {info.recent!.slice(0, 5).map((a) => (
              <div key={a.id} className="flex items-center gap-3 py-[3px] text-[11.5px]">
                <span className="w-[120px] shrink-0 text-fg-3">{fmtServer(a.at)}</span>
                <span className="font-mono text-fg-3">#{a.masterTicket}</span>
                <span className="font-medium">{a.symbol}</span>
                <span className={a.side === "buy" ? "text-up" : "text-down"}>{a.side}</span>
                <span className="ms-auto font-mono text-fg-2">
                  {lots(a.block)} → {lots(a.allocated)} · {a.accounts} acc.
                </span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
