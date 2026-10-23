// Crypto deposits through NOWPayments: port of apps/crm/components/wallet-live/crypto-deposit.tsx.
//   method picker   USDT direct | Crypto (NOWPayments), only when the broker switched NOWPayments on
//   step 1          amount in USD + coin -> POST wallet/nowpayments/payments {amount, pay_currency}
//   step 2          send exactly <pay_amount> <coin> to the address (+ memo / tag), QR, countdown
//   step 3          live status (GET wallet/nowpayments/payments/{id} every 5 s): waiting -> confirming -> credited
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../core/api/api_providers.dart';
import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import '../wallet_api.dart';
import 'wallet_ui.dart';

/// The coin's logo with the network's coin in the corner for tokens (web NpCoinIcon); a letter disc without a logo.
class NpCoinIcon extends StatelessWidget {
  const NpCoinIcon(this.coin, {super.key, this.size = 32});
  final NpCoin coin;
  final double size;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final main = kCoinIcons.contains(coin.icon)
        ? KCoinIcon(coin.icon, size: size)
        : Container(
            width: size,
            height: size,
            alignment: Alignment.center,
            decoration: BoxDecoration(color: k.surface3, shape: BoxShape.circle),
            child: Text(
              coin.ticker.isEmpty ? '?' : coin.ticker.substring(0, 1),
              style: context.text.label.copyWith(fontSize: size * 0.38, fontWeight: FontWeight.w600, color: k.fg2),
            ),
          );
    final chain = coin.chainIcon;
    if (chain == null || !kCoinIcons.contains(chain)) return main;
    return SizedBox(
      width: size + 3,
      height: size + 2,
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          main,
          PositionedDirectional(
            bottom: -1,
            end: -1,
            child: Container(
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                border: Border.all(color: k.surface2, width: 2),
              ),
              child: KCoinIcon(chain, size: (size * 0.47).roundToDouble()),
            ),
          ),
        ],
      ),
    );
  }
}

/// USDT direct or crypto through NOWPayments (web MethodPicker).
class DepositMethodPicker extends StatelessWidget {
  const DepositMethodPicker({super.key, required this.crypto, required this.onChanged});
  final bool crypto;
  final ValueChanged<bool> onChanged;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    Widget option(bool isCrypto, String title, String text, String icon) {
      final on = crypto == isCrypto;
      return Semantics(
        selected: on,
        button: true,
        child: WalletRow(
          selected: on,
          onTap: () => onChanged(isCrypto),
          child: Row(
            children: [
              KCoinIcon(icon, size: 30),
              const SizedBox(width: 12),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(title, style: context.text.headline.copyWith(fontSize: 14, fontWeight: FontWeight.w500)),
                    const SizedBox(height: 2),
                    Text(
                      text,
                      style: context.text.caption.copyWith(color: context.k.fg3, fontWeight: FontWeight.w400, fontSize: 12),
                    ),
                  ],
                ),
              ),
              if (on) _Tick(),
            ],
          ),
        ),
      );
    }

    return Padding(
      padding: const EdgeInsets.only(bottom: kWalletGap),
      child: Column(
        children: [
          option(false, t('wallet.nowpayments.methodDirect'), t('wallet.nowpayments.methodDirectText'), 'usdt'),
          const SizedBox(height: 8),
          option(true, t('wallet.nowpayments.methodCrypto'), t('wallet.nowpayments.methodCryptoText'), 'btc'),
        ],
      ),
    );
  }
}

class _Tick extends StatelessWidget {
  @override
  Widget build(BuildContext context) => Container(
    width: 24,
    height: 24,
    decoration: BoxDecoration(color: context.k.ember, shape: BoxShape.circle),
    child: Icon(LucideIcons.check, size: 14, color: context.k.onEmber),
  );
}

/* ------------------------------------------------------------------ step 1 */

