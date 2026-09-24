import * as React from "react";
import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { ArrowRight, Plus, Search, Users } from "lucide-react";
import { PageHeader } from "@/components/common/PageHeader";
import { EmptyState } from "@/components/common/EmptyState";
import { LoadingState } from "@/components/common/LoadingState";
import { ErrorState } from "@/components/common/ErrorState";
import { StatusBadge } from "@/components/common/StatusBadge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { listSessions, asSessionsError } from "@/lib/sessions";

const updatedAt = new Intl.DateTimeFormat(undefined, {
  month: "short",
  day: "numeric",
  year: "numeric",
});

export function SessionsPage() {
  const [search, setSearch] = React.useState("");
  const sessions = useQuery({ queryKey: ["sessions"], queryFn: listSessions });
  const allSessions = sessions.data ?? [];
  const visibleSessions = allSessions.filter((session) =>
    `${session.name} ${session.subject ?? ""}`
      .toLocaleLowerCase()
      .includes(search.trim().toLocaleLowerCase()),
  );
  const draftCount = allSessions.filter(
    (session) => session.status === "draft",
  ).length;
  const analyzedCount = allSessions.filter(
    (session) => session.status === "analyzed",
  ).length;
  const lockedCount = allSessions.filter(
    (session) => session.status === "locked",
  ).length;

  return (
    <div>
      <PageHeader
        eyebrow="Assignment management"
        title="Sessions"
        description="Keep each assignment, its student submissions, and its review in one local workspace."
        actions={
          <Button asChild>
            <Link to="/sessions/new">
              <Plus className="h-4 w-4" aria-hidden />
              New session
            </Link>
          </Button>
        }
      />
      <div className="mb-5 grid grid-cols-3 gap-2 sm:gap-3">
        {[
          {
            label: "All sessions",
            value: allSessions.length,
            tone: "text-foreground",
          },
          {
            label: "In progress",
            value: draftCount + analyzedCount,
            tone: "text-primary",
          },
          { label: "Locked", value: lockedCount, tone: "text-emerald-800" },
        ].map((stat) => (
          <Card
            key={stat.label}
            className="gap-1 px-3.5 py-3 shadow-sm sm:px-4"
          >
            <p className="text-[10px] font-semibold uppercase tracking-[0.12em] text-muted-foreground sm:text-xs">
              {stat.label}
            </p>
            <p
              className={`mt-1 text-xl font-semibold tracking-tight sm:text-2xl ${stat.tone}`}
            >
              {stat.value}
            </p>
          </Card>
        ))}
      </div>
      <Card className="mb-4 flex-row items-center justify-between gap-3 p-3 shadow-sm sm:px-4">
        <div>
          <h2 className="text-sm font-semibold">Your assignments</h2>
          <p className="mt-0.5 text-xs text-muted-foreground">
            {visibleSessions.length} shown · {allSessions.length} total
          </p>
        </div>
        <div className="relative block w-full sm:max-w-xs">
          <Label htmlFor="session-search" className="sr-only">
            Search sessions
          </Label>
          <Search
            className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground"
            aria-hidden
          />
          <Input
            id="session-search"
            aria-label="Search sessions"
            placeholder="Search by name or subject"
            value={search}
            onChange={(event) => setSearch(event.currentTarget.value)}
            className="pl-9"
          />
        </div>
      </Card>
      {sessions.isPending ? (
        <LoadingState label="Loading sessions…" />
      ) : sessions.isError ? (
        <ErrorState
          title="Could not load sessions"
          message={asSessionsError(sessions.error).message}
          onRetry={() => void sessions.refetch()}
        />
      ) : allSessions.length === 0 ? (
        <EmptyState
          title="No sessions yet"
          description="Create your first assignment session to start collecting submissions."
          action={
            <Button asChild>
              <Link to="/sessions/new">Create session</Link>
            </Button>
          }
        />
      ) : visibleSessions.length === 0 ? (
        <EmptyState
          icon={<Search className="h-7 w-7" />}
          title="No matching sessions"
          description={`Nothing matches “${search.trim()}”. Try a different assignment name or subject.`}
          action={
            <Button variant="outline" onClick={() => setSearch("")}>
              Clear search
            </Button>
          }
        />
      ) : (
        <ul
          className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3"
          aria-label="Sessions"
        >
          {visibleSessions.map((session) => (
            <li key={session.id}>
              <Card className="group h-full gap-0 overflow-hidden p-0 transition-colors hover:border-primary/30">
                <Link
                  to="/sessions/$sessionId"
                  params={{ sessionId: session.id }}
                  className="flex h-full min-h-40 flex-col p-4.5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring sm:p-5"
                >
                  <div className="flex items-start justify-between gap-3">
                    <span className="grid h-10 w-10 shrink-0 place-items-center rounded-md bg-primary/[0.075] text-primary transition-colors group-hover:bg-primary group-hover:text-primary-foreground">
                      <Users className="h-[18px] w-[18px]" aria-hidden />
                    </span>
                    <StatusBadge status={session.status} />
                  </div>
                  <span className="mt-4 line-clamp-2 text-base font-semibold tracking-tight">
                    {session.name}
                  </span>
                  <span className="mt-1 text-sm text-muted-foreground">
                    {session.subject ?? "No subject set"}
                  </span>
                  <span className="mt-auto flex items-center justify-between gap-2 pt-5 text-xs text-muted-foreground">
                    Updated {updatedAt.format(new Date(session.updated_at))}
                    <ArrowRight
                      className="h-4 w-4 shrink-0 text-primary transition-transform group-hover:translate-x-1"
                      aria-hidden
                    />
                  </span>
                </Link>
              </Card>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
