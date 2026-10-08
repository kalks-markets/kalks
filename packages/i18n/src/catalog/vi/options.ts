import type { NsMessages } from "../../core";

// Kalks FX Options in the Client Area: the Options page, a friendly three-card intro and the button that opens Kalks Trader in options mode.
const options: NsMessages<"options"> = {
  // Navigation entry
  "nav.title": "Quyền chọn",

  // Page header
  "page.title": "Kalks FX Options",
  "page.subtitle": "Mua hoặc bán quyền chọn trên forex, vàng, bạc và dầu ngay trong Kalks Trader.",
  "page.statusReady": "Sẵn sàng giao dịch",
  "page.learnCourse": "Khóa học quyền chọn",

  // Hero card
  "hero.eyebrow": "Mới trên Kalks Trader",
  "hero.title": "Quyền chọn trên 13 thị trường, thật đơn giản",
  "hero.text": "Quyền chọn kiểu châu Âu trên các cặp forex chính và cặp chéo, vàng, bạc và dầu thô. Chọn kỳ đáo hạn theo ngày, tuần hoặc tháng. Mọi quyền chọn đều được thanh toán bằng tiền mặt bằng đô la Mỹ, nên bạn không bao giờ phải nhận giao bất kỳ tài sản nào.",
  "hero.feature.underlyings.title": "13 tài sản cơ sở",
  "hero.feature.underlyings.text": "9 cặp forex, vàng, bạc, dầu thô WTI và Brent.",
  "hero.feature.expiries.title": "Theo ngày, tuần, tháng",
  "hero.feature.expiries.text": "Kỳ đáo hạn từ trong ngày đến cuối tháng, chốt lúc 10:00 giờ New York.",
  "hero.feature.settlement.title": "Thanh toán tiền mặt bằng USD",
  "hero.feature.settlement.text": "Thanh toán theo giá giữa trung bình của 30 phút trước giờ chốt.",
  "hero.feature.sides.title": "Mua hoặc bán",
  "hero.feature.sides.text": "Quyền chọn mua và quyền chọn bán, spread, straddle, iron condor và quyền chọn rào cản.",
  "hero.class.forex": "Forex",
  "hero.class.metals": "Kim loại",
  "hero.class.energies": "Năng lượng",
  "hero.start": "Bắt đầu ngay",
  "hero.howItWorks": "Cách quyền chọn hoạt động",

  // Friendly intro before the first options trade: three illustrated cards, one checkbox and the Start button
  "intro.title": "Quyền chọn qua ba ý đơn giản",
  "intro.subtitle": "Xem nhanh trước giao dịch quyền chọn đầu tiên của bạn.",
  "intro.call.title": "Mua quyền chọn mua (Call)",
  "intro.call.text": "Bạn nghĩ giá sẽ tăng.",
  "intro.put.title": "Mua quyền chọn bán (Put)",
  "intro.put.text": "Bạn nghĩ giá sẽ giảm.",
  "intro.risk.title": "Rủi ro có giới hạn khi bạn mua",
  "intro.risk.text": "Khoản lỗ tối đa là giá bạn trả. (Bán quyền chọn có thể lỗ nhiều hơn.)",
  "intro.legend.result": "Kết quả của bạn khi đáo hạn",
  "intro.legend.cost": "Giá bạn trả",
  "intro.confirm": "Tôi hiểu cách quyền chọn hoạt động",
  "intro.terms": "Đọc toàn bộ điều khoản",
  "intro.consent": "Khi bắt đầu, bạn chấp nhận các điều khoản quyền chọn.",
  "intro.start": "Bắt đầu giao dịch quyền chọn",
  "intro.quiz": "Tự kiểm tra (trắc nghiệm)",
  "intro.gotIt": "Đã hiểu",
  "intro.toastStarted": "Bạn đã sẵn sàng giao dịch quyền chọn",
  "intro.toastFailed": "Không thể bắt đầu giao dịch quyền chọn. Vui lòng thử lại.",
  "intro.toastUpdated": "Điều khoản quyền chọn vừa được cập nhật. Hãy xem qua, rồi nhấn Bắt đầu lần nữa.",

  // Full terms dialog (the published text itself comes from the server, in English)
  "terms.title": "Điều khoản quyền chọn",
  "terms.version": "Phiên bản {version} · công bố ngày {date}",
  "terms.inShort": "Tóm tắt",
  "terms.point.buy": "Mua quyền chọn: khoản lỗ tối đa là số tiền bạn trả.",
  "terms.point.sell": "Bán quyền chọn có thể lỗ nhiều hơn số tiền bạn nhận được và cần dùng ký quỹ.",
  "terms.point.prices": "Giá được hình thành trên sổ lệnh của Kalks và do Kalks đưa ra.",
  "terms.point.settle": "Quyền chọn được thanh toán bằng tiền mặt khi đáo hạn.",
  "terms.englishNote": "Toàn văn bên dưới là bản có giá trị ràng buộc, bằng tiếng Anh.",
  "terms.acceptedOn": "Bạn đã chấp nhận phiên bản {version} vào {date}.",
  "terms.close": "Đóng",
  "terms.unavailable": "Hiện không thể hiển thị điều khoản quyền chọn. Vui lòng thử lại sau.",

  // Kalks Trader button
  "trade.ready": "Mọi thứ đã sẵn sàng. Quyền chọn được giao dịch trong Kalks Trader, trên tài khoản quyền chọn của bạn.",
  "trade.cta": "Giao dịch quyền chọn trong Kalks Trader",
  "trade.chooseAccount": "Chọn tài khoản",
  "trade.noAccount": "Bạn cần có một tài khoản quyền chọn đang hoạt động để giao dịch quyền chọn.",
  "trade.openAccount": "Mở tài khoản",
  "trade.cashOnly": "Phí quyền chọn và ký quỹ được lấy từ tiền mặt của chính tài khoản. Không thể dùng tiền thưởng và tín dụng.",
  "trade.live": "Thực",
  "trade.demo": "Demo",

  // Key facts card
  "facts.title": "Cách Kalks FX Options hoạt động",
  "facts.style": "Kiểu châu Âu: tự động thực hiện khi đáo hạn, không bao giờ thực hiện trước đó.",
  "facts.premium": "Phí quyền chọn tính bằng USD trên mỗi hợp đồng; người mua trả toàn bộ khi mở vị thế.",
  "facts.contracts": "Một hợp đồng: 10,000 đơn vị tiền tệ, 1 oz vàng, 50 oz bạc hoặc 10 thùng dầu.",
  "facts.close": "Đóng toàn bộ hoặc một phần bất cứ lúc nào trước khi đáo hạn theo giá niêm yết.",
  "facts.cutoff": "Không thể mở vị thế mới trong 15 phút cuối trước giờ chốt.",
  "facts.margin": "Người bán duy trì ký quỹ dựa trên các kịch bản căng thẳng; mức này có thể tăng trước cuối tuần.",

  // Academy card
  "learn.title": "Mới làm quen với quyền chọn?",
  "learn.text": "Tham gia khóa học quyền chọn miễn phí trong Học viện: quyền chọn mua và bán, cơ cấu lãi lỗ, các chỉ số Hy Lạp (Greeks), chiến lược và rủi ro khi bán.",
  "learn.cta": "Mở khóa học",

  // Read-only sessions (view-only logins, staff sessions)
  "readOnly": "Chỉ khách hàng, khi đăng nhập vào tài khoản của chính mình, mới có thể bắt đầu giao dịch quyền chọn.",

  // Loading errors
  "error.load": "Không thể tải trạng thái quyền chọn của bạn.",
  "error.retry": "Thử lại",

  // Demo build
  "demo.note": "Demo: không có gì ở đây được lưu lại.",
  // CFD / Options account split
  "account.noneTitle": "Chưa có tài khoản quyền chọn",
  "account.noneText": "Quyền chọn được giao dịch trong tài khoản riêng, tách khỏi các tài khoản CFD. Mở một tài khoản trong một phút, thật hoặc demo.",
  "account.open": "Mở tài khoản quyền chọn",
};
export default options;
