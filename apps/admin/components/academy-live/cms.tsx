"use client";

import * as React from "react";
import { ArrowDown, ArrowUp, Award, BookOpen, ChevronDown, ChevronRight, Eye, FileText, GraduationCap, History, Plus, RotateCcw, RotateCw, Save, Trash2, Users } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, CardHeader, Chip, Dialog, EmptyState, KpiCard, PageHeader, Progress, Reveal, Segmented, Skeleton, Toggle, cn } from "@kalks/ui";
import { useStaff } from "@/components/staff-session";
import { contentAllows } from "@/lib/academy";
import { CmsError, LEVELS, TRACK_ORDER, cms, fmtWhen, trackText, useCms, type AuditRow, type Kind, type NodeData, type NodeFull, type QuizQ, type Stats, type Track, type Tree, type TreePhase } from "./api";
import { Markdown } from "./markdown";
import { QuizEditor, inputCls } from "./quiz-editor";
import { useConfirm } from "@/components/confirm";

type Sel = { kind: Kind; slug: string };

const SOURCE_CHIP = { default: null, override: { tone: "info" as const, label: "Edited" }, custom: { tone: "gold" as const, label: "Custom" } };

function StatusDot({ on }: { on: boolean }) {
  return <span className={cn("size-1.5 shrink-0 rounded-full", on ? "bg-up" : "bg-fg-3/50")} title={on ? "Published" : "Unpublished"} />;
}

/* ------------------------------------------------------------------ */
/* Course tree                                                         */
/* ------------------------------------------------------------------ */

