# Kalks 2 — style sheet (R0 direction)

Status: **direction for the founder's yes** (Track 4 · R0, 9 Oct 2026). Nothing in the apps uses this yet; R1 turns it
into `packages/ui` tokens/components, the website preset and `apps/mobile/lib/ui/tokens.dart`.

- Live sample: [`kalks2-sample.html`](kalks2-sample.html) (open in Chrome; Auto / Light / Dark switch top right).
- Images used by the sample: [`img/`](img/) · image brief for new art: [`IMAGE-BRIEF.md`](IMAGE-BRIEF.md).

Replaces: ember orange, Geist / Geist Mono, Plus Jakarta Sans, the pastel-orange Client Area, green-up candles in
Kalks Trader. The client's copy of the old platform keeps all of that, so nothing below may look like it.

---

## 1. Principles

1. **The picture carries the brand.** One striking subject on one flat, saturated colour (robot on red, figure on
   yellow). The interface around it stays quiet: black, white, warm greys.
2. **Controls you press.** Buttons are NeoPOP blocks — a flat face on a solid edge that the face sinks into. No blur
   shadows on controls, no glow rings.
3. **Colour means something.** Red = brand and the sell / down / loss side. Blue = up / buy / profit / TP. Yellow =
   active, new, highlight (Deposit, AI, the selected tab). Green appears only in status chips ("Completed").
4. **One saturated action per panel.** Everything else is ink, ghost or text.
5. **Money is typography.** Big numbers in the display face with tabular figures, cents set smaller; prices in mono.
6. **Glass only where something is behind it** — over a photo, over the canvas blooms, over scrolling content.
   Glass is frosted (blur 26, saturate 160 %, ≥ 56 % fill), never see-through.
7. **Compact.** Founder rule: no oversized buttons; trade buttons ≤ 44 px total; phone touch targets ≥ 40 px.

Where each founder reference went:

| Reference | What we took | Where |
|---|---|---|
| Channel Analytics (dark glass) | photo hero card + glass stat strip, black icon rail, radius 28, thin yellow chart line | Client Area dashboard |
| Neo-Tactile UI kit | tactile toggles / sliders / segmented with a raised thumb | all controls (with NeoPOP edges, without the glow) |
| Santorini travel (light) | pill tabs inside the photo hero, yellow assistant card, pale canvas | Client Area light, Ask Kalks AI card |
| Exness terminal | blue/red candles, symbol tabs with flags, chart bar with inline Sell/Buy, drawing rail, range bar + UTC clock | Kalks Trader |
| CRED NeoPOP | flat face + solid bottom-right edge, press = translate into the edge | every button, the K logo itself |
| Founder's solid-colour images | one subject, one flat backdrop | every hero; [`IMAGE-BRIEF.md`](IMAGE-BRIEF.md) |

---

## 2. Colour

### 2.1 Brand constants (same in both themes)

| Token | Hex | Role |
|---|---|---|
| `k-red` | `#D4112A` | Kalks red: the K face, primary buttons, sell face |
| `k-yellow` | `#FFD21F` | Kalks yellow: the K edge, highlight buttons, AI card, active markers |
| `k-ink` | `#0B0809` | warm black: text on yellow, the dark canvas, app icon |
| `neon-red` | `#FF2D55` | notification dot, live dot — tiny accents only |
| `neon-yellow` | `#F4FF4A` | focus ring (dark), focus halo (light), equity-chart fill (light) |
| `img-red` | `#E00302` | backdrop of the robot image (CSS extension colour) |
| `img-yellow` | `#FFD224` | backdrop of the figure image (CSS extension colour) |

### 2.2 Dark theme — black / dark red / yellow

| Token | Hex | Role |
|---|---|---|
| `bg` | `#0B0809` | canvas |
| `bg2` | `#070505` | sunken (Trader canvas between panels, segmented track) |
| `s1` | `#141011` | cards, panels |
| `s2` | `#1B1517` | inputs, raised rows |
| `s3` | `#251C1F` | chips, hover |
| `s4` | `#30252A` | tracks (toggle off, slider) |
| `line` / `line2` | `rgba(255,236,230,.07)` / `.13` | hairlines / control outlines |
| `tx` | `#F6EEE8` | primary text (warm white) |
| `tx2` | `#B8AAA5` | secondary text |
| `tx3` | `#928380` | labels, axis, timestamps |
| `wine` | `#3A0A12` | dark-red surfaces (avatar, red-tinted tiles); bloom `rgba(212,17,42,.26)` |
| `red` / `red-edge` | `#D4112A` / `#5E0611` | primary face / edge |
| `red-tx` | `#FF5A66` | red text: loss, sell, errors |
| `yellow` / `yellow-edge` | `#FFD21F` / `#9A7700` | highlight face / edge; also toggle-on, slider fill, equity line |
| `up` (graphics) | `#2F7BFF` | up candles, buy line, TP line, blue chart marks |
| `up-face` / `up-edge` | `#1F62EA` / `#0A2C78` | Buy button face / edge (white label) |
| `up-tx` | `#5B97FF` | profit text, positive change |
| `dn` (graphics) | `#F23645` | down candles, SL line |
| `ok` | `#3CCB7F` | status "Completed" only |
| `focus` | `#F4FF4A` | 2 px focus ring |
| `glass` | `rgba(22,13,15,.56)` (strip over photos `.72`) | frosted panels |

