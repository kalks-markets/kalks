"use client";

import * as React from "react";
import Link from "next/link";
import { CalendarClock, Download, FileSpreadsheet, FileText, Mail } from "lucide-react";
import { toast } from "sonner";
import { Button, Menu } from "@kalks/ui";

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** "2026-09-24" → "24 Sep" (timezone-independent). */
export function dayLabel(iso: string, year = false) {
  const [y, m, d] = iso.split("-");
  return `${Number(d)} ${MONTHS[Number(m) - 1]}${year ? ` ${y}` : ""}`;
}

export function weekday(iso: string) {
  const [y, m, d] = iso.split("-").map(Number);
  return ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][new Date(Date.UTC(y!, m! - 1, d!)).getUTCDay()];
}

export const RANGES = ["7D", "30D", "90D", "YTD"] as const;
export type Range = (typeof RANGES)[number];
export const RANGE_DAYS: Record<Range, number> = { "7D": 7, "30D": 30, "90D": 90, YTD: 267 };

/** Export menu + schedule shortcut used in analytics page headers. */
export function ExportActions({ name, scheduleHref = "/analytics/scheduled" }: { name: string; scheduleHref?: string }) {
  const done = (fmt: string) =>
    toast.success(`${name} exported`, { description: `${name.toLowerCase().replace(/[^a-z0-9]+/g, "-")}-2026-09-24.${fmt.toLowerCase()} is ready in Downloads` });
  return (
    <>
      <Link href={scheduleHref}>
        <Button variant="surface" size="md">
          <CalendarClock /> Schedule
        </Button>
      </Link>
      <Menu
        trigger={
          <Button variant="ember" size="md">
            <Download /> Export
          </Button>
        }
        items={[
          { label: "CSV", icon: <FileText />, hint: ".csv", onSelect: () => done("CSV") },
          { label: "Excel", icon: <FileSpreadsheet />, hint: ".xlsx", onSelect: () => done("XLSX") },
          { label: "PDF report", icon: <FileText />, hint: ".pdf", onSelect: () => done("PDF") },
          "sep",
          { label: "Email to me", icon: <Mail />, onSelect: () => toast.success("Report queued", { description: "Sending to priya.nair@kalkstrade.com within 2 minutes" }) },
        ]}
      />
    </>
  );
}

export function pct(a: number, b: number, d = 1) {
  return b ? `${((a / b) * 100).toFixed(d)}%` : "—";
}
