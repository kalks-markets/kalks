// The Wallet module (lib/features/wallet): unit tests of the web's rules (amount cleaning and validation, min / max /
// fee maths, address and hash checks, status maps, activity titles, the wallet apps' deep links, error texts) and
// widget tests on the sample-data API (overview sections in the phone order, deposit, withdraw with the emailed code,
// cancel, transfers both ways, history tabs and pager).
import 'dart:convert';
import 'dart:math';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:kalks/core/api/api_providers.dart';
import 'package:kalks/core/models/account.dart';
import 'package:kalks/core/models/user.dart';
import 'package:kalks/features/wallet/deposit_screen.dart';
import 'package:kalks/features/wallet/transfer_screen.dart';
import 'package:kalks/features/wallet/wallet_api.dart';
import 'package:kalks/features/wallet/wallet_history_screen.dart';
import 'package:kalks/features/wallet/wallet_screen.dart';
import 'package:kalks/features/wallet/withdraw_screen.dart';
import 'package:kalks/i18n/t.dart';
import 'package:kalks/preview/c1/preview_wallet.dart';
import 'package:kalks/preview/preview_data.dart';
import 'package:kalks/router/router.dart';
import 'package:kalks/ui/ui.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import 'helpers/test_app.dart';

Map<String, Object?> _en() => {
  'wallet.depositLine': 'Deposit · USDT {network}',
  'wallet.withdrawalLine': 'Withdrawal · USDT {network}',
  'wallet.activity.toTrading': 'To trading account #{login}',
  'wallet.activity.fromTrading': 'From trading account #{login}',
  'wallet.activity.walletTx': 'Wallet transaction',
  'wallet.activity.confirmations': '{done} / {required} confirmations',
  'wallet.activity.to': 'To {address}',
  'wallet.kind.ibPayout': 'Partner payout',
  'wallet.kind.adjustment': 'Balance adjustment',
  'wallet.error.unavailable': 'The wallet is unavailable. Please try again shortly.',
  'wallet.error.generic': 'Something went wrong.',
  'wallet.error.insufficientFunds': 'Not enough available balance.',
  'common.networkError': 'Network error. Check your connection and try again.',
  'wallet.nowpayments.activityLine': 'Crypto deposit · {coin}',
};

final T _t = T('en', null, _en());

ActivityItem _a(Map<String, dynamic> j) => ActivityItem.fromJson({'id': '1', 'created_at': '2026-10-01T10:00:00Z', ...j});

EngineAccount _acc(Map<String, dynamic> j) => EngineAccount.fromJson({
  'login': 10042817,
  'type': 'live',
  'group': 'pro',
  'groupName': 'Pro',
  'currency': 'USD',
  'status': 'active',
  'balance': 1000,
  'withdrawable': 800,
  ...j,
});

/// A KButton by its icon (the same in every language).
Finder _button(IconData icon) => find.byWidgetPredicate((w) => w is KButton && (w.icon == icon || w.trailingIcon == icon));

/// A KIconButton by its accessibility label.
Finder _iconButton(String label) => find.byWidgetPredicate((w) => w is KIconButton && w.semanticLabel == label);

/// Scrolls `f` to the middle of its page (clear of the frosted header and the tab bar).
Future<void> _center(WidgetTester tester, Finder f) async {
  await tester.runAsync(() => Scrollable.ensureVisible(tester.element(f.first), alignment: 0.5));
  await tester.pump(const Duration(milliseconds: 300));
}

Future<void> _tap(WidgetTester tester, Finder f) async {
  await _center(tester, f);
  await tester.tap(f.first);
  await settle(tester, frames: 6);
}

Future<ProviderContainer> _open(WidgetTester tester, String location) async {
  final c = await pumpApp(tester, signedIn: true);
  c.read(routerProvider).go(location);
  await settle(tester);
  return c;
}

List<({String method, String path, Map<String, dynamic> body})> _writes(String path) => previewWalletCalls.where((c) => c.path == path).toList();

int _intentReads() => previewWalletReads.where((r) => r.startsWith('wallet/deposits/intents/')).length;

/// Steps the clock 100 ms at a time until the open deposit is polled again, lets the answer render, and returns how
/// long the wait was.
Future<Duration> _nextPoll(WidgetTester tester) async {
  final n = _intentReads();
  var waited = Duration.zero;
  while (_intentReads() == n && waited < const Duration(seconds: 10)) {
    await tester.pump(const Duration(milliseconds: 100));
    await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 2)));
    waited += const Duration(milliseconds: 100);
  }
  expect(_intentReads(), n + 1, reason: 'one poll');
  // the answer lands and renders without the clock moving
  for (var i = 0; i < 6; i++) {
    await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 10)));
    await tester.pump();
  }
  return waited;
}

