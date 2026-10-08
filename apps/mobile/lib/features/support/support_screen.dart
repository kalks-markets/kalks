// Support (port of the Live branch of apps/crm/app/(app)/support/page.tsx = components/support/live-support.tsx), in
// the web's phone order: page header, the live chat (AI bot that hands over to our team), your conversations (history
// with a read-only transcript), the email channel (write to support, copy client ID) and the notifications note.
// A broker that switched the live chat off (module `support_chat`) keeps the page without the chat and the
// conversations: the header, the email channel and the note (web live-support.tsx `chat` false).
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../core/api/api_providers.dart';
import '../../core/auth/auth_controller.dart';
import '../../core/config/app_config.dart';
import '../../core/format/format.dart';
import '../../core/notifications/notifications.dart';
import '../../i18n/i18n.dart';
import '../../ui/ui.dart';
import 'live_chat.dart';
import 'live_chat_controller.dart';
import 'support_data.dart';
import 'support_models.dart';
import 'widgets/message_row.dart';

/// The web's SUPPORT_EMAIL (apps/crm/lib/live.ts), used when the broker's config has none.
const String kDefaultSupportEmail = 'support@kalkstrade.com';

class SupportScreen extends ConsumerStatefulWidget {
  const SupportScreen({super.key, this.query = const {}});

  /// The route's query parameters (the web page's search params).
  final Map<String, String> query;

  @override
  ConsumerState<SupportScreen> createState() => _SupportScreenState();
}

class _SupportScreenState extends ConsumerState<SupportScreen> {
  /// Made (and started) on the first build with the live chat on, never for the email-only page.
  LiveChatController? _chatCtl;
  LiveChatController get _chat => _chatCtl ??= createLiveChatController(ref);
  final GlobalKey<_HistoryCardState> _history = GlobalKey();
  final GlobalKey _head = GlobalKey();
  double? _headH;

  @override
  void dispose() {
    _chatCtl?.dispose();
    super.dispose();
  }

  /// Measures the page header after layout, so the chat fills the rest of the first screen.
  void _measure() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      final box = _head.currentContext?.findRenderObject();
      if (!mounted || box is! RenderBox || !box.hasSize) return;
      if (box.size.height != _headH) setState(() => _headH = box.size.height);
    });
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    if (!ref.watch(configProvider).moduleOn('support_chat')) {
      // switched off while open: the chat stops once its widgets are gone
      final old = _chatCtl;
      _chatCtl = null;
      if (old != null) WidgetsBinding.instance.addPostFrameCallback((_) => old.dispose());
      // as the web (live-support.tsx `chat` false): the header, the email channel and the notifications note
      return KPageScroll(
        children: [
          KPageHeader(title: t('support.page.title'), subtitle: Text(t('support.page.subtitle'))),
          const SizedBox(height: 18),
          const _EmailCard(),
          const SizedBox(height: 16),
          const _NoticeCard(),
        ],
      );
    }
    final mq = MediaQuery.of(context);
    // the shell doesn't resize for the keyboard: keep the composer above it
    final keyboard = mq.viewInsets.bottom;
    _measure();
    // the chat fills the first screen under the page header, its composer just above the tab bar (the page scrolls
    // under the shell's bars, whose heights arrive as padding; 12 = the page's top padding, 18 = the gap)
    final chatH = mq.size.height - mq.padding.top - mq.padding.bottom - 12 - (_headH ?? 90) - 18 - 12;
    return Padding(
      padding: EdgeInsets.only(bottom: keyboard),
      child: KPageScroll(
        onRefresh: () async {
          await Future.wait<void>([_chat.load(), if (_history.currentState != null) _history.currentState!.load()]);
        },
        children: [
          KPageHeader(key: _head, title: t('support.page.title'), subtitle: Text(t('support.page.subtitle'))),
          const SizedBox(height: 18),
          LiveChat(controller: _chat, height: chatH),
          const SizedBox(height: 16),
          _HistoryCard(key: _history, chat: _chat),
          const SizedBox(height: 16),
          const _EmailCard(),
          const SizedBox(height: 16),
          const _NoticeCard(),
        ],
      ),
    );
  }
}

