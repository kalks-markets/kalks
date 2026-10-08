// Small pieces of the Accounts pages, ported from the web's trading components:
//   KindBadge, StatusBadge, FlavorChip, DefaultStar            (ui.tsx, archive.tsx, extras.tsx)
//   ProductChip, ProductSection, byProduct                     the OPTIONS tag, CFD / Options account sections
//   LoginCopy                                                  "#10042817" + CopyButton
//   SecretField, PasswordInput, PasswordRules                  (ui.tsx SecretField / PasswordRules, accounts/security.tsx)
//   TradeSymbolAvatar, OptionTag, symbolLabel, optionLabel     (instrument.tsx)
//   serverTimeText, fmtDate, fmtContracts                      (api.ts serverTime / fmtDate, instrument.tsx)
//   KGauge, StatTile, StepLine, ResultSteps, KTextArea, LeveragePills, StepsList
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:intl/intl.dart' show DateFormat;
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../core/api/api_providers.dart';
import '../../../core/format/format.dart';
import '../../../core/models/account.dart';
import '../../../core/models/trading.dart';
import '../../../core/notifications/notifications.dart';
import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import '../accounts_data.dart';

/* ------------------------------------------------------------------ feedback */

/// The web's `toast.success / error(title, {description})`: a banner kept in the bell.
void accountToast(BuildContext context, NotificationKind kind, String title, {String? description}) {
  if (!context.mounted) return;
  ProviderScope.containerOf(context, listen: false).read(notificationsProvider.notifier).toast(kind, title, description: description);
}

/// The web's errorToast(title, e): the error's message (translated where the app knows its code) as description.
void accountErrorToast(BuildContext context, String title, Object e) {
  if (!context.mounted) return;
  accountToast(context, NotificationKind.error, title, description: errorText(e, context.t));
}

/* ------------------------------------------------------------------ badges */

/// LIVE (ember) / DEMO (gold) / PROP (neutral) (web KindBadge).
class KindBadge extends StatelessWidget {
  const KindBadge({super.key, required this.account, this.small = true});
  final EngineAccount account;
  final bool small;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final a = account;
    if (a.prop) return KChip(label: t('accounts.badge.prop'), small: small);
    return a.live
        ? KChip(label: t('accounts.badge.live'), tone: KChipTone.ember, small: small)
        : KChip(label: t('accounts.badge.demo'), tone: KChipTone.gold, small: small);
  }
}

/// The status chip, hidden while the account is active (web StatusBadge).
class StatusBadge extends StatelessWidget {
  const StatusBadge({super.key, required this.account});
  final EngineAccount account;

  @override
  Widget build(BuildContext context) {
    if (account.status == 'active') return const SizedBox.shrink();
    return StatusChip(status: account.status);
  }
}

/// The status chip in any state (Account information, Account details).
class StatusChip extends StatelessWidget {
  const StatusChip({super.key, required this.status});
  final String status;

  @override
  Widget build(BuildContext context) => KChip(
    label: context.t.dyn('accounts.status.$status', fallback: kAccountStatus[status]?.label ?? status),
    tone: statusTone(status),
    small: true,
  );
}

/// COPY / PAMM / MAM (web FlavorChip).
class FlavorChip extends StatelessWidget {
  const FlavorChip({super.key, required this.account});
  final EngineAccount account;

  @override
  Widget build(BuildContext context) {
    final f = accountFlavor(account);
    if (f == null) return const SizedBox.shrink();
    return KChip(label: context.t('accounts.badge.$f'), tone: f == 'copy' ? KChipTone.info : KChipTone.neutral, small: true);
  }
}

/// OPTIONS on an Options account (a CFD account carries no tag).
class ProductChip extends StatelessWidget {
  const ProductChip({super.key, required this.account});
  final EngineAccount account;

  @override
  Widget build(BuildContext context) {
    if (!account.isOptions) return const SizedBox.shrink();
    return KChip(label: context.t('accounts.product.chipOptions'), tone: KChipTone.info, small: true);
  }
}

