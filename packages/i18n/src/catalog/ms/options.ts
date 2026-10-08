import type { NsMessages } from "../../core";

// Kalks FX Options in the Client Area: the Options page, a friendly three-card intro and the button that opens Kalks Trader in options mode.
const options: NsMessages<"options"> = {
  // Navigation entry
  "nav.title": "Opsyen",

  // Page header
  "page.title": "Kalks FX Options",
  "page.subtitle": "Beli atau jual opsyen forex, emas, perak dan minyak terus di dalam Kalks Trader.",
  "page.statusReady": "Sedia untuk berdagang",
  "page.learnCourse": "Kursus opsyen",

  // Hero card
  "hero.eyebrow": "Baharu dalam Kalks Trader",
  "hero.title": "Opsyen pada 13 pasaran, dipermudahkan",
  "hero.text": "Opsyen gaya Eropah bagi pasangan mata wang utama dan silang forex, emas, perak dan minyak mentah. Pilih tamat tempoh harian, mingguan atau bulanan. Setiap opsyen diselesaikan secara tunai dalam dolar AS, jadi anda tidak akan menerima serahan fizikal apa-apa pun.",
  "hero.feature.underlyings.title": "13 aset pendasar",
  "hero.feature.underlyings.text": "9 pasangan forex, emas, perak, minyak mentah WTI dan Brent.",
  "hero.feature.expiries.title": "Harian, mingguan, bulanan",
  "hero.feature.expiries.text": "Tamat tempoh dari hari yang sama hingga hujung bulan, dengan waktu cut-off 10:00 waktu New York.",
  "hero.feature.settlement.title": "Diselesaikan secara tunai dalam USD",
  "hero.feature.settlement.text": "Diselesaikan pada purata harga tengah dalam 30 minit sebelum waktu cut-off.",
  "hero.feature.sides.title": "Beli atau jual",
  "hero.feature.sides.text": "Call dan put, spread, straddle, iron condor dan opsyen barrier.",
  "hero.class.forex": "Forex",
  "hero.class.metals": "Logam",
  "hero.class.energies": "Tenaga",
  "hero.start": "Mulakan",
  "hero.howItWorks": "Cara opsyen berfungsi",

  // Friendly intro before the first options trade: three illustrated cards, one checkbox and the Start button
  "intro.title": "Opsyen dalam tiga idea mudah",
  "intro.subtitle": "Imbasan ringkas sebelum dagangan opsyen pertama anda.",
  "intro.call.title": "Beli Call",
  "intro.call.text": "Anda menjangka harga akan naik.",
  "intro.put.title": "Beli Put",
  "intro.put.text": "Anda menjangka harga akan turun.",
  "intro.risk.title": "Risiko anda terhad apabila anda membeli",
  "intro.risk.text": "Kerugian maksimum anda ialah harga yang anda bayar. (Menjual opsyen boleh rugi lebih banyak.)",
  "intro.legend.result": "Hasil anda semasa tamat tempoh",
  "intro.legend.cost": "Harga yang anda bayar",
  "intro.confirm": "Saya faham cara opsyen berfungsi",
  "intro.terms": "Baca terma penuh",
  "intro.consent": "Dengan bermula, anda menerima terma opsyen.",
  "intro.start": "Mulakan dagangan opsyen",
  "intro.quiz": "Uji diri anda (kuiz)",
  "intro.gotIt": "Faham",
  "intro.toastStarted": "Anda sudah sedia untuk dagangan opsyen",
  "intro.toastFailed": "Tidak dapat memulakan dagangan opsyen. Sila cuba lagi.",
  "intro.toastUpdated": "Terma opsyen baru sahaja dikemas kini. Lihat sekilas, kemudian tekan Mulakan sekali lagi.",

  // Full terms dialog (the published text itself comes from the server, in English)
  "terms.title": "Terma opsyen",
  "terms.version": "Versi {version} · diterbitkan {date}",
  "terms.inShort": "Ringkasnya",
  "terms.point.buy": "Membeli opsyen: kerugian maksimum anda ialah jumlah yang anda bayar.",
  "terms.point.sell": "Menjual opsyen boleh rugi lebih banyak daripada yang anda terima, dan ia menggunakan margin.",
  "terms.point.prices": "Harga ditetapkan dalam buku pesanan Kalks dan oleh Kalks.",
  "terms.point.settle": "Opsyen diselesaikan secara tunai semasa tamat tempoh.",
  "terms.englishNote": "Teks penuh di bawah ialah versi yang mengikat, dalam bahasa Inggeris.",
  "terms.acceptedOn": "Anda menerima versi {version} pada {date}.",
  "terms.close": "Tutup",
  "terms.unavailable": "Terma opsyen tidak tersedia buat masa ini. Sila cuba lagi kemudian.",

  // Kalks Trader button
  "trade.ready": "Semuanya sudah sedia. Opsyen didagangkan dalam Kalks Trader, pada akaun opsyen anda.",
  "trade.cta": "Dagangkan opsyen dalam Kalks Trader",
  "trade.chooseAccount": "Pilih akaun",
  "trade.noAccount": "Anda memerlukan akaun opsyen yang aktif untuk mendagangkan opsyen.",
  "trade.openAccount": "Buka akaun",
  "trade.cashOnly": "Premium dan margin diambil daripada wang tunai akaun anda sendiri. Bonus dan kredit tidak boleh digunakan.",
  "trade.live": "Sebenar",
  "trade.demo": "Demo",

  // Key facts card
  "facts.title": "Cara Kalks FX Options berfungsi",
  "facts.style": "Gaya Eropah: dilaksanakan secara automatik semasa tamat tempoh, tidak sekali-kali lebih awal.",
  "facts.premium": "Premium dalam USD bagi setiap kontrak; pembeli membayarnya sepenuhnya semasa membuka posisi.",
  "facts.contracts": "Satu kontrak: 10,000 unit mata wang, 1 auns emas, 50 auns perak atau 10 tong minyak.",
  "facts.close": "Tutup pada bila-bila masa sebelum tamat tempoh pada harga sebut harga, sepenuhnya atau sebahagian.",
  "facts.cutoff": "Tiada posisi baharu dalam 15 minit terakhir sebelum waktu cut-off.",
  "facts.margin": "Penjual memegang margin berdasarkan senario tekanan; margin boleh meningkat sebelum hujung minggu.",

  // Academy card
  "learn.title": "Baru mengenali opsyen?",
  "learn.text": "Ikuti kursus opsyen percuma di Akademi: call dan put, profil bayaran, Greeks, strategi dan risiko menjual opsyen.",
  "learn.cta": "Buka kursus",

  // Read-only sessions (view-only logins, staff sessions)
  "readOnly": "Hanya klien yang boleh memulakan dagangan opsyen, setelah log masuk ke akaun mereka sendiri.",

  // Loading errors
  "error.load": "Tidak dapat memuatkan status opsyen anda.",
  "error.retry": "Cuba semula",

  // Demo build
  "demo.note": "Demo: tiada apa-apa di sini disimpan.",
  // CFD / Options account split
  "account.noneTitle": "Belum ada akaun opsyen",
  "account.noneText": "Opsyen didagangkan dalam akaun tersendiri, berasingan daripada akaun CFD anda. Buka satu dalam seminit, langsung atau demo.",
  "account.open": "Buka akaun opsyen",
};
export default options;
