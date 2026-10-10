// Profile & Security › Profile: port of the web's live profile (apps/crm/components/profile/live-profile.tsx), the
// client's real record from the gateway, read-only (there is no profile-update API yet). Phone order:
//   1 header (Profile · "Your personal details as registered")
//   2 identity card: avatar, name + KYC chip, contacts, Client ID / email status
//   3 Personal information (+ Request a correction by email) and the identity-lock note
//   4 Change password (step-up `account_password`)
//   5 Sign-in & security facts + Reset password
//   6 Identity verification link card, 7 Notifications link card
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../core/auth/auth_controller.dart';
import '../../core/config/app_config.dart';
import '../../core/models/user.dart';
import '../../i18n/i18n.dart';
import '../../shell/page_hero.dart';
import '../../ui/ui.dart';
import 'widgets/change_password_card.dart';
import 'widgets/profile_ui.dart';

/// web KYC_CHIP: the client's KYC status as a chip (tone, label).
(KChipTone, String) kycChip(KycStatus s, T t) => switch (s) {
  KycStatus.unverified => (KChipTone.warn, t.dyn('shell.kyc.unverified', fallback: 'Verify your identity')),
  KycStatus.pending => (KChipTone.warn, t.dyn('shell.kyc.pending', fallback: 'KYC in review')),
  KycStatus.verified => (KChipTone.up, t.dyn('shell.kyc.verified', fallback: 'Verified')),
  KycStatus.rejected => (KChipTone.down, t.dyn('shell.kyc.rejected', fallback: 'KYC rejected')),
};

/// The identity-verification link card's chip and line (web live-profile).
({KChipTone tone, String chip, String text}) kycCardState(SessionUser me, T t) {
  final (tone, label) = kycChip(me.kycStatus, t);
  final moreInfo = me.kycCaseStatus == 'more_info';
  final chip = moreInfo
      ? t('profile.kycCard.actionNeeded')
      : me.kycStatus == KycStatus.unverified
      ? (me.kycCaseStatus == 'draft' ? t('profile.kycCard.inProgress') : t('profile.kycCard.notStarted'))
      : label;
  final text = me.kycStatus == KycStatus.verified
      ? t('profile.kycCard.verified')
      : moreInfo
      ? t('profile.kycCard.moreInfo')
      : me.kycStatus == KycStatus.pending
      ? t('profile.kycCard.pending')
      : t('profile.kycCard.unverified');
  return (tone: moreInfo ? KChipTone.warn : tone, chip: chip, text: text);
}

class ProfileScreen extends ConsumerWidget {
  const ProfileScreen({super.key, this.query = const {}});

  /// The route's query parameters (the web page's search params).
  final Map<String, String> query;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final me = ref.watch(meProvider);
    if (me == null) return const SizedBox.shrink();
    final cfg = ref.watch(configProvider);
    final user = me.raw['user'] is Map ? (me.raw['user'] as Map).cast<String, dynamic>() : me.raw;
    final identityLocked = user['identity_locked'] == true;
    final (kycTone, kycLabel) = kycChip(me.kycStatus, t);
    final id = me.clientId;
    final phone = [me.phoneDial, me.phone].where((s) => s.isNotEmpty).join(' ');
    final email = cfg.supportEmail ?? 'support@kalkstrade.com';
    final correction = Uri.parse('mailto:$email?subject=${Uri.encodeComponent('Profile correction · $id')}');
    final dob = fromIsoDate(me.dateOfBirth.length >= 10 ? me.dateOfBirth.substring(0, 10) : me.dateOfBirth);
    final kycCard = kycCardState(me, t);
    final contact = context.text.footnote.copyWith(color: k.fg2, fontSize: 13);

