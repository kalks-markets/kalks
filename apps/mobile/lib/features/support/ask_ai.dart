// Ask Kalks AI (port of apps/crm/components/ai/ask-ai.tsx, phone layout): the compact pill in the Dashboard's flow
// that opens a bottom sheet with the suggestions, the question box, the bot's streamed answer and its follow-ups
// (Continue in chat, Talk to a person, New question); answered by the real support bot over the support chat
// (ask_ai_engine.dart), with the open-request-for-a-person handling (hold the question; close that request and ask
// Kalks AI, or send it to the team; View opens the chat). View-only and read-only sessions get nothing (the web's
// dashboard leaves it out for them).
// `chat` false (the broker switched the live chat off, module `support_chat`; web AskAi chat={false}): no way into
// the chat or to a person from here (no chat button, no Continue in chat, no Talk to a person).
// CONTRACT used by the Dashboard — keep these names and parameters:
//   AskAi(chips:, chat:, hero:)       the pill (or, `hero`, the glass bar on Home's photo) that opens the phone sheet
//   AiChip(key:, label:, question:, extra:)   a suggestion; `extra` shows under the question it asked
//   AiFacts(title:, rows:)            the client's own figures under an answer
//   AiLink(href:, label:)             a link to a Client Area page under an answer
import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/api/api_providers.dart';
import '../../core/auth/auth_controller.dart';
import '../../core/lifecycle.dart';
import '../../i18n/i18n.dart';
import '../../ui/ui.dart';
import 'ask_ai_engine.dart';
import 'launcher.dart';
import 'support_data.dart';
import 'widgets/message_row.dart';

class AiChip {
  const AiChip({required this.key, required this.label, this.question, this.extra});
  final String key;

  /// The suggestion's text (also the question sent, unless `question` is given).
  final String label;
  final String? question;

  /// Shown under the question this suggestion asked (AiFacts, AiLink).
  final Widget? extra;
}

/// One row of AiFacts. `tone`: up | warn | down | null.
typedef AiFact = ({String label, String value, String? tone});

/// Label / value rows shown under a question (e.g. the client's real free margin per account).
class AiFacts extends StatelessWidget {
  const AiFacts({super.key, required this.title, required this.rows});
  final String title;
  final List<AiFact> rows;

  @override
  Widget build(BuildContext context) {
    if (rows.isEmpty) return const SizedBox.shrink();
    final k = context.k;
    return LayoutBuilder(
      builder: (context, c) => ConstrainedBox(
        constraints: BoxConstraints(maxWidth: c.maxWidth.isFinite ? c.maxWidth * 0.85 : 340),
        child: Container(
          width: double.infinity,
          padding: const EdgeInsets.fromLTRB(14, 10, 14, 6),
          decoration: BoxDecoration(
            color: k.surface,
            borderRadius: BorderRadius.circular(16),
            border: Border.all(color: k.line),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(
                title,
                style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w600),
              ),
              const SizedBox(height: 4),
              for (var i = 0; i < rows.length; i++) ...[
                if (i > 0) const KDivider(),
                Padding(
                  padding: const EdgeInsets.symmetric(vertical: 6),
                  child: Row(
                    children: [
                      Expanded(
                        child: Text(
                          rows[i].label,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: context.text.footnote.copyWith(color: k.fg2),
                        ),
                      ),
                      const SizedBox(width: 12),
                      Text(
                        rows[i].value,
                        textDirection: TextDirection.ltr,
                        style: context.text.footnote.copyWith(
                          fontWeight: FontWeight.w600,
                          fontFeatures: kTabular,
                          color: switch (rows[i].tone) {
                            'up' => k.up,
                            'warn' => k.warn,
                            'down' => k.down,
                            _ => k.fg,
                          },
                        ),
                      ),
                    ],
                  ),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

/// A link to a Client Area page under an answer (e.g. "Open account"); closes the sheet first.
class AiLink extends StatelessWidget {
  const AiLink({super.key, required this.href, required this.label});
  final String href;
  final String label;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final rtl = Directionality.of(context) == TextDirection.rtl;
    return KPressable(
      semanticLabel: label,
      onTap: () {
        final router = GoRouter.of(context);
        Navigator.of(context, rootNavigator: true).popUntil((r) => r is! PopupRoute);
        router.go(href);
      },
      child: Container(
        height: 36,
        padding: const EdgeInsetsDirectional.fromSTEB(12, 0, 10, 0),
        decoration: BoxDecoration(color: k.emberSoft, borderRadius: BorderRadius.circular(12)),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              label,
              style: context.text.footnote.copyWith(color: k.ember, fontWeight: FontWeight.w600),
            ),
            const SizedBox(width: 6),
            Icon(rtl ? LucideIcons.chevronLeft : LucideIcons.chevronRight, size: 16, color: k.ember),
          ],
        ),
      ),
    );
  }
}

/// The Dashboard's AI entry on phones: a compact pill that opens the Ask Kalks AI sheet; on Home's photo (`hero`), the
/// slim glass bar with the suggestions under it (web AskAi hero, "like the Claude / ChatGPT home").
class AskAi extends ConsumerStatefulWidget {
  const AskAi({super.key, required this.chips, this.chat = true, this.hero = false});
  final List<AiChip> chips;

  /// The live chat is on: the sheet offers the way into it and to a person.
  final bool chat;

  /// The glass bar over a photo, white text, the suggestions as glass pills.
  final bool hero;

  @override
  ConsumerState<AskAi> createState() => _AskAiState();
}

class _AskAiState extends ConsumerState<AskAi> {
  AskAiEngine? _e;

  /// The latest suggestions (the Dashboard rebuilds them with fresh figures while the sheet is open).
  late final ValueNotifier<List<AiChip>> _chips = ValueNotifier(widget.chips);

  @override
  void initState() {
    super.initState();
    final me = ref.read(meProvider);
    if (me == null || me.readOnly) return;
    _e = AskAiEngine(
      api: ref.read(apiProvider),
      frames: ref.read(supportFramesProvider),
      t: () => ref.read(tProvider),
      foreground: () => ref.read(appForegroundProvider),
    )..start();
  }

  @override
  void didUpdateWidget(AskAi old) {
    super.didUpdateWidget(old);
    // the open sheet listens in another route: hand it the fresh suggestions after this build, not during it
    if (old.chips != widget.chips) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _chips.value = widget.chips;
      });
    }
  }

