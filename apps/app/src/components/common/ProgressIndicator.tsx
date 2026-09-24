import { Progress } from "@/components/ui/progress";

export function ProgressIndicator({
  value,
  label,
}: {
  value: number;
  label?: string;
}) {
  const rounded = Math.round(Math.min(100, Math.max(0, value)));
  return (
    <div className="space-y-1">
      {label ? (
        <div className="flex items-center justify-between text-xs text-muted-foreground">
          <span>{label}</span>
          <span aria-hidden>{rounded}%</span>
        </div>
      ) : null}
      <Progress
        value={rounded}
        aria-label={label ?? "Progress"}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={rounded}
      />
    </div>
  );
}
