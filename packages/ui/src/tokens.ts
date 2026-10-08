// Kalks 2 design tokens: the one source of truth (docs/design/KALKS2.md §2–§5, visual truth
// docs/design/kalks2-sample.html). `pnpm --filter @kalks/ui tokens` writes src/tokens.css from this file; the CSS,
// the Tailwind theme (styles.css), canvas charts (hex values below) and later the website preset and the app's
// tokens.dart all read the same numbers.
//
// Three-state theme: Auto follows the OS (prefers-color-scheme), Light / Dark override it via `data-theme` (or the
// legacy `.light` / `.dark` class) on <html> or on any element, so a subtree can be forced into one theme.
//
// Every accent has a face (fills, lines, candles: `bg-up`, `border-ember`) and a text tone (`text-up`, `text-ember`)
// that reads at AA on the surfaces; Tailwind maps `text-<accent>` to the text tone (styles.css, --text-color-*).

/** Brand constants: the same in both themes. */
export const BRAND = {
  red: "#D4112A", // Kalks red: the K face, primary buttons, sell face
  yellow: "#FFD21F", // Kalks yellow: the K edge, highlight buttons, AI card, active markers
  black: "#0B0809", // warm black: text on yellow, the dark canvas, the app icon, the icon rail
  white: "#FFFFFF",
  "neon-red": "#FF2D55", // notification / live dot only
  "neon-yellow": "#F4FF4A", // focus ring (dark), focus halo (light), equity fill (light)
  "img-red": "#E00302", // backdrop of the robot image (CSS extension colour)
  "img-yellow": "#FFD224", // backdrop of the figure image
} as const;

/**
 * Accent families a white-label broker replaces (brandCss in shell/brand-vars.ts rewrites these constants; the
 * theme roles below point at them). Kalks keeps the hand-tuned values of KALKS2 §2.
 */
export const ACCENTS = {
  "on-red": "#FFFFFF", // label on the red face (5.37 : 1)
  "on-yellow": "#0B0809", // label on the yellow face (13.8 : 1)
  "red-edge-light": "#8A0A1B",
  "red-edge-dark": "#5E0611",
  "red-tx-light": "#C8102E",
  "red-tx-dark": "#FF5A66",
  "red-soft-light": "#FFE1E1",
  "red-soft-dark": "rgba(212,17,42,.18)",
  "red-pastel-light": "#FFC9C9",
  "red-pastel-dark": "#3A0A12",
  "yellow-edge-light": "#B58C00",
  "yellow-edge-dark": "#9A7700",
  "yellow-tx-light": "#7A5D00",
  "yellow-tx-dark": "#FFD21F",
  "yellow-soft-light": "#FFF1A8",
  "yellow-soft-dark": "rgba(255,210,31,.12)",
} as const;

type Pair = readonly [light: string, dark: string];

