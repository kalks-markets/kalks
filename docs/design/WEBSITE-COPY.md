# Kalks website — copy deck (Kalks 2, R4)

Status: **built** in `kalks-website/apps/trader` (9 Oct 2026, not yet deployed). Voice and rules: [`KALKS2.md` §10](KALKS2.md#10-voice--copy).
Every number on the site comes from `apps/trader/src/content/facts.ts`; this deck shows the words around them.

**What changed everywhere**

- **One account → one product.** The platform now splits CFD and Options accounts (`services/trading/migrations/20261022120000_account_products.sql`).
  Every "options sit in the same account as your CFDs / one account / cross-margin" line is gone. New line, used on
  Home, Options, Accounts, FAQ: *"An account trades one product. Open both side by side and fund them from one USDT wallet."*
- **No unprovable claims.** "The first forex options platform", "One kingdom", "Enter the kingdom", "Trade our capital"
  and "Trade like a sovereign" are removed. No regulation, awards, client counts or "first / best / #1".
- **Counts lead with what is live.** 1,389 is no longer presented as "instruments you can trade": the site says
  **261 markets live** (28 core + 233 catalogue markets with live trading on, checked against
  `trade.kalkstrade.com/api/engine/symbols` on 2026-10-09) and **1,389 on demo** (1,100 stocks, 19 forex, 7 crypto and
  2 index markets are demo-only).
- **Options Pro** is seeded *disabled* with placeholder fees, so it is shown as "Coming soon" with no numbers. When the
  founder enables it, change the Accounts headline to the approved *"Five ways to trade CFDs. Two for options."* and add
  its fee and minimum to `OPTIONS_ACCOUNTS` in facts.ts.
- **CTAs** are verb + object: Open account · Try the demo · Explore options · Compare accounts · Start a challenge ·
  Start copying · Become a partner · Start learning · Talk to us. "Learn more" alone is gone; "Read more" on the risk
  note is now "Read the risk warning".
- **Risk wording** (`RISK_WARNING`, `OPTIONS_RISK`) stays verbatim on every product page and in the footer.
- Headlines end with a full stop. Minus is "−". Up is blue, down is red.

---

## Global

| Element | Before | After |
|---|---|---|
| Site title | Kalks: forex options, CFDs and prop trading on one account | Kalks: options on forex, CFDs and prop trading |
| Site description | … real option chains on forex, gold and oil, 1,389 CFD instruments … | Calls and puts on forex, gold, silver and oil, where buying an option means the premium is the most you can lose. Plus CFDs on 261 live markets, prop challenges, copy trading and PAMM. Start with $10, fund with USDT. |
| Nav | Options · Markets · Accounts · Platforms · Prop · More (dropdown) | same items; **More** is a mega menu (Copy trading & PAMM, Partners, Academy, White-label & API, About, Help & contact) with an Android card (*"Kalks in your pocket." · Get the app*) and the theme switch |
| Nav CTA | Open account | Open account (+ Log in, language) |
| Footer band | **Make your move.** / "One account for FX options, CFDs, prop and copy trading. Open it in a minute; start on demo." / [Open account] [Kalks Trader] | **Start on demo. Trade when ready.** / "Practise free with $10,000 in virtual funds on live prices. Fund with USDT when you are ready." / [Open account] [Try the demo] |
| Footer blurb | A global multi-asset trading platform … on one account. | Kalks FX Options, CFDs on forex, metals, energies, indices and crypto, prop challenges, copy trading and PAMM. One wallet funds them all. |
| Footer columns | Accounts: Account types, Demo, Funding … | Accounts: **CFD accounts**, **Options account**, Demo account, Funding, Prop challenges, Copy trading & PAMM |
| Theme | dark only | Auto (follows the device) · Light · Dark, in the menu and the footer |

## Home `/`

| Element | Before | After |
|---|---|---|
| Eyebrow | Kalks FX Options · the first forex options platform | NEW · Kalks FX Options |
| Headline | Trade like a sovereign. | **Options on forex, made simple.** |
| Sub-line | Options on forex, gold and oil. 1,389 CFDs. Instant funding. One kingdom. | Calls and puts on forex, gold, silver and oil. Buy an option and the premium is the most you can lose. |
| CTAs | [Enter the kingdom] | [Open account →] [Try the demo] |
| Facts line | — | $10 to start · Fund with USDT · 22 languages |
| Stat strip | — | **12** live underlyings: 8 FX pairs, gold, silver, US and UK oil · **Daily** plus weekly and monthly expiries, cut at 10:00 New York · **$0.25** per contract, capped at 10% of the premium · **USD** settled in cash |
| Live prices / news | — | Live price strip (EUR/USD, USD/JPY, XAU/USD, GBP/USD, US Oil, with day range) and the market news ticker |
| Products intro | **Six ways to trade. One account.** | (removed; the cards speak) |
| Options card | Option chains on forex, gold and oil. | **Know your risk before you trade.** Buy a call or a put and your loss is capped at the premium you pay. → Explore options |
| CFD card | Every market that moves. 1,389 … | **Six asset classes. Start with $10.** … 261 markets live … leverage up to 1:1000 → Compare accounts |
| Prop card | Trade our capital. | **Get funded. Keep up to 90%.** A 1-step or 2-step evaluation, or instant funding. Simulated accounts from $5k to $200k. → See the plans |
| Small cards | Follow a master, or become one. · Earn on every lot. | **Follow a master. Or become one.** · **Earn on every lot your network trades.** · **118 lessons in 9 phases.** |
| Quick trade | An option in three taps. | 01 — **An option in three taps.** Pick up or down, a date and how far. Before you confirm, the ticket shows what you pay and the most you can lose. [Explore options] [Try the demo] |
| Why Kalks | Built like an exchange. Explained like a friend. (4 long paragraphs, incl. "one account for CFDs and options") | removed; its facts moved into the sections below |
| Platform | One terminal. Every market. | 02 — **A big chart. Nothing to install.** [Open Kalks Trader] [See platforms] |
| Accounts | An account for how you trade. "Five live account types …" | 03 — **CFDs or options. One account each.** An account trades one product. Open both side by side and fund them from one USDT wallet. [Compare accounts] |
| Funding | (in Why Kalks) | 04 — **Deposit USDT. Trade in a minute.** 10 USDT minimum · ~1 min to credit · $0 wallet to account · 1 USDT withdrawal fee |
| Languages | A global platform, in your language. | 05 — **Your language. Light or dark.** |

## Options `/options`

| Element | Before | After |
|---|---|---|
| Title tag | Kalks FX Options: the first forex options platform | Kalks FX Options: calls and puts on forex, gold, silver and oil |
| Headline | **The first forex options platform.** | **Calls and puts on forex. Settled in dollars.** |
| Sub-line | Buy or sell calls and puts on 9 FX pairs, gold, silver and oil … in the same account as your CFDs. | Daily, weekly and monthly expiries on forex, gold, silver and oil. $0.25 a contract, capped at 10% of the premium. Options trade in their own Options account. |
| CTAs | [Open account] [How it works] | [Open account →] [How it works] |
| Facts | Inside Kalks Trader. Start on a free demo account. | Options Standard · Free demo · Inside Kalks Trader |
| Sections | Read a chain in thirty seconds. · Up or down. By when. How far. · Spreads, straddles, iron condors. · Three trades, worked through. · Clear rules, stated up front. · Analytics that explain the price. · Forex majors, metals and oil. · Options, answered. | same headlines kept (they were already plain), numbered 01–07; Analytics folded into the rules; "9 FX pairs" → **8 FX pairs live, NZDUSD next** |
| Order book | "Rolling out … The Book tab already shows depth" | **Exchange-style order book · Coming** (the public book reports `book_inactive`) |
| Contract terms, Account row | Same account as your CFDs, cross-margined | An Options account (Options Standard); CFDs trade in a separate CFD account |
| FAQ "Do I need a separate account?" | No. Options sit in the same Kalks trading account … | **Yes, an Options account.** Each Kalks account trades one product … One USDT wallet funds both … |
| Closing band | Make the call. Try it on demo. | (global footer band) |

## Markets `/markets`

| Element | Before | After |
|---|---|---|
| Title tag | Markets: 1,389 instruments across … | Markets: 261 live markets in forex, metals, energies, indices and crypto |
| Headline | **1,389 instruments. Six asset classes.** | **261 markets live. Six asset classes.** |
| Sub-line | Forex, metals, energies, indices and crypto, with 261 markets live … and 1,100 stocks coming soon. | Forex, metals, energies, indices, crypto and stocks, long or short, from $10. Every market, the 1,100 demo-only stocks included, trades on a free demo. |
| CTAs | [Open account] [Search the list] | [Open account →] [Search markets] |
| Hero visual | class tiles with totals | live board: EUR/USD, gold, WTI, US Tech 100, Bitcoin, NVIDIA (real quotes) |
| Strip | — | **261** live · **1,389** on demo · **1:1000** on FX (Standard, Cent) · **24/7** crypto |
| Class cards | "63 instruments", "167 instruments" … | live counts: Forex 44 · Metals 16 · Energies 4 · Indices 32 · Crypto 160 · Stocks 5 live + more soon |
| Sections | Every market that moves. · Find your market. · What you see is what you pay. | unchanged headlines; list status "Live soon · demo now" → **Demo now** |

## Accounts `/accounts`

| Element | Before | After |
|---|---|---|
| Title tag | Account types: Standard, Pro, ECN, Cent, VIP and demo | Accounts: five CFD accounts, an Options account and free demo |
| Headline | **Five live accounts. Free demo.** | **Five ways to trade CFDs. One for options.** (→ "Two for options." when Options Pro is enabled) |
| Sub-line | … Every account trades CFDs and Kalks FX Options. | All-in spreads or raw pricing with commission, in dollars or cents, from $10. Options get their own account. Free demo with $10,000 in virtual funds. |
| CTAs | [Open account] [Compare all] | [Open account →] [Compare accounts] |
| Strip | — | **$10** Standard and Cent · **1:1000** · **$3** per lot on VIP · **$10,000** demo |
| Sections | Pick your pricing. · Compare every account. · Trading in four steps. · Deposit USDT. Trade in a minute. · Accounts, answered. | 01 **Pick your pricing.** (CFD) · 02 **Options get their own account.** (Options Standard; Options Pro *Coming soon*; "How the split works") · 03 **Compare the CFD accounts.** · 04 **Trading in four steps.** · 05 **Deposit USDT. Trade in a minute.** · 06 **Accounts, answered.** |
| Compare table row | Kalks FX Options: "Yes, same account" | removed |
| Step 2 | Choose live or demo, the account type and your leverage. | CFD or Options, live or demo. Choose the type and leverage; your login is issued at once. |
| New FAQ | — | "Can one account trade CFDs and options?" → No. Each account trades one product … |

## Platforms `/platforms`

| Element | Before | After |
|---|---|---|
| Headline | **Kalks Trader. Nothing to install.** | same (approved brief W-05) |
| Sub-line | An MT5-style web terminal for CFDs and options with a big, clean chart. The Client Area handles everything around your trading … | A big, clean chart for CFDs and options, in your browser and on Android. The Client Area runs everything around it. |
| CTAs | [Open Kalks Trader] [Create an account] | [Open Kalks Trader →] [Get the app] |
| Trader feature "Options tab" | Switch between CFD and Options at the top. Same account, same balance. | **Options** — option chains, quick trade and the strategy builder for your Options account. |
| Android block | Kalks on Android. … [Download for Android] [Open Kalks Trader] | **Kalks on Android.** … [Download for Android] [Open in the browser] — version, build 3, 44 MB, Android 7.0+, universal APK, full SHA-256 |
| API teaser | Automate it. [API & algo] | same headline; [API & algo →] [Try the demo] |

## Prop `/prop`

| Element | Before | After |
|---|---|---|
| Headline | **Prove your edge. Keep up to 90%.** | **Get funded. Keep up to 90%.** |
| Sub-line | Pass a 1-Step or 2-Step evaluation, or start funded instantly … | Pass a 1-step or 2-step evaluation, or start funded today. Simulated accounts from $5k to $200k, rules checked live, payouts in USDT. |
| CTAs | [Start a challenge] [Compare plans] | same |
| Strip | 90% · $200k · $49 | **90%** · **$200k** · **$49** · **14 days** to the first payout (Classic, Rapid) |
| Sections | Three ways to get funded. · One-time fee, by account size. · From challenge to payout. · Prop, answered. | same, "One fee, by account size." |

## Copy trading `/copy-trading`

| Element | Before | After |
|---|---|---|
| Headline | **Follow a master. Or become one.** | same (KALKS2: keep) |
| Sub-line | Mirror verified traders with limits you set … | Mirror approved traders with limits you set, invest in a managed fund, or let a manager trade your account. Fees only on new highs. ("verified" → "approved": masters are approved by the team) |
| CTAs | [Start copying] [Become a master] | same |
| Sections | Choose how hands-on you want to be. · Four steps, your rules. · Become a master. Earn on new highs. | **As hands-on as you like.** · **Four steps. Your rules.** · **Become a master. Earn on new highs.** [Apply in the Client Area] |
| Facts | — | CFD accounts only · pause or stop at any time |

## Partners `/partners`

| Element | Before | After |
|---|---|---|
| Headline | **Earn on every lot your network trades.** | same |
| Sub-line | Every Kalks client is a partner from sign-up … paid every week in USDT. | Every Kalks client is a partner from day one. Share your link, earn per lot on three tiers of referrals, and get paid every Monday in USDT. |
| CTAs | [Become a partner] [See the rates] | same |
| Strip | — | **$13** per FX major lot at Diamond · **3** tiers (100% · 20% · 10%) · **Weekly** · **$10** minimum payout |
| Closing | Your link is waiting. | (footer band) |

## Academy `/academy`

| Element | Before | After |
|---|---|---|
| Headline | **Learn it properly. Then trade it.** | same |
| Sub-line | 118 lessons in 9 phases … certificates anyone can verify. | same, shortened |
| CTAs | [Start learning] [See the phases] | same |
| Sections | Nine phases, one direction. · (3 cards) | **Nine phases, one direction.** · **Short lessons. Real exams.** |

## White-label & API `/white-label`

| Element | Before | After |
|---|---|---|
| Headline | **Your brokerage. Our platform.** | same |
| Sub-line | Launch a broker on the Kalks platform … Or plug your own systems into ours with the API. | Launch a broker under your own brand and domains, with forex options, CFDs, prop, copy trading and partners built in. Or connect your systems through the API. |
| CTAs | [Talk to us] [API & algo] | same |
| Sections | Everything Kalks runs, under your name. · Automate your trading. | same |

## About `/about`

| Element | Before | After |
|---|---|---|
| Headline | **A trading platform built for every market.** | **Built for traders everywhere.** (the crimson emblem above it, text below the image as the brief asks) |
| Sub-line | Kalks brings forex options, CFDs, prop challenges, copy trading and a partner programme together on one account … | Forex options, CFDs, prop challenges, copy trading and a partner programme in one place, on technology we build ourselves. |
| Principle 1 | One account, one wallet — "Options sit in the same trading account as CFDs." | **One wallet for everything** — CFD and Options accounts, prop, copy trading, PAMM and partner earnings sit side by side, all funded from one USDT wallet. |
| Sections | Six products. One platform. · Clear, fast, and our own. | same |

## Help & contact, FAQ, 404

| Page | Before | After |
|---|---|---|
| Contact | **We are here to help.** | same; topics mention CFD and Options accounts |
| FAQ | **Questions, answered.** "Five live account types … Standard, Pro, ECN, Cent and VIP" · "Is there a mobile app? A native Android app is coming soon." | Accounts answer lists five CFD accounts **and Options Standard**; new "Can one account trade CFDs and options?"; mobile app answer: **Yes, for Android** (APK on Platforms) |
| 404 | (default Next.js page) | **This page moved, or never existed.** [Go home] [Explore options] |

## Legal pages

Restyled only (document card, contents rail, red callout). Wording unchanged except UI chrome in sentence case:
"Open Account" → "Open account", "Trade Responsibly" → "Trade responsibly.", "Fund Your Account" → "Fund your account.",
"Contact Support" → "Contact support". No account wording in the legal texts claimed one account for both products, so
nothing there needed a fact fix. Open: the Risk Disclosure (`/risk`) has no options section; the options risk sentence
appears on `/risk-warning`'s meta description and in the footer only.
