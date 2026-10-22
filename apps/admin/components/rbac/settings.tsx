"use client";

import * as React from "react";
import Link from "next/link";
import { Construction, Flag, Layers, Power, RotateCcw } from "lucide-react";
import { Button, Card, CardHeader, Chip, EmptyState, Field, Input, KpiCard, PageHeader, Toggle } from "@kalks/ui";
import { useCan } from "@/components/staff-session";
import { ErrorState, TableSkeleton, ago, useApi, useNow, when } from "@/components/live/kit";
import { useConfirm } from "@/components/confirm";
import { Textarea, act } from "./kit";

type Maint = { enabled: boolean; active: boolean; message: string; until: string | null; since: string | null; default_message: string; can_edit: boolean };

/** datetime-local value (local time) ↔ ISO. */
const toLocal = (iso: string | null) => (iso ? new Date(new Date(iso).getTime() - new Date().getTimezoneOffset() * 60000).toISOString().slice(0, 16) : "");
const fromLocal = (v: string) => (v ? new Date(v).toISOString() : null);

export function LiveMaintenance() {
  const now = useNow();
  const canRead = useCan("settings.read");
  const { data, error, reload } = useApi<Maint>(canRead ? "/api/admin/settings/maintenance" : null);
  const [message, setMessage] = React.useState("");
  const [until, setUntil] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [ask, confirmDialog] = useConfirm();
  React.useEffect(() => {
    if (data) {
      setMessage(data.message === data.default_message ? "" : data.message);
      setUntil(toLocal(data.until));
    }
  }, [data]);
  if (!canRead) return <EmptyState title="Not available for your role" illustration="locked" />;

  async function save(enabled: boolean) {
    if (data && enabled !== data.active) {
      const end = fromLocal(until);
      const ok = await ask(
        enabled
          ? {
              title: "Start maintenance now?",
              text: `Every client is held on the maintenance page straight away: Client Area pages show it, client sign-in and sign-up are paused and client API calls answer 503. ${end ? `It ends by itself at ${when(end)}.` : "It stays on until someone ends it here."} Staff and the Back Office keep working.`,
              confirm: "Start maintenance",
              tone: "danger",
            }
          : {
              title: "End maintenance now?",
              text: "The Client Area opens again immediately: clients can sign in and sign up, and client API calls work again.",
              confirm: "End maintenance",
            },
      );
      if (!ok) return;
    }
    setBusy(true);
    const ok = await act("PUT", "/api/admin/settings/maintenance", { enabled, message: message || null, until: enabled ? fromLocal(until) : null }, enabled ? "Maintenance mode is on" : "Maintenance mode is off", enabled ? "Clients see the maintenance page; staff keep working." : "The Client Area is open again.");
    setBusy(false);
    if (ok) reload();
  }

  return (
    <div className="pb-10">
      {confirmDialog}
      <PageHeader title="Maintenance mode" subtitle="Hold clients on a maintenance page during upgrades. Staff and the Back Office keep working." />
      {error ? (
        <ErrorState error={error} onRetry={reload} />
      ) : !data ? (
        <TableSkeleton rows={3} />
      ) : (
        <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
          <Card className="xl:col-span-7">
            <CardHeader
              title="Client Area"
              subtitle={data.active ? `In maintenance since ${when(data.since)}` : "Open to clients"}
              icon={<Construction />}
              action={
                <Chip tone={data.active ? "warn" : "up"} dot>
                  <span data-testid="maintenance-state">{data.active ? "Maintenance" : "Live"}</span>
                </Chip>
              }
            />
            <div className="space-y-4 px-4 pb-5 pt-4 sm:px-6">
              <Field label="Message to clients" hint="Leave empty for the standard text">
                <Textarea value={message} onChange={(e) => setMessage(e.target.value)} placeholder={data.default_message} maxLength={500} disabled={!data.can_edit} name="maintenance-message" />
              </Field>
              <Field label="Expected end (optional)" hint="The page shows it; maintenance ends by itself at this time">
                <Input type="datetime-local" value={until} onChange={(e) => setUntil(e.target.value)} disabled={!data.can_edit} />
              </Field>
              {data.can_edit && (
                <div className="flex flex-wrap gap-2">
                  {data.active ? (
                    <>
                      <Button variant="ember" onClick={() => save(false)} disabled={busy} data-testid="maintenance-off">
                        <Power /> End maintenance
                      </Button>
                      <Button variant="surface" onClick={() => save(true)} disabled={busy}>
                        Update message
                      </Button>
                    </>
                  ) : (
                    <Button variant="ember" onClick={() => save(true)} disabled={busy} data-testid="maintenance-on">
                      <Construction /> Start maintenance
                    </Button>
                  )}
                </div>
              )}
              <p className="text-[12.5px] text-fg-3">While on: Client Area pages show the maintenance page, client sign-in and sign-up are paused, and client API calls answer 503. Every switch is audited.</p>
            </div>
          </Card>
          <Card className="xl:col-span-5">
            <CardHeader title="What clients see" subtitle="Preview" />
            <div className="px-4 pb-5 pt-4 sm:px-6">
              <div className="rounded-[18px] border border-line bg-surface-2 px-6 py-10 text-center">
                <Construction className="mx-auto size-8 text-ember" />
                <div className="mt-4 text-[20px] font-medium">We&apos;ll be right back</div>
                <p className="mx-auto mt-2 max-w-sm text-[13.5px] text-fg-2">{message || data.default_message}</p>
                {until && <p className="mt-3 text-[12.5px] text-fg-3">Expected back by {when(fromLocal(until))}{data.until ? ` (${ago(data.until, now)})` : ""}</p>}
              </div>
            </div>
          </Card>
        </div>
      )}
    </div>
  );
}