/// Past conversations with a read-only transcript (web HistoryCard).
class _HistoryCard extends ConsumerStatefulWidget {
  const _HistoryCard({super.key, required this.chat});
  final LiveChatController chat;

  @override
  ConsumerState<_HistoryCard> createState() => _HistoryCardState();
}

class _HistoryCardState extends ConsumerState<_HistoryCard> {
  List<SupportConversation>? _items;
  void Function()? _off;
  Timer? _debounce;
  String _chatSig = '';

  @override
  void initState() {
    super.initState();
    unawaited(load());
    // refresh when a conversation opens, changes status or is rated (stream frames; the chat's own changes when
    // there is no live stream)
    _off = ref.read(supportFramesProvider).listen((f) {
      if (f['type'] == 'conversation' || f['type'] == 'reconnected') _later();
    });
    widget.chat.addListener(_onChat);
  }

  void _onChat() {
    final c = widget.chat.conv;
    final sig = c == null ? '' : '${c.id}|${c.status}|${c.csat?.rating}';
    if (sig == _chatSig) return;
    _chatSig = sig;
    if (sig.isNotEmpty) _later();
  }

  void _later() {
    _debounce?.cancel();
    _debounce = Timer(const Duration(milliseconds: 400), () => unawaited(load()));
  }

  @override
  void dispose() {
    _off?.call();
    _debounce?.cancel();
    widget.chat.removeListener(_onChat);
    super.dispose();
  }

  Future<void> load() async {
    try {
      final d = await ref.read(apiProvider).get<Map<String, dynamic>>('support/conversations');
      final items = [for (final c in (d['items'] is List ? d['items'] as List : const [])) ?SupportConversation.tryParse(c)];
      if (mounted) setState(() => _items = items);
    } catch (_) {
      if (mounted) setState(() => _items = const []);
    }
  }

  Future<void> _view(SupportConversation c) async {
    final t = context.t;
    try {
      final d = await ref.read(apiProvider).get<Map<String, dynamic>>('support/conversations/${c.id}');
      final conv = SupportConversation.tryParse(d['conversation']);
      if (conv == null) throw const ApiException(status: 404, code: 'not_found', message: 'Not found.');
      if (!mounted) return;
      await _transcript(conv, parseMessages(d['messages']));
    } catch (e) {
      ref.read(notificationsProvider.notifier).toast(NotificationKind.error, t('support.toast.openFailed'), description: errorText(e, t));
    }
  }

  Future<void> _transcript(SupportConversation c, List<SupportMessage> msgs) {
    final t = context.t;
    final me = ref.read(meProvider);
    final f = LocaleFormat(t.locale);
    return showKSheet<void>(
      context,
      title: c.subject.isEmpty ? t('support.conversation') : c.subject,
      builder: (ctx) => KSheetContent(
        children: [
          Text(
            '${f.date(c.createdAt)} · ${t(supportStatus(c.status).label)}',
            textAlign: TextAlign.center,
            style: ctx.text.footnote.copyWith(color: ctx.k.fg3),
          ),
          const SizedBox(height: 16),
          for (final m in msgs) ...[MessageRow(m: m, botName: 'Kalks AI', meName: me?.name ?? ''), const SizedBox(height: 16)],
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final f = LocaleFormat(t.locale);
    final items = _items;
    return KCard(
      key: const ValueKey('support-history'),
      padding: const EdgeInsets.fromLTRB(16, 18, 16, 14),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 4),
            child: KCardHeader(title: t('support.history.title'), subtitle: t('support.history.subtitle')),
          ),
          const SizedBox(height: 10),
          if (items == null)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 22),
              child: Text(
                t('common.loading'),
                textAlign: TextAlign.center,
                style: context.text.footnote.copyWith(color: k.fg3, fontSize: 13),
              ),
            )
          else if (items.isEmpty)
            KEmptyState(compact: true, icon: LucideIcons.bot, title: t('support.history.emptyTitle'), text: t('support.history.emptyText'))
          else
            for (final c in items)
              KPressable(
                onTap: () => _view(c),
                pressedScale: 1,
                semanticLabel: c.subject.isEmpty ? t('support.conversation') : c.subject,
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 9),
                  child: Row(
                    children: [
                      Container(
                        width: 36,
                        height: 36,
                        alignment: Alignment.center,
                        decoration: BoxDecoration(color: k.surface3, shape: BoxShape.circle),
                        child: Icon(LucideIcons.messageSquareText, size: 16, color: k.fg2),
                      ),
                      const SizedBox(width: 12),
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              c.subject.isEmpty ? t('support.conversation') : c.subject,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: context.text.callout.copyWith(color: k.fg),
                            ),
                            const SizedBox(height: 2),
                            Row(
                              children: [
                                Text(
                                  f.date(c.createdAt),
                                  style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                                ),
                                if (c.csat != null) ...[
                                  Text(
                                    ' · ${c.csat!.rating} ',
                                    style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                                  ),
                                  Icon(Icons.star_rounded, size: 13, color: k.gold),
                                ],
                              ],
                            ),
                          ],
                        ),
                      ),
                      const SizedBox(width: 8),
                      KChip(label: t(supportStatus(c.status).label), tone: supportStatus(c.status).tone, small: true),
                      const SizedBox(width: 4),
                      Icon(Directionality.of(context) == TextDirection.rtl ? LucideIcons.chevronLeft : LucideIcons.chevronRight, size: 16, color: k.fg3),
                    ],
                  ),
                ),
              ),
        ],
      ),
    );
  }
}