/** Theme roles: [light, dark]. CSS variable = `--k-<name>`. */
export const ROLES = {
  // canvas and surfaces
  page: ["#EDE6E3", "#050404"], // behind framed layouts
  bg: ["#F6F1EE", "#0B0809"], // canvas
  "bg-2": ["#EEE7E3", "#070505"], // sunken: Trader canvas between panels, segmented track
  surface: ["#FFFFFF", "#141011"], // s1: cards, panels
  "surface-2": ["#FBF8F7", "#1B1517"], // s2: inputs, raised rows
  "surface-3": ["#F2ECE9", "#251C1F"], // s3: chips, hover
  "surface-4": ["#E9E1DE", "#30252A"], // s4: tracks (toggle off, slider)
  border: ["rgba(46,22,20,.08)", "rgba(255,236,230,.07)"], // hairlines
  "border-2": ["rgba(46,22,20,.15)", "rgba(255,236,230,.13)"], // control outlines
  // text
  fg: ["#160F0E", "#F6EEE8"],
  "fg-2": ["#5A4E4C", "#B8AAA5"],
  "fg-3": ["#6F6260", "#928380"],
  // red (brand, primary, errors) and yellow (highlight, active, new)
  "red-edge": ["var(--k-red-edge-light)", "var(--k-red-edge-dark)"],
  "red-tx": ["var(--k-red-tx-light)", "var(--k-red-tx-dark)"],
  "red-soft": ["var(--k-red-soft-light)", "var(--k-red-soft-dark)"],
  "red-pastel": ["var(--k-red-pastel-light)", "var(--k-red-pastel-dark)"], // deeper tint; dark = wine #3A0A12
  "yellow-edge": ["var(--k-yellow-edge-light)", "var(--k-yellow-edge-dark)"],
  "yellow-tx": ["var(--k-yellow-tx-light)", "var(--k-yellow-tx-dark)"],
  "yellow-soft": ["var(--k-yellow-soft-light)", "var(--k-yellow-soft-dark)"],
  // ink: the strong secondary button face (near-black in light, warm white in dark), toggle-on in light
  ink: ["#231B1C", "#F6EEE8"],
  "ink-edge": ["#000000", "#7D6F6B"],
  "ink-fg": ["#FFFFFF", "#0B0809"],
  "white-edge": ["#CDBFBB", "#9E928E"], // white button (on photos)
  // up = blue (buy, profit, TP) — everywhere, Client Area included
  up: ["#1F5FE0", "#2F7BFF"], // candles, lines, bars
  "up-face": ["#1F5FE0", "#1F62EA"], // Buy face (white label 5.57 / 5.26)
  "up-edge": ["#0B2F80", "#0A2C78"],
  "up-tx": ["#1A56D6", "#5B97FF"], // profit text
  "up-soft": ["#E3ECFF", "rgba(47,123,255,.15)"],
  // down = red (sell, loss, SL)
  down: ["#E0182F", "#F23645"],
  "down-face": ["var(--k-sell)", "var(--k-sell)"],
  "down-edge": ["#8A0A1B", "#5E0611"],
  "down-tx": ["#C8102E", "#FF5A66"],
  "down-soft": ["#FFE4E4", "rgba(242,54,69,.15)"],
  // statuses: green only for "Completed"-type states
  ok: ["#137A45", "#3CCB7F"],
  "ok-soft": ["#DCF2E5", "rgba(60,203,127,.14)"],
  warn: ["#A87F00", "#FFD21F"], // dots, bars
  "warn-tx": ["#7A5D00", "#FFD21F"],
  "warn-soft": ["#FFF1A8", "rgba(255,210,31,.13)"],
  // glass: frosted, never see-through (blur 26 + saturate 160 %)
  glass: ["rgba(255,255,255,.62)", "rgba(22,13,15,.56)"],
  "glass-strong": ["rgba(255,255,255,.86)", "rgba(18,11,13,.72)"], // strips over photos
  "glass-line": ["rgba(255,255,255,.8)", "rgba(255,255,255,.09)"],
  // focus: 2 px ring; light adds the neon-yellow halo (its neon touch). Never red (reads as an error).
  focus: ["#160F0E", "#F4FF4A"],
  "focus-halo": ["#F4FF4A", "rgba(244,255,74,.16)"],
  "accent-line": ["#160F0E", "#FFD21F"], // selected underline, active marker
  "accent-fill": ["#F4FF4A", "#FFD21F"], // equity fill
  // elevation (controls never use these: they have NeoPOP edges)
  "shadow-card": [
    "0 1px 2px rgba(40,18,16,.04), 0 12px 32px -14px rgba(40,18,16,.14)",
    "inset 0 1px 0 rgba(255,255,255,.025), 0 24px 48px -28px rgba(0,0,0,.7)",
  ],
  "shadow-pop": ["0 30px 80px -24px rgba(40,18,16,.30)", "0 40px 100px -20px rgba(0,0,0,.75)"],
  "bloom-a": ["rgba(255,186,186,.75)", "rgba(212,17,42,.26)"], // pastel red top-right / red glow
  "bloom-b": ["rgba(255,238,150,.7)", "rgba(255,210,31,.06)"], // pastel yellow bottom-left
  // charts
  "chart-bg": ["#FFFFFF", "#0E0A0B"],
  grid: ["rgba(46,22,20,.055)", "rgba(255,236,230,.05)"],
  axis: ["#6F6260", "#928380"],
  ma: ["#C99A00", "#FFD21F"], // moving averages, equity line (dark)
  draw: ["#5A4E4C", "#B8AAA5"], // user drawings
  // controls
  "tog-on": ["#160F0E", "#FFD21F"], // toggle on, slider fill
  "seg-edge": ["#D9CCC8", "#000000"], // raised segmented item
  "puck-edge": ["#C9BCB8", "#8E817D"], // toggle / slider puck
  // phone tab bar (ink in both themes)
  tabbar: ["#160F0E", "#1B1517"],
  "tab-ic": ["#B8AAA5", "#928380"],
  "tab-on": ["#FFFFFF", "#FFD21F"],
  "tab-on-ic": ["#160F0E", "#0B0809"],
} as const satisfies Record<string, Pair>;

/**
 * Names the apps used before Kalks 2, kept so every page restyles at once (R1): old name → new value. Values may
 * point at roles above. Remove an alias once no page uses it.
 */
