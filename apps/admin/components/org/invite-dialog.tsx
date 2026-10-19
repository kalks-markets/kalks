"use client";

import * as React from "react";
import { toast } from "sonner";
import { Mail, Send, UserPlus } from "lucide-react";
import { Button, Dialog, DialogClose, Field, Input, Toggle } from "@kalks/ui";
import { ORG_DESKS, ORG_ROLE_META, ORG_TENANTS, type OrgDeskKey, type OrgRoleKey, type OrgTenantKey } from "@kalks/mock/admin-platform-security";
import { PickPill } from "./shared";

export interface InviteDraft {
  email: string;
  name: string;
  role: OrgRoleKey;
  desk: OrgDeskKey | null;
  tenants: OrgTenantKey[];
  hardwareKey: boolean;
}

const EMAIL = /^[^\s@]+@[^\s@]+\.[a-z]{2,}$/i;

export function InviteDialog({ onInvite }: { onInvite: (d: InviteDraft) => void }) {
  const [open, setOpen] = React.useState(false);
  const [email, setEmail] = React.useState("");
  const [name, setName] = React.useState("");
  const [role, setRole] = React.useState<OrgRoleKey>("support");
  const [desk, setDesk] = React.useState<OrgDeskKey | null>("support-en");
  const [tenants, setTenants] = React.useState<OrgTenantKey[]>(["kalks"]);
  const [hw, setHw] = React.useState(true);
  const [touched, setTouched] = React.useState(false);

  const emailErr = !EMAIL.test(email) ? "Enter a valid work email" : undefined;
  const nameErr = name.trim().length < 3 ? "Enter the full name" : undefined;
  const tenantErr = tenants.length === 0 ? "Grant access to at least one broker" : undefined;
  const external = EMAIL.test(email) && !email.toLowerCase().endsWith("@kalkstrade.com");

  const reset = () => {
    setEmail("");
    setName("");
    setRole("support");
    setDesk("support-en");
    setTenants(["kalks"]);
    setHw(true);
    setTouched(false);
  };

  const submit = () => {
    setTouched(true);
    if (emailErr || nameErr || tenantErr) {
      toast.error("Check the invite form", { description: emailErr ?? nameErr ?? tenantErr });
      return;
    }
    onInvite({ email: email.trim(), name: name.trim(), role, desk, tenants, hardwareKey: hw });
    toast.success(`Invite sent to ${email.trim()}`, { description: `${ORG_ROLE_META[role].name} · expires in 72 h · logged to audit` });
    setOpen(false);
    reset();
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o);
        if (!o) reset();
      }}
      width={600}
      title="Invite employee"
      description="They'll get a one-time link to set a password and enrol 2FA."
      trigger={
        <Button variant="ember">
          <UserPlus /> Invite employee
        </Button>
      }
      footer={
        <>
          <DialogClose asChild>
            <Button size="sm" variant="ghost">
              Cancel
            </Button>
          </DialogClose>
          <Button size="sm" variant="ember" onClick={submit}>
            <Send /> Send invite
          </Button>
        </>
      }
    >
      <div className="space-y-5">
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <Field label="Full name" error={touched ? nameErr : undefined}>
            <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="Nadia Khoury" />
          </Field>
          <Field label="Work email" error={touched ? emailErr : undefined} hint={external ? <span className="text-warn">External domain</span> : undefined}>
            <Input value={email} onChange={(e) => setEmail(e.target.value)} placeholder="nadia.khoury@kalkstrade.com" leading={<Mail />} />
          </Field>
        </div>
        <div>
          <div className="mb-2 text-[12.5px] font-medium text-fg-2">Role</div>
          <div className="flex flex-wrap gap-1.5">
            {(Object.keys(ORG_ROLE_META) as OrgRoleKey[])
              .filter((k) => k !== "custom")
              .map((k) => (
                <PickPill key={k} on={role === k} onClick={() => setRole(k)}>
                  {ORG_ROLE_META[k].name}
                </PickPill>
              ))}
          </div>
          <p className="mt-2 text-[12px] text-fg-3">{ORG_ROLE_META[role].description}</p>
        </div>
        <div>
          <div className="mb-2 text-[12.5px] font-medium text-fg-2">Desk</div>
          <div className="flex flex-wrap gap-1.5">
            <PickPill on={desk === null} onClick={() => setDesk(null)}>
              No desk
            </PickPill>
            {ORG_DESKS.map((d) => (
              <PickPill key={d.key} on={desk === d.key} onClick={() => setDesk(d.key)}>
                {d.name}
              </PickPill>
            ))}
          </div>
        </div>
        <div>
          <div className="mb-2 flex items-center justify-between text-[12.5px] font-medium text-fg-2">
            Tenant access
            {touched && tenantErr && <span className="text-xs font-normal text-down">{tenantErr}</span>}
          </div>
          <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
            {ORG_TENANTS.map((t) => {
              const on = tenants.includes(t.key);
              return (
                <button
                  key={t.key}
                  type="button"
                  onClick={() => setTenants((ts) => (on ? ts.filter((x) => x !== t.key) : [...ts, t.key]))}
                  className={`flex items-center gap-3 rounded-[14px] border px-3.5 py-2.5 text-left transition-colors ${on ? "border-ember/40 bg-ember-soft" : "border-line bg-surface-2 hover:bg-surface-3"}`}
                >
                  <span className="grid size-7 place-items-center rounded-full text-[10px] font-bold text-black/80" style={{ background: t.color }}>
                    {t.short}
                  </span>
                  <span className="flex-1 text-[13px]">{t.name}</span>
                  <span className={`grid size-4 place-items-center rounded-[5px] border ${on ? "border-ember bg-ember" : "border-fg-3/50"}`}>
                    {on && (
                      <svg viewBox="0 0 12 12" className="size-3 fill-none stroke-white stroke-2">
                        <path d="M2.5 6.2l2.2 2.2 4.8-4.8" />
                      </svg>
                    )}
                  </span>
                </button>
              );
            })}
          </div>
        </div>
        <div className="flex items-center justify-between rounded-[14px] border border-line px-4 py-3">
          <div>
            <div className="text-[13px] font-medium">Require hardware security key</div>
            <div className="text-[12px] text-fg-3">FIDO2 key must be enrolled before first sign-in</div>
          </div>
          <Toggle checked={hw} onChange={setHw} label="Require hardware key" />
        </div>
      </div>
    </Dialog>
  );
}
