import { Link, useRouterState } from "@tanstack/react-router";
import { HardDrive, ShieldCheck } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { cn } from "@/lib/utils";
import { NAV_ITEMS } from "./nav";

export function AppSidebar() {
  const pathname = useRouterState({ select: (s) => s.location.pathname });

  return (
    <nav aria-label="Primary" className="flex min-h-full flex-col px-3 py-5">
      <Link
        to="/"
        className="mb-8 flex items-center gap-3 rounded-md px-2 py-1.5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        <span className="grid h-10 w-10 place-items-center rounded-md bg-primary text-primary-foreground shadow-md shadow-primary/20">
          <ShieldCheck className="h-5 w-5" aria-hidden />
        </span>
        <span className="min-w-0">
          <span className="block text-base font-semibold tracking-tight">
            Provenance
          </span>
          <span className="mt-0.5 block text-[10px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
            Evidence workspace
          </span>
        </span>
      </Link>
      <p className="mb-2 px-3 text-[10px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
        Workspace
      </p>
      <div className="flex flex-col gap-1">
        {NAV_ITEMS.map((item) => {
          const active = item.end
            ? pathname === item.to
            : pathname === item.to || pathname.startsWith(`${item.to}/`);
          return (
            <Button
              key={item.to}
              asChild
              variant={active ? "secondary" : "ghost"}
              className={cn(
                "relative h-10 w-full justify-start px-3 text-sm font-medium",
                active
                  ? "bg-primary/[0.09] text-primary hover:bg-primary/[0.09]"
                  : "text-muted-foreground",
              )}
            >
              <Link to={item.to} aria-current={active ? "page" : undefined}>
                {active ? (
                  <span
                    className="absolute inset-y-2 left-0 w-0.5 rounded-full bg-primary"
                    aria-hidden
                  />
                ) : null}
                <item.icon className="h-[18px] w-[18px] shrink-0" aria-hidden />
                {item.label}
              </Link>
            </Button>
          );
        })}
      </div>
      <div className="mt-auto border-t border-sidebar-border pt-4">
        <Card className="gap-0 border-sidebar-border bg-white/70 p-3.5 shadow-sm">
          <span className="flex items-center gap-2 text-xs font-semibold text-foreground">
            <HardDrive className="h-4 w-4 text-primary" aria-hidden />
            Stored on this device
          </span>
          <p className="mt-1.5 pl-6 text-xs leading-relaxed text-muted-foreground">
            Your assignment text stays in this local workspace.
          </p>
        </Card>
      </div>
    </nav>
  );
}
