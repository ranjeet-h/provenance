import { Link } from "@tanstack/react-router";
import { Button } from "@/components/ui/button";
import { ErrorState } from "@/components/common/ErrorState";

export function NotFoundPage() {
  return (
    <div>
      <ErrorState
        title="Page not found"
        message="The page you are looking for does not exist."
      />
      <div className="mt-4 flex justify-center">
        <Button asChild variant="outline">
          <Link to="/">Back to dashboard</Link>
        </Button>
      </div>
    </div>
  );
}