/// Accounts by product, CFD first: `[(cfd, [...]), (options, [...])]` without empty groups (order kept).
List<(String, List<EngineAccount>)> byProduct(List<EngineAccount> accounts) => [
  for (final p in const ['cfd', 'options'])
    if (accounts.any((a) => a.product == p)) (p, accounts.where((a) => a.product == p).toList()),
];

/// A product section's title ("CFD accounts" / "Options accounts") over its account rows.
class ProductSection extends StatelessWidget {
  const ProductSection({super.key, required this.product, this.padding = const EdgeInsets.only(bottom: 10)});
  final String product;
  final EdgeInsetsGeometry padding;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final options = product == 'options';
    return Padding(
      padding: padding,
      child: Row(
        children: [
          Icon(options ? LucideIcons.layers2 : LucideIcons.candlestickChart, size: 15, color: k.fg3),
          const SizedBox(width: 8),
          Expanded(
            child: Text(
              options ? t('accounts.product.groupOptions') : t('accounts.product.groupCfd'),
              style: context.text.label.copyWith(color: k.fg2, fontWeight: FontWeight.w600),
            ),
          ),
        ],
      ),
    );
  }
}

/// The starred default account (web DefaultStar).
class DefaultStar extends StatelessWidget {
  const DefaultStar({super.key, required this.account});
  final EngineAccount account;

  @override
  Widget build(BuildContext context) {
    if (!account.isDefault) return const SizedBox.shrink();
    return KChip(label: context.t('accounts.default.badge'), tone: KChipTone.gold, icon: LucideIcons.star, small: true);
  }
}

/// "#10042817" with the copy button (web CopyButton next to the login).
class LoginCopy extends StatelessWidget {
  const LoginCopy({super.key, required this.login, this.color, this.size = 13});
  final int login;
  final Color? color;
  final double size;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(
          '#$login',
          textDirection: TextDirection.ltr,
          style: context.text.mono(size, color: color ?? k.fg2),
        ),
        KPressable(
          minSize: 32,
          semanticLabel: context.t('common.copy'),
          onTap: () => kCopy(context, '$login'),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 5),
            child: Icon(LucideIcons.copy, size: 13, color: k.fg3),
          ),
        ),
      ],
    );
  }
}

/* ------------------------------------------------------------------ credentials and passwords */

/// A read-only credential with copy (and Show / Hide for secrets) (web SecretField).
class SecretField extends StatefulWidget {
  const SecretField({super.key, required this.label, required this.value, this.hint, this.secret = false});
  final String label;
  final String value;
  final String? hint;
  final bool secret;

  @override
  State<SecretField> createState() => _SecretFieldState();
}