export const LEGACY: Record<string, Pair> = {
  "border-top": ["var(--k-border-2)", "var(--k-border-2)"], // was the inset top highlight; now the outline hairline
  ember: ["var(--k-red)", "var(--k-red)"], // the action colour (brokers: their primary)
  "ember-2": ["var(--k-red-tx)", "var(--k-red-tx)"],
  "ember-tx": ["var(--k-red-tx)", "var(--k-red-tx)"],
  "ember-soft": ["var(--k-red-soft)", "var(--k-red-soft)"],
  "on-ember": ["var(--k-on-red)", "var(--k-on-red)"],
  gold: ["var(--k-yellow)", "var(--k-yellow)"],
  "gold-tx": ["var(--k-yellow-tx)", "var(--k-yellow-tx)"],
  "gold-soft": ["var(--k-yellow-soft)", "var(--k-yellow-soft)"],
  info: ["var(--k-up)", "var(--k-up)"],
  "info-tx": ["var(--k-up-tx)", "var(--k-up-tx)"],
  "info-soft": ["var(--k-up-soft)", "var(--k-up-soft)"],
  wine: ["var(--k-red-pastel)", "var(--k-red-pastel)"],
  "card-grad": ["linear-gradient(transparent, transparent)", "linear-gradient(transparent, transparent)"],
  "glow-strength": [".35", ".8"],
};

/** Non-colour constants (same in both themes). */
export const CONSTANTS = {
  sell: "#D4112A", // Sell face: the brand red, fixed (a broker's primary never turns Sell into another colour)
  // motion (KALKS2 §7)
  "ease-press": "cubic-bezier(.3,.7,.4,1)",
  "ease-spring": "cubic-bezier(.3,1.4,.5,1)",
  "ease-out": "cubic-bezier(.2,.8,.2,1)",
  "dur-press": "90ms",
  "dur-hover": "120ms",
  "dur-toggle": "220ms",
  "dur-seg": "180ms",
  "dur-sheet": "320ms",
  "dur-toast": "360ms",
  "dur-theme": "250ms",
  // glass recipe
  "glass-blur": "blur(26px) saturate(160%)",
} as const;

/** Radius scale (KALKS2 §4) → Tailwind `rounded-<name>`. */
export const RADIUS = {
  tag: "6px",
  tool: "8px",
  "btn-sm": "10px",
  btn: "12px",
  seg: "13px",
  "btn-lg": "14px",
  panel: "14px",
  "btn-xl": "16px",
  strip: "20px",
  card: "24px",
  hero: "28px",
  frame: "30px",
} as const;

/** Type scale (KALKS2 §3) → Tailwind `text-<name>` (size / line-height). */
export const TYPE = {
  "display-xl": ["80px", "0.9"],
  "display-l": ["56px", "0.95"],
  "display-m": ["46px", "0.95"],
  title: ["34px", "1"],
  "card-title": ["20px", "1"],
  "money-xl": ["34px", "1"],
  "money-l": ["30px", "1"],
  "money-m": ["27px", "1"],
  "body-l": ["21px", "1.42"],
  body: ["15px", "1.5"],
  small: ["13.5px", "1.4"],
  label: ["12.5px", "1.2"],
  th: ["11px", "1"],
  kicker: ["12px", "1"],
} as const;

/** NeoPOP button sizes (KALKS2 §5): height, edge depth, radius, padding-x, label size, icon size. */
export const BUTTON_SIZES = {
  28: { h: 28, d: 2, r: 9, px: 10, fs: 12.5, icon: 15 }, // dense tables (Back Office)
  32: { h: 32, d: 3, r: 10, px: 12, fs: 13, icon: 16 },
  40: { h: 40, d: 4, r: 12, px: 16, fs: 14, icon: 18 },
  48: { h: 48, d: 5, r: 14, px: 20, fs: 15, icon: 18 },
  56: { h: 56, d: 6, r: 16, px: 26, fs: 16, icon: 20 },
} as const;

/** Resolved hex/rgba value of a role in a theme (canvas charts, emails, the app's tokens). */
export function tokenValue(name: keyof typeof ROLES, theme: "light" | "dark"): string {
  const v: string = ROLES[name][theme === "light" ? 0 : 1];
  const m = /^var\(--k-([a-z0-9-]+)\)$/.exec(v);
  if (!m) return v;
  const key = m[1] as string;
  if (key in ACCENTS) return ACCENTS[key as keyof typeof ACCENTS];
  if (key === "sell") return CONSTANTS.sell;
  return v;
}
