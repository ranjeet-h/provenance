/* eslint-disable react-refresh/only-export-components */
import {
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
  RouterProvider,
} from "@tanstack/react-router";
import { AppShell } from "./components/layout/AppShell";
import { DashboardPage } from "./pages/DashboardPage";
import { SessionsPage } from "./pages/SessionsPage";
import { NewSessionPage } from "./pages/NewSessionPage";
import { SessionDetailPage } from "./pages/SessionDetailPage";
import { ReferenceLibrariesPage } from "./pages/ReferenceLibrariesPage";
import { SettingsPage } from "./pages/SettingsPage";
import { NotFoundPage } from "./pages/NotFoundPage";

const rootRoute = createRootRoute({
  component: AppShell,
  notFoundComponent: NotFoundPage,
});

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: DashboardPage,
});

const sessionsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/sessions",
  component: SessionsPage,
});

const newSessionRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/sessions/new",
  component: NewSessionPage,
});

const sessionDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/sessions/$sessionId",
  component: SessionDetailPage,
});

const librariesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/reference-libraries",
  component: ReferenceLibrariesPage,
});

const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/settings",
  component: SettingsPage,
});

const routeTree = rootRoute.addChildren([
  indexRoute,
  sessionsRoute,
  newSessionRoute,
  sessionDetailRoute,
  librariesRoute,
  settingsRoute,
]);

export const router = createRouter({ routeTree });

/** Isolated router for tests: fresh history per render, no shared state. */
export function createAppRouter(initialPath = "/") {
  const history = createMemoryHistory({ initialEntries: [initialPath] });
  return createRouter({ routeTree, history });
}

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

export function RootRouter() {
  return <RouterProvider router={router} />;
}