    Widget contactItem(Widget icon, String text, {bool ltr = false}) => Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        icon,
        const SizedBox(width: 6),
        Flexible(
          child: Text(text, style: contact, textDirection: ltr ? TextDirection.ltr : null, maxLines: 1, overflow: TextOverflow.ellipsis),
        ),
      ],
    );

    Widget stat(String label, String value) => Expanded(
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 12),
        decoration: BoxDecoration(
          color: k.surface.withValues(alpha: 0.4),
          borderRadius: BorderRadius.circular(16),
          border: Border.all(color: k.line),
        ),
        child: Column(
          children: [
            Text(
              label,
              style: context.text.footnote.copyWith(color: k.fg3, fontSize: 12),
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
            ),
            const SizedBox(height: 4),
            Text(
              value,
              style: context.text.mono(14, weight: FontWeight.w600),
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
            ),
          ],
        ),
      ),
    );

    final rows = <(String, Widget)>[
      (t('profile.field.firstName'), Text(me.firstName.isEmpty ? '—' : me.firstName)),
      (t('profile.field.lastName'), Text(me.lastName.isEmpty ? '—' : me.lastName)),
      (t('profile.field.dob'), Text(fmtDay(t.locale, dob))),
      (
        t('profile.field.country'),
        Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (me.country.isNotEmpty) ...[KFlag(me.country.toLowerCase(), size: 16), const SizedBox(width: 8)],
            Flexible(child: Text(countryLabel(t, me.country), maxLines: 1, overflow: TextOverflow.ellipsis)),
          ],
        ),
      ),
      (t('common.email'), Text(me.email)),
      (t('common.phone'), Text(phone.isEmpty ? '—' : phone, textDirection: TextDirection.ltr)),
      (t('profile.stat.clientId'), Text(id, style: context.text.mono(13.5))),
      (t('profile.field.registered'), Text(fmtDay(t.locale, me.createdAt))),
    ];

    final hero = pageHero(context, ref, path: '/profile', title: t('profile.title'), lead: t('profile.liveSubtitle'));
    return KPageScroll(
      onRefresh: () => ref.read(authProvider.notifier).refreshMe(),
      hero: hero,
      padding: EdgeInsets.fromLTRB(KSpace.page, hero == null ? 12 : 18, KSpace.page, 24),
      children: [
        if (hero == null) PPageHeader(title: t('profile.title'), subtitle: t('profile.liveSubtitle')),
        // 2 identity
        KCard(
          hot: true,
          padding: const EdgeInsets.all(20),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              PToneAvatar(name: me.name, verified: me.kycStatus == KycStatus.verified),
              const SizedBox(height: 20),
              Wrap(
                spacing: 8,
                runSpacing: 6,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  Text(me.name, style: context.text.title1.copyWith(fontSize: 24, fontWeight: FontWeight.w500)),
                  KChip(label: kycLabel, tone: kycTone, dot: true),
                ],
              ),
              const SizedBox(height: 8),
              Wrap(
                spacing: 20,
                runSpacing: 4,
                children: [
                  contactItem(Icon(LucideIcons.mail, size: 14, color: k.fg2), me.email),
                  if (phone.isNotEmpty) contactItem(Icon(LucideIcons.phone, size: 14, color: k.fg2), phone, ltr: true),
                  if (me.country.isNotEmpty) contactItem(KFlag(me.country.toLowerCase(), size: 16), countryLabel(t, me.country)),
                  contactItem(Icon(LucideIcons.calendarDays, size: 14, color: k.fg2), t('profile.memberSince', {'date': fmtMonthYear(t.locale, me.createdAt)})),
                ],
              ),
              const SizedBox(height: 20),
              Row(
                children: [
                  stat(t('profile.stat.clientId'), id),
                  const SizedBox(width: 12),
                  stat(t('common.email'), me.emailVerified ? t('common.verified') : t('common.unverified')),
                ],
              ),
            ],
          ),
        ),
        const SizedBox(height: 16),
        // 3 personal information
        KCard(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              PCardHeader(
                title: t('profile.personal.title'),
                subtitle: t('profile.personal.liveSubtitle'),
                icon: LucideIcons.userRound,
                action: KButton(
                  label: t('profile.personal.requestCorrection'),
                  icon: LucideIcons.mail,
                  size: KButtonSize.sm,
                  variant: KButtonVariant.surface,
                  onPressed: () => launchUrl(correction, mode: LaunchMode.externalApplication),
                ),
              ),
              const SizedBox(height: 6),
              for (var i = 0; i < rows.length; i++) PRow(label: rows[i].$1, last: i == rows.length - 1, child: rows[i].$2),
              const SizedBox(height: 8),
              Container(
                padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
                decoration: BoxDecoration(
                  color: k.surface2,
                  borderRadius: BorderRadius.circular(12),
                  border: Border.all(color: k.line),
                ),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Padding(
                      padding: const EdgeInsets.only(top: 1),
                      child: Icon(LucideIcons.lock, size: 14, color: k.fg3),
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Text(
                        identityLocked ? t('profile.personal.identityLocked') : t('profile.personal.identityUnlocked'),
                        style: context.text.footnote.copyWith(color: k.fg3, fontSize: 12),
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
        // 4 change password (hidden in read-only sessions)
        if (!me.readOnly) ...[const SizedBox(height: 16), const ChangePasswordCard(forgotToastKey: 'profile.signin.signingOut')],
        const SizedBox(height: 16),
        // 5 sign-in & security
        KCard(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              PCardHeader(title: t('profile.signin.title'), icon: LucideIcons.shieldCheck),
              const SizedBox(height: 6),
              PRow(
                label: t('profile.signin.emailAddress'),
                child: me.emailVerified
                    ? KChip(label: t('common.verified'), tone: KChipTone.up, small: true)
                    : KChip(label: t('profile.notVerified'), tone: KChipTone.warn, small: true),
              ),
              PRow(label: t('profile.signin.method'), child: Text(t('common.password'))),
              PRow(label: t('profile.signin.newDevices'), child: Text(t('profile.signin.emailCodeRequired'))),
              PRow(label: t('profile.signin.sensitive'), last: true, child: Text(t('profile.signin.emailCodeRequired'))),
              const SizedBox(height: 12),
              KButton(
                label: t('profile.signin.resetPassword'),
                icon: LucideIcons.keyRound,
                variant: KButtonVariant.surface,
                expand: true,
                onPressed: () => resetPasswordFlow(context, ref, 'profile.signin.signingOut'),
              ),
              const SizedBox(height: 8),
              Text(
                t('profile.signin.resetHint'),
                style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
              ),
            ],
          ),
        ),
        const SizedBox(height: 16),
        // 6 identity verification
        PLinkCard(
          icon: LucideIcons.idCard,
          title: t('profile.kycCard.title'),
          chip: KChip(label: kycCard.chip, tone: kycCard.tone, dot: true, small: true),
          text: kycCard.text,
          href: '/profile/verification',
        ),
        const SizedBox(height: 16),
        // 7 notifications
        PLinkCard(icon: LucideIcons.bell, title: t('profile.notifCard.title'), text: t('profile.notifCard.hint'), href: '/profile/notifications'),
      ],
    );
  }
}
