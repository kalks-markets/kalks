import type { NsMessages } from "../../core";

// Kalks FX Options in the Client Area: the Options page, a friendly three-card intro and the button that opens Kalks Trader in options mode.
const options: NsMessages<"options"> = {
  // Navigation entry
  "nav.title": "期权",

  // Page header
  "page.title": "Kalks FX Options",
  "page.subtitle": "直接在 Kalks Trader 中买入或卖出外汇、黄金、白银和原油期权。",
  "page.statusReady": "可以交易",
  "page.learnCourse": "期权课程",

  // Hero card
  "hero.eyebrow": "Kalks Trader 新功能",
  "hero.title": "13 个市场的期权，简单易上手",
  "hero.text": "提供外汇主要货币对和交叉货币对、黄金、白银及原油的欧式期权。可选择每日、每周或每月到期。所有期权均以美元现金结算，您无需进行任何实物交割。",
  "hero.feature.underlyings.title": "13 种标的",
  "hero.feature.underlyings.text": "9 个外汇货币对、黄金、白银、WTI 原油和布伦特原油。",
  "hero.feature.expiries.title": "每日、每周、每月到期",
  "hero.feature.expiries.text": "到期日从当日至月末不等，截止时间为纽约时间 10:00。",
  "hero.feature.settlement.title": "以美元现金结算",
  "hero.feature.settlement.text": "按截止时间前 30 分钟的平均中间价结算。",
  "hero.feature.sides.title": "可买可卖",
  "hero.feature.sides.text": "看涨期权和看跌期权、价差组合、跨式组合、铁鹰式组合及障碍期权。",
  "hero.class.forex": "外汇",
  "hero.class.metals": "贵金属",
  "hero.class.energies": "能源",
  "hero.start": "立即开始",
  "hero.howItWorks": "期权如何运作",

  // Friendly intro before the first options trade: three illustrated cards, one checkbox and the Start button
  "intro.title": "三个简单要点，读懂期权",
  "intro.subtitle": "在您首次交易期权前，先快速了解一下。",
  "intro.call.title": "买入看涨期权",
  "intro.call.text": "您认为价格会上涨。",
  "intro.put.title": "买入看跌期权",
  "intro.put.text": "您认为价格会下跌。",
  "intro.risk.title": "买入时，您的风险有限",
  "intro.risk.text": "您的最大亏损就是您支付的价格。（卖出期权可能亏损更多。）",
  "intro.legend.result": "到期时您的盈亏",
  "intro.legend.cost": "您支付的价格",
  "intro.confirm": "我了解期权的运作方式",
  "intro.terms": "阅读完整条款",
  "intro.consent": "开始即表示您接受期权条款。",
  "intro.start": "开始交易期权",
  "intro.quiz": "考考自己（小测验）",
  "intro.gotIt": "知道了",
  "intro.toastStarted": "您已准备好交易期权",
  "intro.toastFailed": "无法开始期权交易。请重试。",
  "intro.toastUpdated": "期权条款刚刚更新。请快速浏览一下，然后再次点击“开始”。",

  // Full terms dialog (the published text itself comes from the server, in English)
  "terms.title": "期权条款",
  "terms.version": "第 {version} 版 · 发布于 {date}",
  "terms.inShort": "简而言之",
  "terms.point.buy": "买入期权：您的最大亏损就是您支付的金额。",
  "terms.point.sell": "卖出期权的亏损可能超过您收取的金额，并且需要占用保证金。",
  "terms.point.prices": "价格来自 Kalks 订单簿，也来自 Kalks 的报价。",
  "terms.point.settle": "期权在到期时以现金结算。",
  "terms.englishNote": "下方完整文本为具有约束力的英文版本。",
  "terms.acceptedOn": "您已于 {date} 接受第 {version} 版。",
  "terms.close": "关闭",
  "terms.unavailable": "期权条款暂时无法显示。请稍后再试。",

  // Kalks Trader button
  "trade.ready": "一切就绪。期权在 Kalks Trader 中、在您的期权账户里交易。",
  "trade.cta": "在 Kalks Trader 中交易期权",
  "trade.chooseAccount": "选择账户",
  "trade.noAccount": "您需要一个有效的期权账户才能交易期权。",
  "trade.openAccount": "开立账户",
  "trade.cashOnly": "权利金和保证金仅使用账户中的自有资金。赠金和信用额度不能使用。",
  "trade.live": "真实",
  "trade.demo": "模拟",

  // Key facts card
  "facts.title": "Kalks FX Options 如何运作",
  "facts.style": "欧式期权：到期时自动行权，不会提前行权。",
  "facts.premium": "权利金按每份合约以美元计价；买方在开仓时全额支付。",
  "facts.contracts": "一份合约：10,000 单位货币、1 盎司黄金、50 盎司白银或 10 桶原油。",
  "facts.close": "到期前可随时按报价全部或部分平仓。",
  "facts.cutoff": "截止时间前最后 15 分钟内不能开立新仓位。",
  "facts.margin": "卖方需根据压力情景持有保证金；周末前所需保证金可能会提高。",

  // Academy card
  "learn.title": "刚开始接触期权？",
  "learn.text": "在学院中学习免费的期权课程：看涨和看跌期权、损益结构、希腊字母、交易策略以及卖出期权的风险。",
  "learn.cta": "打开课程",

  // Read-only sessions (view-only logins, staff sessions)
  "readOnly": "只有客户本人登录自己的账户后，才能开始交易期权。",

  // Loading errors
  "error.load": "无法加载您的期权开通状态。",
  "error.retry": "重试",

  // Demo build
  "demo.note": "演示版：此处的任何内容都不会被保存。",
  // CFD / Options account split
  "account.noneTitle": "还没有期权账户",
  "account.noneText": "期权在独立账户中交易，与您的 CFD 账户分开。一分钟即可开立，真实或模拟均可。",
  "account.open": "开立期权账户",
};
export default options;