### 2.3 Light theme — pastel red + pastel yellow, neon touches

| Token | Hex | Role |
|---|---|---|
| `bg` | `#F6F1EE` | canvas (warm off-white) with two blooms: pastel red top-right `rgba(255,186,186,.75)`, pastel yellow bottom-left `rgba(255,238,150,.7)` |
| `bg2` | `#EEE7E3` | sunken |
| `s1` | `#FFFFFF` | cards |
| `s2` | `#FBF8F7` | inputs |
| `s3` / `s4` | `#F2ECE9` / `#E9E1DE` | chips, hover / tracks |
| `line` / `line2` | `rgba(46,22,20,.08)` / `.15` | hairlines / outlines |
| `tx` | `#160F0E` | primary text |
| `tx2` | `#5A4E4C` | secondary |
| `tx3` | `#6F6260` | labels, axis |
| `pastel-red` | `#FFE1E1` (deeper `#FFC9C9`) | red tints: error/loss chips, blooms, light module heroes |
| `pastel-yellow` | `#FFF1A8` | yellow tints: pending chips, AI card top, light module heroes |
| `red` / `red-edge` | `#D4112A` / `#8A0A1B` | primary |
| `red-tx` | `#C8102E` | loss, sell, errors |
| `yellow` / `yellow-edge` | `#FFD21F` / `#B58C00` | highlight |
| `ink` / `ink-edge` | `#231B1C` / `#000000` | ink buttons, toggle-on, selected chip |
| `up` / `up-face` / `up-edge` | `#1F5FE0` / `#1F5FE0` / `#0B2F80` | candles, Buy |
| `up-tx` | `#1A56D6` | profit text |
| `dn` | `#E0182F` | down candles, SL line |
| `ok` | `#137A45` | status "Completed" only |
| `focus` / `focus-halo` | `#160F0E` / `#F4FF4A` | 2 px ink ring + 7 px neon-yellow halo (the light theme's neon touch) |
| `glass` | `rgba(255,255,255,.62)` (strip over photos `.86`) | frosted panels |

Neon is never text and never a large fill: it is the focus halo, the equity fill, the live/notification dot.

### 2.4 Contrast (WCAG 2.2 AA: 4.5 : 1 text, 3 : 1 large text and UI graphics)

| Pair | Ratio | | Pair | Ratio |
|---|---|---|---|---|
| **Dark** `tx` on `bg` | 17.4 | | **Light** `tx` on `bg` | 16.9 |
| `tx2` on `s1` | 8.4 | | `tx2` on `s1` | 8.0 |
| `tx3` on `bg` / `s1` / `s2` / `s3` | 5.5 / 5.2 / 5.0 / 4.6 | | `tx3` on `bg` / `s1` / `s3` | 5.2 / 5.9 / 5.0 |
| `red-tx` on `s1` | 6.2 | | `red-tx` on `s1` / on `pastel-red` | 5.9 / 4.8 |
| `up-tx` on `s1` | 6.6 | | `up-tx` on `s1` / on `up-soft #E3ECFF` | 6.3 / 5.3 |
| `ok` on `s1` | 9.0 | | `ok` on `ok-soft #DCF2E5` | 4.6 |
| yellow text on `s1` | 13.0 | | `#7A5D00` on `pastel-yellow` (Pending) | 5.4 |
| `up` candle on chart `#0E0A0B` | 5.1 | | `up` candle on white | 5.6 |
| `dn` candle on chart | 5.1 | | `dn` candle on white | 4.8 |
| focus `#F4FF4A` on `s1` | 17.3 | | focus ink on white | 18.9 |

Button labels:

| Face | Label | Ratio |
|---|---|---|
| red `#D4112A` | white | 5.37 |
| yellow `#FFD21F` | `#0B0809` | 13.8 |
| Buy dark `#1F62EA` / light `#1F5FE0` | white | 5.26 / 5.57 |
| ink light `#231B1C` | white | 16.9 |
| ink dark `#F6EEE8` | `#0B0809` | 17.4 |
| white (on imagery) | `#0B0809` | 19.9 |
| website hero: ink on `#FFD224` | — | 13.8 (lede `#2A2016` 11.0, facts `#4A3A20` 7.6) |

`#2F7BFF` with white text is only 3.9 : 1 — that is why the Buy **face** is the deeper `#1F62EA` / `#1F5FE0`
while candles and lines keep the brighter blue.

---

## 3. Type

All OFL, all on Google Fonts. Geist and Plus Jakarta Sans are retired; Geist Mono is replaced as well, so nothing
of the old type stack remains.

### Option A — recommended: **Archivo** (display) + **Instrument Sans** (text) + **JetBrains Mono** (prices)

- **Archivo** variable (`wdth` 62–125, `wght` 100–900): set Expanded (`wdth` 125) ExtraBold 800 for headlines —
  wide, heavy and engineered, it echoes the bevelled lettering of the founder's crimson Kalks emblem. Set at `wdth` 108–112
  Bold 700 with `tnum` for money.
- **Instrument Sans** (`wdth` 75–100, `wght` 400–700): crisp, slightly narrow, good in dense tables.
- **JetBrains Mono** 400–700: prices, lots, IDs, times; slashed-free zero, clear 0/O and 1/l.
- Gaps: Archivo and Instrument Sans have **no Cyrillic** and Instrument Sans has no Vietnamese. Fallbacks per locale:
  Russian → **Golos Text** (OFL, text) and **Unbounded** (display); Vietnamese → **Be Vietnam Pro**. JetBrains Mono
  covers Cyrillic.

### Option B — **Unbounded** (display) + **Onest** (text) + JetBrains Mono

Rounder, more "crypto exchange"; both cover Cyrillic and Vietnamese natively. Weaker hierarchy at small sizes and
closer to what many exchanges use. A specimen line is in the sample (Components › Type).

### Non-Latin scripts (both options)

Arabic / Urdu / Farsi → Noto Sans Arabic (text), Noto Kufi Arabic (display) · Hindi → Noto Sans Devanagari ·
Bengali → Noto Sans Bengali · Tamil → Noto Sans Tamil · Thai → Noto Sans Thai · Chinese / Japanese / Korean → Noto
Sans SC / JP / KR. Display headlines in these scripts use the Bold weight of the text family (no expanded width).

### Scale

| Style | Font | Size / line | Settings |
|---|---|---|---|
| Display XL (website hero) | Archivo | 80 / 0.90 (phone 46) | wdth 125, 800, −0.045 em |
| Display L (section, page hero) | Archivo | 56 / 0.95 (phone 34) | wdth 125, 800, −0.035 em |
| Display M (hero cards) | Archivo | 46 / 0.95 (phone 34) | wdth 125, 800, −0.035 em |
| Title (page H1) | Archivo | 34 / 1.0 | wdth 112, 400 + name 800 ("Good evening, **Arjun**") |
| Card title | Archivo | 19–21 / 1.0 | wdth 110, 700, −0.015 em |
| Money XL / L / M | Archivo | 34 / 30 / 27 | wdth 108, 700, `tnum`; cents at 0.55–0.6 em in `tx2` |
| Body L (website lede) | Instrument Sans | 21 / 1.42 | 500 |
| Body | Instrument Sans | 15 / 1.5 | 400; emphasis 600–700 |
| Small | Instrument Sans | 13–13.5 / 1.4 | 500–600 |
| Label | Instrument Sans | 12.5 / 1.2 | 600, `tx2` |
| Table head | Instrument Sans | 11 / 1 | 600, uppercase, +0.07 em, `tx3` |
| Price / number | JetBrains Mono | 12–14 (order ticket 24 for the last two digits) | 500–700, tabular |
| Kicker | JetBrains Mono | 12 / 1 | 600, +0.04 em, red or yellow |

Headlines end with a full stop. Minus is U+2212 (−), never a hyphen.

---

## 4. Radius, spacing, elevation

**Radius:** 6 tags · 8 toolbar buttons · 10 button 32 · 12 button 40 / inputs · 13 segmented track · 14 button 48 /
Trader panels · 16 button 56 · 20 glass strips, toasts · 24 cards · 28 hero cards · 30 frames, sheet tops · 999 chips,
toggles. *(Open question: true NeoPOP is near-square — radius 3. Both are shown in the sample.)*

**Spacing** (4-point): 2 · 4 · 6 · 8 · 10 · 12 · 14 · 16 · 20 · 24 · 28 · 32 · 44 · 56 · 72. Client Area: 12-column
grid, 16 px gutters, 30 px page padding (phone 12). Trader: panel grid with 6 px gaps on `bg2`. Page gutters 32
desktop / 16 phone.

**Elevation:**

| Level | Use | Dark | Light |
|---|---|---|---|
| e0 | rows, chips | hairline `line` | hairline `line` |
| e1 | cards | inset 1 px `line` + `0 24px 48px -28px rgba(0,0,0,.7)` | inset 1 px `line` + `0 1px 2px rgba(40,18,16,.04), 0 12px 32px -14px rgba(40,18,16,.14)` |
| e2 | sheets, menus, modals, toasts | `0 40px 100px -20px rgba(0,0,0,.75)` | `0 30px 80px -24px rgba(40,18,16,.30)` |
| glass | strips over photos, header on scroll, activity table over blooms | `blur(26px) saturate(160%)` + inset 1 px `rgba(255,255,255,.09)` | same + inset 1 px `rgba(255,255,255,.8)` |
| edge | **controls only** | NeoPOP edge (§5) — never a blur shadow | same |

---

## 5. NeoPOP button

A flat face on a solid edge that sits bottom-right. The edge is drawn as stacked 1 px offset shadows, which gives the
diagonal corner joins of an extruded block and still follows the corner radius. The button reserves its edge depth as
right/bottom margin, so pressing never moves its neighbours.

```css
.btn{ --d:4px; --face:…; --edge:…; --lbl:…;
  --e:1px 1px 0 var(--edge),2px 2px 0 var(--edge),3px 3px 0 var(--edge),4px 4px 0 var(--edge);
  background:var(--face); color:var(--lbl); box-shadow:var(--e); margin:0 var(--d) var(--d) 0;
  transition:transform 90ms cubic-bezier(.3,.7,.4,1), box-shadow 90ms cubic-bezier(.3,.7,.4,1); }
.btn:active{ transform:translate(var(--d),var(--d)); box-shadow:0 0 0 var(--edge); }   /* sinks into the edge */
```

Flutter: the same list of `BoxShadow(offset: Offset(i, i), blurRadius: 0, color: edge)` for i = 1…d, and a
`Transform.translate` on press.

### Sizes

| Size | Height | Edge depth | Radius | Padding x | Label | Icon | Use |
|---|---|---|---|---|---|---|---|
| 32 | 32 | 3 | 10 | 12 | 13 / 600 | 16 | table/card actions, Trader bars, chart Sell/Buy |
| 40 | 40 | 4 | 12 | 16 | 14 / 600 | 18 | default; order ticket (40 + 4 = 44 ≤ trade cap) |
| 48 | 48 | 5 | 14 | 20 | 15 / 600 | 18 | hero CTAs in cards, phone primary |
| 56 | 56 | 6 | 16 | 26 | 16 / 600 | 20 | website hero only |

Round icon buttons use the same sizes with `border-radius: 50%`. On phones a 32 button gets a 40 px hit area
(transparent padding), never a bigger face.

### Variants

| Variant | Dark face / edge / label | Light face / edge / label | Use |
|---|---|---|---|
| Primary red | `#D4112A` / `#5E0611` / white | `#D4112A` / `#8A0A1B` / white | the one main action: Open account, Confirm, Start a challenge |
| Highlight yellow | `#FFD21F` / `#9A7700` / `#0B0809` | `#FFD21F` / `#B58C00` / `#0B0809` | Deposit, Ask AI, "new" offers |
| Ink | `#F6EEE8` / `#7D6F6B` / `#0B0809` | `#231B1C` / `#000000` / white | strong secondary: Trade, Close position |
| White | `#FFFFFF` / `#9E928E` / `#0B0809` | `#FFFFFF` / `#CDBFBB` / `#0B0809` | on photos only (hero CTAs) |
| Ghost | transparent, 1.5 px `line2` inset, no edge | same | Cancel, Try the demo, secondary links |
| Buy blue | `#1F62EA` / `#0A2C78` / white | `#1F5FE0` / `#0B2F80` / white | Kalks Trader buy |
| Sell red | `#D4112A` / `#5E0611` / white | `#D4112A` / `#8A0A1B` / white | Kalks Trader sell |

On a coloured surface the ink variant takes that surface's contrast: on the yellow website hero, ink is `#0B0809`
with a yellow label; on the red card, white with a `#5E0611` edge.

### States

| State | Look |
|---|---|
| Rest | face + full edge |
| Hover (pointer) | face `brightness(1.07)`; no movement |
| Pressed | face translates by the edge depth, edge collapses to 0, 90 ms in / 90 ms out |
| Focus-visible | 2 px `focus` outline, 3 px offset; light theme adds a 7 px neon-yellow halo. Focus never uses red, so it can't be read as an error. |
| Loading | label hidden, three dots in the label colour, same width, not pressable |
| Disabled | face `s3`, label `tx3`, **no edge** (sits half-pressed: nothing to press), `not-allowed` |
| Selected (side tiles) | raised = selected; the unselected tile sits pressed with a tinted outline |

---

## 6. Components

**Chips** — 30 px pills, `s3` face, `tx2` 13/600. Selected: ink (`tx` on `bg`). Outline chip: 1.5 px `line2`.
Status chips are 24 px with a 6 px dot: Completed `ok`, Pending yellow on pastel yellow, Rejected red on pastel
red, Open/Closed blue on `up-soft`. Tags are 20 px, radius 6, 10.5/700 uppercase: `LIVE` (outlined ink), `DEMO`
(`s3`), `CFD` (ink), `OPTIONS` (yellow), `NEW` (red). Change chips are mono on `up-soft` / `dn-soft`.

**Segmented control** — track `bg2` with a 1 px inner shadow (sunken); items 30 px (small 26), radius 10; the
selected item is raised: `s1` face with a 2 px NeoPOP edge (`#D9CCC8` light / black dark). Used for Market | Limit |
Stop, timeframes, All | Live | Demo, the theme switch.

**Toggle** — 46 × 28 track (Trader rows 40 × 24), sunken; the thumb is a white puck with a 2 px edge. On = ink track
(light) / yellow track (dark), thumb springs over in 220 ms with a small overshoot.

**Slider** — 6 px sunken track; fill ink (light) / yellow (dark); 24 px white puck with a 3 px edge; value bubble
in ink above the thumb; mono ticks under it. **Progress meter** (Prop): 10 px track, blue fill, ink target tick.

**Inputs** — 44 px, radius 12, `s2` face, 1 px `line2` inner ring, label above (12.5/600 `tx2`), hint below
(12.5 `tx3`). Focus: 1.5 px `focus` ring + 4 px halo. Error: 1.5 px `red-tx` ring + message in `red-tx` that says
what to do. Suffix units in `tx3`; search shows a `⌘K` key cap. Stepper: round 44 ink buttons (`s3` face, edge
`line2`) either side of a mono value.

**Choice cards** (open-account wizard, CFD vs Options) — `s2` cards with a radio dot; selected = `s1` face, 1.5 px
ink ring + 2 px ink edge, filled dot.

**Cards** — solid by default (`s1`, radius 24, e1). **Glass** only over something: the KPI strip over the hero
photo, the activity table over the canvas blooms, search / account pills in the top bar, the sticky header when
content scrolls under it.

**Tables** — 11 px uppercase heads in `tx3`; 52–56 px rows (Trader 40); hairline dividers; numbers mono and
right-aligned; first column is an icon tile (38 px, radius 12, tinted by meaning) + title + one sub-line; status as
a chip; row hover `s3` at 60 %. No zebra stripes, no vertical lines.

**Sheets & modals** — radius 26–30, `s1`, e2, a grabber (40 × 5) on phones; title in Archivo 20, one line of
consequence (with the number), then the action pair: ghost (keep) + ink or red (do). Destructive = red.

**Toasts** — iOS banners: frosted `s2` card, radius 20, 38 px icon tile coloured by meaning (blue fill for fills,
red for rejects), bold title + one line, time on the right. Drop in from the top with a spring (360 ms), swipe up to
dismiss.

**Icon rail (Client Area)** — 84 px column, a black rail (in both themes) with radius 24 and 14 px inset; K mark
on top; 44 px square buttons radius 14, icon 20, `#9C8D89`; active = yellow square with ink icon and a 2 px yellow
NeoPOP edge; bottom: alerts (neon dot), settings, avatar. Phone: becomes the app's ink tab bar (64 px, radius 24,
active icon in a disc — white in light, yellow in dark).

