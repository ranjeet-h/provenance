import * as React from "react";
import { useQuery } from "@tanstack/react-query";
import { Play } from "lucide-react";
import { EmptyState } from "@/components/common/EmptyState";
import { LoadingState } from "@/components/common/LoadingState";
import { ProgressIndicator } from "@/components/common/ProgressIndicator";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import {
  analyzeSession,
  cellCoverage,
  cellCoverageByKind,
  exclusionLabel,
  type ExactAnalysis,
  type PairAnalysis,
} from "@/lib/analysis";
import { asSessionsError, listSubmissions, type Student } from "@/lib/sessions";
import { HighlightedText, PassageCard } from "./PassageCard";
import { cn } from "@/lib/utils";

function formatPct(value: number): string {
  return `${Math.round(value)}%`;
}

function formatOptionalPct(value: number | null | undefined): string {
  return value === null || value === undefined ? "not assessable" : formatPct(value);
}

export function AnalysisSection({
  sessionId,
  students,
  submissionCount,
}: {
  sessionId: string;
  students: Student[];
  submissionCount: number;
}) {
  const [report, setReport] = React.useState<ExactAnalysis | null>(null);
  const [analyzing, setAnalyzing] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [selected, setSelected] = React.useState<string | null>(null);

  const submissionsQuery = useQuery({
    queryKey: ["submissions", sessionId],
    queryFn: () => listSubmissions(sessionId),
    enabled: report !== null,
  });
  const textByStudent = React.useMemo(
    () => new Map((submissionsQuery.data ?? []).map((s) => [s.student_id, s.original_text])),
    [submissionsQuery.data],
  );
  const nameById = React.useMemo(
    () => new Map(students.map((s) => [s.id, s.display_name])),
    [students],
  );

  async function onAnalyze(): Promise<void> {
    setAnalyzing(true);
    setError(null);
    try {
      const result = await analyzeSession(sessionId);
      setReport(result);
      setSelected(null);
    } catch (err) {
      setError(asSessionsError(err).message);
    } finally {
      setAnalyzing(false);
    }
  }

  function pairKey(pair: PairAnalysis): string {
    return `${pair.a_student_id}::${pair.b_student_id}`;
  }

  const selectedPair = report?.pairs.find((p) => pairKey(p) === selected) ?? null;

  return (
    <section aria-label="Plagiarism analysis" className="mt-6">
      <h2 className="text-lg font-medium">Analysis</h2>
      <p className="mt-1 text-sm text-muted-foreground">
        Exact and modified-copy detection in this phase. Matching content is reported —
        never an accusation of who copied from whom.
      </p>

      {submissionCount < 2 ? (
        <div className="mt-3">
          <EmptyState
            title="Not enough submissions to analyze"
            description="Add submissions for at least two students, then run the analysis."
          />
        </div>
      ) : report === null ? (
        <div className="mt-3">
          <Button onClick={() => void onAnalyze()} disabled={analyzing}>
            <Play className="h-4 w-4" aria-hidden />
            {analyzing ? "Analyzing…" : "Analyze session"}
          </Button>
          {error ? (
            <p role="alert" className="mt-2 text-sm text-destructive">{error}</p>
          ) : null}
        </div>
      ) : (
        <div className="mt-3 space-y-6">
          <div className="flex items-center gap-2">
            <Button variant="outline" size="sm" onClick={() => void onAnalyze()} disabled={analyzing}>
              {analyzing ? "Analyzing…" : "Re-run analysis"}
            </Button>
            {error ? (
              <p role="alert" className="text-sm text-destructive">{error}</p>
            ) : null}
          </div>

          <div>
            <h3 className="text-sm font-medium">Pairwise overlap</h3>
            <div className="mt-2 overflow-x-auto rounded-lg border">
              <table className="w-full min-w-[320px] text-sm">
                <thead>
                  <tr className="border-b bg-muted/50">
                    <th className="p-2 text-left font-medium" scope="col">
                      <span className="sr-only">Student</span>
                    </th>
                    {students.map((s) => (
                      <th key={s.id} className="p-2 text-left font-medium" scope="col">
                        {s.display_name}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {students.map((row) => (
                    <tr key={row.id} className="border-b last:border-0">
                      <th className="p-2 text-left font-medium" scope="row">
                        {row.display_name}
                      </th>
                      {students.map((col) => {
                        if (row.id === col.id) {
                          return (
                            <td key={col.id} className="p-2 text-muted-foreground" aria-label={`${row.display_name} vs self`}>
                              —
                            </td>
                          );
                        }
                        const pair = report.pairs.find(
                          (p) =>
                            (p.a_student_id === row.id && p.b_student_id === col.id) ||
                            (p.a_student_id === col.id && p.b_student_id === row.id),
                        );
                        const value = pair ? cellCoverage(pair, row.id) : 0;
                        const label =
                          value === null || value === undefined
                            ? "not assessable"
                            : `${formatPct(value)} overlap`;
                        const hasPassages = (pair?.passages.length ?? 0) > 0;
                        return (
                          <td key={col.id} className="p-2">
                            <button
                              type="button"
                              aria-label={`${row.display_name} vs ${col.display_name}: ${label}`}
                              onClick={() => pair && setSelected(pairKey(pair))}
                              className={cn(
                                "rounded px-2 py-1 font-mono text-xs focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                hasPassages
                                  ? "bg-destructive/10 font-semibold text-destructive hover:bg-destructive/20"
                                  : "text-muted-foreground hover:bg-accent",
                              )}
                            >
                              {value === null || value === undefined ? "—" : formatPct(value)}
                            </button>
                          </td>
                        );
                      })}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>

          <div>
            <h3 className="text-sm font-medium">Per-student overlap</h3>
            <ul className="mt-2 space-y-3">
              {report.per_student.map((row) => (
                <li key={row.student_id}>
                  <div className="flex items-center justify-between text-sm">
                    <span className="font-medium">
                      {nameById.get(row.student_id) ?? "Unknown student"}
                    </span>
                    {row.coverage === null || row.coverage === undefined ? (
                      <span className="font-mono text-xs text-muted-foreground">
                        Not assessable · insufficient assessable text
                      </span>
                    ) : (
                      <span className="font-mono text-xs text-muted-foreground">
                        {formatPct(row.coverage)} · {row.matched_tokens}/{row.eligible_tokens} eligible words
                      </span>
                    )}
                  </div>
                  {row.coverage === null || row.coverage === undefined ? null : (
                    <p className="text-xs text-muted-foreground">
                      Exact {formatOptionalPct(row.exact_coverage)} · Modified {formatOptionalPct(row.modified_coverage)}
                      <span className="ml-1">(breakdown only; do not add)</span>
                    </p>
                  )}
                  {row.coverage === null || row.coverage === undefined ? null : (
                    <ProgressIndicator value={row.coverage} label={`Overlap for ${nameById.get(row.student_id) ?? "student"}`} />
                  )}
                </li>
              ))}
            </ul>
          </div>

          {selectedPair ? (
            <div aria-label="Pair detail">
              <Separator className="my-4" />
              <h3 className="text-sm font-medium">
                {nameById.get(selectedPair.a_student_id)} vs{" "}
                {nameById.get(selectedPair.b_student_id)} —{" "}
                {selectedPair.coverage_a === null || selectedPair.coverage_a === undefined
                  ? "not assessable"
                  : formatPct(selectedPair.coverage_a)}{" "}
                /{" "}
                {selectedPair.coverage_b === null || selectedPair.coverage_b === undefined
                  ? "not assessable"
                  : formatPct(selectedPair.coverage_b)}
              </h3>
              <p className="mt-1 text-xs text-muted-foreground">
                {nameById.get(selectedPair.a_student_id)}: exact {formatOptionalPct(cellCoverageByKind(selectedPair, selectedPair.a_student_id, "exact"))}, modified {formatOptionalPct(cellCoverageByKind(selectedPair, selectedPair.a_student_id, "modified"))} ·{" "}
                {nameById.get(selectedPair.b_student_id)}: exact {formatOptionalPct(cellCoverageByKind(selectedPair, selectedPair.b_student_id, "exact"))}, modified {formatOptionalPct(cellCoverageByKind(selectedPair, selectedPair.b_student_id, "modified"))}. Type values are subsets; combined coverage uses unique spans.
              </p>
              {selectedPair.passages.length === 0 ? (
                <p className="mt-2 text-sm text-muted-foreground">
                  No matching passages detected between these two submissions.
                </p>
              ) : (
                <ol className="mt-3 space-y-4">
                  {selectedPair.passages.map((passage, i) => (
                    <PassageCard
                      key={`${passage.a_token_start}-${passage.b_token_start}-${i}`}
                      index={i}
                      passage={passage}
                      aName={nameById.get(selectedPair.a_student_id) ?? "Student A"}
                      bName={nameById.get(selectedPair.b_student_id) ?? "Student B"}
                      aText={textByStudent.get(selectedPair.a_student_id) ?? ""}
                      bText={textByStudent.get(selectedPair.b_student_id) ?? ""}
                    />
                  ))}
                </ol>
              )}
              {selectedPair.excluded.length > 0 ? (
                <div className="mt-4">
                  <h4 className="text-sm font-medium">Excluded from scoring</h4>
                  <p className="mt-1 text-xs text-muted-foreground">
                    Matching assignment material that does not count toward the
                    score. Shared session text stays counted and is flagged on
                    the passage itself.
                  </p>
                  <ul aria-label="Excluded passages" className="mt-2 space-y-2">
                    {selectedPair.excluded.map((item, i) => {
                      const studentId =
                        item.side === "a" ? selectedPair.a_student_id : selectedPair.b_student_id;
                      return (
                        <li key={i} className="rounded-lg border border-dashed p-3">
                          <p className="text-xs font-medium text-muted-foreground">
                            {exclusionLabel(item.reason)} · {nameById.get(studentId)} ·{" "}
                            {item.tokens} words not counted
                          </p>
                          <div className="mt-1">
                            <HighlightedText
                              text={textByStudent.get(studentId) ?? ""}
                              start={item.char_start}
                              end={item.char_end}
                            />
                          </div>
                        </li>
                      );
                    })}
                  </ul>
                </div>
              ) : null}
            </div>
          ) : null}
        </div>
      )}
      {report !== null && submissionsQuery.isPending ? (
        <LoadingState label="Loading submission texts…" />
      ) : null}
    </section>
  );
}