  @override
  void dispose() {
    _e?.dispose();
    _chips.dispose();
    super.dispose();
  }

  void _openChat(BuildContext sheetContext) {
    Navigator.of(sheetContext).pop();
    if (mounted) openSupportChat(context);
  }

  Future<void> _open({AiChip? ask}) async {
    final e = _e;
    if (e == null) return;
    // a suggestion on the photo asks at once; the sheet opens on its answer
    if (ask != null) e.ask(ask.question ?? ask.label, chip: ask.key);
    await showKSheet<void>(
      context,
      builder: (ctx) => _AskAiSheet(e: e, chips: _chips, chat: widget.chat, onOpenChat: () => _openChat(ctx), onClose: () => Navigator.of(ctx).pop()),
    );
  }

  @override
  Widget build(BuildContext context) {
    final e = _e;
    if (e == null) return const SizedBox.shrink();
    final t = context.t;
    final k = context.k;
    return ListenableBuilder(
      listenable: e,
      builder: (context, _) {
        final label = t('dashboard.ai.title', {'name': e.botName});
        if (widget.hero) return _hero(context, e, label);
        return KPressable(
          key: const ValueKey('ask-ai-pill'),
          onTap: _open,
          pressedScale: 0.99,
          semanticLabel: label,
          // the card fill, the ember glow over it, the shadow outside only (a BoxDecoration paints its gradient instead
          // of its colour, and a BoxShadow under the see-through glow showed as a dark bar)
          child: KOuterShadow(
            shadows: k.shadowCard,
            borderRadius: BorderRadius.circular(28),
            child: DecoratedBox(
              decoration: BoxDecoration(color: k.cardBg, borderRadius: BorderRadius.circular(28)),
              child: Container(
                height: 56,
                padding: const EdgeInsetsDirectional.fromSTEB(8, 8, 8, 8),
                decoration: BoxDecoration(
                  borderRadius: BorderRadius.circular(28),
                  border: Border.all(color: k.cardBorder),
                  gradient: RadialGradient(
                    center: AlignmentDirectional.topStart.resolve(Directionality.of(context)),
                    radius: 2.4,
                    colors: [k.ember.withValues(alpha: 0.11), k.ember.withValues(alpha: 0)],
                    stops: const [0, 0.58],
                  ),
                ),
                child: Row(
                  children: [
                    const AiSpark(),
                    const SizedBox(width: 12),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        mainAxisAlignment: MainAxisAlignment.center,
                        children: [
                          Text(label, maxLines: 1, overflow: TextOverflow.ellipsis, style: context.text.headline.copyWith(fontSize: 14)),
                          Text(
                            // the last turn on one line (web: a truncated span; the bold markers left out)
                            e.turns.isNotEmpty
                                ? e.turns.last.text.replaceAll('**', '').replaceAll(RegExp(r'\s+'), ' ')
                                : t('support.composer.ask', {'name': e.botName}),
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                          ),
                        ],
                      ),
                    ),
                    const SizedBox(width: 8),
                    Container(
                      width: 40,
                      height: 40,
                      alignment: Alignment.center,
                      decoration: BoxDecoration(
                        color: k.ember,
                        shape: BoxShape.circle,
                        boxShadow: [BoxShadow(color: k.ember.withValues(alpha: 0.5), offset: const Offset(0, 10), blurRadius: 22, spreadRadius: -12)],
                      ),
                      child: Icon(LucideIcons.arrowUp, size: 18, color: k.onEmber),
                    ),
                  ],
                ),
              ),
            ),
          ),
        );
      },
    );
  }

  /// The glass bar on Home's photo: the spark, "Ask Kalks AI anything…" (or the last turn), the orange send disc; the
  /// suggestions under it as glass pills that ask at once.
  Widget _hero(BuildContext context, AskAiEngine e, String label) {
    final t = context.t;
    final k = context.k;
    final hint = e.turns.isNotEmpty ? e.turns.last.text.replaceAll('**', '').replaceAll(RegExp(r'\s+'), ' ') : t('support.composer.ask', {'name': e.botName});
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        KPressable(
          key: const ValueKey('ask-ai-pill'),
          onTap: _open,
          pressedScale: 0.99,
          semanticLabel: label,
          child: Container(
            height: 54,
            padding: const EdgeInsetsDirectional.fromSTEB(8, 6, 6, 6),
            decoration: BoxDecoration(
              color: Colors.black.withValues(alpha: 0.5),
              borderRadius: BorderRadius.circular(27),
              border: Border.all(color: Colors.white.withValues(alpha: 0.16)),
              boxShadow: const [BoxShadow(color: Color(0x66000000), offset: Offset(0, 18), blurRadius: 40, spreadRadius: -18)],
            ),
            child: Row(
              children: [
                const AiSpark(size: 36),
                const SizedBox(width: 10),
                Expanded(
                  child: Text(
                    hint,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: context.text.body.copyWith(color: Colors.white.withValues(alpha: 0.72), fontSize: 15),
                  ),
                ),
                const SizedBox(width: 8),
                Container(
                  width: 42,
                  height: 42,
                  alignment: Alignment.center,
                  decoration: BoxDecoration(
                    color: k.ember,
                    shape: BoxShape.circle,
                    boxShadow: [BoxShadow(color: k.ember.withValues(alpha: 0.6), offset: const Offset(0, 8), blurRadius: 22, spreadRadius: -10)],
                  ),
                  child: Icon(LucideIcons.arrowUp, size: 19, color: k.onEmber),
                ),
              ],
            ),
          ),
        ),
        if (widget.chips.isNotEmpty) ...[
          const SizedBox(height: 12),
          SizedBox(
            height: 32,
            child: ListView.separated(
              scrollDirection: Axis.horizontal,
              itemCount: widget.chips.length,
              separatorBuilder: (_, _) => const SizedBox(width: 8),
              itemBuilder: (context, i) {
                final c = widget.chips[i];
                return KPressable(
                  key: ValueKey('ask-ai-chip-${c.key}'),
                  onTap: () => _open(ask: c),
                  semanticLabel: c.label,
                  child: Container(
                    height: 32,
                    padding: const EdgeInsets.symmetric(horizontal: 13),
                    alignment: Alignment.center,
                    decoration: BoxDecoration(
                      color: Colors.white.withValues(alpha: 0.12),
                      borderRadius: BorderRadius.circular(16),
                      border: Border.all(color: Colors.white.withValues(alpha: 0.2)),
                    ),
                    child: Text(
                      c.label,
                      style: context.text.label.copyWith(color: Colors.white, fontSize: 12.5, fontWeight: FontWeight.w500),
                    ),
                  ),
                );
              },
            ),
          ),
        ],
      ],
    );
  }
}

