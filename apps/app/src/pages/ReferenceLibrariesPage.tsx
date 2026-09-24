import * as React from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Archive, Library, Upload } from "lucide-react";
import { PageHeader } from "@/components/common/PageHeader";
import { EmptyState } from "@/components/common/EmptyState";
import { LoadingState } from "@/components/common/LoadingState";
import { ErrorState } from "@/components/common/ErrorState";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import {
  deleteReferenceLibrary,
  exportReferenceLibrary,
  importReferenceLibrary,
  listReferenceLibraries,
  listReferenceSubmissions,
  MAX_PLAGPACK_BYTES,
} from "@/lib/referenceLibraries";
import { asSessionsError } from "@/lib/sessions";

export function ReferenceLibrariesPage() {
  const queryClient = useQueryClient();
  const importInput = React.useRef<HTMLInputElement>(null);
  const [selectedId, setSelectedId] = React.useState<string | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [message, setMessage] = React.useState<string | null>(null);
  const [importing, setImporting] = React.useState(false);
  const [exportingId, setExportingId] = React.useState<string | null>(null);
  const librariesQuery = useQuery({
    queryKey: ["reference-libraries"],
    queryFn: listReferenceLibraries,
    retry: false,
  });
  const submissionsQuery = useQuery({
    queryKey: ["reference-library-submissions", selectedId],
    queryFn: () => listReferenceSubmissions(selectedId ?? ""),
    enabled: selectedId !== null,
    retry: false,
  });

  async function onDelete(id: string): Promise<void> {
    setError(null);
    try {
      await deleteReferenceLibrary(id);
      if (selectedId === id) setSelectedId(null);
      await queryClient.invalidateQueries({
        queryKey: ["reference-libraries"],
      });
      queryClient.setQueriesData({ queryKey: ["analysis"] }, null);
      await queryClient.invalidateQueries({ queryKey: ["analysis"] });
    } catch (cause) {
      setError(asSessionsError(cause).message);
    }
  }

  async function onImport(
    event: React.ChangeEvent<HTMLInputElement>,
  ): Promise<void> {
    const input = event.currentTarget;
    const file = input.files?.[0];
    if (!file) return;
    setError(null);
    setMessage(null);
    setImporting(true);
    try {
      if (!file.name.toLowerCase().endsWith(".plagpack")) {
        throw {
          code: "validation",
          message: "Choose a .plagpack reference library.",
        };
      }
      if (file.size > MAX_PLAGPACK_BYTES) {
        throw {
          code: "validation",
          message: "The .plagpack file exceeds the 50 MB limit.",
        };
      }
      const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
      const imported = await importReferenceLibrary(bytes);
      await queryClient.invalidateQueries({
        queryKey: ["reference-libraries"],
      });
      setMessage(`Imported “${imported.name}” with verified text and hashes.`);
    } catch (cause) {
      setError(asSessionsError(cause).message);
    } finally {
      setImporting(false);
      input.value = "";
    }
  }

  async function onExport(id: string, name: string): Promise<void> {
    setError(null);
    setMessage(null);
    setExportingId(id);
    try {
      const savedPath = await exportReferenceLibrary(id);
      setMessage(
        savedPath
          ? `Saved “${name}” as an anonymized .plagpack: ${savedPath}`
          : "Export cancelled. No file was written.",
      );
    } catch (cause) {
      setError(asSessionsError(cause).message);
    } finally {
      setExportingId(null);
    }
  }

  return (
    <div>
      <PageHeader
        eyebrow="Comparison corpora"
        title="Reference Libraries"
        description="Manage local, read-only archives. A session compares only against the libraries you explicitly select."
        actions={
          <>
            <Button
              onClick={() => importInput.current?.click()}
              disabled={importing}
            >
              <Upload className="h-4 w-4" aria-hidden />
              {importing ? "Importing…" : "Import archive"}
            </Button>
            <Input
              ref={importInput}
              type="file"
              accept=".plagpack,application/zip"
              aria-label="Import .plagpack reference library"
              disabled={importing}
              onChange={(event) => void onImport(event)}
              className="sr-only"
            />
          </>
        }
      />
      {librariesQuery.data ? (
        <div className="mb-5 grid grid-cols-2 gap-3 sm:max-w-lg">
          <Card className="gap-1 px-4 py-3 shadow-sm">
            <p className="text-[10px] font-semibold uppercase tracking-[0.13em] text-muted-foreground">
              Local archives
            </p>
            <p className="mt-1 text-xl font-semibold tracking-tight">
              {librariesQuery.data.length}
            </p>
          </Card>
          <Card className="gap-1 px-4 py-3 shadow-sm">
            <p className="text-[10px] font-semibold uppercase tracking-[0.13em] text-muted-foreground">
              Portable format
            </p>
            <p className="mt-1 text-sm font-semibold tracking-tight">
              .plagpack · 50 MB max
            </p>
          </Card>
        </div>
      ) : null}
      {error ? (
        <Alert variant="destructive" className="mb-3">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      ) : null}
      {message ? (
        <Alert role="status" className="mb-3">
          <AlertDescription>{message}</AlertDescription>
        </Alert>
      ) : null}
      {librariesQuery.isPending ? (
        <LoadingState label="Loading reference libraries…" />
      ) : librariesQuery.isError ? (
        <ErrorState
          title="Could not load reference libraries"
          message={asSessionsError(librariesQuery.error).message}
          onRetry={() => void librariesQuery.refetch()}
        />
      ) : librariesQuery.data.length === 0 ? (
        <EmptyState
          title="No reference libraries yet"
          description="Run an analysis with at least two submissions, then archive that session. The snapshot can be selected later without mixing it into current-student scores."
          icon={<Archive className="h-8 w-8" />}
        />
      ) : (
        <div>
          <ul aria-label="Reference library list" className="space-y-3">
            {librariesQuery.data.map((library) => (
              <li key={library.id}>
                <Card className="gap-4 p-4 shadow-sm sm:p-5">
                  <div className="flex flex-col gap-4 xl:flex-row xl:items-start xl:justify-between">
                    <div className="flex min-w-0 items-start gap-3">
                      <span className="grid h-10 w-10 shrink-0 place-items-center rounded-md bg-primary/[0.075] text-primary">
                        <Library className="h-[18px] w-[18px]" aria-hidden />
                      </span>
                      <div className="min-w-0">
                        <h2 className="truncate text-base font-semibold tracking-tight">
                          {library.name}
                        </h2>
                        <p className="mt-1 text-sm text-muted-foreground">
                          Archived from {library.source_session_name} ·{" "}
                          {new Date(library.created_at).toLocaleDateString()}
                        </p>
                        <p className="mt-2 text-xs text-muted-foreground">
                          Matching versions: fingerprints{" "}
                          {library.fingerprint_version}, normalization{" "}
                          {library.normalization_version}, modified{" "}
                          {library.modified_version}
                        </p>
                      </div>
                    </div>
                    <div className="flex flex-wrap items-center gap-2 xl:justify-end">
                      <Button
                        variant="secondary"
                        size="sm"
                        disabled={exportingId === library.id}
                        onClick={() => void onExport(library.id, library.name)}
                      >
                        {exportingId === library.id
                          ? "Exporting…"
                          : "Export .plagpack"}
                      </Button>
                      <Button
                        variant="outline"
                        size="sm"
                        aria-expanded={selectedId === library.id}
                        onClick={() =>
                          setSelectedId(
                            selectedId === library.id ? null : library.id,
                          )
                        }
                      >
                        {selectedId === library.id
                          ? "Hide sources"
                          : "View sources"}
                      </Button>
                      <ConfirmDialog
                        title={`Remove ${library.name}?`}
                        description="This deletes the archived snapshot and removes it from every session's selected comparison libraries."
                        confirmLabel="Remove library"
                        onConfirm={() => void onDelete(library.id)}
                        trigger={
                          <Button
                            size="sm"
                            variant="ghost"
                            className="text-destructive hover:bg-destructive/10 hover:text-destructive"
                          >
                            Remove
                          </Button>
                        }
                      />
                    </div>
                  </div>
                  {selectedId === library.id ? (
                    <div className="mt-4 rounded-md border border-border/70 bg-muted/20 p-3 sm:p-4">
                      {submissionsQuery.isPending ? (
                        <LoadingState label="Loading archived submissions…" />
                      ) : submissionsQuery.isError ? (
                        <ErrorState
                          title="Could not load archived submissions"
                          message={
                            asSessionsError(submissionsQuery.error).message
                          }
                          onRetry={() => void submissionsQuery.refetch()}
                        />
                      ) : (
                        <ul
                          aria-label="Archived submissions"
                          className="mt-2 divide-y rounded-md border"
                        >
                          {submissionsQuery.data.map((submission) => (
                            <li key={submission.id} className="p-3">
                              <p className="text-sm font-medium">
                                {submission.source_label}
                                {submission.source_filename
                                  ? ` · ${submission.source_filename}`
                                  : ""}
                              </p>
                              <p className="mt-1 whitespace-pre-wrap break-words text-xs text-muted-foreground">
                                {submission.original_text.slice(0, 500)}
                                {submission.original_text.length > 500
                                  ? "…"
                                  : ""}
                              </p>
                            </li>
                          ))}
                        </ul>
                      )}
                    </div>
                  ) : null}
                </Card>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
