import type { NsMessages } from "../../core";

// Kalks FX Options in the Client Area: the Options page, a friendly three-card intro and the button that opens Kalks Trader in options mode.
const options: NsMessages<"options"> = {
  // Navigation entry
  "nav.title": "Опционы",

  // Page header
  "page.title": "Kalks FX Options",
  "page.subtitle": "Покупайте и продавайте опционы на валюты, золото, серебро и нефть прямо в Kalks Trader.",
  "page.statusReady": "Можно торговать",
  "page.learnCourse": "Курс по опционам",

  // Hero card
  "hero.eyebrow": "Новое в Kalks Trader",
  "hero.title": "Опционы на 13 рынках — просто и понятно",
  "hero.text": "Европейские опционы на основные валютные пары и кросс-курсы, золото, серебро и сырую нефть. Выбирайте дневные, недельные или месячные экспирации. Все опционы рассчитываются деньгами в долларах США, поэтому Вы никогда не получаете физическую поставку актива.",
  "hero.feature.underlyings.title": "13 базовых активов",
  "hero.feature.underlyings.text": "9 валютных пар, золото, серебро, нефть WTI и Brent.",
  "hero.feature.expiries.title": "Дневные, недельные, месячные",
  "hero.feature.expiries.text": "Экспирации от текущего дня до конца месяца, время экспирации — 10:00 по нью-йоркскому времени.",
  "hero.feature.settlement.title": "Денежные расчёты в USD",
  "hero.feature.settlement.text": "Расчёт по средней mid-цене за 30 минут до времени экспирации.",
  "hero.feature.sides.title": "Покупка или продажа",
  "hero.feature.sides.text": "Коллы и путы, спреды, стрэддлы, «железные кондоры» и барьерные опционы.",
  "hero.class.forex": "Форекс",
  "hero.class.metals": "Металлы",
  "hero.class.energies": "Энергоносители",
  "hero.start": "Начать",
  "hero.howItWorks": "Как работают опционы",

  // Friendly intro before the first options trade: three illustrated cards, one checkbox and the Start button
  "intro.title": "Опционы: три простые идеи",
  "intro.subtitle": "Короткое знакомство перед Вашей первой сделкой с опционами.",
  "intro.call.title": "Купить колл",
  "intro.call.text": "Вы считаете, что цена вырастет.",
  "intro.put.title": "Купить пут",
  "intro.put.text": "Вы считаете, что цена упадёт.",
  "intro.risk.title": "При покупке Ваш риск ограничен",
  "intro.risk.text": "Максимум, что Вы можете потерять, — цена, которую Вы платите. (При продаже опционов можно потерять больше.)",
  "intro.legend.result": "Ваш результат при экспирации",
  "intro.legend.cost": "Цена, которую Вы платите",
  "intro.confirm": "Я понимаю, как работают опционы",
  "intro.terms": "Прочитать полные условия",
  "intro.consent": "Начиная торговлю, Вы принимаете условия торговли опционами.",
  "intro.start": "Начать торговать опционами",
  "intro.quiz": "Проверьте себя (тест)",
  "intro.gotIt": "Понятно",
  "intro.toastStarted": "Всё готово к торговле опционами",
  "intro.toastFailed": "Не удалось начать торговлю опционами. Пожалуйста, повторите попытку.",
  "intro.toastUpdated": "Условия торговли опционами только что обновились. Просмотрите их и снова нажмите «Начать».",

  // Full terms dialog (the published text itself comes from the server, in English)
  "terms.title": "Условия торговли опционами",
  "terms.version": "Версия {version} · опубликована {date}",
  "terms.inShort": "Вкратце",
  "terms.point.buy": "Покупка опциона: максимум, что Вы можете потерять, — то, что Вы платите.",
  "terms.point.sell": "Продавая опцион, можно потерять больше полученного, и для этого нужна маржа.",
  "terms.point.prices": "Цены формируются в книге заявок Kalks, а также котируются напрямую Kalks.",
  "terms.point.settle": "Опционы рассчитываются деньгами при экспирации.",
  "terms.englishNote": "Полный текст ниже на английском языке — это юридически обязывающая версия.",
  "terms.acceptedOn": "Вы приняли версию {version} {date}.",
  "terms.close": "Закрыть",
  "terms.unavailable": "Условия торговли опционами сейчас недоступны. Пожалуйста, повторите попытку позже.",

  // Kalks Trader button
  "trade.ready": "Всё готово. Опционы торгуются в Kalks Trader, на Вашем опционном счёте.",
  "trade.cta": "Торговать опционами в Kalks Trader",
  "trade.chooseAccount": "Выберите счёт",
  "trade.noAccount": "Для торговли опционами нужен активный опционный счёт.",
  "trade.openAccount": "Открыть счёт",
  "trade.cashOnly": "Премии и маржа оплачиваются только из собственных денежных средств счёта. Бонус и кредит использовать нельзя.",
  "trade.live": "Реальный",
  "trade.demo": "Демо",

  // Key facts card
  "facts.title": "Как работают Kalks FX Options",
  "facts.style": "Европейский тип: автоматическое исполнение при экспирации, не раньше.",
  "facts.premium": "Премия в USD за контракт; покупатель уплачивает её полностью при открытии.",
  "facts.contracts": "Один контракт: 10,000 единиц валюты, 1 унция золота, 50 унций серебра или 10 баррелей нефти.",
  "facts.close": "Закрывайте позицию полностью или частично в любой момент до экспирации по котируемой цене.",
  "facts.cutoff": "За 15 минут до времени экспирации новые позиции не открываются.",
  "facts.margin": "Продавцы держат маржу, рассчитанную по стресс-сценариям; перед выходными она может вырасти.",

  // Academy card
  "learn.title": "Новичок в опционах?",
  "learn.text": "Пройдите бесплатный курс по опционам в Академии: коллы и путы, профили выплат, греки, стратегии и риски продажи опционов.",
  "learn.cta": "Открыть курс",

  // Read-only sessions (view-only logins, staff sessions)
  "readOnly": "Начать торговлю опционами может только сам клиент, войдя в свой личный кабинет.",

  // Loading errors
  "error.load": "Не удалось загрузить статус опционов.",
  "error.retry": "Повторить",

  // Demo build
  "demo.note": "Демо: здесь ничего не сохраняется.",
  // CFD / Options account split
  "account.noneTitle": "Опционного счёта пока нет",
  "account.noneText": "Опционы торгуются на отдельном счёте, не на счетах CFD. Откройте его за минуту, реальный или демо.",
  "account.open": "Открыть опционный счёт",
};
export default options;