class _SecretFieldState extends State<SecretField> {
  late bool _show = !widget.secret;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        Padding(
          padding: const EdgeInsets.only(bottom: 6),
          child: Row(
            children: [
              Expanded(
                child: Text(widget.label, style: context.text.label.copyWith(color: k.fg2)),
              ),
              if (widget.hint != null) Text(widget.hint!, style: context.text.footnote.copyWith(color: k.fg3)),
            ],
          ),
        ),
        Container(
          height: KSize.field,
          padding: const EdgeInsetsDirectional.only(start: 14, end: 4),
          decoration: BoxDecoration(
            color: k.surface2,
            borderRadius: BorderRadius.circular(14),
            border: Border.all(color: k.line),
          ),
          child: Row(
            children: [
              Expanded(
                child: Text(
                  _show ? widget.value : '•' * math.min(12, widget.value.length),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  textDirection: TextDirection.ltr,
                  textAlign: Directionality.of(context) == TextDirection.rtl ? TextAlign.right : TextAlign.left,
                  style: context.text.mono(14, color: k.fg),
                ),
              ),
              if (widget.secret)
                KPressable(
                  minSize: 36,
                  onTap: () => setState(() => _show = !_show),
                  child: Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 6),
                    child: Text(_show ? t('accounts.secret.hide') : t('accounts.secret.show'), style: context.text.caption.copyWith(color: k.fg3)),
                  ),
                ),
              KPressable(
                minSize: 40,
                semanticLabel: t('common.copy'),
                onTap: () => kCopy(context, widget.value),
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 8),
                  child: Icon(LucideIcons.copy, size: 15, color: k.fg3),
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

/// A password field with show / hide and, with `generate`, a Generate button (web PasswordInput).
class PasswordInput extends StatefulWidget {
  const PasswordInput({super.key, required this.value, required this.onChanged, this.label, this.placeholder, this.generate = false, this.error});
  final String value;
  final ValueChanged<String> onChanged;
  final String? label;
  final String? placeholder;
  final bool generate;
  final String? error;

  @override
  State<PasswordInput> createState() => _PasswordInputState();
}

class _PasswordInputState extends State<PasswordInput> {
  late final TextEditingController _c = TextEditingController(text: widget.value);
  bool _show = false;

  @override
  void didUpdateWidget(PasswordInput old) {
    super.didUpdateWidget(old);
    if (widget.value != _c.text) _c.text = widget.value;
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    return KTextField(
      label: widget.label,
      controller: _c,
      obscure: !_show,
      ltr: true,
      error: widget.error,
      placeholder: widget.placeholder ?? t('accountDetail.pwInput.placeholder'),
      autofillHints: const [AutofillHints.newPassword],
      onChanged: widget.onChanged,
      trailing: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (widget.generate)
            KPressable(
              minSize: 36,
              onTap: () {
                final p = generatePassword();
                _c.text = p;
                setState(() => _show = true);
                widget.onChanged(p);
                accountToast(context, NotificationKind.success, t('accountDetail.pwInput.generated'));
              },
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: 6),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Icon(LucideIcons.refreshCw, size: 12, color: k.ember),
                    const SizedBox(width: 4),
                    Text(
                      t('accountDetail.pwInput.generate'),
                      style: context.text.caption.copyWith(color: k.ember, fontWeight: FontWeight.w600),
                    ),
                  ],
                ),
              ),
            ),
          KPressable(
            minSize: 40,
            semanticLabel: _show ? t('accountDetail.pwInput.hide') : t('accountDetail.pwInput.show'),
            onTap: () => setState(() => _show = !_show),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 8),
              child: Icon(_show ? LucideIcons.eyeOff : LucideIcons.eye, size: 16, color: k.fg3),
            ),
          ),
        ],
      ),
    );
  }
}

/// The trading-password rules with live ticks (web PasswordRules).
class PasswordRules extends StatelessWidget {
  const PasswordRules({super.key, required this.password});
  final String password;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        for (final r in kLivePasswordRules)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 3),
            child: Builder(
              builder: (context) {
                final ok = livePasswordRule(r.$1, password);
                return Row(
                  children: [
                    Container(
                      width: 16,
                      height: 16,
                      decoration: BoxDecoration(
                        shape: BoxShape.circle,
                        color: ok ? k.upSoft : null,
                        border: Border.all(color: ok ? k.up.withValues(alpha: 0.4) : k.line),
                      ),
                      child: ok ? Icon(LucideIcons.check, size: 10, color: k.up) : null,
                    ),
                    const SizedBox(width: 8),
                    Text(context.t(r.$2), style: context.text.footnote.copyWith(color: ok ? k.up : k.fg3)),
                  ],
                );
              },
            ),
          ),
      ],
    );
  }
}

/* ------------------------------------------------------------------ symbols */

const Map<String, String> _ccyFlag = {
  'EUR': 'eu',
  'GBP': 'gb',
  'USD': 'us',
  'JPY': 'jp',
  'AUD': 'au',
  'CAD': 'ca',
  'CHF': 'ch',
  'NZD': 'nz',
  'INR': 'in',
  'SGD': 'sg',
  'HKD': 'hk',
  'ZAR': 'za',
  'MXN': 'mx',
  'NOK': 'no',
  'SEK': 'se',
  'DKK': 'dk',
  'PLN': 'pl',
  'TRY': 'tr',
  'CNH': 'cn',
  'CNY': 'cn',
  'HUF': 'hu',
  'CZK': 'cz',
};

