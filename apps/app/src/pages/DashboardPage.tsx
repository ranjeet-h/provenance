import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import {
  ArrowRight,
  ArrowUpRight,
  Library,
  Plus,
  Settings,
  ShieldCheck,
  Users,
} from "lucide-react";
import { PageHeader } from "@/components/common/PageHeader";
import { StatusBadge } from "@/components/common/StatusBadge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { listReferenceLibraries } from "@/lib/referenceLibraries";
import { listSessions } from "@/lib/sessions";

const WORKSPACE_LINKS = [
  {
    to: "/sessions",
    title: "Sessions",
    description: "Organize assignments, students, and digital submissions.",
    icon: Users,
    metric: (count: number) =>
      `${count} ${count === 1 ? "assignment" : "assignments"}`,
  },
  {
    to: "/reference-libraries",
    title: "Reference Libraries",
    description: "Review local archives as a separate comparison corpus.",
    icon: Library,
    metric: (count: number) =>
      `${count} ${count === 1 ? "archive" : "archives"}`,
  },
  {
    to: "/settings",
    title: "Settings",
    description: "Review local storage and supported document types.",
    icon: Settings,
    metric: "Local preferences",
  },
] as const;

export function DashboardPage() {
  const sessionsQuery = useQuery({
    queryKey: ["sessions"],
    queryFn: listSessions,
    retry: false,
  });
  const librariesQuery = useQuery({
    queryKey: ["reference-libraries"],
    queryFn: listReferenceLibraries,
    retry: false,
  });
  const sessions = sessionsQuery.data ?? [];
  const libraries = librariesQuery.data ?? [];
  const recentSessions = [...sessions]
    .sort((left, right) => right.updated_at.localeCompare(left.updated_at))
    .slice(0, 4);

  return (
    <div className="space-y-8">
      <Card
        role="region"
        aria-label="Workspace summary"
        className="relative isolate overflow-hidden gap-0 border-primary/10 bg-gradient-to-br from-white via-white to-indigo-50/80 p-6 shadow-md sm:p-9"
      >
        <div
          className="pointer-events-none absolute -right-20 -top-28 -z-10 h-72 w-72 rounded-full bg-primary/[0.055] blur-2xl"
          aria-hidden
        />
        <div
          className="pointer-events-none absolute bottom-0 right-0 -z-10 h-40 w-2/5 bg-[radial-gradient(ellipse_at_bottom_right,oklch(0.83_0.08_257_/_0.13),transparent_70%)]"
          aria-hidden
        />
        <div className="flex flex-wrap items-end justify-between gap-6">
          <div className="max-w-2xl">
            <span className="inline-flex items-center gap-2 rounded-full border border-primary/15 bg-white/80 px-3 py-1.5 text-[10px] font-semibold uppercase tracking-[0.15em] text-primary shadow-sm">
              <ShieldCheck className="h-3.5 w-3.5" aria-hidden />
              Local evidence workspace
            </span>
            <PageHeader
              className="mb-0 mt-4"
              title="Provenance"
              description="Review matching text in the sources you choose. Your assignment workspace stays on this device."
            />
          </div>
          <Button asChild className="shadow-md shadow-primary/15">
            <Link to="/sessions/new">
              <Plus className="h-4 w-4" aria-hidden />
              New session
            </Link>
          </Button>
        </div>

        <div className="mt-8 grid gap-3 sm:grid-cols-3">
          <Card className="gap-1 border-white/80 bg-white/75 px-4 py-3.5 shadow-sm backdrop-blur">
            <p className="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
              Assignments
            </p>
            <p className="mt-1 text-2xl font-semibold tracking-tight">
              {sessions.length}
            </p>
          </Card>
          <Card className="gap-1 border-white/80 bg-white/75 px-4 py-3.5 shadow-sm backdrop-blur">
            <p className="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
              Needs review
            </p>
            <p className="mt-1 text-2xl font-semibold tracking-tight">
              {sessions.filter((session) => session.status !== "locked").length}
            </p>
          </Card>
          <Card className="gap-1 border-white/80 bg-white/75 px-4 py-3.5 shadow-sm backdrop-blur">
            <p className="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
              Reference archives
            </p>
            <p className="mt-1 text-2xl font-semibold tracking-tight">
              {libraries.length}
            </p>
          </Card>
        </div>
      </Card>

      <section aria-labelledby="workspace-links-title">
        <div className="mb-4 flex items-end justify-between gap-4">
          <div>
            <p className="text-[10px] font-semibold uppercase tracking-[0.15em] text-primary">
              Workspace
            </p>
            <h2
              id="workspace-links-title"
              className="mt-1 text-lg font-semibold tracking-tight"
            >
              Your tools
            </h2>
          </div>
        </div>
        <div className="grid gap-4 sm:grid-cols-2">
          {WORKSPACE_LINKS.map((item) => {
            const metric =
              typeof item.metric === "string"
                ? item.metric
                : item.metric(
                    item.title === "Sessions"
                      ? sessions.length
                      : libraries.length,
                  );
            return (
              <Card
                key={item.to}
                className="group gap-0 overflow-hidden p-0 transition-colors hover:border-primary/30"
              >
                <Link
                  to={item.to}
                  aria-label={`${item.title} ${item.description}`}
                  className="flex h-full flex-col p-5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring sm:p-6"
                >
                  <div className="flex items-start justify-between gap-4">
                    <span className="grid h-11 w-11 place-items-center rounded-md bg-primary/[0.08] text-primary transition-colors group-hover:bg-primary group-hover:text-primary-foreground">
                      <item.icon className="h-5 w-5" aria-hidden />
                    </span>
                    <span className="inline-flex items-center gap-1 text-xs font-medium text-muted-foreground">
                      {metric}
                      <ArrowUpRight
                        className="h-3.5 w-3.5 transition-transform group-hover:-translate-y-0.5 group-hover:translate-x-0.5"
                        aria-hidden
                      />
                    </span>
                  </div>
                  <h3 className="mt-5 text-base font-semibold tracking-tight">
                    {item.title}
                  </h3>
                  <p className="mt-1 max-w-md text-sm leading-relaxed text-muted-foreground">
                    {item.description}
                  </p>
                  <span className="mt-5 inline-flex items-center gap-2 text-sm font-semibold text-primary">
                    Open {item.title}
                    <ArrowRight
                      className="h-4 w-4 transition-transform group-hover:translate-x-1"
                      aria-hidden
                    />
                  </span>
                </Link>
              </Card>
            );
          })}
        </div>
      </section>

      <Card
        role="region"
        aria-labelledby="recent-sessions-title"
        className="gap-0 p-5 sm:p-6"
      >
        <div className="mb-4 flex flex-wrap items-end justify-between gap-3">
          <div>
            <p className="text-[10px] font-semibold uppercase tracking-[0.15em] text-primary">
              Recently updated
            </p>
            <h2
              id="recent-sessions-title"
              className="mt-1 text-lg font-semibold tracking-tight"
            >
              Recent sessions
            </h2>
          </div>
          <Button asChild variant="ghost" size="sm">
            <Link to="/sessions">
              All sessions <ArrowRight className="ml-1 h-4 w-4" aria-hidden />
            </Link>
          </Button>
        </div>
        {recentSessions.length === 0 ? (
          <div className="flex flex-col items-start gap-3 rounded-md border border-dashed border-border bg-muted/25 px-4 py-5 sm:flex-row sm:items-center sm:justify-between">
            <div>
              <p className="text-sm font-medium">
                Your workspace is ready for its first assignment.
              </p>
              <p className="mt-1 text-sm text-muted-foreground">
                Create a session, then add student submissions.
              </p>
            </div>
            <Button asChild variant="outline" size="sm">
              <Link to="/sessions/new">Create a session</Link>
            </Button>
          </div>
        ) : (
          <ul
            className="divide-y divide-border/70"
            aria-label="Recent sessions"
          >
            {recentSessions.map((session) => (
              <li key={session.id}>
                <Link
                  to="/sessions/$sessionId"
                  params={{ sessionId: session.id }}
                  className="flex flex-wrap items-center justify-between gap-3 rounded-lg py-3.5 transition-colors hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                >
                  <span className="min-w-0">
                    <span className="block truncate text-sm font-semibold">
                      {session.name}
                    </span>
                    <span className="mt-0.5 block text-xs text-muted-foreground">
                      {session.subject ?? "No subject set"}
                    </span>
                  </span>
                  <span className="flex items-center gap-3">
                    <StatusBadge status={session.status} />
                    <ArrowRight
                      className="h-4 w-4 text-muted-foreground"
                      aria-hidden
                    />
                  </span>
                </Link>
              </li>
            ))}
          </ul>
        )}
      </Card>
    </div>
  );
}
