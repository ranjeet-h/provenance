import * as React from "react";
import { Link, useNavigate, useParams } from "@tanstack/react-router";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Trash2, UserPlus } from "lucide-react";
import { PageHeader } from "@/components/common/PageHeader";
import { EmptyState } from "@/components/common/EmptyState";
import { LoadingState } from "@/components/common/LoadingState";
import { ErrorState } from "@/components/common/ErrorState";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { StatusBadge } from "@/components/common/StatusBadge";
import { AnalysisSection } from "@/components/analysis/AnalysisSection";
import { SubmissionDialog } from "@/components/submissions/SubmissionDialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Separator } from "@/components/ui/separator";
import {
  addStudent,
  asSessionsError,
  deleteSession,
  getSession,
  listStudents,
  listSubmissions,
  removeStudent,
  updateSession,
  sourceLabel,
  type Session,
} from "@/lib/sessions";

function RenameSessionForm({ session }: { session: Session }) {
  const queryClient = useQueryClient();
  const [rename, setRename] = React.useState(session.name);
  const [renameError, setRenameError] = React.useState<string | null>(null);

  async function onRename(e: React.FormEvent): Promise<void> {
    e.preventDefault();
    setRenameError(null);
    if (rename.trim() === "") {
      setRenameError("Enter a session name.");
      return;
    }
    try {
      await updateSession(session.id, { name: rename });
      await queryClient.invalidateQueries({ queryKey: ["session", session.id] });
      await queryClient.invalidateQueries({ queryKey: ["sessions"] });
    } catch (err) {
      setRenameError(asSessionsError(err).message);
    }
  }

  return (
    <section aria-label="Rename session" className="rounded-lg border p-4">
      <h2 className="text-sm font-medium">Rename session</h2>
      <form className="mt-2 flex max-w-md gap-2" onSubmit={(e) => void onRename(e)}>
        <Input
          aria-label="Session name"
          value={rename}
          onChange={(e) => setRename(e.currentTarget.value)}
        />
        <Button type="submit" variant="outline">
          Save
        </Button>
      </form>
      {renameError ? (
        <p role="alert" className="mt-2 text-sm text-destructive">{renameError}</p>
      ) : null}
    </section>
  );
}

function SessionSettingsForm({ session }: { session: Session }) {
  const queryClient = useQueryClient();
  const [prompt, setPrompt] = React.useState(session.assignment_prompt ?? "");
  const [reference, setReference] = React.useState(session.excluded_reference_text ?? "");
  const [excludeCommon, setExcludeCommon] = React.useState(session.exclude_common_text);
  const [message, setMessage] = React.useState<{ kind: "ok" | "error"; text: string } | null>(null);
  const [saving, setSaving] = React.useState(false);

  async function onSave(e: React.FormEvent): Promise<void> {
    e.preventDefault();
    setMessage(null);
    setSaving(true);
    try {
      await updateSession(session.id, {
        assignmentPrompt: prompt.trim() === "" ? null : prompt,
        excludedReferenceText: reference.trim() === "" ? null : reference,
        excludeCommonText: excludeCommon,
      });
      await queryClient.invalidateQueries({ queryKey: ["session", session.id] });
      await queryClient.invalidateQueries({ queryKey: ["sessions"] });
      setMessage({ kind: "ok", text: "Settings saved." });
    } catch (err) {
      setMessage({ kind: "error", text: asSessionsError(err).message });
    } finally {
      setSaving(false);
    }
  }

  return (
    <section aria-label="Session settings" className="mt-4 rounded-lg border p-4">
      <h2 className="text-sm font-medium">Session settings</h2>
      <p className="mt-1 text-sm text-muted-foreground">
        The assignment question is excluded from overlap scoring. Text shared
        across many submissions is flagged as shared but still counted —
        frequency alone never erases a match.
      </p>
      <form className="mt-3 max-w-xl space-y-3" onSubmit={(e) => void onSave(e)}>
        <div className="space-y-1">
          <label htmlFor={`prompt-${session.id}`} className="text-sm font-medium">
            Assignment question / instructions
          </label>
          <textarea
            id={`prompt-${session.id}`}
            rows={3}
            value={prompt}
            onChange={(e) => setPrompt(e.currentTarget.value)}
            placeholder="Paste the assignment question every student received…"
            className="flex min-h-[72px] w-full rounded-md border border-input bg-background px-3 py-2 text-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          />
        </div>
        <div className="space-y-1">
          <label htmlFor={`reference-${session.id}`} className="text-sm font-medium">
            Extra reference text to exclude <span className="text-muted-foreground">(optional)</span>
          </label>
          <textarea
            id={`reference-${session.id}`}
            rows={2}
            value={reference}
            onChange={(e) => setReference(e.currentTarget.value)}
            placeholder="Required declarations, standard headings…"
            className="flex min-h-[56px] w-full rounded-md border border-input bg-background px-3 py-2 text-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          />
        </div>
        <div className="flex items-center gap-2">
          <input
            id={`exclude-common-${session.id}`}
            type="checkbox"
            checked={excludeCommon}
            onChange={(e) => setExcludeCommon(e.currentTarget.checked)}
            className="h-4 w-4 rounded border-input"
          />
          <label htmlFor={`exclude-common-${session.id}`} className="text-sm">
            Flag text shared across many submissions
          </label>
        </div>
        {message ? (
          <p
            role={message.kind === "error" ? "alert" : "status"}
            className={
              message.kind === "error" ? "text-sm text-destructive" : "text-sm text-muted-foreground"
            }
          >
            {message.text}
          </p>
        ) : null}
        <Button type="submit" variant="outline" disabled={saving}>
          {saving ? "Saving…" : "Save settings"}
        </Button>
      </form>
    </section>
  );
}

