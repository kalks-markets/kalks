// Sign up (web apps/crm/app/(auth)/register/page.tsx): Details -> Verify email -> Done, the same fields, texts and
// checks. The session from the code step is kept until "Open client area".
import 'package:flutter/cupertino.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/api/api_providers.dart';
import '../../core/auth/auth_api.dart';
import '../../core/auth/auth_controller.dart';
import '../../core/notifications/notifications.dart';
import '../../i18n/i18n.dart';
import '../../ui/ui.dart';
import 'auth_widgets.dart';

class RegisterScreen extends ConsumerStatefulWidget {
  const RegisterScreen({super.key, this.referral});

  /// ?ref= of a partner link.
  final String? referral;

  @override
  ConsumerState<RegisterScreen> createState() => _RegisterScreenState();
}

class _RegisterScreenState extends ConsumerState<RegisterScreen> {
  int _step = 0;
  final _first = TextEditingController();
  final _last = TextEditingController();
  final _email = TextEditingController();
  final _phone = TextEditingController();
  final _referral = TextEditingController();
  final _password = TextEditingController();
  final _countryName = TextEditingController(text: kCountries.first.$2);
  final _dobText = TextEditingController();
  String _country = 'in';
  DateTime? _dob;
  bool _agree = false;
  bool _marketing = true;
  bool _show = false;
  bool _loading = false;
  ApiException? _err;
  OtpChallenge? _otp;
  int _otpKey = 0;
  String _code = '';
  SignedIn? _result;

  @override
  void initState() {
    super.initState();
    final ref = widget.referral;
    if (ref != null && ref.isNotEmpty) _referral.text = ref.length > 24 ? ref.substring(0, 24) : ref;
    _password.addListener(() => setState(() {}));
  }

  @override
  void dispose() {
    for (final c in [_first, _last, _email, _phone, _referral, _password, _countryName, _dobText]) {
      c.dispose();
    }
    super.dispose();
  }

  String get _dial => kCountries.firstWhere((c) => c.$1 == _country, orElse: () => kCountries.first).$3;
  String _iso(DateTime d) => '${d.year}-${d.month.toString().padLeft(2, '0')}-${d.day.toString().padLeft(2, '0')}';

  AuthApi get _auth => ref.read(authApiProvider);

  Future<void> _submit() async {
    if (_loading || !_agree) return;
    FocusScope.of(context).unfocus();
    setState(() {
      _err = null;
      _loading = true;
    });
    try {
      final c = await _auth.register(
        RegisterForm(
          firstName: _first.text.trim(),
          lastName: _last.text.trim(),
          email: _email.text.trim(),
          password: _password.text,
          country: _country,
          phoneDial: _dial,
          phone: _phone.text.trim(),
          dateOfBirth: _dob == null ? '' : _iso(_dob!),
          referralCode: _referral.text.trim(),
          acceptTerms: _agree,
          marketingConsent: _marketing,
        ),
      );
      if (!mounted) return;
      setState(() {
        _otp = c;
        _otpKey++;
        _step = 1;
        _loading = false;
      });
    } on ApiException catch (e) {
      if (mounted) {
        setState(() {
          _err = e;
          _loading = false;
        });
      }
    }
  }

  Future<void> _verify([String? value]) async {
    final otp = _otp;
    final code = value ?? _code;
    if (otp == null || code.length != 6 || _loading) return;
    setState(() {
      _err = null;
      _loading = true;
    });
    try {
      final r = await _auth.verifyEmail(otp.challenge, code);
      if (!mounted) return;
      KHaptics.success();
      setState(() {
        _result = r is SignedIn ? r : null;
        _step = 2;
        _loading = false;
      });
    } on ApiException catch (e) {
      if (!mounted) return;
      setState(() {
        _err = e;
        _loading = false;
        _code = '';
        _otpKey++;
      });
    }
  }

  Future<int?> _resend() async {
    final otp = _otp;
    if (otp == null) return null;
    try {
      final next = await _auth.resend(otp.challenge);
      if (!mounted) return null;
      setState(() {
        _err = null;
        _otp = next;
        _otpKey++;
      });
      ref
          .read(notificationsProvider.notifier)
          .toast(
            NotificationKind.success,
            context.t('auth.toast.newCodeSent'),
            description: context.t('auth.toast.checkEmail', {'email': next.emailMasked}),
            keep: false,
          );
      return null;
    } on ApiException catch (e) {
      if (mounted) setState(() => _err = e);
      return e.retryAfter;
    }
  }

