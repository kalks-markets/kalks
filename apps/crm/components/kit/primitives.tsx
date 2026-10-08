"use client";

// Client Area-only pieces of the kit. Button, IconButton, Chip, StatusChip, Card and CardHeader come from @kalks/ui
// (Kalks 2, docs/design/KALKS2.md); the kit index re-exports them.

import * as React from "react";
import { cn } from "@kalks/ui";

/** The chip tones the Client Area pages use (a subset of @kalks/ui's ChipTone). */
export type ChipTone = "neutral" | "up" | "down" | "ember" | "gold" | "warn" | "info" | "solid";

/** Icon tile tinted by meaning (globals.css .k-tile-*: accent = the brand red, amber = yellow, sky = blue, mint = green, …). */
export type TileTone = "accent" | "amber" | "coral" | "pink" | "lavender" | "mint" | "sky" | "neutral";
export function IconTile({ tone = "accent", size = 44, className, children }: { tone?: TileTone; size?: number; className?: string; children: React.ReactNode }) {
  return (
    <span className={cn("k-tile shrink-0", `k-tile-${tone}`, className)} style={{ width: size, height: size, borderRadius: Math.round(size * 0.32) }}>
      {children}
    </span>
  );
}
