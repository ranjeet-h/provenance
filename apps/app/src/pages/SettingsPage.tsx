import * as React from "react";
import { Check, FileText, HardDrive, ShieldCheck } from "lucide-react";
import { PageHeader } from "@/components/common/PageHeader";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Separator } from "@/components/ui/separator";
import { greet } from "@/lib/tauri";
import { asSessionsError } from "@/lib/sessions";
import { inspectText, type Inspection } from "@/lib/inspect";
import { cn } from "@/lib/utils";

export function SettingsPage() {
  const [name, setName] = React.useState("");
  const [message, setMessage] = React.useState("");

  async function onSubmit(e: React.FormEvent): Promise<void> {
    e.preventDefault();
    try {
      setMessage(await greet(name.trim() === "" ? "Provenance" : name));
    } catch {
      setMessage("Greeting failed (not running in Tauri shell?)");
    }
  }

  return (
    <div className="space-y-7">
      <PageHeader
        eyebrow="Workspace preferences"
        title="Settings"
        description="Review how this workspace handles assignment data and the document types it accepts."
      />
      <section aria-label="Local data and supported sources" className="grid gap-4 lg:grid-cols-2">
        <article className="rounded-2xl border border-primary/15 bg-gradient-to-br from-primary/[0.06] to-card p-5 shadow-sm shadow-primary/[0.035] sm:p-6">
          <div className="flex items-start gap-3">
            <span className="grid h-11 w-11 shrink-0 place-items-center rounded-xl bg-primary/10 text-primary">
              <HardDrive className="h-5 w-5" aria-hidden />
            </span>
            <div>
              <h2 className="text-base font-semibold tracking-tight">Stored on this device</h2>
              <p className="mt-1 text-sm leading-relaxed text-muted-foreground">
                Sessions, submissions, analysis, and reference archives are kept in this local workspace.
              </p>
            </div>
          </div>
          <div className="mt-5 flex items-center gap-2 rounded-xl border border-white/80 bg-white/70 px-3 py-2.5 text-sm font-medium text-foreground">
            <ShieldCheck className="h-4 w-4 text-emerald-700" aria-hidden />
            Data remains in the desktop app
          </div>
        </article>

        <article className="rounded-2xl border border-border/80 bg-card p-5 shadow-sm shadow-slate-900/[0.025] sm:p-6">
          <div className="flex items-start gap-3">
            <span className="grid h-11 w-11 shrink-0 place-items-center rounded-xl bg-secondary text-secondary-foreground">
              <FileText className="h-5 w-5" aria-hidden />
            </span>
            <div>
              <h2 className="text-base font-semibold tracking-tight">Supported text sources</h2>
              <p className="mt-1 text-sm leading-relaxed text-muted-foreground">
                Add pasted text or import a digital document with extractable text.
              </p>
            </div>
          </div>
          <ul className="mt-4 grid grid-cols-2 gap-2" aria-label="Supported input types">
            {["Pasted text", "TXT", "Markdown", "Digital PDF", "DOCX"].map((source) => (
              <li key={source} className="flex items-center gap-2 rounded-lg border border-border/70 bg-background px-2.5 py-2 text-xs font-medium">
                <Check className="h-3.5 w-3.5 shrink-0 text-emerald-700" aria-hidden />
                {source}
              </li>
            ))}
          </ul>
          <p className="mt-3 text-xs leading-relaxed text-muted-foreground">
            Scanned PDFs and image files are not supported.
          </p>
        </article>
      </section>

      <section aria-label="Shell diagnostics" className="max-w-3xl rounded-2xl border border-border/80 bg-card p-5 shadow-sm shadow-slate-900/[0.025] sm:p-6">
        <p className="text-[10px] font-semibold uppercase tracking-[0.15em] text-primary">Desktop connection</p>
        <h2 className="mt-1 text-lg font-semibold tracking-tight">Check the app connection</h2>
        <p className="mt-1 text-sm text-muted-foreground">
          Sends only the name below to the local Tauri command; no assignment data is included.
        </p>
        <Separator className="my-4" />
        <form
          className="flex max-w-md flex-col gap-2 sm:flex-row"
          onSubmit={(e) => {
            void onSubmit(e);
          }}
        >
          <Input
            aria-label="Your name"
            placeholder="Enter a name..."
            value={name}
            onChange={(e) => setName(e.currentTarget.value)}
          />
          <Button type="submit" className="sm:min-w-24">Test connection</Button>
        </form>
        {message !== "" ? (
          <p role="status" className="mt-3 rounded-lg bg-muted/50 px-3 py-2 text-sm">
            {message}
          </p>
        ) : null}
      </section>

      <details className="group max-w-3xl rounded-2xl border border-border/80 bg-card p-5 shadow-sm shadow-slate-900/[0.025] open:shadow-md sm:p-6">
        <summary className="cursor-pointer list-none text-base font-semibold tracking-tight marker:hidden focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring [&::-webkit-details-marker]:hidden">
          <span className="flex items-center justify-between gap-3">
            Advanced text inspection
            <span className="text-xs font-medium text-primary group-open:hidden">Open</span>
            <span className="hidden text-xs font-medium text-primary group-open:inline">Close</span>
          </span>
        </summary>
        <p className="mt-2 text-sm leading-relaxed text-muted-foreground">
          Inspect normalized tokens and their source ranges for troubleshooting.
        </p>
        <Separator className="my-4" />
        <InspectionTool />
      </details>
    </div>
  );
}

function InspectionTool() {
  const [input, setInput] = React.useState("");
  const [result, setResult] = React.useState<Inspection | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);

  async function onInspect(): Promise<void> {
    setError(null);
    setResult(null);
    if (input.trim() === "") {
      setError("Paste a paragraph first.");
      return;
    }
    setBusy(true);
    try {
      setResult(await inspectText(input));
    } catch (err) {
      setError(asSessionsError(err).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="space-y-3">
      <label htmlFor="inspect-input" className="text-sm font-semibold">
        Original text
      </label>
      <textarea
        id="inspect-input"
        rows={4}
        value={input}
        onChange={(e) => setInput(e.currentTarget.value)}
        placeholder="Paste a paragraph with punctuation and capitals…"
        className={cn(
          "flex min-h-[140px] w-full rounded-xl border border-input bg-background px-3 py-2.5 text-sm leading-relaxed shadow-sm shadow-slate-900/[0.02]",
          "placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
        )}
      />
      <Button onClick={() => void onInspect()} disabled={busy}>
        {busy ? "Inspecting…" : "Inspect text"}
      </Button>
      {error ? (
        <p role="alert" className="text-sm text-destructive">{error}</p>
      ) : null}
      {result ? (
        <div className="space-y-2">
          <p className="text-sm text-muted-foreground">
            {result.tokens.length} tokens · {result.sentences.length} sentences ·{" "}
            normalization v{result.normalization_version}
          </p>
          <p className="rounded-xl border bg-muted/50 p-3 font-mono text-xs leading-relaxed">
            {result.normalized_text}
          </p>
          <div className="max-h-72 overflow-auto rounded-xl border">
            <table className="w-full min-w-[420px] text-left text-xs">
              <thead>
                <tr className="border-b">
                  <th className="p-2 font-medium">Token</th>
                  <th className="p-2 font-medium">Original range</th>
                </tr>
              </thead>
              <tbody>
                {result.tokens.map((t) => (
                  <tr key={t.ordinal} className="border-b last:border-0">
                    <td className="p-2 font-mono">{t.normalized}</td>
                    <td className="p-2 font-mono text-muted-foreground">
                      [{t.original_start}, {t.original_end})
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      ) : null}
    </div>
  );
}
