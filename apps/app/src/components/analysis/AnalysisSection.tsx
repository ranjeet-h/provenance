import * as React from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { EmptyState } from "@/components/common/EmptyState";
import { LoadingState } from "@/components/common/LoadingState";
import { ProgressIndicator } from "@/components/common/ProgressIndicator";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import {
  analyzeSession,
  cellCoverage,
  cellCoverageByKind,
  generateCertifiedStudentReport,
  generateStudentReportPdf,
  verifyCertifiedStudentReport,
  type HistoricalPairAnalysis,
  exclusionLabel,
  loadSessionAnalysis,
  type AnalysisProgress,
  type PairAnalysis,
} from "@/lib/analysis";
import {
  asSessionsError,
  getSessionLock,
  listSubmissions,
  lockSession,
  type Student,
} from "@/lib/sessions";
import { selectedReferenceLibraryIds } from "@/lib/referenceLibraries";
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
  sessionStatus,
}: {
  sessionId: string;
  students: Student[];
  submissionCount: number;
  sessionStatus: "draft" | "locked" | "analyzed";
}) {
  const [analyzing, setAnalyzing] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [selected, setSelected] = React.useState<string | null>(null);
  const [progress, setProgress] = React.useState<AnalysisProgress | null>(null);
  const [anonymizeReports, setAnonymizeReports] = React.useState(false);
  const [reportingStudentId, setReportingStudentId] = React.useState<string | null>(null);
  const [reportError, setReportError] = React.useState<string | null>(null);
  const [reportNotice, setReportNotice] = React.useState<string | null>(null);
  const [selfCheckStudentId, setSelfCheckStudentId] = React.useState<string | null>(null);
  const [selfCheckError, setSelfCheckError] = React.useState<string | null>(null);
  const [selfCheckNotice, setSelfCheckNotice] = React.useState<string | null>(null);
  const [locking, setLocking] = React.useState(false);
  const [lockError, setLockError] = React.useState<string | null>(null);
  const [verificationFile, setVerificationFile] = React.useState<File | null>(null);
  const [verifying, setVerifying] = React.useState(false);
  const [verificationError, setVerificationError] = React.useState<string | null>(null);
  const [verificationNote, setVerificationNote] = React.useState<string | null>(null);
  const queryClient = useQueryClient();
  const analysisQuery = useQuery({
    queryKey: ["analysis", sessionId],
    queryFn: () => loadSessionAnalysis(sessionId),
    retry: false,
  });
  const report = analysisQuery.data ?? null;
  const loadingSaved = analysisQuery.isPending;
  const lockQuery = useQuery({
    queryKey: ["session-lock", sessionId],
    queryFn: () => getSessionLock(sessionId),
    enabled: sessionStatus === "locked",
    retry: false,
  });
  const selectedLibrariesQuery = useQuery({
    queryKey: ["selected-reference-library-ids", sessionId],
    queryFn: () => selectedReferenceLibraryIds(sessionId),
    retry: false,
  });
  const selectedLibraryIds = selectedLibrariesQuery.data ?? [];
  const canAnalyze = submissionCount >= 2 || (submissionCount >= 1 && selectedLibraryIds.length > 0);
  const displayError = error ?? (analysisQuery.isError
    ? asSessionsError(analysisQuery.error).message
    : null);

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
    setProgress(null);
    try {
      const result = await analyzeSession(sessionId, setProgress);
      queryClient.setQueryData(["analysis", sessionId], result);
      setSelected(null);
    } catch (err) {
      setError(asSessionsError(err).message);
    } finally {
      setAnalyzing(false);
    }
  }

  async function onGenerateReport(studentId: string): Promise<void> {
    setReportingStudentId(studentId);
    setReportError(null);
    setReportNotice(null);
    let objectUrl: string | null = null;
    try {
      const exported = await generateCertifiedStudentReport(
        sessionId,
        studentId,
        anonymizeReports,
      );
      const pdf = new Blob([Uint8Array.from(exported.pdfBytes)], { type: "application/pdf" });
      objectUrl = URL.createObjectURL(pdf);
      const link = document.createElement("a");
      link.href = objectUrl;
      link.download = `provenance-${studentId}-certified-report.pdf`;
      document.body.appendChild(link);
      link.click();
      link.remove();
      const downloadedUrl = objectUrl;
      window.setTimeout(() => URL.revokeObjectURL(downloadedUrl), 0);
      const jsonBlob = new Blob([exported.signedReportJson], { type: "application/json" });
      const jsonUrl = URL.createObjectURL(jsonBlob);
      const jsonLink = document.createElement("a");
      jsonLink.href = jsonUrl;
      jsonLink.download = `provenance-${studentId}-certified-report.json`;
      document.body.appendChild(jsonLink);
      jsonLink.click();
      jsonLink.remove();
      window.setTimeout(() => URL.revokeObjectURL(jsonUrl), 0);
      setReportNotice("Signed PDF and verification JSON were generated locally.");
    } catch (cause) {
      if (objectUrl !== null) URL.revokeObjectURL(objectUrl);
      setReportError(asSessionsError(cause).message);
    } finally {
      setReportingStudentId(null);
    }
  }

  async function onGenerateSelfCheck(studentId: string): Promise<void> {
    setSelfCheckStudentId(studentId);
    setSelfCheckError(null);
    setSelfCheckNotice(null);
    let objectUrl: string | null = null;
    try {
      const bytes = await generateStudentReportPdf(
        sessionId,
        studentId,
        anonymizeReports,
        "self_check",
      );
      objectUrl = URL.createObjectURL(
        new Blob([Uint8Array.from(bytes)], { type: "application/pdf" }),
      );
      const link = document.createElement("a");
      link.href = objectUrl;
      link.download = `provenance-${studentId}-self-check.pdf`;
      document.body.appendChild(link);
      link.click();
      link.remove();
      const downloadedUrl = objectUrl;
      window.setTimeout(() => URL.revokeObjectURL(downloadedUrl), 0);
      setSelfCheckNotice("Self Check PDF downloaded. It is not teacher certified.");
    } catch (cause) {
      if (objectUrl !== null) URL.revokeObjectURL(objectUrl);
      setSelfCheckError(asSessionsError(cause).message);
    } finally {
      setSelfCheckStudentId(null);
    }
  }

  async function onLockSession(): Promise<void> {
    setLocking(true);
    setLockError(null);
    try {
      await lockSession(sessionId);
      await queryClient.invalidateQueries({ queryKey: ["session", sessionId] });
      await queryClient.invalidateQueries({ queryKey: ["sessions"] });
      await queryClient.invalidateQueries({ queryKey: ["session-lock", sessionId] });
    } catch (cause) {
      setLockError(asSessionsError(cause).message);
    } finally {
      setLocking(false);
    }
  }

  async function onVerifyReport(): Promise<void> {
    if (!verificationFile) return;
    setVerifying(true);
    setVerificationError(null);
    setVerificationNote(null);
    try {
      if (verificationFile.size > 20_000_000) {
        throw { code: "validation", message: "Choose a signed report JSON file under 20 MB." };
      }
      const result = await verifyCertifiedStudentReport(await verificationFile.text());
      setVerificationNote(`Signature verified · key ${result.signing_key_id}. ${result.note}`);
    } catch (cause) {
      setVerificationError(asSessionsError(cause).message);
    } finally {
      setVerifying(false);
    }
  }

  function pairKey(pair: PairAnalysis): string {
    return `${pair.a_student_id}::${pair.b_student_id}`;
  }

  const selectedPair = report?.pairs.find((p) => pairKey(p) === selected) ?? null;

  return (
    <section aria-label="Plagiarism analysis" className="mt-7 rounded-2xl border border-border/80 bg-card p-4 shadow-sm shadow-slate-900/[0.025] sm:p-6">
      <p className="text-[10px] font-semibold uppercase tracking-[0.15em] text-primary">Evidence review</p>
      <h2 className="mt-1 text-xl font-semibold tracking-tight">Analysis</h2>
      <p className="mt-1 max-w-3xl text-sm leading-relaxed text-muted-foreground">
        Exact and modified-copy detection in this phase. Matching content is reported —
        never an accusation of who copied from whom.
      </p>

      {report !== null && sessionStatus !== "locked" ? (
        <section aria-labelledby="session-certification-title" className="mt-5 max-w-3xl rounded-2xl border border-border/80 bg-muted/30 p-4 sm:p-5">
          <p className="text-xs font-medium uppercase tracking-[0.12em] text-muted-foreground">Session certification · Step 1 of 2</p>
          <h3 id="session-certification-title" className="mt-1 text-base font-semibold tracking-tight">Freeze the reviewed evidence</h3>
          <p className="mt-1 max-w-2xl text-sm leading-relaxed text-muted-foreground">
            Locking signs the current inputs and comparison corpus, then prevents edits, re-analysis, and library removal. There is no unlock path. The private key stays in the operating-system credential store.
          </p>
          {lockError ? <p role="alert" className="mt-3 text-sm text-destructive">{lockError}</p> : null}
          <Button className="mt-4" onClick={() => void onLockSession()} disabled={locking || analyzing}>
            {locking ? "Signing and locking…" : "Lock session for certification"}
          </Button>
        </section>
      ) : null}

      {sessionStatus === "locked" ? (
        <section aria-labelledby="locked-session-title" className="mt-5 max-w-3xl rounded-2xl border border-emerald-900/15 bg-emerald-50/50 px-4 py-4 sm:px-5">
          <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
            <h3 id="locked-session-title" className="text-base font-semibold tracking-tight">Session locked · inputs frozen</h3>
            {lockQuery.data ? <time className="text-xs text-muted-foreground" dateTime={lockQuery.data.locked_at}>{new Date(lockQuery.data.locked_at).toLocaleString()}</time> : null}
          </div>
          {lockQuery.isPending ? <p className="mt-2 text-sm text-muted-foreground">Verifying the signed lock…</p> : null}
          {lockQuery.isError ? <p role="alert" className="mt-2 text-sm text-destructive">{asSessionsError(lockQuery.error).message}</p> : null}
          {lockQuery.data ? (
            <>
              <p className="mt-2 text-sm text-muted-foreground">Manifest SHA-256</p>
              <code className="mt-1 block break-all font-mono text-xs">{lockQuery.data.manifest_sha256}</code>
              <p className="mt-3 text-sm text-muted-foreground">Signer key ID</p>
              <code className="mt-1 block break-all font-mono text-xs">{lockQuery.data.signing_key_id}</code>
              <p className="mt-3 text-xs leading-relaxed text-muted-foreground">This device key identifies the signing key, not a person. Confirm its fingerprint with the teacher through a trusted channel.</p>
            </>
          ) : null}
        </section>
      ) : null}

      {loadingSaved ? (
        <div className="mt-3"><LoadingState label="Loading saved analysis…" /></div>
      ) : sessionStatus === "locked" && report === null ? (
        <div className="mt-3">
          <EmptyState
            title="Locked session has no readable analysis"
            description="The saved analysis is missing or invalid. The lock cannot be changed here; inspect the local session data before producing a certificate."
          />
        </div>
      ) : submissionCount === 1 && selectedLibrariesQuery.isPending ? (
        <div className="mt-3"><LoadingState label="Checking the selected comparison corpus…" /></div>
      ) : submissionCount === 1 && selectedLibrariesQuery.isError ? (
        <div role="alert" className="mt-3 rounded-xl border border-destructive/30 bg-destructive/5 p-4 text-sm text-destructive">
          Could not check the selected comparison libraries: {asSessionsError(selectedLibrariesQuery.error).message}
        </div>
      ) : !canAnalyze ? (
        <div className="mt-3">
          <EmptyState
            title={submissionCount === 0 ? "Add a submission to begin" : "Choose a comparison source"}
            description={
              submissionCount === 0
                ? "Add student submissions before running a comparison."
                : "With one submission, select a local Reference Library above or add another student's submission. Only the sources you select are compared."
            }
            action={submissionCount > 0 ? (
              <Button
                variant="outline"
                onClick={() => document.getElementById("session-reference-libraries")?.scrollIntoView({ block: "center" })}
              >
                Choose a reference library
              </Button>
            ) : undefined}
          />
        </div>
      ) : report === null ? (
        <div className="mt-3">
          <Button onClick={() => void onAnalyze()} disabled={analyzing}>
            {analyzing ? "Analyzing…" : "Analyze session"}
          </Button>
          {displayError ? (
            <p role="alert" className="mt-2 text-sm text-destructive">{displayError}</p>
          ) : null}
          {analyzing && progress ? <AnalysisProgressPanel progress={progress} /> : null}
        </div>
      ) : (
        <div className="mt-5 space-y-7">
          <div className="flex items-center gap-2">
            {sessionStatus === "locked" ? (
              <p className="text-sm text-muted-foreground">Analysis is frozen with the signed session lock.</p>
            ) : (
              <Button variant="outline" size="sm" onClick={() => void onAnalyze()} disabled={analyzing}>
                {analyzing ? "Analyzing…" : "Re-run analysis"}
              </Button>
            )}
            {displayError ? (
              <p role="alert" className="text-sm text-destructive">{displayError}</p>
            ) : null}
          </div>

          {sessionStatus !== "locked" ? (
            <section aria-labelledby="self-check-title" className="max-w-4xl rounded-2xl border border-primary/15 bg-gradient-to-br from-primary/[0.055] to-card p-4 shadow-sm shadow-primary/[0.035] sm:p-5">
              <div className="flex flex-wrap items-center gap-2">
                <span className="rounded-full bg-primary/10 px-2.5 py-1 text-xs font-semibold tracking-wide text-primary">SELF CHECK</span>
                <span className="text-xs font-medium text-muted-foreground">Not Teacher Certified</span>
              </div>
              <h3 id="self-check-title" className="mt-3 text-base font-semibold tracking-tight">Review against your selected sources</h3>
              <p className="mt-1 max-w-3xl text-sm leading-relaxed text-muted-foreground">
                This report shows matching passages only in the comparison corpus listed here. It is not a universal plagiarism check and does not make a misconduct decision.
              </p>
              <div className="mt-3 flex flex-wrap gap-2" aria-label="Self-check comparison corpus">
                <span className="rounded-md border bg-background/70 px-2.5 py-1.5 text-xs text-muted-foreground">
                  {report.pairs.length > 0 ? `${report.pairs.length} current-session comparisons` : "No other current submissions"}
                </span>
                {report.compared_libraries.map((library) => (
                  <span key={library.id} className="rounded-md border bg-background/70 px-2.5 py-1.5 text-xs text-muted-foreground">
                    {library.name} · {library.source_count} archived {library.source_count === 1 ? "submission" : "submissions"}
                  </span>
                ))}
                {report.compared_libraries.length === 0 && report.pairs.length === 0 ? (
                  <span className="rounded-md border border-dashed px-2.5 py-1.5 text-xs text-muted-foreground">No comparison sources were selected</span>
                ) : null}
              </div>
            </section>
          ) : null}

          {sessionStatus === "locked" && lockQuery.data ? (
            <section aria-labelledby="verify-certificate-title" className="max-w-3xl rounded-2xl border border-border/80 bg-muted/25 p-4 sm:p-5">
              <h3 id="verify-certificate-title" className="text-sm font-semibold">Verify a signed report</h3>
              <p className="mt-1 text-sm leading-relaxed text-muted-foreground">Choose the JSON file exported beside the PDF. Verification checks the report contents, reviewer footer, lock digest, key ID, and Ed25519 signature.</p>
              <div className="mt-3 flex flex-col items-start gap-3 sm:flex-row sm:items-center">
                <input
                  aria-label="Signed report JSON file"
                  type="file"
                  accept=".json,application/json"
                  onChange={(event) => {
                    setVerificationFile(event.currentTarget.files?.[0] ?? null);
                    setVerificationError(null);
                    setVerificationNote(null);
                  }}
                  className="max-w-full rounded-xl border border-dashed border-border bg-background p-2 text-sm file:mr-3 file:rounded-lg file:border file:bg-muted file:px-3 file:py-2 file:text-sm file:font-medium hover:file:bg-muted/70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                />
                <Button variant="outline" onClick={() => void onVerifyReport()} disabled={!verificationFile || verifying}>
                  {verifying ? "Verifying…" : "Verify signature"}
                </Button>
              </div>
              {verificationError ? <p role="alert" className="mt-3 text-sm text-destructive">{verificationError}</p> : null}
              {verificationNote ? <p role="status" className="mt-3 border-l-2 border-foreground/30 pl-3 text-sm leading-relaxed">{verificationNote}</p> : null}
            </section>
          ) : null}
          {analyzing && progress ? <AnalysisProgressPanel progress={progress} /> : null}

          <div>
            <h3 className="text-sm font-medium">Pairwise overlap</h3>
            <div className="mt-3 overflow-x-auto rounded-xl border border-border/80">
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
            <label className="mt-2 flex items-start gap-2 text-sm">
              <input
                type="checkbox"
                checked={anonymizeReports}
                onChange={(event) => setAnonymizeReports(event.currentTarget.checked)}
                className="mt-0.5 h-4 w-4 rounded border-input"
              />
              <span>
                Anonymize names and filenames in exported PDFs
                <span className="block text-xs text-muted-foreground">
                  Submitted text is unchanged and may itself identify someone.
                </span>
              </span>
            </label>
            {reportError ? <p role="alert" className="mt-2 text-sm text-destructive">{reportError}</p> : null}
            {reportNotice ? <p role="status" className="mt-2 text-sm text-muted-foreground">{reportNotice}</p> : null}
            {selfCheckError ? <p role="alert" className="mt-2 text-sm text-destructive">{selfCheckError}</p> : null}
            {selfCheckNotice ? <p role="status" className="mt-2 text-sm text-muted-foreground">{selfCheckNotice}</p> : null}
            <ul className="mt-3 grid gap-3">
              {report.per_student.map((row) => (
                <li key={row.student_id} className="rounded-xl border border-border/70 bg-background p-3.5 sm:p-4">
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
                  {sessionStatus === "locked" ? (
                    <Button
                      className="mt-2"
                      size="sm"
                      variant="outline"
                      onClick={() => void onGenerateReport(row.student_id)}
                      disabled={
                        reportingStudentId !== null ||
                        lockQuery.data === null ||
                        lockQuery.data === undefined ||
                        submissionsQuery.isPending ||
                        !(textByStudent.get(row.student_id)?.trim())
                      }
                    >
                      {reportingStudentId === row.student_id ? "Signing report…" : "Generate certified report for " + (nameById.get(row.student_id) ?? "student")}
                    </Button>
                  ) : (
                    <div className="mt-2 flex flex-wrap items-center gap-2">
                      <Button
                        size="sm"
                        variant="outline"
                        onClick={() => void onGenerateSelfCheck(row.student_id)}
                        disabled={
                          selfCheckStudentId !== null ||
                          submissionsQuery.isPending ||
                          !(textByStudent.get(row.student_id)?.trim())
                        }
                      >
                        {selfCheckStudentId === row.student_id ? "Preparing PDF…" : "Download Self Check PDF"}
                      </Button>
                      <span className="rounded-full bg-primary/10 px-2 py-1 text-[11px] font-medium text-primary">Not Teacher Certified</span>
                    </div>
                  )}
                </li>
              ))}
            </ul>
          </div>

          {report.compared_libraries.length > 0 ? (
            <section aria-label="Historical reference comparisons">
              <Separator className="my-4" />
              <h3 className="text-sm font-semibold">Historical reference comparisons</h3>
              <p className="mt-1 text-xs text-muted-foreground">
                Historical evidence is shown separately and is not included in
                current-student pair scores.
              </p>
              <ul className="mt-2 space-y-1 text-sm">
                {report.compared_libraries.map((library) => (
                  <li key={library.id}>
                    {library.name} · {library.source_count} historical submissions
                    <span className="text-muted-foreground"> · from {library.source_session_name}</span>
                  </li>
                ))}
              </ul>
              {report.historical_matches.length === 0 ? (
                <p className="mt-3 text-sm text-muted-foreground">
                  No matching passages were found in the selected historical libraries.
                </p>
              ) : (
                <ol className="mt-3 space-y-4">
                  {report.historical_matches.map((match) => (
                    <HistoricalMatchCard
                      key={`${match.student_id}:${match.reference_submission_id}`}
                      match={match}
                      studentName={nameById.get(match.student_id) ?? "Unknown student"}
                      studentText={textByStudent.get(match.student_id) ?? ""}
                    />
                  ))}
                </ol>
              )}
            </section>
          ) : null}

          {selectedPair ? (
            <div aria-label="Pair detail" className="rounded-2xl border border-border/80 bg-muted/20 p-4 sm:p-5">
              <h3 className="text-sm font-semibold">
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

function HistoricalMatchCard({
  match,
  studentName,
  studentText,
}: {
  match: HistoricalPairAnalysis;
  studentName: string;
  studentText: string;
}) {
  return (
    <li className="rounded-lg border p-4">
      <h4 className="text-sm font-medium">
        {studentName} · {match.library_name} · {match.reference_label}
      </h4>
      <p className="mt-1 text-xs text-muted-foreground">
        Historical source from {match.reference_filename ?? "an archived submission"} · current-text coverage {formatOptionalPct(match.coverage_current)}
      </p>
      <p className="mt-1 text-xs text-muted-foreground">
        Exact {formatOptionalPct(match.exact_coverage_current)} · Modified {formatOptionalPct(match.modified_coverage_current)}
      </p>
      <ol className="mt-3 space-y-3">
        {match.passages.map((passage, index) => (
          <PassageCard
            key={`${passage.a_token_start}-${passage.b_token_start}-${index}`}
            index={index}
            passage={passage}
            aName={studentName}
            bName={`${match.library_name} · ${match.reference_label}`}
            aText={studentText}
            bText={match.reference_text}
          />
        ))}
      </ol>
    </li>
  );
}

const PROGRESS_LABELS: Record<AnalysisProgress["stage"], string> = {
  preparing: "Preparing submissions",
  exact: "Finding exact matches",
  modified: "Finding modified matches",
  aligning: "Aligning evidence",
  scoring: "Scoring unique matched spans",
  historical: "Comparing selected reference libraries",
  saving: "Saving analysis",
  complete: "Analysis complete",
  failed: "Analysis failed",
};

function AnalysisProgressPanel({ progress }: { progress: AnalysisProgress }) {
  return (
    <div role="status" aria-live="polite" aria-label="Analysis progress" className="mt-3 rounded-md border p-3">
      <div className="flex justify-between gap-3 text-sm">
        <span>{PROGRESS_LABELS[progress.stage]}</span>
        <span className="font-mono">{Math.round(progress.fraction * 100)}%</span>
      </div>
      <progress
        className="mt-2 h-2 w-full accent-primary"
        max={1}
        value={progress.fraction}
        aria-label="Session analysis progress"
      />
      {progress.total_pairs > 0 ? (
        <p className="mt-1 text-xs text-muted-foreground">
          {progress.completed_pairs}/{progress.total_pairs} pairs · {progress.cached_pairs} unchanged raw pair results reused
        </p>
      ) : null}
    </div>
  );
}
