"use client";

import * as React from "react";
import Link from "next/link";
import { Ban, KeyRound, LogOut, Mail, MoreHorizontal, RefreshCw, RotateCcw, ShieldCheck, UserCheck, UserPlus, Users } from "lucide-react";
import { toast } from "sonner";
import { Avatar, Button, Card, CardHeader, Chip, DataTable, Dialog, EmptyState, Field, IconButton, Input, KpiCard, Menu, PageHeader, Reveal, Segmented, type Column } from "@kalks/ui";
import { useCan, useStaff } from "@/components/staff-session";
import { ErrorState, TableSkeleton, ago, day, device, useApi, useNow, when } from "@/components/live/kit";
import { useConfirm } from "@/components/confirm";
import { InviteLink, STATUS_TONE, Select, act, call, cap, permLabel, useBusy, type RolesResp } from "./kit";

type Member = {
  id: number;
  email: string;
  name: string;
  role: string;
  role_id: number | null;
  role_label: string;
  role_kind: string;
  status: "active" | "disabled" | "invited";
  locked: boolean;
  invited_at: string | null;
  invite_expires_at: string | null;
  disabled_at: string | null;
  disabled_reason: string | null;
  last_login_at: string | null;
  last_activity_at: string | null;
  created_at: string;
  active_sessions: number;
  trusted_devices: number;
  is_me: boolean;
};

type Detail = {
  staff: Member & { permissions: string[]; password_set: boolean; invited_by: string | null; manageable: boolean };
  sessions: { id: number; ip: string | null; user_agent: string | null; created_at: string; last_seen_at: string; expires_at: string; current: boolean }[];
  trusted_devices: { user_agent: string | null; created_at: string; last_seen_at: string }[];
  events: { id: number; action: string; ip: string | null; created_at: string; meta: Record<string, unknown> }[];
};

type Filter = "all" | "active" | "invited" | "disabled";

function MyAccess({ roles }: { roles?: RolesResp | null }) {
  const me = useStaff();
  const perms = me.permissions ?? [];
  return (
    <Card className="h-full">
      <CardHeader title="Your access" subtitle={`${me.role_label} · ${me.tenant.name}`} icon={<KeyRound />} />
      <div className="px-4 pb-5 pt-3 sm:px-6">
        <div className="flex items-center gap-3">
          <Avatar name={me.name} size={44} online />
          <div className="min-w-0">
            <div className="truncate font-medium">{me.name}</div>
            <div className="truncate text-[12.5px] text-fg-3">{me.email}</div>
          </div>
        </div>
        <div className="mt-4 text-[12px] text-fg-3">{perms.length} permissions</div>
        <div className="mt-2 flex max-h-56 flex-wrap gap-1.5 overflow-y-auto" data-testid="my-permissions">
          {perms.map((p) => (
            <Chip key={p} size="sm" tone={p.startsWith("owner.") ? "ember" : "neutral"}>
              {roles ? permLabel(roles.catalogue, p) : p}
            </Chip>
          ))}
        </div>
        <p className="mt-4 text-[12px] text-fg-3">Sign-in needs your password and a one-time code sent to your work email. Sessions end after 2 hours idle or 12 hours.</p>
      </div>
    </Card>
  );
}

