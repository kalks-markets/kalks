// Sample data for development previews (`--dart-define=KALKS_PREVIEW=true`): web screenshots of the design system and
// golden tests run without a server or an account. The shapes are the real API's (docs/MOBILE-API.md); the values
// are made up. The in-app demo ("Try the demo" on the sign-in page, demoModeProvider) runs on the same data.
//
// Web preview URL switches (preview builds only): ?signedIn=1 (open the Client Area as the sample client),
// ?theme=dark|light, ?lang=<locale>.
import '../core/auth/secure_store.dart';
import '../core/models/user.dart';
import '../env.dart';

Map<String, String> get _query => Env.preview ? Uri.base.queryParameters : const {};

/// Start signed in as the sample client.
bool get previewSignedIn => const bool.fromEnvironment('KALKS_PREVIEW_SIGNED_IN') || _query['signedIn'] == '1';

/// Start on the biometric lock screen (?locked=1).
bool get previewLocked => _query['locked'] == '1';

/// Theme / language forced by the preview URL (null: the saved choice).
String? get previewTheme => _query['theme'];
String? get previewLang => _query['lang'];

Session previewSession() => Session(token: 'preview_${'x' * 40}', expiresAt: DateTime.now().add(const Duration(days: 7)));

final Map<String, dynamic> previewMe = {
  'user': {
    'id': 80412,
    'email': 'arjun.mehta@example.com',
    'first_name': 'Arjun',
    'last_name': 'Mehta',
    'name': 'Arjun Mehta',
    'phone_dial': '+91',
    'phone': '98201 44721',
    'country': 'in',
    'date_of_birth': '1991-04-12',
    'kyc_status': 'verified',
    'email_verified': true,
    'referral_code': 'ARJUN24',
    'created_at': '2026-03-02T09:00:00Z',
    'tenant': {'slug': 'kalks', 'name': 'Kalks'},
  },
  'viewer': null,
  'session': {'id': 1, 'idle_minutes': 60, 'expires_at': DateTime.now().add(const Duration(days: 7)).toIso8601String()},
};

SessionUser previewUser() => SessionUser.fromJson(previewMe);

final Map<String, dynamic> previewConfig = {
  'apiVersion': 1,
  'minAppVersion': null,
  'urls': {
    'app': 'https://app.kalkstrade.com',
    'terminal': 'https://trade.kalkstrade.com',
    'marketData': {'http': 'https://api.kalkstrade.com', 'ws': 'wss://api.kalkstrade.com/v1/stream'},
    'streams': {
      'engine': 'wss://trade.kalkstrade.com/engine/stream',
      'options': 'wss://trade.kalkstrade.com/options/stream',
      'support': 'wss://app.kalkstrade.com/support/stream',
    },
  },
  'tenant': {
    'slug': 'kalks',
    'name': 'Kalks Markets',
    'default': true,
    'logoUrl': null,
    'primary': '#f2600c',
    'accent': '#ffa040',
    'supportEmail': 'support@kalkstrade.com',
    'website': 'https://kalkstrade.com',
  },
  'modules': {'academy': true, 'algo': true, 'api': true, 'copy_trading': true, 'ib': true, 'pamm': true, 'prop': true, 'rewards': true, 'wallet': true},
  'flags': {'client_registration': true, 'demo_accounts': true, 'google_login': true, 'trade_sharing': true},
  'maintenance': {'active': false, 'message': '', 'until': null},
};

Map<String, dynamic> _account(
  int login,
  String type,
  String group,
  String groupName,
  double balance,
  double equity, {
  bool cent = false,
  int positions = 0,
  double margin = 0,
  String name = '',
}) => {
  'login': login,
  'type': type,
  'group': group,
  'groupName': groupName,
  'mode': 'hedging',
  'cent': cent,
  'currency': cent ? 'USC' : 'USD',
  'baseCurrency': 'USD',
  'leverage': type == 'demo' ? 500 : 200,
  'leverages': [50, 100, 200, 500],
  'status': 'active',
  'name': name,
  'marginCall': false,
  'marginCallLevel': 100,
  'stopOutLevel': 50,
  'positions': positions,
  'orders': 0,
  'controls': {'tradingDisabled': false, 'closeOnly': false, 'maxLot': null},
  'balance': balance,
  'credit': 0,
  'bonus': 0,
  'profit': equity - balance,
  'swap': 0,
  'equity': equity,
  'margin': margin,
  'freeMargin': equity - margin,
  'marginLevel': margin > 0 ? equity / margin * 100 : null,
  'withdrawable': balance,
  'createdAt': '2026-03-04T10:00:00Z',
};

