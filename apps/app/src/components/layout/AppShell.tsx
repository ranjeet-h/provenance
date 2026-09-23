import * as React from "react";
import { Outlet } from "@tanstack/react-router";
import { AppSidebar } from "./AppSidebar";
import { TopBar } from "./TopBar";
import { Sheet, SheetContent, SheetTitle } from "@/components/ui/sheet";

export function AppShell() {
  const [mobileOpen, setMobileOpen] = React.useState(false);

  return (
    <div className="min-h-screen bg-[radial-gradient(ellipse_at_top_left,oklch(0.94_0.035_258_/_0.48),transparent_36rem)] text-foreground">
      <aside className="fixed inset-y-0 left-0 z-30 hidden w-[17rem] border-r border-sidebar-border bg-sidebar md:block" aria-label="Sidebar">
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

      <div className="flex min-h-screen min-w-0 flex-col md:pl-[17rem]">
        <TopBar onMenuClick={() => setMobileOpen(true)} />
        <main className="mx-auto w-full max-w-[1360px] flex-1 px-4 py-6 sm:px-7 sm:py-8 lg:px-10 lg:py-10">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
