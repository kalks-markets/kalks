import type { NsMessages } from "../../core";

// Kalks FX Options in the Client Area: the Options page, a friendly three-card intro and the button that opens Kalks Trader in options mode.
const options: NsMessages<"options"> = {
  // Navigation entry
  "nav.title": "Options",

  // Page header
  "page.title": "Kalks FX Options",
  "page.subtitle": "Achetez ou vendez des options sur le forex, l'or, l'argent et le pétrole, directement dans Kalks Trader.",
  "page.statusReady": "Prêt à trader",
  "page.learnCourse": "Cours sur les options",

  // Hero card
  "hero.eyebrow": "Nouveau dans Kalks Trader",
  "hero.title": "Des options sur 13 marchés, en toute simplicité",
  "hero.text": "Des options européennes sur les principales paires de devises et les croisées, l'or, l'argent et le pétrole brut. Choisissez des échéances quotidiennes, hebdomadaires ou mensuelles. Toutes les options sont réglées en espèces, en dollars américains : vous ne recevez donc jamais de livraison physique.",
  "hero.feature.underlyings.title": "13 sous-jacents",
  "hero.feature.underlyings.text": "9 paires de devises, l'or, l'argent et le pétrole brut WTI et Brent.",
  "hero.feature.expiries.title": "Quotidiennes, hebdomadaires, mensuelles",
  "hero.feature.expiries.text": "Des échéances du jour même à la fin du mois, avec un cut à 10:00, heure de New York.",
  "hero.feature.settlement.title": "Réglées en espèces en USD",
  "hero.feature.settlement.text": "Réglées au cours mid moyen des 30 minutes précédant le cut.",
  "hero.feature.sides.title": "Achat ou vente",
  "hero.feature.sides.text": "Calls et puts, spreads, straddles, iron condors et options à barrière.",
  "hero.class.forex": "Forex",
  "hero.class.metals": "Métaux",
  "hero.class.energies": "Énergies",
  "hero.start": "Commencer",
  "hero.howItWorks": "Comment fonctionnent les options",

  // Friendly intro before the first options trade: three illustrated cards, one checkbox and the Start button
  "intro.title": "Les options en trois idées simples",
  "intro.subtitle": "Un rapide aperçu avant votre premier trade sur options.",
  "intro.call.title": "Acheter un call",
  "intro.call.text": "Vous pensez que le prix va monter.",
  "intro.put.title": "Acheter un put",
  "intro.put.text": "Vous pensez que le prix va baisser.",
  "intro.risk.title": "À l'achat, votre risque est limité",
  "intro.risk.text": "Vous ne pouvez pas perdre plus que le prix payé. (Vendre des options peut vous faire perdre davantage.)",
  "intro.legend.result": "Votre résultat à l'échéance",
  "intro.legend.cost": "Le prix que vous payez",
  "intro.confirm": "Je comprends le fonctionnement des options",
  "intro.terms": "Lire les conditions complètes",
  "intro.consent": "En commençant, vous acceptez les conditions des options.",
  "intro.start": "Commencer à trader des options",
  "intro.quiz": "Testez-vous (quiz)",
  "intro.gotIt": "Compris",
  "intro.toastStarted": "Tout est prêt pour trader des options",
  "intro.toastFailed": "Impossible de commencer à trader des options. Veuillez réessayer.",
  "intro.toastUpdated": "Les conditions des options viennent d'être mises à jour. Jetez-y un œil, puis cliquez à nouveau sur Commencer.",

  // Full terms dialog (the published text itself comes from the server, in English)
  "terms.title": "Conditions des options",
  "terms.version": "Version {version} · publiée le {date}",
  "terms.inShort": "En bref",
  "terms.point.buy": "Acheter une option : vous ne pouvez pas perdre plus que ce que vous payez.",
  "terms.point.sell": "Vendre une option peut vous faire perdre plus que ce que vous recevez, et mobilise de la marge.",
  "terms.point.prices": "Les prix se forment dans le carnet d'ordres de Kalks et sont aussi cotés directement par Kalks.",
  "terms.point.settle": "Les options sont réglées en espèces à l'échéance.",
  "terms.englishNote": "Le texte intégral ci-dessous, en anglais, est la version qui fait foi.",
  "terms.acceptedOn": "Vous avez accepté la version {version} le {date}.",
  "terms.close": "Fermer",
  "terms.unavailable": "Les conditions des options ne sont pas disponibles pour le moment. Veuillez réessayer plus tard.",

  // Kalks Trader button
  "trade.ready": "Tout est prêt. Les options se tradent dans Kalks Trader, sur votre compte options.",
  "trade.cta": "Trader des options dans Kalks Trader",
  "trade.chooseAccount": "Choisissez un compte",
  "trade.noAccount": "Vous avez besoin d'un compte options actif pour trader des options.",
  "trade.openAccount": "Ouvrir un compte",
  "trade.cashOnly": "Les primes et la marge sont prélevées sur les liquidités propres de votre compte. Le bonus et le crédit ne peuvent pas être utilisés.",
  "trade.live": "Réel",
  "trade.demo": "Démo",

  // Key facts card
  "facts.title": "Fonctionnement des Kalks FX Options",
  "facts.style": "Style européen : exercice automatique à l'échéance, jamais avant.",
  "facts.premium": "Prime en USD par contrat ; l'acheteur la paie intégralement à l'ouverture.",
  "facts.contracts": "Un contrat : 10,000 unités d'une devise, 1 oz d'or, 50 oz d'argent ou 10 barils de pétrole.",
  "facts.close": "Clôture possible à tout moment avant l'échéance au prix coté, en totalité ou en partie.",
  "facts.cutoff": "Aucune nouvelle position dans les 15 dernières minutes avant le cut.",
  "facts.margin": "Les vendeurs détiennent une marge fondée sur des scénarios de stress ; elle peut augmenter avant les week-ends.",

  // Academy card
  "learn.title": "Débutant en options ?",
  "learn.text": "Suivez le cours gratuit sur les options dans l'Académie : calls et puts, profils de gain, les grecques, les stratégies et les risques de la vente.",
  "learn.cta": "Ouvrir le cours",

  // Read-only sessions (view-only logins, staff sessions)
  "readOnly": "Seul le client, connecté à son propre compte, peut commencer à trader des options.",

  // Loading errors
  "error.load": "Impossible de charger le statut de vos options.",
  "error.retry": "Réessayer",

  // Demo build
  "demo.note": "Démo : rien n'est enregistré ici.",
  // CFD / Options account split
  "account.noneTitle": "Pas encore de compte options",
  "account.noneText": "Les options se tradent sur leur propre compte, séparé de vos comptes CFD. Ouvrez-en un en une minute, réel ou démo.",
  "account.open": "Ouvrir un compte options",
};
export default options;