/// Amount in USD and the coin (web CryptoStartForm).
class CryptoStartForm extends ConsumerStatefulWidget {
  const CryptoStartForm({super.key, required this.cfg, required this.onCreated});
  final NpConfig cfg;
  final ValueChanged<NpPayment> onCreated;

  @override
  ConsumerState<CryptoStartForm> createState() => _CryptoStartFormState();
}

class _CryptoStartFormState extends ConsumerState<CryptoStartForm> {
  late String _code = widget.cfg.coins.firstOrNull?.code ?? '';
  final _amount = TextEditingController();
  bool _busy = false;
  String? _err;

  @override
  void dispose() {
    _amount.dispose();
    super.dispose();
  }

  NpCoin? get _coin => widget.cfg.coin(_code);
  bool get _valid => _coin != null && npAmountOk(_amount.text, _coin!.minUsd, widget.cfg.maxUsd);

  Future<void> _submit() async {
    if (!_valid || _busy) return;
    final t = context.t;
    setState(() {
      _busy = true;
      _err = null;
    });
    try {
      final r = await ref
          .read(apiProvider)
          .post<Map<String, dynamic>>('wallet/nowpayments/payments', body: {'amount': _amount.text.trim(), 'pay_currency': _code});
      final p = NpPayment.fromJson((r['payment'] as Map).cast<String, dynamic>());
      if (mounted) widget.onCreated(p);
    } catch (e) {
      if (mounted) setState(() => _err = walletErrorText(e, t, fallback: t('wallet.nowpayments.startFailed')));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final cfg = widget.cfg;
    final coin = _coin;
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(title: t('wallet.nowpayments.title'), subtitle: t('wallet.nowpayments.subtitle')),
          const SizedBox(height: 16),
          KTextField(
            label: t('wallet.nowpayments.amountLabel'),
            controller: _amount,
            placeholder: '100.00',
            keyboardType: const TextInputType.numberWithOptions(decimal: true),
            inputFormatters: [amountFormatter],
            ltr: true,
            onChanged: (_) => setState(() {}),
            onSubmitted: (_) => _submit(),
            trailing: Padding(
              padding: const EdgeInsetsDirectional.only(end: 10),
              child: Text('USD', style: context.text.label.copyWith(color: k.fg2, fontSize: 12.5)),
            ),
          ),
          FieldHint(t('wallet.nowpayments.amountHint', {'min': fmt(coin?.minUsd ?? cfg.minUsd), 'max': fmt(cfg.maxUsd)})),
          const SizedBox(height: 16),
          Text(t('wallet.nowpayments.coin'), style: context.text.label.copyWith(color: k.fg2)),
          const SizedBox(height: 8),
          for (final c in cfg.coins) ...[
            Semantics(
              selected: c.code == _code,
              button: true,
              child: WalletRow(
                selected: c.code == _code,
                onTap: () => setState(() => _code = c.code),
                child: Row(
                  children: [
                    NpCoinIcon(c),
                    const SizedBox(width: 12),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(c.ticker, style: context.text.headline.copyWith(fontSize: 14, fontWeight: FontWeight.w500)),
                          const SizedBox(height: 2),
                          Text(
                            '${c.network.isEmpty ? c.name : c.network} · ${t('wallet.nowpayments.coinMin', {'min': fmt(c.minUsd)})}',
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400, fontSize: 12),
                          ),
                        ],
                      ),
                    ),
                    if (c.code == _code) _Tick(),
                  ],
                ),
              ),
            ),
            const SizedBox(height: 8),
          ],
          if (cfg.feePaidByUser) ...[const SizedBox(height: 4), Text(t('wallet.nowpayments.feeNote'), style: context.text.footnote.copyWith(color: k.fg3))],
          if (cfg.sandbox) ...[const SizedBox(height: 10), KNotice(tone: KChipTone.warn, text: t('wallet.nowpayments.sandbox'))],
          WalletInlineError(_err, top: 14),
          const SizedBox(height: 18),
          KButton(
            label: t('wallet.nowpayments.getDetails'),
            icon: LucideIcons.wallet,
            size: KButtonSize.lg,
            expand: true,
            loading: _busy,
            onPressed: _valid ? _submit : null,
          ),
          const SizedBox(height: 10),
          Text(
            t('wallet.nowpayments.poweredBy'),
            style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
          ),
        ],
      ),
    );
  }
}

