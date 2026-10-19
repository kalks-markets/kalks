"use client";

// Contest rank widget in the screen bar (phase 4): the acting account's rank in its running contest ("#3 of 120"),
// trades still needed to rank, time left and the client's level; a click opens the Client Area leaderboard. Live
// builds with an engine session only (not guests or investor logins), and only while the broker has the rewards
// module on. Refreshes every 60 s (contest scores update every 15 s in the growth service).
import * as React from "react";
import { Trophy } from "lucide-react";
import { cn } from "@kalks/ui";
import { useT } from "@kalks/i18n/react";
import { useTerminal } from "@/lib/store";
import { CLIENT_AREA } from "@/lib/guest";
import { endsIn, pickContest, tradesToGo, type RankData } from "@/lib/rank";
import { useModuleOn } from "@/components/modules";
import { Tip } from "@/components/ui/kit";

function useRank(login: string | null): RankData | null {
  const [data, setData] = React.useState<RankData | null>(null);
  React.useEffect(() => {
    setData(null);
    if (!login) return;
    let stop = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const run = async () => {
      if (document.visibilityState === "visible") {
        try {
          const r = await fetch("/api/growth/rank", { headers: { "x-kalks-login": login }, cache: "no-store", credentials: "same-origin" });
          if (r.status === 401 || r.status === 403) {
            // signed out, investor login or rewards switched off: no widget, no more polling
            if (!stop) setData(null);
            return;
          }
          if (r.ok && !stop) setData((await r.json()) as RankData);
        } catch {
          /* keep the last answer */
        }
      }
      if (!stop) timer = setTimeout(run, 60_000);
    };
    void run();
    return () => {
      stop = true;
      if (timer) clearTimeout(timer);
    };
  }, [login]);
  return data;
}

export function RankWidget() {
  const T = useTerminal();
  const t = useT();
  const on = useModuleOn()("rewards");
  const acting = on && T.engine && !T.guest && !T.readOnly ? T.session.login : null;
  const data = useRank(acting);
  const c = pickContest(data?.contests);
  if (!acting || !data || !c) return null;
  const toGo = tradesToGo(c);
  const left = endsIn(c.endsAt);
  const label = c.rank && toGo === 0 ? t("challenges.trader.rank", { rank: c.rank, ranked: c.ranked }) : t("challenges.trader.unranked");
  const tip = (
    <div className="space-y-0.5">
      <div className="font-medium">{c.name}</div>
      {toGo > 0 && <div>{t("challenges.trader.needsTrades", { count: toGo })}</div>}
      {left && <div>{t("challenges.trader.ends", { time: left })}</div>}
      {data.level && <div>{t("challenges.trader.level", { level: data.level.level, name: data.level.name })}</div>}
      <div className="text-fg-3">{t("challenges.trader.open")}</div>
    </div>
  );
  return (
    <Tip content={tip} side="top">
      <a
        href={`${CLIENT_AREA.replace(/\/+$/, "")}/rewards/leaderboard`}
        target="_blank"
        rel="noopener noreferrer"
        data-testid="rank-widget"
        aria-label={`${t("challenges.trader.title")}: ${label}`}
        className={cn(
          "flex h-7 shrink-0 items-center gap-1.5 rounded-[8px] border px-2.5 text-[12px] font-semibold transition-colors",
          c.rank && c.rank <= 3 && toGo === 0 ? "border-gold/40 bg-gold-soft/50 text-gold" : "border-line bg-panel-2/70 text-fg-2 hover:border-ember/40",
        )}
      >
        <Trophy className="size-3.5" aria-hidden />
        <span className="k-num whitespace-nowrap">{label}</span>
      </a>
    </Tip>
  );
}