**Top bar (Client Area)** — mono date kicker, "Good evening, **Name**" title, then search (⌘K), account switcher pill
(product tag, login, balance), one yellow Deposit (40). No other buttons.

**Photo hero card + glass stat strip** — radius 28, 440 px tall (phone 560), image `object-fit: cover`; a left
scrim `rgba(12,3,4,.78 → 0)` across 58 % for text; pill tabs (glass) top-right switch the hero's message; kicker
(mono, yellow), Display M headline in white, one sentence, one white 48 CTA + text link. The strip sits 12 px inside
the bottom edge: four KPIs (Money M + 12.5 label with a colour key) separated by hairlines, a round ink "more" button
at the end; dark strip `rgba(18,11,13,.72)`, light `rgba(255,255,255,.86)`. Phone: 2 × 2.

**Ask Kalks AI card** — yellow card (`#FFE768 → #FFD21F`, top to 62 %), ink text; header with a black spark disc;
the prompt as a large 25 px sentence with the key words bold; 3 suggestion chips (white 55 %); input bar (white 72 %)
with a round ink mic button.

### 6.1 Kalks Trader

Layout (desktop): title bar 54 · then a 6 px-gap panel grid on `bg2`: drawing rail 48 | chart panel | order panel
312; positions panel 186 under rail + chart. Panels `s1`, radius 14, inset hairline.