/* ------------------------------------------------------------------ steps 2 + 3 */

class _Countdown extends StatefulWidget {
  const _Countdown({required this.until, required this.style});
  final DateTime until;
  final TextStyle style;

  @override
  State<_Countdown> createState() => _CountdownState();
}

class _CountdownState extends State<_Countdown> {
  Timer? _timer;

  @override
  void initState() {
    super.initState();
    _timer = Timer.periodic(const Duration(seconds: 1), (_) {
      if (mounted) setState(() {});
    });
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final left = widget.until.difference(DateTime.now()).inSeconds;
    if (left <= 0) return Text(context.t('wallet.deposit.expired'), style: widget.style.copyWith(color: context.k.warn));
    final h = left ~/ 3600, m = (left % 3600) ~/ 60, s = (left % 60).toString().padLeft(2, '0');
    return Text(
      h > 0 ? '$h:${m.toString().padLeft(2, '0')}:$s' : '$m:$s',
      textDirection: TextDirection.ltr,
      style: widget.style.copyWith(fontFeatures: kTabular),
    );
  }
}

/// A value to send (amount, address, memo) with its copy button.
class _CopyRow extends StatelessWidget {
  const _CopyRow({required this.label, required this.value, this.big = false});
  final String label, value;
  final bool big;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          label.toUpperCase(),
          style: context.text.caption.copyWith(color: k.fg3, letterSpacing: 0.8, fontWeight: FontWeight.w500),
        ),
        const SizedBox(height: 6),
        WalletRow(
          padding: const EdgeInsetsDirectional.fromSTEB(12, 6, 4, 6),
          child: Row(
            children: [
              Expanded(
                child: Text(
                  value,
                  textDirection: TextDirection.ltr,
                  style: big ? context.text.label.copyWith(fontSize: 18, fontWeight: FontWeight.w600, fontFeatures: kTabular) : context.text.mono(12.5),
                ),
              ),
              KIconButton(icon: LucideIcons.copy, size: 32, semanticLabel: label, onPressed: () => kCopy(context, value)),
            ],
          ),
        ),
      ],
    );
  }
}

