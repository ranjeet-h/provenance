import { Skeleton } from "@/components/ui/skeleton";

export function LoadingState({ label = "Loading…" }: { label?: string }) {
  return (
    <div role="status" aria-label={label} className="space-y-3">
      <Skeleton className="h-8 w-1/3" />
      <Skeleton className="h-4 w-full" />
      <Skeleton className="h-4 w-2/3" />
      <span className="sr-only">{label}</span>
    </div>
  );
}