void main() {
  setUp(resetPreviewWallet);

  group('amounts', () {
    test('cleanAmount keeps digits, one point and two decimals (comma = point)', () {
      expect(cleanAmount('1,5'), '1.5');
      expect(cleanAmount('12.345'), '12.34');
      expect(cleanAmount('1.2.3'), '1.23');
      expect(cleanAmount(r'$ 1 000abc'), '1000');
      expect(cleanAmount('.5'), '.5');
      expect(cleanAmount(''), '');
    });

    test('amountOk: up to 2 decimals, above zero, 12 digits at most', () {
      expect(amountOk('100'), isTrue);
      expect(amountOk(' 0.01 '), isTrue);
      expect(amountOk('0'), isFalse);
      expect(amountOk('1.234'), isFalse);
      expect(amountOk('1234567890123'), isFalse);
      expect(amountOk('.5'), isFalse);
      expect(amountOk(''), isFalse);
    });

    test('depositAmountOk: 6 decimals and the network minimum', () {
      expect(depositAmountOk('10', '10'), isTrue);
      expect(depositAmountOk('9.999999', '10'), isFalse);
      expect(depositAmountOk('10.123456', '10'), isTrue);
      expect(depositAmountOk('10.1234567', '10'), isFalse);
      expect(depositAmountOk('0', '0'), isFalse);
    });

    test('transfers stay within the maximum', () {
      expect(transferAmountOk('100', 100), isTrue);
      expect(transferAmountOk('100.01', 100), isFalse);
      expect(transferAmountOk('3000.4', 3000.40), isTrue);
    });

    test('Max rounds the cents down and writes the number like JavaScript', () {
      expect(maxAmount('3250.40'), '3250.4');
      expect(maxAmount('3000'), '3000');
      expect(maxAmount(12.999), '12.99');
      expect(maxAmount('0'), '0');
      expect(maxAmount(null), '0');
    });

    test('fee label, daily limit share and display amounts', () {
      const l = WalletLimits(
        withdrawMin: '20',
        withdrawMax: '10000',
        withdrawDailyMax: '50000',
        withdrawFeeFlat: '1',
        withdrawFeePct: '0',
        depositCooldownHours: 24,
        intentTtlMinutes: 60,
      );
      expect(feeLabel(l), '1.00 USDT');
      const pct = WalletLimits(
        withdrawMin: '20',
        withdrawMax: '10000',
        withdrawDailyMax: '50000',
        withdrawFeeFlat: '1.5',
        withdrawFeePct: '0.5',
        depositCooldownHours: 0,
        intentTtlMinutes: 60,
      );
      expect(feeLabel(pct), '1.50 USDT + 0.5%');
      expect(limitUsed('250', '50000'), closeTo(0.005, 1e-9));
      expect(limitUsed('60000', '50000'), 1);
      expect(limitUsed('10', '0'), 0);
      expect(fmt('1234.5'), '1,234.50');
      expect(fmt('0.123456'), '0.123456');
      expect(fmt('abc'), '—');
    });

    test('requestId: 24 hex characters, fresh each time', () {
      final a = requestId(Random(1));
      final b = requestId(Random(2));
      expect(a, matches(RegExp(r'^[0-9a-f]{24}$')));
      expect(a, isNot(b));
      expect(requestId(), matches(RegExp(r'^[0-9a-f]{24}$')));
    });
  });

  group('addresses, hashes, links', () {
    test('address formats per network', () {
      expect(addressLooksValid('bsc', '0x7a1F3c9B2e4D5f60718293aBcDeF0123456789aB'), isTrue);
      expect(addressLooksValid('bsc', '0x7a1F3c9B2e4D5f60718293aBcDeF0123456789a'), isFalse);
      expect(addressLooksValid('bsc', 'TQn9Y2khEsLJW1ChVWFMSMeRDow5KcbLSE'), isFalse);
      expect(addressLooksValid('tron', 'TQn9Y2khEsLJW1ChVWFMSMeRDow5KcbLSE'), isTrue);
      expect(addressLooksValid('tron', ' TQn9Y2khEsLJW1ChVWFMSMeRDow5KcbLSE '), isTrue);
      // base58 has no 0, O, I or l
      expect(addressLooksValid('tron', 'TQn9Y2khEsLJW1ChVWFMSMeRDow5KcbLS0'), isFalse);
    });

    test('hashes, request ids, short hashes', () {
      expect(txHashLooksValid('0x${'a' * 64}'), isTrue);
      expect(txHashLooksValid('A' * 64), isTrue);
      expect(txHashLooksValid('0x${'a' * 63}'), isFalse);
      expect(intentIdValid('dep_4c1d9e2f7a8b6c5d3e2f1a0b'), isTrue);
      expect(intentIdValid('dep_4C1D9E2F7A8B6C5D3E2F1A0B'), isFalse);
      expect(intentIdValid(null), isFalse);
      expect(shortHash('0x1234567890abcdef', 4, 3), '0x12…def');
      expect(shortHash('short'), 'short');
    });

    test('MetaMask and TronLink deep links open this deposit in the wallet app', () {
      expect(appHost('https://app.kalkstrade.com'), 'app.kalkstrade.com');
      expect(appHost('http://127.0.0.1:3000'), '127.0.0.1:3000');
      expect(metamaskDeepLink('app.kalkstrade.com', 'dep_abc'), 'https://metamask.app.link/dapp/app.kalkstrade.com/wallet/deposit?intent=dep_abc');
      final tl = Uri.parse(tronlinkDeepLink('app.kalkstrade.com', 'dep_abc'));
      expect(tl.scheme, 'tronlinkoutside');
      expect(tl.host, 'pull.activity');
      final param = jsonDecode(tl.queryParameters['param']!) as Map;
      expect(param, {'url': 'https://app.kalkstrade.com/wallet/deposit?intent=dep_abc', 'action': 'open', 'protocol': 'tronlink', 'version': '1.0'});
      expect(walletAppLink('bsc', 'h', 'dep_x'), startsWith('https://metamask.app.link/'));
      expect(walletAppLink('tron', 'h', 'dep_x'), startsWith('tronlinkoutside://'));
      expect(chainLabel('bsc'), (name: 'BNB Smart Chain', short: 'BEP20', wallet: 'MetaMask'));
      expect(chainLabel('tron').short, 'TRC20');
    });
  });

  group('statuses and activity', () {
    test('status maps follow the web', () {
      expect(kDepositStatus['confirming'], (tone: KChipTone.info, label: 'wallet.status.deposit.confirming'));
      expect(kDepositStatus['unmatched']!.label, 'wallet.status.deposit.review');
      expect(kWithdrawalStatus['requested'], (tone: KChipTone.warn, label: 'wallet.status.withdrawal.requested'));
      expect(kWithdrawalStatus['cancelled']!.tone, KChipTone.neutral);
      expect(kTransferStatus['pending']!.label, 'common.processing');
      expect(activityStatus(_a({'type': 'other', 'status': 'completed'})), isNull);
      expect(activityStatus(_a({'type': 'withdrawal', 'status': 'paid'}))!.label, 'wallet.status.withdrawal.paid');
    });

    test('activity titles, details and currencies', () {
      expect(activityTitle(_a({'type': 'deposit', 'chain': 'tron'}), _t), 'Deposit · USDT TRC20');
      expect(activityTitle(_a({'type': 'withdrawal', 'chain': null}), _t), 'Withdrawal · USDT');
      expect(activityTitle(_a({'type': 'transfer', 'direction': 'out', 'login': 10042817}), _t), 'To trading account #10042817');
      expect(activityTitle(_a({'type': 'transfer', 'direction': 'in', 'login': 10042817}), _t), 'From trading account #10042817');
      expect(activityTitle(_a({'type': 'other', 'kind': 'ib_payout'}), _t), 'Partner payout');
      expect(activityTitle(_a({'type': 'other', 'kind': 'adjustment_in'}), _t), 'Balance adjustment');
      expect(activityTitle(_a({'type': 'other', 'kind': 'mystery'}), _t), 'Wallet transaction');
      expect(activitySub(_a({'type': 'deposit', 'status': 'confirming', 'confirmations': 20, 'required_confirmations': 15}), _t), '15 / 15 confirmations');
      expect(activitySub(_a({'type': 'withdrawal', 'address': 'TXLAQ63Xg1NAzckPwKHvzw7CSEmLMEqcdj'}), _t), 'To TXLAQ6…qcdj');
      expect(activitySub(_a({'type': 'other', 'note': 'IB commission'}), _t), 'IB commission');
      expect(activityCurrency(_a({'type': 'transfer', 'direction': 'in'})), 'USD');
      expect(activityCurrency(_a({'type': 'transfer', 'direction': 'out'})), 'USDT');
      expect(_a({'type': 'deposit', 'status': 'rejected'}).dim, isTrue);
    });

    test('crypto deposits (NOWPayments): coin names, activity rows, amounts, models', () {
      expect(npCoinName('usdttrc20'), 'USDT TRC20');
      expect(npCoinName('usdtbsc'), 'USDT BEP20');
      expect(npCoinName('USDTERC20'), 'USDT ERC20');
      expect(npCoinName('bnbbsc'), 'BNB');
      expect(npCoinName('btc'), 'BTC');
      expect(npCoinName(null), '');
      final row = _a({'type': 'deposit', 'kind': 'nowpayments', 'status': 'credited', 'note': 'usdtbsc', 'chain': null});
      expect(activityTitle(row, _t), 'Crypto deposit · USDT BEP20');
      expect(activitySub(row, _t), '');
      expect(activityStatus(row)!.label, 'wallet.status.deposit.credited');
      expect(npAmountOk('100', '10', '50000'), isTrue);
      expect(npAmountOk('9.99', '10', '50000'), isFalse);
      expect(npAmountOk('50001', '10', '50000'), isFalse);
      expect(npAmountOk('10.123', '10', '50000'), isFalse);
      expect(kNpStatus['confirming'], (tone: KChipTone.info, label: 'wallet.nowpayments.status.confirming'));
      final p = NpPayment.fromJson({
        'id': 7,
        'status': 'waiting',
        'price_amount': '100',
        'coin': {'code': 'xrp', 'ticker': 'XRP', 'name': 'XRP', 'network': 'XRP Ledger', 'icon': 'xrp', 'chain_icon': null, 'needs_extra_id': true},
        'pay_amount': '100.5',
        'pay_address': 'rLNaPoKeeBjZe2qs6x52yVPZpZ8td4dc6w',
        'payin_extra_id': '771234',
        'expires_at': '2026-10-23T10:20:00Z',
      });
      expect((p.id, p.open, p.coin.label, p.payinExtraId, p.coin.needsExtraId), (7, true, 'XRP · XRP Ledger', '771234', true));
      final off = NpConfig.fromJson({'enabled': false, 'coins': <Object>[]});
      expect(off.offered, isFalse);
    });

    test('KYC notice per status', () {
      expect(kycNoticeTitle(KycStatus.verified), isNull);
      expect(kycNoticeTitle(KycStatus.pending), 'wallet.kyc.inReview');
      expect(kycNoticeTitle(KycStatus.rejected), 'wallet.kyc.needsAttention');
      expect(kycNoticeTitle(KycStatus.unverified), 'wallet.kyc.verifyToWithdraw');
    });

    test('accounts the wallet transfers to (prop and blocked accounts excluded)', () {
      expect(isTransferable(_acc({})), isTrue);
      expect(isTransferable(_acc({'group': 'prop-100k'})), isFalse);
      expect(isTransferable(_acc({'type': 'demo'})), isFalse);
      expect(isTransferable(_acc({'status': 'disabled'})), isFalse);
      expect(isBetweenEligible(_acc({'status': 'archived'})), isFalse);
      expect(withdrawableUsd(_acc({})), 800);
      expect(withdrawableUsd(_acc({'cent': true, 'currency': 'USC', 'withdrawable': 25000})), 250);
    });

    test('error texts: the wallet wording for outages, the server reason otherwise', () {
      expect(walletErrorText(const ApiException(status: 503, code: 'unavailable', message: 'x'), _t), 'The wallet is unavailable. Please try again shortly.');
      expect(walletErrorText(ApiException.network, _t), 'Network error. Check your connection and try again.');
      expect(
        walletErrorText(const ApiException(status: 422, code: 'below_minimum', message: 'The minimum withdrawal is 20 USDT.'), _t),
        'The minimum withdrawal is 20 USDT.',
      );
      expect(walletErrorText(const ApiException(status: 422, code: 'insufficient_funds', message: 'x'), _t), 'Not enough available balance.');
      expect(walletErrorText(StateError('x'), _t), 'Something went wrong.');
    });
  });

  group('models and the sample API', () {
    test('overview, config and pages parse the API shapes', () {
      final (_, o) = previewWallet('GET', 'wallet/overview', const {}, const {})!;
      final ov = WalletOverviewData.fromJson(o as Map<String, dynamic>);
      // the dashboard's sample total stays 3,250.40
      expect(ov.usdt.total, closeTo(3250.40, 1e-9));
      expect(ov.pendingDeposits.single.status, 'confirming');
      expect(ov.openWithdrawals.single.status, 'requested');
      final (_, c) = previewWallet('GET', 'wallet/config', const {}, const {})!;
      final cfg = WalletConfig.fromJson(c as Map<String, dynamic>);
      expect(cfg.chains.map((x) => x.chain), ['bsc', 'tron']);
      expect(cfg.chain('tron')!.confirmations, 20);
      expect(cfg.limits.withdrawMin, '20');
      final (_, p) = previewWallet('GET', 'wallet/activity', const {}, const {'type': 'all', 'page': '2', 'limit': '25'})!;
      final page = WalletPage.fromJson(p as Map<String, dynamic>, ActivityItem.fromJson);
      expect(page.page, 2);
      expect(page.total, greaterThan(25));
    });

    test('a withdrawal without the emailed code is refused, and books once with it', () {
      final body = {'chain': 'tron', 'amount': '120', 'to_address': 'TXLAQ63Xg1NAzckPwKHvzw7CSEmLMEqcdj', 'idempotency_key': 'abcdef123456'};
      final (status, refused) = previewWallet('POST', 'wallet/withdrawals', body, const {})!;
      expect(status, 403);
      expect(ApiException.fromResponse(status, refused).isStepUp, isTrue);
      final (ok, first) = previewWallet('POST', 'wallet/withdrawals', {...body, 'stepup_token': 't'}, const {})!;
      final (_, again) = previewWallet('POST', 'wallet/withdrawals', {...body, 'stepup_token': 't'}, const {})!;
      expect(ok, 200);
      expect((first as Map)['withdrawal']['id'], (again as Map)['withdrawal']['id']);
      expect((first['withdrawal'] as Map)['net_amount'], '118.00');
    });

    test('a deposit advances pending -> confirming -> credited across its polls', () {
      final (_, r) = previewWallet('POST', 'wallet/deposits/intents', const {'chain': 'tron', 'amount': '250'}, const {})!;
      final intent = DepositIntent.fromJson(((r as Map)['intent'] as Map).cast<String, dynamic>());
      expect(intentIdValid(intent.id), isTrue);
      IntentView poll() => IntentView.fromJson(previewWallet('GET', 'wallet/deposits/intents/${intent.id}', const {}, const {})!.$2 as Map<String, dynamic>);
      expect(poll().deposit, isNull);
      previewWallet('POST', 'wallet/deposits/submit', {'intent_id': intent.id, 'tx_hash': 'f' * 64}, const {});
      expect(
        [poll().deposit!.status, poll().deposit!.status, poll().deposit!.status, poll().deposit!.status],
        ['pending', 'confirming', 'confirming', 'credited'],
      );
    });
  });

  group('screens', () {
    testWidgets('overview: the web sections in the phone order', (tester) async {
      await _open(tester, '/wallet');
      expect(find.byType(WalletScreen), findsOneWidget);
      final page = find.descendant(of: find.byType(WalletScreen), matching: find.byType(Scrollable)).first;
      Finder inPage(String s) => find.descendant(of: find.byType(WalletScreen), matching: find.text(s));
      // the header's actions share one row: History, then Deposit USDT after it
      expect(tester.getTopLeft(inPage('Deposit USDT')).dy, tester.getTopLeft(inPage('History')).dy);
      expect(tester.getTopLeft(inPage('Deposit USDT')).dx, greaterThan(tester.getTopLeft(inPage('History')).dx));
      var lastY = double.negativeInfinity;
      for (final s in [
        'Wallet',
        'Your USDT wallet on BNB Chain and TRON. Deposit, withdraw and fund your trading accounts.',
        'History',
        'Wallet balance',
        'Needs identity verification',
        'In progress',
        'Recent activity',
        'Fund a trading account',
        'Updates',
      ]) {
        if (s == 'Needs identity verification') {
          // the sample client is verified: the Withdraw quick action says where the money goes instead
          expect(find.text(s), findsNothing);
          continue;
        }
        // (lazily built: no .first before the section exists)
        await tester.scrollUntilVisible(inPage(s), 200, scrollable: page);
        // ensureVisible jumps without a frame: lay the page out at its new offset before measuring
        await tester.pump();
        // the page's own scroll offset (a nested horizontal list would report its own)
        final y = tester.getTopLeft(inPage(s).first).dy + tester.state<ScrollableState>(page).position.pixels;
        expect(y, greaterThan(lastY), reason: s);
        lastY = y;
      }
      expect(find.text('Mark all read'), findsOneWidget);
      await tester.tap(find.text('Mark all read'));
      await settle(tester, frames: 6);
      expect(_writes('wallet/notifications/read'), hasLength(1));
      expect(find.text('Everything read'), findsOneWidget);
      await unmount(tester);
    });

    testWidgets('overview, read-only session: balances and activity, never money actions', (tester) async {
      previewMe['impersonation'] = {'mode': 'read_only'};
      addTearDown(() => previewMe.remove('impersonation'));
      await _open(tester, '/wallet');
      Finder inPage(String s) => find.descendant(of: find.byType(WalletScreen), matching: find.text(s));
      expect(inPage('Wallet balance'), findsOneWidget);
      expect(inPage('History'), findsOneWidget);
      for (final s in ['Deposit USDT', 'USDT on BNB Chain or TRON', 'To your own USDT address', 'Wallet ↔ your trading accounts']) {
        expect(inPage(s), findsNothing, reason: s);
      }
      final page = find.descendant(of: find.byType(WalletScreen), matching: find.byType(Scrollable)).first;
      await tester.scrollUntilVisible(inPage('Recent activity'), 300, scrollable: page);
      expect(inPage('View all'), findsOneWidget);
      // to the end: no funding card, no wallet notices
      await tester.drag(page, const Offset(0, -6000));
      await settle(tester, frames: 4);
      expect(inPage('Fund a trading account'), findsNothing);
      expect(inPage('Updates'), findsNothing);
      await unmount(tester);
    });

    testWidgets('identity in review: the notice on the overview and the withdraw form stays off', (tester) async {
      final user = previewMe['user'] as Map<String, dynamic>;
      user['kyc_status'] = 'pending';
      addTearDown(() => user['kyc_status'] = 'verified');
      await _open(tester, '/wallet');
      Finder inPage(String s) => find.descendant(of: find.byType(WalletScreen), matching: find.text(s));
      expect(inPage('Needs identity verification'), findsOneWidget);
      final page = find.descendant(of: find.byType(WalletScreen), matching: find.byType(Scrollable)).first;
      await tester.scrollUntilVisible(inPage('Your identity check is in review'), 200, scrollable: page);
      expect(inPage('View verification'), findsOneWidget);
      await unmount(tester);
      await _open(tester, '/wallet/withdraw');
      expect(find.text('Your identity check is in review'), findsOneWidget);
      // network, address, amount and Max are off; Withdraw too
      expect(tester.widget<KButton>(_button(LucideIcons.arrowUpFromLine)).onPressed, isNull);
      expect(tester.widgetList<TextField>(find.byType(TextField)).every((f) => f.enabled == false), isTrue);
      await unmount(tester);
    });

    testWidgets('deposit: network -> amount -> address + copy -> hash -> status polled every 5 s', (tester) async {
      await _open(tester, '/wallet/deposit');
      expect(find.byType(DepositScreen), findsOneWidget);
      expect(find.text('New deposit'), findsOneWidget);
      expect(find.text('How deposits work'), findsOneWidget);
      await _tap(tester, find.text('TRON · pay with TronLink'));
      await tester.enterText(find.byType(TextField).first, '5');
      await tester.pump();
      // under the 10 USDT minimum
      expect(tester.widget<KButton>(_button(LucideIcons.wallet)).onPressed, isNull);
      await tester.enterText(find.byType(TextField).first, '250');
      await tester.pump();
      await _tap(tester, _button(LucideIcons.wallet));
      final created = _writes('wallet/deposits/intents').single.body;
      expect(created, {'chain': 'tron', 'amount': '250'});
      expect(find.text('Send 250.00 USDT'), findsOneWidget);
      expect(find.text('Open in TronLink'), findsOneWidget);
      expect(find.text('TQn9Y2khEsLJW1ChVWFMSMeRDow5KcbLSE'), findsOneWidget);
      expect(find.byType(KQrCode), findsOneWidget);
      // copy the address
      await _tap(tester, _iconButton('Address'));
      // the banner lasts 2.6 s
      expect(find.text('Copied to clipboard'), findsWidgets);
      // the hash
      final hash = find.byType(TextField).last;
      await tester.ensureVisible(hash);
      await tester.enterText(hash, 'ab' * 32);
      await tester.pump();
      await _tap(tester, find.text('I have sent it'));
      expect(_writes('wallet/deposits/submit').single.body['tx_hash'], 'ab' * 32);
      await settle(tester);
      expect(find.text('Deposit on its way'), findsOneWidget);
      expect(find.text('Waiting for the first block'), findsOneWidget);
      final writes = previewWalletCalls.length;
      // the next poll finds it on the network
      await _nextPoll(tester);
      expect(find.text('6 / 20 confirmations', findRichText: true), findsOneWidget);
      expect(find.text('Confirming'), findsWidgets);
      // polled every 5 s: more confirmations, then credited
      final gap = await _nextPoll(tester);
      expect(gap.inMilliseconds, inInclusiveRange(4800, 5200));
      expect(find.text('13 / 20 confirmations', findRichText: true), findsOneWidget);
      await _nextPoll(tester);
      expect(find.text('Deposit credited'), findsOneWidget);
      expect(find.text('20 / 20 confirmations', findRichText: true), findsOneWidget);
      // polls only read: nothing was sent again
      expect(previewWalletCalls.length, writes);
      await unmount(tester);
    });

    testWidgets('deposit: no crypto method until the broker offers NOWPayments', (tester) async {
      await _open(tester, '/wallet/deposit');
      expect(find.text('New deposit'), findsOneWidget);
      expect(find.text('Crypto (NOWPayments)'), findsNothing);
      await unmount(tester);
    });

    testWidgets('crypto deposit (NOWPayments): amount + coin -> exact amount, address, memo -> confirming -> credited', (tester) async {
      previewNowPayments = true;
      await _open(tester, '/wallet/deposit');
      expect(find.text('USDT direct'), findsOneWidget);
      await _tap(tester, find.text('Crypto (NOWPayments)'));
      expect(find.text('Deposit with crypto'), findsWidgets);
      await tester.enterText(find.byType(TextField).first, '5');
      await tester.pump();
      // under the 10 USD minimum
      expect(tester.widget<KButton>(_button(LucideIcons.wallet)).onPressed, isNull);
      await tester.enterText(find.byType(TextField).first, '100');
      await tester.pump();
      await _tap(tester, find.text('XRP'));
      await _tap(tester, _button(LucideIcons.wallet));
      expect(_writes('wallet/nowpayments/payments').single.body, {'amount': '100', 'pay_currency': 'xrp'});
      await settle(tester);
      expect(find.text('Send 100.500000 XRP'), findsOneWidget);
      expect(find.text('rLNaPoKeeBjZe2qs6x52yVPZpZ8td4dc6w'), findsOneWidget);
      // the memo / tag the coin needs, with its warning
      expect(find.text('771234'), findsOneWidget);
      expect(find.textContaining('You must include this memo'), findsOneWidget);
      expect(find.byType(KQrCode), findsOneWidget);
      int reads() => previewWalletReads.where((r) => r.startsWith('wallet/nowpayments/payments/')).length;
      Future<void> nextPoll() async {
        final n = reads();
        var waited = 0;
        while (reads() == n && waited < 100) {
          await tester.pump(const Duration(milliseconds: 100));
          await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 2)));
          waited++;
        }
        for (var i = 0; i < 6; i++) {
          await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 10)));
          await tester.pump();
        }
      }

      await nextPoll();
      await nextPoll();
      expect(find.text('Payment found, confirming'), findsOneWidget);
      expect(find.text('Received 100.500000 XRP'), findsOneWidget);
      await nextPoll();
      await nextPoll();
      expect(find.text('Deposit credited'), findsOneWidget);
      expect(find.text('100.00 USDT was added to your wallet.'), findsOneWidget);
      await unmount(tester);
    });

    testWidgets('withdraw: refused without the code -> the code sheet -> requested once with the token', (tester) async {
      final c = await _open(tester, '/wallet/withdraw');
      // the server refuses a withdrawal without the emailed code (stepup_required)
      final body = {'chain': 'tron', 'amount': '120', 'to_address': 'TXLAQ63Xg1NAzckPwKHvzw7CSEmLMEqcdj', 'idempotency_key': 'abcdef123456'};
      Object? refused;
      await tester.runAsync(() async {
        try {
          await c.read(apiProvider).post<Object?>('wallet/withdrawals', body: body);
        } catch (e) {
          refused = e;
        }
      });
      expect(refused, isA<ApiException>().having((e) => e.isStepUp, 'isStepUp', isTrue));
      expect(find.text('Confirm your withdrawal'), findsNothing);
      previewWalletCalls.clear();
      expect(find.byType(WithdrawScreen), findsOneWidget);
      expect(find.text('Withdraw USDT'), findsOneWidget);
      await _tap(tester, find.text('USDT · TRC20'));
      await tester.enterText(find.byType(TextField).at(0), 'TXLAQ63Xg1NAzckPwKHvzw7CSEmLMEqcdj');
      await tester.pump();
      await tester.enterText(find.byType(TextField).at(1), '120');
      await tester.pump(const Duration(milliseconds: 500));
      await settle(tester, frames: 6);
      final quote = _writes('wallet/withdrawals/quote');
      expect(quote.last.body, {'chain': 'tron', 'amount': '120', 'to_address': 'TXLAQ63Xg1NAzckPwKHvzw7CSEmLMEqcdj'});
      expect(find.text('118.00 USDT'), findsOneWidget);
      await _tap(tester, _button(LucideIcons.arrowUpFromLine));
      expect(find.text('Confirm your withdrawal'), findsOneWidget);
      // nothing is requested before the code
      expect(_writes('wallet/withdrawals'), isEmpty);
      await tester.enterText(find.byType(TextField).last, '123456');
      await settle(tester);
      final sent = _writes('wallet/withdrawals');
      expect(sent, hasLength(1));
      expect(sent.single.body['stepup_token'], 'preview-stepup');
      expect(sent.single.body['idempotency_key'], matches(RegExp(r'^[0-9a-f]{24}$')));
      expect(find.text('Withdrawal requested'), findsWidgets);
      await unmount(tester);
    });

    testWidgets('withdraw: cancel a request waiting for review', (tester) async {
      await _open(tester, '/wallet/withdraw');
      final page = find.descendant(of: find.byType(WithdrawScreen), matching: find.byType(Scrollable)).first;
      await tester.scrollUntilVisible(find.text('Your withdrawals'), 300, scrollable: page);
      final row = find.byKey(const ValueKey('withdrawal-318'));
      await tester.scrollUntilVisible(row, 200, scrollable: page);
      await _tap(tester, find.descendant(of: row, matching: find.text('Cancel')));
      expect(_writes('wallet/withdrawals/318/cancel'), hasLength(1));
      // the banner lasts 2.6 s
      expect(find.text('Withdrawal cancelled'), findsWidgets);
      await settle(tester);
      expect(find.descendant(of: row, matching: find.text('Cancelled')), findsOneWidget);
      await unmount(tester);
    });

    testWidgets('transfer: wallet -> account and account -> wallet', (tester) async {
      await _open(tester, '/wallet/transfer?to=10042817');
      expect(find.byType(TransferScreen), findsOneWidget);
      expect(find.text('New transfer'), findsOneWidget);
      expect(find.text('Move to another account'), findsOneWidget);
      await tester.enterText(find.byType(TextField).first, '100');
      await tester.pump();
      await _tap(tester, _button(LucideIcons.arrowLeftRight).last);
      final to = _writes('wallet/transfers/to-trading').single.body;
      expect(to['login'], 10042817);
      expect(to['amount'], '100');
      expect(to['idempotency_key'], matches(RegExp(r'^[0-9a-f]{24}$')));
      expect(find.text('Transfer completed'), findsWidgets);
      // the other way: the account's withdrawable money back to the wallet
      await _tap(tester, find.text('Account → wallet'));
      await tester.enterText(find.byType(TextField).first, '50');
      await tester.pump();
      await _tap(tester, _button(LucideIcons.arrowLeftRight).last);
      final from = _writes('wallet/transfers/from-trading').single.body;
      expect(from['login'], 10042817);
      expect(from['idempotency_key'], isNot(to['idempotency_key']));
      // over the maximum: Transfer stays off
      await tester.enterText(find.byType(TextField).first, '99999');
      await tester.pump();
      expect(tester.widget<KButton>(_button(LucideIcons.arrowLeftRight).last).onPressed, isNull);
      await unmount(tester);
    });

    testWidgets('history: tabs and pager', (tester) async {
      await _open(tester, '/wallet/history');
      expect(find.byType(WalletHistoryScreen), findsOneWidget);
      expect(find.text('Wallet history'), findsOneWidget);
      final page = find.descendant(of: find.byType(WalletHistoryScreen), matching: find.byType(Scrollable)).first;
      await tester.scrollUntilVisible(find.textContaining(RegExp(r'^1–25 of \d+$')), 400, scrollable: page);
      await _tap(tester, _iconButton('Next page'));
      expect(find.textContaining(RegExp(r'^26–\d+ of \d+$')), findsOneWidget);
      await tester.drag(page, const Offset(0, 4000));
      await settle(tester, frames: 4);
      await _tap(tester, find.text('Withdrawals'));
      expect(find.text('Withdrawal · USDT TRC20'), findsWidgets);
      expect(find.text('Deposit · USDT TRC20'), findsNothing);
      await unmount(tester);
    });
  });
}
