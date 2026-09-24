import { Badge } from "@/components/ui/badge";
import { sliceByByteOffsets, utf8ByteToUtf16Index } from "@/lib/offsets";
import type { Passage } from "@/lib/analysis";

export function HighlightedText({
  text,
  start,
  end,
  context = 80,
}: {
  text: string;
  /**
   * Rust UTF-8 byte offsets (all passage `*_char_start/end` values).
   * Converted to JS indices internally — never sliced directly.
   */
  start: number;
  end: number;
  context?: number;
}) {
  const startIdx = utf8ByteToUtf16Index(text, start) ?? 0;
  const endIdx = utf8ByteToUtf16Index(text, end) ?? text.length;
  const before = text.slice(Math.max(0, startIdx - context), startIdx);
  const match = sliceByByteOffsets(text, start, end);
  const after = text.slice(endIdx, endIdx + context);
  return (
    <p className="whitespace-pre-wrap text-sm leading-relaxed">
      {startIdx - context > 0 ? <span aria-hidden>…</span> : null}
      <span className="text-muted-foreground">{before}</span>
      <mark className="rounded-sm bg-yellow-200 px-0.5 dark:bg-yellow-900">{match}</mark>
      <span className="text-muted-foreground">{after}</span>
      {endIdx + context < text.length ? <span aria-hidden>…</span> : null}
    </p>
  );
}

function kindLabel(passage: Passage): string {
  if (passage.kind === "modified") {
    const pct =
      passage.identity === null || passage.identity === undefined
        ? null
        : Math.round(passage.identity * 100);
    return pct === null ? "Modified match" : `Modified match · ${pct}% alike`;
  }
  return "Exact match";
}

export function PassageCard({
  index,
  passage,
  aName,
  bName,
  aText,
  bText,
}: {
  index: number;
  passage: Passage;
  aName: string;
  bName: string;
  aText: string;
  bText: string;
}) {
  return (
    <li className="rounded-lg border p-4">
      <p className="flex flex-wrap items-center gap-2 text-xs font-medium text-muted-foreground">
        <span>
          Passage {index + 1} · {passage.tokens} matching words
        </span>
        <Badge variant={passage.kind === "exact" ? "default" : "secondary"}>
          {kindLabel(passage)}
        </Badge>
        {passage.common_text ? (
          <Badge
            variant="outline"
            title="Shared across many submissions in this session. Still fully counted in the score."
          >
            Shared across submissions
          </Badge>
        ) : null}
      </p>
      <div className="mt-2 grid gap-4 md:grid-cols-2">
        <div>
          <p className="mb-1 text-xs font-medium">{aName}</p>
          <HighlightedText
            text={aText}
            start={passage.a_char_start}
            end={passage.a_char_end}
          />
        </div>
        <div>
          <p className="mb-1 text-xs font-medium">{bName}</p>
          <HighlightedText
            text={bText}
            start={passage.b_char_start}
            end={passage.b_char_end}
          />
        </div>
      </div>
    </li>
  );
}
