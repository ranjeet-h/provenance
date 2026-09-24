import * as React from "react";
import { Button } from "@/components/ui/button";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Label } from "@/components/ui/label";
import { Input } from "@/components/ui/input";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  asSessionsError,
  MAX_UPLOAD_BYTES,
  saveFileSubmission,
  saveTextSubmission,
  type Submission,
} from "@/lib/sessions";
import { isPreviewableText, isSupportedFile } from "@/lib/filenames";

const PREVIEW_CHARS = 1000;

function readFileBytes(file: File): Promise<Uint8Array> {
  // FileReader — File.arrayBuffer() is unavailable in some runtimes (jsdom).
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(new Uint8Array(reader.result as ArrayBuffer));
    reader.onerror = () =>
      reject(new Error(`"${file.name}" could not be read. Please try again.`));
    reader.readAsArrayBuffer(file);
  });
}

async function decodeTextFile(file: File): Promise<string> {
  const bytes = await readFileBytes(file);
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    throw new Error(
      `"${file.name}" could not be read as text. Only TXT and Markdown files are supported.`,
    );
  }
}

export function SubmissionDialog({
  studentName,
  studentId,
  existing,
  open,
  onOpenChange,
  onSaved,
  trigger,
}: {
  studentName: string;
  studentId: string;
  existing?: Submission | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSaved: () => void;
  trigger: React.ReactNode;
}) {
  const [mode, setMode] = React.useState<"paste" | "upload">("paste");
  const [text, setText] = React.useState(existing?.original_text ?? "");
  const [file, setFile] = React.useState<File | null>(null);
  const [fileName, setFileName] = React.useState<string | null>(
    existing?.source_filename ?? null,
  );
  const [error, setError] = React.useState<string | null>(null);
  const [saving, setSaving] = React.useState(false);

  // Reset the draft when a different submission is opened (render-time
  // adjustment — the endorsed alternative to syncing state in an effect).
  const draftKey = `${studentId}:${existing?.id ?? "new"}:${existing?.updated_at ?? ""}`;
  const [seenKey, setSeenKey] = React.useState(draftKey);
  if (seenKey !== draftKey) {
    setSeenKey(draftKey);
    setText(existing?.original_text ?? "");
    setFile(null);
    setFileName(existing?.source_filename ?? null);
    setError(null);
    setMode("paste");
  }

  async function onFileChosen(chosen: File | undefined): Promise<void> {
    setError(null);
    if (!chosen) {
      return;
    }
    if (!isSupportedFile(chosen.name)) {
      setError("Unsupported file type. Use TXT, Markdown, PDF, or DOCX.");
      return;
    }
    setFile(chosen);
    setFileName(chosen.name);
    if (isPreviewableText(chosen.name)) {
      try {
        setText(await decodeTextFile(chosen));
      } catch (err) {
        setText("");
        setError(
          err instanceof Error ? err.message : "Could not read that file.",
        );
      }
    } else {
      setText("");
    }
  }

  async function onSaveUpload(): Promise<void> {
    if (!file || !fileName) {
      setError("Choose a file first.");
      return;
    }
    // Fast client-side guard (the Rust core re-enforces the same limit):
    // never send an oversized upload over IPC.
    if (file.size > MAX_UPLOAD_BYTES) {
      setError(`File is too large (max ${MAX_UPLOAD_BYTES / 1_000_000} MB).`);
      return;
    }
    setSaving(true);
    try {
      const bytes = Array.from(await readFileBytes(file));
      await saveFileSubmission({ studentId, filename: fileName, bytes });
      onOpenChange(false);
      onSaved();
    } catch (err) {
      setError(asSessionsError(err).message);
    } finally {
      setSaving(false);
    }
  }

  async function onSave(): Promise<void> {
    setError(null);
    if (mode === "upload") {
      await onSaveUpload();
      return;
    }
    if (text.trim() === "") {
      setError("Enter or upload some text first.");
      return;
    }
    setSaving(true);
    try {
      await saveTextSubmission({
        studentId,
        sourceType: "pasted_text",
        filename: null,
        text,
      });
      onOpenChange(false);
      onSaved();
    } catch (err) {
      setError(asSessionsError(err).message);
    } finally {
      setSaving(false);
    }
  }

  const preview = text.slice(0, PREVIEW_CHARS);
  const showPreview =
    mode === "paste" || (file !== null && isPreviewableText(fileName ?? ""));

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogTrigger asChild>{trigger}</DialogTrigger>
      <DialogContent className="max-h-[92vh] overflow-y-auto p-5 sm:max-w-2xl sm:p-7">
        <DialogHeader>
          <DialogTitle>
            {existing
              ? `Replace submission — ${studentName}`
              : `Add submission — ${studentName}`}
          </DialogTitle>
          <DialogDescription>
            Paste text or upload a TXT, Markdown, PDF, or Word file. Saved on
            this device only.
          </DialogDescription>
        </DialogHeader>

        <Tabs
          value={mode}
          onValueChange={(value) => {
            setMode(value as "paste" | "upload");
            setError(null);
          }}
          className="w-full"
        >
          <TabsList aria-label="Input method">
            <TabsTrigger value="paste">Paste text</TabsTrigger>
            <TabsTrigger value="upload">Upload file</TabsTrigger>
          </TabsList>
          <TabsContent value="paste" className="space-y-2">
            <Label htmlFor={`submission-text-${studentId}`}>
              Submission text
            </Label>
            <Textarea
              id={`submission-text-${studentId}`}
              rows={8}
              value={text}
              onChange={(e) => setText(e.currentTarget.value)}
              placeholder="Paste the assignment text here…"
              aria-invalid={error !== null}
              className="min-h-[180px] leading-relaxed"
            />
          </TabsContent>
          <TabsContent value="upload" className="space-y-2">
            <Label htmlFor={`submission-file-${studentId}`}>
              Assignment file
            </Label>
            <Input
              id={`submission-file-${studentId}`}
              type="file"
              accept=".txt,.md,.markdown,.pdf,.docx,text/plain,text/markdown,application/pdf,application/vnd.openxmlformats-officedocument.wordprocessingml.document"
              onChange={(e) => void onFileChosen(e.currentTarget.files?.[0])}
              className="h-auto border-dashed bg-muted/20 p-3 text-sm file:mr-3 file:rounded-md file:border file:border-input file:bg-background file:px-3 file:py-2 file:text-sm file:font-medium hover:file:bg-muted"
            />
            <p className="text-xs leading-relaxed text-muted-foreground">
              Supported: TXT, Markdown, digital PDF, and DOCX. Scanned or
              image-only files are not supported.
            </p>
            {fileName ? (
              <p className="text-sm text-muted-foreground">
                Selected: {fileName}
              </p>
            ) : null}
          </TabsContent>
        </Tabs>

        <div className="space-y-1">
          <h3 className="text-sm font-semibold">Preview</h3>
          {showPreview ? (
            preview !== "" ? (
              <pre
                aria-label="Submission preview"
                className="max-h-56 overflow-y-auto whitespace-pre-wrap rounded-md border border-border/80 bg-muted/35 p-3.5 text-sm leading-relaxed"
              >
                {preview}
                {text.length > PREVIEW_CHARS ? (
                  <span className="text-muted-foreground">
                    {`\n… +${text.length - PREVIEW_CHARS} more characters`}
                  </span>
                ) : null}
              </pre>
            ) : (
              <p className="text-sm text-muted-foreground">
                Nothing to preview yet.
              </p>
            )
          ) : (
            <p className="text-sm text-muted-foreground">
              {fileName !== null && !isPreviewableText(fileName)
                ? "Text will be extracted when you save."
                : "Nothing to preview yet."}
            </p>
          )}
        </div>

        {error ? (
          <Alert variant="destructive">
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        ) : null}
        <DialogFooter className="flex-col-reverse gap-2 border-t border-border/70 pt-4 sm:flex-row sm:gap-2 sm:pt-5">
          <Button
            className="w-full sm:w-auto"
            variant="outline"
            onClick={() => onOpenChange(false)}
          >
            Cancel
          </Button>
          <Button
            className="w-full sm:w-auto"
            onClick={() => void onSave()}
            disabled={saving}
          >
            {saving
              ? "Saving…"
              : existing
                ? "Replace submission"
                : "Save submission"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
