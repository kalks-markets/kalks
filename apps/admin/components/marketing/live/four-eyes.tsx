"use client";

/**
 * Four-eyes (maker-checker) on marketing money out: contest prizes, mission rewards, cashback payouts and cashback
 * redemptions at or above the growth setting `fourEyesThreshold` need two different staff members with
 * marketing.approve. The first approval moves the payout to `awaiting_second_approval` (nothing is paid); the
 * service refuses the same staff member's second approval with 409 four_eyes_same_staff. Either approver may reject.
 */
import * as React from "react";
import { Check, ShieldCheck, X } from "lucide-react";
import { Button } from "@kalks/ui";
import { when } from "@/components/live/kit";
import { useStaff } from "@/components/staff-session";
import { mkSend, type FourEyes } from "./api";
import { usd, type useAction } from "./kit";

export const AWAITING = "awaiting_second_approval";
export const SAME_STAFF_TEXT = "You gave the first approval. Another staff member must give the second approval (four-eyes).";
export const FIRST_DONE_TEXT = "First approval recorded · waiting for a second approver";

/** Whether the signed-in staff member gave this payout's first approval. */
export function useIsFirstApprover(fe: FourEyes | null | undefined): boolean {
  const me = useStaff();
  return !!fe?.firstApprovedById && String(fe.firstApprovedById) === String(me.id);
}

/** Approving now is the first of two approvals (at or above the threshold, no first approval yet). */
export function isFirstOfTwo(status: string, amount: number | null | undefined, fe: FourEyes | null | undefined, threshold: number | null | undefined): boolean {
  if (status === AWAITING || fe?.firstApprovedById) return false;
  if (fe?.required) return true;
  return amount != null && threshold != null && amount >= threshold;
}

/** "First approval by <name> · <when>" (+ note) under a row waiting for its second approval. */
export function FirstApproval({ fe, className }: { fe: FourEyes | null | undefined; className?: string }) {
  if (!fe?.firstApprovedById) return null;
  return (
    <div className={className ?? "flex items-center gap-1 text-[11.5px] text-warn"}>
      <ShieldCheck className="size-3.5 shrink-0" />
      <span className="truncate">
        First approval by {fe.firstApprovedBy || `staff #${fe.firstApprovedById}`}
        {fe.firstApprovedAt ? ` · ${when(fe.firstApprovedAt)}` : ""}
        {fe.firstApprovalNote ? ` · “${fe.firstApprovalNote}”` : ""}
      </span>
    </div>
  );
}

/** One-line rule shown in card subtitles. */
export function fourEyesRule(threshold: number | null | undefined): string {
  if (threshold == null) return "";
  return threshold > 0 ? `Payouts of ${usd(threshold, 0)} or more need two different staff members (four-eyes).` : "Four-eyes: every payout needs two different staff members.";
}

type Act = ReturnType<typeof useAction>;

/**
 * Approve / Reject for one payout waiting for a decision. The first approver sees Approve disabled with the reason;
 * Reject stays available to both. `statusOf` reads the payout's new status from the approve response.
 */
export function FourEyesDecide<T>({
  act,
  what,
  status,
  amount,
  fe,
  threshold,
  approvePath,
  rejectPath,
  statusOf,
  paidText,
  describe,
  onDone,
  testId,
}: {
  act: Pick<Act, "ask">;
  /** "prize", "reward", "payout", "redemption" */
  what: string;
  status: string;
  amount: number | null | undefined;
  fe: FourEyes | null | undefined;
  threshold: number | null | undefined;
  approvePath: string;
  rejectPath: string;
  statusOf: (d: T) => string | undefined;
  paidText: string;
  describe?: string;
  onDone: () => void;
  testId?: string;
}) {
  const mine = useIsFirstApprover(fe);
  const first = isFirstOfTwo(status, amount, fe, threshold);
  const second = status === AWAITING;
  const approve = () =>
    act.ask<T>({
      title: first ? `Approve ${usd(amount)} (1 of 2)` : second ? `Second approval · pay ${usd(amount)}` : `Approve and pay ${usd(amount)}`,
      description: first
        ? `At or above the four-eyes threshold${threshold != null ? ` (${usd(threshold, 0)})` : ""}: your approval is recorded and nothing is paid until another staff member gives the second approval.`
        : second
          ? `${fe?.firstApprovedBy || "Another staff member"} gave the first approval. Your approval releases the ${what}.${describe ? ` ${describe}` : ""}`
          : describe ?? `Releases the ${what}.`,
      confirmLabel: first ? "Approve (1 of 2)" : second ? "Approve (2 of 2) and pay" : "Approve and pay",
      confirmVariant: first ? "ember" : "buy",
      confirmTestId: testId ? `${testId}-approve-confirm` : undefined,
      note: "optional",
      run: (note) => mkSend<T>(approvePath, { note }),
      success: (d: T) => (statusOf(d) === AWAITING ? FIRST_DONE_TEXT : paidText),
      onDone: () => onDone(),
    });
  const reject = () =>
    act.ask<T>({
      title: `Reject ${usd(amount)}`,
      description: second ? `Nothing has been paid. The first approval by ${fe?.firstApprovedBy || "another staff member"} is discarded.` : `Nothing is paid.`,
      confirmLabel: `Reject ${what}`,
      confirmVariant: "sell",
      note: "required",
      run: (note) => mkSend<T>(rejectPath, { note }),
      success: `${what[0]!.toUpperCase()}${what.slice(1)} rejected`,
      onDone: () => onDone(),
    });
  return (
    <div className="flex flex-col items-end gap-1">
      <div className="flex gap-1.5">
        <Button
          size="xs"
          variant={first ? "surface" : "buy"}
          disabled={second && mine}
          title={second && mine ? SAME_STAFF_TEXT : undefined}
          onClick={approve}
          data-testid={testId ? `${testId}-approve` : undefined}
        >
          <Check /> {first ? "Approve (1 of 2)" : second ? "Approve (2 of 2)" : "Approve"}
        </Button>
        <Button size="xs" variant="down-outline" onClick={reject} data-testid={testId ? `${testId}-reject` : undefined}>
          <X /> Reject
        </Button>
      </div>
      {second && mine && <span className="max-w-64 text-right text-[11px] leading-snug text-fg-3">{SAME_STAFF_TEXT}</span>}
    </div>
  );
}
