import type { NsMessages } from "../../core";

// Kalks FX Options in the Client Area: the Options page, a friendly three-card intro and the button that opens Kalks Trader in options mode.
const options: NsMessages<"options"> = {
  // Navigation entry
  "nav.title": "Opzioni",

  // Page header
  "page.title": "Kalks FX Options",
  "page.subtitle": "Compra o vendi opzioni su forex, oro, argento e petrolio, direttamente in Kalks Trader.",
  "page.statusReady": "Pronto per operare",
  "page.learnCourse": "Corso sulle opzioni",

  // Hero card
  "hero.eyebrow": "Novità in Kalks Trader",
  "hero.title": "Opzioni su 13 mercati, in tutta semplicità",
  "hero.text": "Opzioni europee sulle principali coppie forex e sui cross, oro, argento e petrolio greggio. Scegli scadenze giornaliere, settimanali o mensili. Ogni opzione è regolata in contanti, in dollari USA, quindi non ricevi mai alcuna consegna fisica.",
  "hero.feature.underlyings.title": "13 sottostanti",
  "hero.feature.underlyings.text": "9 coppie forex, oro, argento e petrolio greggio WTI e Brent.",
  "hero.feature.expiries.title": "Giornaliere, settimanali, mensili",
  "hero.feature.expiries.text": "Scadenze dallo stesso giorno fino a fine mese, con cut alle 10:00 ora di New York.",
  "hero.feature.settlement.title": "Regolamento in contanti in USD",
  "hero.feature.settlement.text": "Regolate al prezzo mid medio dei 30 minuti che precedono il cut.",
  "hero.feature.sides.title": "Compra o vendi",
  "hero.feature.sides.text": "Call e put, spread, straddle, iron condor e opzioni con barriera.",
  "hero.class.forex": "Forex",
  "hero.class.metals": "Metalli",
  "hero.class.energies": "Energie",
  "hero.start": "Inizia",
  "hero.howItWorks": "Come funzionano le opzioni",

  // Friendly intro before the first options trade: three illustrated cards, one checkbox and the Start button
  "intro.title": "Le opzioni in tre semplici idee",
  "intro.subtitle": "Uno sguardo veloce prima della tua prima operazione in opzioni.",
  "intro.call.title": "Compra una call",
  "intro.call.text": "Pensi che il prezzo salirà.",
  "intro.put.title": "Compra una put",
  "intro.put.text": "Pensi che il prezzo scenderà.",
  "intro.risk.title": "Quando compri, il tuo rischio è limitato",
  "intro.risk.text": "Il massimo che puoi perdere è il prezzo che paghi. (Vendendo opzioni puoi perdere di più.)",
  "intro.legend.result": "Il tuo risultato a scadenza",
  "intro.legend.cost": "Il prezzo che paghi",
  "intro.confirm": "Ho capito come funzionano le opzioni",
  "intro.terms": "Leggi le condizioni complete",
  "intro.consent": "Iniziando, accetti le condizioni delle opzioni.",
  "intro.start": "Inizia a fare trading di opzioni",
  "intro.quiz": "Mettiti alla prova (quiz)",
  "intro.gotIt": "Capito",
  "intro.toastStarted": "Tutto pronto per le opzioni",
  "intro.toastFailed": "Impossibile iniziare il trading di opzioni. Riprova.",
  "intro.toastUpdated": "Le condizioni delle opzioni sono appena state aggiornate. Dai un'occhiata veloce, poi premi di nuovo Inizia.",

  // Full terms dialog (the published text itself comes from the server, in English)
  "terms.title": "Condizioni delle opzioni",
  "terms.version": "Versione {version} · pubblicata il {date}",
  "terms.inShort": "In breve",
  "terms.point.buy": "Acquistare un'opzione: il massimo che puoi perdere è quanto paghi.",
  "terms.point.sell": "Vendere un'opzione può farti perdere più di quanto incassi e richiede margine.",
  "terms.point.prices": "I prezzi si formano nel book di negoziazione di Kalks e sono anche quotati direttamente da Kalks.",
  "terms.point.settle": "Le opzioni sono regolate in contanti a scadenza.",
  "terms.englishNote": "Il testo completo qui sotto, in inglese, è la versione vincolante.",
  "terms.acceptedOn": "Hai accettato la versione {version} il {date}.",
  "terms.close": "Chiudi",
  "terms.unavailable": "Le condizioni delle opzioni non sono disponibili al momento. Riprova più tardi.",

  // Kalks Trader button
  "trade.ready": "Tutto pronto. Le opzioni si negoziano in Kalks Trader, sul tuo conto opzioni.",
  "trade.cta": "Fai trading di opzioni in Kalks Trader",
  "trade.chooseAccount": "Scegli un conto",
  "trade.noAccount": "Ti serve un conto opzioni attivo per negoziare opzioni.",
  "trade.openAccount": "Apri un conto",
  "trade.cashOnly": "Premi e margine provengono dalla liquidità propria del conto. Bonus e credito non possono essere utilizzati.",
  "trade.live": "Reale",
  "trade.demo": "Demo",

  // Key facts card
  "facts.title": "Come funzionano le Kalks FX Options",
  "facts.style": "Stile europeo: esercizio automatico alla scadenza, mai prima.",
  "facts.premium": "Premio in USD per contratto; chi acquista lo paga per intero all'apertura.",
  "facts.contracts": "Un contratto: 10,000 unità di una valuta, 1 oz di oro, 50 oz di argento o 10 barili di petrolio.",
  "facts.close": "Chiudi in qualsiasi momento prima della scadenza al prezzo quotato, in tutto o in parte.",
  "facts.cutoff": "Nessuna nuova posizione negli ultimi 15 minuti prima del cut.",
  "facts.margin": "I venditori detengono un margine basato su scenari di stress; può aumentare prima dei fine settimana.",

  // Academy card
  "learn.title": "Nuovo alle opzioni?",
  "learn.text": "Segui il corso gratuito sulle opzioni nell'Academy: call e put, profili di payoff, le greche, le strategie e i rischi della vendita.",
  "learn.cta": "Apri il corso",

  // Read-only sessions (view-only logins, staff sessions)
  "readOnly": "Solo il cliente, collegato al proprio account, può iniziare il trading di opzioni.",

  // Loading errors
  "error.load": "Impossibile caricare lo stato delle tue opzioni.",
  "error.retry": "Riprova",

  // Demo build
  "demo.note": "Demo: qui non viene salvato nulla.",
  // CFD / Options account split
  "account.noneTitle": "Nessun conto opzioni per ora",
  "account.noneText": "Le opzioni si negoziano su un conto dedicato, separato dai tuoi conti CFD. Aprine uno in un minuto, reale o demo.",
  "account.open": "Apri un conto opzioni",
};
export default options;
