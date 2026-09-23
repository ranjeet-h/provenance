import { Menu, WifiOff } from "lucide-react";
import { useRouterState } from "@tanstack/react-router";
import { Button } from "@/components/ui/button";
import { NAV_ITEMS } from "./nav";

export function TopBar({ onMenuClick }: { onMenuClick: () => void }) {
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const current = NAV_ITEMS.find((item) => item.end
    ? pathname === item.to
    : pathname === item.to || pathname.startsWith(`${item.to}/`));
  const pageTitle = pathname === "/sessions/new"
    ? "New session"
    : pathname.startsWith("/sessions/")
      ? "Assignment workspace"
      : current?.label ?? "Workspace";

  return (
    <header className="sticky top-0 z-20 flex h-16 items-center gap-3 border-b border-border/70 bg-background/85 px-4 backdrop-blur-xl sm:px-7 lg:px-10">
      <Button
        variant="ghost"
        size="icon"
        className="md:hidden"
        onClick={onMenuClick}
        aria-label="Open navigation"
      >
        <Menu className="h-5 w-5" aria-hidden />
      </Button>
      <div className="min-w-0 md:ml-1">
        <p className="hidden text-[10px] font-semibold uppercase tracking-[0.15em] text-muted-foreground md:block">Provenance</p>
        <div className="flex items-center gap-1.5 text-sm font-semibold md:mt-0.5">
          <span className="truncate">{pageTitle}</span>
        </div>
      </div>
      <div className="ml-auto flex items-center gap-2">
        <span className="inline-flex items-center gap-2 rounded-full border border-emerald-900/10 bg-emerald-50/80 px-3 py-1.5 text-xs font-medium text-emerald-900">
          <WifiOff className="h-3.5 w-3.5" aria-hidden />
          <span className="hidden sm:inline">Offline ready</span>
          <span className="sm:hidden">Local</span>
        </span>
      </div>
    </header>
  );
}
