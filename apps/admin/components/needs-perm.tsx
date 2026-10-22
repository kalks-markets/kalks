"use client";

import * as React from "react";
import { Tooltip } from "@kalks/ui";

/**
 * Shows a control as unavailable (dimmed, not clickable or focusable) with a tooltip naming the missing permission,
 * for actions the role can see but the server would refuse. Renders the control unchanged when `ok`.
 */
export function NeedsPerm({ ok, perm, text, children }: { ok: boolean; perm: string; text?: string; children: React.ReactNode }) {
  if (ok) return <>{children}</>;
  return (
    <Tooltip content={text ?? `Needs the ${perm} permission`}>
      <span className="inline-flex cursor-not-allowed opacity-50" tabIndex={0} aria-disabled="true" data-needs-perm={perm}>
        <span inert className="inline-flex">
          {children}
        </span>
      </span>
    </Tooltip>
  );
}
