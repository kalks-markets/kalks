"use client";

import * as React from "react";
import { cn } from "../lib/cn";

/**
 * Before Kalks 2 a card with a pointer-following spotlight; Kalks 2 has no cursor effects (KALKS2 §7), so it is the
 * solid card. `color` is accepted for compatibility.
 */
export function SpotlightCard({ className, children, color: _color, hot, ...props }: React.HTMLAttributes<HTMLDivElement> & { color?: string; hot?: boolean }) {
  return (
    <div className={cn(hot ? "k-hot-card rounded-[var(--radius-card)]" : "k-card", "overflow-hidden", className)} {...props}>
      {children}
    </div>
  );
}

/**
 * Formerly drifting star particles on hero cards. Removed by design direction (no moving decoration on
 * cards); kept as a no-op so existing call sites stay valid.
 */
export function Starfield(_props: { density?: number; className?: string }) {
  return null;
}

/** Fade-up container for page sections. */
export function Reveal({ children, delay = 0, className }: { children: React.ReactNode; delay?: number; className?: string }) {
  return (
    // CSS entrance (styles.css .k-reveal): visible from the server-rendered HTML, no wait for hydration
    <div className={cn("k-reveal", className)} style={delay ? ({ "--k-reveal-delay": `${delay}s` } as React.CSSProperties) : undefined}>
      {children}
    </div>
  );
}