- **Symbol tabs bar** — K mark, tabs 40 px: overlapping round flags (base in front), symbol 13.5/600, last price mono
  in `tx3` (active tab: price coloured by direction, `s1` face and a 2 px yellow underline), `+` to add; right: account
  switcher (product tag, "LIVE · STANDARD", equity mono), alerts bell, yellow Deposit 32.
- **Chart bar** (44) — compare `+` · timeframes 1m 5m 15m 1h 4h D (selected = ink pill) + more ▾ · chart type ·
  ƒx Indicators · layouts · bar replay · undo / redo · *(flex)* · Save ▾ · screenshot · full screen · inline
  **Sell 32 (red) · spread · Buy 32 (blue)** with mono prices.
- **Drawing rail** (48, 36 × 34 buttons, icon 18) — crosshair, trend line, horizontal levels, parallel channel,
  Fibonacci, patterns, brush, text, icons · measure, zoom · *(flex)* · magnet, lock, hide, remove. Active tool =
  pastel yellow with a yellow hairline.
- **Chart** — background `#0E0A0B` dark / white light; grid at 5 % opacity; candles up `up` / down `dn`, body 62 % of
  the step, 1 px wicks; volume at 18 %; MA lines thin (1.25 px) in yellow (dark) / `#C99A00` (light); user drawings in
  `tx2` with hollow handles. Legend top-left: flags, name, timeframe, OHLC in the candle colour, indicator values.
  Price axis 72 px mono 11; last-price tag in the candle colour with a countdown; crosshair dashed with ink tags.
