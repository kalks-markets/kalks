// Contest rank widget helpers (lib/rank.ts, phase 4): `node --test apps/terminal/tests`.
// What it guards: the widget shows a running contest only (best rank first, then the one ending soonest), counts the
// trades still needed to rank, formats the time left, and keeps only the fields it needs from the service's answer
// (a login or a user id never reaches the browser).
import { test } from "node:test";
import assert from "node:assert/strict";
import { endsIn, pickContest, rankView, tradesToGo } from "../lib/rank.ts";

const c = (id, rank, endsAt, running = true, trades = 5, minTrades = 5) => ({ id, name: `C${id}`, scoring: "return_pct", endsAt, rank, ranked: 40, score: 1, trades, minTrades, running });

test("picks the running contest with the best rank, then the one ending first", () => {
  assert.equal(pickContest(null), null);
  assert.equal(pickContest([c(1, 1, "2026-10-30T00:00:00Z", false)]), null, "ended contests are not shown");
  assert.equal(pickContest([c(1, 9, "2026-10-30T00:00:00Z"), c(2, 3, "2026-11-30T00:00:00Z")]).id, 2);
  assert.equal(pickContest([c(1, null, "2026-10-30T00:00:00Z"), c(2, null, "2026-10-25T00:00:00Z")]).id, 2);
  assert.equal(pickContest([c(1, null, "2026-10-20T00:00:00Z"), c(2, 7, "2026-12-25T00:00:00Z")]).id, 2, "a ranked entry beats an unranked one");
});

test("trades to go and time left", () => {
  assert.equal(tradesToGo({ trades: 2, minTrades: 5 }), 3);
  assert.equal(tradesToGo({ trades: 9, minTrades: 5 }), 0);
  const now = Date.parse("2026-10-20T10:00:00Z");
  assert.equal(endsIn("2026-10-22T14:30:00Z", now), "2d 4h");
  assert.equal(endsIn("2026-10-20T15:12:00Z", now), "5h 12m");
  assert.equal(endsIn("2026-10-20T10:00:20Z", now), "1m");
  assert.equal(endsIn("2026-10-20T09:00:00Z", now), "");
  assert.equal(endsIn("nope", now), "");
});

test("the view keeps only what the widget shows", () => {
  const v = rankView({
    contests: [{ id: 3, name: "Weekly", scoring: "profit", endsAt: "2026-10-26T21:00:00Z", rank: 2, ranked: 18, score: 120.5, trades: 6, minTrades: 5, login: 50000123, userId: 42, running: true }],
    level: { xp: 340, level: 3, name: "Pro", badge: "silver", progressPct: 10, levels: [{ level: 1 }], next: { level: 4 } },
    updatedAt: "x",
  });
  assert.deepEqual(v, {
    contests: [{ id: 3, name: "Weekly", scoring: "profit", endsAt: "2026-10-26T21:00:00Z", rank: 2, ranked: 18, score: 120.5, trades: 6, minTrades: 5, running: true }],
    level: { level: 3, name: "Pro", badge: "silver", xp: 340, progressPct: 10 },
  });
  assert.ok(!JSON.stringify(v).includes("50000123") && !JSON.stringify(v).includes("userId"));
  assert.deepEqual(rankView(null), { contests: [], level: null });
  assert.deepEqual(rankView({ contests: "x", level: { name: 1 } }), { contests: [], level: null });
});
