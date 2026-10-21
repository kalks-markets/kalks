"use client";

import * as React from "react";
import Link from "next/link";
import { ArrowUpRight, Clock, Gift, Mail, Monitor, Phone, ShieldCheck, UserRound } from "lucide-react";
import { Avatar, Card, CardHeader, Chip, CopyButton, DataTable, EmptyState, Flag, KeyValue, Skeleton, cn } from "@kalks/ui";
import { useCan } from "@/components/staff-session";
import { EmailChip, ErrorState, KycChip, Mono, actionLabel, actionTone, ago, countryName, day, device, useApi, useNow, when } from "./kit";
import { RevokeDialog, sessionColumns } from "./sessions";
import { ClientSecurityCard } from "./client-security";
import { ClientKycCard } from "@/components/kyc/client-kyc-card";
import { ClientBalanceCard } from "@/components/clients/balance-card";
import type { ClientDetail, Session, SessionsPage } from "./types";

/** UTM source / medium / campaign, landing page, referrer and marketing-email consent. */
function attributionRows(a: ClientDetail["attribution"]): [string, React.ReactNode][] {
  if (!a) return [];
  const utm = [a.utm_source, a.utm_medium].filter(Boolean).join(" / ");
  return [
    ["UTM source / medium", utm ? <Mono key="us">{utm}</Mono> : <span key="us" className="text-fg-3">None (direct or organic)</span>],
    ["UTM campaign", a.utm_campaign ? <Mono key="uc">{a.utm_campaign}</Mono> : "—"],
    ...(a.utm_term || a.utm_content ? ([["Term / content", <Mono key="ut">{[a.utm_term, a.utm_content].filter(Boolean).join(" · ")}</Mono>]] as [string, React.ReactNode][]) : []),
    ...(a.partner_campaign ? ([["Partner campaign", <Mono key="pc">{a.partner_campaign}</Mono>]] as [string, React.ReactNode][]) : []),
    ["Landing page", a.landing_page ? <Mono key="lp" className="break-all">{a.landing_page}</Mono> : "—"],
    ["Referring site", a.referrer ? <Mono key="rf">{a.referrer}</Mono> : "—"],
    [
      "Marketing emails",
      a.marketing_consent ? (
        <Chip key="mc" size="sm" tone="up">Allowed</Chip>
      ) : (
        <span key="mc" className="inline-flex items-center gap-2">
          <Chip size="sm" tone="warn">Unsubscribed</Chip>
          {a.marketing_unsubscribed_at && <span className="text-[11.5px] text-fg-3">{when(a.marketing_unsubscribed_at)}</span>}
        </span>
      ),
    ],
  ];
}

const DOT: Record<string, string> = { up: "bg-up", down: "bg-down", warn: "bg-warn", ember: "bg-ember" };

function age(dob: string) {
  const d = new Date(dob + "T00:00:00Z");
  const n = new Date();
  let a = n.getUTCFullYear() - d.getUTCFullYear();
  if (n.getUTCMonth() < d.getUTCMonth() || (n.getUTCMonth() === d.getUTCMonth() && n.getUTCDate() < d.getUTCDate())) a -= 1;
  return a;
}

export function ClientHeader({ d }: { d: ClientDetail }) {
  const u = d.user;
  return (
    <div className="flex flex-wrap items-center gap-4">
      <Avatar name={u.name} size={56} verified={u.email_verified} />
      <div className="min-w-0">
        <div className="flex flex-wrap items-center gap-2">
          <div className="truncate text-[22px] font-medium tracking-tight">{u.name}</div>
          <Flag country={u.country} />
        </div>
        <div className="mt-0.5 flex items-center gap-1 text-[13px] text-fg-3">
          {u.email}
          <CopyButton value={u.email} label="Email" />
          <span className="mx-1">·</span>
          <Mono>ID {u.id}</Mono>
        </div>
        <div className="mt-2 flex flex-wrap gap-1.5">
          <EmailChip verified={u.email_verified} />
          <KycChip status={u.kyc_status} />
          {u.erased_at ? (
            <Chip size="sm" tone="down" dot>
              <span title={`Personal data erased ${when(u.erased_at)}`}>Erased</span>
            </Chip>
          ) : (
            u.status !== "active" && (
              <Chip size="sm" tone="down" dot>
                {u.status === "blocked" ? "Blocked" : "Closed"}
              </Chip>
            )
          )}
          {u.locked && (
            <Chip size="sm" tone="down" dot>
              Locked after failed sign-ins
            </Chip>
          )}
          {/* presence (client_controls): Online = active in the last 2 minutes, Away = 2–15 minutes */}
          {u.presence === "online" || u.presence === "away" ? (
            <Chip size="sm" tone={u.presence === "online" ? "up" : "warn"} dot>
              {u.presence === "online" ? "Online" : "Away"}
            </Chip>
          ) : d.sessions.active > 0 ? (
            <Chip size="sm" tone="neutral" dot>
              Signed in · {d.sessions.active} session{d.sessions.active > 1 ? "s" : ""}
            </Chip>
          ) : null}
        </div>
      </div>
    </div>
  );
}