type Feature = { key: string; kind: "module" | "flag"; name: string; description: string; enabled: boolean; default: boolean; overridden: boolean; builtin: boolean; updated_at: string | null };
type FeaturesResp = { modules: Feature[]; flags: Feature[]; can_edit_flags: boolean; can_edit_modules: boolean };

function FeatureRow({ f, editable, onSet }: { f: Feature; editable: boolean; onSet: (v: boolean | null) => void }) {
  return (
    <div className="flex items-start justify-between gap-4 border-b border-line py-3.5 last:border-0">
      <div className="min-w-0">
        <div className="flex flex-wrap items-center gap-2 font-medium">
          {f.name}
          <code className="text-[11px] font-normal text-fg-3">{f.key}</code>
          {f.overridden && (
            <Chip size="sm" tone="info">
              Changed from default
            </Chip>
          )}
        </div>
        <div className="text-[12.5px] text-fg-3">{f.description}</div>
      </div>
      <div className="flex shrink-0 items-center gap-2" data-testid={`feature-${f.key}`}>
        {editable && f.overridden && (
          <button type="button" className="text-fg-3 hover:text-ember" title="Back to the platform default" onClick={() => onSet(null)}>
            <RotateCcw className="size-4" />
          </button>
        )}
        {editable ? (
          <Toggle checked={f.enabled} onChange={(v) => onSet(v)} />
        ) : (
          <Chip size="sm" tone={f.enabled ? "up" : "neutral"} dot>
            {f.enabled ? "On" : "Off"}
          </Chip>
        )}
      </div>
    </div>
  );
}

export function LiveFeatures() {
  const canRead = useCan("settings.read");
  const { data, error, reload } = useApi<FeaturesResp>(canRead ? "/api/admin/settings/features" : null);
  if (!canRead) return <EmptyState title="Not available for your role" illustration="locked" />;
  const set = async (f: Feature, v: boolean | null) => {
    if (await act("PUT", `/api/admin/settings/features/${f.key}`, { enabled: v }, `${f.name}: ${v === null ? "platform default" : v ? "on" : "off"}`)) reload();
  };
  return (
    <div className="pb-10">
      <PageHeader title="Features & modules" subtitle="What your clients can use. Modules are switched by the platform owner; flags by your Super Admin." />
      {error ? (
        <ErrorState error={error} onRetry={reload} />
      ) : !data ? (
        <TableSkeleton rows={6} />
      ) : (
        <>
          <div className="mb-4 grid grid-cols-2 gap-4 lg:grid-cols-4">
            <KpiCard label="Modules on" value={`${data.modules.filter((m) => m.enabled).length} / ${data.modules.length}`} icon={<Layers />} />
            <KpiCard label="Flags on" value={`${data.flags.filter((m) => m.enabled).length} / ${data.flags.length}`} icon={<Flag />} />
          </div>
          <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
            <Card>
              <CardHeader
                title="Modules"
                subtitle={data.can_edit_modules ? "You can switch modules as platform owner" : "Included in your plan. Ask the platform owner to change them."}
                icon={<Layers />}
                action={
                  data.can_edit_modules ? (
                    <Link href="/brokers/modules">
                      <Button size="xs" variant="surface">
                        All tenants
                      </Button>
                    </Link>
                  ) : undefined
                }
              />
              <div className="px-4 pb-3 pt-2 sm:px-6">
                {data.modules.map((f) => (
                  <FeatureRow key={f.key} f={f} editable={data.can_edit_modules} onSet={(v) => set(f, v)} />
                ))}
              </div>
            </Card>
            <Card>
              <CardHeader title="Feature flags" subtitle="Switch client-facing features on or off for this brokerage" icon={<Flag />} />
              <div className="px-4 pb-3 pt-2 sm:px-6">
                {data.flags.map((f) => (
                  <FeatureRow key={f.key} f={f} editable={data.can_edit_flags} onSet={(v) => set(f, v)} />
                ))}
              </div>
            </Card>
          </div>
        </>
      )}
    </div>
  );
}
