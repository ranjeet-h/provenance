import * as React from "react";
import { Inbox } from "lucide-react";

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
    <div className="flex flex-col items-center justify-center rounded-2xl border border-dashed border-border bg-card/75 px-6 py-12 text-center shadow-sm shadow-slate-900/[0.015]">
      <div className="mb-4 grid h-14 w-14 place-items-center rounded-2xl bg-primary/[0.07] text-primary" aria-hidden>
        {icon ?? <Inbox className="h-7 w-7" />}
      </div>
      <h2 className="text-base font-semibold tracking-tight">{title}</h2>
      {description ? (
        <p className="mt-1 max-w-sm text-sm leading-relaxed text-muted-foreground">{description}</p>
      ) : null}
      {action ? <div className="mt-4">{action}</div> : null}
    </div>
  );
}