const Set<String> _coins = {'BTC', 'ETH', 'SOL', 'XRP', 'LTC', 'ADA', 'DOGE', 'DOT', 'BNB', 'AVAX', 'LINK', 'MATIC', 'TRX'};

/// Any engine symbol's avatar: two flags for a currency pair, the coin for crypto, initials otherwise; an option
/// shows its underlying with a small C / P mark (web TradeSymbolAvatar).
class TradeSymbolAvatar extends StatelessWidget {
  const TradeSymbolAvatar({super.key, required this.symbol, this.size = 28});
  final String symbol;
  final double size;

  @override
  Widget build(BuildContext context) {
    final p = parseSeries(symbol);
    if (p == null) return _base(context, symbol);
    final badge = math.max(10.0, (size * 0.5).roundToDouble());
    final k = context.k;
    return Stack(
      clipBehavior: Clip.none,
      children: [
        _base(context, p.underlying),
        PositionedDirectional(
          bottom: -3,
          end: -3,
          child: Container(
            width: badge,
            height: badge,
            alignment: Alignment.center,
            decoration: BoxDecoration(
              color: p.right == 'call' ? k.up : k.down,
              shape: BoxShape.circle,
              border: Border.all(color: k.surface, width: 1.5),
            ),
            child: Text(
              p.right == 'call' ? 'C' : 'P',
              style: context.text.micro.copyWith(color: Colors.white, fontSize: badge * 0.55, height: 1, fontWeight: FontWeight.w800),
            ),
          ),
        ),
      ],
    );
  }

  Widget _base(BuildContext context, String s) {
    final up = s.toUpperCase();
    if (up.length == 6 && _ccyFlag[up.substring(0, 3)] != null && _ccyFlag[up.substring(3)] != null) {
      return SizedBox(
        width: size * 1.45,
        height: size,
        child: Stack(
          children: [
            PositionedDirectional(start: 0, top: 0, child: KFlag(_ccyFlag[up.substring(0, 3)]!, size: size)),
            PositionedDirectional(end: 0, top: 0, child: KFlag(_ccyFlag[up.substring(3)]!, size: size)),
          ],
        ),
      );
    }
    for (final c in _coins) {
      if (up.startsWith(c) && (up.length == c.length || up.endsWith('USD') || up.endsWith('USDT'))) return KCoinIcon(c, size: size);
    }
    final k = context.k;
    final initials = up.replaceAll(RegExp(r'[^A-Z0-9]'), '');
    return Container(
      width: size,
      height: size,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: k.surface3,
        shape: BoxShape.circle,
        border: Border.all(color: k.line),
      ),
      child: Text(
        initials.isEmpty ? '?' : initials.substring(0, math.min(2, initials.length)),
        style: context.text.micro.copyWith(color: k.fg2, fontSize: math.max(8, size * 0.36), fontWeight: FontWeight.w700),
      ),
    );
  }
}

/// The small "Option" tag next to a trade's name (web OptionTag).
class OptionTag extends StatelessWidget {
  const OptionTag({super.key});

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Container(
      height: 18,
      padding: const EdgeInsets.symmetric(horizontal: 6),
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: k.goldSoft,
        borderRadius: BorderRadius.circular(9),
        border: Border.all(color: k.gold.withValues(alpha: 0.3)),
      ),
      child: Text(context.t('accounts.opt.tag').toUpperCase(), style: context.text.micro.copyWith(color: k.gold, fontSize: 9.5, letterSpacing: 0.4)),
    );
  }
}