/// The payment: what to send, then its live status (web CryptoPayPanel).
class CryptoPayPanel extends StatelessWidget {
  const CryptoPayPanel({super.key, required this.p, required this.onNew});
  final NpPayment p;
  final VoidCallback onNew;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final coin = p.coin;
    final st = kNpStatus[p.status] ?? kNpStatus['waiting']!;
    final done = p.status == 'credited';
    final ended = const {'expired', 'failed', 'refunded', 'rejected'}.contains(p.status);
    final meta = context.text.footnote.copyWith(color: k.fg3);
    final (before, after) = splitAtTag(t('wallet.deposit.expiresIn'), 'time');
    final found = p.status != 'waiting' && p.status != 'creating';
    final steps = [(t('wallet.nowpayments.stepWaiting'), true), (t('wallet.nowpayments.stepConfirming'), found), (t('wallet.nowpayments.stepCredited'), done)];
    final net = coin.network.isEmpty ? coin.name : coin.network;
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(
            title: done
                ? t('wallet.deposit.credited')
                : (p.open && p.payAmount != null
                      ? t('wallet.nowpayments.sendTitle', {'amount': p.payAmount, 'coin': coin.ticker})
                      : t('wallet.nowpayments.title')),
            subtitle: t('wallet.nowpayments.sendSubtitle', {'network': net, 'usd': fmt(p.priceAmount)}),
            action: WalletStatusChip(st),
          ),
          if (p.sandbox) ...[const SizedBox(height: 12), KNotice(tone: KChipTone.warn, text: t('wallet.nowpayments.sandbox'))],
          if (p.status == 'waiting' && p.payAddress != null) ...[
            if (p.expiresAt != null) ...[
              const SizedBox(height: 10),
              Row(
                children: [
                  Icon(LucideIcons.clock, size: 14, color: k.fg3),
                  const SizedBox(width: 6),
                  Text(before, style: meta),
                  _Countdown(until: p.expiresAt!, style: meta),
                  Text(after, style: meta),
                ],
              ),
            ],
            const SizedBox(height: 16),
            Center(child: KQrCode(p.payAddress!, size: 168)),
            const SizedBox(height: 8),
            Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                NpCoinIcon(coin, size: 16),
                const SizedBox(width: 6),
                Flexible(
                  child: Text(
                    coin.label,
                    style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 16),
            if (p.payAmount != null) ...[_CopyRow(label: t('wallet.nowpayments.exactAmount'), value: p.payAmount!, big: true), const SizedBox(height: 12)],
            _CopyRow(label: t('wallet.nowpayments.address'), value: p.payAddress!),
            if (p.payinExtraId != null) ...[
              const SizedBox(height: 12),
              _CopyRow(label: t('wallet.nowpayments.memo'), value: p.payinExtraId!, big: true),
              const SizedBox(height: 8),
              KNotice(tone: KChipTone.down, icon: LucideIcons.shieldAlert, text: t('wallet.nowpayments.memoWarning')),
            ],
            const SizedBox(height: 10),
            KNotice(tone: KChipTone.warn, icon: LucideIcons.circleAlert, text: t('wallet.nowpayments.warning', {'coin': coin.ticker, 'network': net})),
          ],
          if (p.status == 'confirming' || done || p.status == 'review') ...[
            const SizedBox(height: 16),
            Row(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      FittedBox(
                        fit: BoxFit.scaleDown,
                        alignment: AlignmentDirectional.centerStart,
                        child: Text.rich(
                          TextSpan(
                            children: [
                              TextSpan(text: '+${fmt(p.creditAmount ?? p.priceAmount)}'),
                              TextSpan(
                                text: ' USDT',
                                style: TextStyle(fontSize: 15, color: k.fg3, fontWeight: FontWeight.w500, letterSpacing: 0),
                              ),
                            ],
                          ),
                          textDirection: TextDirection.ltr,
                          style: context.text.moneyL.copyWith(fontSize: 30),
                        ),
                      ),
                      if ((double.tryParse(p.actuallyPaid ?? '') ?? 0) > 0) ...[
                        const SizedBox(height: 8),
                        Text(t('wallet.nowpayments.received', {'paid': p.actuallyPaid, 'coin': coin.ticker}), style: meta),
                      ],
                    ],
                  ),
                ),
                if (done) const KIllustration(KIllustrationName.depositCredited, width: 96, maxHeight: 110),
              ],
            ),
          ],
          if (!ended) ...[
            const SizedBox(height: 16),
            for (var i = 0; i < steps.length; i++) ...[
              if (i > 0) const SizedBox(height: 8),
              WalletRow(
                padding: const EdgeInsets.fromLTRB(12, 10, 12, 10),
                child: Row(
                  children: [
                    Container(
                      width: 20,
                      height: 20,
                      alignment: Alignment.center,
                      decoration: BoxDecoration(
                        shape: BoxShape.circle,
                        color: steps[i].$2 ? k.upSoft : null,
                        border: Border.all(color: steps[i].$2 ? k.up.withValues(alpha: 0.4) : k.line),
                      ),
                      child: steps[i].$2
                          ? Icon(LucideIcons.check, size: 12, color: k.up)
                          : Text(
                              '${i + 1}',
                              style: context.text.micro.copyWith(color: k.fg3, fontWeight: FontWeight.w500),
                            ),
                    ),
                    const SizedBox(width: 10),
                    Expanded(
                      child: Text(steps[i].$1, style: context.text.footnote.copyWith(color: steps[i].$2 ? k.fg : k.fg3)),
                    ),
                  ],
                ),
              ),
            ],
          ],
          if (done) ...[
            const SizedBox(height: 14),
            Text(t('wallet.nowpayments.creditedText', {'amount': fmt(p.creditAmount ?? p.priceAmount)}), style: context.text.footnote.copyWith(color: k.fg2)),
          ],
          if (p.status == 'review') ...[const SizedBox(height: 14), KNotice(tone: KChipTone.warn, text: t('wallet.nowpayments.reviewText'))],
          if (p.status == 'expired') WalletInlineError(t('wallet.nowpayments.expiredText'), top: 14),
          if (p.status == 'failed' || p.status == 'refunded' || p.status == 'rejected') WalletInlineError(t('wallet.nowpayments.failedText'), top: 14),
          const SizedBox(height: 18),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              if (done) KButton(label: t('wallet.fundTradingAccount'), icon: LucideIcons.arrowLeftRight, onPressed: () => context.go('/wallet/transfer')),
              if (!p.open) KButton(label: t('wallet.newDeposit'), variant: KButtonVariant.surface, onPressed: onNew),
              KButton(label: t('wallet.backToWallet'), variant: KButtonVariant.ghost, onPressed: () => context.go('/wallet')),
            ],
          ),
        ],
      ),
    );
  }
}

