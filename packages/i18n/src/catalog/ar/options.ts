import type { NsMessages } from "../../core";

// Kalks FX Options in the Client Area: the Options page, a friendly three-card intro and the button that opens Kalks Trader in options mode.
// Kalks, Kalks Trader and Kalks FX Options stay in English.
const options: NsMessages<"options"> = {
  // Navigation entry
  "nav.title": "الخيارات",

  // Page header
  "page.title": "Kalks FX Options",
  "page.subtitle": "اشترِ أو بِع الخيارات على الفوركس والذهب والفضة والنفط، مباشرةً داخل Kalks Trader.",
  "page.statusReady": "جاهز للتداول",
  "page.learnCourse": "دورة الخيارات",

  // Hero card
  "hero.eyebrow": "جديد في Kalks Trader",
  "hero.title": "خيارات على 13 سوقًا، بكل بساطة",
  "hero.text": "خيارات أوروبية على أزواج الفوركس الرئيسية والتقاطعية والذهب والفضة والنفط الخام. اختر تواريخ انتهاء يومية أو أسبوعية أو شهرية. تُسوّى جميع الخيارات نقدًا بالدولار الأمريكي، فلا تستلم أي أصل فعليًا على الإطلاق.",
  "hero.feature.underlyings.title": "13 أصلًا أساسيًا",
  "hero.feature.underlyings.text": "9 أزواج فوركس، والذهب، والفضة، وخام WTI وخام برنت.",
  "hero.feature.expiries.title": "يومية، أسبوعية، شهرية",
  "hero.feature.expiries.text": "تواريخ انتهاء من اليوم نفسه حتى نهاية الشهر، ووقت القطع 10:00 New York.",
  "hero.feature.settlement.title": "تسوية نقدية بـ USD",
  "hero.feature.settlement.text": "تتم التسوية بمتوسط السعر الوسطي خلال الـ 30 دقيقة السابقة لوقت القطع.",
  "hero.feature.sides.title": "شراء أو بيع",
  "hero.feature.sides.text": "خيارات الشراء والبيع (Calls وPuts)، والسبريد، والسترادل، والآيرون كوندور، وخيارات الحاجز.",
  "hero.class.forex": "فوركس",
  "hero.class.metals": "المعادن",
  "hero.class.energies": "الطاقة",
  "hero.start": "ابدأ الآن",
  "hero.howItWorks": "كيف تعمل الخيارات",

  // Friendly intro before the first options trade: three illustrated cards, one checkbox and the Start button
  "intro.title": "الخيارات في ثلاث أفكار بسيطة",
  "intro.subtitle": "نظرة سريعة قبل أول صفقة خيارات لك.",
  "intro.call.title": "اشترِ خيار شراء",
  "intro.call.text": "تتوقع أن يرتفع السعر.",
  "intro.put.title": "اشترِ خيار بيع",
  "intro.put.text": "تتوقع أن ينخفض السعر.",
  "intro.risk.title": "مخاطرتك محدودة عند الشراء",
  "intro.risk.text": "أقصى ما يمكن أن تخسره هو السعر الذي تدفعه. (عند بيع الخيارات قد تخسر أكثر.)",
  "intro.legend.result": "نتيجتك عند الانتهاء",
  "intro.legend.cost": "السعر الذي تدفعه",
  "intro.confirm": "أفهم كيف تعمل الخيارات",
  "intro.terms": "اقرأ الشروط كاملة",
  "intro.consent": "بالبدء، فإنك توافق على شروط الخيارات.",
  "intro.start": "ابدأ تداول الخيارات",
  "intro.quiz": "اختبر نفسك (اختبار قصير)",
  "intro.gotIt": "فهمت",
  "intro.toastStarted": "أنت جاهز لتداول الخيارات",
  "intro.toastFailed": "تعذّر بدء تداول الخيارات. يُرجى المحاولة مرة أخرى.",
  "intro.toastUpdated": "تم تحديث شروط الخيارات للتو. ألقِ نظرة سريعة، ثم اضغط «ابدأ» مرة أخرى.",

  // Full terms dialog (the published text itself comes from the server, in English)
  "terms.title": "شروط الخيارات",
  "terms.version": "الإصدار {version} · نُشر في {date}",
  "terms.inShort": "باختصار",
  "terms.point.buy": "عند شراء خيار: أقصى ما يمكن أن تخسره هو ما تدفعه.",
  "terms.point.sell": "عند بيع خيار، قد تخسر أكثر مما تحصل عليه، ويتطلب ذلك هامشًا.",
  "terms.point.prices": "تُحدَّد الأسعار في دفتر أوامر Kalks ومن قِبل Kalks.",
  "terms.point.settle": "تُسوّى الخيارات نقدًا عند الانتهاء.",
  "terms.englishNote": "النص الكامل أدناه هو النسخة الملزمة، وهو باللغة الإنجليزية.",
  "terms.acceptedOn": "وافقت على الإصدار {version} في {date}.",
  "terms.close": "إغلاق",
  "terms.unavailable": "شروط الخيارات غير متاحة حاليًا. يُرجى المحاولة لاحقًا.",

  // Kalks Trader button
  "trade.ready": "كل شيء جاهز. تُتداول الخيارات في Kalks Trader، في حساب الخيارات الخاص بك.",
  "trade.cta": "تداول الخيارات في Kalks Trader",
  "trade.chooseAccount": "اختر حسابًا",
  "trade.noAccount": "تحتاج إلى حساب خيارات نشط لتداول الخيارات.",
  "trade.openAccount": "فتح حساب",
  "trade.cashOnly": "تُؤخذ العلاوات والهامش من الرصيد النقدي لحسابك نفسه. لا يمكن استخدام المكافآت أو الائتمان.",
  "trade.live": "حقيقي",
  "trade.demo": "تجريبي",

  // Key facts card
  "facts.title": "كيف تعمل Kalks FX Options",
  "facts.style": "النمط الأوروبي: تُنفَّذ تلقائيًا عند الانتهاء، ولا تُنفَّذ قبله أبدًا.",
  "facts.premium": "العلاوة بـ USD لكل عقد؛ يدفعها المشترون كاملةً عند الفتح.",
  "facts.contracts": "العقد الواحد: 10,000 وحدة من العملة، أو 1 أونصة ذهب، أو 50 أونصة فضة، أو 10 براميل نفط.",
  "facts.close": "أغلق في أي وقت قبل الانتهاء بالسعر المعروض، كليًا أو جزئيًا.",
  "facts.cutoff": "لا يمكن فتح صفقات جديدة خلال آخر 15 دقيقة قبل وقت القطع.",
  "facts.margin": "يحتفظ البائعون بهامش محسوب على أساس سيناريوهات الضغط؛ وقد يرتفع قبل عطلات نهاية الأسبوع.",

  // Academy card
  "learn.title": "جديد في عالم الخيارات؟",
  "learn.text": "احضر دورة الخيارات المجانية في الأكاديمية: خيارات الشراء والبيع، والعوائد، والإغريقيات (Greeks)، والاستراتيجيات، ومخاطر البيع.",
  "learn.cta": "افتح الدورة",

  // Read-only sessions (view-only logins, staff sessions)
  "readOnly": "لا يمكن بدء تداول الخيارات إلا من قِبل العميل نفسه، بعد تسجيل الدخول إلى حسابه.",

  // Loading errors
  "error.load": "تعذّر تحميل حالة الخيارات الخاصة بك.",
  "error.retry": "إعادة المحاولة",

  // Demo build
  "demo.note": "نسخة تجريبية: لا يُحفظ أي شيء هنا.",
  // CFD / Options account split
  "account.noneTitle": "لا يوجد حساب خيارات بعد",
  "account.noneText": "تُتداول الخيارات في حساب خاص بها، منفصل عن حسابات عقود الفروقات. افتح حسابًا في دقيقة، حقيقيًا أو تجريبيًا.",
  "account.open": "فتح حساب خيارات",
};
export default options;