/// "2 Oct" (with the year when it isn't this year), in the reader's language (web expiryLabel).
String expiryLabel(String day, String locale) {
  final d = DateTime.tryParse('${day.length >= 10 ? day.substring(0, 10) : day}T00:00:00Z');
  if (d == null) return day;
  final tag = intlLocale(locale);
  final f = d.year == DateTime.now().toUtc().year ? DateFormat.MMMd(tag) : DateFormat.yMMMd(tag);
  return latinDigits(f.format(d));
}

/// The option's terms from its `option` object, falling back to the series code (web optionTerms).
({String series, String underlying, String right, String strikeLabel, String expiry})? optionTerms(String symbol, Map<String, dynamic>? o) {
  final p = parseSeries(o?['series'] as String?) ?? parseSeries(symbol);
  final underlying = '${o?['underlying'] ?? p?.underlying ?? ''}'.toUpperCase();
  final r = '${o?['right'] ?? ''}'.toLowerCase();
  final right = r == 'call' || r == 'c' ? 'call' : (r == 'put' || r == 'p' ? 'put' : p?.right);
  final strike = o?['strike'] is num ? (o!['strike'] as num).toDouble() : p?.strike;
  final exp = '${o?['expiry'] ?? ''}';
  final expiry = exp.length >= 10 ? exp.substring(0, 10) : p?.expiry;
  if (underlying.isEmpty || right == null || strike == null || expiry == null) return null;
  final label = p != null && (p.strike - strike).abs() < 1e-9 ? p.strikeLabel : '${num.parse(strike.toStringAsFixed(8))}';
  return (series: '${o?['series'] ?? p?.series ?? symbol}', underlying: underlying, right: right, strikeLabel: label, expiry: expiry);
}

/// "EURUSD 1.1000 Call · 2 Oct" (web optionLabel).
String optionLabel(T t, ({String series, String underlying, String right, String strikeLabel, String expiry}) o) => t('accounts.opt.label', {
  'underlying': o.underlying,
  'strike': o.strikeLabel,
  'right': t(o.right == 'call' ? 'accounts.opt.call' : 'accounts.opt.put'),
  'date': expiryLabel(o.expiry, t.locale),
});

/// The readable name of an engine symbol: the option label for a series code, else the symbol (web symbolLabel).
String symbolLabel(T t, String symbol, Map<String, dynamic>? option) {
  final o = option != null || parseSeries(symbol) != null ? optionTerms(symbol, option) : null;
  return o == null ? symbol : optionLabel(t, o);
}

/// "3" / "2.5" contracts (web fmtContracts).
String fmtContracts(num v) {
  final s = v.abs().toStringAsFixed(2);
  return s.contains('.') ? s.replaceAll(RegExp(r'0+$'), '').replaceAll(RegExp(r'\.$'), '') : s;
}

/// "24 Sep 2026, 14:03" in trading server time (GMT+3 / GMT+2) (web serverTime).
String serverTimeText(T t, DateTime? d, {bool withYear = true}) {
  if (d == null) return '—';
  final tag = intlLocale(t.locale);
  final f = withYear ? DateFormat.yMMMd(tag).add_Hm() : DateFormat.MMMd(tag).add_Hm();
  return latinDigits(f.format(toTradingServerTime(d)));
}

/// "24 Sep 2026" in the reader's language (web fmtDate).
String fmtDate(T t, DateTime? d) => d == null ? '—' : LocaleFormat(t.locale).date(d);

/* ------------------------------------------------------------------ small layout pieces */

