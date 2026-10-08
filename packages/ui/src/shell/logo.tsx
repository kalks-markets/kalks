"use client";

import * as React from "react";
import { cn } from "../lib/cn";
import { useBrand } from "./brand";

/**
 * The founder's traced K (three shapes: upper arm, stem, lower leg; 653 × 541 units) and the "alks" of the
 * wordmark. Kalks 2 (docs/design/KALKS2.md §9): the K is a NeoPOP block — red face, yellow edge extruded down-right
 * by 30 units (≈ 5.5 % of its height), drawn as stacked copies so the edge is solid.
 */
export const K_PATH =
  "M 403.1 2.5 C 382.2 7.1, 370.8 13.7, 346.5 35.1 C 338.3 42.3, 320.9 57.4, 308 68.6 C 295.1 79.7, 271.2 100.4, 255 114.5 C 238.8 128.7, 217.9 146.8, 208.6 154.8 C 184.1 176, 178.5 183.3, 173.5 200.6 L 171 209.2 171 273.5 C 171 343.2, 170.7 339.4, 176.1 341.3 C 179.1 342.3, 187.4 337.4, 223.5 313.1 C 299.9 261.7, 352.8 224.5, 423 173 C 436.9 162.8, 455.2 149.4, 463.8 143.1 C 472.5 136.9, 484.4 128.1, 490.5 123.6 C 496.6 119, 506.2 111.9, 512 107.6 C 525.7 97.5, 590.5 48.6, 602.8 39.1 C 608.1 35, 617.8 27.6, 624.4 22.6 C 631.1 17.6, 640 10.7, 644.3 7.3 L 652.2 1 530.8 1.1 C 431.4 1.1, 408.3 1.4, 403.1 2.5 M 135.1 63.5 C 106.2 79.8, 63.8 105.7, 49.6 115.8 C 39.3 123.2, 23.9 137.8, 18.5 145.5 C 12.7 153.7, 7.6 164.2, 4.3 174.7 L 1.5 183.5 1.2 338.3 L 1 493.2 5.2 490.2 C 7.6 488.5, 21.9 479, 37 468.9 C 102.8 425.2, 131.2 400.6, 140.7 378.9 C 146.9 365, 146.5 375.3, 146.5 210.5 L 146.5 60.5 144 60.2 C 142.6 60.1, 138.7 61.5, 135.1 63.5 M 320 284.6 C 300.1 288.5, 288.5 295, 231.5 334.3 C 199.9 356.1, 192.9 361.3, 193.2 362.5 C 193.7 364.9, 322.1 498.5, 338.9 514.2 C 351.5 526, 366.5 534.2, 382.1 538 C 390.3 539.9, 393.7 540, 500.3 540 C 560.6 540, 610 539.7, 610 539.4 C 610 538.5, 568 494.1, 509 432.6 C 478.5 400.7, 447.7 368.6, 440.6 361.1 C 415.2 334.4, 386.4 305.7, 380 300.9 C 366.1 290.4, 351 285.2, 333 284.5 C 327.2 284.3, 321.4 284.4, 320 284.6";
