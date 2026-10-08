import type { NsMessages } from "../../core";

// Kalks FX Options in the Client Area: the Options page, a friendly three-card intro and the button that opens Kalks Trader in options mode.
const options: NsMessages<"options"> = {
  // Navigation entry
  "nav.title": "オプション",

  // Page header
  "page.title": "Kalks FX Options",
  "page.subtitle": "FX、金、銀、原油のオプションを、Kalks Trader内で直接売買できます。",
  "page.statusReady": "取引可能",
  "page.learnCourse": "オプションコース",

  // Hero card
  "hero.eyebrow": "Kalks Traderの新機能",
  "hero.title": "13市場のオプションを、シンプルに",
  "hero.text": "FXのメジャー通貨ペアとクロス通貨ペア、金、銀、原油を対象としたヨーロピアンオプションです。満期は日次・週次・月次から選べます。すべてのオプションは米ドルで現金決済されるため、現物の受け渡しは一切ありません。",
  "hero.feature.underlyings.title": "13の原資産",
  "hero.feature.underlyings.text": "FX 9通貨ペア、金、銀、WTI原油、ブレント原油。",
  "hero.feature.expiries.title": "日次・週次・月次",
  "hero.feature.expiries.text": "満期は当日から月末まで。カットオフはニューヨーク時間10:00です。",
  "hero.feature.settlement.title": "米ドルで現金決済",
  "hero.feature.settlement.text": "カットオフ前30分間の平均仲値で決済されます。",
  "hero.feature.sides.title": "買いも売りも可能",
  "hero.feature.sides.text": "コールとプット、スプレッド、ストラドル、アイアン・コンドル、バリアオプション。",
  "hero.class.forex": "FX",
  "hero.class.metals": "貴金属",
  "hero.class.energies": "エネルギー",
  "hero.start": "今すぐ始める",
  "hero.howItWorks": "オプションの仕組み",

  // Friendly intro before the first options trade: three illustrated cards, one checkbox and the Start button
  "intro.title": "3つのポイントでわかるオプション",
  "intro.subtitle": "初めてのオプション取引の前に、さっと確認しましょう。",
  "intro.call.title": "コールを買う",
  "intro.call.text": "価格が上がると予想するとき。",
  "intro.put.title": "プットを買う",
  "intro.put.text": "価格が下がると予想するとき。",
  "intro.risk.title": "買いなら、リスクは限定的です",
  "intro.risk.text": "最大損失は、支払った価格までです。（オプションを売る場合は、それ以上の損失が出ることがあります。）",
  "intro.legend.result": "満期時の損益",
  "intro.legend.cost": "支払う価格",
  "intro.confirm": "オプションの仕組みを理解しました",
  "intro.terms": "規約の全文を読む",
  "intro.consent": "開始すると、オプション取引規約に同意したことになります。",
  "intro.start": "オプション取引を始める",
  "intro.quiz": "理解度をチェック（クイズ）",
  "intro.gotIt": "わかりました",
  "intro.toastStarted": "オプション取引の準備が整いました",
  "intro.toastFailed": "オプション取引を開始できませんでした。もう一度お試しください。",
  "intro.toastUpdated": "オプション取引規約が更新されました。さっと目を通してから、もう一度「始める」を押してください。",

  // Full terms dialog (the published text itself comes from the server, in English)
  "terms.title": "オプション取引規約",
  "terms.version": "バージョン{version} · {date}公開",
  "terms.inShort": "要点",
  "terms.point.buy": "オプションを買う場合：最大損失は支払った金額までです。",
  "terms.point.sell": "オプションを売る場合は、受け取った金額を超える損失が出ることがあり、証拠金も使用します。",
  "terms.point.prices": "価格は、Kalksのオーダーブックと、Kalksによる提示で決まります。",
  "terms.point.settle": "オプションは満期時に現金で決済されます。",
  "terms.englishNote": "以下の全文は英語版で、これが法的拘束力を持つ正式な版です。",
  "terms.acceptedOn": "{date}にバージョン{version}に同意しました。",
  "terms.close": "閉じる",
  "terms.unavailable": "現在、オプション取引規約を表示できません。しばらくしてから再度お試しください。",

  // Kalks Trader button
  "trade.ready": "準備が整いました。オプションはKalks Traderで、オプション口座から取引できます。",
  "trade.cta": "Kalks Traderでオプションを取引",
  "trade.chooseAccount": "口座を選択",
  "trade.noAccount": "オプションを取引するには、有効なオプション口座が必要です。",
  "trade.openAccount": "口座を開設",
  "trade.cashOnly": "プレミアムと証拠金には口座の自己資金が使われます。ボーナスとクレジットは使用できません。",
  "trade.live": "リアル",
  "trade.demo": "デモ",

  // Key facts card
  "facts.title": "Kalks FX Optionsの仕組み",
  "facts.style": "ヨーロピアンタイプ：満期時に自動的に権利行使され、満期前に行使されることはありません。",
  "facts.premium": "プレミアムは1枚あたり米ドル建てで、買い手は新規注文時に全額を支払います。",
  "facts.contracts": "1枚の取引単位：通貨10,000単位、金1オンス、銀50オンス、または原油10バレル。",
  "facts.close": "満期前であればいつでも、提示価格で全部または一部を決済できます。",
  "facts.cutoff": "カットオフ前の最後の15分間は、新規ポジションを建てられません。",
  "facts.margin": "売り手はストレスシナリオに基づく証拠金が必要です。証拠金は週末前に引き上げられることがあります。",

  // Academy card
  "learn.title": "オプションは初めてですか？",
  "learn.text": "アカデミーの無料オプションコースで、コールとプット、ペイオフ（損益）、グリークス、戦略、そして売りのリスクを学びましょう。",
  "learn.cta": "コースを開く",

  // Read-only sessions (view-only logins, staff sessions)
  "readOnly": "オプション取引の開始は、お客様ご本人が自分のアカウントにログインしている場合のみ行えます。",

  // Loading errors
  "error.load": "オプションの利用状況を読み込めませんでした。",
  "error.retry": "再試行",

  // Demo build
  "demo.note": "デモ：ここでの操作は保存されません。",
  // CFD / Options account split
  "account.noneTitle": "オプション口座はまだありません",
  "account.noneText": "オプションはCFD口座とは別の専用口座で取引します。リアルでもデモでも1分で開設できます。",
  "account.open": "オプション口座を開設",
};
export default options;