/// The margin-level ring (web Gauge): a 0.78 arc in the brand colour around the figure.
class KGauge extends StatelessWidget {
  const KGauge({super.key, required this.value, required this.max, required this.display, this.label, this.size = 180});
  final double value, max;
  final String display;
  final String? label;
  final double size;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final stroke = size * 0.075;
    return SizedBox(
      width: size,
      height: size,
      child: Stack(
        alignment: Alignment.center,
        children: [
          CustomPaint(
            size: Size.square(size),
            painter: _GaugePainter(
              pct: max <= 0 ? 0 : (value / max).clamp(0.0, 1.0),
              stroke: stroke,
              track: k.surface3,
              from: mixOklab(k.ember, Colors.white, 0.55),
              to: k.ember,
            ),
          ),
          Positioned.fill(
            child: Padding(
              padding: EdgeInsets.all(stroke + 18),
              child: DecoratedBox(
                decoration: BoxDecoration(color: k.surface, shape: BoxShape.circle, boxShadow: k.shadowCard),
              ),
            ),
          ),
          Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(
                display,
                textDirection: TextDirection.ltr,
                style: context.text.moneyL.copyWith(fontSize: size * 0.15, fontWeight: FontWeight.w700),
              ),
              if (label != null) ...[const SizedBox(height: 6), Text(label!, style: context.text.footnote.copyWith(color: k.fg2))],
            ],
          ),
        ],
      ),
    );
  }
}

class _GaugePainter extends CustomPainter {
  _GaugePainter({required this.pct, required this.stroke, required this.track, required this.from, required this.to});
  final double pct, stroke;
  final Color track, from, to;

  @override
  void paint(Canvas canvas, Size size) {
    const arc = 0.78;
    final r = (size.width - stroke) / 2 - 8;
    final rect = Rect.fromCircle(center: size.center(Offset.zero), radius: r);
    // the web rotates the circle by 129.6° so the 0.78 arc opens at the bottom
    const start = 129.6 * math.pi / 180;
    const sweep = 2 * math.pi * arc;
    Paint p(Color c) => Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = stroke
      ..strokeCap = StrokeCap.round
      ..color = c;
    canvas.drawArc(rect, start, sweep, false, p(track));
    if (pct > 0) {
      final fill = p(to)..shader = LinearGradient(colors: [from, to], begin: Alignment.topLeft, end: Alignment.bottomRight).createShader(rect);
      canvas.drawArc(rect, start, sweep * pct, false, fill);
    }
  }

  @override
  bool shouldRepaint(_GaugePainter old) => old.pct != pct || old.track != track || old.to != to;
}

/// A label / figure tile (web StatTile / k-row).
class StatTile extends StatelessWidget {
  const StatTile({super.key, required this.label, required this.child, this.tone});
  final String label;
  final Widget child;
  final Color? tone;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 11),
      decoration: BoxDecoration(
        color: k.surface2,
        borderRadius: BorderRadius.circular(14),
        border: Border.all(color: k.line),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            label,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: context.text.footnote.copyWith(color: k.fg3, fontSize: 12),
          ),
          const SizedBox(height: 4),
          DefaultTextStyle.merge(
            style: context.text.figure.copyWith(color: tone ?? k.fg),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            child: FittedBox(fit: BoxFit.scaleDown, alignment: AlignmentDirectional.centerStart, child: child),
          ),
        ],
      ),
    );
  }
}

/// Tiles two per row (the web's grid-cols-2 of stat tiles).
class TileGrid extends StatelessWidget {
  const TileGrid({super.key, required this.children, this.spacing = 8});
  final List<Widget> children;
  final double spacing;

  @override
  Widget build(BuildContext context) => Column(
    mainAxisSize: MainAxisSize.min,
    children: [
      for (var i = 0; i < children.length; i += 2) ...[
        if (i > 0) SizedBox(height: spacing),
        Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Expanded(child: children[i]),
            SizedBox(width: spacing),
            Expanded(child: i + 1 < children.length ? children[i + 1] : const SizedBox.shrink()),
          ],
        ),
      ],
    ],
  );
}

/// A numbered line of "We'll do this in one go" (web StepLine).
class StepLine extends StatelessWidget {
  const StepLine({super.key, required this.n, required this.text});
  final int n;
  final String text;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Container(
            width: 20,
            height: 20,
            alignment: Alignment.center,
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              color: k.surface2,
              border: Border.all(color: k.line),
            ),
            child: Text(
              '$n',
              style: context.text.micro.copyWith(color: k.fg2, fontFeatures: kTabular),
            ),
          ),
          const SizedBox(width: 10),
          Expanded(
            child: Text(text, style: context.text.callout.copyWith(color: k.fg)),
          ),
        ],
      ),
    );
  }
}