- **Position line** — solid 1.25 px blue (buy) / red (sell) across the chart, its price tag on the axis; chip:
  `BUY 0.50` (face colour) + P&L (mono, outlined) + **S** (red outline) + **T** (blue outline) + ×. Dragging S/T
  creates the SL/TP line (Track 3); a handle disappears once its line exists. First-time tooltip: "Drag S down for a
  stop · T up for a target". SL line dashed red, TP line dashed blue, pending orders dashed ink, alerts dotted yellow.
- **Range bar** (36) — 5y 1y 6m 3m 1m 5d 1d · go-to-date · *(flex)* · UTC clock (mono) · % · log · auto.
- **Order panel** — symbol header; Market | Limit | Stop segmented; **side tiles** 66 px (label + price with the last
  two digits at 24 px): the selected side is raised in its colour, the other sits pressed with a tinted outline; a
  spread pill between them; volume stepper + quick lots (0.01 … 2.00, selected ink); a mono note (notional, margin);
  rows with S / T / % badges and toggles (stop loss, take profit with price + P&L, size by risk); a summary well (pip
  value, commission, swap); day range; **one** full-width 40 primary in the side colour: "Buy 0.50 lots · at
  1.16418"; free margin + margin level in mono underneath.
- **Positions panel** — tabs with count badges; totals right (balance, equity, margin, free); table with flags,
  side in colour caps, mono numbers, "add S / add T" placeholders, round outline close buttons.
