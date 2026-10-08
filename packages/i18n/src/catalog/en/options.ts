// Keys for this namespace. English is the source; translations live in ../<lang>/options.ts.
// Kalks FX Options in the Client Area: the Options page (what the product is), a friendly three-card intro (Buy a Call,
// Buy a Put, limited risk when you buy) and the one step before the first trade: accept the options terms with
// "Start trading options", which opens Kalks Trader in options mode. No identity check or quiz is needed for options.
// The full options terms come from the server and are shown exactly as published (it is the text clients accept),
// so they are not in this file; the short translated key points are.
const options = {
  // Navigation entry
  "nav.title": "Options",

  // Page header
  "page.title": "Kalks FX Options",
  "page.subtitle": "Buy or sell options on forex, gold, silver and oil, right inside Kalks Trader.",
  "page.statusReady": "Ready to trade",
  "page.learnCourse": "Options course",

  // Hero card
  "hero.eyebrow": "New in Kalks Trader",
  "hero.title": "Options on 13 markets, made simple",
  "hero.text": "European options on forex majors and crosses, gold, silver and crude oil. Choose daily, weekly or monthly expiries. Every option is settled in cash, in US dollars, so you never take delivery of anything.",
  "hero.feature.underlyings.title": "13 underlyings",
  "hero.feature.underlyings.text": "9 forex pairs, gold, silver, WTI and Brent crude oil.",
  "hero.feature.expiries.title": "Daily, weekly, monthly",
  "hero.feature.expiries.text": "Expiries from the same day to the end of the month, cut at 10:00 New York.",
  "hero.feature.settlement.title": "Cash-settled in USD",
  "hero.feature.settlement.text": "Settled at the average mid price of the 30 minutes before the cut.",
  "hero.feature.sides.title": "Buy or sell",
  "hero.feature.sides.text": "Calls and puts, spreads, straddles, iron condors and barrier options.",
  "hero.class.forex": "Forex",
  "hero.class.metals": "Metals",
  "hero.class.energies": "Energies",
  "hero.start": "Get started",
  "hero.howItWorks": "How options work",

  // Friendly intro before the first options trade: three illustrated cards, one checkbox and the Start button
  "intro.title": "Options in three simple ideas",
  "intro.subtitle": "A quick look before your first options trade.",
  "intro.call.title": "Buy a Call",
  "intro.call.text": "You think the price will go up.",
  "intro.put.title": "Buy a Put",
  "intro.put.text": "You think the price will go down.",
  "intro.risk.title": "Your risk is limited when you buy",
  "intro.risk.text": "The most you can lose is the price you pay. (Selling options can lose more.)",
  "intro.legend.result": "Your result at expiry",
  "intro.legend.cost": "The price you pay",
  "intro.confirm": "I understand how options work",
  "intro.terms": "Read full terms",
  "intro.consent": "By starting, you accept the options terms.",
  "intro.start": "Start trading options",
  "intro.quiz": "Test yourself (quiz)",
  "intro.gotIt": "Got it",
  "intro.toastStarted": "You're all set for options",
  "intro.toastFailed": "Couldn't start options trading. Please try again.",
  "intro.toastUpdated": "The options terms were just updated. Take a quick look, then press Start again.",

  // Full terms dialog (the published text itself comes from the server, in English)
  "terms.title": "Options terms",
  "terms.version": "Version {version} · published {date}",
  "terms.inShort": "In short",
  "terms.point.buy": "Buying an option: the most you can lose is what you pay.",
  "terms.point.sell": "Selling an option can lose more than you receive, and it uses margin.",
  "terms.point.prices": "Prices are set on the Kalks order book and by Kalks.",
  "terms.point.settle": "Options settle in cash at expiry.",
  "terms.englishNote": "The full text below is the binding version, in English.",
  "terms.acceptedOn": "You accepted version {version} on {date}.",
  "terms.close": "Close",
  "terms.unavailable": "The options terms aren't available right now. Please try again later.",

  // Kalks Trader button
  "trade.ready": "You're all set. Options trade in Kalks Trader, in your Options account.",
  "trade.cta": "Trade options in Kalks Trader",
  "trade.chooseAccount": "Choose an account",
  "trade.noAccount": "You need an active Options account to trade options.",
  "trade.openAccount": "Open an account",
  "trade.cashOnly": "Premiums and margin come from your account's own cash. Bonus and credit can't be used.",
  "trade.live": "Live",
  "trade.demo": "Demo",

  // Options page without an Options account
  "account.noneTitle": "No Options account yet",
  "account.noneText": "Options trade in their own account, apart from your CFD accounts. Open one in a minute, live or demo.",
  "account.open": "Open an Options account",

  // Key facts card
  "facts.title": "How Kalks FX Options work",
  "facts.style": "European style: exercised automatically at expiry, never before.",
  "facts.premium": "Premium in USD per contract; buyers pay it in full when they open.",
  "facts.contracts": "One contract: 10,000 units of a currency, 1 oz of gold, 50 oz of silver or 10 barrels of oil.",
  "facts.close": "Close at any time before expiry at the quoted price, fully or in part.",
  "facts.cutoff": "No new positions in the last 15 minutes before the cut.",
  "facts.margin": "Sellers hold margin based on stress scenarios; it can rise before weekends.",

  // Academy card
  "learn.title": "New to options?",
  "learn.text": "Take the free options course in the Academy: calls and puts, payoffs, the Greeks, strategies and the risks of selling.",
  "learn.cta": "Open the course",

  // Read-only sessions (view-only logins, staff sessions)
  "readOnly": "Only the client can start options trading, signed in to their own account.",

  // Loading errors
  "error.load": "Couldn't load your options status.",
  "error.retry": "Retry",

  // Demo build
  "demo.note": "Demo: nothing here is saved.",
};
export default options;