/// A boxed list (web k-row p-4): optional caps title, then children.
class RowBox extends StatelessWidget {
  const RowBox({super.key, required this.children, this.title, this.padding = const EdgeInsets.all(14)});
  final List<Widget> children;
  final String? title;
  final EdgeInsetsGeometry padding;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Container(
      padding: padding,
      decoration: BoxDecoration(
        color: k.surface2,
        borderRadius: BorderRadius.circular(16),
        border: Border.all(color: k.line),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          if (title != null) ...[
            Text(
              title!.toUpperCase(),
              style: context.text.caption.copyWith(color: k.fg3, letterSpacing: 0.5, fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 8),
          ],
          ...children,
        ],
      ),
    );
  }
}

/// The steps an archive / closure run did, each with a tick or a cross (web result list).
class ResultSteps extends StatelessWidget {
  const ResultSteps({super.key, required this.ok, required this.title, required this.steps, this.text, this.showState = true});
  final bool ok;
  final String title;
  final String? text;
  final List<RunStep> steps;

  /// " · Done" / " · Failed" after each step (archive) or not (closure).
  final bool showState;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    return RowBox(
      children: [
        Text(title, style: context.text.headline.copyWith(fontSize: 14, color: ok ? k.up : k.warn)),
        if (text != null) ...[const SizedBox(height: 4), Text(text!, style: context.text.footnote.copyWith(color: k.fg2))],
        for (final s in steps)
          Padding(
            padding: const EdgeInsets.only(top: 10),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Container(
                  width: 20,
                  height: 20,
                  decoration: BoxDecoration(
                    shape: BoxShape.circle,
                    color: s.ok ? k.upSoft : k.downSoft,
                    border: Border.all(color: (s.ok ? k.up : k.down).withValues(alpha: 0.4)),
                  ),
                  child: Icon(s.ok ? LucideIcons.check : LucideIcons.x, size: 12, color: s.ok ? k.up : k.down),
                ),
                const SizedBox(width: 10),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text.rich(
                        TextSpan(
                          text: t.dyn('accounts.delete.step.${s.step}', fallback: s.step),
                          children: [
                            if (showState)
                              TextSpan(
                                text: ' · ${s.ok ? t('accounts.delete.stepOk') : t('accounts.delete.stepFailed')}',
                                style: TextStyle(color: k.fg3),
                              ),
                          ],
                        ),
                        style: context.text.callout.copyWith(color: k.fg),
                      ),
                      if (s.detail != null && s.detail!.isNotEmpty) Text(s.detail!, style: context.text.footnote.copyWith(color: k.fg3)),
                    ],
                  ),
                ),
              ],
            ),
          ),
      ],
    );
  }
}

/// A list of blockers in a warning box (web "can't be deleted / closed right now").
class BlockersBox extends StatelessWidget {
  const BlockersBox({super.key, required this.title, required this.blockers});
  final String title;
  final List<Blocker> blockers;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    return KNotice(
      tone: KChipTone.warn,
      icon: LucideIcons.circleSlash,
      title: title,
      text: blockers.map((b) => '• ${t.dyn('accounts.blocker.${b.code}', fallback: b.message)}').join('\n'),
    );
  }
}