- **Phone** — tabs collapse to the active symbol; chart bar keeps timeframes, type, indicators, full screen; no
  rail (long-press opens tools); order panel under the chart; positions in their own tab.

**Up = blue, down = red** applies to everything in the Trader (candles, buttons, P&L, change %, lines, the price
flash). Green does not exist in the Trader.

---

## 7. Motion

| What | Duration | Curve |
|---|---|---|
| Button press / release | 90 ms | `cubic-bezier(.3,.7,.4,1)` |
| Hover tint | 120 ms | ease |
| Toggle thumb | 220 ms | `cubic-bezier(.3,1.4,.5,1)` (small overshoot) |
| Segmented thumb, tabs | 180 ms | `cubic-bezier(.2,.8,.2,1)` |
| Sheet / modal in | 320 ms | `cubic-bezier(.2,.8,.2,1)`; out 200 ms |
| Toast drop | 360 ms | spring (damping 0.8) + light haptic on phones |
| Theme change | 250 ms colour crossfade | ease |
| Price tick | 300 ms flash of `up-soft` / `dn-soft` behind the price | ease-out |
| Numbers (balances) | no rolling digits; crossfade 150 ms | — |

No parallax, no custom cursor, no auto-playing hero video, no looping decorative animation.
`prefers-reduced-motion`: presses still change colour but don't move; sheets and toasts fade.

