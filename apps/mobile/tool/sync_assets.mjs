// Copies the shared Kalks assets into the Flutter app and draws its brand images. Run from the repo root after the
// repo's `pnpm install` (sharp, geist and flag-icons come from the root node_modules):
//
//   node apps/mobile/tool/sync_assets.mjs            (everything)
//   node apps/mobile/tool/sync_assets.mjs --brand    (the brand images only)
//   cd apps/mobile && dart run flutter_launcher_icons && dart run flutter_native_splash:create
//
// Writes (everything below apps/mobile, rewritten on every run):
// - assets/brand/: the logos (kalks-logo.svg, kalks-mark.svg, kalks_logo_black.png, kalks_logo_white.png) and the
//   images drawn from the Kalks mark: the real monochrome logo since 2026-10-10, a white K on black (the Client
//   Area's apps/crm/public/brand/kalks-icon.svg), after the old Expo app's recipe (git show
//   df273f4^:apps/mobile/scripts/brand-assets.mjs):
//     icon.png                     1024 px, opaque: the launcher icon (legacy Android, web)
//     adaptive-icon.png            1024 px, transparent: the adaptive icon's foreground (background is #000000)
//     adaptive-icon-monochrome.png 1024 px, white on transparent: Android 13 themed icons
//     splash.png                   600 px wide white mark: the launch screen before Android 12 (xxxhdpi)
//     splash-android12.png         1152 px, the white mark inside the 768 px circle Android 12+ shows
// - android/app/src/main/res/drawable-*dpi/ic_stat_kalks.png: the status-bar notification icon (white mark on
//   transparent, 24 dp)
// - assets/coins/*.svg, assets/stocks/*.svg, assets/people/*.jpg: copies of the repo assets/
// - assets/flags/<cc>.png: round-ready square flags (flag-icons 1x1, the web's `fi fis`) as 72 px PNGs
// - assets/fonts/: Geist and Geist Mono (Regular, Medium, SemiBold, Bold) with their licence (OFL). Plus Jakarta Sans
//   is committed in assets/fonts/ already (static instances of google/fonts ofl/plusjakartasans, OFL).
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const app = join(dirname(fileURLToPath(import.meta.url)), "..");
const root = join(app, "..", "..");
const require = createRequire(join(root, "package.json"));
const sharp = require("sharp");

// the Client Area's icon (apps/crm/public/brand/kalks-icon.svg): the white K on black, flat fills
const BG = "#000000";
const MARK = "#FFFFFF";
const brandOnly = process.argv.includes("--brand");

const out = (...p) => join(app, ...p);
const fresh = (dir) => {
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
};

/* ---------------- brand ---------------- */

const brandDir = out("assets", "brand");
fresh(brandDir);
for (const f of ["kalks-logo.svg", "kalks-mark.svg"]) copyFileSync(join(root, "assets", "brand", f), join(brandDir, f));
for (const f of ["kalks_logo_black.png", "kalks_logo_white.png"]) copyFileSync(join(root, "brand", f), join(brandDir, f));

const mark = readFileSync(join(root, "assets", "brand", "kalks-mark.svg"), "utf8");
const colored = (c) => Buffer.from(mark.replace('fill="currentColor"', `fill="${c}"`));

async function onSquare(size, bg, markColor, markScale) {
  const w = Math.round(size * markScale);
  const m = await sharp(colored(markColor), { density: 600 }).resize({ width: w }).png().toBuffer();
  const meta = await sharp(m).metadata();
  const base = sharp({ create: { width: size, height: size, channels: 4, background: bg } });
  return base.composite([{ input: m, left: Math.round((size - w) / 2), top: Math.round((size - meta.height) / 2) }]).png();
}
const CLEAR = { r: 0, g: 0, b: 0, alpha: 0 };

// launcher icon: opaque, the white mark on black (the web icon's proportions: the K 60 % of the square)
await sharp(await (await onSquare(1024, BG, MARK, 0.6)).toBuffer()).removeAlpha().png().toFile(join(brandDir, "icon.png"));
// adaptive icon foreground: the mark alone (inside the 66 % safe zone), on the #000000 background layer
await (await onSquare(1024, CLEAR, MARK, 0.42)).toFile(join(brandDir, "adaptive-icon.png"));
await (await onSquare(1024, CLEAR, "#FFFFFF", 0.42)).toFile(join(brandDir, "adaptive-icon-monochrome.png"));
// launch screen: the white mark on black
await sharp(colored(MARK), { density: 600 }).resize({ width: 600 }).png().toFile(join(brandDir, "splash.png"));
// Android 12+: a 1152 px icon whose visible part is the centre 768 px circle
await (await onSquare(1152, CLEAR, MARK, 0.4)).toFile(join(brandDir, "splash-android12.png"));

// status-bar notification icon: white mark on transparent, 24 dp per density
const DENSITIES = { mdpi: 24, hdpi: 36, xhdpi: 48, xxhdpi: 72, xxxhdpi: 96 };
for (const [d, px] of Object.entries(DENSITIES)) {
  const dir = out("android", "app", "src", "main", "res", `drawable-${d}`);
  mkdirSync(dir, { recursive: true });
  await (await onSquare(px, CLEAR, "#FFFFFF", 0.8)).toFile(join(dir, "ic_stat_kalks.png"));
}
if (brandOnly) {
  console.log("brand images written");
  process.exit(0);
}

/* ---------------- shared pictures ---------------- */

for (const [from, to, ext] of [
  [join(root, "assets", "coins"), out("assets", "coins"), ".svg"],
  [join(root, "assets", "stocks"), out("assets", "stocks"), ".svg"],
  [join(root, "assets", "people"), out("assets", "people"), ".jpg"],
]) {
  fresh(to);
  for (const f of readdirSync(from).filter((x) => x.endsWith(ext))) copyFileSync(join(from, f), join(to, f));
}

/* ---------------- flags ---------------- */

const flagSrc = join(dirname(require.resolve("flag-icons/package.json", { paths: [join(root, "packages", "ui")] })), "flags", "1x1");
const flagDir = out("assets", "flags");
fresh(flagDir);
let flags = 0;
for (const f of readdirSync(flagSrc).filter((x) => x.endsWith(".svg"))) {
  await sharp(join(flagSrc, f), { density: 300 }).resize(72, 72).png({ palette: true, quality: 95, compressionLevel: 9 }).toFile(join(flagDir, f.replace(/\.svg$/, ".png")));
  flags++;
}

/* ---------------- fonts ---------------- */

const fontDir = out("assets", "fonts");
mkdirSync(fontDir, { recursive: true });
// geist's package.json is not exported: take the package folder the Client Area links to (pnpm)
const geist = join(root, "apps", "crm", "node_modules", "geist");
const WEIGHTS = ["Regular", "Medium", "SemiBold", "Bold"];
for (const w of WEIGHTS) {
  copyFileSync(join(geist, "dist", "fonts", "geist-sans", `Geist-${w}.ttf`), join(fontDir, `Geist-${w}.ttf`));
  copyFileSync(join(geist, "dist", "fonts", "geist-mono", `GeistMono-${w}.ttf`), join(fontDir, `GeistMono-${w}.ttf`));
}
copyFileSync(join(geist, "LICENSE.txt"), join(fontDir, "Geist-OFL.txt"));
if (!existsSync(join(fontDir, "PlusJakartaSans-Regular.ttf"))) console.warn("assets/fonts: Plus Jakarta Sans is missing (see the header)");

console.log(`brand images, ${flags} flags, coins, stocks, people and Geist fonts written`);
