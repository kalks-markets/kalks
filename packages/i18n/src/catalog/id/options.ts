import type { NsMessages } from "../../core";

// Kalks FX Options in the Client Area: the Options page, a friendly three-card intro and the button that opens Kalks Trader in options mode.
const options: NsMessages<"options"> = {
  // Navigation entry
  "nav.title": "Opsi",

  // Page header
  "page.title": "Kalks FX Options",
  "page.subtitle": "Beli atau jual opsi forex, emas, perak, dan minyak langsung di Kalks Trader.",
  "page.statusReady": "Siap trading",
  "page.learnCourse": "Kursus opsi",

  // Hero card
  "hero.eyebrow": "Baru di Kalks Trader",
  "hero.title": "Opsi di 13 pasar, dibuat mudah",
  "hero.text": "Opsi gaya Eropa untuk pasangan mata uang forex mayor dan silang, emas, perak, dan minyak mentah. Pilih jatuh tempo harian, mingguan, atau bulanan. Setiap opsi diselesaikan secara tunai dalam dolar AS, sehingga Anda tidak pernah menerima penyerahan fisik apa pun.",
  "hero.feature.underlyings.title": "13 aset dasar",
  "hero.feature.underlyings.text": "9 pasangan mata uang forex, emas, perak, minyak mentah WTI dan Brent.",
  "hero.feature.expiries.title": "Harian, mingguan, bulanan",
  "hero.feature.expiries.text": "Jatuh tempo dari hari yang sama hingga akhir bulan, dengan batas waktu pukul 10:00 waktu New York.",
  "hero.feature.settlement.title": "Diselesaikan tunai dalam USD",
  "hero.feature.settlement.text": "Diselesaikan pada rata-rata harga tengah 30 menit sebelum batas waktu.",
  "hero.feature.sides.title": "Beli atau jual",
  "hero.feature.sides.text": "Call dan put, spread, straddle, iron condor, dan opsi barrier.",
  "hero.class.forex": "Forex",
  "hero.class.metals": "Logam",
  "hero.class.energies": "Energi",
  "hero.start": "Mulai sekarang",
  "hero.howItWorks": "Cara kerja opsi",

  // Friendly intro before the first options trade: three illustrated cards, one checkbox and the Start button
  "intro.title": "Opsi dalam tiga ide sederhana",
  "intro.subtitle": "Gambaran singkat sebelum trading opsi pertama Anda.",
  "intro.call.title": "Beli Call",
  "intro.call.text": "Anda memperkirakan harga akan naik.",
  "intro.put.title": "Beli Put",
  "intro.put.text": "Anda memperkirakan harga akan turun.",
  "intro.risk.title": "Risiko Anda terbatas saat membeli",
  "intro.risk.text": "Kerugian maksimal Anda adalah harga yang Anda bayar. (Menjual opsi bisa rugi lebih besar.)",
  "intro.legend.result": "Hasil Anda saat jatuh tempo",
  "intro.legend.cost": "Harga yang Anda bayar",
  "intro.confirm": "Saya memahami cara kerja opsi",
  "intro.terms": "Baca ketentuan lengkap",
  "intro.consent": "Dengan memulai, Anda menyetujui ketentuan opsi.",
  "intro.start": "Mulai trading opsi",
  "intro.quiz": "Uji diri Anda (kuis)",
  "intro.gotIt": "Mengerti",
  "intro.toastStarted": "Anda sudah siap trading opsi",
  "intro.toastFailed": "Tidak dapat memulai trading opsi. Silakan coba lagi.",
  "intro.toastUpdated": "Ketentuan opsi baru saja diperbarui. Lihat sekilas, lalu tekan Mulai lagi.",

  // Full terms dialog (the published text itself comes from the server, in English)
  "terms.title": "Ketentuan opsi",
  "terms.version": "Versi {version} · diterbitkan {date}",
  "terms.inShort": "Singkatnya",
  "terms.point.buy": "Membeli opsi: kerugian maksimal Anda sebesar yang Anda bayar.",
  "terms.point.sell": "Menjual opsi bisa rugi lebih besar daripada yang Anda terima, dan menggunakan margin.",
  "terms.point.prices": "Harga ditentukan di order book Kalks dan oleh Kalks.",
  "terms.point.settle": "Opsi diselesaikan secara tunai saat jatuh tempo.",
  "terms.englishNote": "Teks lengkap di bawah ini adalah versi yang mengikat, dalam bahasa Inggris.",
  "terms.acceptedOn": "Anda menyetujui versi {version} pada {date}.",
  "terms.close": "Tutup",
  "terms.unavailable": "Ketentuan opsi sedang tidak tersedia. Silakan coba lagi nanti.",

  // Kalks Trader button
  "trade.ready": "Semua sudah siap. Opsi diperdagangkan di Kalks Trader, pada akun opsi Anda.",
  "trade.cta": "Trading opsi di Kalks Trader",
  "trade.chooseAccount": "Pilih akun",
  "trade.noAccount": "Anda memerlukan akun opsi aktif untuk trading opsi.",
  "trade.openAccount": "Buka akun",
  "trade.cashOnly": "Premi dan margin diambil dari dana tunai akun Anda sendiri. Bonus dan kredit tidak dapat digunakan.",
  "trade.live": "Live",
  "trade.demo": "Demo",

  // Key facts card
  "facts.title": "Cara kerja Kalks FX Options",
  "facts.style": "Gaya Eropa: dieksekusi otomatis saat jatuh tempo, tidak pernah sebelumnya.",
  "facts.premium": "Premi dalam USD per kontrak; pembeli membayarnya penuh saat membuka posisi.",
  "facts.contracts": "Satu kontrak: 10,000 unit mata uang, 1 ounce emas, 50 ounce perak, atau 10 barel minyak.",
  "facts.close": "Tutup kapan saja sebelum jatuh tempo pada harga kuotasi, seluruhnya atau sebagian.",
  "facts.cutoff": "Tidak ada posisi baru dalam 15 menit terakhir sebelum batas waktu.",
  "facts.margin": "Penjual wajib memiliki margin berdasarkan skenario stres; margin dapat naik menjelang akhir pekan.",

  // Academy card
  "learn.title": "Baru mengenal opsi?",
  "learn.text": "Ikuti kursus opsi gratis di Akademi: call dan put, profil keuntungan, Greeks, strategi, dan risiko menjual opsi.",
  "learn.cta": "Buka kursus",

  // Read-only sessions (view-only logins, staff sessions)
  "readOnly": "Hanya klien yang dapat memulai trading opsi, dengan masuk ke akunnya sendiri.",

  // Loading errors
  "error.load": "Tidak dapat memuat status opsi Anda.",
  "error.retry": "Coba lagi",

  // Demo build
  "demo.note": "Demo: tidak ada yang disimpan di sini.",
  // CFD / Options account split
  "account.noneTitle": "Belum ada akun opsi",
  "account.noneText": "Opsi diperdagangkan di akunnya sendiri, terpisah dari akun CFD Anda. Buka satu dalam semenit, live atau demo.",
  "account.open": "Buka akun opsi",
};
export default options;
