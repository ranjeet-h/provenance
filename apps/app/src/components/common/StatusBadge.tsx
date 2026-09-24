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
      variant={status === "draft" ? "secondary" : status === "locked" ? "outline" : "default"}
      aria-label={`Status: ${LABELS[status]}`}
    >
      {LABELS[status]}
    </Badge>
  );
}
