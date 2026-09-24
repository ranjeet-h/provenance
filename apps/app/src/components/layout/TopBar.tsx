import { Menu } from "lucide-react";
import { Button } from "@/components/ui/button";

export function TopBar({ onMenuClick }: { onMenuClick: () => void }) {
  return (
    <header className="flex h-14 items-center gap-2 border-b bg-background px-4">
      <Button
        variant="ghost"
        size="icon"
        className="md:hidden"
        onClick={onMenuClick}
        aria-label="Open navigation"
      >
        <Menu className="h-5 w-5" aria-hidden />
      </Button>
      <span className="text-sm font-semibold md:hidden">Provenance</span>
      <div className="ml-auto flex items-center gap-2">
        <span className="hidden text-xs text-muted-foreground sm:inline">
          Local-first · Offline ready
        </span>
      </div>
    </header>
  );
}