function CourseTree({ tree, sel, onSelect, canWrite, onReordered }: { tree: Tree; sel: Sel | null; onSelect: (s: Sel) => void; canWrite: boolean; onReordered: () => void }) {
  const phaseOfSel = React.useMemo(() => {
    if (!sel) return tree.phases[0]?.slug;
    for (const p of tree.phases) {
      if (p.slug === sel.slug) return p.slug;
      for (const s of p.sections) if (s.slug === sel.slug || s.chapters.some((c) => c.slug === sel.slug)) return p.slug;
    }
    return tree.phases[0]?.slug;
  }, [tree, sel]);
  const [open, setOpen] = React.useState<string | undefined>(phaseOfSel);
  React.useEffect(() => setOpen(phaseOfSel), [phaseOfSel]);

  const move = async (section: string, slugs: string[], i: number, d: -1 | 1) => {
    const j = i + d;
    if (j < 0 || j >= slugs.length) return;
    const next = [...slugs];
    [next[i], next[j]] = [next[j]!, next[i]!];
    try {
      await cms("reorder", { body: { lang: tree.lang, kind: "chapter", parent: section, slugs: next } });
      onReordered();
    } catch (e) {
      toast.error("Couldn't reorder", { description: (e as Error).message });
    }
  };

  const isSel = (kind: Kind, slug: string) => sel?.kind === kind && sel.slug === slug;
  return (
    <div className="space-y-1.5" data-testid="course-tree">
      {tree.phases.map((p) => {
        const expanded = open === p.slug;
        const chapters = p.sections.reduce((n, s) => n + s.chapters.length, 0);
        return (
          <div key={p.slug} className="rounded-[16px] border border-line bg-surface-2/40">
            <div className="flex items-center gap-2 px-3 py-2.5">
              <button type="button" onClick={() => setOpen(expanded ? undefined : p.slug)} aria-label={expanded ? "Collapse" : "Expand"} className="grid size-6 place-items-center rounded-full text-fg-3 hover:text-fg">
                {expanded ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />}
              </button>
              <button type="button" onClick={() => onSelect({ kind: "phase", slug: p.slug })} className={cn("min-w-0 flex-1 text-left", isSel("phase", p.slug) && "text-ember")}>
                <div className="flex items-center gap-2">
                  <StatusDot on={p.published} />
                  <span className="truncate text-[13.5px] font-medium">
                    {p.order}. {p.title}
                  </span>
                </div>
                <div className="k-num ml-3.5 text-[11px] text-fg-3">
                  {p.level} · {chapters} chapters
                  {p.elective && " · elective"}
                </div>
              </button>
              {SOURCE_CHIP[p.source] && (
                <Chip size="sm" tone={SOURCE_CHIP[p.source]!.tone}>
                  {SOURCE_CHIP[p.source]!.label}
                </Chip>
              )}
            </div>
            {expanded && (
              <div className="space-y-2 border-t border-line px-2 pb-2 pt-2">
                {p.sections.map((s) => {
                  const slugs = s.chapters.map((c) => c.slug);
                  return (
                    <div key={s.slug}>
                      <button type="button" onClick={() => onSelect({ kind: "section", slug: s.slug })} className={cn("flex w-full items-center gap-2 rounded-[10px] px-2 py-1.5 text-left", isSel("section", s.slug) ? "bg-surface-3 text-fg" : "hover:bg-surface-3/60")}>
                        <StatusDot on={s.published} />
                        <span className={cn("text-[10.5px] font-semibold uppercase tracking-wider", trackText(s.track))}>{s.track}</span>
                        <span className="truncate text-[12.5px] text-fg-2">{s.title}</span>
                      </button>
                      <ol className="mt-0.5 space-y-0.5">
                        {s.chapters.map((c, i) => (
                          <li key={c.slug} className={cn("group flex items-center gap-1 rounded-[10px] pl-2 pr-1", isSel("chapter", c.slug) ? "bg-ember-soft" : "hover:bg-surface-3/60")}>
                            <button type="button" onClick={() => onSelect({ kind: "chapter", slug: c.slug })} className="flex min-w-0 flex-1 items-center gap-2 py-1.5 text-left" data-testid={`tree-chapter-${c.slug}`}>
                              <span className="k-num w-5 shrink-0 text-right text-[11px] text-fg-3">{i + 1}</span>
                              <StatusDot on={c.published} />
                              <span className={cn("truncate text-[12.5px]", isSel("chapter", c.slug) ? "text-fg" : "text-fg-2", !c.published && "text-fg-3")}>{c.title}</span>
                              {SOURCE_CHIP[c.source] && <span className={cn("shrink-0 text-[10px] font-medium", c.source === "custom" ? "text-gold" : "text-info")}>{SOURCE_CHIP[c.source]!.label}</span>}
                            </button>
                            {canWrite && (
                              <span className="flex shrink-0 opacity-0 transition-opacity group-hover:opacity-100">
                                <button type="button" aria-label="Move up" disabled={i === 0} onClick={() => move(s.slug, slugs, i, -1)} className="grid size-6 place-items-center rounded-full text-fg-3 hover:text-fg disabled:opacity-30">
                                  <ArrowUp className="size-3" />
                                </button>
                                <button type="button" aria-label="Move down" disabled={i === slugs.length - 1} onClick={() => move(s.slug, slugs, i, 1)} className="grid size-6 place-items-center rounded-full text-fg-3 hover:text-fg disabled:opacity-30">
                                  <ArrowDown className="size-3" />
                                </button>
                              </span>
                            )}
                          </li>
                        ))}
                      </ol>
                    </div>
                  );
                })}
                {p.exam && (
                  <button type="button" onClick={() => onSelect({ kind: "exam", slug: p.slug })} className={cn("flex w-full items-center gap-2 rounded-[10px] px-2 py-1.5 text-left", isSel("exam", p.slug) ? "bg-surface-3" : "hover:bg-surface-3/60")}>
                    <GraduationCap className="size-3.5 text-gold" />
                    <span className="text-[12.5px] text-fg-2">Final exam</span>
                    <span className="k-num ml-auto text-[11px] text-fg-3">
                      {p.exam.questions} q · {p.exam.pass_mark}%
                    </span>
                  </button>
                )}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Editor                                                              */
/* ------------------------------------------------------------------ */

type Draft = { data: NodeData; published: boolean };

function Editor({ sel, lang, canWrite, onSaved, onDeleted }: { sel: Sel; lang: string; canWrite: boolean; onSaved: () => void; onDeleted: () => void }) {
  const [ask, confirmDialog] = useConfirm();
  const { data, error, reload } = useCms<{ node: NodeFull }>(`nodes/${sel.kind}/${sel.slug}?lang=${lang}`);
  const [draft, setDraft] = React.useState<Draft | null>(null);
  const [view, setView] = React.useState<"write" | "preview" | "split">("split");
  const [busy, setBusy] = React.useState(false);
  const [issues, setIssues] = React.useState<string[]>([]);
  const [warnings, setWarnings] = React.useState<string[]>([]);
  const node = data?.node && data.node.slug === sel.slug && data.node.kind === sel.kind ? data.node : null;

  React.useEffect(() => {
    if (node) setDraft({ data: structuredClone(node.data), published: node.published });
    setIssues([]);
    setWarnings([]);
  }, [node]);

  if (error) return <EmptyState illustration="satellite_antenna" title="Couldn't load this item" text={error.message} action={<Button variant="surface" onClick={reload}><RotateCw /> Try again</Button>} />;
  if (!node || !draft) return <Skeleton className="h-[520px] w-full rounded-[18px]" />;

  const d = draft.data;
  const set = (patch: Partial<NodeData>) => setDraft({ ...draft, data: { ...draft.data, ...patch } });
  const dirty = JSON.stringify(draft.data) !== JSON.stringify(node.data);
  const ro = !canWrite;

  const put = async (body: Record<string, unknown>, ok: string) => {
    setBusy(true);
    setIssues([]);
    try {
      const r = await cms<{ node: NodeFull; warnings: string[] }>(`nodes/${sel.kind}/${sel.slug}`, { method: "PUT", body: { lang, ...body } });
      setWarnings(r.warnings ?? []);
      toast.success(ok, { description: r.warnings?.[0] });
      reload();
      onSaved();
    } catch (e) {
      const ce = e as CmsError;
      setIssues(ce.issues?.length ? ce.issues : [ce.message]);
      toast.error("Not saved", { description: ce.message });
    } finally {
      setBusy(false);
    }
  };
  // the service keeps one version per item (no drafts): saving a published item changes what learners see at once
  const save = async () => {
    if (node.published && !(await ask({ title: "Update the live version?", text: `This ${sel.kind} is published: learners see your changes as soon as you save. To prepare changes without learners seeing them, unpublish it first.`, confirm: "Save and update" }))) return;
    const data: Record<string, unknown> = { ...draft.data };
    delete data.words;
    delete data.minutes;
    return put({ data }, "Saved");
  };
  const togglePublish = () => put({ published: !node.published, ...(dirty ? { data: draft.data } : {}) }, node.published ? "Unpublished" : "Published");
  const reset = async () => {
    if (!(await ask({ title: "Restore the platform version?", text: "Your changes to this page are discarded.", confirm: "Restore", tone: "danger" }))) return;
    setBusy(true);
    try {
      await cms(`nodes/${sel.kind}/${sel.slug}?lang=${lang}`, { method: "DELETE" });
      toast.success("Restored the platform version");
      reload();
      onSaved();
    } catch (e) {
      toast.error("Couldn't reset", { description: (e as Error).message });
    } finally {
      setBusy(false);
    }
  };
  // a custom chapter has no platform version to reset to: it is deleted (every language of it)
  const remove = async () => {
    if (!(await ask({ title: "Delete this chapter?", text: "It is removed for your brand in every language, together with its quiz. Learners no longer see it. This can't be undone.", confirm: "Delete", tone: "danger" }))) return;
    setBusy(true);
    try {
      await cms(`nodes/${sel.kind}/${sel.slug}?lang=${lang}`, { method: "DELETE" });
      toast.success("Chapter deleted");
      onDeleted();
    } catch (e) {
      toast.error("Couldn't delete", { description: (e as Error).message });
      setBusy(false);
    }
  };

  const title = sel.kind === "exam" ? "Final exam" : d.title || "Untitled";
  return (
    <Card className="overflow-hidden" data-testid="cms-editor">
      {confirmDialog}
      <div className="flex flex-wrap items-start justify-between gap-3 border-b border-line px-6 py-5">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <Chip size="sm">{sel.kind}</Chip>
            <Chip size="sm" tone={node.published ? "up" : "neutral"} dot>
              {node.published ? "Published" : "Unpublished"}
            </Chip>
            {SOURCE_CHIP[node.source] && (
              <Chip size="sm" tone={SOURCE_CHIP[node.source]!.tone}>
                {node.source === "override" ? "Edited for this tenant" : "Custom (this tenant only)"}
              </Chip>
            )}
            {dirty && (
              <Chip size="sm" tone="warn">
                Unsaved changes
              </Chip>
            )}
          </div>
          <h2 className="mt-2 truncate text-[19px] font-medium tracking-tight">{title}</h2>
          <div className="k-num mt-0.5 text-[12px] text-fg-3">
            {node.slug} · updated {fmtWhen(node.updated_at)} by {node.updated_by}
            {sel.kind === "chapter" && ` · ${node.data.words ?? 0} words · ${node.data.minutes ?? 0} min`}
          </div>
        </div>
        {canWrite && (
          <div className="flex flex-wrap items-center gap-2">
            {node.source === "override" && (
              <Button size="sm" variant="ghost" onClick={reset} disabled={busy}>
                <RotateCcw /> Reset to default
              </Button>
            )}
            {node.source === "custom" && (
              <Button size="sm" variant="ghost" onClick={remove} disabled={busy} data-testid="cms-delete">
                <Trash2 /> Delete
              </Button>
            )}
            <label className="flex items-center gap-2 rounded-full border border-line bg-surface-2 py-1 pl-3 pr-1.5 text-[12.5px] text-fg-2">
              Published
              <Toggle checked={node.published} onChange={() => togglePublish()} label="Published" />
            </label>
            <Button size="sm" variant="ember" onClick={save} disabled={busy || !dirty} data-testid="cms-save">
              <Save /> {busy ? "Saving…" : node.published ? "Save live" : "Save"}
            </Button>
          </div>
        )}
      </div>

      {(issues.length > 0 || warnings.length > 0) && (
        <div className="space-y-1 border-b border-line px-6 py-3 text-[12.5px]" data-testid="cms-issues">
          {issues.map((x, i) => (
            <div key={`e${i}`} className="text-down">
              {x}
            </div>
          ))}
          {warnings.map((x, i) => (
            <div key={`w${i}`} className="text-warn">
              {x}
            </div>
          ))}
        </div>
      )}

      <div className="space-y-5 px-6 py-5">
        {sel.kind !== "exam" && sel.kind !== "term" && (
          <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
            <label className="block">
              <span className="mb-1.5 block text-[12.5px] font-medium text-fg-2">Title</span>
              <input value={d.title ?? ""} disabled={ro} onChange={(e) => set({ title: e.target.value })} className={inputCls} data-testid="cms-title" />
            </label>
            {sel.kind === "phase" ? (
              <label className="block">
                <span className="mb-1.5 block text-[12.5px] font-medium text-fg-2">Level</span>
                <select value={d.level ?? "Beginner"} disabled={ro} onChange={(e) => set({ level: e.target.value })} className={inputCls}>
                  {LEVELS.map((l) => (
                    <option key={l}>{l}</option>
                  ))}
                </select>
              </label>
            ) : sel.kind === "chapter" ? (
              <label className="block">
                <span className="mb-1.5 block text-[12.5px] font-medium text-fg-2">Practise in Kalks Trader · symbol</span>
                <input value={d.practice?.symbol ?? ""} disabled={ro} placeholder="e.g. EURUSD" onChange={(e) => set({ practice: { label: d.practice?.label ?? "", symbol: e.target.value || null } })} className={inputCls} />
              </label>
            ) : (
              <div />
            )}
            <label className="block md:col-span-2">
              <span className="mb-1.5 block text-[12.5px] font-medium text-fg-2">Summary</span>
              <textarea value={d.summary ?? ""} disabled={ro} rows={2} onChange={(e) => set({ summary: e.target.value })} className={inputCls} />
            </label>
          </div>
        )}

        {sel.kind === "chapter" && (
          <>
            <div>
              <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
                <span className="text-[12.5px] font-medium text-fg-2">Body (markdown)</span>
                <Segmented
                  size="xs"
                  value={view}
                  onChange={setView}
                  options={[
                    { value: "write", label: <><FileText className="size-3" /> Write</> },
                    { value: "split", label: "Split" },
                    { value: "preview", label: <><Eye className="size-3" /> Preview</> },
                  ]}
                />
              </div>
              <div className={cn("grid gap-3", view === "split" && "xl:grid-cols-2")}>
                {view !== "preview" && (
                  <textarea
                    value={d.body ?? ""}
                    disabled={ro}
                    onChange={(e) => set({ body: e.target.value })}
                    spellCheck
                    className={cn(inputCls, "h-[560px] resize-y font-mono text-[12.5px] leading-relaxed")}
                    data-testid="cms-body"
                  />
                )}
                {view !== "write" && (
                  <div className="h-[560px] overflow-y-auto rounded-[12px] border border-line bg-surface px-5 py-4" data-testid="cms-preview">
                    <Markdown src={d.body ?? ""} className="text-[14px]" />
                  </div>
                )}
              </div>
              <p className="mt-1.5 text-[11.5px] text-fg-3">Supported: ## and ### headings, lists, tables, **bold**, `code`, callouts (&gt; **Risk warning:** …), ```text blocks and ```svg diagrams. No raw HTML.</p>
            </div>
            <label className="block">
              <span className="mb-1.5 block text-[12.5px] font-medium text-fg-2">Key takeaways (one per line)</span>
              <textarea value={(d.takeaways ?? []).join("\n")} disabled={ro} rows={4} onChange={(e) => set({ takeaways: e.target.value.split("\n").filter((x, i, a) => x.trim() || i === a.length - 1) })} className={inputCls} />
            </label>
            <label className="block">
              <span className="mb-1.5 block text-[12.5px] font-medium text-fg-2">Practice exercise</span>
              <input value={d.practice?.label ?? ""} disabled={ro} placeholder="What to try on a demo account (leave empty for none)" onChange={(e) => set({ practice: e.target.value ? { label: e.target.value, symbol: d.practice?.symbol ?? null } : null })} className={inputCls} />
            </label>
            <div>
              <div className="mb-2 text-[12.5px] font-medium text-fg-2">Chapter quiz · 3–5 questions, learners need 60% to complete the chapter</div>
              <QuizEditor value={d.quiz ?? []} onChange={(quiz) => set({ quiz })} readOnly={ro} min={0} max={5} testId="cms-quiz" />
            </div>
          </>
        )}

        {sel.kind === "exam" && (
          <>
            <label className="block max-w-[200px]">
              <span className="mb-1.5 block text-[12.5px] font-medium text-fg-2">Pass mark (%)</span>
              <input type="number" min={50} max={100} value={d.pass_mark ?? 70} disabled={ro} onChange={(e) => set({ pass_mark: Number(e.target.value) })} className={inputCls} />
            </label>
            <QuizEditor value={(d.questions ?? []) as QuizQ[]} onChange={(questions) => set({ questions })} readOnly={ro} min={1} max={60} testId="cms-exam" />
          </>
        )}
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Stats                                                               */
/* ------------------------------------------------------------------ */

function LearnerStats({ stats, audit }: { stats: Stats; audit: AuditRow[] }) {
  return (
    <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
      <Reveal className="xl:col-span-7">
        <Card className="h-full">
          <CardHeader title="Learners by phase" subtitle="Chapter completions, exam results and certificates for this tenant" icon={<Users />} />
          <div className="overflow-x-auto px-3 pb-4 pt-3 sm:px-5">
            <table className="w-full min-w-[560px] text-[12.5px]" data-testid="cms-stats">
              <thead>
                <tr className="text-left text-[11px] uppercase tracking-wider text-fg-3">
                  <th className="px-2 py-2 font-medium">Phase</th>
                  <th className="px-2 py-2 text-right font-medium">Learners</th>
                  <th className="px-2 py-2 font-medium">Chapter completions</th>
                  <th className="px-2 py-2 text-right font-medium">Exam pass</th>
                  <th className="px-2 py-2 text-right font-medium">Certificates</th>
                </tr>
              </thead>
              <tbody>
                {stats.phases.map((p) => {
                  const possible = p.learners * p.chapters;
                  const rate = possible ? Math.round((p.completions / possible) * 100) : 0;
                  return (
                    <tr key={p.slug} className="border-t border-line">
                      <td className="px-2 py-2.5">
                        <div className="font-medium">
                          {p.order}. {p.title}
                        </div>
                        <div className="text-[11px] text-fg-3">
                          {p.level} · {p.chapters} chapters
                        </div>
                      </td>
                      <td className="k-num px-2 py-2.5 text-right">{p.learners}</td>
                      <td className="px-2 py-2.5">
                        <div className="flex items-center gap-2">
                          <Progress value={rate} className="max-w-[120px]" tone={rate >= 50 ? "up" : "ember"} />
                          <span className="k-num text-fg-3">{p.completions}</span>
                        </div>
                      </td>
                      <td className="k-num px-2 py-2.5 text-right">{p.exam_attempts ? `${p.exam_passed}/${p.exam_attempts}` : "–"}</td>
                      <td className="k-num px-2 py-2.5 text-right">{p.certificates}</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </Card>
      </Reveal>
      <div className="flex flex-col gap-4 xl:col-span-5">
        <Reveal delay={0.05}>
          <Card>
            <CardHeader title="Most studied chapters" icon={<BookOpen />} />
            <div className="space-y-1.5 px-4 pb-5 pt-3 sm:px-6">
              {stats.top_chapters.length === 0 && <p className="text-[12.5px] text-fg-3">No learner activity yet.</p>}
              {stats.top_chapters.map((c) => (
                <div key={c.slug} className="k-row flex items-center gap-3 px-3 py-2">
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-[12.5px] font-medium">{c.title}</div>
                    <div className="truncate text-[11px] text-fg-3">{c.phase}</div>
                  </div>
                  <div className="k-num text-right text-[11.5px] text-fg-2">
                    {c.completed}/{c.learners} done
                    <div className="text-fg-3">quiz {c.quiz_avg ?? "–"}%</div>
                  </div>
                </div>
              ))}
            </div>
          </Card>
        </Reveal>
        <Reveal delay={0.1}>
          <Card>
            <CardHeader title="Edit history" subtitle="Last changes by your team" icon={<History />} />
            <div className="max-h-[260px] space-y-1 overflow-y-auto px-4 pb-5 pt-3 sm:px-6" data-testid="cms-audit">
              {audit.length === 0 && <p className="text-[12.5px] text-fg-3">No edits yet. The course runs on the platform version.</p>}
              {audit.map((a, i) => (
                <div key={i} className="flex items-baseline gap-2 border-b border-line py-1.5 text-[12px] last:border-0">
                  <span className="k-num shrink-0 text-fg-3">{fmtWhen(a.at)}</span>
                  <span className="min-w-0 truncate text-fg-2">
                    <span className="text-fg">{a.staff}</span> {a.action} {a.kind} <span className="text-fg">{a.detail?.title ?? a.slug}</span>
                  </span>
                </div>
              ))}
            </div>
          </Card>
        </Reveal>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Page                                                                */
/* ------------------------------------------------------------------ */

/** "9 phases (1 elective) · fundamental, technical and options tracks · EN" */
function courseSummary(t: Tree) {
  const electives = t.phases.filter((p) => p.elective).length;
  const rank = (k: string) => {
    const i = TRACK_ORDER.indexOf(k as Track);
    return i < 0 ? TRACK_ORDER.length : i;
  };
  const tracks = [...new Set(t.phases.flatMap((p) => p.sections.map((s) => s.track)))].filter(Boolean).sort((a, b) => rank(a) - rank(b));
  const list = tracks.length > 1 ? `${tracks.slice(0, -1).join(", ")} and ${tracks.at(-1)}` : (tracks[0] ?? "no");
  return `${t.phases.length} phases${electives ? ` (${electives} elective)` : ""} · ${list} ${tracks.length === 1 ? "track" : "tracks"} · ${t.lang.toUpperCase()}`;
}

function NewChapter({ tree, open, onOpenChange, onCreated, defaultSection }: { tree: Tree; open: boolean; onOpenChange: (o: boolean) => void; onCreated: (slug: string) => void; defaultSection?: string }) {
  const sections = tree.phases.flatMap((p) => p.sections.map((s) => ({ slug: s.slug, label: `Phase ${p.order} · ${s.track} · ${s.title}` })));
  const [section, setSection] = React.useState(defaultSection ?? sections[0]?.slug ?? "");
  const [title, setTitle] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  React.useEffect(() => {
    if (open) {
      setSection(defaultSection ?? sections[0]?.slug ?? "");
      setTitle("");
    }
  }, [open]); // eslint-disable-line react-hooks/exhaustive-deps
  const create = async () => {
    setBusy(true);
    try {
      const r = await cms<{ slug: string }>("chapters", { body: { lang: tree.lang, section, title } });
      toast.success("Draft chapter created", { description: "It stays unpublished until you publish it." });
      onOpenChange(false);
      onCreated(r.slug);
    } catch (e) {
      toast.error("Couldn't create the chapter", { description: (e as Error).message });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title="New chapter"
      description="Adds a draft chapter for your tenant. Write the body and quiz, then publish it."
      footer={
        <>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button variant="ember" onClick={create} disabled={busy || !title.trim() || !section}>
            <Plus /> Create draft
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <label className="block">
          <span className="mb-1.5 block text-[12.5px] font-medium text-fg-2">Section</span>
          <select value={section} onChange={(e) => setSection(e.target.value)} className={inputCls}>
            {sections.map((s) => (
              <option key={s.slug} value={s.slug}>
                {s.label}
              </option>
            ))}
          </select>
        </label>
        <label className="block">
          <span className="mb-1.5 block text-[12.5px] font-medium text-fg-2">Title</span>
          <input value={title} onChange={(e) => setTitle(e.target.value)} placeholder="e.g. Trading our house account types" className={inputCls} data-testid="new-chapter-title" />
        </label>
      </div>
    </Dialog>
  );
}

export function LiveAcademyCms() {
  const staff = useStaff();
  const canRead = contentAllows(staff, "content.read");
  const canWrite = contentAllows(staff, "content.write");
  const [lang, setLang] = React.useState("en");
  const tree = useCms<Tree>(canRead ? `tree?lang=${lang}` : null);
  const stats = useCms<Stats>(canRead ? `stats?lang=${lang}` : null);
  const audit = useCms<{ audit: AuditRow[] }>(canRead ? "audit" : null);
  const [sel, setSel] = React.useState<Sel | null>(null);
  const [creating, setCreating] = React.useState(false);

  React.useEffect(() => {
    const first = tree.data?.phases[0]?.sections[0]?.chapters[0];
    if (!sel && first) setSel({ kind: "chapter", slug: first.slug });
  }, [tree.data, sel]);

  if (!canRead) return <EmptyState illustration="identification_card" title="No access" text="Your role doesn't include Academy content." />;
  if (tree.error) return <EmptyState illustration="satellite_antenna" title="The Academy service is unavailable" text={tree.error.message} action={<Button variant="surface" onClick={tree.reload}><RotateCw /> Try again</Button>} />;
  const t = tree.data;
  const all = t?.phases.flatMap((p) => p.sections.flatMap((s) => s.chapters)) ?? [];
  const published = all.filter((c) => c.published).length;
  const words = all.reduce((n, c) => n + c.words, 0);
  const s = stats.data;
  const selSection = sel?.kind === "section" ? sel.slug : sel?.kind === "chapter" ? t?.phases.flatMap((p) => p.sections).find((x) => x.chapters.some((c) => c.slug === sel.slug))?.slug : undefined;
  const refresh = () => {
    tree.reload();
    stats.reload();
    audit.reload();
  };

  return (
    <div className="pb-16">
      <PageHeader
        title="Academy"
        subtitle="Phases, chapters, quizzes and exams shown in the Client Area Academy. Edits apply to your brand only; the platform version stays available to reset to."
        actions={
          <>
            {t && t.languages.length > 1 && (
              <select value={lang} onChange={(e) => setLang(e.target.value)} className={cn(inputCls, "h-10 w-auto")} aria-label="Language">
                {t.languages.map((l) => (
                  <option key={l}>{l}</option>
                ))}
              </select>
            )}
            {canWrite && t && (
              <Button variant="ember" onClick={() => setCreating(true)} data-testid="cms-new-chapter">
                <Plus /> New chapter
              </Button>
            )}
          </>
        }
      />

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <KpiCard label="Chapters" icon={<BookOpen />} value={<span className="k-num">{t ? all.length : "–"}</span>} chip={t ? `${published} published · ${Math.round(words / 1000)}k words` : "Loading"} />
        <KpiCard label="Learners" icon={<Users />} value={<span className="k-num">{s?.learners ?? "–"}</span>} chip={s ? `${s.active_7d} active in 7 days` : "Loading"} chipTone="up" delay={0.05} />
        <KpiCard label="Quiz average" icon={<GraduationCap />} value={<span className="k-num">{s?.quiz_avg != null ? `${s.quiz_avg}%` : "–"}</span>} chip={s ? `${s.completions_30d} chapters completed in 30 days` : "Loading"} chipTone="gold" delay={0.1} />
        <KpiCard label="Certificates" icon={<Award />} value={<span className="k-num">{s?.certificates ?? "–"}</span>} chip={s ? `Exams passed ${s.exam_passed}/${s.exam_attempts}` : "Loading"} delay={0.15} />
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal className="xl:col-span-4">
          <Card className="h-full">
            <CardHeader title="Course" subtitle={t ? courseSummary(t) : "Loading"} />
            <div className="max-h-[1120px] overflow-y-auto px-3 pb-4 pt-3 sm:px-4">
              {t ? <CourseTree tree={t} sel={sel} onSelect={setSel} canWrite={canWrite} onReordered={refresh} /> : <Skeleton className="h-[480px] w-full rounded-[16px]" />}
            </div>
            {!canWrite && <p className="px-6 pb-5 text-[12px] text-fg-3">Read-only: your role can view the Academy but not edit it.</p>}
          </Card>
        </Reveal>
        <Reveal delay={0.05} className="min-w-0 xl:col-span-8">
          {sel ? <Editor
              key={`${sel.kind}:${sel.slug}:${lang}`}
              sel={sel}
              lang={lang}
              canWrite={canWrite}
              onSaved={refresh}
              onDeleted={() => {
                setSel(null);
                refresh();
              }}
            /> : <Skeleton className="h-[520px] w-full rounded-[18px]" />}
        </Reveal>
      </div>

      {s && <LearnerStats stats={s} audit={audit.data?.audit ?? []} />}

      {t && <NewChapter tree={t} open={creating} onOpenChange={setCreating} defaultSection={selSection} onCreated={(slug) => { refresh(); setSel({ kind: "chapter", slug }); }} />}
    </div>
  );
}
