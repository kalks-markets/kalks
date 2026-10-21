"use client";

/**
 * "Erase personal data" (GDPR right to erasure, gateway client_erase.rs): anonymises a closed client's profile and
 * removes sign-in data, view-only logins and KYC files. Trading, wallet and IB records stay under the client id.
 * Super Admin only (`clients.erase`); the client must be closed first, and staff give a reason and type ERASE.
 */
import * as React from "react";
import { UserX } from "lucide-react";
import { toast } from "sonner";
import { Button, Dialog, Field, Input } from "@kalks/ui";
import { useCan } from "@/components/staff-session";
import { TextArea } from "@/components/config/kit";
import { Mono, sendJson } from "@/components/live/kit";
import type { Client } from "@/components/live/types";

const WORD = "ERASE";

export function EraseClientButton({ user, onErased }: { user: Pick<Client, "id" | "name" | "status" | "erased_at">; onErased: () => void }) {
  const can = useCan("clients.erase");
  const [open, setOpen] = React.useState(false);
  if (!can || user.erased_at) return null;
  const closed = user.status === "closed";
  return (
    <>
      <Button
        variant="down-outline"
        onClick={() => setOpen(true)}
        disabled={!closed}
        title={closed ? undefined : "Only a closed client can be erased. Complete the client's closure request first."}
        data-testid="erase-open"
      >
        <UserX /> Erase personal data
      </Button>
      {open && <EraseDialog user={user} onClose={() => setOpen(false)} onErased={onErased} />}
    </>
  );
}

function EraseDialog({ user, onClose, onErased }: { user: Pick<Client, "id" | "name">; onClose: () => void; onErased: () => void }) {
  const [reason, setReason] = React.useState("");
  const [word, setWord] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const ready = reason.trim().length >= 3 && word.trim() === WORD;

  async function erase() {
    if (reason.trim().length < 3) return toast.error("Give a reason", { description: "At least 3 characters, e.g. the request or ticket number." });
    if (word.trim() !== WORD) return toast.error(`Type ${WORD} to confirm`);
    setBusy(true);
    const r = await sendJson<{ already_erased: boolean; removed?: { sessions: number; viewers: number; kyc_files: number } }>(`/api/admin/users/${user.id}/erase`, { reason: reason.trim(), confirm: WORD });
    setBusy(false);
    if (!r.ok) return toast.error("Couldn't erase the client", { description: r.error.message });
    const n = r.data.removed;
    toast.success(r.data.already_erased ? "Already erased" : `Personal data of client #${user.id} erased`, {
      description: n ? `${n.sessions} session(s), ${n.viewers} view-only login(s) and ${n.kyc_files} KYC file(s) removed · audited` : "Nothing left to remove.",
    });
    onClose();
    onErased();
  }

  return (
    <Dialog
      open
      onOpenChange={(o) => !o && !busy && onClose()}
      width={520}
      title={`Erase ${user.name}'s personal data`}
      description="This cannot be undone. Use it for a client's right-to-erasure request once their account is closed."
      footer={
        <>
          <Button size="sm" variant="ghost" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button size="sm" variant="down-outline" onClick={erase} disabled={busy || !ready} data-testid="erase-confirm">
            <UserX /> {busy ? "Erasing…" : "Erase personal data"}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <div className="space-y-2 rounded-[12px] border border-down/30 bg-down-soft px-3 py-2.5 text-[12.5px] leading-relaxed text-fg">
          <p>
            <span className="font-medium">Erased:</span> name, email, phone, date of birth, photo, Google link, marketing attribution, sessions, trusted devices, view-only logins and every KYC document. Sign-in becomes impossible.
          </p>
          <p className="text-fg-2">
            <span className="font-medium text-fg">Kept:</span> client <Mono>#{user.id}</Mono>, country, referral code and referrer, the audit log, and the trading, wallet and IB records the broker must retain.
          </p>
        </div>
        <Field label="Reason" hint="Required · audited · no personal data">
          <TextArea value={reason} onChange={setReason} rows={2} placeholder="e.g. Erasure request #3312, closure completed 14 Oct" />
        </Field>
        <Field label={`Type ${WORD} to confirm`}>
          <Input value={word} onChange={(e) => setWord(e.target.value)} placeholder={WORD} autoComplete="off" spellCheck={false} aria-label={`Type ${WORD} to confirm`} className="h-10 font-mono" data-testid="erase-word" />
        </Field>
      </div>
    </Dialog>
  );
}
