import type { NsMessages } from "../../core";

// Kalks FX Options in the Client Area: the Options page, a friendly three-card intro and the button that opens Kalks Trader in options mode.
// Kalks, Kalks Trader and Kalks FX Options stay in English.
const options: NsMessages<"options"> = {
  // Navigation entry
  "nav.title": "ऑप्शंस",

  // Page header
  "page.title": "Kalks FX Options",
  "page.subtitle": "फ़ॉरेक्स, गोल्ड, सिल्वर और ऑयल पर ऑप्शन खरीदें या बेचें, सीधे Kalks Trader में।",
  "page.statusReady": "ट्रेड के लिए तैयार",
  "page.learnCourse": "ऑप्शंस कोर्स",

  // Hero card
  "hero.eyebrow": "Kalks Trader में नया",
  "hero.title": "13 मार्केट पर ऑप्शंस, बिल्कुल आसान",
  "hero.text": "फ़ॉरेक्स मेजर और क्रॉस, गोल्ड, सिल्वर और क्रूड ऑयल पर यूरोपियन ऑप्शन। डेली, वीकली या मंथली एक्सपायरी चुनें। हर ऑप्शन का सेटलमेंट कैश में, US डॉलर में होता है, इसलिए आपको कभी किसी चीज़ की डिलीवरी नहीं लेनी पड़ती।",
  "hero.feature.underlyings.title": "13 अंडरलाइंग",
  "hero.feature.underlyings.text": "9 फ़ॉरेक्स पेयर, गोल्ड, सिल्वर, WTI और Brent क्रूड ऑयल।",
  "hero.feature.expiries.title": "डेली, वीकली, मंथली",
  "hero.feature.expiries.text": "उसी दिन से लेकर महीने के अंत तक की एक्सपायरी, कट 10:00 New York पर।",
  "hero.feature.settlement.title": "USD में कैश सेटलमेंट",
  "hero.feature.settlement.text": "कट से पहले के 30 मिनट के औसत मिड प्राइस पर सेटलमेंट।",
  "hero.feature.sides.title": "खरीदें या बेचें",
  "hero.feature.sides.text": "कॉल और पुट, स्प्रेड, स्ट्रैडल, आयरन कोंडोर और बैरियर ऑप्शन।",
  "hero.class.forex": "फ़ॉरेक्स",
  "hero.class.metals": "मेटल्स",
  "hero.class.energies": "एनर्जी",
  "hero.start": "शुरू करें",
  "hero.howItWorks": "ऑप्शंस कैसे काम करते हैं",

  // Friendly intro before the first options trade: three illustrated cards, one checkbox and the Start button
  "intro.title": "तीन आसान बातों में ऑप्शंस",
  "intro.subtitle": "आपके पहले ऑप्शंस ट्रेड से पहले एक छोटी-सी झलक।",
  "intro.call.title": "कॉल खरीदें",
  "intro.call.text": "आपको लगता है कि कीमत ऊपर जाएगी।",
  "intro.put.title": "पुट खरीदें",
  "intro.put.text": "आपको लगता है कि कीमत नीचे जाएगी।",
  "intro.risk.title": "खरीदने पर आपका जोखिम सीमित है",
  "intro.risk.text": "आपका अधिकतम नुकसान वही कीमत है जो आप चुकाते हैं। (ऑप्शंस बेचने पर नुकसान ज़्यादा हो सकता है।)",
  "intro.legend.result": "एक्सपायरी पर आपका नतीजा",
  "intro.legend.cost": "जो कीमत आप चुकाते हैं",
  "intro.confirm": "मैंने समझ लिया है कि ऑप्शंस कैसे काम करते हैं",
  "intro.terms": "पूरी शर्तें पढ़ें",
  "intro.consent": "शुरू करके, आप ऑप्शंस की शर्तें स्वीकार करते हैं।",
  "intro.start": "ऑप्शंस ट्रेडिंग शुरू करें",
  "intro.quiz": "खुद को परखें (क्विज़)",
  "intro.gotIt": "ठीक है",
  "intro.toastStarted": "आप ऑप्शंस के लिए पूरी तरह तैयार हैं",
  "intro.toastFailed": "ऑप्शंस ट्रेडिंग शुरू नहीं हो सकी। कृपया फिर से कोशिश करें।",
  "intro.toastUpdated": "ऑप्शंस की शर्तें अभी-अभी अपडेट हुई हैं। एक नज़र डाल लें, फिर दोबारा “शुरू करें” दबाएँ।",

  // Full terms dialog (the published text itself comes from the server, in English)
  "terms.title": "ऑप्शंस की शर्तें",
  "terms.version": "वर्ज़न {version} · {date} को प्रकाशित",
  "terms.inShort": "संक्षेप में",
  "terms.point.buy": "ऑप्शन खरीदने पर: आपका अधिकतम नुकसान उतना ही है जितना आप चुकाते हैं।",
  "terms.point.sell": "ऑप्शन बेचने पर मिलने वाली रकम से ज़्यादा नुकसान हो सकता है, और इसमें मार्जिन लगता है।",
  "terms.point.prices": "प्राइस Kalks ऑर्डर बुक पर और Kalks द्वारा तय किए जाते हैं।",
  "terms.point.settle": "ऑप्शंस का सेटलमेंट एक्सपायरी पर कैश में होता है।",
  "terms.englishNote": "नीचे दिया गया पूरा टेक्स्ट अंग्रेज़ी में है, और यही बाध्यकारी वर्ज़न है।",
  "terms.acceptedOn": "आपने वर्ज़न {version} {date} को स्वीकार किया।",
  "terms.close": "बंद करें",
  "terms.unavailable": "ऑप्शंस की शर्तें अभी उपलब्ध नहीं हैं। कृपया बाद में फिर से कोशिश करें।",

  // Kalks Trader button
  "trade.ready": "आप पूरी तरह तैयार हैं। ऑप्शंस Kalks Trader में, आपके ऑप्शंस अकाउंट में ट्रेड होते हैं।",
  "trade.cta": "Kalks Trader में ऑप्शंस ट्रेड करें",
  "trade.chooseAccount": "अकाउंट चुनें",
  "trade.noAccount": "ऑप्शंस ट्रेड करने के लिए आपके पास एक सक्रिय ऑप्शंस अकाउंट होना चाहिए।",
  "trade.openAccount": "अकाउंट खोलें",
  "trade.cashOnly": "प्रीमियम और मार्जिन आपके अकाउंट के अपने कैश से आते हैं। बोनस और क्रेडिट का इस्तेमाल नहीं किया जा सकता।",
  "trade.live": "लाइव",
  "trade.demo": "डेमो",

  // Key facts card
  "facts.title": "Kalks FX Options कैसे काम करते हैं",
  "facts.style": "यूरोपियन स्टाइल: एक्सपायरी पर अपने-आप एक्सरसाइज़, उससे पहले कभी नहीं।",
  "facts.premium": "प्रीमियम USD में, प्रति कॉन्ट्रैक्ट; खरीदार पोज़िशन खोलते समय इसे पूरा चुकाते हैं।",
  "facts.contracts": "एक कॉन्ट्रैक्ट: किसी करेंसी की 10,000 यूनिट, 1 औंस गोल्ड, 50 औंस सिल्वर या 10 बैरल ऑयल।",
  "facts.close": "एक्सपायरी से पहले कभी भी, कोट किए गए प्राइस पर, पूरी या आंशिक पोज़िशन बंद करें।",
  "facts.cutoff": "कट से पहले के आख़िरी 15 मिनट में कोई नई पोज़िशन नहीं।",
  "facts.margin": "विक्रेता स्ट्रेस सिनेरियो पर आधारित मार्जिन रखते हैं; वीकेंड से पहले यह बढ़ सकता है।",

  // Academy card
  "learn.title": "ऑप्शंस में नए हैं?",
  "learn.text": "एकेडमी में मुफ़्त ऑप्शंस कोर्स करें: कॉल और पुट, पेऑफ़, ग्रीक्स, स्ट्रैटेजी और बेचने के जोखिम।",
  "learn.cta": "कोर्स खोलें",

  // Read-only sessions (view-only logins, staff sessions)
  "readOnly": "ऑप्शंस ट्रेडिंग सिर्फ़ क्लाइंट ही शुरू कर सकता है, अपने अकाउंट में साइन इन करके।",

  // Loading errors
  "error.load": "आपका ऑप्शंस स्टेटस लोड नहीं हो सका।",
  "error.retry": "फिर से कोशिश करें",

  // Demo build
  "demo.note": "डेमो: यहाँ कुछ भी सेव नहीं होता।",
  // CFD / Options account split
  "account.noneTitle": "अभी कोई ऑप्शंस अकाउंट नहीं है",
  "account.noneText": "ऑप्शंस अपने अलग अकाउंट में ट्रेड होते हैं, आपके CFD अकाउंट से अलग। एक मिनट में एक खोलें, लाइव या डेमो।",
  "account.open": "ऑप्शंस अकाउंट खोलें",
};
export default options;