  Future<void> _pickDob() async {
    final max = maxDob();
    var picked = _dob ?? DateTime(max.year - 7, max.month, max.day);
    final k = context.k;
    await showKSheet<void>(
      context,
      title: context.t('auth.field.dateOfBirth'),
      builder: (ctx) => Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          SizedBox(
            height: 216,
            child: CupertinoTheme(
              data: CupertinoThemeData(
                brightness: k.brightness,
                textTheme: CupertinoTextThemeData(dateTimePickerTextStyle: ctx.text.body.copyWith(fontSize: 20)),
              ),
              child: CupertinoDatePicker(
                mode: CupertinoDatePickerMode.date,
                initialDateTime: picked,
                maximumDate: max,
                minimumDate: DateTime(1900),
                onDateTimeChanged: (d) => picked = d,
              ),
            ),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(20, 8, 20, 12),
            child: KButton(
              label: ctx.t('common.done'),
              size: KButtonSize.lg,
              expand: true,
              onPressed: () {
                setState(() {
                  _dob = picked;
                  _dobText.text = _iso(picked);
                });
                Navigator.of(ctx).pop();
              },
            ),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final rtl = Directionality.of(context) == TextDirection.rtl;
    String? fieldErr(String f) => _err?.field == f ? localizeError(_err!, t) : null;
    final steps = KStepIndicator(steps: [t('auth.register.stepDetails'), t('auth.register.stepVerify'), t('auth.register.stepDone')], current: _step);

    if (_step == 2) {
      return AuthScaffold(
        child: Column(
          key: const ValueKey('done'),
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            steps,
            const SizedBox(height: 28),
            const Align(alignment: AlignmentDirectional.centerStart, child: KIllustration(KIllustrationName.welcome, width: 150, maxHeight: 176)),
            const SizedBox(height: 22),
            AuthTitle(title: t('auth.register.welcome', {'name': _first.text.trim()}), subtitle: Text(t('auth.register.ready'))),
            const SizedBox(height: 28),
            KButton(
              label: t('auth.register.openClientArea'),
              trailingIcon: rtl ? LucideIcons.arrowLeft : LucideIcons.arrowRight,
              size: KButtonSize.lg,
              expand: true,
              onPressed: () async {
                final r = _result;
                if (r != null) {
                  await ref.read(authProvider.notifier).completeSignIn(r);
                } else if (context.mounted) {
                  context.go('/login');
                }
              },
            ),
          ],
        ),
      );
    }

    if (_step == 1) {
      final otp = _otp!;
      return AuthScaffold(
        child: Column(
          key: const ValueKey('verify'),
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            steps,
            const SizedBox(height: 28),
            AuthTitle(
              icon: LucideIcons.bellRing,
              title: t('auth.register.checkInbox'),
              subtitle: KRichText(t('auth.register.enterCode', {'email': otp.emailMasked}), tags: const {'b': KTag()}),
            ),
            const SizedBox(height: 28),
            if (_err != null) ...[KFormError(localizeError(_err!, t)), const SizedBox(height: 14)],
            KOtpField(
              key: ValueKey(_otpKey),
              onChanged: (v) => setState(() => _code = v),
              onCompleted: _verify,
              semanticLabel: t('auth.otp.digit', {'n': 1, 'total': 6}),
            ),
            KDevCodeHint(otp.devCode),
            const SizedBox(height: 22),
            KButton(
              label: _loading ? t('auth.otp.verifying') : t('auth.register.verifyEmail'),
              size: KButtonSize.lg,
              expand: true,
              loading: _loading,
              onPressed: _code.length == 6 ? _verify : null,
            ),
            const SizedBox(height: 14),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                Text(t('auth.otp.didntGetIt'), style: context.text.footnote.copyWith(color: k.fg3)),
                KResendLink(key: ValueKey(otp.challenge), seconds: otp.resendIn, onResend: _resend),
              ],
            ),
          ],
        ),
      );
    }

    return AuthScaffold(
      child: AutofillGroup(
        child: Column(
          key: const ValueKey('details'),
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            steps,
            const SizedBox(height: 28),
            AuthTitle(title: t('auth.register.title'), subtitle: Text(t('auth.register.subtitle'))),
            const SizedBox(height: 24),
            if (_err != null && _err!.field == null) ...[KFormError(localizeError(_err!, t)), const SizedBox(height: 14)],
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Expanded(
                  child: KTextField(
                    label: t('auth.field.firstName'),
                    placeholder: 'Arjun',
                    controller: _first,
                    leading: LucideIcons.userRound,
                    textCapitalization: TextCapitalization.words,
                    autofillHints: const [AutofillHints.givenName],
                    textInputAction: TextInputAction.next,
                    error: fieldErr('first_name'),
                  ),
                ),
                const SizedBox(width: 12),
                Expanded(
                  child: KTextField(
                    label: t('auth.field.lastName'),
                    placeholder: 'Mehta',
                    controller: _last,
                    textCapitalization: TextCapitalization.words,
                    autofillHints: const [AutofillHints.familyName],
                    textInputAction: TextInputAction.next,
                    error: fieldErr('last_name'),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 14),
            KTextField(
              label: t('auth.field.email'),
              placeholder: t('auth.placeholder.email'),
              controller: _email,
              leading: LucideIcons.mail,
              keyboardType: TextInputType.emailAddress,
              autofillHints: const [AutofillHints.email],
              textInputAction: TextInputAction.next,
              ltr: true,
              error: fieldErr('email'),
            ),
            if (_err?.code == 'email_taken')
              Padding(
                padding: const EdgeInsets.only(top: 6),
                child: KRichText(
                  t('auth.register.emailTaken'),
                  style: context.text.footnote.copyWith(color: k.fg3),
                  tags: {'signin': KTag.link(() => context.go('/login')), 'reset': KTag.link(() => context.push('/forgot'))},
                ),
              ),
            const SizedBox(height: 14),
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Expanded(
                  flex: 10,
                  child: KTextField(
                    label: t('auth.field.country'),
                    controller: _countryName,
                    readOnly: true,
                    onTap: () async {
                      final c = await showCountrySheet(context, _country);
                      if (c != null) {
                        setState(() {
                          _country = c;
                          _countryName.text = kCountries.firstWhere((x) => x.$1 == c).$2;
                        });
                      }
                    },
                    trailing: Padding(padding: const EdgeInsets.only(right: 8), child: KFlag(_country)),
                    error: fieldErr('country'),
                  ),
                ),
                const SizedBox(width: 12),
                Expanded(
                  flex: 12,
                  child: KTextField(
                    label: t('auth.field.phone'),
                    placeholder: '98201 44721',
                    leadingText: _dial,
                    controller: _phone,
                    keyboardType: TextInputType.phone,
                    autofillHints: const [AutofillHints.telephoneNumberNational],
                    ltr: true,
                    error: fieldErr('phone'),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 14),
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Expanded(
                  child: KTextField(
                    label: t('auth.field.dateOfBirth'),
                    controller: _dobText,
                    placeholder: 'YYYY-MM-DD',
                    leading: LucideIcons.calendarDays,
                    readOnly: true,
                    ltr: true,
                    onTap: _pickDob,
                    error: fieldErr('date_of_birth'),
                  ),
                ),
                const SizedBox(width: 12),
                Expanded(
                  child: KTextField(
                    label: t('auth.field.referralCode'),
                    hint: Text(t('auth.field.optionalHint'), style: context.text.caption.copyWith(color: k.fg3)),
                    placeholder: 'ABC1234',
                    controller: _referral,
                    leading: LucideIcons.gift,
                    textCapitalization: TextCapitalization.characters,
                    error: fieldErr('referral_code'),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 14),
            KTextField(
              label: t('auth.field.password'),
              placeholder: t('auth.placeholder.createPassword'),
              controller: _password,
              leading: LucideIcons.lock,
              obscure: !_show,
              autofillHints: const [AutofillHints.newPassword],
              error: fieldErr('password'),
              trailing: KIconButton(
                icon: _show ? LucideIcons.eyeOff : LucideIcons.eye,
                size: 36,
                semanticLabel: t('auth.togglePassword'),
                onPressed: () => setState(() => _show = !_show),
              ),
            ),
            PasswordStrength(_password.text),
            const SizedBox(height: 10),
            KCheckRow(
              value: _agree,
              onChanged: (v) => setState(() => _agree = v),
              child: KRichText(t('auth.register.terms'), tags: const {'agreement': KTag(), 'risk': KTag(), 'privacy': KTag()}),
            ),
            if (fieldErr('accept_terms') != null) Text(fieldErr('accept_terms')!, style: context.text.footnote.copyWith(color: k.down)),
            KCheckRow(
              value: _marketing,
              onChanged: (v) => setState(() => _marketing = v),
              child: Text(t.dyn('auth.register.marketing', fallback: 'Email me trading tips, product news and offers. Unsubscribe any time.')),
            ),
            const SizedBox(height: 16),
            KButton(
              label: _loading ? t('auth.register.creating') : t('auth.register.create'),
              trailingIcon: _loading ? null : (rtl ? LucideIcons.arrowLeft : LucideIcons.arrowRight),
              size: KButtonSize.lg,
              expand: true,
              loading: _loading,
              onPressed: _agree ? _submit : null,
            ),
            const SizedBox(height: 18),
            Center(
              child: KRichText(
                t('auth.register.haveAccount'),
                textAlign: TextAlign.center,
                style: context.text.callout.copyWith(color: k.fg3),
                tags: {
                  'link': KTag.link(
                    () => context.go('/login'),
                    style: TextStyle(color: k.fg, fontWeight: FontWeight.w600),
                  ),
                },
              ),
            ),
          ],
        ),
      ),
    );
  }
}