/// The phone bottom sheet: short with the suggestions, tall once there's a thread (web Sheet `tall`).
class _AskAiSheet extends StatelessWidget {
  const _AskAiSheet({required this.e, required this.chips, required this.chat, required this.onOpenChat, required this.onClose});
  final AskAiEngine e;
  final ValueListenable<List<AiChip>> chips;
  final bool chat;
  final VoidCallback onOpenChat;
  final VoidCallback onClose;

  @override
  Widget build(BuildContext context) {
    final h = MediaQuery.sizeOf(context).height;
    // web: h-[min(86dvh,680px)] including the grabber and the bottom padding
    final tallH = math.min(h * 0.86, 680.0) - 21 - 16;
    return ListenableBuilder(
      listenable: Listenable.merge([e, chips]),
      builder: (context, _) {
        final panel = AskAiPanel(e: e, chips: chips.value, chat: chat, onOpenChat: onOpenChat, onClose: onClose);
        return Padding(
          padding: const EdgeInsets.fromLTRB(16, 2, 16, 16),
          child: AnimatedSize(
            duration: const Duration(milliseconds: 300),
            curve: Curves.easeOutCubic,
            alignment: Alignment.bottomCenter,
            child: e.hasThread ? SizedBox(height: tallH, child: panel) : panel,
          ),
        );
      },
    );
  }
}

