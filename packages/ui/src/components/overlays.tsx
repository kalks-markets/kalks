"use client";

import * as React from "react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import * as Dropdown from "@radix-ui/react-dropdown-menu";
import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import * as PopoverPrimitive from "@radix-ui/react-popover";
import { toast } from "sonner";
import { Check, Info, TriangleAlert, X } from "lucide-react";
import { cn } from "../lib/cn";

/* ------------------------------------------------------------------ */
/* Dialog: modal (centred), sheet (bottom on phones), drawer (side)    */
/* ------------------------------------------------------------------ */

export type DialogProps = {
  open?: boolean;
  onOpenChange?: (o: boolean) => void;
  trigger?: React.ReactNode;
  /** Archivo 20; then one line of consequence (with the number) as `description`. */
  title: React.ReactNode;
  description?: React.ReactNode;
  children?: React.ReactNode;
  /** The action pair: ghost (keep) + ink or red (do). Destructive = red. */
  footer?: React.ReactNode;
  width?: number;
  /** modal: centred · sheet: docked to the bottom with a grabber on phones, centred from 640 px */
  variant?: "modal" | "sheet";
  /** A side drawer (end edge). */
  side?: "right";
  className?: string;
};

/** Radius 28, s1, e2 elevation; 320 ms in, 200 ms out (fade only with reduced motion). */
export function Dialog({ open, onOpenChange, trigger, title, description, children, footer, width = 520, variant = "modal", side, className }: DialogProps) {
  const shape =
    side === "right"
      ? "k-drawer inset-y-3 end-3 w-[min(560px,calc(100vw-24px))] rounded-[26px]"
      : variant === "sheet"
        ? "k-sheet inset-x-0 bottom-0 max-h-[92dvh] rounded-t-[28px] pb-[env(safe-area-inset-bottom)] sm:inset-x-auto sm:bottom-auto sm:left-1/2 sm:top-1/2 sm:w-[calc(100vw-24px)] sm:-translate-x-1/2 sm:-translate-y-1/2 sm:rounded-[28px] sm:pb-0"
        : "k-modal left-1/2 top-1/2 max-h-[92dvh] w-[calc(100vw-24px)] -translate-x-1/2 -translate-y-1/2 rounded-[28px]";
  return (
    <DialogPrimitive.Root open={open} onOpenChange={onOpenChange}>
      {trigger && <DialogPrimitive.Trigger asChild>{trigger}</DialogPrimitive.Trigger>}
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="k-overlay fixed inset-0 z-50" />
        <DialogPrimitive.Content
          className={cn(
            "fixed z-50 flex flex-col overflow-hidden bg-surface text-fg shadow-e2 outline-none ring-1 ring-line",
            shape,
            side !== "right" && (variant === "sheet" ? "sm:max-w-[var(--k-dialog-w)]" : "max-w-[var(--k-dialog-w)]"),
            className,
          )}
          style={side === "right" ? undefined : ({ "--k-dialog-w": `${width}px` } as React.CSSProperties)}
        >
          {variant === "sheet" && side !== "right" && <div aria-hidden className="k-grabber mt-2.5 sm:hidden" />}
          <div className="flex items-start justify-between gap-4 px-6 pb-2 pt-5">
            <div className="min-w-0">
              <DialogPrimitive.Title className="k-title text-[20px] leading-[1.15]">{title}</DialogPrimitive.Title>
              {description && <DialogPrimitive.Description className="mt-2 text-[14px] leading-normal text-fg-2">{description}</DialogPrimitive.Description>}
            </div>
            <DialogPrimitive.Close className="k-icb -me-1.5 -mt-0.5" data-variant="ghost" style={{ "--s": "34px", "--r": "11px", "--ic": "17px" } as React.CSSProperties} aria-label="Close">
              <X />
            </DialogPrimitive.Close>
          </div>
          {children !== undefined && children !== null && <div className="min-h-0 flex-1 overflow-y-auto px-6 py-4">{children}</div>}
          {footer && <div className="flex flex-wrap items-center justify-end gap-2.5 px-6 pb-5 pt-2">{footer}</div>}
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}

/** Centred modal (alias of Dialog). */
export function Modal(props: Omit<DialogProps, "variant" | "side">) {
  return <Dialog {...props} variant="modal" />;
}

/** Bottom sheet on phones (grabber), centred modal from 640 px. */
export function Sheet(props: Omit<DialogProps, "variant">) {
  return <Dialog {...props} variant="sheet" />;
}

export const DialogClose = DialogPrimitive.Close;

/* ------------------------------------------------------------------ */
/* Dropdown menu                                                       */
/* ------------------------------------------------------------------ */

export function Menu({
  trigger,
  items,
  align = "end",
  width = 220,
  header,
}: {
  trigger: React.ReactNode;
  items: ({ label: React.ReactNode; icon?: React.ReactNode; onSelect?: () => void; danger?: boolean; hint?: React.ReactNode; href?: string } | "sep")[];
  align?: "start" | "end" | "center";
  width?: number;
  header?: React.ReactNode;
}) {
  return (
    <Dropdown.Root>
      <Dropdown.Trigger asChild>{trigger}</Dropdown.Trigger>
      <Dropdown.Portal>
        <Dropdown.Content
          align={align}
          sideOffset={8}
          collisionPadding={12}
          className="z-50 overflow-y-auto overscroll-contain rounded-[18px] bg-surface p-1.5 text-fg shadow-e2 ring-1 ring-line"
          // stay inside the viewport: flip/shift is automatic; cap size to the space Radix reports
          style={{ width, maxWidth: "calc(100vw - 24px)", maxHeight: "var(--radix-dropdown-menu-content-available-height)" }}
        >
          {header && <div className="border-b border-line px-3 pb-3 pt-2">{header}</div>}
          {items.map((it, i) =>
            it === "sep" ? (
              <Dropdown.Separator key={i} className="my-1 h-px bg-line" />
            ) : (
              <Dropdown.Item
                key={i}
                onSelect={it.onSelect}
                asChild={!!it.href}
                className={cn(
                  "flex cursor-pointer items-center gap-2.5 rounded-[12px] px-3 py-2 text-[13.5px] font-medium outline-none data-[highlighted]:bg-surface-3 [&_svg]:size-4",
                  it.danger ? "text-down" : "text-fg-2 data-[highlighted]:text-fg",
                )}
              >
                {it.href ? (
                  <a href={it.href}>
                    {it.icon}
                    <span className="flex-1">{it.label}</span>
                    {it.hint && <span className="text-xs text-fg-3">{it.hint}</span>}
                  </a>
                ) : (
                  <>
                    {it.icon}
                    <span className="flex-1">{it.label}</span>
                    {it.hint && <span className="text-xs text-fg-3">{it.hint}</span>}
                  </>
                )}
              </Dropdown.Item>
            ),
          )}
        </Dropdown.Content>
      </Dropdown.Portal>
    </Dropdown.Root>
  );
}

/* ------------------------------------------------------------------ */
/* Tooltip & popover                                                   */
/* ------------------------------------------------------------------ */

export const TooltipProvider = TooltipPrimitive.Provider;

/** Ink chip with an arrow. */
export function Tooltip({ content, children, side = "top" }: { content: React.ReactNode; children: React.ReactNode; side?: "top" | "right" | "bottom" | "left" }) {
  return (
    <TooltipPrimitive.Root delayDuration={120}>
      <TooltipPrimitive.Trigger asChild>{children}</TooltipPrimitive.Trigger>
      <TooltipPrimitive.Portal>
        <TooltipPrimitive.Content side={side} sideOffset={8} className="z-50 max-w-[280px] rounded-[9px] bg-fg px-2.5 py-1.5 text-[12px] font-semibold leading-snug text-bg">
          {content}
          <TooltipPrimitive.Arrow className="fill-[var(--k-fg)]" width={10} height={5} />
        </TooltipPrimitive.Content>
      </TooltipPrimitive.Portal>
    </TooltipPrimitive.Root>
  );
}

export function Popover({ trigger, children, align = "end", width = 360 }: { trigger: React.ReactNode; children: React.ReactNode; align?: "start" | "end" | "center"; width?: number }) {
  return (
    <PopoverPrimitive.Root>
      <PopoverPrimitive.Trigger asChild>{trigger}</PopoverPrimitive.Trigger>
      <PopoverPrimitive.Portal>
        <PopoverPrimitive.Content
          align={align}
          sideOffset={10}
          collisionPadding={12}
          className="z-50 overflow-y-auto overscroll-contain rounded-[20px] bg-surface text-fg shadow-e2 outline-none ring-1 ring-line"
          style={{ width, maxWidth: "calc(100vw - 24px)", maxHeight: "var(--radix-popover-content-available-height)" }}
        >
          {children}
        </PopoverPrimitive.Content>
      </PopoverPrimitive.Portal>
    </PopoverPrimitive.Root>
  );
}

/* ------------------------------------------------------------------ */
/* Toast: iOS banner                                                   */
/* ------------------------------------------------------------------ */

export type ToastTone = "fill" | "reject" | "ok" | "warn" | "info";

const TOAST_TILE: Record<ToastTone, { cls: string; icon: React.ReactNode }> = {
  fill: { cls: "bg-up-face text-white", icon: <Check strokeWidth={2.5} /> },
  reject: { cls: "bg-red text-on-red", icon: <X strokeWidth={2.5} /> },
  ok: { cls: "bg-ok-soft text-ok", icon: <Check strokeWidth={2.5} /> },
  warn: { cls: "bg-yellow text-on-yellow", icon: <TriangleAlert strokeWidth={2.25} /> },
  info: { cls: "bg-surface-3 text-fg-2", icon: <Info strokeWidth={2.25} /> },
};

/**
 * Frosted banner: a 38 px icon tile coloured by meaning (blue fill for fills, red for rejects), bold title, one
 * line, the time on the right. Rendered by `showToast` (sonner), or inline.
 */
export function Toast({ tone = "info", icon, title, text, time, className }: { tone?: ToastTone; icon?: React.ReactNode; title: React.ReactNode; text?: React.ReactNode; time?: React.ReactNode; className?: string }) {
  const tile = TOAST_TILE[tone];
  return (
    <div role="status" className={cn("k-toast", className)}>
      <span className={cn("k-toast-ic", tile.cls)}>{icon ?? tile.icon}</span>
      <span className="min-w-0 flex-1">
        <b className="block text-[14px] font-semibold leading-tight">{title}</b>
        {text && <span className="mt-0.5 block text-[13px] leading-snug text-fg-2">{text}</span>}
      </span>
      {time && <time className="self-start font-mono text-[11.5px] font-medium text-fg-3">{time}</time>}
    </div>
  );
}

/** Shows a banner toast (drops in from the top; swipe to dismiss on touch). */
export function showToast(t: { tone?: ToastTone; title: React.ReactNode; text?: React.ReactNode; time?: React.ReactNode; icon?: React.ReactNode; duration?: number }) {
  return toast.custom(() => <Toast tone={t.tone} title={t.title} text={t.text} time={t.time ?? "now"} icon={t.icon} className="w-[min(420px,calc(100vw-20px))]" />, { duration: t.duration ?? 4000 });
}
