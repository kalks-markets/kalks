/**
 * Back Office · Content · Email templates (editor page).
 * Import via `@kalks/mock/admin-email-templates`. Exports are prefixed EML_.
 */
import { PEOPLE, type Person } from "./people";

export type EmlLang = "en" | "hi" | "ar" | "es" | "pt" | "vi" | "id" | "tr" | "fr" | "ur";

export const EML_LANGS: { code: EmlLang; name: string; native: string; flag: string; rtl: boolean }[] = [
  { code: "en", name: "English", native: "English", flag: "gb", rtl: false },
  { code: "hi", name: "Hindi", native: "हिन्दी", flag: "in", rtl: false },
  { code: "ar", name: "Arabic", native: "العربية", flag: "sa", rtl: true },
  { code: "es", name: "Spanish", native: "Español", flag: "es", rtl: false },
  { code: "pt", name: "Portuguese", native: "Português", flag: "br", rtl: false },
  { code: "vi", name: "Vietnamese", native: "Tiếng Việt", flag: "vn", rtl: false },
  { code: "id", name: "Indonesian", native: "Bahasa Indonesia", flag: "id", rtl: false },
  { code: "tr", name: "Turkish", native: "Türkçe", flag: "tr", rtl: false },
  { code: "fr", name: "French", native: "Français", flag: "fr", rtl: false },
  { code: "ur", name: "Urdu", native: "اردو", flag: "pk", rtl: true },
];

export const EML_VARIABLES: { key: string; label: string; sample: string }[] = [
  { key: "first_name", label: "First name", sample: "Arjun" },
  { key: "amount", label: "Amount", sample: "2,500.00" },
  { key: "currency", label: "Currency", sample: "USDT" },
  { key: "account_login", label: "Account login", sample: "80412337" },
  { key: "otp_code", label: "OTP code", sample: "482915" },
  { key: "method", label: "Method", sample: "USDT · TRC20" },
  { key: "tx_id", label: "Transaction ID", sample: "TX-904412" },
  { key: "device", label: "Device", sample: "Chrome on macOS" },
  { key: "ip_address", label: "IP address", sample: "94.200.18.41" },
  { key: "city", label: "City", sample: "Dubai, AE" },
  { key: "reason", label: "Reason", sample: "Proof of address older than 90 days" },
  { key: "margin_level", label: "Margin level", sample: "84.2%" },
  { key: "stop_out_level", label: "Stop-out level", sample: "50%" },
  { key: "date", label: "Date & time", sample: "24 Sep 2026, 14:32 GMT+3" },
  { key: "commission", label: "Commission", sample: "$4,812.40" },
  { key: "period", label: "Period", sample: "August 2026" },
  { key: "broker_name", label: "Broker name", sample: "Kalks Markets" },
  { key: "support_email", label: "Support email", sample: "support@kalkstrade.com" },
];

export interface EmlContent {
  subject: string;
  preheader: string;
  body: string;
  cta?: string;
}

export interface EmlVersion {
  v: number;
  by: Person;
  at: string;
  note: string;
  live?: boolean;
}

export interface EmlTemplate {
  id: string;
  name: string;
  group: "Account" | "Security" | "Funding" | "Compliance" | "Trading" | "Partners";
  trigger: string;
  icon: string; // Icon3D
  content: Partial<Record<EmlLang, EmlContent>> & { en: EmlContent };
  sent30d: number;
  openRate: number;
  status: "live" | "draft";
  versions: EmlVersion[];
}

const V = (v: number, pi: number, at: string, note: string, live?: boolean): EmlVersion => ({ v, by: PEOPLE[pi]!, at, note, live });

