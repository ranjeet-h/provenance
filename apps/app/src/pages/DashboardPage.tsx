import { Link } from "@tanstack/react-router";
import { Users, Library, Settings, ArrowRight } from "lucide-react";
import { PageHeader } from "@/components/common/PageHeader";

const CARDS = [
  {
    to: "/sessions",
    title: "Sessions",
    description: "Create an assignment session, add students, collect submissions.",
    icon: Users,
  },
  {
    to: "/reference-libraries",
    title: "Reference Libraries",
    description: "Archive past sessions and compare against historical work.",
    icon: Library,
  },
  {
    to: "/settings",
    title: "Settings",
    description: "Storage and app preferences.",
    icon: Settings,
  },
] as const;

export function DashboardPage() {
  return (
    <div>
      <PageHeader
        title="Provenance"
        description="Local-first plagiarism detection for English assignments."
      />
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {CARDS.map((card) => (
          <Link
            key={card.to}
            to={card.to}
            className="group rounded-lg border bg-card p-5 text-card-foreground shadow-sm transition-colors hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <card.icon className="mb-3 h-6 w-6" aria-hidden />
            <h2 className="text-base font-medium">{card.title}</h2>
            <p className="mt-1 text-sm text-muted-foreground">{card.description}</p>
            <span className="mt-3 inline-flex items-center gap-1 text-sm font-medium text-primary">
              Open
              <ArrowRight className="h-4 w-4 transition-transform group-hover:translate-x-0.5" aria-hidden />
            </span>
          </Link>
        ))}
      </div>
    </div>
  );
}
