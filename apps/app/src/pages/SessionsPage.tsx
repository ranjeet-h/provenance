import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { Plus } from "lucide-react";
import { PageHeader } from "@/components/common/PageHeader";
import { EmptyState } from "@/components/common/EmptyState";
import { LoadingState } from "@/components/common/LoadingState";
import { ErrorState } from "@/components/common/ErrorState";
import { StatusBadge } from "@/components/common/StatusBadge";
import { Button } from "@/components/ui/button";
import { listSessions, asSessionsError } from "@/lib/sessions";

export function SessionsPage() {
  const sessions = useQuery({ queryKey: ["sessions"], queryFn: listSessions });

  return (
    <div>
      <PageHeader
        title="Sessions"
        description="One session per assignment. Everything stays on this device."
        actions={
          <Button asChild>
            <Link to="/sessions/new">
              <Plus className="h-4 w-4" aria-hidden />
              New session
            </Link>
          </Button>
        }
      />
      {sessions.isPending ? (
        <LoadingState label="Loading sessions…" />
      ) : sessions.isError ? (
        <ErrorState
          title="Could not load sessions"
          message={asSessionsError(sessions.error).message}
          onRetry={() => void sessions.refetch()}
        />
      ) : sessions.data.length === 0 ? (
        <EmptyState
          title="No sessions yet"
          description="Create your first assignment session to start collecting submissions."
          action={
            <Button asChild>
              <Link to="/sessions/new">Create session</Link>
            </Button>
          }
        />
      ) : (
        <ul className="grid gap-3 sm:grid-cols-2" aria-label="Sessions">
          {sessions.data.map((session) => (
            <li key={session.id}>
              <Link
                to="/sessions/$sessionId"
                params={{ sessionId: session.id }}
                className="block rounded-lg border bg-card p-4 text-card-foreground shadow-sm transition-colors hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              >
                <div className="flex items-start justify-between gap-2">
                  <h2 className="text-base font-medium">{session.name}</h2>
                  <StatusBadge status={session.status} />
                </div>
                {session.subject ? (
                  <p className="mt-1 text-sm text-muted-foreground">{session.subject}</p>
                ) : null}
              </Link>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
