import type { NsMessages } from "../../core";

// Kalks FX Options in the Client Area: the Options page, a friendly three-card intro and the button that opens Kalks Trader in options mode.
const options: NsMessages<"options"> = {
  // Navigation entry
  "nav.title": "Optionen",

  // Page header
  "page.title": "Kalks FX Options",
  "page.subtitle": "Kaufen oder verkaufen Sie Optionen auf Devisen, Gold, Silber und Öl, direkt in Kalks Trader.",
  "page.statusReady": "Bereit zum Handeln",
  "page.learnCourse": "Optionskurs",

  // Hero card
  "hero.eyebrow": "Neu in Kalks Trader",
  "hero.title": "Optionen auf 13 Märkte, ganz einfach",
  "hero.text": "Europäische Optionen auf Devisen-Majors und -Crosses, Gold, Silber und Rohöl. Wählen Sie tägliche, wöchentliche oder monatliche Verfallstermine. Jede Option wird bar in US-Dollar abgerechnet, Sie erhalten also nie eine physische Lieferung.",
  "hero.feature.underlyings.title": "13 Basiswerte",
  "hero.feature.underlyings.text": "9 Währungspaare, Gold, Silber sowie WTI- und Brent-Rohöl.",
  "hero.feature.expiries.title": "Täglich, wöchentlich, monatlich",
  "hero.feature.expiries.text": "Verfallstermine vom selben Tag bis zum Monatsende, Cut um 10:00 Uhr New Yorker Zeit.",
  "hero.feature.settlement.title": "Barausgleich in USD",
  "hero.feature.settlement.text": "Abgerechnet zum durchschnittlichen Mittelkurs der 30 Minuten vor dem Cut.",
  "hero.feature.sides.title": "Kaufen oder verkaufen",
  "hero.feature.sides.text": "Calls und Puts, Spreads, Straddles, Iron Condors und Barriere-Optionen.",
  "hero.class.forex": "Forex",
  "hero.class.metals": "Metalle",
  "hero.class.energies": "Energie",
  "hero.start": "Jetzt starten",
  "hero.howItWorks": "So funktionieren Optionen",

  // Friendly intro before the first options trade: three illustrated cards, one checkbox and the Start button
  "intro.title": "Optionen in drei einfachen Punkten",
  "intro.subtitle": "Ein kurzer Überblick vor Ihrem ersten Optionstrade.",
  "intro.call.title": "Call kaufen",
  "intro.call.text": "Sie erwarten, dass der Kurs steigt.",
  "intro.put.title": "Put kaufen",
  "intro.put.text": "Sie erwarten, dass der Kurs fällt.",
  "intro.risk.title": "Beim Kauf ist Ihr Risiko begrenzt",
  "intro.risk.text": "Sie können höchstens den Preis verlieren, den Sie zahlen. (Beim Verkauf von Optionen können Sie mehr verlieren.)",
  "intro.legend.result": "Ihr Ergebnis bei Verfall",
  "intro.legend.cost": "Der Preis, den Sie zahlen",
  "intro.confirm": "Ich verstehe, wie Optionen funktionieren",
  "intro.terms": "Vollständige Bedingungen lesen",
  "intro.consent": "Mit dem Start akzeptieren Sie die Optionsbedingungen.",
  "intro.start": "Optionshandel starten",
  "intro.quiz": "Testen Sie Ihr Wissen (Quiz)",
  "intro.gotIt": "Verstanden",
  "intro.toastStarted": "Alles bereit für den Optionshandel",
  "intro.toastFailed": "Der Optionshandel konnte nicht gestartet werden. Bitte versuchen Sie es erneut.",
  "intro.toastUpdated": "Die Optionsbedingungen wurden soeben aktualisiert. Sehen Sie sie sich kurz an und klicken Sie dann erneut auf Starten.",

  // Full terms dialog (the published text itself comes from the server, in English)
  "terms.title": "Optionsbedingungen",
  "terms.version": "Version {version} · veröffentlicht am {date}",
  "terms.inShort": "Kurz gesagt",
  "terms.point.buy": "Option kaufen: Sie können höchstens verlieren, was Sie zahlen.",
  "terms.point.sell": "Beim Verkauf einer Option können Sie mehr verlieren, als Sie erhalten, und es wird Margin benötigt.",
  "terms.point.prices": "Die Preise entstehen im Kalks-Orderbuch und werden auch direkt von Kalks gestellt.",
  "terms.point.settle": "Optionen werden bei Verfall bar abgerechnet.",
  "terms.englishNote": "Der vollständige Text unten ist die verbindliche Fassung, auf Englisch.",
  "terms.acceptedOn": "Sie haben Version {version} am {date} akzeptiert.",
  "terms.close": "Schließen",
  "terms.unavailable": "Die Optionsbedingungen sind derzeit nicht verfügbar. Bitte versuchen Sie es später erneut.",

  // Kalks Trader button
  "trade.ready": "Alles bereit. Optionen handeln Sie in Kalks Trader, auf Ihrem Optionskonto.",
  "trade.cta": "Optionen in Kalks Trader handeln",
  "trade.chooseAccount": "Konto auswählen",
  "trade.noAccount": "Sie benötigen ein aktives Optionskonto, um Optionen zu handeln.",
  "trade.openAccount": "Konto eröffnen",
  "trade.cashOnly": "Prämien und Margin stammen aus dem eigenen Barguthaben Ihres Kontos. Bonus und Kredit können nicht verwendet werden.",
  "trade.live": "Live",
  "trade.demo": "Demo",

  // Key facts card
  "facts.title": "So funktionieren Kalks FX Options",
  "facts.style": "Europäischer Stil: automatische Ausübung bei Verfall, nie vorher.",
  "facts.premium": "Prämie in USD pro Kontrakt; Käufer zahlen sie bei Eröffnung vollständig.",
  "facts.contracts": "Ein Kontrakt: 10,000 Einheiten einer Währung, 1 oz Gold, 50 oz Silber oder 10 Barrel Öl.",
  "facts.close": "Jederzeit vor dem Verfall zum quotierten Kurs schließen, ganz oder teilweise.",
  "facts.cutoff": "In den letzten 15 Minuten vor dem Cut keine neuen Positionen.",
  "facts.margin": "Verkäufer hinterlegen Margin auf Basis von Stressszenarien; sie kann vor Wochenenden steigen.",

  // Academy card
  "learn.title": "Neu bei Optionen?",
  "learn.text": "Belegen Sie den kostenlosen Optionskurs in der Academy: Calls und Puts, Auszahlungsprofile, die Griechen, Strategien und die Risiken des Verkaufs.",
  "learn.cta": "Kurs öffnen",

  // Read-only sessions (view-only logins, staff sessions)
  "readOnly": "Nur der Kunde selbst kann den Optionshandel starten, angemeldet in seinem eigenen Konto.",

  // Loading errors
  "error.load": "Ihr Optionsstatus konnte nicht geladen werden.",
  "error.retry": "Erneut versuchen",

  // Demo build
  "demo.note": "Demo: Hier wird nichts gespeichert.",
  // CFD / Options account split
  "account.noneTitle": "Noch kein Optionskonto",
  "account.noneText": "Optionen werden auf einem eigenen Konto gehandelt, getrennt von Ihren CFD-Konten. Eröffnen Sie eines in einer Minute, live oder demo.",
  "account.open": "Optionskonto eröffnen",
};
export default options;