/// "Prefer email?": the support address, how to write, Write to support and Copy client ID.
class _EmailCard extends ConsumerWidget {
  const _EmailCard();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final me = ref.watch(meProvider);
    final cfg = ref.watch(configProvider);
    final email = (cfg.supportEmail == null || cfg.supportEmail!.isEmpty) ? kDefaultSupportEmail : cfg.supportEmail!;
    final id = me?.clientId ?? '';
    return KCard(
      key: const ValueKey('support-email'),
      padding: const EdgeInsets.all(20),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Container(
            width: 44,
            height: 44,
            alignment: Alignment.center,
            decoration: BoxDecoration(
              color: k.emberSoft,
              shape: BoxShape.circle,
              border: Border.all(color: k.ember.withValues(alpha: 0.3)),
            ),
            child: Icon(LucideIcons.mail, size: 18, color: k.ember),
          ),
          const SizedBox(width: 14),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(t('support.email.prefer'), style: context.text.footnote.copyWith(color: k.fg3, fontSize: 13)),
                const SizedBox(height: 2),
                Text(
                  email,
                  textDirection: TextDirection.ltr,
                  style: context.text.mono(15, color: k.fg),
                ),
                const SizedBox(height: 6),
                KRichText(
                  t('support.email.writeFrom', {'email': me?.email ?? '', 'id': id}),
                  style: context.text.footnote.copyWith(color: k.fg2, height: 1.5),
                  tags: {
                    'email': KTag(
                      style: TextStyle(color: k.fg, fontWeight: FontWeight.w400),
                    ),
                    'id': KTag(style: context.text.mono(12.5, color: k.fg)),
                  },
                ),
                const SizedBox(height: 14),
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    KButton(
                      label: t('support.email.write'),
                      icon: LucideIcons.mail,
                      variant: KButtonVariant.surface,
                      size: KButtonSize.sm,
                      onPressed: () =>
                          launchUrl(Uri.parse('mailto:$email?subject=${Uri.encodeComponent('Support request · $id')}'), mode: LaunchMode.externalApplication),
                    ),
                    KButton(
                      label: t('support.email.copyId'),
                      icon: LucideIcons.copy,
                      variant: KButtonVariant.ghost,
                      size: KButtonSize.sm,
                      onPressed: () => kCopy(context, id, message: t('support.toast.copied', {'what': t('support.clientId')})),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/// "Replies from our team also appear in the notifications bell…".
class _NoticeCard extends StatelessWidget {
  const _NoticeCard();

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return KCard(
      padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 14),
      child: Row(
        children: [
          Icon(LucideIcons.history, size: 16, color: k.fg3),
          const SizedBox(width: 12),
          Expanded(
            child: Text(context.t('support.notice'), style: context.text.footnote.copyWith(color: k.fg2)),
          ),
        ],
      ),
    );
  }
}
