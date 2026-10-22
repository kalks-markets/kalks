"use client";

// Browser client for the Back Office Academy BFF (/api/academy/*, see app/api/academy/[...path]/route.ts).

import * as React from "react";

export type Kind = "phase" | "section" | "chapter" | "exam" | "term";
export type Source = "default" | "override" | "custom";
type Meta = { slug: string; published: boolean; source: Source; updated_at: string; updated_by: string; lang: string; order: number; parent: string };

export type TreeChapter = Meta & { title: string; summary: string; words: number; minutes: number; questions: number; learners: number; completed: number; quiz_avg: number | null };
/** Tracks this build knows (services/academy/src/content.rs TRACKS): core tracks, then product tracks. */
export type Track = "fundamental" | "technical" | "options";
/** A section's track as served; the service may add tracks this build doesn't know yet. */
export type TrackKey = Track | (string & {});
export type TreeSection = Meta & { title: string; summary: string; track: TrackKey; chapters: TreeChapter[] };
/** `elective`: a product phase (e.g. phase 9 "Kalks FX Options", one `options` section). Older builds don't send it. */
export type TreePhase = Meta & { title: string; level: string; summary: string; elective?: boolean; exam: (Meta & { pass_mark: number; questions: number }) | null; sections: TreeSection[] };
export type Tree = { tenant: string; lang: string; languages: string[]; phases: TreePhase[] };

export type QuizQ = { question: string; options: string[]; answer: number; explanation: string; chapter?: string };
export type NodeData = {
  title?: string;
  summary?: string;
  level?: string;
  track?: string;
  body?: string;
  takeaways?: string[];
  practice?: { label: string; symbol?: string | null } | null;
  quiz?: QuizQ[];
  words?: number;
  minutes?: number;
  pass_mark?: number;
  questions?: QuizQ[];
  // glossary terms
  term?: string;
  category?: string;
  definition?: string;
  related?: string[];
};
export type GlossaryTerm = Meta & { term: string; category: string; definition: string; related: string[] };
export type NodeFull = Meta & { kind: Kind; data: NodeData; default: { data: NodeData; published: boolean; order: number } | null };

export type Stats = {
  learners: number;
  active_7d: number;
  completions: number;
  completions_30d: number;
  quiz_avg: number | null;
  exam_attempts: number;
  exam_passed: number;
  exam_avg: number | null;
  certificates: number;
  phases: { slug: string; order: number; title: string; level: string; chapters: number; learners: number; completions: number; exam_attempts: number; exam_passed: number; certificates: number }[];
  top_chapters: { slug: string; title: string; phase: string; learners: number; completed: number; quiz_avg: number | null }[];
  recent_certificates: { code: string; learner_name: string; phase_order: number; phase_title: string; score_pct: number; issued_at: string }[];
};
export type AuditRow = { staff: string; action: string; kind: string; slug: string; lang: string; detail: { title?: string }; at: string };

export class CmsError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
    public issues: string[] = [],
  ) {
    super(message);
  }
}

export async function cms<T>(path: string, init?: { method?: "GET" | "POST" | "PUT" | "DELETE"; body?: unknown; signal?: AbortSignal }): Promise<T> {
  const method = init?.method ?? (init?.body !== undefined ? "POST" : "GET");
  let res: Response;
  try {
    res = await fetch(`/api/academy/${path}`, {
      method,
      headers: method === "GET" ? undefined : { "content-type": "application/json" },
      body: init?.body !== undefined ? JSON.stringify(init.body) : undefined,
      cache: "no-store",
      signal: init?.signal,
    });
  } catch (e) {
    if ((e as Error).name === "AbortError") throw e;
    throw new CmsError(0, "network", "Network error. Check your connection and try again.");
  }
  const data = (await res.json().catch(() => ({}))) as { error?: { code?: string; message?: string; issues?: string[] } };
  if (!res.ok) throw new CmsError(res.status, data.error?.code ?? "error", data.error?.message ?? "Something went wrong.", data.error?.issues ?? []);
  return data as T;
}

export function useCms<T>(path: string | null) {
  const [data, setData] = React.useState<T | null>(null);
  const [error, setError] = React.useState<CmsError | null>(null);
  const [tick, setTick] = React.useState(0);
  const reload = React.useCallback(() => setTick((t) => t + 1), []);
  React.useEffect(() => {
    if (!path) return;
    const ctl = new AbortController();
    cms<T>(path, { signal: ctl.signal })
      .then((d) => {
        setData(d);
        setError(null);
      })
      .catch((e) => {
        if ((e as Error).name !== "AbortError") setError(e instanceof CmsError ? e : new CmsError(0, "error", "Something went wrong."));
      });
    return () => ctl.abort();
  }, [path, tick]);
  return { data, error, reload };
}

/** Text colour of a track label in the course tree; unknown tracks are muted. */
const TRACK_TEXT: Record<Track, string> = { fundamental: "text-info", technical: "text-ember", options: "text-gold" };
export const trackText = (k: TrackKey) => (Object.prototype.hasOwnProperty.call(TRACK_TEXT, k) ? TRACK_TEXT[k as Track] : "text-fg-3");
/** Display order of tracks (core, then product, then unknown). */
export const TRACK_ORDER: readonly Track[] = ["fundamental", "technical", "options"];

export const fmtWhen = (iso: string) => new Date(iso).toLocaleString("en-GB", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
export const LEVELS = ["Beginner", "Intermediate", "Advanced", "Professional"] as const;
/** Glossary categories (services/academy/src/content.rs CATEGORIES). */
export const CATEGORIES = ["Markets", "Trading mechanics", "Technical analysis", "Fundamental analysis", "Risk management", "Psychology", "Platform"] as const;
