// Runs the chart page's script (assets/chart/chart.html) in Node with a stand-in for lightweight-charts and the DOM,
// so tests can check what the page draws for a sequence of app commands (chart_page_test.dart).
//
//   node test/terminal/chart_page_harness.mjs <steps.json>
//
// steps: [{"recv": <command>} | {"batch": [<command>, …]} | {"frames": n, "dt": ms}] — commands as the app sends
// them (chart_bridge.dart ChartCmd); "frames" runs n animation frames dt ms apart (the glide). Prints the main
// series' drawn points as {"<time>": [open, high, low, close]} after the last step (what is on screen).
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const html = readFileSync(join(here, "..", "..", "assets", "chart", "chart.html"), "utf8");
const scripts = [...html.matchAll(/<script>([\s\S]*?)<\/script>/g)].map((m) => m[1]);
const page = scripts[scripts.length - 1];
const steps = JSON.parse(readFileSync(process.argv[2], "utf8"));

// anything: a callable, chainable no-op (DOM nodes, chart parts the test does not look at)
const anything = () =>
  new Proxy(function () {}, {
    get: (t, p) => (p === Symbol.toPrimitive ? () => 0 : p === "then" ? undefined : p in t ? t[p] : (t[p] = anything())),
    set: (t, p, v) => ((t[p] = v), true),
    apply: () => anything(),
  });

const drawn = new Map();
const series = (record) => {
  const s = anything();
  s.setData = (pts) => {
    if (!record) return;
    drawn.clear();
    for (const p of pts) drawn.set(p.time, p);
  };
  s.update = (p) => {
    if (!record) return;
    const times = [...drawn.keys()];
    const last = times.length ? times[times.length - 1] : -Infinity;
    if (p.time < last) throw new Error(`update of an older bar ${p.time} < ${last}`);
    drawn.set(p.time, p);
  };
  return s;
};
let added = 0;
const chart = anything();
chart.addSeries = () => series(added++ % 2 === 0); // the main series, then volume (each init adds both again)
chart.panes = () => [anything()];
const LightweightCharts = anything();
LightweightCharts.createChart = () => {
  added = 0;
  return chart;
};

let rafs = [];
let now = 1000;
const g = globalThis;
g.window = g;
g.LightweightCharts = LightweightCharts;
g.KalksInd = undefined;
g.document = anything();
g.document.hidden = false;
g.matchMedia = () => ({ matches: false });
g.requestAnimationFrame = (f) => (rafs.push(f), rafs.length);
g.cancelAnimationFrame = () => {};
g.setInterval = () => 0;
g.addEventListener = () => {};
g.KalksChart = { postMessage() {} };
g.Path2D = function () {};

new Function(page)();

for (const s of steps) {
  if (s.recv) g.K.recv(JSON.stringify(s.recv));
  if (s.batch) g.K.batch(JSON.stringify(s.batch));
  if (s.frames) {
    for (let i = 0; i < s.frames; i++) {
      now += s.dt ?? 16;
      const run = rafs;
      rafs = [];
      for (const f of run) f(now);
    }
  }
}
const out = {};
for (const [t, p] of drawn) out[t] = [p.open, p.high, p.low, p.close];
process.stdout.write(JSON.stringify(out));
