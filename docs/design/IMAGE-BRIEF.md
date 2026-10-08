# Kalks 2 — image brief (solid-colour AI images)

For the founder to generate. Every image follows the imagery rule in [`KALKS2.md` §8](KALKS2.md#8-imagery): **one
black, glossy, sculptural subject on one flat, saturated backdrop** from the palette, like the robot on red and the
figure on yellow. Pinterest is mood only.

## How to use this brief

1. Paste the prompt into Midjourney (v7; `--style raw` keeps the backdrop flat). For other tools (Firefly, Ideogram,
   DALL·E, Flux) use the "Other tools" sentence; it says the same thing without Midjourney flags.
2. Generate at the tool's largest size, then upscale to the **size** in the entry (2× upscaler, no "creative"
   upscaling — it invents texture in the flat backdrop).
3. **Backdrop colour:** generators drift. Check the backdrop with a colour picker. If it is off by more than a little,
   send it back to us with the hex: we re-grade only the backdrop-coloured pixels (as done for
   `img/figure-yellow.jpg`, amber `#F09B00` → `#FFD224`) and extend the same hex in CSS.
4. **Safe area** = where text, buttons or the glass strip go. Keep it as empty backdrop: no subject, no shadow, no
   texture.
5. Deliver PNG (or max-quality JPEG) named by the entry ID, e.g. `W-01-dark.png`. We make AVIF / WebP / JPEG and the
   phone crops.

**Palette backdrops:** Kalks red `#D4112A` · Kalks yellow `#FFD21F` · ink `#0B0809` · light-mode pastels: pastel red
`#FFE1E1`, pastel yellow `#FFF1A8`. Subject colours: black gloss, with yellow or red inlays/light only.

**Midjourney suffix used below** (already in every prompt):
`--style raw --v 7 --no text, letters, logo, watermark, gradient, vignette, pattern, extra objects, people in background`

**Theme rule:** website heroes are brand moments — **one image for both themes**. Client Area module heroes and the
app have a **dark** variant (saturated or ink backdrop) and a **light** variant (same subject, pastel backdrop).

---

## Website

