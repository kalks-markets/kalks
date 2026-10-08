import type { NsMessages } from "../../core";

// Kalks FX Options in the Client Area: the Options page, a friendly three-card intro and the button that opens Kalks Trader in options mode.
const options: NsMessages<"options"> = {
  // Navigation entry
  "nav.title": "Opsiyonlar",

  // Page header
  "page.title": "Kalks FX Options",
  "page.subtitle": "Döviz, altın, gümüş ve petrol opsiyonlarını doğrudan Kalks Trader içinde alın veya satın.",
  "page.statusReady": "İşleme hazır",
  "page.learnCourse": "Opsiyon kursu",

  // Hero card
  "hero.eyebrow": "Kalks Trader'da yeni",
  "hero.title": "13 piyasada opsiyonlar, kolay ve sade",
  "hero.text": "Majör ve çapraz döviz pariteleri, altın, gümüş ve ham petrol üzerine Avrupa tipi opsiyonlar. Günlük, haftalık veya aylık vadeler arasından seçim yapın. Tüm opsiyonlar ABD doları cinsinden nakdi uzlaşıyla kapanır; bu nedenle hiçbir zaman fiziki teslimat almazsınız.",
  "hero.feature.underlyings.title": "13 dayanak varlık",
  "hero.feature.underlyings.text": "9 döviz paritesi, altın, gümüş, WTI ve Brent ham petrol.",
  "hero.feature.expiries.title": "Günlük, haftalık, aylık",
  "hero.feature.expiries.text": "Aynı günden ay sonuna kadar vadeler; kesim saati New York saatiyle 10:00.",
  "hero.feature.settlement.title": "USD ile nakdi uzlaşı",
  "hero.feature.settlement.text": "Kesim saatinden önceki 30 dakikanın ortalama orta (mid) fiyatından uzlaşılır.",
  "hero.feature.sides.title": "Alın veya satın",
  "hero.feature.sides.text": "Call ve put'lar, spread, straddle, iron condor ve bariyerli opsiyonlar.",
  "hero.class.forex": "Forex",
  "hero.class.metals": "Metaller",
  "hero.class.energies": "Enerji",
  "hero.start": "Hemen başla",
  "hero.howItWorks": "Opsiyonlar nasıl işler",

  // Friendly intro before the first options trade: three illustrated cards, one checkbox and the Start button
  "intro.title": "Üç basit fikirle opsiyonlar",
  "intro.subtitle": "İlk opsiyon işleminizden önce kısa bir bakış.",
  "intro.call.title": "Call al",
  "intro.call.text": "Fiyatın yükseleceğini düşünüyorsunuz.",
  "intro.put.title": "Put al",
  "intro.put.text": "Fiyatın düşeceğini düşünüyorsunuz.",
  "intro.risk.title": "Alım yaptığınızda riskiniz sınırlıdır",
  "intro.risk.text": "Kaybedebileceğiniz en fazla tutar, ödediğiniz fiyattır. (Opsiyon satışında daha fazlası kaybedilebilir.)",
  "intro.legend.result": "Vade sonundaki sonucunuz",
  "intro.legend.cost": "Ödediğiniz fiyat",
  "intro.confirm": "Opsiyonların nasıl işlediğini anlıyorum",
  "intro.terms": "Koşulların tamamını oku",
  "intro.consent": "Başlayarak opsiyon koşullarını kabul etmiş olursunuz.",
  "intro.start": "Opsiyon işlemlerine başla",
  "intro.quiz": "Kendinizi sınayın (test)",
  "intro.gotIt": "Anladım",
  "intro.toastStarted": "Opsiyon işlemleri için her şey hazır",
  "intro.toastFailed": "Opsiyon işlemleri başlatılamadı. Lütfen tekrar deneyin.",
  "intro.toastUpdated": "Opsiyon koşulları az önce güncellendi. Kısaca göz atın, ardından tekrar Başla'ya basın.",

  // Full terms dialog (the published text itself comes from the server, in English)
  "terms.title": "Opsiyon koşulları",
  "terms.version": "Sürüm {version} · {date} tarihinde yayımlandı",
  "terms.inShort": "Kısaca",
  "terms.point.buy": "Opsiyon almak: kaybedebileceğiniz en fazla tutar, ödediğiniz tutardır.",
  "terms.point.sell": "Opsiyon satışında aldığınızdan fazlasını kaybedebilirsiniz ve satış teminat gerektirir.",
  "terms.point.prices": "Fiyatlar, Kalks emir defterinde ve doğrudan Kalks tarafından belirlenir.",
  "terms.point.settle": "Opsiyonlar vade sonunda nakdi olarak uzlaşılır.",
  "terms.englishNote": "Aşağıdaki tam metin, bağlayıcı olan İngilizce sürümdür.",
  "terms.acceptedOn": "{version} sürümünü {date} tarihinde kabul ettiniz.",
  "terms.close": "Kapat",
  "terms.unavailable": "Opsiyon koşulları şu anda kullanılamıyor. Lütfen daha sonra tekrar deneyin.",

  // Kalks Trader button
  "trade.ready": "Her şey hazır. Opsiyonlar Kalks Trader'da, opsiyon hesabınızda işlem görür.",
  "trade.cta": "Kalks Trader'da opsiyon işlemi yap",
  "trade.chooseAccount": "Bir hesap seçin",
  "trade.noAccount": "Opsiyon işlemi yapmak için aktif bir opsiyon hesabınız olmalı.",
  "trade.openAccount": "Hesap aç",
  "trade.cashOnly": "Primler ve teminat, hesabınızın kendi nakit bakiyesinden karşılanır. Bonus ve kredi kullanılamaz.",
  "trade.live": "Gerçek",
  "trade.demo": "Demo",

  // Key facts card
  "facts.title": "Kalks FX Options nasıl işler",
  "facts.style": "Avrupa tipi: vade sonunda otomatik olarak kullanılır, asla daha önce değil.",
  "facts.premium": "Prim, kontrat başına USD cinsindendir; alıcılar işlemi açarken tamamını öder.",
  "facts.contracts": "Bir kontrat: 10,000 birim döviz, 1 ons altın, 50 ons gümüş veya 10 varil petrol.",
  "facts.close": "Vade sonundan önce istediğiniz zaman, kote edilen fiyattan tamamen veya kısmen kapatabilirsiniz.",
  "facts.cutoff": "Kesim saatinden önceki son 15 dakikada yeni pozisyon açılamaz.",
  "facts.margin": "Satıcılar stres senaryolarına dayalı teminat tutar; teminat hafta sonlarından önce artabilir.",

  // Academy card
  "learn.title": "Opsiyonlarda yeni misiniz?",
  "learn.text": "Akademideki ücretsiz opsiyon kursuna katılın: call ve put'lar, getiri profilleri, Greeks (duyarlılık ölçüleri), stratejiler ve satışın riskleri.",
  "learn.cta": "Kursu aç",

  // Read-only sessions (view-only logins, staff sessions)
  "readOnly": "Opsiyon işlemlerine yalnızca kendi hesabına giriş yapmış olan müşteri başlayabilir.",

  // Loading errors
  "error.load": "Opsiyon durumunuz yüklenemedi.",
  "error.retry": "Tekrar dene",

  // Demo build
  "demo.note": "Demo: burada hiçbir şey kaydedilmez.",
  // CFD / Options account split
  "account.noneTitle": "Henüz opsiyon hesabınız yok",
  "account.noneText": "Opsiyonlar CFD hesaplarınızdan ayrı, kendi hesabında işlem görür. Bir dakikada gerçek ya da demo bir hesap açın.",
  "account.open": "Opsiyon hesabı aç",
};
export default options;
