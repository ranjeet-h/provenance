import { PageHeader } from "@/components/common/PageHeader";
import { EmptyState } from "@/components/common/EmptyState";

export function ReferenceLibrariesPage() {
  return (
    <div>
      <PageHeader
        title="Reference Libraries"
        description="Read-only historical comparison material. Archiving and .plagpack arrive in Phases 17–18."
      />
      <EmptyState
        title="No reference libraries yet"
        description="Archive a completed session here later, or import a .plagpack shared by another teacher."
      />
    </div>
  );
}
