import type { NsMessages } from "../../core";

// Kalks FX Options in the Client Area: the Options page, a friendly three-card intro and the button that opens Kalks Trader in options mode.
// Kalks, Kalks Trader and Kalks FX Options stay in English.
const options: NsMessages<"options"> = {
  // Navigation entry
  "nav.title": "اختیار معامله",

  // Page header
  "page.title": "Kalks FX Options",
  "page.subtitle": "اختیار معامله روی فارکس، طلا، نقره و نفت را مستقیماً داخل Kalks Trader بخرید یا بفروشید.",
  "page.statusReady": "آماده معامله",
  "page.learnCourse": "دوره اختیار معامله",

  // Hero card
  "hero.eyebrow": "جدید در Kalks Trader",
  "hero.title": "اختیار معامله روی 13 بازار، به ساده‌ترین شکل",
  "hero.text": "اختیار معامله اروپایی روی جفت‌ارزهای اصلی و فرعی فارکس، طلا، نقره و نفت خام. سررسید روزانه، هفتگی یا ماهانه را انتخاب کنید. همه اختیارها به‌صورت نقدی و به دلار آمریکا تسویه می‌شوند، بنابراین هرگز چیزی را به‌صورت فیزیکی تحویل نمی‌گیرید.",
  "hero.feature.underlyings.title": "13 دارایی پایه",
  "hero.feature.underlyings.text": "9 جفت‌ارز فارکس، طلا، نقره، و نفت خام WTI و برنت.",
  "hero.feature.expiries.title": "روزانه، هفتگی، ماهانه",
  "hero.feature.expiries.text": "سررسیدهایی از همان روز تا پایان ماه، با زمان کات 10:00 New York.",
  "hero.feature.settlement.title": "تسویه نقدی به USD",
  "hero.feature.settlement.text": "تسویه بر اساس میانگین قیمت میانی در 30 دقیقه پیش از زمان کات.",
  "hero.feature.sides.title": "خرید یا فروش",
  "hero.feature.sides.text": "اختیار خرید و اختیار فروش (Call و Put)، اسپرد، استرادل، آیرون کاندور و اختیارهای مانع‌دار.",
  "hero.class.forex": "فارکس",
  "hero.class.metals": "فلزات",
  "hero.class.energies": "انرژی",
  "hero.start": "شروع کنید",
  "hero.howItWorks": "اختیار معامله چگونه کار می‌کند",

  // Friendly intro before the first options trade: three illustrated cards, one checkbox and the Start button
  "intro.title": "اختیار معامله در سه نکته ساده",
  "intro.subtitle": "نگاهی کوتاه پیش از اولین معامله اختیار شما.",
  "intro.call.title": "خرید اختیار خرید",
  "intro.call.text": "فکر می‌کنید قیمت بالا می‌رود.",
  "intro.put.title": "خرید اختیار فروش",
  "intro.put.text": "فکر می‌کنید قیمت پایین می‌آید.",
  "intro.risk.title": "وقتی می‌خرید، ریسک شما محدود است",
  "intro.risk.text": "حداکثر زیان شما همان قیمتی است که می‌پردازید. (فروش اختیار ممکن است زیان بیشتری داشته باشد.)",
  "intro.legend.result": "نتیجه شما در سررسید",
  "intro.legend.cost": "قیمتی که می‌پردازید",
  "intro.confirm": "می‌دانم اختیار معامله چگونه کار می‌کند",
  "intro.terms": "مطالعه شرایط کامل",
  "intro.consent": "با شروع، شرایط اختیار معامله را می‌پذیرید.",
  "intro.start": "شروع معامله اختیار",
  "intro.quiz": "خودتان را بسنجید (آزمون)",
  "intro.gotIt": "متوجه شدم",
  "intro.toastStarted": "همه‌چیز برای معامله اختیار آماده است",
  "intro.toastFailed": "شروع معامله اختیار انجام نشد. لطفاً دوباره تلاش کنید.",
  "intro.toastUpdated": "شرایط اختیار معامله همین الان به‌روزرسانی شد. نگاهی کوتاه بیندازید، سپس دوباره «شروع» را بزنید.",

  // Full terms dialog (the published text itself comes from the server, in English)
  "terms.title": "شرایط اختیار معامله",
  "terms.version": "نسخه {version} · منتشرشده در {date}",
  "terms.inShort": "به‌طور خلاصه",
  "terms.point.buy": "خرید اختیار: حداکثر زیان شما همان مبلغی است که می‌پردازید.",
  "terms.point.sell": "فروش اختیار ممکن است بیش از مبلغ دریافتی زیان داشته باشد و به مارجین نیاز دارد.",
  "terms.point.prices": "قیمت‌ها در دفتر سفارش Kalks و توسط Kalks تعیین می‌شوند.",
  "terms.point.settle": "اختیارها در سررسید به‌صورت نقدی تسویه می‌شوند.",
  "terms.englishNote": "متن کامل زیر، به زبان انگلیسی، نسخه الزام‌آور است.",
  "terms.acceptedOn": "شما نسخه {version} را در تاریخ {date} پذیرفتید.",
  "terms.close": "بستن",
  "terms.unavailable": "شرایط اختیار معامله در حال حاضر در دسترس نیست. لطفاً بعداً دوباره تلاش کنید.",

  // Kalks Trader button
  "trade.ready": "همه‌چیز آماده است. اختیار معامله در Kalks Trader و در حساب اختیار معامله شما انجام می‌شود.",
  "trade.cta": "معامله اختیار در Kalks Trader",
  "trade.chooseAccount": "یک حساب انتخاب کنید",
  "trade.noAccount": "برای معامله اختیار به یک حساب اختیار معامله فعال نیاز دارید.",
  "trade.openAccount": "افتتاح حساب",
  "trade.cashOnly": "پرمیوم‌ها و مارجین از موجودی نقدی خود حساب شما تأمین می‌شوند. بونوس و اعتبار قابل استفاده نیستند.",
  "trade.live": "واقعی",
  "trade.demo": "دمو",

  // Key facts card
  "facts.title": "Kalks FX Options چگونه کار می‌کنند",
  "facts.style": "سبک اروپایی: در سررسید به‌طور خودکار اعمال می‌شوند، هرگز پیش از آن.",
  "facts.premium": "پرمیوم به USD و برای هر قرارداد؛ خریداران هنگام باز کردن پوزیشن آن را کامل می‌پردازند.",
  "facts.contracts": "هر قرارداد: 10,000 واحد از یک ارز، 1 اونس طلا، 50 اونس نقره یا 10 بشکه نفت.",
  "facts.close": "در هر زمان پیش از سررسید، با قیمت اعلام‌شده، به‌طور کامل یا جزئی ببندید.",
  "facts.cutoff": "در 15 دقیقه پایانی پیش از زمان کات، امکان باز کردن پوزیشن جدید وجود ندارد.",
  "facts.margin": "فروشندگان مارجینی بر اساس سناریوهای تنش نگه می‌دارند؛ این مارجین ممکن است پیش از آخر هفته افزایش یابد.",

  // Academy card
  "learn.title": "با اختیار معامله تازه آشنا شده‌اید؟",
  "learn.text": "دوره رایگان اختیار معامله را در آکادمی بگذرانید: اختیار خرید و فروش، نمودار سود و زیان، یونانی‌ها، استراتژی‌ها و ریسک‌های فروش.",
  "learn.cta": "باز کردن دوره",

  // Read-only sessions (view-only logins, staff sessions)
  "readOnly": "فقط خود مشتری، پس از ورود به حساب خودش، می‌تواند معامله اختیار را شروع کند.",

  // Loading errors
  "error.load": "بارگذاری وضعیت اختیار معامله شما انجام نشد.",
  "error.retry": "تلاش مجدد",

  // Demo build
  "demo.note": "دمو: هیچ چیزی در اینجا ذخیره نمی‌شود.",
  // CFD / Options account split
  "account.noneTitle": "هنوز حساب اختیار معامله ندارید",
  "account.noneText": "اختیار معامله در حساب جداگانه‌ای جدا از حساب‌های CFD شما معامله می‌شود. در یک دقیقه یکی باز کنید، واقعی یا دمو.",
  "account.open": "افتتاح حساب اختیار معامله",
};
export default options;