final Map<String, dynamic> previewAccounts = {
  'accounts': [
    _account(10042817, 'live', 'pro', 'Pro', 12480.55, 12893.4, positions: 3, margin: 1840, name: 'Main'),
    _account(10051123, 'live', 'cent', 'Cent', 254300, 251980.2, cent: true, positions: 1, margin: 12000),
    _account(20017734, 'demo', 'standard', 'Standard', 10000, 10342.75, positions: 2, margin: 640),
  ],
};

final Map<String, dynamic> previewWalletOverview = {
  'balances': [
    {'currency': 'USDT', 'available': '3250.40', 'locked': '0'},
  ],
  'pending_deposits': [],
  'open_withdrawals': [],
  'limits': {'used_today': '0', 'remaining_today': '50000', 'daily_max': '50000', 'cooldown_until': null},
  'notifications_unread': 0,
};

Map<String, dynamic> get previewWalletActivity => {
  'items': [
    {
      'type': 'deposit',
      'id': '881',
      'status': 'credited',
      'amount': '1500.00',
      'currency': 'USDT',
      'chain': 'tron',
      'network': 'TRC20',
      'direction': 'in',
      'created_at': DateTime.now().subtract(const Duration(hours: 5)).toIso8601String(),
    },
    {
      'type': 'transfer',
      'id': '412',
      'status': 'completed',
      'amount': '1000.00',
      'currency': 'USDT',
      'login': 10042817,
      'direction': 'out',
      'created_at': DateTime.now().subtract(const Duration(days: 1)).toIso8601String(),
    },
    {
      'type': 'withdrawal',
      'id': '77',
      'status': 'completed',
      'amount': '250.00',
      'currency': 'USDT',
      'chain': 'bsc',
      'network': 'BEP20',
      'direction': 'out',
      'created_at': DateTime.now().subtract(const Duration(days: 4)).toIso8601String(),
    },
  ],
  'page': 1,
  'limit': 5,
  'total': 3,
};

final Map<String, dynamic> previewWalletConfig = {
  'chains': [
    {'chain': 'tron', 'network': 'TRC20', 'token': 'USDT', 'min_deposit': '10', 'deposits_enabled': true, 'withdrawals_enabled': true, 'withdraw_fee': '1'},
    {'chain': 'bsc', 'network': 'BEP20', 'token': 'USDT', 'min_deposit': '10', 'deposits_enabled': true, 'withdrawals_enabled': true, 'withdraw_fee': '0.5'},
  ],
  'limits': {},
};

final Map<String, dynamic> previewRewards = {
  'points': {'balance': 18420, 'lifetime': 26100, 'earnedThisMonth': 2140, 'lotsThisMonth': 21.4, 'earned12m': 26100, 'expiringSoon': null},
  'tier': {'key': 'gold', 'name': 'Gold', 'rank': 3, 'multiplier': 1.5, 'minPoints': 15000, 'perks': []},
  'pointValue': 0.01,
};

Map<String, dynamic> previewAnalytics() {
  final pts = <Map<String, dynamic>>[];
  var eq = 15200.0;
  for (var i = 13; i >= 0; i--) {
    final d = DateTime.now().subtract(Duration(days: i));
    eq += (i.isEven ? 1 : -0.4) * 85;
    pts.add({'day': '${d.year}-${d.month.toString().padLeft(2, '0')}-${d.day.toString().padLeft(2, '0')}', 'balance': eq - 120, 'equity': eq, 'flow': 0});
  }
  return {
    'curve': {'points': pts},
  };
}

Map<String, dynamic> get previewNotifications => {
  'unread': 2,
  'items': [
    {
      'id': 9001,
      'type': 'wallet.deposit_credited',
      'category': 'wallet',
      'severity': 'success',
      'title': 'Deposit credited',
      'body': '1,500.00 USDT arrived in your wallet (TRC20).',
      'link': '/wallet/history',
      'read': false,
      'createdAt': DateTime.now().subtract(const Duration(hours: 5)).toIso8601String(),
    },
    {
      'id': 9000,
      'type': 'trading.margin_call',
      'category': 'trading_alerts',
      'severity': 'warning',
      'title': 'Margin level at 180% on #10051123',
      'body': 'Add funds or reduce positions to stay above the 100% margin call level.',
      'link': '/accounts/10051123',
      'read': false,
      'createdAt': DateTime.now().subtract(const Duration(hours: 9)).toIso8601String(),
    },
    {
      'id': 8990,
      'type': 'kyc.approved',
      'category': 'kyc',
      'severity': 'success',
      'title': 'Identity verified',
      'body': 'Your documents were approved. Live trading and withdrawals are open.',
      'link': '/profile/verification',
      'read': true,
      'createdAt': DateTime.now().subtract(const Duration(days: 3)).toIso8601String(),
    },
  ],
};

Map<String, dynamic> previewChallenge(String purpose) => {
  'status': 'otp_required',
  'challenge': 'preview-challenge',
  'purpose': purpose,
  'email_masked': 'a***n@example.com',
  'expires_in': 600,
  'resend_in': 30,
};
