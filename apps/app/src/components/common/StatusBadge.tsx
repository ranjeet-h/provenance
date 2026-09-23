import { Badge } from "@/components/ui/badge";

export type EntityStatus = "draft" | "locked" | "analyzed";

const LABELS: Record<EntityStatus, string> = {
  draft: "Draft",
  locked: "Locked",
  analyzed: "Analyzed",
};

export function StatusBadge({ status }: { status: EntityStatus }) {
  return (
    <Badge
      variant="outline"
      className={
        status === "draft"
          ? "border-amber-700/15 bg-amber-50 text-amber-900"
          : status === "locked"
            ? "border-emerald-800/15 bg-emerald-50 text-emerald-900"
            : "border-primary/15 bg-primary/[0.07] text-primary"
      }
      aria-label={`Status: ${LABELS[status]}`}
    >
      {LABELS[status]}
    </Badge>
  );
}
