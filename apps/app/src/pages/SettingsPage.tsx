import * as React from "react";
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
    <div>
      <PageHeader
        title="Settings"
        description="App preferences and text inspection tools."
      />
      <section aria-label="Shell diagnostics" className="rounded-lg border p-5">
        <h2 className="text-base font-medium">Shell diagnostics</h2>
        <p className="mt-1 text-sm text-muted-foreground">
          Verifies the React → Tauri command boundary. No assignment data is sent.
        </p>
        <Separator className="my-4" />
        <form
          className="flex max-w-md gap-2"
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
          <Button type="submit">Greet</Button>
        </form>
        {message !== "" ? (
          <p aria-live="polite" className="mt-3 text-sm">
            {message}
          </p>
        ) : null}
      </section>

      <details className="mt-4 rounded-lg border p-5">
        <summary className="cursor-pointer text-base font-medium">
          Developer inspection
        </summary>
        <p className="mt-1 text-sm text-muted-foreground">
          Shows normalized tokens and their original ranges. Temporary debug UI.
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
      <label htmlFor="inspect-input" className="text-sm font-medium">
        Original text
      </label>
      <textarea
        id="inspect-input"
        rows={4}
        value={input}
        onChange={(e) => setInput(e.currentTarget.value)}
        placeholder="Paste a paragraph with punctuation and capitals…"
        className={cn(
          "flex min-h-[100px] w-full rounded-md border border-input bg-background px-3 py-2 text-sm",
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
          <p className="rounded-md border bg-muted/50 p-3 font-mono text-xs">
            {result.normalized_text}
          </p>
          <div className="max-h-64 overflow-y-auto rounded-md border">
            <table className="w-full text-left text-xs">
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
