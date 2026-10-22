"use client";

import * as React from "react";
import Link from "next/link";
import { Copy, Lock, Plus, RotateCcw, Save, ShieldCheck, Trash2, Users } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, CardHeader, Chip, Dialog, EmptyState, Field, Input, PageHeader, cn } from "@kalks/ui";
import { useCan } from "@/components/staff-session";
import { ErrorState, TableSkeleton, useApi } from "@/components/live/kit";
import { useConfirm } from "@/components/confirm";
import { Select, act, call, type Catalogue, type Role, type RolesResp } from "./kit";

const ACTIONS = [
  ["view", "View"],
  ["create", "Create"],
  ["edit", "Edit"],
  ["approve", "Approve"],
  ["export", "Export"],
] as const;

const KIND_LABEL = { system: "Full access", preset: "Preset", custom: "Custom" } as const;

/** Module × action matrix: one checkbox per permission key in its cell. */
function Matrix({ cat, value, onChange, mine, readOnly }: { cat: Catalogue; value: Set<string>; onChange: (next: Set<string>) => void; mine: Set<string>; readOnly: boolean }) {
  const toggle = (key: string, module: string, on: boolean) => {
    const next = new Set(value);
    const perms = cat.permissions.filter((p) => p.module === module);
    if (on) {
      next.add(key);
      // anything in a module implies seeing it
      const view = perms.find((p) => p.action === "view" && (module !== "audit" || (key === "sessions.revoke" ? p.key === "sessions.read" : p.key === "audit.read")));
      if (view && view.key !== key) next.add(view.key);
    } else {
      next.delete(key);
      const def = perms.find((p) => p.key === key);
      // turning off the view key clears the module (except audit's second view)
      if (def?.action === "view" && module !== "audit") perms.forEach((p) => next.delete(p.key));
    }
    onChange(next);
  };
  const rowAll = (module: string, on: boolean) => {
    const next = new Set(value);
    cat.permissions.filter((p) => p.module === module && mine.has(p.key)).forEach((p) => (on ? next.add(p.key) : next.delete(p.key)));
    onChange(next);
  };
  return (
    <div className="overflow-x-auto">
      <table className="w-full min-w-[760px] border-separate border-spacing-0 text-[13px]" data-testid="role-matrix">
        <thead>
          <tr className="text-left text-[11.5px] uppercase tracking-[0.08em] text-fg-3">
            <th className="sticky left-0 z-10 border-b border-line bg-surface px-3 py-2.5 font-medium">Module</th>
            {ACTIONS.map(([k, l]) => (
              <th key={k} className="border-b border-line px-3 py-2.5 font-medium">
                {l}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {cat.modules.map((m) => {
            const perms = cat.permissions.filter((p) => p.module === m.key);
            const on = perms.filter((p) => value.has(p.key)).length;
            return (
              <tr key={m.key} className="align-top">
                <td className="sticky left-0 z-10 border-b border-line bg-surface px-3 py-3">
                  <div className="flex items-center justify-between gap-2">
                    <div>
                      <div className="font-medium text-fg">{m.label}</div>
                      <div className="text-[11.5px] text-fg-3">{m.description}</div>
                    </div>
                    {!readOnly && (
                      <button type="button" className="shrink-0 text-[11.5px] text-fg-3 hover:text-ember" onClick={() => rowAll(m.key, on < perms.length)}>
                        {on < perms.length ? "All" : "None"}
                      </button>
                    )}
                  </div>
                </td>
                {ACTIONS.map(([a]) => (
                  <td key={a} className="border-b border-line px-3 py-3">
                    <div className="flex flex-col gap-1.5">
                      {perms
                        .filter((p) => p.action === a)
                        .map((p) => {
                          const disabled = readOnly || !mine.has(p.key);
                          return (
                            <label key={p.key} className={cn("flex items-start gap-2", disabled ? "cursor-not-allowed opacity-60" : "cursor-pointer")} title={!mine.has(p.key) ? "You can't grant a permission you don't hold" : p.key}>
                              <input
                                type="checkbox"
                                className="mt-0.5 size-4 accent-[var(--k-ember)]"
                                checked={value.has(p.key)}
                                disabled={disabled}
                                onChange={(e) => toggle(p.key, m.key, e.target.checked)}
                                data-perm={p.key}
                              />
                              <span className="leading-snug text-fg-2">{p.label}</span>
                            </label>
                          );
                        })}
                    </div>
                  </td>
                ))}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function NewRoleDialog({ open, onOpenChange, roles, onCreated }: { open: boolean; onOpenChange: (o: boolean) => void; roles: Role[]; onCreated: (id: number) => void }) {
  const [name, setName] = React.useState("");
  const [description, setDescription] = React.useState("");
  const [from, setFrom] = React.useState("");
  const [err, setErr] = React.useState<{ field?: string; message: string } | null>(null);
  React.useEffect(() => {
    if (open) {
      setName("");
      setDescription("");
      setFrom("");
      setErr(null);
    }
  }, [open]);
  const copyable = roles.filter((r) => r.editable || r.kind === "preset");
  return (
    <Dialog open={open} onOpenChange={onOpenChange} title="New role" description="Start empty or from an existing role, then pick permissions in the matrix.">
      <div className="space-y-4">
        {err && !err.field && <div className="rounded-[14px] border border-down/25 bg-down-soft px-4 py-3 text-[13px] text-down">{err.message}</div>}
        <Field label="Name" error={err?.field === "name" ? err.message : undefined}>
          <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="e.g. KYC Desk" name="role-name" />
        </Field>
        <Field label="Description">
          <Input value={description} onChange={(e) => setDescription(e.target.value)} placeholder="What this role is for" name="role-description" />
        </Field>
        <Field label="Start from">
          <Select value={from} onChange={setFrom} name="role-from">
            <option value="">Empty role</option>
            {copyable.map((r) => (
              <option key={r.id} value={r.id}>
                {r.name}
              </option>
            ))}
          </Select>
        </Field>
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            variant="ember"
            disabled={!name.trim()}
            onClick={async () => {
              const base = roles.find((r) => String(r.id) === from);
              const r = await call<{ role: { id: number } }>("POST", "/api/admin/roles", { name, description, permissions: base?.permissions ?? [] });
              if (!r.ok) return setErr(r.error);
              toast.success("Role created", { description: name });
              onCreated(r.data.role.id);
              onOpenChange(false);
            }}
          >
            Create role
          </Button>
        </div>
      </div>
    </Dialog>
  );
}

export function LiveRoles() {
  const canRead = useCan("staff.read");
  const canEdit = useCan("staff.roles");
  const { data, error, reload } = useApi<RolesResp>(canRead ? "/api/admin/roles" : null);
  const [sel, setSel] = React.useState<number | null>(null);
  const [draft, setDraft] = React.useState<Set<string>>(new Set());
  const [meta, setMeta] = React.useState({ name: "", description: "" });
  const [creating, setCreating] = React.useState(false);
  const [saving, setSaving] = React.useState(false);
  const [ask, confirmDialog] = useConfirm();

  const role = data?.items.find((r) => r.id === sel) ?? null;
  React.useEffect(() => {
    if (data && sel === null) setSel(data.items.find((r) => r.kind !== "system")?.id ?? data.items[0]?.id ?? null);
  }, [data, sel]);
  React.useEffect(() => {
    if (role) {
      setDraft(new Set(role.permissions));
      setMeta({ name: role.name, description: role.description });
    }
  }, [role]);

  if (!canRead) return <EmptyState title="Not available for your role" text="Your role doesn't include staff and roles." illustration="locked" />;
  const mine = new Set(data?.my_permissions ?? []);
  const dirty = !!role && (draft.size !== role.permissions.length || role.permissions.some((p) => !draft.has(p)) || meta.name !== role.name || meta.description !== role.description);
  const readOnly = !role?.editable;

  async function save() {
    if (!role) return;
    setSaving(true);
    const body: Record<string, unknown> = { permissions: [...draft] };
    if (role.kind === "custom") body.name = meta.name;
    if (meta.description !== role.description) body.description = meta.description;
    const ok = await act("PATCH", `/api/admin/roles/${role.id}`, body, "Role saved", `${draft.size} permissions · audited`);
    setSaving(false);
    if (ok) reload();
  }

  return (
    <div className="pb-10">
      {confirmDialog}
      <PageHeader
        title="Roles & permissions"
        subtitle="Presets for every desk, plus your own roles. Changes apply to everyone in the role on their next click."
        actions={
          <>
            <Link href="/org">
              <Button variant="surface">
                <Users /> Staff
              </Button>
            </Link>
            {canEdit && (
              <Button variant="ember" onClick={() => setCreating(true)} disabled={!data}>
                <Plus /> New role
              </Button>
            )}
          </>
        }
      />
      {error ? (
        <ErrorState error={error} onRetry={reload} />
      ) : !data ? (
        <TableSkeleton rows={6} />
      ) : (
        <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
          <Card className="xl:col-span-3">
            <CardHeader title="Roles" subtitle={`${data.items.length} in this workspace`} icon={<ShieldCheck />} />
            <ul className="space-y-1 px-3 pb-4 pt-3" data-testid="role-list">
              {data.items.map((r) => (
                <li key={r.id}>
                  <button
                    type="button"
                    onClick={() => setSel(r.id)}
                    className={cn("flex w-full items-center justify-between gap-2 rounded-[12px] px-3 py-2.5 text-left text-[13.5px] transition-colors", r.id === sel ? "bg-surface-3 text-fg" : "text-fg-2 hover:bg-surface-2")}
                  >
                    <span className="min-w-0">
                      <span className="flex items-center gap-1.5 truncate font-medium">
                        {r.kind === "system" && <Lock className="size-3.5 text-fg-3" />}
                        {r.name}
                      </span>
                      <span className="block text-[11.5px] text-fg-3">
                        {KIND_LABEL[r.kind]}
                        {r.customised ? " · edited" : ""} · {r.permissions.length} perms
                      </span>
                    </span>
                    <Chip size="sm">{r.members}</Chip>
                  </button>
                </li>
              ))}
            </ul>
          </Card>
          <Card className="xl:col-span-9">
            {role && (
              <>
                <CardHeader
                  title={role.name}
                  subtitle={`${role.description || KIND_LABEL[role.kind]} · ${role.members} member${role.members === 1 ? "" : "s"} · services see it as “${role.service_role}”`}
                  action={
                    role.editable ? (
                      <>
                        {role.kind === "preset" && role.customised && (
                          <Button size="sm" variant="surface" onClick={async () => {
                              const ok = await ask({
                                title: `Reset ${role.name} to default?`,
                                text: `Your edits to this preset are discarded and its ${role.permissions.length} permissions are replaced by the built-in defaults. This applies to all ${role.members} member${role.members === 1 ? "" : "s"} on their next click.`,
                                confirm: "Reset to default",
                                tone: "danger",
                              });
                              if (ok && (await act("POST", `/api/admin/roles/${role.id}/reset`, {}, "Preset restored"))) reload();
                            }}>
                            <RotateCcw /> Reset to default
                          </Button>
                        )}
                        {role.kind === "custom" && (
                          <Button
                            size="sm"
                            variant="surface"
                            disabled={role.members > 0}
                            title={role.members > 0 ? "Move its members to another role first" : undefined}
                            onClick={async () => {
                              const ok = await ask({
                                title: `Delete the ${role.name} role?`,
                                text: `The role and its ${role.permissions.length} permissions are deleted permanently; this can't be undone. Disabled staff still assigned to it also block deletion, so move them to another role first.`,
                                confirm: "Delete role",
                                tone: "danger",
                              });
                              if (!ok) return;
                              if (await act("DELETE", `/api/admin/roles/${role.id}`, {}, "Role deleted")) {
                                setSel(null);
                                reload();
                              }
                            }}
                          >
                            <Trash2 /> Delete
                          </Button>
                        )}
                        <Button size="sm" variant="ember" disabled={!dirty || saving} onClick={save}>
                          <Save /> {saving ? "Saving…" : "Save"}
                        </Button>
                      </>
                    ) : (
                      <Chip tone={role.kind === "system" ? "gold" : "neutral"}>
                        <Lock className="mr-1 inline size-3" /> {role.kind === "system" ? "Always full access" : "Read-only for you"}
                      </Chip>
                    )
                  }
                />
                <div className="px-4 pb-5 pt-4 sm:px-6">
                  {role.kind === "custom" && role.editable && (
                    <div className="mb-4 grid gap-3 sm:grid-cols-2">
                      <Field label="Name">
                        <Input value={meta.name} onChange={(e) => setMeta((m) => ({ ...m, name: e.target.value }))} />
                      </Field>
                      <Field label="Description">
                        <Input value={meta.description} onChange={(e) => setMeta((m) => ({ ...m, description: e.target.value }))} />
                      </Field>
                    </div>
                  )}
                  <div className="mb-3 flex flex-wrap items-center gap-2 text-[12.5px] text-fg-3">
                    <Chip size="sm" tone="ember">
                      {draft.size} selected
                    </Chip>
                    Greyed boxes are permissions you don&apos;t hold yourself, so you can&apos;t grant them.
                    {role.kind !== "system" && (
                      <button type="button" className="inline-flex items-center gap-1 text-fg-2 hover:text-ember" onClick={() => void navigator.clipboard?.writeText([...draft].sort().join("\n")).then(() => toast.success("Permission keys copied"))}>
                        <Copy className="size-3.5" /> Copy keys
                      </button>
                    )}
                  </div>
                  <Matrix cat={data.catalogue} value={draft} onChange={setDraft} mine={mine} readOnly={readOnly} />
                </div>
              </>
            )}
          </Card>
        </div>
      )}
      {data && (
        <NewRoleDialog
          open={creating}
          onOpenChange={setCreating}
          roles={data.items}
          onCreated={(id) => {
            reload();
            setSel(id);
          }}
        />
      )}
    </div>
  );
}