/// A multi-line text field (web textarea), styled like KTextField.
class KTextArea extends StatelessWidget {
  const KTextArea({super.key, required this.controller, this.label, this.placeholder, this.maxLength = 1000, this.lines = 3});
  final TextEditingController controller;
  final String? label;
  final String? placeholder;
  final int maxLength;
  final int lines;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        if (label != null)
          Padding(
            padding: const EdgeInsets.only(bottom: 6),
            child: Text(label!, style: context.text.footnote.copyWith(color: k.fg3)),
          ),
        Container(
          decoration: BoxDecoration(
            color: k.surface2,
            borderRadius: BorderRadius.circular(14),
            border: Border.all(color: k.line),
          ),
          child: TextField(
            controller: controller,
            minLines: lines,
            maxLines: lines,
            inputFormatters: [LengthLimitingTextInputFormatter(maxLength)],
            style: context.text.callout.copyWith(color: k.fg),
            cursorColor: k.ember,
            decoration: InputDecoration(
              isCollapsed: true,
              border: InputBorder.none,
              hintText: placeholder,
              hintStyle: context.text.callout.copyWith(color: k.fg3),
              contentPadding: const EdgeInsets.symmetric(horizontal: 14, vertical: 11),
            ),
          ),
        ),
      ],
    );
  }
}

/// The leverage choices as pills (web leverage buttons: ember-soft when chosen).
class LeveragePills extends StatelessWidget {
  const LeveragePills({super.key, required this.values, required this.selected, required this.onSelect, this.enabled = true});
  final List<int> values;
  final int selected;
  final ValueChanged<int> onSelect;
  final bool enabled;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Opacity(
      opacity: enabled ? 1 : 0.45,
      child: Wrap(
        spacing: 8,
        runSpacing: 8,
        children: [
          for (final l in values)
            KPressable(
              onTap: enabled ? () => onSelect(l) : null,
              pressedScale: 0.96,
              child: AnimatedContainer(
                duration: const Duration(milliseconds: 160),
                height: 40,
                constraints: const BoxConstraints(minWidth: 80),
                padding: const EdgeInsets.symmetric(horizontal: 16),
                alignment: Alignment.center,
                decoration: BoxDecoration(
                  color: l == selected ? k.emberSoft : k.surface2,
                  borderRadius: BorderRadius.circular(20),
                  border: Border.all(color: l == selected ? k.ember.withValues(alpha: 0.6) : k.line),
                ),
                child: Text(
                  levLabel(l),
                  textDirection: TextDirection.ltr,
                  style: context.text.label.copyWith(
                    fontWeight: FontWeight.w600,
                    fontSize: 13.5,
                    color: l == selected ? k.ember : k.fg2,
                    fontFeatures: kTabular,
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}

/// A selectable box (web choice buttons: ember-soft border when chosen, faded when not allowed).
class ChoiceBox extends StatelessWidget {
  const ChoiceBox({super.key, required this.child, required this.selected, required this.onTap, this.enabled = true, this.tone});
  final Widget child;
  final bool selected;
  final VoidCallback onTap;
  final bool enabled;

  /// The selected colour (default ember; gold for demo balances).
  final Color? tone;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final c = tone ?? k.ember;
    return Opacity(
      opacity: enabled ? 1 : 0.6,
      child: KPressable(
        onTap: enabled ? onTap : null,
        pressedScale: 0.99,
        child: AnimatedContainer(
          duration: const Duration(milliseconds: 160),
          width: double.infinity,
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 11),
          decoration: BoxDecoration(
            color: selected ? Color.lerp(k.surface2, c, 0.1) : k.surface2,
            borderRadius: BorderRadius.circular(14),
            border: Border.all(color: selected ? c.withValues(alpha: 0.5) : k.line),
          ),
          child: child,
        ),
      ),
    );
  }
}

/// The API client, for the dialogs' one-off requests.
ApiClient apiOf(BuildContext context) => ProviderScope.containerOf(context, listen: false).read(apiProvider);

/// Margin-level colour (web levelTone): up / warn / down, or the plain text colour.
Color levelColor(BuildContext context, num? ml) => switch (levelTone(ml)) {
  'up' => context.k.up,
  'warn' => context.k.warn,
  'down' => context.k.down,
  _ => context.k.fg,
};

/// Profit colour by sign.
Color pnlColor(BuildContext context, num v) => v > 0 ? context.k.up : (v < 0 ? context.k.down : context.k.fg);
