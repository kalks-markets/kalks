// Wallet › Deposit: port of apps/crm/components/wallet-live/deposit-page.tsx (LiveDepositPage), phone order:
//   header (Deposit USDT + ← Wallet)
//   step 1  network + amount            -> POST wallet/deposits/intents {chain, amount}
//   step 2  pay: open the deposit in MetaMask / TronLink (deep link: the app has no injected wallet, as the web on a
//           phone), or send manually to the address (copy, QR) and submit the hash -> POST wallet/deposits/submit
//   step 3  tracker: waiting -> confirmations -> credited (GET wallet/deposits/intents/{id} every 5 s)
//   how deposits work
// When the broker offers crypto deposits through NOWPayments, a method picker switches to widgets/crypto_deposit.dart
// (`?method=crypto`, a payment resumes with `?np=<id>`).
// `?intent=dep_…` resumes a deposit (the overview's in-progress rows link here; the page keeps it in the location).
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/api/api_error.dart';
import '../../i18n/i18n.dart';
import '../../ui/ui.dart';
import 'wallet_api.dart';
import 'widgets/crypto_deposit.dart';
import 'widgets/deposit_panels.dart';
import 'widgets/wallet_ui.dart';

class DepositScreen extends ConsumerStatefulWidget {
  const DepositScreen({super.key, this.query = const {}});

  /// The route's query parameters (the web page's search params).
  final Map<String, String> query;

  @override
  ConsumerState<DepositScreen> createState() => _DepositScreenState();
}

class _DepositScreenState extends ConsumerState<DepositScreen> {
  // the request id lives in state (the web mirrors it into ?intent=… so a reload resumes the same deposit)
  late String? _intentId = widget.query['intent'];
  // crypto through NOWPayments (?method=crypto; a payment resumes with ?np=<id>), when the broker offers it
  late int? _npId = _npOf(widget.query);
  late bool _crypto = widget.query['method'] == 'crypto' || _npId != null;

  static int? _npOf(Map<String, String> q) {
    final v = q['np'] ?? '';
    return RegExp(r'^\d{1,18}$').hasMatch(v) ? int.parse(v) : null;
  }

  @override
  void didUpdateWidget(DepositScreen old) {
    super.didUpdateWidget(old);
    if (old.query['intent'] != widget.query['intent']) _intentId = widget.query['intent'];
    if (old.query['np'] != widget.query['np'] || old.query['method'] != widget.query['method']) {
      _npId = _npOf(widget.query);
      _crypto = widget.query['method'] == 'crypto' || _npId != null;
    }
  }

  /// The request id is mirrored into the location (?intent=…), like the web's URL, so coming back to the Wallet
  /// tab resumes the same deposit.
  void _setIntent(String? id) {
    setState(() => _intentId = id);
    context.go(id == null ? '/wallet/deposit' : '/wallet/deposit?intent=$id');
  }

  void _setNp(int? id) {
    setState(() => _npId = id);
    context.go(id == null ? '/wallet/deposit?method=crypto' : '/wallet/deposit?np=$id');
  }

  void _setMethod(bool crypto) {
    setState(() => _crypto = crypto);
    final np = _npId, intent = _intentId;
    context.go(
      crypto
          ? (np == null ? '/wallet/deposit?method=crypto' : '/wallet/deposit?np=$np')
          : (intent == null ? '/wallet/deposit' : '/wallet/deposit?intent=$intent'),
    );
  }

  Future<void> _refresh() async {
    ref.invalidate(walletCfgProvider);
    ref.invalidate(npConfigProvider);
    final id = _intentId;
    if (intentIdValid(id)) ref.invalidate(depositIntentProvider(id!));
    final np = _npId;
    if (np != null) ref.invalidate(npPaymentProvider(np));
    await ref.read(walletCfgProvider.future).then((_) {}, onError: (Object _) {});
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final cfg = ref.watch(walletCfgProvider);
    final npCfg = ref.watch(npConfigProvider);
    final id = _intentId;
    final valid = intentIdValid(id);
    final view = valid ? ref.watch(depositIntentProvider(id!)) : null;
    final cryptoOn = npCfg.value?.offered ?? false;
    // a payment link keeps working even if the broker switched the option off since
    final crypto = _crypto && (cryptoOn || _npId != null);

    Widget main;
    if (crypto) {
      main = npCfg.hasValue ? CryptoDeposit(cfg: npCfg.requireValue, paymentId: _npId, onPayment: _setNp) : const KSkeletonCard(height: 420, lines: 6);
    } else if (cfg.hasError && !cfg.hasValue) {
      main = WalletUnavailable(onRetry: () => ref.invalidate(walletCfgProvider));
    } else if (!cfg.hasValue || (view != null && !view.hasValue && !view.hasError)) {
      main = const KSkeletonCard(height: 420, lines: 6);
    } else if (view != null && !view.hasValue) {
      final e = view.error;
      main = WalletUnavailable(
        onRetry: () => ref.invalidate(depositIntentProvider(id!)),
        message: e is ApiException && e.status == 404 ? t('wallet.deposit.notFound') : null,
      );
    } else if (view != null && view.requireValue.deposit != null) {
      main = DepositTracker(view: view.requireValue, onNew: () => _setIntent(null));
    } else if (view != null) {
      main = PayPanel(view: view.requireValue, onSubmitted: () => ref.invalidate(depositIntentProvider(id!)));
    } else {
      main = StartForm(cfg: cfg.requireValue, onCreated: (i) => _setIntent(i.id));
    }

    return KPageScroll(
      onRefresh: _refresh,
      children: [
        WalletHeader(
          title: crypto ? t('wallet.nowpayments.title') : t('wallet.depositUsdt'),
          subtitle: crypto ? t('wallet.nowpayments.subtitle') : t('wallet.deposit.subtitle'),
          actions: const [BackToWallet()],
        ),
        if (cryptoOn) DepositMethodPicker(crypto: crypto, onChanged: _setMethod),
        main,
        if (crypto) ...[
          const SizedBox(height: kWalletGap),
          const CryptoHowItWorks(),
        ] else if (cfg.hasValue) ...[
          const SizedBox(height: kWalletGap),
          HowItWorks(cfg: cfg.requireValue),
        ],
      ],
    );
  }
}
