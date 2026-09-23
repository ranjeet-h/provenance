import { TriangleAlert } from "lucide-react";
import { Button } from "@/components/ui/button";

export function ErrorState({
  title = "Something went wrong",
  message,
  onRetry,
}: {
  title?: string;
  message?: string;
  onRetry?: () => void;
}) {
  return (
    <div
      role="alert"
      className="flex flex-col items-center justify-center rounded-2xl border border-destructive/20 bg-destructive/[0.025] px-6 py-12 text-center shadow-sm shadow-destructive/[0.025]"
    >
      <TriangleAlert className="mb-3 h-8 w-8 text-destructive" aria-hidden />
      <h2 className="text-base font-semibold tracking-tight">{title}</h2>
      {message ? (
        <p className="mt-1 max-w-sm text-sm leading-relaxed text-muted-foreground">{message}</p>
      ) : null}
      {onRetry ? (
        <Button variant="outline" className="mt-4" onClick={onRetry}>
          Try again
        </Button>
      ) : null}
    </div>
  );
}
