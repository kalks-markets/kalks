// Where a location may go: sign-in state, biometric lock, maintenance, forced update, view-only logins, deep links,
// and the broker's module switches (a switched-off module's page -> /unavailable; nav without it).
import 'package:flutter_test/flutter_test.dart';
import 'package:kalks/core/auth/auth_controller.dart';
import 'package:kalks/core/auth/secure_store.dart';
import 'package:kalks/core/config/app_config.dart';
import 'package:kalks/core/models/user.dart';
import 'package:kalks/router/router.dart';
import 'package:kalks/shell/nav.dart';

void main() {
  final session = Session(token: 'x' * 40, expiresAt: DateTime.now().add(const Duration(days: 7)));
  final me = SessionUser.fromJson(const {
    'user': {'id': 7, 'email': 'a@b.c', 'first_name': 'A', 'last_name': 'B', 'kyc_status': 'verified'},
  });
  final viewer = SessionUser.fromJson(const {
    'user': {'id': 7, 'email': 'a@b.c', 'first_name': 'A', 'last_name': 'B'},
    'viewer': {
      'id': 3,
      'label': 'Accountant',
      'sections': ['wallet', 'history'],
    },
  });
  String? go(AuthState a, String location, {bool maintenance = false, bool update = false, Map<String, bool> modules = const {}}) =>
      redirectFor(auth: a, maintenance: maintenance, updateRequired: update, uri: Uri.parse(location), modules: modules);

  test('booting, signed out and locked', () {
    expect(go(const AuthBooting(), '/wallet'), '/boot');
    expect(go(const AuthSignedOut(), '/wallet/deposit?x=1'), '/login?next=%2Fwallet%2Fdeposit%3Fx%3D1');
    expect(go(const AuthSignedOut(), '/'), '/login');
    expect(go(const AuthSignedOut(), '/register'), isNull);
    expect(go(AuthLocked(session, me), '/portfolio'), '/unlock?next=%2Fportfolio');
    expect(go(AuthLocked(session, me), '/unlock'), isNull);
  });

  test('signed in: auth pages lead into the Client Area (or the page asked for)', () {
    final s = AuthSignedIn(session, me);
    expect(go(s, '/login'), '/');
    expect(go(s, '/unlock?next=%2Fportfolio'), '/portfolio');
    expect(go(s, '/login?next=//evil.example'), '/');
    expect(go(s, '/wallet/withdraw'), isNull);
    expect(go(s, '/trader?login=10042817'), isNull);
  });

  test('maintenance and a forced update win; deep links keep their path', () {
    final s = AuthSignedIn(session, me);
    expect(go(s, '/wallet', maintenance: true), '/maintenance');
    expect(go(s, '/maintenance'), '/');
    expect(go(s, '/', update: true), '/update');
    expect(go(s, 'https://trade.kalkstrade.com/'), '/trader');
    expect(go(const AuthSignedOut(), 'kalks://app/wallet/deposit'), '/login?next=%2Fwallet%2Fdeposit');
  });

  test('view-only logins stay inside their sections', () {
    final s = AuthSignedIn(session, viewer);
    expect(go(s, '/wallet'), isNull);
    expect(go(s, '/portfolio/history'), isNull);
    expect(go(s, '/wallet/withdraw'), '/portfolio/history');
    expect(go(s, '/accounts'), '/portfolio/history');
    expect(go(s, '/trader'), '/portfolio/history');
    final nav = navFor(AppConfig.fallback, viewer);
    expect(nav.map((m) => m.key), ['wallet', 'portfolio']);
    expect(nav.firstWhere((m) => m.key == 'wallet').sub.map((x) => x.href), ['/wallet', '/wallet/history']);
  });

  test('module switches hide pages like the web (navForFeatures)', () {
    final cfg = AppConfig.fromJson(const {
      'modules': {'wallet': false, 'pamm': false, 'algo': false},
    });
    final nav = navFor(cfg, me);
    expect(nav.any((m) => m.key == 'wallet'), isFalse);
    final social = nav.firstWhere((m) => m.key == 'social');
    expect(social.sub.map((s) => s.href), isNot(contains('/social/pamm')));
    expect(nav.firstWhere((m) => m.key == 'developer').sub.map((s) => s.href), ['/developer', '/developer/webhooks', '/developer/docs']);
    expect(isOlderVersion('1.0.0+5', '1.0.1'), isTrue);
    expect(isOlderVersion('1.10.0', '1.9.9'), isFalse);
    expect(isOlderVersion('1.0.0', null), isFalse);
  });

  group('module switches (redirect)', () {
    const off = {'options': false, 'mam': false, 'algo': false, 'news': false, 'wallet': false};

    test('a page of a switched-off module goes to /unavailable with the module named', () {
      final s = AuthSignedIn(session, me);
      expect(go(s, '/options', modules: off), '/unavailable?m=options');
      expect(go(s, '/social/mam', modules: off), '/unavailable?m=mam');
      expect(go(s, '/social/managed', modules: off), '/unavailable?m=mam');
      expect(go(s, '/developer/strategies', modules: off), '/unavailable?m=algo');
      expect(go(s, '/news', modules: off), '/unavailable?m=news');
      expect(go(s, '/wallet/deposit?amount=50', modules: off), '/unavailable?m=wallet');
      // a deep link lands the same way
      expect(go(s, 'kalks://app/options', modules: off), '/unavailable?m=options');
      expect(go(s, 'https://app.kalkstrade.com/social/mam', modules: off), '/unavailable?m=mam');
    });

    test('an on module, a missing key and pages without a module stay where they are', () {
      final s = AuthSignedIn(session, me);
      expect(go(s, '/options'), isNull);
      expect(go(s, '/options', modules: const {'options': true}), isNull);
      expect(go(s, '/options', modules: const {'news': false}), isNull);
      // the rest of the module's neighbours: /social is copy trading, /developer is API keys
      expect(go(s, '/social', modules: off), isNull);
      expect(go(s, '/social/pamm', modules: off), isNull);
      expect(go(s, '/developer', modules: off), isNull);
      expect(go(s, '/', modules: off), isNull);
      expect(go(s, '/accounts', modules: off), isNull);
      expect(go(s, '/support', modules: const {'support_chat': false}), isNull);
      expect(go(s, '/trader', modules: off), isNull);
    });

    test('/unavailable never redirects itself (no loop), for viewers too', () {
      expect(go(AuthSignedIn(session, me), '/unavailable?m=options', modules: off), isNull);
      expect(go(AuthSignedIn(session, me), '/unavailable'), isNull);
      expect(go(AuthSignedIn(session, viewer), '/unavailable?m=wallet', modules: off), isNull);
    });

    test('signed out, locked, maintenance and update keep their priority', () {
      expect(go(const AuthSignedOut(), '/options', modules: off), '/login?next=%2Foptions');
      expect(go(const AuthSignedOut(), '/unavailable?m=options', modules: off), '/login');
      expect(go(AuthLocked(session, me), '/options', modules: off), '/unlock?next=%2Foptions');
      expect(go(AuthLocked(session, me), '/unavailable', modules: off), '/unlock');
      expect(go(const AuthBooting(), '/options', modules: off), '/boot');
      expect(go(AuthSignedIn(session, me), '/options', modules: off, maintenance: true), '/maintenance');
      expect(go(AuthSignedIn(session, me), '/unavailable', modules: off, maintenance: true), '/maintenance');
      expect(go(AuthSignedIn(session, me), '/options', modules: off, update: true), '/update');
      // after sign-in the page asked for is checked again
      expect(go(AuthSignedIn(session, me), '/login?next=%2Foptions', modules: off), '/options');
      expect(go(AuthSignedIn(session, me), '/options', modules: off), '/unavailable?m=options');
    });

    test('a view-only login: an off module first, and its fallback skips switched-off sections', () {
      expect(go(AuthSignedIn(session, viewer), '/wallet', modules: off), '/unavailable?m=wallet');
      final payer = SessionUser.fromJson(const {
        'user': {'id': 7, 'email': 'a@b.c', 'first_name': 'A', 'last_name': 'B'},
        'viewer': {
          'id': 4,
          'label': 'Payouts',
          'sections': ['wallet', 'partner'],
        },
      });
      final s = AuthSignedIn(session, payer);
      expect(go(s, '/accounts'), '/wallet');
      expect(go(s, '/accounts', modules: const {'wallet': false}), '/partner');
      expect(go(s, '/accounts', modules: const {'wallet': false, 'ib': false}), '/more');
    });
  });

  group('module switches (navigation)', () {
    List<NavModule> nav(Map<String, Object> modules) => navFor(AppConfig.fromJson({'modules': modules}), me);
    List<String> subs(List<NavModule> n, String key) => n.firstWhere((m) => m.key == key).sub.map((s) => s.href).toList();

    test('every page maps to its module (longest prefix wins)', () {
      expect(pageModule('/options'), 'options');
      expect(pageModule('/news'), 'news');
      expect(pageModule('/calendar'), 'calendar');
      expect(pageModule('/markets'), 'markets');
      expect(pageModule('/social/mam'), 'mam');
      expect(pageModule('/social/managed'), 'mam');
      expect(pageModule('/social/pamm'), 'pamm');
      expect(pageModule('/social/investments'), 'pamm');
      expect(pageModule('/social/masters/12'), 'copy_trading');
      expect(pageModule('/social/master'), 'copy_trading');
      expect(pageModule('/'), isNull);
      expect(pageModule('/support'), isNull);
      expect(pageModule('/accounts/new'), isNull);
      expect(offModuleOf(const {'mam': false}, '/social/mam'), 'mam');
      expect(offModuleOf(const {'mam': true}, '/social/mam'), isNull);
      expect(
        pageOn(
          AppConfig.fromJson(const {
            'modules': {'options': false},
          }),
          '/options',
        ),
        isFalse,
      );
      expect(pageOn(AppConfig.fallback, '/options'), isTrue);
    });

    test('the Dashboard keeps Overview alone when Markets, News and Calendar are off', () {
      expect(subs(nav(const {}), 'dashboard'), ['/', '/markets', '/news', '/calendar']);
      expect(subs(nav(const {'news': false}), 'dashboard'), ['/', '/markets', '/calendar']);
      final n = nav(const {'markets': false, 'news': false, 'calendar': false});
      expect(subs(n, 'dashboard'), ['/']);
      expect(moduleOf(n, '/')?.sub.length, 1);
    });

    test('Options leaves the navigation; MAM pages leave Copy & PAMM; a module with every page off disappears', () {
      expect(nav(const {}).any((m) => m.key == 'options'), isTrue);
      expect(nav(const {'options': false}).any((m) => m.key == 'options'), isFalse);
      expect(subs(nav(const {'mam': false}), 'social'), ['/social', '/social/copy', '/social/pamm', '/social/investments', '/social/master']);
      // copy trading off: Copy & PAMM opens on its first page left (PAMM)
      final noCopy = nav(const {'copy_trading': false}).firstWhere((m) => m.key == 'social');
      expect(noCopy.href, '/social/pamm');
      expect(noCopy.sub.map((s) => s.href), ['/social/pamm', '/social/investments', '/social/managed', '/social/mam']);
      expect(nav(const {'copy_trading': false, 'pamm': false, 'mam': false}).any((m) => m.key == 'social'), isFalse);
      // `ai` and `support_chat` have no page of their own: the navigation is unchanged
      expect(nav(const {'ai': false, 'support_chat': false}).map((m) => m.key), nav(const {}).map((m) => m.key));
    });
  });
}