export const EML_TEMPLATES: EmlTemplate[] = [
  {
    id: "welcome",
    name: "Welcome",
    group: "Account",
    trigger: "user.registered",
    icon: "party_popper",
    sent30d: 14820,
    openRate: 68.4,
    status: "live",
    content: {
      en: { subject: "Welcome to {{broker_name}}, {{first_name}}", preheader: "Your client area is ready.", body: "Hi {{first_name}},\n\nWelcome to {{broker_name}}. Your client area is ready and a demo account with $100,000 in virtual funds is waiting for you.\n\nTo trade live, verify your identity and make your first deposit. It takes less than five minutes.", cta: "Complete verification" },
      hi: { subject: "{{broker_name}} में आपका स्वागत है, {{first_name}}", preheader: "आपका क्लाइंट एरिया तैयार है।", body: "नमस्ते {{first_name}},\n\n{{broker_name}} में आपका स्वागत है। आपका क्लाइंट एरिया तैयार है और $100,000 वर्चुअल फंड वाला डेमो खाता आपका इंतज़ार कर रहा है।\n\nलाइव ट्रेडिंग के लिए अपनी पहचान सत्यापित करें और पहला डिपॉज़िट करें।", cta: "सत्यापन पूरा करें" },
      ar: { subject: "مرحباً بك في {{broker_name}} يا {{first_name}}", preheader: "منطقة العميل الخاصة بك جاهزة.", body: "مرحباً {{first_name}}،\n\nأهلاً بك في {{broker_name}}. منطقة العميل جاهزة، وحساب تجريبي بقيمة 100,000 دولار افتراضية بانتظارك.\n\nللتداول الحقيقي، أكمل التحقق من هويتك وقم بأول إيداع. لن يستغرق ذلك أكثر من خمس دقائق.", cta: "أكمل التحقق" },
      es: { subject: "Bienvenido a {{broker_name}}, {{first_name}}", preheader: "Tu área de cliente está lista.", body: "Hola {{first_name}},\n\nBienvenido a {{broker_name}}. Tu área de cliente está lista y una cuenta demo con $100,000 virtuales te espera.\n\nPara operar en real, verifica tu identidad y haz tu primer depósito. Toma menos de cinco minutos.", cta: "Completar verificación" },
      pt: { subject: "Bem-vindo à {{broker_name}}, {{first_name}}", preheader: "Sua área do cliente está pronta.", body: "Olá {{first_name}},\n\nBem-vindo à {{broker_name}}. Sua área do cliente está pronta e uma conta demo com $100.000 virtuais espera por você.\n\nPara operar ao vivo, verifique sua identidade e faça seu primeiro depósito.", cta: "Concluir verificação" },
      vi: { subject: "Chào mừng {{first_name}} đến với {{broker_name}}", preheader: "Khu vực khách hàng của bạn đã sẵn sàng.", body: "Xin chào {{first_name}},\n\nChào mừng bạn đến với {{broker_name}}. Tài khoản demo với $100,000 ảo đã sẵn sàng.\n\nĐể giao dịch thật, hãy xác minh danh tính và nạp tiền lần đầu.", cta: "Hoàn tất xác minh" },
    },
    versions: [V(7, 4, "2026-09-12T11:20:00+03:00", "Shortened intro, new CTA copy", true), V(6, 12, "2026-08-02T09:05:00+03:00", "Added demo account line"), V(5, 4, "2026-06-18T16:40:00+03:00", "Hindi + Vietnamese translations")],
  },
  {
    id: "otp",
    name: "Email OTP",
    group: "Security",
    trigger: "auth.otp_requested",
    icon: "key",
    sent30d: 96410,
    openRate: 91.2,
    status: "live",
    content: {
      en: { subject: "{{otp_code}} is your {{broker_name}} verification code", preheader: "This code expires in 10 minutes.", body: "Hi {{first_name}},\n\nUse this code to confirm your action. It expires in 10 minutes.\n\n{{otp_code}}\n\nIf you didn't request it, change your password immediately and contact {{support_email}}." },
      hi: { subject: "{{otp_code}} आपका {{broker_name}} सत्यापन कोड है", preheader: "यह कोड 10 मिनट में समाप्त हो जाएगा।", body: "नमस्ते {{first_name}},\n\nअपनी कार्रवाई की पुष्टि के लिए यह कोड इस्तेमाल करें। यह 10 मिनट में समाप्त हो जाएगा।\n\n{{otp_code}}\n\nअगर आपने यह अनुरोध नहीं किया, तो तुरंत पासवर्ड बदलें।" },
      ar: { subject: "{{otp_code}} هو رمز التحقق من {{broker_name}}", preheader: "تنتهي صلاحية هذا الرمز خلال 10 دقائق.", body: "مرحباً {{first_name}}،\n\nاستخدم هذا الرمز لتأكيد العملية. تنتهي صلاحيته خلال 10 دقائق.\n\n{{otp_code}}\n\nإذا لم تطلب هذا الرمز، غيّر كلمة المرور فوراً وتواصل مع {{support_email}}." },
      es: { subject: "{{otp_code}} es tu código de verificación de {{broker_name}}", preheader: "Este código caduca en 10 minutos.", body: "Hola {{first_name}},\n\nUsa este código para confirmar tu acción. Caduca en 10 minutos.\n\n{{otp_code}}\n\nSi no lo solicitaste, cambia tu contraseña y contacta con {{support_email}}." },
      pt: { subject: "{{otp_code}} é o seu código de verificação da {{broker_name}}", preheader: "Este código expira em 10 minutos.", body: "Olá {{first_name}},\n\nUse este código para confirmar sua ação. Ele expira em 10 minutos.\n\n{{otp_code}}" },
      vi: { subject: "{{otp_code}} là mã xác minh {{broker_name}} của bạn", preheader: "Mã hết hạn sau 10 phút.", body: "Xin chào {{first_name}},\n\nSử dụng mã này để xác nhận thao tác. Mã hết hạn sau 10 phút.\n\n{{otp_code}}" },
      id: { subject: "{{otp_code}} adalah kode verifikasi {{broker_name}} Anda", preheader: "Kode berlaku 10 menit.", body: "Halo {{first_name}},\n\nGunakan kode ini untuk mengonfirmasi tindakan Anda. Berlaku 10 menit.\n\n{{otp_code}}" },
      tr: { subject: "{{otp_code}} {{broker_name}} doğrulama kodunuzdur", preheader: "Kod 10 dakika içinde geçersiz olur.", body: "Merhaba {{first_name}},\n\nİşleminizi onaylamak için bu kodu kullanın. 10 dakika geçerlidir.\n\n{{otp_code}}" },
    },
    versions: [V(4, 12, "2026-09-02T10:10:00+03:00", "Security copy reviewed by Compliance", true), V(3, 12, "2026-05-11T12:00:00+03:00", "Code shown in large mono block")],
  },
  {
    id: "new_device",
    name: "New device login",
    group: "Security",
    trigger: "security.new_device",
    icon: "locked",
    sent30d: 18840,
    openRate: 72.6,
    status: "live",
    content: {
      en: { subject: "New sign-in to your {{broker_name}} account", preheader: "{{device}} · {{city}}", body: "Hi {{first_name}},\n\nWe noticed a new sign-in to your account.\n\nDevice: {{device}}\nLocation: {{city}}\nIP address: {{ip_address}}\nTime: {{date}}\n\nIf this was you, no action is needed. If not, secure your account now.", cta: "Secure my account" },
      ar: { subject: "تسجيل دخول جديد إلى حسابك في {{broker_name}}", preheader: "{{device}} · {{city}}", body: "مرحباً {{first_name}}،\n\nلاحظنا تسجيل دخول جديد إلى حسابك.\n\nالجهاز: {{device}}\nالموقع: {{city}}\nعنوان IP: {{ip_address}}\nالوقت: {{date}}\n\nإذا لم تكن أنت، قم بتأمين حسابك الآن.", cta: "تأمين حسابي" },
      es: { subject: "Nuevo inicio de sesión en tu cuenta de {{broker_name}}", preheader: "{{device}} · {{city}}", body: "Hola {{first_name}},\n\nDetectamos un nuevo inicio de sesión.\n\nDispositivo: {{device}}\nUbicación: {{city}}\nIP: {{ip_address}}\nHora: {{date}}\n\nSi no fuiste tú, protege tu cuenta ahora.", cta: "Proteger mi cuenta" },
    },
    versions: [V(3, 4, "2026-08-21T15:30:00+03:00", "Added IP address row", true), V(2, 12, "2026-04-09T09:00:00+03:00", "Initial security review")],
  },
  {
    id: "deposit",
    name: "Deposit confirmed",
    group: "Funding",
    trigger: "wallet.deposit_completed",
    icon: "money_with_wings",
    sent30d: 21340,
    openRate: 74.9,
    status: "live",
    content: {
      en: { subject: "Deposit of {{amount}} {{currency}} received", preheader: "Funds are available in your wallet.", body: "Hi {{first_name}},\n\nWe've received your deposit of {{amount}} {{currency}} via {{method}}. The funds are now available in your wallet.\n\nTransaction ID: {{tx_id}}\nDate: {{date}}", cta: "Transfer to trading account" },
      hi: { subject: "{{amount}} {{currency}} का डिपॉज़िट प्राप्त हुआ", preheader: "फंड आपके वॉलेट में उपलब्ध है।", body: "नमस्ते {{first_name}},\n\nहमें {{method}} से {{amount}} {{currency}} का आपका डिपॉज़िट मिल गया है। फंड अब आपके वॉलेट में उपलब्ध है।\n\nलेनदेन ID: {{tx_id}}", cta: "ट्रेडिंग खाते में ट्रांसफर करें" },
      ar: { subject: "تم استلام إيداع بقيمة {{amount}} {{currency}}", preheader: "الأموال متاحة في محفظتك.", body: "مرحباً {{first_name}}،\n\nاستلمنا إيداعك بقيمة {{amount}} {{currency}} عبر {{method}}. الأموال متاحة الآن في محفظتك.\n\nرقم العملية: {{tx_id}}\nالتاريخ: {{date}}", cta: "التحويل إلى حساب التداول" },
      es: { subject: "Depósito de {{amount}} {{currency}} recibido", preheader: "Los fondos ya están en tu billetera.", body: "Hola {{first_name}},\n\nHemos recibido tu depósito de {{amount}} {{currency}} vía {{method}}. Los fondos ya están disponibles en tu billetera.\n\nID de transacción: {{tx_id}}\nFecha: {{date}}", cta: "Transferir a cuenta de trading" },
      pt: { subject: "Depósito de {{amount}} {{currency}} recebido", preheader: "Os fundos estão na sua carteira.", body: "Olá {{first_name}},\n\nRecebemos seu depósito de {{amount}} {{currency}} via {{method}}. Os fundos já estão disponíveis.\n\nID da transação: {{tx_id}}", cta: "Transferir para conta de trading" },
      vi: { subject: "Đã nhận khoản nạp {{amount}} {{currency}}", preheader: "Tiền đã có trong ví của bạn.", body: "Xin chào {{first_name}},\n\nChúng tôi đã nhận khoản nạp {{amount}} {{currency}} qua {{method}}.\n\nMã giao dịch: {{tx_id}}", cta: "Chuyển vào tài khoản giao dịch" },
    },
    versions: [V(9, 12, "2026-09-18T10:45:00+03:00", "Added transfer CTA", true), V(8, 13, "2026-08-30T17:20:00+03:00", "USDT network in method line"), V(7, 12, "2026-07-14T09:00:00+03:00", "Portuguese copy fix")],
  },
  {
    id: "wd_requested",
    name: "Withdrawal requested",
    group: "Funding",
    trigger: "wallet.withdrawal_requested",
    icon: "hourglass_not_done",
    sent30d: 11240,
    openRate: 79.8,
    status: "live",
    content: {
      en: { subject: "We received your withdrawal request of {{amount}} {{currency}}", preheader: "Usually processed within 4 hours.", body: "Hi {{first_name}},\n\nYour request to withdraw {{amount}} {{currency}} via {{method}} is being reviewed. Most withdrawals are processed within 4 hours.\n\nRequest ID: {{tx_id}}", cta: "Track request" },
      ar: { subject: "استلمنا طلب سحب بقيمة {{amount}} {{currency}}", preheader: "تتم المعالجة عادةً خلال 4 ساعات.", body: "مرحباً {{first_name}}،\n\nطلبك لسحب {{amount}} {{currency}} عبر {{method}} قيد المراجعة. تتم معالجة معظم طلبات السحب خلال 4 ساعات.\n\nرقم الطلب: {{tx_id}}", cta: "تتبع الطلب" },
      es: { subject: "Recibimos tu solicitud de retiro de {{amount}} {{currency}}", preheader: "Normalmente se procesa en 4 horas.", body: "Hola {{first_name}},\n\nTu solicitud de retiro de {{amount}} {{currency}} vía {{method}} está en revisión.\n\nID de solicitud: {{tx_id}}", cta: "Seguir solicitud" },
    },
    versions: [V(3, 13, "2026-09-05T12:00:00+03:00", "SLA wording 4 hours", true), V(2, 12, "2026-06-01T10:00:00+03:00", "Initial")],
  },
  {
    id: "wd_approved",
    name: "Withdrawal approved",
    group: "Funding",
    trigger: "wallet.withdrawal_approved",
    icon: "dollar_banknote",
    sent30d: 9820,
    openRate: 81.3,
    status: "live",
    content: {
      en: { subject: "Your withdrawal of {{amount}} {{currency}} is on its way", preheader: "Approved and sent to the network.", body: "Hi {{first_name}},\n\nYour withdrawal of {{amount}} {{currency}} via {{method}} has been approved and sent. Network confirmations usually take a few minutes.\n\nTransaction ID: {{tx_id}}", cta: "View transaction" },
      ar: { subject: "سحبك بقيمة {{amount}} {{currency}} في الطريق", preheader: "تمت الموافقة والإرسال إلى الشبكة.", body: "مرحباً {{first_name}}،\n\nتمت الموافقة على سحبك بقيمة {{amount}} {{currency}} عبر {{method}} وتم إرساله. يستغرق التأكيد على الشبكة بضع دقائق عادةً.\n\nرقم العملية: {{tx_id}}", cta: "عرض العملية" },
      es: { subject: "Tu retiro de {{amount}} {{currency}} está en camino", preheader: "Aprobado y enviado a la red.", body: "Hola {{first_name}},\n\nTu retiro de {{amount}} {{currency}} vía {{method}} fue aprobado y enviado.\n\nID de transacción: {{tx_id}}", cta: "Ver transacción" },
      hi: { subject: "{{amount}} {{currency}} की आपकी निकासी भेज दी गई है", preheader: "स्वीकृत और नेटवर्क पर भेजा गया।", body: "नमस्ते {{first_name}},\n\n{{method}} से {{amount}} {{currency}} की आपकी निकासी स्वीकृत होकर भेज दी गई है।\n\nलेनदेन ID: {{tx_id}}", cta: "लेनदेन देखें" },
    },
    versions: [V(5, 12, "2026-09-18T10:50:00+03:00", "Network confirmation line", true), V(4, 13, "2026-07-02T11:00:00+03:00", "Hindi translation")],
  },
  {
    id: "wd_rejected",
    name: "Withdrawal rejected",
    group: "Funding",
    trigger: "wallet.withdrawal_rejected",
    icon: "warning",
    sent30d: 412,
    openRate: 88.1,
    status: "live",
    content: {
      en: { subject: "Your withdrawal of {{amount}} {{currency}} could not be processed", preheader: "Funds have been returned to your wallet.", body: "Hi {{first_name}},\n\nWe couldn't process your withdrawal of {{amount}} {{currency}}. The funds have been returned to your wallet.\n\nReason: {{reason}}\n\nYou can submit a new request at any time.", cta: "Open wallet" },
      ar: { subject: "تعذّر تنفيذ سحبك بقيمة {{amount}} {{currency}}", preheader: "أُعيدت الأموال إلى محفظتك.", body: "مرحباً {{first_name}}،\n\nتعذّر علينا تنفيذ سحبك بقيمة {{amount}} {{currency}}. أُعيدت الأموال إلى محفظتك.\n\nالسبب: {{reason}}", cta: "فتح المحفظة" },
      es: { subject: "No pudimos procesar tu retiro de {{amount}} {{currency}}", preheader: "Los fondos volvieron a tu billetera.", body: "Hola {{first_name}},\n\nNo pudimos procesar tu retiro. Los fondos volvieron a tu billetera.\n\nMotivo: {{reason}}", cta: "Abrir billetera" },
    },
    versions: [V(2, 6, "2026-08-11T14:00:00+03:00", "Compliance-approved reason wording", true)],
  },
  {
    id: "kyc_ok",
    name: "KYC approved",
    group: "Compliance",
    trigger: "kyc.approved",
    icon: "check_mark_button",
    sent30d: 6210,
    openRate: 79.1,
    status: "live",
    content: {
      en: { subject: "You're verified, {{first_name}}", preheader: "Deposits and withdrawals are unlocked.", body: "Hi {{first_name}},\n\nGood news: your identity has been verified. Deposits, withdrawals and live trading are now fully unlocked on {{broker_name}}.", cta: "Make your first deposit" },
      ar: { subject: "تم التحقق من هويتك يا {{first_name}}", preheader: "الإيداع والسحب متاحان الآن.", body: "مرحباً {{first_name}}،\n\nأخبار سارة: تم التحقق من هويتك. أصبح الإيداع والسحب والتداول الحقيقي متاحاً بالكامل على {{broker_name}}.", cta: "قم بأول إيداع" },
      es: { subject: "Ya estás verificado, {{first_name}}", preheader: "Depósitos y retiros desbloqueados.", body: "Hola {{first_name}},\n\nBuenas noticias: tu identidad fue verificada. Ya puedes depositar, retirar y operar en real en {{broker_name}}.", cta: "Haz tu primer depósito" },
      pt: { subject: "Você foi verificado, {{first_name}}", preheader: "Depósitos e saques liberados.", body: "Olá {{first_name}},\n\nSua identidade foi verificada. Depósitos, saques e trading ao vivo estão liberados.", cta: "Fazer primeiro depósito" },
    },
    versions: [V(3, 6, "2026-09-05T09:30:00+03:00", "New CTA", true)],
  },
  {
    id: "kyc_rej",
    name: "KYC rejected",
    group: "Compliance",
    trigger: "kyc.rejected",
    icon: "identification_card",
    sent30d: 1840,
    openRate: 83.6,
    status: "live",
    content: {
      en: { subject: "Action needed: we couldn't verify your document", preheader: "Please upload a new document.", body: "Hi {{first_name}},\n\nWe couldn't verify the document you uploaded.\n\nReason: {{reason}}\n\nPlease upload a new document from your client area. Most re-submissions are reviewed within 15 minutes.", cta: "Upload new document" },
      ar: { subject: "إجراء مطلوب: تعذّر التحقق من مستندك", preheader: "يرجى تحميل مستند جديد.", body: "مرحباً {{first_name}}،\n\nتعذّر علينا التحقق من المستند الذي قمت بتحميله.\n\nالسبب: {{reason}}\n\nيرجى تحميل مستند جديد من منطقة العميل. تتم مراجعة معظم الطلبات خلال 15 دقيقة.", cta: "تحميل مستند جديد" },
      es: { subject: "Acción requerida: no pudimos verificar tu documento", preheader: "Sube un documento nuevo.", body: "Hola {{first_name}},\n\nNo pudimos verificar tu documento.\n\nMotivo: {{reason}}\n\nSube uno nuevo desde tu área de cliente.", cta: "Subir documento" },
    },
    versions: [V(4, 6, "2026-09-05T09:40:00+03:00", "15-minute SLA line", true), V(3, 10, "2026-07-20T13:15:00+03:00", "Reason variable")],
  },
  {
    id: "margin_call",
    name: "Margin call",
    group: "Trading",
    trigger: "account.margin_call",
    icon: "warning",
    sent30d: 3120,
    openRate: 88.7,
    status: "live",
    content: {
      en: { subject: "Margin call on account {{account_login}}", preheader: "Margin level {{margin_level}}", body: "Hi {{first_name}},\n\nThe margin level on account {{account_login}} has fallen to {{margin_level}}. If it reaches {{stop_out_level}}, positions will be closed automatically, starting with the largest loss.\n\nConsider depositing funds or reducing your exposure.", cta: "Deposit now" },
      ar: { subject: "نداء هامش على الحساب {{account_login}}", preheader: "مستوى الهامش {{margin_level}}", body: "مرحباً {{first_name}}،\n\nانخفض مستوى الهامش في الحساب {{account_login}} إلى {{margin_level}}. إذا وصل إلى {{stop_out_level}} فسيتم إغلاق الصفقات تلقائياً بدءاً بأكبر خسارة.\n\nننصحك بإيداع أموال أو تقليل تعرضك.", cta: "أودع الآن" },
      es: { subject: "Llamada de margen en la cuenta {{account_login}}", preheader: "Nivel de margen {{margin_level}}", body: "Hola {{first_name}},\n\nEl nivel de margen de la cuenta {{account_login}} bajó a {{margin_level}}. Si llega a {{stop_out_level}}, las posiciones se cerrarán automáticamente.", cta: "Depositar ahora" },
      hi: { subject: "खाता {{account_login}} पर मार्जिन कॉल", preheader: "मार्जिन स्तर {{margin_level}}", body: "नमस्ते {{first_name}},\n\nखाता {{account_login}} का मार्जिन स्तर {{margin_level}} तक गिर गया है। {{stop_out_level}} पर पोज़िशन अपने आप बंद हो जाएँगी।", cta: "अभी जमा करें" },
    },
    versions: [V(6, 19, "2026-08-20T08:15:00+03:00", "Stop-out level variable", true), V(5, 4, "2026-06-02T10:00:00+03:00", "Dealing review")],
  },
  {
    id: "stop_out",
    name: "Stop-out",
    group: "Trading",
    trigger: "account.stop_out",
    icon: "chart_decreasing",
    sent30d: 842,
    openRate: 90.4,
    status: "live",
    content: {
      en: { subject: "Positions closed on account {{account_login}}", preheader: "Stop-out was triggered.", body: "Hi {{first_name}},\n\nStop-out was triggered on account {{account_login}} at {{date}} and some positions were closed to protect your balance. Negative balance protection applies, so you can never lose more than your deposit.", cta: "Review trade history" },
      ar: { subject: "تم إغلاق صفقات على الحساب {{account_login}}", preheader: "تم تفعيل مستوى الإيقاف.", body: "مرحباً {{first_name}}،\n\nتم تفعيل مستوى الإيقاف على الحساب {{account_login}} في {{date}} وأُغلقت بعض الصفقات لحماية رصيدك. تنطبق حماية الرصيد السلبي، فلن تخسر أبداً أكثر من إيداعك.", cta: "مراجعة سجل التداول" },
      es: { subject: "Posiciones cerradas en la cuenta {{account_login}}", preheader: "Se activó el stop-out.", body: "Hola {{first_name}},\n\nSe activó el stop-out en la cuenta {{account_login}} el {{date}}. Aplica la protección de saldo negativo.", cta: "Ver historial" },
    },
    versions: [V(3, 19, "2026-08-20T08:20:00+03:00", "NBP sentence", true)],
  },
  {
    id: "ib_payout",
    name: "IB payout",
    group: "Partners",
    trigger: "partner.commission_paid",
    icon: "handshake",
    sent30d: 1204,
    openRate: 72.8,
    status: "live",
    content: {
      en: { subject: "Your commission of {{commission}} has been paid", preheader: "Partner payout for {{period}}.", body: "Hi {{first_name}},\n\nYour partner commission of {{commission}} for {{period}} has been credited to your wallet.\n\nThank you for growing with {{broker_name}}.", cta: "Open partner dashboard" },
      ar: { subject: "تم دفع عمولتك بقيمة {{commission}}", preheader: "دفعة الشريك عن {{period}}.", body: "مرحباً {{first_name}}،\n\nتمت إضافة عمولة الشريك بقيمة {{commission}} عن {{period}} إلى محفظتك.\n\nشكراً لنموك معنا في {{broker_name}}.", cta: "لوحة الشريك" },
      es: { subject: "Tu comisión de {{commission}} ha sido pagada", preheader: "Pago de socio de {{period}}.", body: "Hola {{first_name}},\n\nTu comisión de {{commission}} de {{period}} se acreditó en tu billetera.", cta: "Panel de socio" },
      pt: { subject: "Sua comissão de {{commission}} foi paga", preheader: "Pagamento de parceiro de {{period}}.", body: "Olá {{first_name}},\n\nSua comissão de {{commission}} referente a {{period}} foi creditada na sua carteira.", cta: "Painel do parceiro" },
    },
    versions: [V(2, 7, "2026-09-01T09:00:00+03:00", "Period variable", true)],
  },
  {
    id: "statement",
    name: "Monthly statement",
    group: "Account",
    trigger: "statement.monthly",
    icon: "receipt",
    sent30d: 41880,
    openRate: 44.2,
    status: "draft",
    content: {
      en: { subject: "Your {{period}} statement for account {{account_login}}", preheader: "Balance, trades and fees for the month.", body: "Hi {{first_name}},\n\nYour monthly statement for account {{account_login}} covering {{period}} is ready. It includes your closing balance, all trades, swaps, commissions and deposits.\n\nThe PDF is attached and also available in your client area.", cta: "Download statement" },
      ar: { subject: "كشف حساب {{period}} للحساب {{account_login}}", preheader: "الرصيد والصفقات والرسوم للشهر.", body: "مرحباً {{first_name}}،\n\nكشف الحساب الشهري للحساب {{account_login}} عن {{period}} جاهز، ويتضمن الرصيد الختامي وجميع الصفقات والعمولات والإيداعات.", cta: "تنزيل الكشف" },
    },
    versions: [V(2, 12, "2026-09-23T18:10:00+03:00", "Draft: attach PDF note"), V(1, 12, "2026-03-01T09:00:00+03:00", "Initial", true)],
  },
];
