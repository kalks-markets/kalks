// Kalks 2 brand files (docs/design/KALKS2.md §9) from the founder's traced wordmark (assets/brand/kalks-logo.svg):
// the K as a NeoPOP block (red face, yellow edge extruded 30 units down-right), the lock-ups per background, the app
// icon and the web favicons. Run `node scripts/brand-icons.mjs` after changing the source; `pnpm sync-assets` copies
// assets/ into the apps. kalks-logo.svg / kalks-mark.svg stay the one-colour originals (CSS masks, QR codes).
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import sharp from "sharp";

const dir = join(dirname(fileURLToPath(import.meta.url)), "..", "assets", "brand");
const src = readFileSync(join(dir, "kalks-logo.svg"), "utf8");
const d = /d="([^"]+)"/.exec(src)[1];
// the K is the three subpaths starting M 403.113 2.545 (upper arm), M 135.134 63.542 (stem), M 320 284.624 (leg)
const subs = d.split(/\s(?=M\s)/);
const isK = (p) => /^M (403\.113 2\.545|135\.134 63\.542|320 284\.624)/.test(p);
const K = subs.filter(isK).join(" ");
const ALKS = subs.filter((p) => !isK(p)).join(" ");

const RED = "#D4112A";
const YELLOW = "#FFD21F";
const BLACK = "#0B0809";
const WARM_WHITE = "#F6EEE8";
const EDGE = 30; // units, ≈ 5.5 % of the 541-unit glyph

/** The K: `edge` stacked 1-unit copies (a solid extrusion with diagonal joins), then the face. */
function kBlock({ face = RED, edge = YELLOW, x = 0, y = 0, scale = 1, flat = false } = {}) {
  const copies = flat ? "" : Array.from({ length: EDGE }, (_, i) => `<use href="#k" x="${i + 1}" y="${i + 1}"/>`).join("");
  return `<g transform="translate(${x} ${y}) scale(${scale})">${flat ? "" : `<g fill="${edge}">${copies}</g>`}<use href="#k" fill="${face}"/></g>`;
}
const defs = `<defs><path id="k" d="${K}"/></defs>`;
const svg = (w, h, body, extra = "") => `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${w} ${h}" width="${w}" height="${h}"${extra}>${defs}${body}</svg>\n`;

const files = {
  // the mark alone, transparent background
  "kalks-k.svg": svg(690, 580, kBlock()),
  // lock-ups (KALKS2 §9 table): on dark / on light / on red / on yellow, transparent background
  "kalks-logo-dark.svg": svg(1990, 580, kBlock() + `<path fill="${WARM_WHITE}" fill-rule="evenodd" d="${ALKS}"/>`),
  "kalks-logo-light.svg": svg(1990, 580, kBlock() + `<path fill="${BLACK}" fill-rule="evenodd" d="${ALKS}"/>`),
  "kalks-logo-on-red.svg": svg(1990, 580, kBlock({ face: BLACK, edge: YELLOW }) + `<path fill="#FFFFFF" fill-rule="evenodd" d="${ALKS}"/>`),
  "kalks-logo-on-yellow.svg": svg(1990, 580, kBlock({ face: RED, edge: BLACK }) + `<path fill="${BLACK}" fill-rule="evenodd" d="${ALKS}"/>`),
};

// app icon: black squircle, the K at 60 % width, the block (face + edge) centred = the face nudged up-left by half the edge
function appIcon(size, { radius = 0.225, flat = false } = {}) {
  const s = (size * 0.6) / 653;
  const bw = (653 + (flat ? 0 : EDGE)) * s;
  const bh = (541 + (flat ? 0 : EDGE)) * s;
  const r = radius * size;
  return svg(size, size, `<rect width="${size}" height="${size}" rx="${r}" fill="${BLACK}"/>` + kBlock({ x: (size - bw) / 2, y: (size - bh) / 2, scale: s, flat }));
}
files["kalks-app-icon.svg"] = appIcon(1024);

for (const [name, body] of Object.entries(files)) writeFileSync(join(dir, name), body);

// PNGs: favicons (32 px = the K with its edge on a black rounded square; 16 px = the flat red K on black), the Apple
// touch icon (full-bleed: iOS rounds it), and 192 / 512 for manifests
const png = async (svgText, size, out) => sharp(Buffer.from(svgText), { density: 72 }).resize(size, size).png({ compressionLevel: 9 }).toFile(join(dir, out));
await png(appIcon(512, { radius: 0.22 }), 32, "favicon-32.png");
await png(appIcon(256, { radius: 0.25, flat: true }), 16, "favicon-16.png");
await png(appIcon(1024, { radius: 0 }), 180, "apple-touch-icon.png");
await png(appIcon(1024, { radius: 0.225 }), 192, "icon-192.png");
await png(appIcon(1024, { radius: 0.225 }), 512, "icon-512.png");
console.log("brand files written:", [...Object.keys(files), "favicon-32.png", "favicon-16.png", "apple-touch-icon.png", "icon-192.png", "icon-512.png"].join(", "));
