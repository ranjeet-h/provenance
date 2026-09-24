import * as React from "react";
import { Link, useNavigate } from "@tanstack/react-router";
import { useQueryClient } from "@tanstack/react-query";
import { useForm } from "react-hook-form";
import { z } from "zod";
import { zodResolver } from "@hookform/resolvers/zod";
import { PageHeader } from "@/components/common/PageHeader";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { createSession, type SessionsError } from "@/lib/sessions";
import { cn } from "@/lib/utils";

const formSchema = z.object({
  name: z
    .string()
    .trim()
    .min(1, "Enter a session name.")
    .max(200, "Keep the name under 200 characters."),
  subject: z
    .string()
    .trim()
    .max(200, "Keep the subject under 200 characters.")
    .optional()
    .or(z.literal("")),
});

type FormValues = z.infer<typeof formSchema>;

export function NewSessionPage() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [submitError, setSubmitError] = React.useState<string | null>(null);
  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
  } = useForm<FormValues>({
    resolver: zodResolver(formSchema),
    defaultValues: { name: "", subject: "" },
  });

  async function onSubmit(values: FormValues): Promise<void> {
    setSubmitError(null);
    try {
      const session = await createSession({
        name: values.name,
        subject:
          values.subject !== undefined && values.subject !== ""
            ? values.subject
            : null,
      });
      await queryClient.invalidateQueries({ queryKey: ["sessions"] });
      await navigate({
        to: "/sessions/$sessionId",
        params: { sessionId: session.id },
      });
    } catch (err) {
      setSubmitError(
        (err as SessionsError).message ?? "Could not create the session.",
      );
    }
  }

  return (
    <div>
      <PageHeader
        eyebrow="Assignment setup · Step 1 of 1"
        title="New session"
        description="Start an assignment workspace. You can add students, paste text, or import supported digital documents next."
      />
      <Card className="max-w-2xl p-0">
        <form
          aria-label="New session"
          className="p-5 sm:p-7"
          onSubmit={(e) => {
            void handleSubmit(onSubmit)(e);
          }}
        >
          <div className="space-y-2">
            <Label htmlFor="session-name">Assignment name</Label>
            <Input
              id="session-name"
              placeholder="e.g. Biology · Cell structure"
              aria-invalid={errors.name ? true : undefined}
              className={cn(errors.name && "border-destructive")}
              {...register("name")}
            />
            {errors.name ? (
              <p role="alert" className="text-sm text-destructive">
                {errors.name.message}
              </p>
            ) : null}
            <p className="text-xs leading-relaxed text-muted-foreground">
              Choose a name you will recognize in your assignment list.
            </p>
          </div>
          <div className="mt-6 space-y-2">
            <Label htmlFor="session-subject">
              Subject <span className="text-muted-foreground">(optional)</span>
            </Label>
            <Input
              id="session-subject"
              placeholder="e.g. Biology"
              aria-invalid={errors.subject ? true : undefined}
              className={cn(errors.subject && "border-destructive")}
              {...register("subject")}
            />
            {errors.subject ? (
              <p role="alert" className="text-sm text-destructive">
                {errors.subject.message}
              </p>
            ) : null}
            <p className="text-xs leading-relaxed text-muted-foreground">
              Used as a label to help distinguish assignments.
            </p>
          </div>
          {submitError ? (
            <Alert variant="destructive" className="mt-6">
              <AlertDescription>{submitError}</AlertDescription>
            </Alert>
          ) : null}
          <div className="mt-7 flex flex-col-reverse gap-2 border-t border-border/70 pt-5 sm:flex-row sm:justify-end">
            <Button asChild variant="ghost">
              <Link to="/sessions">Cancel</Link>
            </Button>
            <Button type="submit" disabled={isSubmitting}>
              {isSubmitting ? "Creating…" : "Create session"}
            </Button>
          </div>
        </form>
      </Card>
    </div>
  );
}
