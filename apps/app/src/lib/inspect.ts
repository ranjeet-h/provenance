import { z } from "zod";
import { getInvokeImpl } from "./tauri";
import { asSessionsError } from "./sessions";

export const InspectedTokenSchema = z.object({
  normalized: z.string(),
  original_start: z.number(),
  original_end: z.number(),
  ordinal: z.number(),
});

export type InspectedToken = z.infer<typeof InspectedTokenSchema>;

export const InspectionSchema = z.object({
  original: z.string(),
  normalization_version: z.number(),
  tokens: z.array(InspectedTokenSchema),
  normalized_text: z.string(),
  sentences: z.array(
    z.object({
      token_start: z.number(),
      token_end: z.number(),
      char_start: z.number(),
      char_end: z.number(),
    }),
  ),
  paragraph_starts: z.array(z.number()),
  meaningful: z.array(z.boolean()),
});

export type Inspection = z.infer<typeof InspectionSchema>;

export async function inspectText(text: string): Promise<Inspection> {
  let raw: unknown;
  try {
    raw = await getInvokeImpl()("inspect_text", { text });
  } catch (err) {
    throw asSessionsError(err);
  }
  const parsed = InspectionSchema.safeParse(raw);
  if (!parsed.success) {
    throw { code: "protocol", message: "Unexpected response from the app core." };
  }
  return parsed.data;
}
