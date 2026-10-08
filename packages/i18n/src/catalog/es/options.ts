import type { NsMessages } from "../../core";

// Kalks FX Options in the Client Area: the Options page, a friendly three-card intro and the button that opens Kalks Trader in options mode.
const options: NsMessages<"options"> = {
  // Navigation entry
  "nav.title": "Opciones",

  // Page header
  "page.title": "Kalks FX Options",
  "page.subtitle": "Compre o venda opciones sobre divisas, oro, plata y petróleo, directamente en Kalks Trader.",
  "page.statusReady": "Listo para operar",
  "page.learnCourse": "Curso de opciones",

  // Hero card
  "hero.eyebrow": "Novedad en Kalks Trader",
  "hero.title": "Opciones sobre 13 mercados, sin complicaciones",
  "hero.text": "Opciones europeas sobre los principales pares de divisas y cruces, oro, plata y petróleo crudo. Elija vencimientos diarios, semanales o mensuales. Todas las opciones se liquidan en efectivo, en dólares estadounidenses, por lo que nunca recibe la entrega física de nada.",
  "hero.feature.underlyings.title": "13 subyacentes",
  "hero.feature.underlyings.text": "9 pares de divisas, oro, plata y petróleo WTI y Brent.",
  "hero.feature.expiries.title": "Diarios, semanales, mensuales",
  "hero.feature.expiries.text": "Vencimientos desde el mismo día hasta fin de mes, con corte a las 10:00 hora de Nueva York.",
  "hero.feature.settlement.title": "Liquidación en efectivo en USD",
  "hero.feature.settlement.text": "Se liquidan al precio mid promedio de los 30 minutos anteriores al corte.",
  "hero.feature.sides.title": "Compre o venda",
  "hero.feature.sides.text": "Calls y puts, spreads, straddles, iron condors y opciones con barrera.",
  "hero.class.forex": "Forex",
  "hero.class.metals": "Metales",
  "hero.class.energies": "Energías",
  "hero.start": "Empezar",
  "hero.howItWorks": "Cómo funcionan las opciones",

  // Friendly intro before the first options trade: three illustrated cards, one checkbox and the Start button
  "intro.title": "Las opciones en tres ideas sencillas",
  "intro.subtitle": "Un vistazo rápido antes de su primera operación con opciones.",
  "intro.call.title": "Comprar una call",
  "intro.call.text": "Cree que el precio subirá.",
  "intro.put.title": "Comprar una put",
  "intro.put.text": "Cree que el precio bajará.",
  "intro.risk.title": "Al comprar, su riesgo es limitado",
  "intro.risk.text": "Lo máximo que puede perder es el precio que paga. (Vender opciones puede hacerle perder más.)",
  "intro.legend.result": "Su resultado al vencimiento",
  "intro.legend.cost": "El precio que paga",
  "intro.confirm": "Entiendo cómo funcionan las opciones",
  "intro.terms": "Leer las condiciones completas",
  "intro.consent": "Al empezar, acepta las condiciones de las opciones.",
  "intro.start": "Empezar a operar con opciones",
  "intro.quiz": "Póngase a prueba (cuestionario)",
  "intro.gotIt": "Entendido",
  "intro.toastStarted": "Todo listo para operar con opciones",
  "intro.toastFailed": "No se pudo empezar a operar con opciones. Inténtelo de nuevo.",
  "intro.toastUpdated": "Las condiciones de las opciones se acaban de actualizar. Écheles un vistazo y vuelva a hacer clic en Empezar.",

  // Full terms dialog (the published text itself comes from the server, in English)
  "terms.title": "Condiciones de las opciones",
  "terms.version": "Versión {version} · publicada el {date}",
  "terms.inShort": "En resumen",
  "terms.point.buy": "Comprar una opción: lo máximo que puede perder es lo que paga.",
  "terms.point.sell": "Vender una opción puede hacerle perder más de lo que recibe, y requiere margen.",
  "terms.point.prices": "Los precios se forman en el libro de órdenes de Kalks y también los cotiza Kalks directamente.",
  "terms.point.settle": "Las opciones se liquidan en efectivo al vencimiento.",
  "terms.englishNote": "El texto completo que figura a continuación es la versión vinculante, en inglés.",
  "terms.acceptedOn": "Aceptó la versión {version} el {date}.",
  "terms.close": "Cerrar",
  "terms.unavailable": "Las condiciones de las opciones no están disponibles en este momento. Inténtelo de nuevo más tarde.",

  // Kalks Trader button
  "trade.ready": "Todo listo. Las opciones se operan en Kalks Trader, en su cuenta de opciones.",
  "trade.cta": "Operar con opciones en Kalks Trader",
  "trade.chooseAccount": "Elija una cuenta",
  "trade.noAccount": "Necesita una cuenta de opciones activa para operar con opciones.",
  "trade.openAccount": "Abrir una cuenta",
  "trade.cashOnly": "Las primas y el margen se cubren con el efectivo propio de su cuenta. No se pueden usar el bono ni el crédito.",
  "trade.live": "Real",
  "trade.demo": "Demo",

  // Key facts card
  "facts.title": "Cómo funcionan las Kalks FX Options",
  "facts.style": "Estilo europeo: se ejercen automáticamente al vencimiento, nunca antes.",
  "facts.premium": "Prima en USD por contrato; los compradores la pagan íntegramente al abrir.",
  "facts.contracts": "Un contrato: 10,000 unidades de una divisa, 1 oz de oro, 50 oz de plata o 10 barriles de petróleo.",
  "facts.close": "Cierre en cualquier momento antes del vencimiento al precio cotizado, total o parcialmente.",
  "facts.cutoff": "No se pueden abrir nuevas posiciones en los últimos 15 minutos antes del corte.",
  "facts.margin": "Los vendedores mantienen un margen basado en escenarios de estrés; puede aumentar antes de los fines de semana.",

  // Academy card
  "learn.title": "¿Es nuevo en opciones?",
  "learn.text": "Haga el curso gratuito de opciones en la Academia: calls y puts, perfiles de pago, las griegas, estrategias y los riesgos de vender.",
  "learn.cta": "Abrir el curso",

  // Read-only sessions (view-only logins, staff sessions)
  "readOnly": "Solo el cliente, con la sesión iniciada en su propia cuenta, puede empezar a operar con opciones.",

  // Loading errors
  "error.load": "No se pudo cargar el estado de sus opciones.",
  "error.retry": "Reintentar",

  // Demo build
  "demo.note": "Demo: aquí no se guarda nada.",
  // CFD / Options account split
  "account.noneTitle": "Aún no tiene una cuenta de opciones",
  "account.noneText": "Las opciones se operan en su propia cuenta, aparte de sus cuentas de CFD. Abra una en un minuto, real o demo.",
  "account.open": "Abrir una cuenta de opciones",
};
export default options;