/// The sheet's body (web Panel, variant "sheet").
class AskAiPanel extends StatefulWidget {
  const AskAiPanel({super.key, required this.e, required this.chips, required this.onOpenChat, this.onClose, this.chat = true});
  final AskAiEngine e;
  final List<AiChip> chips;

  /// The live chat is on (module `support_chat`): the chat button, Continue in chat and Talk to a person show.
  final bool chat;
  final VoidCallback onOpenChat;
  final VoidCallback? onClose;

  @override
  State<AskAiPanel> createState() => _AskAiPanelState();
}

class _AskAiPanelState extends State<AskAiPanel> {
  final TextEditingController _q = TextEditingController();
  final FocusNode _focus = FocusNode();
  final ScrollController _scroll = ScrollController();
  final Map<String, GlobalKey> _keys = {};
  String _sig = '';

  @override
  void initState() {
    super.initState();
    _focus.addListener(() => setState(() {}));
  }

  @override
  void dispose() {
    _q.dispose();
    _focus.dispose();
    _scroll.dispose();
    super.dispose();
  }

  void _submit(String text, {String? chip}) {
    if (text.trim().isEmpty) return;
    widget.e.ask(text, chip: chip);
    _q.clear();
  }

  /// Keeps the latest question at the top of the thread, so a long answer is read from its first line.
  void _follow(AskAiEngine e) {
    final sig = '${e.turns.length}|${e.streaming != null}';
    if (sig == _sig) return;
    _sig = sig;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || !_scroll.hasClients) return;
      final last = e.turns.lastWhere(
        (x) => x.role == 'you',
        orElse: () => const AiTurn(id: '', role: '', text: ''),
      );
      final ctx = _keys[last.id]?.currentContext;
      if (ctx != null) {
        Scrollable.ensureVisible(ctx, duration: const Duration(milliseconds: 300), curve: Curves.easeOutCubic);
      } else {
        _scroll.animateTo(_scroll.position.maxScrollExtent, duration: const Duration(milliseconds: 300), curve: Curves.easeOutCubic);
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final e = widget.e;
    final has = e.hasThread;
    final byKey = {for (final c in widget.chips) c.key: c};
    final lastYou = e.turns.any((x) => x.role == 'you');
    if (has) _follow(e);

    final header = Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const AiSpark(),
        const SizedBox(width: 12),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(t('dashboard.ai.title', {'name': e.botName}), style: context.text.title2.copyWith(letterSpacing: -0.25)),
              const SizedBox(height: 2),
              Text(t('dashboard.ai.subtitle'), style: context.text.footnote.copyWith(color: k.fg3, height: 1.3)),
            ],
          ),
        ),
        const SizedBox(width: 8),
        if (widget.chat)
          KIconButton(icon: LucideIcons.messageCircle, size: 36, filled: true, semanticLabel: t('dashboard.ai.openChat'), onPressed: widget.onOpenChat),
        if (widget.onClose != null) KIconButton(icon: LucideIcons.x, size: 36, semanticLabel: t('common.close'), color: k.fg3, onPressed: widget.onClose),
      ],
    );

    final thread = ListView(
      key: const ValueKey('ai-thread'),
      controller: _scroll,
      padding: const EdgeInsets.only(top: 16, bottom: 4),
      children: [
        for (final turn in e.turns)
          Padding(
            key: turn.role == 'you' ? _keys.putIfAbsent(turn.id, GlobalKey.new) : null,
            padding: const EdgeInsets.only(bottom: 14),
            child: _Bubble(turn: turn, botName: e.botName, extra: turn.role == 'you' && turn.chip != null ? byKey[turn.chip]?.extra : null),
          ),
        if (e.streaming != null)
          Padding(
            key: const ValueKey('ai-streaming'),
            padding: const EdgeInsets.only(bottom: 14),
            child: _BotLine(
              name: e.botName,
              avatar: const AiSpark(size: 28),
              child: e.streaming!.isEmpty
                  ? TypingDots(label: t('dashboard.ai.thinking', {'name': e.botName}))
                  : ChatRich(e.streaming!, style: _aiText(context)),
            ),
          ),
        // a question held while a request for a person is open
        if (e.blocked)
          Container(
            key: const ValueKey('ai-blocked'),
            margin: const EdgeInsets.only(bottom: 14),
            padding: const EdgeInsets.fromLTRB(14, 12, 14, 12),
            decoration: BoxDecoration(
              color: k.surface,
              borderRadius: BorderRadius.circular(16),
              border: Border.all(color: k.line),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(t('dashboard.ai.blocked', {'name': e.botName}), style: context.text.footnote.copyWith(color: k.fg2)),
                const SizedBox(height: 10),
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    KButton(label: t('dashboard.ai.closeAndAsk', {'name': e.botName}), size: KButtonSize.sm, onPressed: e.sending ? null : e.closeAndAsk),
                    KButton(
                      label: t('dashboard.ai.sendToTeam'),
                      size: KButtonSize.sm,
                      variant: KButtonVariant.surface,
                      onPressed: e.sending ? null : e.sendToTeam,
                    ),
                    KButton(label: t('dashboard.ai.view'), size: KButtonSize.sm, variant: KButtonVariant.ghost, onPressed: widget.onOpenChat),
                  ],
                ),
              ],
            ),
          ),
        // handed over to people (by the bot or the client): no bot answer is coming
        if (e.withTeam && !e.waiting)
          Padding(
            padding: const EdgeInsets.only(bottom: 14),
            child: _TeamNote(
              text: e.agentName != null ? t('dashboard.ai.withAgent', {'name': e.agentName}) : t('dashboard.ai.passed'),
              onView: widget.onOpenChat,
            ),
          ),
        if (e.slow)
          Container(
            margin: const EdgeInsets.only(bottom: 14),
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8),
            decoration: BoxDecoration(color: k.warnSoft, borderRadius: BorderRadius.circular(14)),
            child: Text(t('dashboard.ai.slow'), style: context.text.footnote.copyWith(color: k.fg2, fontSize: 12)),
          ),
      ],
    );

    final canSend = !e.sending && !e.blocked;
    final composer = AnimatedContainer(
      duration: const Duration(milliseconds: 160),
      padding: const EdgeInsetsDirectional.fromSTEB(16, 6, 6, 6),
      decoration: BoxDecoration(
        color: k.surface,
        borderRadius: BorderRadius.circular(16),
        border: Border.all(color: _focus.hasFocus ? k.ember.withValues(alpha: 0.5) : k.line),
        boxShadow: _focus.hasFocus
            ? [BoxShadow(color: k.ember.withValues(alpha: 0.1), spreadRadius: 4)]
            : const [BoxShadow(color: Color(0x0A301C40), offset: Offset(0, 1), blurRadius: 2)],
      ),
      child: Row(
        children: [
          Expanded(
            child: TextField(
              key: const ValueKey('ai-input'),
              controller: _q,
              focusNode: _focus,
              autofocus: !has,
              textInputAction: TextInputAction.send,
              inputFormatters: [LengthLimitingTextInputFormatter(4000)],
              onSubmitted: _submit,
              style: context.text.body.copyWith(fontSize: 14),
              cursorColor: k.ember,
              decoration: InputDecoration(
                isCollapsed: true,
                border: InputBorder.none,
                hintText: lastYou ? t('dashboard.ai.followUp') : t('dashboard.ai.placeholder'),
                hintMaxLines: 1,
                hintStyle: context.text.body.copyWith(fontSize: 14, color: k.fg3),
                contentPadding: const EdgeInsets.symmetric(vertical: 9),
              ),
            ),
          ),
          const SizedBox(width: 8),
          ValueListenableBuilder<TextEditingValue>(
            valueListenable: _q,
            builder: (context, v, _) {
              final enabled = canSend && v.text.trim().isNotEmpty;
              return KPressable(
                onTap: enabled ? () => _submit(_q.text) : null,
                semanticLabel: t('common.send'),
                minSize: 40,
                child: AnimatedOpacity(
                  duration: const Duration(milliseconds: 150),
                  opacity: enabled ? 1 : 0.4,
                  child: Container(
                    width: 36,
                    height: 36,
                    alignment: Alignment.center,
                    decoration: BoxDecoration(color: k.ember, borderRadius: BorderRadius.circular(12)),
                    child: Icon(LucideIcons.arrowUp, size: 16, color: k.onEmber),
                  ),
                ),
              );
            },
          ),
        ],
      ),
    );

    return Column(
      key: const ValueKey('ask-ai-panel'),
      mainAxisSize: has ? MainAxisSize.max : MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        header,
        // an open request for a person (the bot doesn't answer there): say so, and open it in the chat
        if (e.openRequest && !e.blocked)
          Padding(
            padding: const EdgeInsets.only(top: 12),
            child: _TeamNote(
              text: e.agentName != null ? t('dashboard.ai.withAgent', {'name': e.agentName}) : t('dashboard.ai.openRequest'),
              onView: widget.onOpenChat,
            ),
          ),
        if (has) Expanded(child: thread),
        if (e.error != null)
          Container(
            margin: const EdgeInsets.only(top: 12),
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8),
            decoration: BoxDecoration(color: k.downSoft, borderRadius: BorderRadius.circular(14)),
            child: Text(e.error!, style: context.text.footnote.copyWith(color: k.down)),
          ),
        // follow-up actions once an answer is in
        if (e.answered)
          Padding(
            padding: const EdgeInsets.only(top: 12),
            child: Wrap(
              spacing: 8,
              runSpacing: 4,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: [
                if (widget.chat)
                  KButton(
                    label: t('dashboard.ai.continueChat'),
                    icon: LucideIcons.messageCircle,
                    variant: KButtonVariant.ink,
                    size: KButtonSize.sm,
                    onPressed: widget.onOpenChat,
                  ),
                if (widget.chat && !e.human)
                  KButton(
                    label: t('support.menu.talkToPerson'),
                    icon: LucideIcons.userRound,
                    variant: KButtonVariant.surface,
                    size: KButtonSize.sm,
                    onPressed: e.handover,
                  ),
                KButton(
                  label: t('dashboard.ai.newQuestion'),
                  icon: LucideIcons.rotateCcw,
                  variant: KButtonVariant.ghost,
                  size: KButtonSize.sm,
                  onPressed: e.reset,
                ),
              ],
            ),
          ),
        SizedBox(height: has ? 12 : 16),
        composer,
        // suggestions before the first question
        if (!has)
          Padding(
            padding: const EdgeInsets.only(top: 12),
            child: Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                for (final c in widget.chips)
                  KPressable(
                    key: ValueKey('ai-chip-${c.key}'),
                    onTap: () => _submit(c.question ?? c.label, chip: c.key),
                    minSize: 36,
                    semanticLabel: c.label,
                    child: Container(
                      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 7),
                      decoration: BoxDecoration(color: k.emberSoft, borderRadius: BorderRadius.circular(16)),
                      child: Text(
                        c.label,
                        style: context.text.footnote.copyWith(color: k.ember, fontWeight: FontWeight.w600),
                      ),
                    ),
                  ),
              ],
            ),
          ),
        if (has)
          Padding(
            padding: const EdgeInsets.only(top: 8),
            child: Text(
              t('support.disclaimer', {'name': e.botName}),
              textAlign: TextAlign.center,
              style: context.text.micro.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
            ),
          ),
      ],
    );
  }
}