function Section({ title, icon, children, action }: { title: string; icon: React.ReactNode; children: React.ReactNode; action?: React.ReactNode }) {
  return (
    <Card>
      <CardHeader title={title} icon={icon} action={action} />
      <div className="px-4 pb-5 pt-2 sm:px-6">{children}</div>
    </Card>
  );
}

/** Full client record from the gateway: profile, referral, sessions and activity. `compact` = drawer layout. */
export function ClientDetailView({ id, compact = false }: { id: number; compact?: boolean }) {
  const now = useNow();
  const canSessions = useCan("clients.read");
  const canRevoke = useCan("sessions.revoke");
  const { data: d, error, reload } = useApi<ClientDetail>(`/api/admin/users/${id}`);
  const sessions = useApi<SessionsPage>(canSessions ? `/api/admin/sessions?kind=user&subject=${id}&per_page=50` : null);
  const [revoking, setRevoking] = React.useState<Session | null>(null);

  if (error) return <ErrorState error={error} onRetry={reload} />;
  if (!d)
    return (
      <div className="space-y-4">
        <Skeleton className="h-20 w-full" />
        <Skeleton className="h-64 w-full" />
        <Skeleton className="h-40 w-full" />
      </div>
    );
  const u = d.user;

  const profile = (
    <Section title="Profile" icon={<UserRound />}>
      <KeyValue
        rows={[
          ["Client ID", <Mono key="id">{u.id}</Mono>],
          ["Email", <span key="e" className="inline-flex items-center gap-1"><Mail className="size-3.5 text-fg-3" />{u.email}</span>],
          ["Phone", u.erased_at ? <span key="p" className="text-fg-3">Erased</span> : <span key="p"className="inline-flex items-center gap-1"><Phone className="size-3.5 text-fg-3" /><Mono>{u.phone_dial} {u.phone}</Mono><CopyButton value={`${u.phone_dial}${u.phone}`} label="Phone" /></span>],
          ["Country", <span key="c" className="inline-flex items-center gap-2"><Flag country={u.country} className="size-4" />{countryName(u.country)}</span>],
          ["Date of birth", u.erased_at ? <span key="dob" className="text-fg-3">Erased</span> : `${day(u.date_of_birth)} · ${age(u.date_of_birth)} years`],
          ...(u.erased_at ? ([["Personal data erased", <span key="er" className="text-down">{when(u.erased_at)}</span>]] as [string, React.ReactNode][]) : []),
          ["Registered", when(u.created_at)],
          ["Terms accepted", when(u.terms_accepted_at)],
          ["Email verified", u.email_verified_at ? when(u.email_verified_at) : <span key="v" className="text-warn">Not yet</span>],
          ["KYC", <KycChip key="k" status={u.kyc_status} />],
        ]}
      />
    </Section>
  );

  const security = (
    <Section title="Sign-in & security" icon={<ShieldCheck />}>
      <KeyValue
        rows={[
          ["Last sign-in", d.last_login ? <span key="l" title={when(d.last_login.at, true)}>{ago(d.last_login.at, now)}</span> : "Never"],
          ["Last sign-in IP", <Mono key="ip">{d.last_login?.ip ?? "—"}</Mono>],
          ["Last device", d.last_login ? device(d.last_login.user_agent) : "—"],
          ["Active sessions", `${d.sessions.active} of ${d.sessions.total} total`],
          ["Trusted devices", d.trusted_devices],
          ["Failed sign-ins", <span key="f" className={cn(u.failed_logins > 0 && "text-warn")}>{u.failed_logins}</span>],
          ["Locked until", u.locked ? <span key="lu" className="text-down">{when(u.locked_until)}</span> : "Not locked"],
        ]}
      />
    </Section>
  );

  const referral = (
    <Section title="Referral & attribution" icon={<Gift />}>
      <KeyValue
        rows={[
          ["Own referral code", <span key="rc" className="inline-flex items-center gap-1"><Mono>{u.referral_code}</Mono><CopyButton value={u.referral_code} label="Referral code" /></span>],
          [
            "Referred by",
            d.referrer ? (
              <Link key="rb" href={`/clients/${d.referrer.id}`} className="inline-flex items-center gap-1 text-ember hover:underline">
                {d.referrer.name} <ArrowUpRight className="size-3.5" />
              </Link>
            ) : u.referred_code_raw ? (
              <span key="raw" className="text-fg-3">Code <Mono>{u.referred_code_raw}</Mono> (no match)</span>
            ) : (
              "Direct sign-up"
            ),
          ],
          ["Clients referred", d.referrals.total],
          ...attributionRows(d.attribution),
        ]}
      />
      {d.referrals.items.length > 0 && (
        <div className="mt-3 divide-y divide-line rounded-[14px] border border-line">
          {d.referrals.items.slice(0, compact ? 5 : 50).map((r) => (
            <Link key={r.id} href={`/clients/${r.id}`} className="flex items-center justify-between gap-3 px-3 py-2 hover:bg-surface-2">
              <span className="flex min-w-0 items-center gap-2.5">
                <Avatar name={r.name} size={26} verified={r.email_verified} />
                <span className="min-w-0">
                  <span className="block truncate text-[13px] font-medium">{r.name}</span>
                  <span className="block truncate text-[11.5px] text-fg-3">{r.email}</span>
                </span>
              </span>
              <span className="shrink-0 text-[11.5px] text-fg-3">{day(r.created_at)}</span>
            </Link>
          ))}
        </div>
      )}
    </Section>
  );

  const sessionsCard = (
    <Section title="Active sessions" icon={<Monitor />}>
      {sessions.error ? (
        <ErrorState error={sessions.error} onRetry={sessions.reload} className="py-6" />
      ) : !sessions.data ? (
        <Skeleton className="h-24 w-full" />
      ) : sessions.data.items.length === 0 ? (
        <EmptyState title="Not signed in" text="This client has no active session right now." illustration="locked" className="py-6" />
      ) : (
        <DataTable rows={sessions.data.items} rowKey={(s) => String(s.id)} dense pageSize={20} columns={sessionColumns({ now, canRevoke, onRevoke: setRevoking, showSubject: false })} />
      )}
    </Section>
  );

  const activity = (
    <Section title="Activity" icon={<Clock />} action={<Link href={`/security?actor=user:${u.id}`} className="inline-flex items-center gap-1 text-[12.5px] text-fg-3 hover:text-fg">Full audit trail <ArrowUpRight className="size-3.5" /></Link>}>
      {d.events.length === 0 ? (
        <EmptyState title="No activity yet" illustration="calendar" className="py-6" />
      ) : (
        <ol className="relative space-y-3 border-l border-line pl-5">
          {d.events.slice(0, compact ? 10 : 30).map((e) => (
            <li key={e.id} className="relative">
              <span className={cn("absolute -left-[25px] top-1.5 size-2.5 rounded-full ring-4 ring-surface", DOT[actionTone(e.action)] ?? "bg-fg-3")} />
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="text-[13px] font-medium">{actionLabel(e.action)}</span>
                <span className="text-[11.5px] text-fg-3" title={when(e.created_at, true)}>
                  {ago(e.created_at, now)}
                </span>
              </div>
              <div className="mt-0.5 text-[11.5px] text-fg-3">
                <Mono className="text-[11.5px]">{e.ip ?? "—"}</Mono> · {device(e.user_agent)}
                {e.actor.kind === "staff" && " · by staff"}
              </div>
            </li>
          ))}
        </ol>
      )}
    </Section>
  );

  return (
    <div className="space-y-4">
      {compact && <ClientHeader d={d} />}
      {compact ? (
        <>
          {profile}
          <ClientKycCard userId={u.id} kycStatus={u.kyc_status} />
          {security}
          {sessionsCard}
          <ClientSecurityCard userId={u.id} onChanged={() => (sessions.reload(), reload())} />
          {referral}
          {activity}
        </>
      ) : (
        <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
          <div className="space-y-4 xl:col-span-7">
            {profile}
            {sessionsCard}
            {activity}
          </div>
          <div className="space-y-4 xl:col-span-5">
            {security}
            <ClientSecurityCard userId={u.id} onChanged={() => (sessions.reload(), reload())} />
            <ClientKycCard userId={u.id} kycStatus={u.kyc_status} />
            {referral}
            <ClientBalanceCard userId={u.id} clientName={u.name} />
          </div>
        </div>
      )}
      <RevokeDialog
        session={revoking}
        onClose={() => setRevoking(null)}
        onDone={() => {
          sessions.reload();
          reload();
        }}
      />
    </div>
  );
}
