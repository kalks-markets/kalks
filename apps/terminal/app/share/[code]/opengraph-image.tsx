import { readFile } from "node:fs/promises";
import path from "node:path";
import { ImageResponse } from "next/og";
import { fetchPublicShare } from "@/lib/gateway";
import { shareTotals, signed, usd } from "@/lib/share-stats";
import type { PublicShare } from "@/lib/share";

export const runtime = "nodejs";
export const alt = "Shared trades on Kalks Trader";
export const size = { width: 1200, height: 630 };
export const contentType = "image/png";

// Kalks tokens (OG images can't read CSS variables)
const C = { bg: "#07070a", surface: "#111114", surface2: "#17171c", line: "#26262d", fg: "#f5f5f7", fg2: "#a1a1aa", fg3: "#63636e", ember: "#ff5a1f", gold: "#e9b949", up: "#2f7bff", down: "#f04438" };

async function logo() {
  try {
    const svg = await readFile(path.join(process.cwd(), "public/assets/brand/kalks-logo.svg"), "utf8");
    return `data:image/svg+xml;base64,${Buffer.from(svg.replace(/currentColor/g, C.fg)).toString("base64")}`;
  } catch {
    return null;
  }
}

export default async function OgImage({ params }: { params: Promise<{ code: string }> }) {
  const { code } = await params;
  const [r, mark] = await Promise.all([fetchPublicShare(code), logo()]);
  const s = r.data as unknown as PublicShare | null;

  const brand = mark ? <img src={mark} height={44} width={Math.round(44 * (1954 / 541))} alt="" /> : <div style={{ fontSize: 40, fontWeight: 700 }}>Kalks</div>;

  if (!s)
    return new ImageResponse(
      (
        <div style={{ width: "100%", height: "100%", display: "flex", flexDirection: "column", justifyContent: "space-between", background: C.bg, color: C.fg, padding: 72 }}>
          {brand}
          <div style={{ fontSize: 56, fontWeight: 700 }}>This link is no longer available</div>
          <div style={{ fontSize: 28, color: C.fg3 }}>Kalks Trader</div>
        </div>
      ),
      size,
    );

  const t = shareTotals(s.trades, {}, s.show_amounts);
  const symbols = [...new Set(s.trades.map((x) => x.symbol))].slice(0, 6);
  const stats: { k: string; v: string; tone?: string }[] = [
    { k: "Trades", v: String(s.trades.length) },
    { k: "Open now", v: String(t.open), tone: t.open ? C.ember : undefined },
    { k: "Win rate", v: t.winRate === null ? "—" : `${t.winRate.toFixed(0)}%` },
    t.profit !== null && t.closed ? { k: "Realised P&L", v: usd(t.profit), tone: t.profit >= 0 ? C.up : C.down } : { k: "Closed pips", v: t.closed ? signed(t.pips) : "—", tone: t.pips > 0 ? C.up : t.pips < 0 ? C.down : undefined },
  ];
  const title = s.title.length > 48 ? `${s.title.slice(0, 47)}…` : s.title;

  return new ImageResponse(
    (
      <div style={{ width: "100%", height: "100%", display: "flex", flexDirection: "column", background: C.bg, color: C.fg, fontFamily: "sans-serif" }}>
        <div style={{ display: "flex", height: 6, background: `linear-gradient(90deg, ${C.ember}, ${C.gold} 60%, ${C.bg})` }} />
        <div style={{ display: "flex", flexDirection: "column", flex: 1, padding: "56px 72px 60px" }}>
          <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
            {brand}
            <div style={{ display: "flex", alignItems: "center", gap: 12, fontSize: 24, color: C.fg2, border: `2px solid ${C.line}`, borderRadius: 999, padding: "8px 22px" }}>
              {t.open ? <div style={{ width: 12, height: 12, borderRadius: 999, background: C.ember }} /> : null}
              {t.open ? "Live trades" : "Shared trades"}
            </div>
          </div>
          <div style={{ display: "flex", marginTop: 52, fontSize: 64, fontWeight: 700, letterSpacing: -1 }}>{title}</div>
          <div style={{ display: "flex", marginTop: 16, fontSize: 28, color: C.fg2, gap: 18 }}>
            <span>{s.alias}</span>
            {s.account ? <span style={{ color: C.fg3 }}>{s.account}</span> : null}
            <span style={{ color: C.gold }}>{symbols.join("  ")}</span>
          </div>
          <div style={{ display: "flex", marginTop: "auto", gap: 18 }}>
            {stats.map((x) => (
              <div key={x.k} style={{ display: "flex", flexDirection: "column", flex: 1, background: C.surface, border: `2px solid ${C.line}`, borderRadius: 20, padding: "22px 26px" }}>
                <div style={{ fontSize: 20, color: C.fg3, textTransform: "uppercase", letterSpacing: 2 }}>{x.k}</div>
                <div style={{ fontSize: 46, fontWeight: 700, marginTop: 6, color: x.tone ?? C.fg }}>{x.v}</div>
              </div>
            ))}
          </div>
        </div>
      </div>
    ),
    size,
  );
}