/// The crypto method: the start form, or payment `paymentId` (polled every 5 s).
class CryptoDeposit extends ConsumerWidget {
  const CryptoDeposit({super.key, required this.cfg, required this.paymentId, required this.onPayment});
  final NpConfig cfg;
  final int? paymentId;
  final ValueChanged<int?> onPayment;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final id = paymentId;
    if (id == null) return CryptoStartForm(cfg: cfg, onCreated: (p) => onPayment(p.id));
    final view = ref.watch(npPaymentProvider(id));
    if (view.hasValue) return CryptoPayPanel(p: view.requireValue, onNew: () => onPayment(null));
    if (view.hasError) {
      final e = view.error;
      return KCard(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            WalletInlineError(e is ApiException && e.status == 404 ? t('wallet.nowpayments.notFound') : walletErrorText(e, t)),
            const SizedBox(height: 12),
            Align(
              alignment: AlignmentDirectional.centerStart,
              child: KButton(label: t('wallet.newDeposit'), variant: KButtonVariant.surface, onPressed: () => onPayment(null)),
            ),
          ],
        ),
      );
    }
    return const KSkeletonCard(height: 420, lines: 6);
  }
}

/// The three steps of a crypto deposit (web CryptoHowItWorks).
class CryptoHowItWorks extends StatelessWidget {
  const CryptoHowItWorks({super.key});

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final steps = [
      (t('wallet.nowpayments.how1Title'), t('wallet.nowpayments.how1Text')),
      (t('wallet.nowpayments.how2Title'), t('wallet.nowpayments.how2Text')),
      (t('wallet.nowpayments.how3Title'), t('wallet.nowpayments.how3Text')),
    ];
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(title: t('wallet.howDepositsWork')),
          const SizedBox(height: 14),
          for (var i = 0; i < steps.length; i++)
            Padding(
              padding: EdgeInsets.only(top: i == 0 ? 0 : 14),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Container(
                    width: 24,
                    height: 24,
                    alignment: Alignment.center,
                    decoration: BoxDecoration(
                      shape: BoxShape.circle,
                      border: Border.all(color: k.line),
                    ),
                    child: Text('${i + 1}', style: context.text.caption.copyWith(color: k.fg2, fontSize: 11)),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(steps[i].$1, style: context.text.label.copyWith(fontWeight: FontWeight.w600)),
                        const SizedBox(height: 2),
                        Text(steps[i].$2, style: context.text.footnote.copyWith(color: k.fg3)),
                      ],
                    ),
                  ),
                ],
              ),
            ),
        ],
      ),
    );
  }
}