---

## 8. Imagery

- **One subject, one flat colour.** Backdrops are palette colours: Kalks red (`#D4112A`; the robot art sits at
  `#E00302`), Kalks yellow (`#FFD21F`; figure art `#FFD224`), ink `#0B0809`; light-mode module art may use pastel red
  `#FFE1E1` or pastel yellow `#FFF1A8`.
- **Subjects:** black, glossy, sculptural — robots, statues, masked figures, a hand with one object. They face or
  move toward the text. Yellow or red inlays are fine; no other colours.
- **Never:** stock-photo traders, smiling people at laptops, screens, charts, cityscapes, money piles, coins with
  real logos, rainbow light, lens flares, text inside the image.
- **The frame never ends:** sample the backdrop hex and extend it in CSS; fade the image's inner edge with a
  mask (≈ 14 %) where it meets the extension.
- **Sharpness:** source ≥ 2× the largest CSS size it is shown at; full-bleed ≥ 3840 px wide. Deliver AVIF + WebP +
  JPEG; check DPR-2 screenshots.
- **Colour grading:** generators drift; re-grade the backdrop to the exact hex (the sample's figure was re-graded
  from amber `#F09B00` to `#FFD224`, only the amber pixels moved).
- **One image per screen.** Heroes only; cards and tables carry no photos.
- **Licences:** founder's own AI images, or Unsplash / Pexels images that match this style, with the licence noted
  in `assets/LICENSES.md`. Pinterest is mood only.

---

## 9. Logo

The founder's traced K (`kalks-mark.svg`: three shapes — upper arm, stem, lower leg) becomes a **NeoPOP block:
red face `#D4112A`, yellow edge `#FFD21F`** extruded down-right by ≈ 5.5 % of the glyph height (30 units on the
541-unit glyph, drawn as stacked copies so the edge is solid). The rest of the wordmark ("alks") is one flat colour.
To colour the wordmark separately, split `kalks-logo.svg`: the K is the subpaths starting `M 403.113 2.545`,
`M 135.134 63.542` and `M 320 284.624`; the other five are "alks" (keep `fill-rule: evenodd` for the "a").

| Background | K face | K edge | "alks" |
|---|---|---|---|
| black / dark canvas | red | yellow | warm white `#F6EEE8` |
| white / light canvas | red | yellow | ink `#0B0809` |
| red | ink | yellow | white |
| yellow | red | ink | ink |
| photo | use the black or white lock-up on a solid plate; never directly on a busy image |

- **Minimum size:** with edge ≥ 20 px tall; below that (16 px favicon) a flat red K, no edge.
- **App icon:** black `#0B0809` squircle, K at 60 % width, nudged up-left by half the edge so the block looks
  centred; Android adaptive icon: foreground K + edge inside the 66 dp safe zone, background `#0B0809`, themed
  (monochrome) icon = K silhouette. Red and yellow icon variants are for campaigns and white-label previews.
- **Favicon:** 32 px black rounded square with the K + edge; 16 px flat red K on black; `apple-touch-icon` = the
  app icon.
- **White-label brokers:** their logo replaces the K; their brand colour replaces `k-red` in buttons (the edge is
  the colour mixed 55 % with black); yellow, the imagery and the Kalks K never appear for them.

---

## 10. Voice & copy

**Tone:** calm, exact, confident, warm. We sound like a good trader explaining something to a friend — short
sentences, plain words, the number that matters. Not hype, not royalty, no war or casino metaphors, no emojis.

**Rules**

