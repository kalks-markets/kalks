import type { NsMessages } from "../../core";

// Kalks FX Options in the Client Area: the Options page, a friendly three-card intro and the button that opens Kalks Trader in options mode.
// Kalks, Kalks Trader and Kalks FX Options stay in English.
const options: NsMessages<"options"> = {
  // Navigation entry
  "nav.title": "آپشنز",

  // Page header
  "page.title": "Kalks FX Options",
  "page.subtitle": "فاریکس، گولڈ، سلور اور آئل پر آپشنز خریدیں یا فروخت کریں، براہِ راست Kalks Trader میں۔",
  "page.statusReady": "ٹریڈ کے لیے تیار",
  "page.learnCourse": "آپشنز کورس",

  // Hero card
  "hero.eyebrow": "Kalks Trader میں نیا",
  "hero.title": "13 مارکیٹس پر آپشنز، آسان انداز میں",
  "hero.text": "فاریکس میجرز اور کراسز، گولڈ، سلور اور خام تیل پر یورپی آپشنز۔ روزانہ، ہفتہ وار یا ماہانہ ایکسپائری منتخب کریں۔ ہر آپشن کا سیٹلمنٹ نقد، امریکی ڈالر میں ہوتا ہے، اس لیے آپ کو کبھی کسی چیز کی ڈیلیوری نہیں لینی پڑتی۔",
  "hero.feature.underlyings.title": "13 انڈرلائنگ اثاثے",
  "hero.feature.underlyings.text": "9 فاریکس پیئرز، گولڈ، سلور، WTI اور Brent خام تیل۔",
  "hero.feature.expiries.title": "روزانہ، ہفتہ وار، ماہانہ",
  "hero.feature.expiries.text": "اسی دن سے مہینے کے آخر تک کی ایکسپائری، کٹ 10:00 New York پر۔",
  "hero.feature.settlement.title": "USD میں نقد سیٹلمنٹ",
  "hero.feature.settlement.text": "کٹ سے پہلے کے 30 منٹ کی اوسط مڈ پرائس پر سیٹلمنٹ۔",
  "hero.feature.sides.title": "خریدیں یا فروخت کریں",
  "hero.feature.sides.text": "کالز اور پٹس، اسپریڈز، اسٹریڈلز، آئرن کونڈورز اور بیریئر آپشنز۔",
  "hero.class.forex": "فاریکس",
  "hero.class.metals": "دھاتیں",
  "hero.class.energies": "توانائی",
  "hero.start": "شروع کریں",
  "hero.howItWorks": "آپشنز کیسے کام کرتے ہیں",

  // Friendly intro before the first options trade: three illustrated cards, one checkbox and the Start button
  "intro.title": "تین آسان باتوں میں آپشنز",
  "intro.subtitle": "آپ کی پہلی آپشنز ٹریڈ سے پہلے ایک مختصر جائزہ۔",
  "intro.call.title": "کال خریدیں",
  "intro.call.text": "آپ کو لگتا ہے کہ قیمت اوپر جائے گی۔",
  "intro.put.title": "پٹ خریدیں",
  "intro.put.text": "آپ کو لگتا ہے کہ قیمت نیچے جائے گی۔",
  "intro.risk.title": "خریدنے پر آپ کا رسک محدود ہے",
  "intro.risk.text": "آپ کا زیادہ سے زیادہ نقصان وہی قیمت ہے جو آپ ادا کرتے ہیں۔ (آپشنز فروخت کرنے پر نقصان زیادہ ہو سکتا ہے۔)",
  "intro.legend.result": "ایکسپائری پر آپ کا نتیجہ",
  "intro.legend.cost": "جو قیمت آپ ادا کرتے ہیں",
  "intro.confirm": "میں نے سمجھ لیا ہے کہ آپشنز کیسے کام کرتے ہیں",
  "intro.terms": "مکمل شرائط پڑھیں",
  "intro.consent": "شروع کر کے آپ آپشنز کی شرائط قبول کرتے ہیں۔",
  "intro.start": "آپشنز ٹریڈنگ شروع کریں",
  "intro.quiz": "خود کو آزمائیں (کوئز)",
  "intro.gotIt": "ٹھیک ہے",
  "intro.toastStarted": "آپ آپشنز کے لیے بالکل تیار ہیں",
  "intro.toastFailed": "آپشنز ٹریڈنگ شروع نہیں ہو سکی۔ براہ کرم دوبارہ کوشش کریں۔",
  "intro.toastUpdated": "آپشنز کی شرائط ابھی اپ ڈیٹ ہوئی ہیں۔ ایک نظر ڈال لیں، پھر دوبارہ “شروع کریں” دبائیں۔",

  // Full terms dialog (the published text itself comes from the server, in English)
  "terms.title": "آپشنز کی شرائط",
  "terms.version": "ورژن {version} · {date} کو شائع ہوا",
  "terms.inShort": "مختصراً",
  "terms.point.buy": "آپشن خریدنے پر: آپ کا زیادہ سے زیادہ نقصان وہی ہے جو آپ ادا کرتے ہیں۔",
  "terms.point.sell": "آپشن فروخت کرنے پر ملنے والی رقم سے زیادہ نقصان ہو سکتا ہے، اور اس میں مارجن استعمال ہوتا ہے۔",
  "terms.point.prices": "قیمتیں Kalks آرڈر بک پر اور Kalks کی طرف سے طے ہوتی ہیں۔",
  "terms.point.settle": "آپشنز کا سیٹلمنٹ ایکسپائری پر نقد میں ہوتا ہے۔",
  "terms.englishNote": "نیچے دیا گیا مکمل متن انگریزی میں ہے، اور یہی قانونی طور پر پابند ورژن ہے۔",
  "terms.acceptedOn": "آپ نے ورژن {version} {date} کو قبول کیا۔",
  "terms.close": "بند کریں",
  "terms.unavailable": "آپشنز کی شرائط اس وقت دستیاب نہیں ہیں۔ براہ کرم بعد میں دوبارہ کوشش کریں۔",

  // Kalks Trader button
  "trade.ready": "آپ بالکل تیار ہیں۔ آپشنز Kalks Trader میں، آپ کے آپشنز اکاؤنٹ میں ٹریڈ ہوتے ہیں۔",
  "trade.cta": "Kalks Trader میں آپشنز ٹریڈ کریں",
  "trade.chooseAccount": "اکاؤنٹ منتخب کریں",
  "trade.noAccount": "آپشنز ٹریڈ کرنے کے لیے آپ کے پاس ایک فعال آپشنز اکاؤنٹ ہونا ضروری ہے۔",
  "trade.openAccount": "اکاؤنٹ کھولیں",
  "trade.cashOnly": "پریمیم اور مارجن آپ کے اکاؤنٹ کی اپنی نقد رقم سے آتے ہیں۔ بونس اور کریڈٹ استعمال نہیں کیے جا سکتے۔",
  "trade.live": "لائیو",
  "trade.demo": "ڈیمو",

  // Key facts card
  "facts.title": "Kalks FX Options کیسے کام کرتے ہیں",
  "facts.style": "یورپی اسٹائل: ایکسپائری پر خودکار طور پر ایکسرسائز، اس سے پہلے کبھی نہیں۔",
  "facts.premium": "پریمیم USD میں، فی کنٹریکٹ؛ خریدار پوزیشن کھولتے وقت اسے پورا ادا کرتے ہیں۔",
  "facts.contracts": "ایک کنٹریکٹ: کسی کرنسی کے 10,000 یونٹس، 1 اونس گولڈ، 50 اونس سلور یا 10 بیرل تیل۔",
  "facts.close": "ایکسپائری سے پہلے کسی بھی وقت کوٹ کی گئی قیمت پر مکمل یا جزوی طور پر بند کریں۔",
  "facts.cutoff": "کٹ سے پہلے کے آخری 15 منٹ میں کوئی نئی پوزیشن نہیں۔",
  "facts.margin": "فروخت کنندگان اسٹریس سیناریوز پر مبنی مارجن رکھتے ہیں؛ یہ ویک اینڈ سے پہلے بڑھ سکتا ہے۔",

  // Academy card
  "learn.title": "آپشنز میں نئے ہیں؟",
  "learn.text": "اکیڈمی میں مفت آپشنز کورس کریں: کالز اور پٹس، پے آف، گریکس، حکمتِ عملیاں اور فروخت کرنے کے خطرات۔",
  "learn.cta": "کورس کھولیں",

  // Read-only sessions (view-only logins, staff sessions)
  "readOnly": "آپشنز ٹریڈنگ صرف کلائنٹ ہی شروع کر سکتا ہے، اپنے اکاؤنٹ میں سائن اِن ہو کر۔",

  // Loading errors
  "error.load": "آپ کا آپشنز اسٹیٹس لوڈ نہیں ہو سکا۔",
  "error.retry": "دوبارہ کوشش کریں",

  // Demo build
  "demo.note": "ڈیمو: یہاں کچھ بھی محفوظ نہیں ہوتا۔",
  // CFD / Options account split
  "account.noneTitle": "ابھی کوئی آپشنز اکاؤنٹ نہیں",
  "account.noneText": "آپشنز اپنے الگ اکاؤنٹ میں ٹریڈ ہوتے ہیں، آپ کے CFD اکاؤنٹس سے الگ۔ ایک منٹ میں لائیو یا ڈیمو اکاؤنٹ کھولیں۔",
  "account.open": "آپشنز اکاؤنٹ کھولیں",
};
export default options;