function InviteDialog({ open, onOpenChange, roles, onDone }: { open: boolean; onOpenChange: (o: boolean) => void; roles: RolesResp["items"]; onDone: () => void }) {
  const assignable = roles.filter((r) => r.assignable);
  const [email, setEmail] = React.useState("");
  const [name, setName] = React.useState("");
  const [roleId, setRoleId] = React.useState<string>("");
  const [err, setErr] = React.useState<{ field?: string; message: string } | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [link, setLink] = React.useState<string | undefined>();
  React.useEffect(() => {
    if (open) {
      setEmail("");
      setName("");
      setErr(null);
      setLink(undefined);
      setRoleId(String(assignable.find((r) => r.key === "support")?.id ?? assignable[0]?.id ?? ""));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);
  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    const r = await call<{ invite: { dev_invite_url?: string; email_sent: boolean } }>("POST", "/api/admin/staff/invite", { email, name, role_id: Number(roleId) || null });
    setBusy(false);
    if (!r.ok) return setErr(r.error);
    toast.success("Invite sent", { description: r.data.invite.email_sent ? `${email} will get a link to set a password.` : "Email isn't configured: copy the link below." });
    onDone();
    if (r.data.invite.dev_invite_url) setLink(r.data.invite.dev_invite_url);
    else onOpenChange(false);
  }
  return (
    <Dialog open={open} onOpenChange={onOpenChange} title="Invite staff" description="They get a one-time link to set a password; every sign-in then asks for an emailed code.">
      {link ? (
        <div>
          <p className="text-[14px] text-fg-2">Invite created for {email}. The link works once and expires in 72 hours.</p>
          <InviteLink url={link} />
          <div className="mt-5 flex justify-end">
            <Button variant="surface" onClick={() => onOpenChange(false)}>
              Done
            </Button>
          </div>
        </div>
      ) : (
        <form onSubmit={submit} className="space-y-4" noValidate>
          {err && !err.field && <div className="rounded-[14px] border border-down/25 bg-down-soft px-4 py-3 text-[13px] text-down">{err.message}</div>}
          <Field label="Work email" error={err?.field === "email" ? err.message : undefined}>
            <Input leading={<Mail />} type="email" value={email} onChange={(e) => setEmail(e.target.value)} placeholder="name@broker.com" name="invite-email" />
          </Field>
          <Field label="Full name" error={err?.field === "name" ? err.message : undefined}>
            <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="Jane Doe" name="invite-name" />
          </Field>
          <Field label="Role" error={err?.field === "role_id" ? err.message : undefined}>
            <Select value={roleId} onChange={setRoleId} name="invite-role">
              {assignable.map((r) => (
                <option key={r.id} value={r.id}>
                  {r.name} · {r.permissions.length} permissions
                </option>
              ))}
            </Select>
          </Field>
          <div className="flex justify-end gap-2 pt-1">
            <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" variant="ember" disabled={busy || !email || !name || !roleId}>
              {busy ? "Sending…" : "Send invite"}
            </Button>
          </div>
        </form>
      )}
    </Dialog>
  );
}

function RoleDialog({ member, roles, onClose, onDone }: { member: Member | null; roles: RolesResp["items"]; onClose: () => void; onDone: () => void }) {
  const [roleId, setRoleId] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  React.useEffect(() => setRoleId(String(member?.role_id ?? "")), [member]);
  const options = roles.filter((r) => r.assignable || r.id === member?.role_id);
  return (
    <Dialog open={!!member} onOpenChange={(o) => !o && onClose()} title="Change role" description={member ? `${member.name} · ${member.email}` : undefined}>
      <Field label="Role">
        <Select value={roleId} onChange={setRoleId} name="change-role">
          {options.map((r) => (
            <option key={r.id} value={r.id} disabled={!r.assignable}>
              {r.name}
            </option>
          ))}
        </Select>
      </Field>
      <p className="mt-3 text-[12.5px] text-fg-3">The new permissions apply on their next click; they stay signed in. The change is written to the audit log.</p>
      <div className="mt-5 flex justify-end gap-2">
        <Button variant="ghost" onClick={onClose}>
          Cancel
        </Button>
        <Button
          variant="ember"
          disabled={busy || !member || String(member.role_id) === roleId}
          onClick={async () => {
            if (!member) return;
            setBusy(true);
            const ok = await act("PATCH", `/api/admin/staff/${member.id}`, { role_id: Number(roleId) }, "Role changed");
            setBusy(false);
            if (ok) {
              onDone();
              onClose();
            }
          }}
        >
          Save role
        </Button>
      </div>
    </Dialog>
  );
}

function DetailDrawer({ id, roles, onClose, onChanged }: { id: number | null; roles?: RolesResp | null; onClose: () => void; onChanged: () => void }) {
  const now = useNow();
  const { data, error, reload } = useApi<Detail>(id ? `/api/admin/staff/${id}` : null);
  const s = data?.staff;
  const byModule = React.useMemo(() => {
    if (!s || !roles) return [];
    return roles.catalogue.modules
      .map((m) => ({ m, perms: s.permissions.filter((k) => roles.catalogue.permissions.find((p) => p.key === k)?.module === m.key) }))
      .filter((x) => x.perms.length);
  }, [s, roles]);
  return (
    <Dialog open={id !== null} onOpenChange={(o) => !o && onClose()} side="right" title={s?.name ?? "Staff member"} description={s ? `${s.email} · ${s.role_label}` : undefined}>
      {error ? (
        <ErrorState error={error} onRetry={reload} />
      ) : !s ? (
        <TableSkeleton rows={4} />
      ) : (
        <div className="space-y-6">
          <div className="flex flex-wrap gap-2">
            <Chip tone={STATUS_TONE[s.status] ?? "neutral"} dot>
              {cap(s.status)}
            </Chip>
            {s.locked && (
              <Chip tone="warn" dot>
                Locked
              </Chip>
            )}
            <Chip>{s.permissions.length} permissions</Chip>
            {s.invited_by && <Chip>Invited by {s.invited_by}</Chip>}
          </div>
          <section>
            <h4 className="text-[13px] font-medium text-fg-2">Live sessions</h4>
            {data.sessions.length === 0 ? (
              <p className="mt-2 text-[13px] text-fg-3">Not signed in.</p>
            ) : (
              <ul className="mt-2 divide-y divide-line rounded-[14px] border border-line">
                {data.sessions.map((x) => (
                  <li key={x.id} className="flex items-center justify-between gap-3 px-4 py-3 text-[13px]">
                    <span className="min-w-0">
                      <span className="block truncate">{device(x.user_agent)}</span>
                      <span className="block text-[12px] text-fg-3">
                        {x.ip ?? "unknown IP"} · active {ago(x.last_seen_at, now)}
                      </span>
                    </span>
                    {x.current ? (
                      <Chip size="sm" tone="ember">
                        This session
                      </Chip>
                    ) : s.manageable ? (
                      <Button
                        size="xs"
                        variant="surface"
                        onClick={async () => {
                          if (await act("POST", `/api/admin/sessions/${x.id}/revoke`, { reason: "Revoked from staff profile" }, "Session revoked")) reload();
                        }}
                      >
                        Revoke
                      </Button>
                    ) : null}
                  </li>
                ))}
              </ul>
            )}
          </section>
          <section>
            <h4 className="text-[13px] font-medium text-fg-2">Trusted devices (skip the email code on new-device checks)</h4>
            <p className="mt-2 text-[13px] text-fg-3">{data.trusted_devices.length ? data.trusted_devices.map((d) => device(d.user_agent)).join(" · ") : "None"}</p>
          </section>
          <section>
            <h4 className="text-[13px] font-medium text-fg-2">Access</h4>
            <div className="mt-2 space-y-2">
              {byModule.map(({ m, perms }) => (
                <div key={m.key} className="flex flex-wrap items-center gap-1.5 text-[12.5px]">
                  <span className="w-40 shrink-0 text-fg-3">{m.label}</span>
                  {perms.map((p) => (
                    <Chip key={p} size="sm">
                      {permLabel(roles?.catalogue, p)}
                    </Chip>
                  ))}
                </div>
              ))}
            </div>
          </section>
          <section>
            <div className="flex items-center justify-between">
              <h4 className="text-[13px] font-medium text-fg-2">Recent activity</h4>
              <Link href={`/security?actor=staff:${s.id}`} className="text-[12.5px] text-ember hover:underline">
                Full audit trail
              </Link>
            </div>
            <ul className="mt-2 space-y-1.5 text-[12.5px]">
              {data.events.map((e) => (
                <li key={e.id} className="flex justify-between gap-3">
                  <span className="font-mono text-fg-2">{e.action}</span>
                  <span className="shrink-0 text-fg-3" title={when(e.created_at)}>
                    {ago(e.created_at, now)}
                  </span>
                </li>
              ))}
            </ul>
          </section>
          {s.manageable && (
            <Button variant="surface" onClick={onChanged}>
              <RefreshCw /> Refresh list
            </Button>
          )}
        </div>
      )}
    </Dialog>
  );
}

export function LiveStaff() {
  const now = useNow();
  const me = useStaff();
  const canList = useCan("staff.read");
  const canWrite = useCan("staff.write");
  const { data, error, reload } = useApi<{ items: Member[]; total: number }>(canList ? "/api/admin/staff" : null);
  const { data: roles, reload: reloadRoles } = useApi<RolesResp>(canList ? "/api/admin/roles" : null);
  const [filter, setFilter] = React.useState<Filter>("all");
  const [invite, setInvite] = React.useState(false);
  const [roleFor, setRoleFor] = React.useState<Member | null>(null);
  const [open, setOpen] = React.useState<number | null>(null);
  const [disableFor, setDisableFor] = React.useState<Member | null>(null);
  const [reason, setReason] = React.useState("");
  const [link, setLink] = React.useState<string | undefined>();
  const { busy, run } = useBusy();
  const [ask, confirmDialog] = useConfirm();

  const items = data?.items ?? [];
  const rows = filter === "all" ? items : items.filter((s) => s.status === filter);
  const count = (f: Filter) => (f === "all" ? items.length : items.filter((s) => s.status === f).length);
  const online = items.filter((s) => s.active_sessions > 0).length;
  const refresh = () => {
    reload();
    reloadRoles();
  };

  const actions = (s: Member) => {
    if (!canWrite || s.is_me || s.role === "platform_owner") return [];
    const it: Parameters<typeof Menu>[0]["items"] = [
      { label: "View details", icon: <Users />, onSelect: () => setOpen(s.id) },
      { label: "Change role", icon: <ShieldCheck />, onSelect: () => setRoleFor(s) },
    ];
    if (s.status === "invited")
      it.push({
        label: "Resend invite",
        icon: <Mail />,
        onSelect: () =>
          void run(`resend-${s.id}`, async () => {
            const d = await act<{ invite: { dev_invite_url?: string } }>("POST", `/api/admin/staff/${s.id}/resend-invite`, {}, "Invite sent again");
            if (d?.invite.dev_invite_url) setLink(d.invite.dev_invite_url);
            reload();
          }),
      });
    if (s.status === "active") {
      it.push({
        label: "Reset 2FA",
        hint: "Forget devices",
        icon: <RotateCcw />,
        onSelect: async () => {
          const ok = await ask({
            title: `Reset 2FA for ${s.name}?`,
            text: `This forgets all ${s.trusted_devices} remembered device${s.trusted_devices === 1 ? "" : "s"}, cancels any unused sign-in codes, clears a sign-in lock and signs ${s.name} out everywhere (${s.active_sessions} active session${s.active_sessions === 1 ? "" : "s"}). Their next sign-in needs a fresh email code.`,
            confirm: "Reset 2FA and sign out",
            tone: "danger",
          });
          if (ok) void run(`2fa-${s.id}`, async () => (await act("POST", `/api/admin/staff/${s.id}/reset-2fa`, {}, "2FA reset", `${s.name} is signed out everywhere; their next sign-in needs a fresh email code.`)) && reload());
        },
      });
      it.push({
        label: "Force sign-out",
        icon: <LogOut />,
        onSelect: async () => {
          const ok = await ask({
            title: `Sign ${s.name} out everywhere?`,
            text: `Ends all ${s.active_sessions} active Back Office session${s.active_sessions === 1 ? "" : "s"} for ${s.name}. They can sign in again with their password and an emailed code.`,
            confirm: "Sign out everywhere",
            tone: "danger",
          });
          if (ok) void run(`out-${s.id}`, async () => (await act("POST", `/api/admin/staff/${s.id}/sign-out`, {}, "Signed out everywhere")) && reload());
        },
      });
    }
    it.push("sep");
    if (s.status === "disabled") it.push({ label: "Enable account", icon: <UserCheck />, onSelect: () => void run(`en-${s.id}`, async () => (await act("POST", `/api/admin/staff/${s.id}/enable`, {}, "Account enabled")) && reload()) });
    else it.push({ label: "Disable account", icon: <Ban />, danger: true, onSelect: () => (setReason(""), setDisableFor(s)) });
    return it;
  };

  const columns: Column<Member>[] = [
    {
      key: "who",
      header: "Staff member",
      cell: (s) => (
        <span className="flex min-w-0 items-center gap-3">
          <Avatar name={s.name} size={34} online={s.active_sessions > 0} />
          <span className="min-w-0">
            <span className="flex items-center gap-1.5 truncate font-medium">
              {s.name}
              {s.is_me && (
                <Chip size="sm" tone="ember">
                  You
                </Chip>
              )}
            </span>
            <span className="block truncate text-[12px] text-fg-3">{s.email}</span>
          </span>
        </span>
      ),
      sort: (s) => s.name,
    },
    { key: "role", header: "Role", cell: (s) => <Chip size="sm" tone={s.role === "platform_owner" ? "ember" : s.role === "super_admin" ? "gold" : s.role_kind === "custom" ? "info" : "neutral"}>{s.role_label}</Chip>, sort: (s) => s.role_label },
    {
      key: "status",
      header: "Status",
      cell: (s) => (
        <span className="flex flex-wrap items-center gap-1.5" data-testid={`status-${s.email}`}>
          <Chip size="sm" tone={STATUS_TONE[s.status] ?? "neutral"} dot>
            {s.status === "invited" ? "Invite pending" : cap(s.status)}
          </Chip>
          {s.locked && (
            <Chip size="sm" tone="warn">
              Locked
            </Chip>
          )}
        </span>
      ),
    },
    { key: "online", header: "Online", hideOn: "sm", cell: (s) => (s.active_sessions > 0 ? <span className="whitespace-nowrap text-up">{s.active_sessions} session{s.active_sessions > 1 ? "s" : ""}</span> : <span className="text-fg-3">No</span>) },
    { key: "login", header: "Last sign-in", hideOn: "md", cell: (s) => <span className="whitespace-nowrap text-fg-2" title={when(s.last_login_at)}>{s.status === "invited" ? `invited ${ago(s.invited_at, now)}` : ago(s.last_login_at, now)}</span> },
    { key: "added", header: "Added", hideOn: "xl", cell: (s) => <span className="whitespace-nowrap text-fg-3">{day(s.created_at)}</span> },
    {
      key: "act",
      header: "",
      align: "right",
      cell: (s) => {
        const it = actions(s);
        if (!it.length) return null;
        return (
          <span onClick={(e) => e.stopPropagation()}>
            <Menu
              align="end"
              items={it}
              trigger={
                <IconButton size="sm" aria-label={`Actions for ${s.email}`} disabled={busy?.endsWith(`-${s.id}`)}>
                  <MoreHorizontal />
                </IconButton>
              }
            />
          </span>
        );
      },
    },
  ];

  return (
    <div className="pb-10">
      {confirmDialog}
      <PageHeader
        title="Staff"
        subtitle={`Back Office accounts for ${me.tenant.name}`}
        actions={
          <>
            {canList && (
              <Button variant="surface" onClick={refresh}>
                <RefreshCw /> Refresh
              </Button>
            )}
            <Link href="/org/roles">
              <Button variant="surface">
                <ShieldCheck /> Roles
              </Button>
            </Link>
            {canWrite && (
              <Button variant="ember" onClick={() => setInvite(true)} disabled={!roles}>
                <UserPlus /> Invite staff
              </Button>
            )}
          </>
        }
      />
      {canList && data && (
        <div className="mb-4 grid grid-cols-2 gap-4 lg:grid-cols-4">
          <KpiCard label="Staff accounts" value={data.total} icon={<Users />} />
          <KpiCard label="Signed in now" value={online} chip={online ? "live" : undefined} chipTone="up" />
          <KpiCard label="Invites pending" value={count("invited")} chipTone="warn" />
          <KpiCard label="Disabled" value={count("disabled")} />
        </div>
      )}
      <InviteLink url={link} />
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal className="xl:col-span-8">
          <Card>
            <CardHeader
              title="Team"
              subtitle="Click a row for sessions, devices and activity"
              icon={<Users />}
              action={
                canList ? (
                  <Segmented
                    size="xs"
                    value={filter}
                    onChange={setFilter}
                    options={(["all", "active", "invited", "disabled"] as Filter[]).map((f) => ({ value: f, label: `${f === "all" ? "All" : cap(f)} ${count(f)}` }))}
                  />
                ) : undefined
              }
            />
            <div className="px-4 pb-5 pt-4 sm:px-6">
              {!canList ? (
                <EmptyState title="Not available for your role" text="Your role doesn't include the staff list. Your own access is on the right." illustration="locked" />
              ) : error ? (
                <ErrorState error={error} onRetry={reload} />
              ) : !data ? (
                <TableSkeleton rows={3} />
              ) : (
                <DataTable
                  rows={rows}
                  columns={columns}
                  rowKey={(s) => String(s.id)}
                  pageSize={50}
                  onRowClick={(s) => setOpen(s.id)}
                  search={(s) => `${s.name} ${s.email} ${s.role_label}`}
                  searchPlaceholder="Search staff"
                  empty={<EmptyState title="Nobody here" illustration="busts_in_silhouette" />}
                />
              )}
            </div>
          </Card>
        </Reveal>
        <Reveal delay={0.05} className="xl:col-span-4">
          <MyAccess roles={roles} />
        </Reveal>
      </div>

      {roles && <InviteDialog open={invite} onOpenChange={setInvite} roles={roles.items} onDone={refresh} />}
      {roles && <RoleDialog member={roleFor} roles={roles.items} onClose={() => setRoleFor(null)} onDone={refresh} />}
      <DetailDrawer id={open} roles={roles} onClose={() => setOpen(null)} onChanged={refresh} />
      <Dialog
        open={!!disableFor}
        onOpenChange={(o) => !o && setDisableFor(null)}
        title="Disable account"
        description={disableFor ? `${disableFor.name} is signed out everywhere and can't sign in until re-enabled.` : undefined}
      >
        <Field label="Reason (audit log)">
          <Input value={reason} onChange={(e) => setReason(e.target.value)} placeholder="e.g. left the company" name="disable-reason" />
        </Field>
        <div className="mt-5 flex justify-end gap-2">
          <Button variant="ghost" onClick={() => setDisableFor(null)}>
            Cancel
          </Button>
          <Button
            variant="ember"
            onClick={async () => {
              if (!disableFor) return;
              const ok = await act("POST", `/api/admin/staff/${disableFor.id}/disable`, { reason }, "Account disabled");
              if (ok) {
                setDisableFor(null);
                reload();
              }
            }}
          >
            Disable
          </Button>
        </div>
      </Dialog>
    </div>
  );
}
