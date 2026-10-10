// The app's section photos (founder 2026-10-10): the founder's own portraits from "brand images user dashboard/" at
// their native size (the web's apps/crm/public/heroes are 2800 x 1120 widened copies of the same pictures; a phone
// shows them portrait), plus a tiny pre-blurred copy of each for the page backdrop. Run from the repo root:
//
//   node apps/mobile/tool/heroes.mjs
//
// The section -> picture map follows the web's (components/page-hero.tsx photoForPath). The Dashboard's picture is
// the landscape KALKS poster ("home t.png") set full width on a portrait canvas, its own dark red extended above and
// below, so the letters stay whole on a phone.
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const app = join(dirname(fileURLToPath(import.meta.url)), "..");
const root = join(app, "..", "..");
const require = createRequire(join(root, "package.json"));
const sharp = require("sharp");
const B = join(root, "brand images user dashboard");
const OUT = join(app, "assets", "heroes");
const SRC = {
  accounts: "5a3c99dd862a1c8503877a183fa09a19.jpg",
  wallet: "c8753928adf3edff14a59a9c3bcd24bb.jpg",
  portfolio: "650a365e9399a0b891f7d388c2bbff9f.jpg",
  copy: "f664337f76ee3ce44c524c1d7c1815fc.jpg",
  prop: "4318b0a02575658a4d065e5810abb1ab.jpg",
  markets: "3a9ab0e5bb0813957ba88bdd54add6d1.jpg",
  options: "5b553c0d613d0102beeb212d2cc6034d.jpg",
  partner: "be0c4021f3541b76b2949e6784e4ba0c.jpg",
  rewards: "4b35dabc55a40f6f235baf776edc7bf1.jpg",
  academy: "download (1).png",
  profile: "81be2aeaf8155ebfdfc2618830212fb1.jpg",
  support: "home page.jpg",
  news: "8674fbbe770054bc680b50f66de61206.jpg",
};
const jpeg = { quality: 82, mozjpeg: true, chromaSubsampling: "4:2:0" };
for (const [page, f] of Object.entries(SRC)) {
  const img = sharp(`${B}/${f}`).removeAlpha();
  const m = await img.metadata();
  await img.jpeg(jpeg).toFile(`${OUT}/${page}.jpg`);
  console.log(page, m.width, m.height);
}
// Dashboard: the landscape KALKS poster on a portrait canvas, full width, its own dark red extended above and below.
{
  const W = 1080, H = 1240, TOP = 170;
  const f = `${B}/home t.png`;
  const ih = Math.round((W * 941) / 1672);
  const pic = await sharp(f).removeAlpha().resize(W, ih, { kernel: "lanczos3" }).png().toBuffer();
  const t0 = TOP / H, t1 = (TOP + ih) / H;
  const bg = Buffer.from(`<svg width="${W}" height="${H}"><defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="#120000"/><stop offset="${t0}" stop-color="#390101"/><stop offset="${t1}" stop-color="#290101"/><stop offset="1" stop-color="#000000"/></linearGradient></defs>
    <rect width="${W}" height="${H}" fill="url(#g)"/></svg>`);
  const F = 90;
  const feather = Buffer.from(`<svg width="${W}" height="${H}"><defs>
    <linearGradient id="a" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#390101" stop-opacity="1"/><stop offset="1" stop-color="#390101" stop-opacity="0"/></linearGradient>
    <linearGradient id="b" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#290101" stop-opacity="0"/><stop offset="1" stop-color="#290101" stop-opacity="1"/></linearGradient></defs>
    <rect x="0" y="${TOP}" width="${W}" height="${F}" fill="url(#a)"/><rect x="0" y="${TOP + ih - F}" width="${W}" height="${F}" fill="url(#b)"/></svg>`);
  await sharp(bg).composite([{ input: pic, left: 0, top: TOP }, { input: feather, left: 0, top: 0 }]).jpeg(jpeg).toFile(`${OUT}/dashboard.jpg`);
  console.log("dashboard", W, H);
}
// Backdrops: each photo blurred to a few colours, tiny (drawn full screen under a black wash).
for (const page of [...Object.keys(SRC), "dashboard"]) {
  await sharp(`${OUT}/${page}.jpg`).resize(48, 84, { fit: "cover" }).blur(5).jpeg({ quality: 70 }).toFile(`${OUT}/blur/${page}.jpg`);
}
console.log("done");
