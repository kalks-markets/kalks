import type { NsMessages } from "../../core";

// Kalks FX Options in the Client Area: the Options page, a friendly three-card intro and the button that opens Kalks Trader in options mode.
const options: NsMessages<"options"> = {
  // Navigation entry
  "nav.title": "Opções",

  // Page header
  "page.title": "Kalks FX Options",
  "page.subtitle": "Compre ou venda opções de forex, ouro, prata e petróleo, direto no Kalks Trader.",
  "page.statusReady": "Pronto para negociar",
  "page.learnCourse": "Curso de opções",

  // Hero card
  "hero.eyebrow": "Novidade no Kalks Trader",
  "hero.title": "Opções em 13 mercados, sem complicação",
  "hero.text": "Opções europeias sobre os principais pares de moedas e cruzamentos, ouro, prata e petróleo bruto. Escolha vencimentos diários, semanais ou mensais. Todas as opções são liquidadas em dinheiro, em dólares americanos, então você nunca recebe a entrega física de nada.",
  "hero.feature.underlyings.title": "13 ativos subjacentes",
  "hero.feature.underlyings.text": "9 pares de moedas, ouro, prata e petróleo WTI e Brent.",
  "hero.feature.expiries.title": "Diários, semanais, mensais",
  "hero.feature.expiries.text": "Vencimentos do mesmo dia até o fim do mês, com corte às 10:00 de Nova York.",
  "hero.feature.settlement.title": "Liquidação financeira em USD",
  "hero.feature.settlement.text": "Liquidadas pela média do preço mid nos 30 minutos anteriores ao corte.",
  "hero.feature.sides.title": "Compre ou venda",
  "hero.feature.sides.text": "Calls e puts, spreads, straddles, iron condors e opções com barreira.",
  "hero.class.forex": "Forex",
  "hero.class.metals": "Metais",
  "hero.class.energies": "Energia",
  "hero.start": "Começar",
  "hero.howItWorks": "Como as opções funcionam",

  // Friendly intro before the first options trade: three illustrated cards, one checkbox and the Start button
  "intro.title": "Opções em três ideias simples",
  "intro.subtitle": "Uma visão rápida antes da sua primeira operação com opções.",
  "intro.call.title": "Comprar uma call",
  "intro.call.text": "Você acha que o preço vai subir.",
  "intro.put.title": "Comprar uma put",
  "intro.put.text": "Você acha que o preço vai cair.",
  "intro.risk.title": "Na compra, seu risco é limitado",
  "intro.risk.text": "O máximo que você pode perder é o preço que paga. (Vendendo opções, você pode perder mais.)",
  "intro.legend.result": "Seu resultado no vencimento",
  "intro.legend.cost": "O preço que você paga",
  "intro.confirm": "Entendo como as opções funcionam",
  "intro.terms": "Ler os termos completos",
  "intro.consent": "Ao começar, você aceita os termos de opções.",
  "intro.start": "Começar a negociar opções",
  "intro.quiz": "Teste seus conhecimentos (quiz)",
  "intro.gotIt": "Entendi",
  "intro.toastStarted": "Tudo pronto para negociar opções",
  "intro.toastFailed": "Não foi possível começar a negociar opções. Tente novamente.",
  "intro.toastUpdated": "Os termos de opções acabaram de ser atualizados. Dê uma olhada rápida e clique em Começar novamente.",

  // Full terms dialog (the published text itself comes from the server, in English)
  "terms.title": "Termos de opções",
  "terms.version": "Versão {version} · publicada em {date}",
  "terms.inShort": "Em resumo",
  "terms.point.buy": "Comprar uma opção: o máximo que você pode perder é o que paga.",
  "terms.point.sell": "Vender uma opção pode fazer você perder mais do que recebe, e exige margem.",
  "terms.point.prices": "Os preços são formados no livro de ofertas da Kalks e também cotados diretamente pela Kalks.",
  "terms.point.settle": "As opções são liquidadas em dinheiro no vencimento.",
  "terms.englishNote": "O texto completo abaixo é a versão vinculante, em inglês.",
  "terms.acceptedOn": "Você aceitou a versão {version} em {date}.",
  "terms.close": "Fechar",
  "terms.unavailable": "Os termos de opções não estão disponíveis no momento. Tente novamente mais tarde.",

  // Kalks Trader button
  "trade.ready": "Tudo pronto. As opções são negociadas no Kalks Trader, na sua conta de opções.",
  "trade.cta": "Negociar opções no Kalks Trader",
  "trade.chooseAccount": "Escolha uma conta",
  "trade.noAccount": "Você precisa de uma conta de opções ativa para negociar opções.",
  "trade.openAccount": "Abrir uma conta",
  "trade.cashOnly": "Prêmios e margem saem do saldo em dinheiro da própria conta. Bônus e crédito não podem ser usados.",
  "trade.live": "Real",
  "trade.demo": "Demo",

  // Key facts card
  "facts.title": "Como funcionam as Kalks FX Options",
  "facts.style": "Estilo europeu: exercidas automaticamente no vencimento, nunca antes.",
  "facts.premium": "Prêmio em USD por contrato; o comprador o paga integralmente na abertura.",
  "facts.contracts": "Um contrato: 10,000 unidades de uma moeda, 1 oz de ouro, 50 oz de prata ou 10 barris de petróleo.",
  "facts.close": "Feche a qualquer momento antes do vencimento pelo preço cotado, total ou parcialmente.",
  "facts.cutoff": "Não é possível abrir novas posições nos últimos 15 minutos antes do corte.",
  "facts.margin": "Os vendedores mantêm margem baseada em cenários de estresse; ela pode aumentar antes dos fins de semana.",

  // Academy card
  "learn.title": "Novo em opções?",
  "learn.text": "Faça o curso gratuito de opções na Academia: calls e puts, perfis de payoff, as gregas, estratégias e os riscos de vender.",
  "learn.cta": "Abrir o curso",

  // Read-only sessions (view-only logins, staff sessions)
  "readOnly": "Somente o cliente, conectado à própria conta, pode começar a negociar opções.",

  // Loading errors
  "error.load": "Não foi possível carregar o status das suas opções.",
  "error.retry": "Tentar novamente",

  // Demo build
  "demo.note": "Demo: nada aqui é salvo.",
  // CFD / Options account split
  "account.noneTitle": "Ainda não tem conta de opções",
  "account.noneText": "As opções são negociadas numa conta própria, separada das suas contas de CFD. Abra uma em um minuto, real ou demo.",
  "account.open": "Abrir uma conta de opções",
};
export default options;