### W-01 · Home hero — "Options on forex, made simple."
- **Subject:** the seated, meditating black figure with glowing yellow circuit-line inlays and a thin yellow halo
  ring behind the head (the founder's existing figure, regenerated at full resolution). Calm, in control.
- **Backdrop:** `#FFD21F` (sample currently uses the re-graded `#FFD224`).
- **Framing:** subject in the right 40 %, head 12–20 % from the top, cropped at the lap at the bottom; facing
  slightly left (toward the headline).
- **Safe area:** left 55 % (headline, CTAs), bottom 18 % (glass fact strip).
- **Size:** 3840 × 2160 (16 : 9). Phone crop 1290 × 2796: subject in the lower 60 %, top 40 % empty.
- **Theme:** one image, both themes.
- **Prompt:** `studio photograph of a sculptural black glossy humanoid figure seated cross-legged in meditation, one hand raised in a mudra, body covered in fine glowing yellow circuit-line inlays, thin yellow halo ring behind the head, seamless flat bright yellow backdrop #FFD21F, no floor line, subject on the right third facing slightly left, large empty yellow space on the left, crisp rim light, ultra sharp, high-end editorial --ar 16:9 --style raw --v 7 --no text, letters, logo, watermark, gradient, vignette, pattern, extra objects, people in background`
- **Other tools:** A sharp studio photo of a glossy black humanoid figure meditating cross-legged, with thin glowing yellow circuit lines on its body and a yellow halo ring, on a perfectly flat yellow (#FFD21F) background; subject on the right, left half empty; no text, no gradient, no vignette.

### W-02 · Options page — "Calls and puts on forex. Settled in dollars."
- **Subject:** a black chrome robotic hand balancing one glossy yellow sphere on a single fingertip (the premium:
  small, known, held).
- **Backdrop:** `#D4112A`.
- **Framing:** hand enters from the bottom-right, sphere at 35 % from the top, right third.
- **Safe area:** left 58 %, bottom 15 %.
- **Size:** 3840 × 1800 (≈ 2.1 : 1). Phone crop 1290 × 1600.
- **Theme:** one image.
- **Prompt:** `studio photograph of a black chrome robotic hand rising from the bottom right, balancing a single glossy yellow sphere on one fingertip, seamless flat crimson red backdrop #D4112A, subject in the right third, generous empty red space on the left, hard rim light, reflective black metal, minimal, ultra sharp --ar 21:10 --style raw --v 7 --no text, letters, logo, watermark, gradient, vignette, pattern, extra objects, people in background`
- **Other tools:** Glossy black robotic hand from the lower right balancing one yellow sphere on a fingertip, flat crimson (#D4112A) background, subject on the right, left side empty, no text.

### W-03 · Markets page — "Six asset classes."
- **Subject:** a floating black obsidian globe with thin red meridian lines, a faint red reflection beneath.
- **Backdrop:** `#FFD21F`.
- **Framing:** globe centred at 72 % width, 45 % height, 50 % of the image height in diameter.
- **Safe area:** left 52 %.
- **Size:** 3840 × 1800. Phone 1290 × 1600 (globe lower half).
- **Theme:** one image.
- **Prompt:** `studio photograph of a floating polished black obsidian globe with thin glowing red meridian and latitude lines, no continents, seamless flat yellow backdrop #FFD21F, globe on the right, empty yellow space on the left, soft contact shadow, ultra sharp, minimal --ar 21:10 --style raw --v 7 --no text, letters, logo, watermark, gradient, vignette, pattern, extra objects, people in background`
- **Other tools:** A floating polished black globe with thin red grid lines (no continents) on a flat yellow (#FFD21F) background, globe on the right, no text.

### W-04 · Accounts page — "Five ways to trade CFDs. Two for options."
- **Subject:** a black glossy mannequin hand fanning five matte-black cards, each with a thin red edge; two of them
  slightly apart with a yellow edge (the two Options accounts).
- **Backdrop:** `#FFD21F`.
- **Framing:** hand from the right edge, cards fanned upward, right 40 %.
- **Safe area:** left 55 %.
- **Size:** 3840 × 1800. Phone 1290 × 1600.
- **Theme:** one image.
- **Prompt:** `studio photograph of a glossy black mannequin hand entering from the right, fanning seven blank matte black cards, five with thin red edges and two with thin yellow edges, seamless flat yellow backdrop #FFD21F, subject on the right, empty space on the left, crisp light, minimal, ultra sharp --ar 21:10 --style raw --v 7 --no text, letters, logo, numbers, watermark, gradient, vignette, pattern, people in background`
- **Other tools:** A glossy black mannequin hand fanning seven blank black cards (five red-edged, two yellow-edged) on flat yellow (#FFD21F), subject right, no text or numbers.

### W-05 · Platforms page (Kalks Trader) — "Kalks Trader. Nothing to install."
- **Subject:** the Kalks robot (black glossy helmet, leather collar — same character as the Client Area hero) in
  three-quarter view, the visor slit glowing yellow.
- **Backdrop:** ink `#0B0809`, one red rim light from the right.
- **Framing:** head and shoulders in the right 45 %, looking left.
- **Safe area:** left 52 %, top 12 % (nav).
- **Size:** 3840 × 1800. Phone 1290 × 1600.
- **Theme:** one image (this is the site's one dark hero).
- **Prompt:** `studio portrait of a sleek robot with a glossy black full helmet and black leather jacket collar, three-quarter view looking left, a thin horizontal visor slit glowing yellow, seamless flat pure black backdrop #0B0809, single crimson red rim light from the right, subject on the right, empty black space on the left, ultra sharp, cinematic --ar 21:10 --style raw --v 7 --no text, letters, logo, watermark, gradient, vignette, pattern, extra objects, people in background`
- **Other tools:** Glossy black-helmeted robot in a black leather jacket, three-quarter view facing left, yellow visor slit, flat black background with one red rim light, subject right, no text.

### W-06 · Prop page — "Get funded. Keep up to 90%."
- **Subject:** a featureless black glossy mannequin sprinter in starting blocks, about to launch.
- **Backdrop:** `#D4112A`, flat floor in the same red.
- **Framing:** full body, right 50 %, low camera, facing left.
- **Safe area:** left 50 %.
- **Size:** 3840 × 1800. Phone 1290 × 1600.
- **Theme:** one image.
- **Prompt:** `studio photograph of a featureless glossy black mannequin sprinter crouched in starting blocks, ready to launch to the left, low camera angle, seamless flat crimson red backdrop and floor #D4112A, subject on the right half, empty red space on the left, crisp rim light, ultra sharp --ar 21:10 --style raw --v 7 --no text, letters, logo, numbers, watermark, gradient, vignette, pattern, track lines, people in background`
- **Other tools:** A faceless glossy black mannequin sprinter in starting blocks facing left, on a flat crimson (#D4112A) background and floor, subject right, no text.

### W-07 · Copy trading & PAMM — "Follow a master. Or become one."
- **Subject:** two identical black marble statues in profile, walking left; the front one leads, the second mirrors
  its pose half a step behind. (Replaces the blindfold statue: "follow blindly" is the wrong message for copy
  trading.)
- **Backdrop:** `#FFD21F`.
- **Framing:** both figures waist-up in the right 50 %.
- **Safe area:** left 50 %.
- **Size:** 3840 × 1800. Phone 1290 × 1600.
- **Theme:** one image.
- **Prompt:** `studio photograph of two identical classical statues carved from glossy black marble, seen in profile walking to the left, the second statue half a step behind mirroring the first, waist-up, seamless flat yellow backdrop #FFD21F, subjects on the right half, empty space on the left, crisp light, ultra sharp --ar 21:10 --style raw --v 7 --no text, letters, logo, watermark, gradient, vignette, blindfold, pattern, people in background`
- **Other tools:** Two identical glossy black marble statues in profile walking left, one leading and one following, on flat yellow (#FFD21F), subjects right, no text.

### W-08 · Partners (IB) — "Earn on every lot your network trades."
- **Subject:** a black glossy figure holding one taut red thread that splits into three thinner red threads running
  off-frame to the left.
- **Backdrop:** `#FFD21F`.
- **Framing:** figure right 35 %; the threads cross the safe area only as thin lines.
- **Safe area:** left 55 % (thin threads allowed, nothing else).
- **Size:** 3840 × 1800. Phone 1290 × 1600.
- **Theme:** one image.
- **Prompt:** `studio photograph of a glossy black sculptural humanoid figure on the right holding a single taut red thread that splits into three thin red threads running off to the left edge, seamless flat yellow backdrop #FFD21F, mostly empty space on the left, minimal, ultra sharp --ar 21:10 --style raw --v 7 --no text, letters, logo, watermark, gradient, vignette, pattern, extra objects, people in background`
- **Other tools:** A glossy black humanoid figure on the right holding a red thread that branches into three threads running left, flat yellow (#FFD21F) background, no text.

### W-09 · Academy — "118 lessons, 9 phases."
- **Subject:** a black glossy owl sculpture with fine yellow inlay lines on its feathers.
- **Backdrop:** `#D4112A`.
- **Framing:** owl perched, right 35 %, looking left.
- **Safe area:** left 58 %.
- **Size:** 3840 × 1800. Phone 1290 × 1600.
- **Theme:** one image.
- **Prompt:** `studio photograph of a polished black owl sculpture with fine glowing yellow inlay lines along its feathers, perched, looking left, seamless flat crimson red backdrop #D4112A, subject on the right third, empty red space on the left, crisp rim light, ultra sharp --ar 21:10 --style raw --v 7 --no text, letters, logo, watermark, gradient, vignette, pattern, branch, people in background`
- **Other tools:** A polished black owl sculpture with thin yellow inlay lines, perched and facing left, on flat crimson (#D4112A), subject right, no text.

### W-10 · White-label & API
- **Subject:** a black chrome robotic hand holding a single glossy red cube with a clean cut-out notch (no letters).
- **Backdrop:** ink `#0B0809` with a faint yellow rim light.
- **Framing:** right 35 %.
- **Safe area:** left 58 %.
- **Size:** 3840 × 1800. Phone 1290 × 1600.
- **Theme:** one image.
- **Prompt:** `studio photograph of a black chrome robotic hand on the right holding a single glossy crimson red cube with one clean geometric notch, seamless flat black backdrop #0B0809, faint yellow rim light, empty black space on the left, minimal, ultra sharp --ar 21:10 --style raw --v 7 --no text, letters, logo, watermark, gradient, vignette, pattern, extra objects, people in background`
- **Other tools:** A black chrome robot hand on the right holding a glossy red cube with a notch, flat black background, faint yellow rim light, no text.

### W-11 · About Kalks
- **Subject:** the founder's crimson emblem (woman in a black coat with two black dogs before the giant black
  "Kalks" letters) — keep; regenerate only for resolution.
- **Backdrop:** red (art at ≈ `#F80E0D`; re-grade to `#D4112A` if regenerated).
- **Framing / safe area:** centred; text goes **below** the image, never on it.
- **Size:** 3840 × 2160.
- **Theme:** one image.
- **Prompt:** `fashion editorial photograph, a woman in a long black tailored coat, black sunglasses and red gloves standing between two black great danes, in front of giant glossy black bevelled letters spelling KALKS, seamless flat crimson red backdrop and floor #D4112A, symmetrical, ultra sharp --ar 16:9 --style raw --v 7 --no extra text, watermark, gradient, vignette, people in background`
- **Other tools:** As above — the wordmark is the only text allowed in any image, and only here.

### W-12 · Footer band
- **Subject:** a lone small figure standing under a single vertical red light beam (the founder's "footer end
  image", widened).
- **Backdrop:** ink `#0B0809`.
- **Framing:** beam at 70 % width, figure at the bottom of the beam.
- **Safe area:** left 60 % (footer links sit on it).
- **Size:** 3840 × 1200.
- **Theme:** one image.
- **Prompt:** `minimal cinematic image, a lone small human silhouette standing at the base of a single thin vertical crimson red light beam, pure black backdrop #0B0809, red glow on the floor around the figure, beam at seventy percent of the width, vast empty black space on the left, ultra sharp --ar 16:5 --style raw --v 7 --no text, letters, logo, watermark, stars, pattern, people in background`
- **Other tools:** A tiny human silhouette under one thin vertical red light beam on a pure black background, beam right of centre, empty left side, no text.

### W-13 · Android app download section
- **Subject:** a black glossy mannequin hand holding the Kalks app icon as a physical block — a black rounded square
  with the red K and its yellow edge (render the icon from `kalks2-sample.html` › Brand and use it as an image
  reference).
- **Backdrop:** `#FFD21F`.
- **Framing:** right 40 %.
- **Safe area:** left 55 %.
- **Size:** 3200 × 1600. Phone 1290 × 1400.
- **Theme:** one image.
- **Prompt:** `studio photograph of a glossy black mannequin hand on the right holding a thick black rounded-square app icon block with a red letter K and a yellow extruded edge, seamless flat yellow backdrop #FFD21F, empty space on the left, crisp light, ultra sharp --ar 2:1 --style raw --v 7 --no phone, screen, extra text, watermark, gradient, vignette, pattern, people in background` (attach the icon PNG as an image prompt)
- **Other tools:** A glossy black mannequin hand holding a black app-icon block with a red K, on flat yellow (#FFD21F); attach the icon as reference.

---

## Client Area

Module heroes sit in a 28-radius card about 1100 × 320 CSS px (dashboard hero 900 × 440). Text and pill tabs sit on
the left; the glass strip (dashboard only) sits on the bottom 25 %.

### CA-01 · Dashboard hero — "Add an Options account." (the sample's hero)
- **Subject:** the Kalks robot in profile facing right (existing `src-hero-robot.png`), regenerated sharper.
- **Backdrop:** red `#D4112A` (art today `#E00302`) with the soft black wave forms kept low and to the left.
- **Framing:** head at 55–70 % width, 10–50 % height; shoulders to the bottom.
- **Safe area:** left 48 % (headline, CTA), bottom 25 % (KPI strip), top-right 30 × 12 % (pill tabs).
- **Size:** 3200 × 1560 (≈ 2 : 1); phone crop 1290 × 1700 (head in the top half).
- **Theme:** dark and light use the same image (it sits in a card with its own scrim and strip).
- **Prompt:** `studio portrait of a sleek robot in profile facing right, glossy black full helmet with a small glowing red circular eye mechanism at the temple, black leather jacket collar, seamless flat crimson red backdrop #D4112A with a few soft dark curved shapes low on the left, subject at sixty percent of the width, calm empty red space on the left, crisp highlights, ultra sharp --ar 2:1 --style raw --v 7 --no text, letters, logo, watermark, gradient, vignette, pattern, people in background`
- **Other tools:** Profile of a glossy black-helmeted robot in a leather jacket facing right, on flat crimson (#D4112A) with a few soft dark curves low left, subject centre-right, no text.

### CA-02 · Wallet — "Deposit in USDT. Usually credited within a minute."
- **Subject:** a black glossy hand catching a falling plain yellow coin (no symbol, no logo).
- **Dark variant:** backdrop `#D4112A`. **Light variant:** backdrop `#FFE1E1`.
- **Framing:** right 35 %. **Safe area:** left 62 %.
- **Size:** 3000 × 880 each (≈ 3.4 : 1).
- **Prompt (dark):** `studio photograph of a glossy black mannequin hand on the right catching a single falling plain polished yellow coin with no markings, motion frozen, seamless flat crimson red backdrop #D4112A, wide empty space on the left, ultra sharp, minimal --ar 17:5 --style raw --v 7 --no text, letters, logo, currency symbol, watermark, gradient, vignette, pattern, people in background`
- **Prompt (light):** same, with `seamless flat pastel blush pink backdrop #FFE1E1`.
- **Other tools:** A glossy black hand catching one plain yellow coin (no symbol), on flat crimson (#D4112A) / pastel blush (#FFE1E1); subject right; no text.

### CA-03 · Accounts — "CFD or Options. One account each."
- **Subject:** two thick black cards standing upright side by side, one with a red edge (CFD), one with a yellow
  edge (Options), a soft contact shadow.
- **Dark variant:** backdrop ink `#0B0809`. **Light variant:** `#FFF1A8`.
- **Framing:** right 35 %. **Safe area:** left 62 %.
- **Size:** 3000 × 880 each.
- **Prompt (dark):** `studio product photograph of two thick blank matte black cards standing upright side by side, one with a glowing red edge and one with a glowing yellow edge, seamless flat black backdrop #0B0809, objects on the right, wide empty space on the left, ultra sharp, minimal --ar 17:5 --style raw --v 7 --no text, letters, numbers, logo, chip, watermark, gradient, vignette, pattern`
- **Prompt (light):** same, with `seamless flat pastel butter yellow backdrop #FFF1A8` and edges in red and deep yellow.
- **Other tools:** Two blank thick black cards standing upright, one red-edged and one yellow-edged, on flat black / pastel yellow (#FFF1A8), right side, no text.

### CA-04 · Copy & PAMM
- **Subject:** W-07's two statues, tight crop on the heads (leader + follower).
- **Dark:** `#FFD21F`. **Light:** `#FFF1A8`.
- **Framing:** right 35 %. **Safe area:** left 62 %. **Size:** 3000 × 880 each.
- **Prompt:** W-07 prompt with `close-up on the two heads in profile` and `--ar 17:5`; light variant with `#FFF1A8`.

### CA-05 · Prop
- **Subject:** W-06's sprinter, tight crop (hands on the line, head down).
- **Dark:** `#D4112A`. **Light:** `#FFE1E1`.
- **Framing:** right 35 %. **Safe area:** left 62 %. **Size:** 3000 × 880 each.
- **Prompt:** W-06 prompt with `close-up crop of the hands on the start line and lowered head` and `--ar 17:5`;
  light variant with `#FFE1E1`.

### CA-06 · Partner
- **Subject:** W-08's figure and branching red thread, tight crop.
- **Dark:** ink `#0B0809` (red thread glowing). **Light:** `#FFF1A8`.
- **Framing:** right 35 %. **Safe area:** left 62 % (threads only). **Size:** 3000 × 880 each.
- **Prompt:** W-08 prompt with the dark or light backdrop and `--ar 17:5`.

### CA-07 · Academy
- **Subject:** W-09's owl, head and shoulders.
- **Dark:** `#D4112A`. **Light:** `#FFE1E1`.
- **Framing:** right 35 %. **Safe area:** left 62 %. **Size:** 3000 × 880 each.
- **Prompt:** W-09 prompt with `head and shoulders crop` and `--ar 17:5`; light variant with `#FFE1E1`.

### CA-08 · Options
- **Subject:** W-02's robotic hand with the yellow sphere.
- **Dark:** ink `#0B0809` with a red rim light. **Light:** `#FFE1E1`.
- **Framing:** right 35 %. **Safe area:** left 62 %. **Size:** 3000 × 880 each.
- **Prompt:** W-02 prompt with the dark or light backdrop and `--ar 17:5`.

---

## App (Android)

### APP-01 · Welcome page — "Options on forex, made simple."
- **Subject:** the W-01 figure, full body seated, slightly smaller in frame.
- **Dark variant:** ink `#0B0809`, the yellow inlays glowing. **Light variant:** `#FFD21F`.
- **Framing:** portrait; figure in the lower 60 %, head at about 45 % height, centred.
- **Safe area:** top 38 % (status bar + headline), bottom 22 % (Log in / Open account pills sit on the image).
- **Size:** 1440 × 3120 (19.5 : 9) each.
- **Prompt (light):** `studio photograph of a sculptural black glossy humanoid figure seated cross-legged in meditation, glowing yellow circuit-line inlays, thin yellow halo ring behind the head, centred in the lower half of a tall portrait frame, seamless flat bright yellow backdrop #FFD21F, large empty space above, ultra sharp --ar 9:19.5 --style raw --v 7 --no text, letters, logo, watermark, gradient, vignette, pattern, people in background`
- **Prompt (dark):** same, with `seamless flat pure black backdrop #0B0809, the yellow inlays glowing softly`.
- **Other tools:** Tall portrait: the glossy black meditating figure with yellow circuit lines, centred in the lower half, flat yellow (#FFD21F) / flat black background, empty top, no text.

### APP-02 · Dashboard hero
- **Subject:** the CA-01 robot, tighter (helmet and collar).
- **Backdrop:** `#D4112A` in both themes (the sheet over it carries the theme).
- **Framing:** portrait 4 : 5, head in the upper-middle (25–60 % height), facing right.
- **Safe area:** top 12 % (round floating buttons), bottom 35 % (the sheet slides over it).
- **Size:** 1440 × 1800.
- **Prompt:** `studio portrait of a sleek robot in profile facing right, glossy black full helmet with a small glowing red circular eye mechanism, black leather jacket collar, seamless flat crimson red backdrop #D4112A, head in the upper middle of a portrait frame, empty red space at the top and bottom, ultra sharp --ar 4:5 --style raw --v 7 --no text, letters, logo, watermark, gradient, vignette, pattern, people in background`
- **Other tools:** Portrait of the glossy black-helmeted robot facing right, head upper-middle, flat crimson (#D4112A), empty top and bottom, no text.

---

## Checklist per delivered image

- [ ] Backdrop within a few steps of the hex (or flagged for re-grade)
- [ ] Safe area empty — no subject, shadow, texture or noise
- [ ] Size ≥ the entry's size; no upscaling artefacts in the flat colour
- [ ] No text, logos, symbols or extra people (W-11 wordmark excepted)
- [ ] Subject faces or moves toward the text side
- [ ] Light and dark variants delivered where the entry asks for both
