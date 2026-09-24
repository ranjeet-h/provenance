import { Check, FileText, HardDrive, ShieldCheck } from "lucide-react";
import { PageHeader } from "@/components/common/PageHeader";
import { Card, CardContent, CardHeader } from "@/components/ui/card";

export function SettingsPage() {
  return (
    <div className="space-y-7">
      <PageHeader
        eyebrow="Workspace preferences"
        title="Settings"
        description="Review how this workspace handles assignment data and the document types it accepts."
      />
      <section
        aria-label="Local data and supported sources"
        className="grid gap-4 lg:grid-cols-2"
      >
        <Card className="h-full gap-0 p-0">
          <CardHeader className="flex-row items-start gap-3 p-5 pb-0 sm:p-6 sm:pb-0">
            <span className="grid size-9 shrink-0 place-items-center rounded-md bg-primary/10 text-primary">
              <HardDrive className="h-5 w-5" aria-hidden />
            </span>
            <div className="space-y-1">
              <h2 className="text-base font-semibold tracking-tight">
                Stored on this device
              </h2>
              <p className="text-sm leading-relaxed text-muted-foreground">
                Sessions, submissions, analysis, and reference archives are kept
                in this local workspace.
              </p>
            </div>
          </CardHeader>
          <CardContent className="px-5 pt-4 pb-5 sm:px-6 sm:pb-6">
            <div className="flex items-center gap-2 border-t pt-4 text-sm font-medium text-foreground">
              <ShieldCheck className="h-4 w-4 text-emerald-700" aria-hidden />
              Data remains in the desktop app
            </div>
          </CardContent>
        </Card>

        <Card className="h-full gap-0 p-0">
          <CardHeader className="flex-row items-start gap-3 p-5 pb-0 sm:p-6 sm:pb-0">
            <span className="grid size-9 shrink-0 place-items-center rounded-md bg-secondary text-secondary-foreground">
              <FileText className="h-5 w-5" aria-hidden />
            </span>
            <div className="space-y-1">
              <h2 className="text-base font-semibold tracking-tight">
                Supported text sources
              </h2>
              <p className="text-sm leading-relaxed text-muted-foreground">
                Add pasted text or import a digital document with extractable
                text.
              </p>
            </div>
          </CardHeader>
          <CardContent className="px-5 pt-4 pb-5 sm:px-6 sm:pb-6">
            <ul
              className="grid grid-cols-2 gap-x-4 gap-y-2 border-t pt-4"
              aria-label="Supported input types"
            >
              {["Pasted text", "TXT", "Markdown", "Digital PDF", "DOCX"].map(
                (source) => (
                  <li key={source} className="flex items-center gap-2 text-sm">
                    <Check
                      className="size-4 shrink-0 text-emerald-700"
                      aria-hidden
                    />
                    {source}
                  </li>
                ),
              )}
            </ul>
            <p className="mt-3 text-xs leading-relaxed text-muted-foreground">
              Scanned PDFs and image files are not supported.
            </p>
          </CardContent>
        </Card>
      </section>
    </div>
  );
}
