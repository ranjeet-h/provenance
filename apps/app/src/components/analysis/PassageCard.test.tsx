import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { PassageCard } from "./PassageCard";
import type { Passage } from "@/lib/analysis";

const BASE: Passage = {
  kind: "exact",
  identity: null,
  common_text: false,
  a_token_start: 0,
  a_token_end: 4,
  b_token_start: 0,
  b_token_end: 4,
  a_char_start: 0,
  a_char_end: 26,
  b_char_start: 0,
  b_char_end: 26,
  tokens: 4,
};

describe("PassageCard", () => {
  it("labels exact evidence without scary words", () => {
    render(
      <PassageCard
        index={0}
        passage={BASE}
        aName="Amit"
        bName="Priya"
        aText="The quick brown fox jumps."
        bText="The quick brown fox jumps."
      />,
    );
    expect(screen.getByText("Exact match")).toBeInTheDocument();
    expect(screen.queryByText(/copied|plagiar/i)).not.toBeInTheDocument();
    expect(screen.getAllByText("The quick brown fox jumps.")).toHaveLength(2);
  });

  it("labels modified evidence with its identity", () => {
    render(
      <PassageCard
        index={1}
        passage={{ ...BASE, kind: "modified", identity: 0.72 }}
        aName="Amit"
        bName="Priya"
        aText="The quick brown fox jumps."
        bText="The quick brown fox leaps."
      />,
    );
    expect(screen.getByText("Modified match · 72% alike")).toBeInTheDocument();
    expect(screen.getByText(/passage 2 · 4 matching words/i)).toBeInTheDocument();
  });

  it("labels modified evidence without identity gracefully", () => {
    render(
      <PassageCard
        index={0}
        passage={{ ...BASE, kind: "modified", identity: null }}
        aName="A"
        bName="B"
        aText="Some edited words here today."
        bText="Some altered words here today."
      />,
    );
    expect(screen.getByText("Modified match")).toBeInTheDocument();
  });

  it("flags common session text without removing it from evidence", () => {
    render(
      <PassageCard
        index={0}
        passage={{ ...BASE, common_text: true }}
        aName="Amit"
        bName="Priya"
        aText="The quick brown fox jumps."
        bText="The quick brown fox jumps."
      />,
    );
    // Still rendered as scored evidence, with an explanatory badge.
    expect(screen.getByText("Exact match")).toBeInTheDocument();
    expect(screen.getByText("Shared across submissions")).toBeInTheDocument();
    expect(screen.getAllByText("The quick brown fox jumps.")).toHaveLength(2);
  });

  it("omits the shared badge for uncommon passages", () => {
    render(
      <PassageCard
        index={0}
        passage={BASE}
        aName="Amit"
        bName="Priya"
        aText="The quick brown fox jumps."
        bText="The quick brown fox jumps."
      />,
    );
    expect(screen.queryByText("Shared across submissions")).not.toBeInTheDocument();
  });

  it("highlights the exact text after non-ASCII lead-in", () => {
    // Byte offsets from Rust must not shift the highlight past accented,
    // emoji, or CJK text. Offsets computed independently via TextEncoder.
    const enc = new TextEncoder();
    const cases: Array<[string, string, string]> = [
      ["naïve intro. Copied words here now.", "naïve intro. ", "Copied words here"],
      ["😀 grin. Copied words here now.", "😀 grin. ", "Copied words here"],
      ["中文测试 Copied words here now.", "中文测试 ", "Copied words here"],
      ["éclair. Copied words here now.", "éclair. ", "Copied words here"],
    ];
    for (const [text, prefix, expected] of cases) {
      const start = enc.encode(prefix).length;
      const end = start + enc.encode(expected).length;
      const { unmount } = render(
        <PassageCard
          index={0}
          passage={{ ...BASE, a_char_start: start, a_char_end: end, b_char_start: start, b_char_end: end }}
          aName="Amit"
          bName="Priya"
          aText={text}
          bText={text}
        />,
      );
      const marks = screen.getAllByText(expected);
      // Both sides highlight exactly the expected text (plus context text).
      expect(marks).toHaveLength(2);
      for (const mark of marks) {
        expect(mark.tagName).toBe("MARK");
      }
      unmount();
    }
  });
});
