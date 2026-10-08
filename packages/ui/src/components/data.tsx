"use client";

import * as React from "react";
import Link from "next/link";
import { ArrowUpRight, Check, Copy, ChevronLeft, ChevronRight, ArrowUpDown, Search, Download } from "lucide-react";
import { toast } from "sonner";
import { cn } from "../lib/cn";
import { Chip, type ChipTone, IconButton, Button } from "./primitives";
import { Icon3D } from "./avatars";
import { Illustration, type IllustrationName } from "./illustration";
import { SpotlightCard } from "../effects/effects";

/* ------------------------------------------------------------------ */
/* KPI card — Signal-AI reference                                      */
/* ------------------------------------------------------------------ */

export function KpiCard({
  label,
  value,
  icon,
  chip,
  chipTone = "neutral",
  href,
  illustration,
  hot,
  footer,
  className,
  delay = 0,
}: {
  label: string;
  value: React.ReactNode;
  icon?: React.ReactNode;
  chip?: React.ReactNode;
  chipTone?: ChipTone;
  href?: string;
  illustration?: string;
  hot?: boolean;
  footer?: React.ReactNode;
  className?: string;
  delay?: number;
}) {
  return (
    <div className={cn("k-reveal min-w-0", className)} style={{ "--k-reveal-y": "14px", "--k-reveal-ms": "500ms", ...(delay ? { "--k-reveal-delay": `${delay}s` } : {}) } as React.CSSProperties}>
      <SpotlightCard hot={hot} className="flex h-full flex-col">
        <div className="relative flex flex-1 flex-col px-6 pb-5 pt-6">
          {/* min-h keeps the value on the same line in a row of cards with and without an icon */}
          <div className="flex min-h-10 items-start justify-between">
            <span className="k-label">{label}</span>
            {icon && <span className="grid size-[38px] place-items-center rounded-[12px] bg-surface-3 text-fg-2 [&_svg]:size-[18px]">{icon}</span>}
          </div>
          <div className="k-money mt-5 text-[30px] leading-none text-fg sm:text-[32px]">{value}</div>
        </div>
        {(chip || href || footer) && (
          <div className="flex items-center justify-between gap-2 rounded-b-[var(--radius-card)] border-t border-line bg-surface-2 px-6 py-3.5">
            {footer ?? (chip ? <Chip tone={chipTone}>{chip}</Chip> : <span />)}
            {href && (
              <Link href={href} className="text-fg-3 transition-colors hover:text-fg" aria-label={`Open ${label}`}>
                <ArrowUpRight className="size-4" />
              </Link>
            )}
          </div>
        )}
      </SpotlightCard>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Copy                                                                */
/* ------------------------------------------------------------------ */

export function CopyButton({ value, label, className }: { value: string; label?: string; className?: string }) {
  const [done, setDone] = React.useState(false);
  return (
    <button
      type="button"
      onClick={() => {
        navigator.clipboard?.writeText(value).catch(() => {});
        setDone(true);
        toast.success(label ? `${label} copied` : "Copied to clipboard");
        setTimeout(() => setDone(false), 1400);
      }}
      className={cn("inline-grid size-6 place-items-center rounded-md text-fg-3 transition-colors hover:bg-surface-3 hover:text-fg", className)}
      aria-label="Copy"
    >
      {done ? <Check className="size-3.5 text-up" /> : <Copy className="size-3.5" />}
    </button>
  );
}

/* ------------------------------------------------------------------ */
/* Empty state                                                         */
/* ------------------------------------------------------------------ */

/** `art` shows one of the founder's illustrations instead of the icon: only for a state with nothing to show (the
 *  whole page or panel is empty) or a result worth marking, never for a quiet line inside a data screen. */
export function EmptyState({ illustration = "package", art, title, text, action, className }: { illustration?: string; art?: IllustrationName; title: string; text?: string; action?: React.ReactNode; className?: string }) {
  return (
    <div className={cn("flex flex-col items-center justify-center px-6 py-14 text-center", className)}>
      {art ? <Illustration name={art} width={208} maxHeight={156} className="mb-2" /> : <Icon3D name={illustration} size={72} />}
      <h4 className="mt-4 text-base font-medium">{title}</h4>
      {text && <p className="mt-1 max-w-sm text-sm text-fg-3">{text}</p>}
      {action && <div className="mt-5">{action}</div>}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Label / value stack                                                 */
/* ------------------------------------------------------------------ */

export function Stat({ label, value, sub, className, align = "left" }: { label: React.ReactNode; value: React.ReactNode; sub?: React.ReactNode; className?: string; align?: "left" | "right" }) {
  return (
    <div className={cn("min-w-0", align === "right" && "text-right", className)}>
      <div className="text-[11.5px] uppercase tracking-[0.05em] text-fg-3">{label}</div>
      <div className="k-num mt-1 truncate text-[15px] font-medium text-fg">{value}</div>
      {sub && <div className="mt-0.5 text-xs text-fg-3">{sub}</div>}
    </div>
  );
}

export function KeyValue({ rows, className }: { rows: [React.ReactNode, React.ReactNode][]; className?: string }) {
  return (
    <dl className={cn("divide-y divide-line", className)}>
      {rows.map(([k, v], i) => (
        <div key={i} className="flex items-center justify-between gap-4 py-3 text-sm">
          <dt className="text-fg-3">{k}</dt>
          <dd className="k-num text-right font-medium text-fg">{v}</dd>
        </div>
      ))}
    </dl>
  );
}

/* ------------------------------------------------------------------ */
/* Data table                                                          */
/* ------------------------------------------------------------------ */

export interface Column<T> {
  key: string;
  header: React.ReactNode;
  cell: (row: T, i: number) => React.ReactNode;
  align?: "left" | "right" | "center";
  width?: string;
  sort?: (row: T) => number | string;
  /** Plain value used for CSV export (falls back to `sort`). */
  csv?: (row: T) => number | string;
  hideOn?: "sm" | "md" | "lg" | "xl";
  className?: string;
}

export function DataTable<T>({
  columns,
  rows,
  pageSize = 10,
  search,
  searchPlaceholder = "Search…",
  toolbar,
  onRowClick,
  empty,
  dense,
  exportName,
  rowKey,
  className,
}: {
  columns: Column<T>[];
  rows: T[];
  pageSize?: number;
  search?: (row: T) => string;
  searchPlaceholder?: string;
  toolbar?: React.ReactNode;
  onRowClick?: (row: T) => void;
  empty?: React.ReactNode;
  dense?: boolean;
  exportName?: string;
  rowKey?: (row: T, i: number) => string;
  className?: string;
}) {
  const [q, setQ] = React.useState("");
  const [page, setPage] = React.useState(0);
  const [sort, setSort] = React.useState<{ key: string; dir: 1 | -1 } | null>(null);

  const filtered = React.useMemo(() => {
    let r = rows;
    if (search && q) r = r.filter((x) => search(x).toLowerCase().includes(q.toLowerCase()));
    if (sort) {
      const col = columns.find((c) => c.key === sort.key);
      if (col?.sort) r = [...r].sort((a, b) => (col.sort!(a) > col.sort!(b) ? sort.dir : -sort.dir));
    }
    return r;
  }, [rows, q, sort, columns, search]);
  const pages = Math.max(1, Math.ceil(filtered.length / pageSize));
  const view = filtered.slice(page * pageSize, page * pageSize + pageSize);
  React.useEffect(() => setPage(0), [q, rows.length]);

  const hide = (h?: Column<T>["hideOn"]) => (h === "sm" ? "hidden sm:table-cell" : h === "md" ? "hidden md:table-cell" : h === "lg" ? "hidden lg:table-cell" : h === "xl" ? "hidden xl:table-cell" : "");

  return (
    <div className={cn("min-w-0", className)}>
      {(search || toolbar || exportName) && (
        <div className="mb-3 flex flex-wrap items-center gap-2">
          {toolbar}
          <div className="ml-auto flex items-center gap-2">
            {search && (
              <div className="k-field" data-size="sm">
                <Search className="size-3.5 shrink-0 text-fg-3" />
                <input value={q} onChange={(e) => setQ(e.target.value)} placeholder={searchPlaceholder} aria-label={searchPlaceholder} className="w-40 sm:w-52" />
              </div>
            )}
            {exportName && (
              <Button size="sm" variant="surface" onClick={() => downloadCsv(exportName, columns, filtered)}>
                <Download /> CSV
              </Button>
            )}
          </div>
        </div>
      )}
      <div className="overflow-x-auto">
        <table className="k-table" data-dense={dense ? "" : undefined} style={{ minWidth: Math.min(640, columns.length * 110) }}>
          <thead>
            <tr>
              {columns.map((c) => (
                <th
                  key={c.key}
                  style={{ width: c.width }}
                  aria-sort={sort?.key === c.key ? (sort.dir === 1 ? "ascending" : "descending") : undefined}
                  className={cn(c.align === "right" ? "!text-right" : c.align === "center" ? "!text-center" : "", hide(c.hideOn))}
                >
                  {c.sort ? (
                    <button type="button" className={cn("inline-flex items-center gap-1 uppercase tracking-[inherit] hover:text-fg", c.align === "right" && "flex-row-reverse")} onClick={() => setSort((s) => (s?.key === c.key ? { key: c.key, dir: s.dir === 1 ? -1 : 1 } : { key: c.key, dir: -1 }))}>
                      {c.header}
                      <ArrowUpDown className={cn("size-3", sort?.key === c.key ? "text-fg" : "opacity-50")} />
                    </button>
                  ) : (
                    c.header
                  )}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {view.map((r, ri) => (
              <tr key={rowKey ? rowKey(r, ri) : ri} onClick={onRowClick ? () => onRowClick(r) : undefined} className={cn(onRowClick && "cursor-pointer")}>
                {columns.map((c) => (
                  <td
                    key={c.key}
                    className={cn(c.align === "right" ? "k-num text-right" : c.align === "center" ? "text-center" : "text-start", hide(c.hideOn), c.className)}
                  >
                    {c.cell(r, page * pageSize + ri)}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
        {view.length === 0 && (empty ?? <EmptyState title="Nothing here yet" text="Try changing filters or the date range." illustration="magnifying_glass_tilted_left" />)}
      </div>
      {pages > 1 && (
        <div className="mt-4 flex items-center justify-between text-[12.5px] text-fg-3">
          <span className="k-num">
            {page * pageSize + 1}–{Math.min(filtered.length, (page + 1) * pageSize)} of {filtered.length}
          </span>
          <div className="flex items-center gap-1.5">
            <IconButton size="sm" disabled={page === 0} onClick={() => setPage((p) => p - 1)} aria-label="Previous page">
              <ChevronLeft />
            </IconButton>
            <span className="k-num px-2">
              {page + 1} / {pages}
            </span>
            <IconButton size="sm" disabled={page >= pages - 1} onClick={() => setPage((p) => p + 1)} aria-label="Next page">
              <ChevronRight />
            </IconButton>
          </div>
        </div>
      )}
    </div>
  );
}

function downloadCsv<T>(name: string, columns: Column<T>[], rows: T[]) {
  const cols = columns.filter((c) => c.csv || c.sort);
  if (cols.length === 0) {
    toast.error("Nothing to export", { description: "No exportable columns." });
    return;
  }
  const esc = (v: unknown) => {
    const s = String(v ?? "");
    return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
  };
  const head = cols.map((c) => esc(typeof c.header === "string" ? c.header : c.key)).join(",");
  const body = rows.map((r) => cols.map((c) => esc((c.csv ?? c.sort)!(r))).join(",")).join("\n");
  const url = URL.createObjectURL(new Blob([head + "\n" + body], { type: "text/csv;charset=utf-8" }));
  const a = document.createElement("a");
  a.href = url;
  a.download = `${name}.csv`;
  a.click();
  URL.revokeObjectURL(url);
  toast.success(`${name}.csv downloaded`, { description: `${rows.length} rows · ${cols.length} columns` });
}

/* ------------------------------------------------------------------ */
/* List row (signals / trades / watchlist style)                       */
/* ------------------------------------------------------------------ */

export function ListRow({ children, className, onClick, href, target, rel }: { children: React.ReactNode; className?: string; onClick?: () => void; href?: string; target?: string; rel?: string }) {
  const cls = cn("k-row flex items-center gap-3 px-4 py-3 transition-colors hover:bg-[color-mix(in_srgb,var(--k-surface-3)_60%,transparent)]", (onClick || href) && "cursor-pointer", className);
  if (href)
    return (
      <Link href={href} target={target} rel={rel} className={cls}>
        {children}
      </Link>
    );
  return (
    <div className={cls} onClick={onClick}>
      {children}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Table primitives (KALKS2 §6): caps heads, hairlines, row hover,     */
/* numeric columns mono + right-aligned                                */
/* ------------------------------------------------------------------ */

export function Table({ dense, className, children, ...props }: React.TableHTMLAttributes<HTMLTableElement> & { dense?: boolean }) {
  return (
    <div className="min-w-0 overflow-x-auto">
      <table className={cn("k-table", className)} data-dense={dense ? "" : undefined} {...props}>
        {children}
      </table>
    </div>
  );
}

/** Header cell; `num` right-aligns it over a numeric column. */
export function Th({ num, className, ...props }: React.ThHTMLAttributes<HTMLTableCellElement> & { num?: boolean }) {
  return <th scope="col" className={cn(num && "k-td-num", className)} {...props} />;
}

/** Body cell; `num` = mono, tabular, right-aligned. */
export function Td({ num, className, ...props }: React.TdHTMLAttributes<HTMLTableCellElement> & { num?: boolean }) {
  return <td className={cn(num && "k-td-num", className)} {...props} />;
}

/** First-column cell: a 38 px icon tile tinted by meaning + title + one sub-line. */
export function TitleCell({ icon, tone = "neutral", title, sub }: { icon: React.ReactNode; tone?: "neutral" | "up" | "down" | "yellow" | "red"; title: React.ReactNode; sub?: React.ReactNode }) {
  const tint = { neutral: "bg-surface-3 text-fg-2", up: "bg-up-soft text-up", down: "bg-down-soft text-down", yellow: "bg-yellow-soft text-yellow", red: "bg-red-soft text-red" }[tone];
  return (
    <div className="flex items-center gap-3">
      <span className={cn("grid size-[38px] shrink-0 place-items-center rounded-[12px] [&_svg]:size-[18px]", tint)}>{icon}</span>
      <span className="min-w-0">
        <b className="block truncate text-[14px] font-semibold leading-tight">{title}</b>
        {sub && <span className="mt-[3px] block truncate text-[12.5px] leading-tight text-fg-3">{sub}</span>}
      </span>
    </div>
  );
}
