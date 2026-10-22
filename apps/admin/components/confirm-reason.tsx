"use client";

import * as React from "react";
import { Button, Dialog, Field, Textarea } from "@kalks/ui";

type Ask = {
  title: string;
  text?: React.ReactNode;
  confirm?: string;
  tone?: "ember" | "danger";
  /** Label for the reason box (default "Reason"). */
  label?: string;
  placeholder?: string;
  /** Minimum characters before the confirm button enables (default 3). */
  min?: number;
  max?: number;
};

/**
 * Like `useConfirm`, but for actions whose API records a reason/note: the dialog asks for one and
 * resolves with the trimmed text, or `null` when cancelled.
 * `const [askReason, reasonDialog] = useConfirmReason();` then `const why = await askReason({ title }); if (why === null) return;`.
 */
export function useConfirmReason(): [(a: Ask) => Promise<string | null>, React.ReactNode] {
  const [state, setState] = React.useState<(Ask & { resolve: (v: string | null) => void }) | null>(null);
  const [text, setText] = React.useState("");
  const ask = React.useCallback(
    (a: Ask) =>
      new Promise<string | null>((resolve) => {
        setText("");
        setState({ ...a, resolve });
      }),
    [],
  );
  const close = (v: string | null) => {
    state?.resolve(v);
    setState(null);
  };
  const min = state?.min ?? 3;
  const ready = text.trim().length >= min;
  const dialog = (
    <Dialog
      open={!!state}
      onOpenChange={(o) => !o && close(null)}
      width={460}
      title={state?.title ?? ""}
      footer={
        <>
          <Button variant="ghost" onClick={() => close(null)}>
            Cancel
          </Button>
          <Button variant={state?.tone === "danger" ? "down-outline" : "ember"} disabled={!ready} onClick={() => close(text.trim())} data-testid="confirm-ok">
            {state?.confirm ?? "Confirm"}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        {state?.text ? <div className="text-[13.5px] leading-relaxed text-fg-2">{state.text}</div> : null}
        <Field label={state?.label ?? "Reason"} hint={`Required, recorded in the audit log${min > 1 ? ` (at least ${min} characters)` : ""}.`}>
          <Textarea value={text} onChange={(e) => setText(e.target.value)} placeholder={state?.placeholder} maxLength={state?.max ?? 300} rows={3} data-testid="confirm-reason" />
        </Field>
      </div>
    </Dialog>
  );
  return [ask, dialog];
}
