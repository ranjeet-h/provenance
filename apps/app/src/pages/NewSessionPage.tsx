import * as React from "react";
import { useNavigate } from "@tanstack/react-router";
import { useQueryClient } from "@tanstack/react-query";
import { useForm } from "react-hook-form";
import { z } from "zod";
import { zodResolver } from "@hookform/resolvers/zod";
import { PageHeader } from "@/components/common/PageHeader";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
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
        subject: values.subject !== undefined && values.subject !== "" ? values.subject : null,
      });
      await queryClient.invalidateQueries({ queryKey: ["sessions"] });
      await navigate({ to: "/sessions/$sessionId", params: { sessionId: session.id } });
    } catch (err) {
      setSubmitError((err as SessionsError).message ?? "Could not create the session.");
    }
  }

  return (
    <div>
      <PageHeader
        title="New session"
        description="Sessions and students are stored locally in SQLite."
      />
      <form
        aria-label="New session"
        className="max-w-md space-y-4"
        onSubmit={(e) => {
          void handleSubmit(onSubmit)(e);
        }}
      >
        <div className="space-y-1">
          <label htmlFor="session-name" className="text-sm font-medium">
            Assignment name
          </label>
          <Input
            id="session-name"
            placeholder="Biology Assignment 1"
            aria-invalid={errors.name ? true : undefined}
            className={cn(errors.name && "border-destructive")}
            {...register("name")}
          />
          {errors.name ? <p role="alert" className="text-sm text-destructive">{errors.name.message}</p> : null}
        </div>
        <div className="space-y-1">
          <label htmlFor="session-subject" className="text-sm font-medium">
            Subject <span className="text-muted-foreground">(optional)</span>
          </label>
          <Input
            id="session-subject"
            placeholder="Biology"
            aria-invalid={errors.subject ? true : undefined}
            className={cn(errors.subject && "border-destructive")}
            {...register("subject")}
          />
          {errors.subject ? (
            <p role="alert" className="text-sm text-destructive">{errors.subject.message}</p>
          ) : null}
        </div>
        {submitError ? (
          <p role="alert" className="text-sm text-destructive">{submitError}</p>
        ) : null}
        <Button type="submit" disabled={isSubmitting}>
          {isSubmitting ? "Creating…" : "Create session"}
        </Button>
      </form>
    </div>
  );
}