TextStyle _aiText(BuildContext context) => context.text.callout.copyWith(fontSize: 13.5, height: 1.5, color: context.k.fg2);

/// A bot or agent line: avatar, name, the bubble (web Bubble, not "you").
class _BotLine extends StatelessWidget {
  const _BotLine({required this.name, required this.avatar, required this.child, this.cites = const []});
  final String name;
  final Widget avatar;
  final Widget child;
  final List<({String slug, String title})> cites;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return LayoutBuilder(
      builder: (context, c) => Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          avatar,
          const SizedBox(width: 10),
          Flexible(
            child: ConstrainedBox(
              constraints: BoxConstraints(maxWidth: c.maxWidth * 0.88),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisSize: MainAxisSize.min,
                children: [
                  Padding(
                    padding: const EdgeInsets.only(bottom: 4),
                    child: Text(
                      name,
                      style: context.text.caption.copyWith(color: k.fg2, fontWeight: FontWeight.w600),
                    ),
                  ),
                  Container(
                    padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
                    decoration: BoxDecoration(
                      color: k.surface2,
                      borderRadius: const BorderRadiusDirectional.only(
                        topStart: Radius.circular(6),
                        topEnd: Radius.circular(18),
                        bottomStart: Radius.circular(18),
                        bottomEnd: Radius.circular(18),
                      ).resolve(Directionality.of(context)),
                    ),
                    child: child,
                  ),
                  if (cites.isNotEmpty) CitePills(cites, bordered: false),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _Bubble extends StatelessWidget {
  const _Bubble({required this.turn, required this.botName, this.extra});
  final AiTurn turn;
  final String botName;
  final Widget? extra;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    if (turn.role == 'system') {
      return Center(
        child: Container(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
          decoration: BoxDecoration(color: k.surface2, borderRadius: BorderRadius.circular(14)),
          child: Text(
            turn.text,
            textAlign: TextAlign.center,
            style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
          ),
        ),
      );
    }
    if (turn.role == 'you') {
      return LayoutBuilder(
        builder: (context, c) => Column(
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            ConstrainedBox(
              constraints: BoxConstraints(maxWidth: c.maxWidth * 0.85),
              child: Container(
                padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8),
                decoration: BoxDecoration(
                  color: k.ember,
                  borderRadius: const BorderRadiusDirectional.only(
                    topStart: Radius.circular(18),
                    topEnd: Radius.circular(18),
                    bottomStart: Radius.circular(18),
                    bottomEnd: Radius.circular(6),
                  ).resolve(Directionality.of(context)),
                ),
                child: Text(turn.text, style: _aiText(context).copyWith(color: k.onEmber)),
              ),
            ),
            if (extra != null) ...[const SizedBox(height: 8), extra!],
          ],
        ),
      );
    }
    final agent = turn.role == 'agent';
    return _BotLine(
      name: agent ? (turn.name ?? '') : botName,
      avatar: agent ? KAvatar(name: turn.name ?? 'Support', size: 28) : const AiSpark(size: 28),
      cites: turn.cites,
      child: ChatRich(turn.text, style: _aiText(context)),
    );
  }
}

/// "Your request for a person is still open · View" and similar team notes.
class _TeamNote extends StatelessWidget {
  const _TeamNote({required this.text, required this.onView});
  final String text;
  final VoidCallback onView;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final rtl = Directionality.of(context) == TextDirection.rtl;
    return Container(
      key: const ValueKey('ai-team-note'),
      padding: const EdgeInsetsDirectional.fromSTEB(14, 4, 4, 4),
      decoration: BoxDecoration(color: k.infoSoft, borderRadius: BorderRadius.circular(14)),
      child: Row(
        children: [
          Icon(LucideIcons.userRound, size: 16, color: k.info),
          const SizedBox(width: 10),
          Expanded(
            child: Padding(
              padding: const EdgeInsets.symmetric(vertical: 6),
              child: Text(text, style: context.text.footnote.copyWith(color: k.fg2)),
            ),
          ),
          KPressable(
            onTap: onView,
            semanticLabel: context.t('dashboard.ai.view'),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 10),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(
                    context.t('dashboard.ai.view'),
                    style: context.text.footnote.copyWith(color: k.info, fontWeight: FontWeight.w600),
                  ),
                  const SizedBox(width: 4),
                  Icon(rtl ? LucideIcons.chevronLeft : LucideIcons.chevronRight, size: 14, color: k.info),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}
