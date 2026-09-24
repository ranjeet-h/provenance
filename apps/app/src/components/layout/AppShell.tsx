import * as React from "react";
import { Outlet } from "@tanstack/react-router";
import { AppSidebar } from "./AppSidebar";
import { TopBar } from "./TopBar";
import { Sheet, SheetContent, SheetTitle } from "@/components/ui/sheet";

export function AppShell() {
  const [mobileOpen, setMobileOpen] = React.useState(false);

  return (
    <div className="flex min-h-screen bg-background text-foreground">
      <aside className="hidden w-64 shrink-0 border-r md:block" aria-label="Sidebar">
        <div className="sticky top-0 h-screen overflow-y-auto">
          <AppSidebar />
        </div>
      </aside>

      <Sheet open={mobileOpen} onOpenChange={setMobileOpen}>
        <SheetContent side="left" className="p-0">
          <SheetTitle className="sr-only">Navigation</SheetTitle>
          <div onClick={() => setMobileOpen(false)}>
            <AppSidebar />
          </div>
        </SheetContent>
      </Sheet>

      <div className="flex min-w-0 flex-1 flex-col">
        <TopBar onMenuClick={() => setMobileOpen(true)} />
        <main className="mx-auto w-full max-w-5xl flex-1 p-4 sm:p-6">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