export const ALKS_PATH =
  "M 1017.9 68.2 C 1017.9 77.7, 1017.9 153.2, 1017.9 236 C 1018 318.8, 1018 407.7, 1018 433.6 L 1018 480.7 1061.8 480.5 C 1085.8 480.4, 1109 480.3, 1113.3 480.2 L 1121 480 1120.8 265.8 L 1120.5 51.5 1069.3 51.2 L 1018 51 1017.9 68.2 M 1159 265.3 L 1159 479.7 1179.3 480.3 C 1205.1 481.2, 1234.3 481.2, 1249.8 480.3 L 1262 479.7 1262 409.7 L 1262 339.7 1269.8 347.1 C 1320.7 395.5, 1393.1 463.8, 1405.5 475 L 1411.5 480.5 1467 480.3 C 1497.5 480.3, 1529.3 480.1, 1537.5 480.1 L 1552.5 480 1547.5 475.2 C 1537.6 465.9, 1491 422.9, 1467 401 C 1423.5 361.3, 1420.6 358.7, 1401.2 341.1 C 1390.6 331.6, 1382 323.6, 1382 323.4 C 1382 323.2, 1388.3 318.3, 1395.9 312.6 C 1403.6 306.8, 1425.1 290.3, 1443.8 275.8 C 1497.9 233.8, 1518.7 217.8, 1542.6 199.5 L 1558.3 187.5 1484.1 187.2 L 1409.9 187 1405.2 190.8 C 1402.6 193, 1390.4 202.7, 1378 212.6 C 1365.6 222.4, 1345.9 238.3, 1334.1 248 C 1322.4 257.6, 1301.4 274.8, 1287.6 286.1 L 1262.5 306.7 1262.3 271.1 C 1262.1 251.5, 1261.9 194.1, 1261.8 143.5 L 1261.5 51.5 1210.3 51.2 L 1159 51 1159 265.3 M 1654.5 186.1 C 1619.5 189, 1590 206.4, 1571.3 235.2 C 1555.7 259.1, 1554.1 289.6, 1567.1 313.8 C 1579.6 337.2, 1602.5 355.7, 1630 364.9 C 1647.2 370.5, 1643.4 370.3, 1739.5 370.9 L 1828.5 371.5 1832.7 373.7 C 1840 377.7, 1842.5 382, 1842.5 391.1 C 1842.5 398.2, 1842.3 399.1, 1839.4 402.6 C 1837.8 404.7, 1834.6 407.3, 1832.4 408.4 C 1828.5 410.5, 1827.7 410.5, 1710 410.7 C 1644.8 410.9, 1588.5 411, 1584.8 411 L 1578 411 1578 445.4 L 1578 479.7 1591.8 480.4 C 1614.6 481.4, 1846.3 481.1, 1858 479.9 C 1904 475.5, 1941.5 445.5, 1951.1 405.5 C 1954.2 392.6, 1953.5 373, 1949.6 361.5 C 1940 333.1, 1916.5 312.6, 1884.3 304.4 C 1869 300.6, 1853 300, 1767 300.3 C 1723.3 300.5, 1686 300.3, 1684.2 299.9 C 1675.5 298, 1669.5 290.2, 1669.6 280.9 C 1669.7 274.2, 1671.8 270.1, 1677.3 265.6 L 1681.2 262.5 1804.6 262.2 L 1928 262 1928 224.1 L 1928 186.3 1910.8 185.6 C 1888.1 184.7, 1666.1 185.2, 1654.5 186.1 M 727 187.1 C 680.4 187.7, 674.5 188.2, 655.5 193.6 C 594.1 210.8, 551.3 254.1, 541.4 309 C 537.6 329.7, 539.7 357.9, 546.4 377.7 C 565.2 432.9, 625.4 475.3, 691.5 479.8 C 714.7 481.4, 778.2 480.6, 787.8 478.6 C 816.9 472.6, 835.6 461.3, 869.7 429.2 L 885 414.8 885.2 447.7 L 885.5 480.5 915 480.8 C 931.2 481, 952.7 480.8, 962.8 480.5 L 981 479.8 981 399.6 C 981 312.8, 980.6 304.1, 975.6 286.8 C 965 250.2, 937.6 219.9, 898.5 201.4 C 887.2 196, 868.5 190.4, 856.1 188.5 C 844.6 186.8, 792.5 186.2, 727 187.1 M 698.6 265.6 C 668.6 272.1, 644.9 292.2, 638.5 316.6 C 636.6 324, 636.6 337.6, 638.5 345.8 C 643.5 367, 662.1 386.1, 686.5 394.9 C 700.1 399.8, 705.7 400.2, 761 399.8 C 810 399.4, 811.7 399.3, 819.5 397.1 C 848.7 388.8, 871.4 368.7, 878.4 345.1 C 880.4 338.3, 880.5 324.3, 878.5 317 C 871.8 291.9, 848.8 272, 819.5 265.9 C 807.4 263.4, 710.1 263.2, 698.6 265.6";

/** Edge copies (x = y offset in glyph units). */
const EDGE = [5, 10, 15, 20, 25, 30];

/**
 * Colour sets by background: auto (red K, yellow edge, "alks" in the text colour: ink on light, warm white on
 * dark), on-red (ink K, yellow edge, white), on-yellow (red K, ink edge, ink), mono (all the current colour).
 */
