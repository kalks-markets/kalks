"use client";

import * as React from "react";
import { RotateCw } from "lucide-react";
import {
  Avatar,
  Button,
  Card,
  Chip,
  EmptyState,
  Flag,
  Illustration,
  PageHeader,
  Skeleton,
  cn,
  type IllustrationName,
} from "@/components/kit";
import { useT } from "@kalks/i18n/react";
import { ThemedImage, useContentImage } from "@kalks/ui/content";
import type { ClientStatus, PartnerApiError } from "./api";

/* ------------------------------------------------------------------ */
/* Load states                                                         */
/* ------------------------------------------------------------------ */

/** The partner profile is created from the sign-up a few seconds after registration (409 not_ready). */
export function SettingUp({ onRetry }: { onRetry: () => void }) {
  const t = useT();
  // retry quietly a few times; the button stays for impatient users
  React.useEffect(() => {
    let n = 0;
    const id = setInterval(() => {
      n += 1;
      if (n > 6) clearInterval(id);
      else onRetry();
    }, 5000);
    return () => clearInterval(id);
  }, [onRetry]);
  return (
    <Card>
      <EmptyState
        art="partnerIb"
        title={t("partner.load.settingUpTitle")}
        text={t("partner.load.settingUpText")}
        action={
          <Button variant="surface" onClick={onRetry}>
            <RotateCw /> {t("partner.load.checkAgain")}
          </Button>
        }
      />
    </Card>
  );
}

export function LoadError({
  error,
  onRetry,
}: {
  error: PartnerApiError;
  onRetry: () => void;
}) {
  const t = useT();
  return (
    <Card>
      <EmptyState
        art="connectionLost"
        title={t("partner.load.errorTitle")}
        text={error.message || t("partner.load.errorText")}
        action={
          <Button variant="surface" onClick={onRetry}>
            <RotateCw /> {t("common.retry")}
          </Button>
        }
      />
    </Card>
  );
}

/** Renders the not-ready / error state for a failed load, or null when there is nothing to show. */
export function LoadProblem({
  error,
  onRetry,
}: {
  error: PartnerApiError | null;
  onRetry: () => void;
}) {
  if (!error) return null;
  if (error.status === 409 && error.code === "not_ready")
    return <SettingUp onRetry={onRetry} />;
  return <LoadError error={error} onRetry={onRetry} />;
}

/** Page shell used while the first load is in flight or failed. */
export function PageFallback({
  title,
  subtitle,
  error,
  onRetry,
  skeleton,
}: {
  title: string;
  subtitle: string;
  error: PartnerApiError | null;
  onRetry: () => void;
  skeleton: React.ReactNode;
}) {
  return (
    <div className="pb-24">
      <PageHeader title={title} subtitle={subtitle} />
      {error ? <LoadProblem error={error} onRetry={onRetry} /> : skeleton}
    </div>
  );
}