export function SessionDetailPage() {  const { sessionId } = useParams({ strict: false }) as { sessionId?: string };
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [studentName, setStudentName] = React.useState("");
  const [studentError, setStudentError] = React.useState<string | null>(null);
  const [dialogFor, setDialogFor] = React.useState<string | null>(null);
  const enabled = sessionId !== undefined && sessionId !== "";

  const sessionQuery = useQuery({
    queryKey: ["session", sessionId],
    queryFn: () => getSession(sessionId ?? ""),
    enabled,
  });
  const studentsQuery = useQuery({
    queryKey: ["students", sessionId],
    queryFn: () => listStudents(sessionId ?? ""),
    enabled,
  });
  const submissionsQuery = useQuery({
    queryKey: ["submissions", sessionId],
    queryFn: () => listSubmissions(sessionId ?? ""),
    enabled,
  });

  if (!enabled) {
    return <ErrorState title="Session not found" message="Missing session id." />;
  }
  if (sessionQuery.isPending) {
    return <LoadingState label="Loading session…" />;
  }
  if (sessionQuery.isError || !sessionQuery.data) {
    return (
      <div>
        <ErrorState
          title="Could not load session"
          message={asSessionsError(sessionQuery.error).message}
          onRetry={() => void sessionQuery.refetch()}
        />
        <div className="mt-4">
          <Button asChild variant="outline">
            <Link to="/sessions">Back to sessions</Link>
          </Button>
        </div>
      </div>
    );
  }

  const session = sessionQuery.data;
  const students = studentsQuery.data ?? [];
  const submissionsByStudent = new Map(
    (submissionsQuery.data ?? []).map((s) => [s.student_id, s] as const),
  );

  async function refreshSubmissions(): Promise<void> {
    await queryClient.invalidateQueries({ queryKey: ["submissions", session.id] });
  }

  async function onAddStudent(e: React.FormEvent): Promise<void> {
    e.preventDefault();
    setStudentError(null);
    if (studentName.trim() === "") {
      setStudentError("Enter a student name.");
      return;
    }
    try {
      await addStudent(session.id, studentName);
      setStudentName("");
      await queryClient.invalidateQueries({ queryKey: ["students", session.id] });
    } catch (err) {
      setStudentError(asSessionsError(err).message);
    }
  }

  async function onRemoveStudent(id: string): Promise<void> {
    setStudentError(null);
    try {
      await removeStudent(id);
      await queryClient.invalidateQueries({ queryKey: ["students", session.id] });
    } catch (err) {
      setStudentError(asSessionsError(err).message);
    }
  }

  async function onDeleteSession(): Promise<void> {
    await deleteSession(session.id);
    await queryClient.invalidateQueries({ queryKey: ["sessions"] });
    await navigate({ to: "/sessions" });
  }

  return (
    <div>
      <PageHeader
        title={session.name}
        description={session.subject ?? "No subject set."}
        actions={<StatusBadge status={session.status} />}
      />

      <RenameSessionForm key={session.id} session={session} />

      <SessionSettingsForm key={`settings-${session.id}`} session={session} />

      <section aria-label="Students" className="mt-6">
        <h2 className="text-lg font-medium">Students</h2>
        <form className="mt-2 flex max-w-md gap-2" onSubmit={(e) => void onAddStudent(e)}>
          <Input
            aria-label="Student name"
            placeholder="Add a student…"
            value={studentName}
            onChange={(e) => setStudentName(e.currentTarget.value)}
          />
          <Button type="submit">
            <UserPlus className="h-4 w-4" aria-hidden />
            Add
          </Button>
        </form>
        {studentError ? (
          <p role="alert" className="mt-2 text-sm text-destructive">{studentError}</p>
        ) : null}
        <div className="mt-4">
          {studentsQuery.isPending ? (
            <LoadingState label="Loading students…" />
          ) : studentsQuery.isError ? (
            <ErrorState
              title="Could not load students"
              message={asSessionsError(studentsQuery.error).message}
              onRetry={() => void studentsQuery.refetch()}
            />
          ) : students.length === 0 ? (
            <EmptyState
              title="No students yet"
              description="Add the solvers for this assignment."
            />
          ) : (
            <ul className="divide-y rounded-lg border" aria-label="Student list">
              {students.map((student) => {
                const submission = submissionsByStudent.get(student.id);
                return (
                  <li key={student.id} className="flex flex-col gap-2 p-3 sm:flex-row sm:items-center sm:justify-between">
                    <div className="min-w-0">
                      <span className="text-sm font-medium">{student.display_name}</span>
                      <p className="truncate text-xs text-muted-foreground">
                        {submission
                          ? [
                              sourceLabel(submission.source_type),
                              submission.source_filename,
                              `${submission.original_text.length} characters`,
                            ]
                              .filter((part) => part !== null && part !== "")
                              .join(" · ")
                          : submissionsQuery.isPending
                            ? "Checking submission…"
                            : "No submission yet"}
                      </p>
                      {submission && submission.original_text !== "" ? (
                        <p className="mt-1 truncate text-xs text-muted-foreground">
                          {submission.original_text.slice(0, 120)}
                        </p>
                      ) : null}
                    </div>
                    <div className="flex shrink-0 items-center gap-1">
                      <SubmissionDialog
                        studentName={student.display_name}
                        studentId={student.id}
                        existing={submission ?? null}
                        open={dialogFor === student.id}
                        onOpenChange={(o) => setDialogFor(o ? student.id : null)}
                        onSaved={() => {
                          setDialogFor(null);
                          void refreshSubmissions();
                        }}
                        trigger={
                          <Button variant="outline" size="sm">
                            {submission ? "View / Replace" : "Add submission"}
                          </Button>
                        }
                      />
                      <ConfirmDialog
                        title={`Remove ${student.display_name}?`}
                        description="The student and their submissions leave this session."
                        confirmLabel="Remove"
                        onConfirm={() => void onRemoveStudent(student.id)}
                        trigger={
                          <Button variant="ghost" size="sm" aria-label={`Remove ${student.display_name}`}>
                            <Trash2 className="h-4 w-4" aria-hidden />
                            Remove
                          </Button>
                        }
                      />
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      </section>

      <AnalysisSection
        sessionId={session.id}
        students={students}
        submissionCount={submissionsByStudent.size}
      />

      <Separator className="my-6" />
      <ConfirmDialog
        title={`Delete ${session.name}?`}
        description="This removes the session and its students. This cannot be undone."
        confirmLabel="Delete"
        onConfirm={() => void onDeleteSession()}
        trigger={
          <Button variant="destructive" aria-label={`Delete ${session.name}`}>
            <Trash2 className="h-4 w-4" aria-hidden />
            Delete session
          </Button>
        }
      />
    </div>
  );
}
