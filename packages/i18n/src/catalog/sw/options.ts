import type { NsMessages } from "../../core";

// Kalks FX Options in the Client Area: the Options page, a friendly three-card intro and the button that opens Kalks Trader in options mode.
// Kalks, Kalks Trader and Kalks FX Options stay in English; option terms (call, put, strike, premium, delta…) stay as traders use them.
const options: NsMessages<"options"> = {
  // Navigation entry
  "nav.title": "Options",

  // Page header
  "page.title": "Kalks FX Options",
  "page.subtitle": "Nunua au uza options za forex, dhahabu, fedha na mafuta, moja kwa moja ndani ya Kalks Trader.",
  "page.statusReady": "Uko tayari kufanya biashara",
  "page.learnCourse": "Kozi ya options",

  // Hero card
  "hero.eyebrow": "Mpya kwenye Kalks Trader",
  "hero.title": "Options kwenye masoko 13, kwa urahisi",
  "hero.text": "Options za mtindo wa Ulaya kwenye jozi kuu na jozi mseto za forex, dhahabu, fedha na mafuta ghafi. Chagua tarehe za kuisha za kila siku, kila wiki au kila mwezi. Kila option hulipwa kwa pesa taslimu, kwa dola za Marekani, kwa hivyo kamwe hupokei bidhaa yoyote halisi.",
  "hero.feature.underlyings.title": "Mali 13 za msingi",
  "hero.feature.underlyings.text": "Jozi 9 za forex, dhahabu, fedha, na mafuta ghafi ya WTI na Brent.",
  "hero.feature.expiries.title": "Kila siku, kila wiki, kila mwezi",
  "hero.feature.expiries.text": "Tarehe za kuisha kuanzia siku hiyo hiyo hadi mwisho wa mwezi, muda wa cut ukiwa 10:00 New York.",
  "hero.feature.settlement.title": "Malipo ya pesa taslimu kwa USD",
  "hero.feature.settlement.text": "Hulipwa kwa wastani wa bei ya kati ya dakika 30 kabla ya cut.",
  "hero.feature.sides.title": "Nunua au uza",
  "hero.feature.sides.text": "Call na put, spreads, straddles, iron condors na options za barrier.",
  "hero.class.forex": "Forex",
  "hero.class.metals": "Metali",
  "hero.class.energies": "Nishati",
  "hero.start": "Anza sasa",
  "hero.howItWorks": "Jinsi options zinavyofanya kazi",

  // Friendly intro before the first options trade: three illustrated cards, one checkbox and the Start button
  "intro.title": "Options kwa mambo matatu rahisi",
  "intro.subtitle": "Mtazamo mfupi kabla ya biashara yako ya kwanza ya options.",
  "intro.call.title": "Nunua Call",
  "intro.call.text": "Unadhani bei itapanda.",
  "intro.put.title": "Nunua Put",
  "intro.put.text": "Unadhani bei itashuka.",
  "intro.risk.title": "Hatari yako ina kikomo unaponunua",
  "intro.risk.text": "Kiwango cha juu unachoweza kupoteza ni bei unayolipa. (Ukiuza options, unaweza kupoteza zaidi.)",
  "intro.legend.result": "Matokeo yako wakati wa kuisha",
  "intro.legend.cost": "Bei unayolipa",
  "intro.confirm": "Ninaelewa jinsi options zinavyofanya kazi",
  "intro.terms": "Soma masharti kamili",
  "intro.consent": "Kwa kuanza, unakubali masharti ya options.",
  "intro.start": "Anza biashara ya options",
  "intro.quiz": "Jipime (maswali)",
  "intro.gotIt": "Nimeelewa",
  "intro.toastStarted": "Uko tayari kwa biashara ya options",
  "intro.toastFailed": "Imeshindwa kuanza biashara ya options. Tafadhali jaribu tena.",
  "intro.toastUpdated": "Masharti ya options yamesasishwa sasa hivi. Yapitie haraka, kisha ubonyeze Anza tena.",

  // Full terms dialog (the published text itself comes from the server, in English)
  "terms.title": "Masharti ya options",
  "terms.version": "Toleo {version} · ilichapishwa {date}",
  "terms.inShort": "Kwa ufupi",
  "terms.point.buy": "Ukinunua option: kiwango cha juu unachoweza kupoteza ni kile unacholipa.",
  "terms.point.sell": "Kuuza option kunaweza kukupotezea zaidi ya unachopokea, na kunatumia margin.",
  "terms.point.prices": "Bei hupangwa kwenye order book ya Kalks na pia hutolewa na Kalks.",
  "terms.point.settle": "Options hulipwa kwa pesa taslimu wakati wa kuisha.",
  "terms.englishNote": "Maandishi kamili hapa chini, kwa Kiingereza, ndiyo toleo linalofunga kisheria.",
  "terms.acceptedOn": "Ulikubali toleo {version} tarehe {date}.",
  "terms.close": "Funga",
  "terms.unavailable": "Masharti ya options hayapatikani kwa sasa. Tafadhali jaribu tena baadaye.",

  // Kalks Trader button
  "trade.ready": "Uko tayari. Options hufanyiwa biashara kwenye Kalks Trader, kwenye akaunti yako ya Options.",
  "trade.cta": "Fanya biashara ya options kwenye Kalks Trader",
  "trade.chooseAccount": "Chagua akaunti",
  "trade.noAccount": "Unahitaji akaunti ya Options inayotumika ili kufanya biashara ya options.",
  "trade.openAccount": "Fungua akaunti",
  "trade.cashOnly": "Premium na margin hutoka kwenye pesa taslimu za akaunti yako yenyewe. Bonasi na mkopo haviwezi kutumika.",
  "trade.live": "Halisi",
  "trade.demo": "Demo",

  // Key facts card
  "facts.title": "Jinsi Kalks FX Options zinavyofanya kazi",
  "facts.style": "Mtindo wa Ulaya: hutekelezwa kiotomatiki wakati wa kuisha, kamwe si kabla.",
  "facts.premium": "Premium kwa USD kwa kila mkataba; wanunuzi huilipa yote wanapofungua.",
  "facts.contracts": "Mkataba mmoja: vitengo 10,000 vya sarafu, wakia 1 ya dhahabu, wakia 50 za fedha au mapipa 10 ya mafuta.",
  "facts.close": "Funga wakati wowote kabla ya kuisha kwa bei iliyonukuliwa, yote au sehemu.",
  "facts.cutoff": "Hakuna nafasi mpya katika dakika 15 za mwisho kabla ya cut.",
  "facts.margin": "Wauzaji huweka margin kulingana na hali za msongo; inaweza kupanda kabla ya wikendi.",

  // Academy card
  "learn.title": "Wewe ni mgeni kwenye options?",
  "learn.text": "Soma kozi ya bure ya options kwenye Academy: call na put, malipo, Greeks, mikakati na hatari za kuuza.",
  "learn.cta": "Fungua kozi",

  // Read-only sessions (view-only logins, staff sessions)
  "readOnly": "Ni mteja pekee, akiwa ameingia kwenye akaunti yake mwenyewe, anayeweza kuanza biashara ya options.",

  // Loading errors
  "error.load": "Imeshindwa kupakia hali yako ya options.",
  "error.retry": "Jaribu tena",

  // Demo build
  "demo.note": "Demo: hakuna kinachohifadhiwa hapa.",
  // CFD / Options account split
  "account.noneTitle": "Bado huna akaunti ya Options",
  "account.noneText": "Options hufanyiwa biashara kwenye akaunti yake, tofauti na akaunti zako za CFD. Fungua moja kwa dakika moja, halisi au demo.",
  "account.open": "Fungua akaunti ya Options",
};
export default options;