export function SkeletonGrid({
  rows,
}: {
  rows: { cols: string; h: string; n: number }[];
}) {
  return (
    <div className="space-y-4">
      {rows.map((r, i) => (
        <div key={i} className={cn("grid grid-cols-1 gap-4", r.cols)}>
          {Array.from({ length: r.n }, (_, j) => (
            <Skeleton key={j} className={cn("w-full rounded-[20px]", r.h)} />
          ))}
        </div>
      ))}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Chips & cells                                                       */
/* ------------------------------------------------------------------ */

export function TierChip({
  tier,
  className,
}: {
  tier: number;
  className?: string;
}) {
  return (
    <Chip
      size="sm"
      tone={tier === 1 ? "ember" : tier === 2 ? "gold" : "neutral"}
      className={cn("font-semibold", className)}
    >
      L{tier}
    </Chip>
  );
}

const COMMISSION_STATUS: Record<
  string,
  { label: string; tone: "warn" | "info" | "up" | "down" | "neutral" }
> = {
  pending: { label: "Pending", tone: "warn" },
  approved: { label: "Approved", tone: "info" },
  paid: { label: "Paid", tone: "up" },
  rejected: { label: "Rejected", tone: "down" },
  void: { label: "Void", tone: "neutral" },
};

export function CommissionStatusChip({ status }: { status: string }) {
  const t = useT();
  const m = COMMISSION_STATUS[status] ?? {
    label: status,
    tone: "neutral" as const,
  };
  return (
    <Chip size="sm" dot tone={m.tone}>
      {t.dyn(`partner.commissionStatus.${status}`, m.label)}
    </Chip>
  );
}

const PAYOUT_STATUS: Record<
  string,
  { label: string; tone: "warn" | "info" | "up" | "down" | "neutral" }
> = {
  awaiting_approval: { label: "Awaiting approval", tone: "warn" },
  processing: { label: "Processing", tone: "info" },
  paid: { label: "Paid", tone: "up" },
  rejected: { label: "Rejected", tone: "down" },
};

export function PayoutStatusChip({ status }: { status: string }) {
  const t = useT();
  const m = PAYOUT_STATUS[status] ?? {
    label: status.replace(/_/g, " "),
    tone: "neutral" as const,
  };
  return (
    <Chip size="sm" dot tone={m.tone}>
      {t.dyn(`partner.payoutStatus.${status}`, m.label)}
    </Chip>
  );
}

export const CLIENT_STATUS: Record<
  ClientStatus,
  { label: string; tone: "up" | "info" | "neutral" }
> = {
  active: { label: "Active", tone: "up" },
  funded: { label: "Funded", tone: "info" },
  registered: { label: "Registered", tone: "neutral" },
};

export function ClientStatusChip({ status }: { status: ClientStatus }) {
  const t = useT();
  const m = CLIENT_STATUS[status] ?? {
    label: status,
    tone: "neutral" as const,
  };
  return (
    <Chip size="sm" tone={m.tone}>
      {t.dyn(`partner.clientStatus.${status}`, m.label)}
    </Chip>
  );
}

/** Initials avatar + flag + name. No photos: the partner service doesn't share them. */
export function PersonCell({
  name,
  country,
  sub,
  size = 32,
}: {
  name: string;
  country?: string | null;
  sub?: React.ReactNode;
  size?: number;
}) {
  const initials = name.replace(/[^\p{L}\s]/gu, "").trim() || "?";
  return (
    <span className="flex min-w-0 items-center gap-3">
      <span className="relative shrink-0">
        <Avatar name={initials} size={size} />
        {country && (
          <Flag
            country={country.toLowerCase()}
            className="absolute -bottom-0.5 -end-1 size-3.5 ring-2 ring-surface"
          />
        )}
      </span>
      <span className="min-w-0">
        <span className="block truncate text-[13.5px] font-medium text-fg">
          {name}
        </span>
        {sub && (
          <span className="block truncate text-[11.5px] text-fg-3">{sub}</span>
        )}
      </span>
    </span>
  );
}

export function MiniStat({
  label,
  value,
  sub,
}: {
  label: string;
  value: React.ReactNode;
  sub?: React.ReactNode;
}) {
  return (
    <div className="k-card min-w-0 px-5 py-4">
      <div className="k-label">{label}</div>
      <div className="k-num mt-2 truncate text-[22px] font-semibold leading-none tracking-tight">
        {value}
      </div>
      {sub && (
        <div className="mt-1.5 truncate text-[12px] text-fg-3">{sub}</div>
      )}
    </div>
  );
}

/** Quiet inline empty state for a card body. `art` only where the card has nothing to show yet (not a filter miss). */
export function CardEmpty({
  title,
  text,
  art,
  className,
  children,
  contentSlot,
}: {
  title: string;
  text?: React.ReactNode;
  art?: IllustrationName;
  className?: string;
  children?: React.ReactNode;
  /** an image slot (content service) that replaces the illustration once staff publish one */
  contentSlot?: { page: string; slot: string };
}) {
  const custom = useContentImage(contentSlot?.page ?? "", contentSlot?.slot ?? "");
  return (
    <div
      className={cn(
        "flex flex-col items-center justify-center rounded-[16px] border border-dashed border-line px-5 py-8 text-center",
        className,
      )}
    >
      {custom ? (
        <ThemedImage src={custom.url} srcSet={custom.srcset} darkSrc={custom.dark?.url} darkSrcSet={custom.dark?.srcset} alt={custom.alt} sizes="176px" className="mb-4 h-auto max-h-[132px] w-[176px] object-contain" />
      ) : (
        art && <Illustration name={art} width={176} maxHeight={132} className="mb-4" />
      )}
      <div className="text-[13.5px] font-medium text-fg-2">{title}</div>
      {text && (
        <div className="mt-1 max-w-xs text-[12.5px] leading-snug text-fg-3">
          {text}
        </div>
      )}
      {children && <div className="mt-4">{children}</div>}
    </div>
  );
}