export type LogoTone = "auto" | "on-red" | "on-yellow" | "mono";
const TONES: Record<LogoTone, { face: string; edge: string; word: string }> = {
  auto: { face: "var(--k-red, #D4112A)", edge: "var(--k-yellow, #FFD21F)", word: "currentColor" },
  "on-red": { face: "#0B0809", edge: "#FFD21F", word: "#FFFFFF" },
  "on-yellow": { face: "#D4112A", edge: "#0B0809", word: "#0B0809" },
  mono: { face: "currentColor", edge: "currentColor", word: "currentColor" },
};

function KGlyph({ id, tone, edge }: { id: string; tone: LogoTone; edge: boolean }) {
  const c = TONES[tone];
  return (
    <>
      <defs>
        <path id={id} d={K_PATH} />
      </defs>
      {edge && (
        <g fill={c.edge}>
          {EDGE.map((o) => (
            <use key={o} href={`#${id}`} x={o} y={o} />
          ))}
        </g>
      )}
      <use href={`#${id}`} fill={c.face} />
    </>
  );
}

/**
 * The wordmark. Kalks: the red K with its yellow edge + "alks" in the text colour (set `className="text-…"` to
 * recolour "alks"). Below 20 px the edge is dropped (flat red K). A white-label broker (BrandProvider): its logo
 * image, or its name set in type — the Kalks K never appears for a broker.
 */
export function Logo({ className, height = 22, tone = "auto" }: { className?: string; height?: number; tone?: LogoTone }) {
  const brand = useBrand();
  const id = React.useId();
  if (brand) {
    if (brand.logo_url) {
      return (
        // eslint-disable-next-line @next/next/no-img-element
        <img src={brand.logo_url} alt={brand.name} className={cn("inline-block w-auto max-w-[200px] shrink-0 object-contain object-left", className)} style={{ height }} />
      );
    }
    return (
      <span
        className={cn("inline-block max-w-[220px] shrink-0 truncate font-display font-bold leading-none tracking-[-0.02em] text-fg", className)}
        style={{ fontSize: Math.round(height * 0.82), lineHeight: `${height}px`, height }}
      >
        {brand.name}
      </span>
    );
  }
  const edge = height >= 20;
  // the box includes the edge (30 units below and right of the 1954 × 541 glyphs)
  const h = edge ? (height * 580) / 541 : height;
  const w = edge ? (height * 1990) / 541 : (height * 1954) / 541;
  return (
    <svg
      role="img"
      aria-label="Kalks"
      viewBox={edge ? "0 0 1990 580" : "0 0 1954 541"}
      width={w}
      height={h}
      className={cn("inline-block shrink-0 text-fg", className)}
    >
      <KGlyph id={`k${id.replace(/[^a-zA-Z0-9_-]/g, "")}`} tone={tone} edge={edge} />
      <path d={ALKS_PATH} fill={TONES[tone].word} fillRule="evenodd" />
    </svg>
  );
}

/**
 * The K alone (icon rail, compact headers, the Trader title bar). A broker: its logo, or its initial on its colour.
 */
export function LogoMark({ className, size = 22, tone = "auto" }: { className?: string; size?: number; tone?: LogoTone }) {
  const brand = useBrand();
  const id = React.useId();
  if (brand) {
    if (brand.logo_url) {
      return (
        // eslint-disable-next-line @next/next/no-img-element
        <img src={brand.logo_url} alt={brand.name} className={cn("inline-block shrink-0 object-contain", className)} style={{ height: size, width: size }} />
      );
    }
    return (
      <span
        role="img"
        aria-label={brand.name}
        className={cn("inline-grid shrink-0 place-items-center rounded-[6px] bg-red font-display font-bold leading-none text-on-red", className)}
        style={{ height: size, width: size, fontSize: Math.round(size * 0.58) }}
      >
        {brand.name.trim().charAt(0).toUpperCase()}
      </span>
    );
  }
  const edge = size >= 20;
  const h = edge ? (size * 580) / 541 : size;
  const w = edge ? (size * 690) / 541 : (size * 653) / 541;
  return (
    <svg role="img" aria-label="Kalks" viewBox={edge ? "0 0 690 580" : "0 0 653 541"} width={w} height={h} className={cn("inline-block shrink-0", className)}>
      <KGlyph id={`m${id.replace(/[^a-zA-Z0-9_-]/g, "")}`} tone={tone} edge={edge} />
    </svg>
  );
}
