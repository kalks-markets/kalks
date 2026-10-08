import type { NsMessages } from "../../core";

// Kalks FX Options in the Client Area: the Options page, a friendly three-card intro and the button that opens Kalks Trader in options mode.
const options: NsMessages<"options"> = {
  // Navigation entry
  "nav.title": "ออปชัน",

  // Page header
  "page.title": "Kalks FX Options",
  "page.subtitle": "ซื้อหรือขายออปชันบนฟอเร็กซ์ ทองคำ เงิน และน้ำมัน ได้โดยตรงใน Kalks Trader",
  "page.statusReady": "พร้อมเทรด",
  "page.learnCourse": "หลักสูตรออปชัน",

  // Hero card
  "hero.eyebrow": "ใหม่ใน Kalks Trader",
  "hero.title": "ออปชันใน 13 ตลาด แบบเข้าใจง่าย",
  "hero.text": "ออปชันแบบยุโรปบนคู่เงินหลักและคู่เงินไขว้ในตลาดฟอเร็กซ์ ทองคำ เงิน และน้ำมันดิบ เลือกวันหมดอายุได้ทั้งรายวัน รายสัปดาห์ หรือรายเดือน ออปชันทุกสัญญาชำระราคาเป็นเงินสดสกุลดอลลาร์สหรัฐ คุณจึงไม่ต้องรับมอบสินทรัพย์ใดๆ เลย",
  "hero.feature.underlyings.title": "สินทรัพย์อ้างอิง 13 รายการ",
  "hero.feature.underlyings.text": "คู่เงินฟอเร็กซ์ 9 คู่ ทองคำ เงิน น้ำมันดิบ WTI และ Brent",
  "hero.feature.expiries.title": "รายวัน รายสัปดาห์ รายเดือน",
  "hero.feature.expiries.text": "วันหมดอายุตั้งแต่ภายในวันเดียวกันจนถึงสิ้นเดือน ตัดรอบเวลา 10:00 ตามเวลานิวยอร์ก",
  "hero.feature.settlement.title": "ชำระราคาเป็นเงินสดสกุล USD",
  "hero.feature.settlement.text": "ชำระราคาด้วยราคากลางเฉลี่ยของช่วง 30 นาทีก่อนเวลาตัดรอบ",
  "hero.feature.sides.title": "ซื้อหรือขาย",
  "hero.feature.sides.text": "คอลและพุท สเปรด สแตรดเดิล ไอรอนคอนดอร์ และแบริเออร์ออปชัน",
  "hero.class.forex": "ฟอเร็กซ์",
  "hero.class.metals": "โลหะ",
  "hero.class.energies": "พลังงาน",
  "hero.start": "เริ่มต้นใช้งาน",
  "hero.howItWorks": "ออปชันทำงานอย่างไร",

  // Friendly intro before the first options trade: three illustrated cards, one checkbox and the Start button
  "intro.title": "เข้าใจออปชันใน 3 ข้อง่ายๆ",
  "intro.subtitle": "ดูภาพรวมสั้นๆ ก่อนเทรดออปชันครั้งแรก",
  "intro.call.title": "ซื้อคอล",
  "intro.call.text": "คุณคิดว่าราคาจะขึ้น",
  "intro.put.title": "ซื้อพุท",
  "intro.put.text": "คุณคิดว่าราคาจะลง",
  "intro.risk.title": "เมื่อซื้อ ความเสี่ยงของคุณมีจำกัด",
  "intro.risk.text": "ขาดทุนสูงสุดคือราคาที่คุณจ่าย (การขายออปชันอาจขาดทุนได้มากกว่านี้)",
  "intro.legend.result": "ผลลัพธ์ของคุณ ณ วันหมดอายุ",
  "intro.legend.cost": "ราคาที่คุณจ่าย",
  "intro.confirm": "ฉันเข้าใจหลักการทำงานของออปชันแล้ว",
  "intro.terms": "อ่านข้อกำหนดฉบับเต็ม",
  "intro.consent": "เมื่อเริ่มต้น ถือว่าคุณยอมรับข้อกำหนดการเทรดออปชัน",
  "intro.start": "เริ่มเทรดออปชัน",
  "intro.quiz": "ทดสอบตัวเอง (แบบทดสอบ)",
  "intro.gotIt": "เข้าใจแล้ว",
  "intro.toastStarted": "พร้อมเทรดออปชันแล้ว",
  "intro.toastFailed": "ไม่สามารถเริ่มเทรดออปชันได้ โปรดลองอีกครั้ง",
  "intro.toastUpdated": "ข้อกำหนดการเทรดออปชันเพิ่งได้รับการอัปเดต ลองดูสั้นๆ แล้วกด “เริ่ม” อีกครั้ง",

  // Full terms dialog (the published text itself comes from the server, in English)
  "terms.title": "ข้อกำหนดการเทรดออปชัน",
  "terms.version": "เวอร์ชัน {version} · เผยแพร่เมื่อ {date}",
  "terms.inShort": "สรุปสั้นๆ",
  "terms.point.buy": "การซื้อออปชัน: ขาดทุนสูงสุดเท่ากับจำนวนที่คุณจ่าย",
  "terms.point.sell": "การขายออปชันอาจขาดทุนมากกว่าที่ได้รับ และต้องใช้มาร์จิ้น",
  "terms.point.prices": "ราคากำหนดบนสมุดคำสั่งของ Kalks และโดย Kalks",
  "terms.point.settle": "ออปชันชำระราคาเป็นเงินสดเมื่อหมดอายุ",
  "terms.englishNote": "ข้อความฉบับเต็มด้านล่างเป็นภาษาอังกฤษ และเป็นฉบับที่มีผลผูกพัน",
  "terms.acceptedOn": "คุณยอมรับเวอร์ชัน {version} เมื่อ {date}",
  "terms.close": "ปิด",
  "terms.unavailable": "ขณะนี้ไม่สามารถแสดงข้อกำหนดการเทรดออปชันได้ โปรดลองอีกครั้งในภายหลัง",

  // Kalks Trader button
  "trade.ready": "พร้อมแล้ว เทรดออปชันได้ใน Kalks Trader บนบัญชีออปชันของคุณ",
  "trade.cta": "เทรดออปชันใน Kalks Trader",
  "trade.chooseAccount": "เลือกบัญชี",
  "trade.noAccount": "คุณต้องมีบัญชีออปชันที่ใช้งานอยู่จึงจะเทรดออปชันได้",
  "trade.openAccount": "เปิดบัญชี",
  "trade.cashOnly": "ค่าพรีเมียมและมาร์จิ้นมาจากเงินสดของบัญชีเท่านั้น ไม่สามารถใช้โบนัสและเครดิตได้",
  "trade.live": "จริง",
  "trade.demo": "ทดลอง",

  // Key facts card
  "facts.title": "Kalks FX Options ทำงานอย่างไร",
  "facts.style": "แบบยุโรป: ใช้สิทธิโดยอัตโนมัติเมื่อหมดอายุ และไม่มีการใช้สิทธิก่อนหน้านั้น",
  "facts.premium": "ค่าพรีเมียมคิดเป็น USD ต่อสัญญา ผู้ซื้อจ่ายเต็มจำนวนเมื่อเปิดสถานะ",
  "facts.contracts": "1 สัญญา: สกุลเงิน 10,000 หน่วย ทองคำ 1 ออนซ์ เงิน 50 ออนซ์ หรือน้ำมัน 10 บาร์เรล",
  "facts.close": "ปิดสถานะทั้งหมดหรือบางส่วนได้ทุกเมื่อก่อนหมดอายุ ตามราคาที่เสนอ",
  "facts.cutoff": "ไม่สามารถเปิดสถานะใหม่ในช่วง 15 นาทีสุดท้ายก่อนเวลาตัดรอบ",
  "facts.margin": "ผู้ขายต้องมีมาร์จิ้นตามสถานการณ์จำลองภาวะวิกฤต ซึ่งอาจเพิ่มขึ้นก่อนวันหยุดสุดสัปดาห์",

  // Academy card
  "learn.title": "เพิ่งเริ่มต้นกับออปชัน?",
  "learn.text": "เรียนหลักสูตรออปชันฟรีในอะคาเดมี: คอลและพุท ผลตอบแทนของออปชัน ค่ากรีก กลยุทธ์ และความเสี่ยงของการขาย",
  "learn.cta": "เปิดหลักสูตร",

  // Read-only sessions (view-only logins, staff sessions)
  "readOnly": "เฉพาะลูกค้าที่เข้าสู่ระบบบัญชีของตนเองเท่านั้นที่สามารถเริ่มเทรดออปชันได้",

  // Loading errors
  "error.load": "ไม่สามารถโหลดสถานะออปชันของคุณได้",
  "error.retry": "ลองใหม่",

  // Demo build
  "demo.note": "โหมดสาธิต: ไม่มีการบันทึกข้อมูลใดๆ ในหน้านี้",
  // CFD / Options account split
  "account.noneTitle": "ยังไม่มีบัญชีออปชัน",
  "account.noneText": "ออปชันเทรดในบัญชีของตัวเอง แยกจากบัญชี CFD ของคุณ เปิดได้ในหนึ่งนาที ทั้งบัญชีจริงและเดโม",
  "account.open": "เปิดบัญชีออปชัน",
};
export default options;
