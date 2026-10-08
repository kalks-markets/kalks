import type { NsMessages } from "../../core";

// Kalks FX Options in the Client Area: the Options page, a friendly three-card intro and the button that opens Kalks Trader in options mode.
// Kalks, Kalks Trader and Kalks FX Options stay in English.
const options: NsMessages<"options"> = {
  // Navigation entry
  "nav.title": "অপশন",

  // Page header
  "page.title": "Kalks FX Options",
  "page.subtitle": "ফরেক্স, গোল্ড, সিলভার ও অয়েলে অপশন কিনুন বা বিক্রি করুন, সরাসরি Kalks Trader-এর ভেতরেই।",
  "page.statusReady": "ট্রেডের জন্য প্রস্তুত",
  "page.learnCourse": "অপশন কোর্স",

  // Hero card
  "hero.eyebrow": "Kalks Trader-এ নতুন",
  "hero.title": "13টি মার্কেটে অপশন, একদম সহজভাবে",
  "hero.text": "ফরেক্স মেজর ও ক্রস পেয়ার, গোল্ড, সিলভার এবং ক্রুড অয়েলে ইউরোপিয়ান অপশন। দৈনিক, সাপ্তাহিক বা মাসিক এক্সপায়ারি বেছে নিন। প্রতিটি অপশন নগদে, মার্কিন ডলারে সেটেল হয়, তাই আপনাকে কখনো কোনো কিছুর ডেলিভারি নিতে হয় না।",
  "hero.feature.underlyings.title": "13টি আন্ডারলাইং",
  "hero.feature.underlyings.text": "9টি ফরেক্স পেয়ার, গোল্ড, সিলভার, WTI ও Brent ক্রুড অয়েল।",
  "hero.feature.expiries.title": "দৈনিক, সাপ্তাহিক, মাসিক",
  "hero.feature.expiries.text": "একই দিন থেকে মাসের শেষ পর্যন্ত এক্সপায়ারি, কাট 10:00 New York-এ।",
  "hero.feature.settlement.title": "USD-তে নগদ সেটেলমেন্ট",
  "hero.feature.settlement.text": "কাটের আগের 30 মিনিটের গড় মিড প্রাইসে সেটেলমেন্ট।",
  "hero.feature.sides.title": "কিনুন বা বিক্রি করুন",
  "hero.feature.sides.text": "কল ও পুট, স্প্রেড, স্ট্র্যাডল, আয়রন কন্ডর এবং ব্যারিয়ার অপশন।",
  "hero.class.forex": "ফরেক্স",
  "hero.class.metals": "মেটাল",
  "hero.class.energies": "এনার্জি",
  "hero.start": "শুরু করুন",
  "hero.howItWorks": "অপশন কীভাবে কাজ করে",

  // Friendly intro before the first options trade: three illustrated cards, one checkbox and the Start button
  "intro.title": "তিনটি সহজ কথায় অপশন",
  "intro.subtitle": "আপনার প্রথম অপশন ট্রেডের আগে এক নজরে দেখে নিন।",
  "intro.call.title": "কল কিনুন",
  "intro.call.text": "আপনার মনে হয় দাম বাড়বে।",
  "intro.put.title": "পুট কিনুন",
  "intro.put.text": "আপনার মনে হয় দাম কমবে।",
  "intro.risk.title": "কিনলে আপনার ঝুঁকি সীমিত",
  "intro.risk.text": "আপনার সর্বোচ্চ ক্ষতি হলো আপনার দেওয়া দাম। (অপশন বিক্রি করলে ক্ষতি বেশি হতে পারে।)",
  "intro.legend.result": "এক্সপায়ারিতে আপনার ফলাফল",
  "intro.legend.cost": "আপনার দেওয়া দাম",
  "intro.confirm": "আমি বুঝি অপশন কীভাবে কাজ করে",
  "intro.terms": "সম্পূর্ণ শর্তাবলি পড়ুন",
  "intro.consent": "শুরু করার মাধ্যমে আপনি অপশনের শর্তাবলি গ্রহণ করছেন।",
  "intro.start": "অপশন ট্রেডিং শুরু করুন",
  "intro.quiz": "নিজেকে যাচাই করুন (কুইজ)",
  "intro.gotIt": "বুঝেছি",
  "intro.toastStarted": "অপশনের জন্য আপনি পুরোপুরি প্রস্তুত",
  "intro.toastFailed": "অপশন ট্রেডিং শুরু করা যায়নি। অনুগ্রহ করে আবার চেষ্টা করুন।",
  "intro.toastUpdated": "অপশনের শর্তাবলি এইমাত্র আপডেট হয়েছে। একবার দেখে নিন, তারপর আবার “শুরু করুন” চাপুন।",

  // Full terms dialog (the published text itself comes from the server, in English)
  "terms.title": "অপশনের শর্তাবলি",
  "terms.version": "ভার্সন {version} · প্রকাশিত {date}",
  "terms.inShort": "সংক্ষেপে",
  "terms.point.buy": "অপশন কিনলে: সর্বোচ্চ ক্ষতি হলো আপনার দেওয়া অর্থ।",
  "terms.point.sell": "অপশন বিক্রি করলে প্রাপ্ত অর্থের চেয়ে বেশি ক্ষতি হতে পারে, এবং এতে মার্জিন লাগে।",
  "terms.point.prices": "প্রাইস নির্ধারিত হয় Kalks অর্ডার বুকে এবং Kalks-এর মাধ্যমে।",
  "terms.point.settle": "এক্সপায়ারিতে অপশন নগদে সেটেল হয়।",
  "terms.englishNote": "নিচের সম্পূর্ণ টেক্সটটি ইংরেজিতে, এবং এটিই আইনত বাধ্যতামূলক ভার্সন।",
  "terms.acceptedOn": "আপনি ভার্সন {version} {date} তারিখে গ্রহণ করেছেন।",
  "terms.close": "বন্ধ করুন",
  "terms.unavailable": "অপশনের শর্তাবলি এখন উপলব্ধ নয়। অনুগ্রহ করে পরে আবার চেষ্টা করুন।",

  // Kalks Trader button
  "trade.ready": "আপনি পুরোপুরি প্রস্তুত। অপশন Kalks Trader-এ, আপনার অপশন অ্যাকাউন্টে ট্রেড হয়।",
  "trade.cta": "Kalks Trader-এ অপশন ট্রেড করুন",
  "trade.chooseAccount": "একটি অ্যাকাউন্ট বেছে নিন",
  "trade.noAccount": "অপশন ট্রেড করতে আপনার একটি সক্রিয় অপশন অ্যাকাউন্ট প্রয়োজন।",
  "trade.openAccount": "অ্যাকাউন্ট খুলুন",
  "trade.cashOnly": "প্রিমিয়াম ও মার্জিন আপনার অ্যাকাউন্টের নিজস্ব নগদ থেকে আসে। বোনাস ও ক্রেডিট ব্যবহার করা যায় না।",
  "trade.live": "লাইভ",
  "trade.demo": "ডেমো",

  // Key facts card
  "facts.title": "Kalks FX Options কীভাবে কাজ করে",
  "facts.style": "ইউরোপিয়ান স্টাইল: এক্সপায়ারিতে স্বয়ংক্রিয়ভাবে এক্সারসাইজ হয়, তার আগে কখনো নয়।",
  "facts.premium": "প্রিমিয়াম USD-তে, প্রতি কন্ট্রাক্টে; ক্রেতারা পজিশন খোলার সময় পুরোটা পরিশোধ করেন।",
  "facts.contracts": "একটি কন্ট্রাক্ট: একটি কারেন্সির 10,000 ইউনিট, 1 আউন্স গোল্ড, 50 আউন্স সিলভার বা 10 ব্যারেল অয়েল।",
  "facts.close": "এক্সপায়ারির আগে যেকোনো সময় কোট করা প্রাইসে পুরো বা আংশিক বন্ধ করুন।",
  "facts.cutoff": "কাটের আগের শেষ 15 মিনিটে কোনো নতুন পজিশন খোলা যায় না।",
  "facts.margin": "বিক্রেতারা স্ট্রেস সিনারিওর ভিত্তিতে মার্জিন রাখেন; উইকেন্ডের আগে এটি বাড়তে পারে।",

  // Academy card
  "learn.title": "অপশনে নতুন?",
  "learn.text": "অ্যাকাডেমিতে বিনামূল্যে অপশন কোর্স করুন: কল ও পুট, পেঅফ, গ্রিকস, স্ট্র্যাটেজি এবং বিক্রির ঝুঁকি।",
  "learn.cta": "কোর্স খুলুন",

  // Read-only sessions (view-only logins, staff sessions)
  "readOnly": "শুধু ক্লায়েন্ট নিজেই, নিজের অ্যাকাউন্টে সাইন ইন করে, অপশন ট্রেডিং শুরু করতে পারেন।",

  // Loading errors
  "error.load": "আপনার অপশন স্ট্যাটাস লোড করা যায়নি।",
  "error.retry": "আবার চেষ্টা করুন",

  // Demo build
  "demo.note": "ডেমো: এখানে কিছুই সেভ হয় না।",
  // CFD / Options account split
  "account.noneTitle": "এখনও কোনো অপশন অ্যাকাউন্ট নেই",
  "account.noneText": "অপশন নিজস্ব অ্যাকাউন্টে ট্রেড হয়, আপনার CFD অ্যাকাউন্ট থেকে আলাদা। এক মিনিটে একটি খুলুন, লাইভ বা ডেমো।",
  "account.open": "অপশন অ্যাকাউন্ট খুলুন",
};
export default options;
