import type { NsMessages } from "../../core";

// Kalks FX Options in the Client Area: the Options page, a friendly three-card intro and the button that opens Kalks Trader in options mode.
const options: NsMessages<"options"> = {
  // Navigation entry
  "nav.title": "옵션",

  // Page header
  "page.title": "Kalks FX Options",
  "page.subtitle": "Kalks Trader에서 바로 외환, 금, 은, 원유 옵션을 매수하거나 매도하세요.",
  "page.statusReady": "거래 가능",
  "page.learnCourse": "옵션 과정",

  // Hero card
  "hero.eyebrow": "Kalks Trader의 새로운 기능",
  "hero.title": "13개 시장의 옵션, 쉽고 간단하게",
  "hero.text": "외환 메이저 및 크로스 통화쌍, 금, 은, 원유를 대상으로 하는 유럽형 옵션입니다. 일간, 주간, 월간 만기 중에서 선택할 수 있습니다. 모든 옵션은 미국 달러로 현금결제되므로 실물을 인수하는 일은 없습니다.",
  "hero.feature.underlyings.title": "13개 기초자산",
  "hero.feature.underlyings.text": "외환 통화쌍 9개, 금, 은, WTI 원유, 브렌트유.",
  "hero.feature.expiries.title": "일간, 주간, 월간",
  "hero.feature.expiries.text": "당일부터 월말까지의 만기, 마감 시각은 뉴욕 시간 10:00입니다.",
  "hero.feature.settlement.title": "USD 현금결제",
  "hero.feature.settlement.text": "마감 전 30분 동안의 평균 중간가격으로 결제됩니다.",
  "hero.feature.sides.title": "매수 또는 매도",
  "hero.feature.sides.text": "콜과 풋, 스프레드, 스트래들, 아이언 콘도르, 배리어 옵션.",
  "hero.class.forex": "외환",
  "hero.class.metals": "금속",
  "hero.class.energies": "에너지",
  "hero.start": "지금 시작하기",
  "hero.howItWorks": "옵션의 작동 방식",

  // Friendly intro before the first options trade: three illustrated cards, one checkbox and the Start button
  "intro.title": "세 가지로 쉽게 이해하는 옵션",
  "intro.subtitle": "첫 옵션 거래 전에 가볍게 살펴보세요.",
  "intro.call.title": "콜 매수",
  "intro.call.text": "가격이 오를 것으로 예상할 때.",
  "intro.put.title": "풋 매수",
  "intro.put.text": "가격이 내릴 것으로 예상할 때.",
  "intro.risk.title": "매수하면 위험이 제한됩니다",
  "intro.risk.text": "최대 손실은 지불한 가격입니다. (옵션을 매도하면 더 큰 손실이 날 수 있습니다.)",
  "intro.legend.result": "만기 시 손익",
  "intro.legend.cost": "지불하는 가격",
  "intro.confirm": "옵션의 작동 방식을 이해합니다",
  "intro.terms": "전체 약관 읽기",
  "intro.consent": "시작하면 옵션 약관에 동의하게 됩니다.",
  "intro.start": "옵션 거래 시작하기",
  "intro.quiz": "실력 확인하기(퀴즈)",
  "intro.gotIt": "알겠습니다",
  "intro.toastStarted": "옵션 거래 준비가 완료되었습니다",
  "intro.toastFailed": "옵션 거래를 시작하지 못했습니다. 다시 시도해 주세요.",
  "intro.toastUpdated": "옵션 약관이 방금 업데이트되었습니다. 간단히 확인한 뒤 '시작하기'를 다시 눌러 주세요.",

  // Full terms dialog (the published text itself comes from the server, in English)
  "terms.title": "옵션 약관",
  "terms.version": "버전 {version} · {date} 게시",
  "terms.inShort": "요약",
  "terms.point.buy": "옵션 매수: 최대 손실은 지불한 금액입니다.",
  "terms.point.sell": "옵션 매도는 받은 금액보다 더 큰 손실이 날 수 있으며, 증거금이 사용됩니다.",
  "terms.point.prices": "가격은 Kalks 호가창에서 정해지며, Kalks가 직접 제시하기도 합니다.",
  "terms.point.settle": "옵션은 만기에 현금으로 결제됩니다.",
  "terms.englishNote": "아래 전문은 법적 구속력이 있는 영문본입니다.",
  "terms.acceptedOn": "{date}에 버전 {version}에 동의했습니다.",
  "terms.close": "닫기",
  "terms.unavailable": "지금은 옵션 약관을 불러올 수 없습니다. 잠시 후 다시 시도하세요.",

  // Kalks Trader button
  "trade.ready": "준비가 끝났습니다. 옵션은 Kalks Trader에서 옵션 계좌로 거래합니다.",
  "trade.cta": "Kalks Trader에서 옵션 거래",
  "trade.chooseAccount": "계좌 선택",
  "trade.noAccount": "옵션을 거래하려면 활성 옵션 계좌가 필요합니다.",
  "trade.openAccount": "계좌 개설",
  "trade.cashOnly": "프리미엄과 증거금에는 계좌의 자체 현금만 사용됩니다. 보너스와 크레딧은 사용할 수 없습니다.",
  "trade.live": "실계좌",
  "trade.demo": "데모",

  // Key facts card
  "facts.title": "Kalks FX Options 작동 방식",
  "facts.style": "유럽형: 만기에 자동으로 행사되며, 만기 전에는 행사되지 않습니다.",
  "facts.premium": "프리미엄은 계약당 USD로 표시되며, 매수자는 포지션을 열 때 전액을 지불합니다.",
  "facts.contracts": "1계약: 통화 10,000단위, 금 1온스, 은 50온스 또는 원유 10배럴.",
  "facts.close": "만기 전 언제든 호가로 전부 또는 일부를 청산할 수 있습니다.",
  "facts.cutoff": "마감 전 마지막 15분 동안은 신규 포지션을 열 수 없습니다.",
  "facts.margin": "매도자는 스트레스 시나리오에 기반한 증거금을 유지해야 하며, 증거금은 주말 전에 늘어날 수 있습니다.",

  // Academy card
  "learn.title": "옵션이 처음이신가요?",
  "learn.text": "아카데미에서 무료 옵션 과정을 수강하세요. 콜과 풋, 손익 구조, 그릭스, 전략, 매도의 위험을 배울 수 있습니다.",
  "learn.cta": "과정 열기",

  // Read-only sessions (view-only logins, staff sessions)
  "readOnly": "옵션 거래 시작은 고객 본인이 자신의 계정에 로그인한 상태에서만 할 수 있습니다.",

  // Loading errors
  "error.load": "옵션 이용 상태를 불러오지 못했습니다.",
  "error.retry": "다시 시도",

  // Demo build
  "demo.note": "데모: 여기서는 아무것도 저장되지 않습니다.",
  // CFD / Options account split
  "account.noneTitle": "아직 옵션 계좌가 없습니다",
  "account.noneText": "옵션은 CFD 계좌와 분리된 전용 계좌에서 거래합니다. 실계좌든 데모든 1분이면 개설됩니다.",
  "account.open": "옵션 계좌 개설",
};
export default options;
