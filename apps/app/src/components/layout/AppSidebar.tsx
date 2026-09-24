import { Link, useRouterState } from "@tanstack/react-router";
import { ShieldCheck } from "lucide-react";
import { cn } from "@/lib/utils";
import { NAV_ITEMS } from "./nav";

export function AppSidebar() {
  const pathname = useRouterState({ select: (s) => s.location.pathname });

  return (
    <nav aria-label="Primary" className="flex h-full flex-col gap-1 p-4">
      <div className="mb-4 flex items-center gap-2 px-2">
        <ShieldCheck className="h-5 w-5" aria-hidden />
        <span className="text-lg font-semibold tracking-tight">Provenance</span>
      </div>
      {NAV_ITEMS.map((item) => {
        const active = item.end
          ? pathname === item.to
          : pathname === item.to || pathname.startsWith(`${item.to}/`);
        return (
          <Link
            key={item.to}
            to={item.to}
            aria-current={active ? "page" : undefined}
            className={cn(
              "flex items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors",
              "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
              active
                ? "bg-accent text-accent-foreground"
                : "text-muted-foreground hover:bg-accent/50 hover:text-accent-foreground",
            )}
          >
            <item.icon className="h-4 w-4" aria-hidden />
            {item.label}
          </Link>
        );
      })}
    </nav>
  );
}
