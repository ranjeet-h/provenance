import * as React from "react";
import { Inbox } from "lucide-react";
import { Card } from "@/components/ui/card";

export function EmptyState({
  title,
  description,
  action,
  icon,
}: {
  title: string;
  description?: string;
  action?: React.ReactNode;
  icon?: React.ReactNode;
}) {
  return (
    <Card className="items-center justify-center gap-0 border-dashed bg-card/75 px-6 py-12 text-center">
      <div
        className="mb-4 grid size-12 place-items-center rounded-md bg-primary/[0.07] text-primary"
        aria-hidden
      >
        {icon ?? <Inbox className="h-7 w-7" />}
      </div>
      <h2 className="text-base font-semibold tracking-tight">{title}</h2>
      {description ? (
        <p className="mt-1 max-w-sm text-sm leading-relaxed text-muted-foreground">
          {description}
        </p>
      ) : null}
      {action ? <div className="mt-4">{action}</div> : null}
    </Card>
  );
}