1. Say what it is, then why it helps. Lead with a verb or the product, never with "Welcome to".
2. Hero headlines ≤ 6 words a line, 2 lines, end with a full stop. Sub-lines ≤ 25 words.
3. **Numbers only from `kalks-website/apps/trader/src/content/facts.ts`**, and they must be true today. Re-check
   counts before use: facts.ts lists 1,389 instruments but only 261 live (1,100 stocks "coming soon"), and the
   2026-10-07 rollout may have changed both — until it's re-synced, say "six asset classes", not a count.
4. No unprovable superlatives or claims: no "first", "best", "#1", "leading", regulation, awards, client counts,
   "guaranteed", "risk-free".
5. Every page that sells a product carries the platform's risk wording (`RISK_WARNING`, `OPTIONS_RISK`) verbatim.
6. CTAs are verb + object, 1–3 words: Open account · Try the demo · Start a challenge · Compare accounts ·
   Explore options. Never "Learn more" alone, never "Enter the kingdom".
7. Website copy spells things out (stop loss, take profit); the Trader may use S / T / SL / TP.
8. Errors say what to do: "This is a TRC20 address. Choose TRON as the network."
9. Money: currency first for dollars ($10), unit after for crypto (10 USDT); minus is "−"; percentages without a
   space (8.4%).

**Headline patterns**

| Pattern | Example |
|---|---|
| Product + plain promise | Options on forex, made simple. |
| Two beats | Get funded. Keep up to 90%. |
| Benefit as an instruction | Know your risk before you trade. |
| Fact + low barrier | Six asset classes. Start with $10. |
| Choice | Follow a master. Or become one. *(today's copy-trading line — keep)* |

**Before → after (from today's website)**

1. **Home hero** (`content/site.ts` HERO)
   - Before: *"Kalks FX Options · the first forex options platform"* / **"Trade like a sovereign."** / *"Options on
     forex, gold and oil. 1,389 CFDs. Instant funding. One kingdom."* / [Enter the kingdom]
   - After: *"New · Kalks FX Options"* / **"Options on forex, made simple."** / *"Calls and puts on forex, gold,
     silver and oil. Buy an option and the premium is the most you can lose."* / [Open account] [Try the demo] /
     *$10 to start · Fund with USDT · 22 languages*
   - Why: it says what the product is and the one benefit beginners need — a known maximum loss; drops the
     unprovable "first" and the "1,389" count; "Instant funding" was ambiguous (prop or deposits?).
2. **Options page hero** (`app/options/page.tsx`)
   - Before: **"The first forex options platform."** / *"Buy or sell calls and puts on 9 FX pairs, gold, silver
     and oil, with daily, weekly and monthly expiries. Settled in cash in US dollars, in the same account as your
     CFDs."*
   - After: **"Calls and puts on forex. Settled in dollars."** / *"Daily, weekly and monthly expiries on forex, gold,
     silver and oil. $0.25 a contract, capped at 10% of the premium. Options trade in their own Options account."*
   - Why: no superlative; the price is the strongest fact; "same account as your CFDs" becomes false with the
     CFD / Options account split (Track 1).
3. **Accounts page hero** (`app/accounts/page.tsx`)
   - Before: **"Five live accounts. Free demo."** / *"… Every account trades CFDs and Kalks FX Options."*
   - After: **"Five ways to trade CFDs. Two for options."** / *"All-in spreads or raw pricing with commission, in
     dollars or cents, from $10. Options get their own account: Options Standard or Options Pro. Free demo with
     $10,000 in virtual funds."*
   - Why: matches the account split; gives the reader the choice in one line.
4. **Home tile, prop** (`components/home/ProductsBento.tsx`)
   - Before: **"Trade our capital."** → After: **"Get funded. Keep up to 90%."**
   - Why: accounts are simulated, so "our capital" overstates; the split is the real hook.

The full page-by-page copy deck is R4 (`docs/design/WEBSITE-COPY.md`).

---

## 11. Mapping to the code (R1 preview)

- `packages/ui/src/styles.css`: replace `--font-geist-*`, the ember tokens and the pastel-orange theme with the tokens
  above (`[data-theme]` + `prefers-color-scheme`, default = device). Tailwind 4 theme keys mirror the token names.
- Website (Tailwind 3, own repo): the same tokens as a preset; heroes extend the image hex.
- App: `apps/mobile/lib/ui/tokens.dart` takes both palettes; `traderDark/traderLight` take up = blue; NeoPOP edges
  as stepped `BoxShadow`s; `assets/chart/chart.html` palette as in §6.1.
- Kalks Trader: `--k-up` → blue (`#2F7BFF` dark / `#1F5FE0` light) per Track 3.
