import * as React from "react";
import { cn } from "@/lib/utils";

export function PageHeader({
  title,
  description,
  actions,
  className,
  eyebrow,
}: {
  title: string;
  description?: string;
  actions?: React.ReactNode;
  className?: string;
  eyebrow?: string;
}) {
  return (
    <div className={cn("mb-7 flex flex-wrap items-end justify-between gap-5", className)}>
      <div>
        {eyebrow ? <p className="mb-1.5 text-[10px] font-semibold uppercase tracking-[0.16em] text-primary">{eyebrow}</p> : null}
        <h1 className="text-[1.8rem] font-semibold leading-tight tracking-[-0.04em] sm:text-3xl">{title}</h1>
        {description ? (
          <p className="mt-2 max-w-2xl text-sm leading-relaxed text-muted-foreground">{description}</p>
        ) : null}
      </div>
      {actions ? <div className="flex items-center gap-2 max-sm:w-full max-sm:[&>*]:flex-1">{actions}</div> : null}
    </div>
  );
}
