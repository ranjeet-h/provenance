import { render } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { createAppRouter } from "./router";
import type { InvokeFn } from "./lib/tauri";
import { MAX_UPLOAD_BYTES } from "./lib/sessions";
import type { Session, Student, Submission } from "./lib/sessions";

interface FakeDb {
  sessions: Session[];
  students: Student[];
  submissions: Submission[];
}

function now(): string {
  return new Date().toISOString();
}

/** In-memory fake of the Tauri session commands for unit tests. */
export function createFakeSessions(seed?: Partial<FakeDb>): {
  invoke: InvokeFn;
  db: FakeDb;
} {
  const db: FakeDb = {
    sessions: seed?.sessions ? [...seed.sessions] : [],
    students: seed?.students ? [...seed.students] : [],
    submissions: seed?.submissions ? [...seed.submissions] : [],
  };
  const savedAnalyses = new Map<string, unknown>();
  let counter = 1000;
  const nextId = (prefix: string): string => `${prefix}-${String((counter += 1))}`;

  const fail = (code: string, message: string): Promise<never> =>
    Promise.reject({ code, message });

  const invoke: InvokeFn = (cmd, args: Record<string, unknown> = {}) => {
    switch (cmd) {
      case "list_sessions":
        return Promise.resolve([...db.sessions]);
      case "create_session": {
        const name = String(args["name"] ?? "").trim();
        if (name === "") {
          return fail("validation", "session name must not be empty");
        }
        const subject = args["subject"];
        const session: Session = {
          id: nextId("sess"),
          name,
          subject: typeof subject === "string" && subject.trim() !== "" ? subject : null,
          status: "draft",
          assignment_prompt: null,
          excluded_reference_text: null,
          exclude_common_text: true,
          created_at: now(),
          updated_at: now(),
        };
        db.sessions.push(session);
        return Promise.resolve(session);
      }
      case "get_session": {
        const found = db.sessions.find((s) => s.id === args["id"]);
        return found ? Promise.resolve(found) : fail("not_found", "session not found");
      }
      case "update_session": {
        const found = db.sessions.find((s) => s.id === args["id"]);
        if (!found) {
          return fail("not_found", "session not found");
        }
        if ("name" in args && args["name"] !== undefined) {
          const name = String(args["name"]).trim();
          if (name === "") {
            return fail("validation", "session name must not be empty");
          }
          found.name = name;
        }
        if ("subject" in args) {
          const subject = args["subject"];
          found.subject =
            typeof subject === "string" && subject.trim() !== "" ? subject : null;
        }
        if ("assignmentPrompt" in args) {
          const prompt = args["assignmentPrompt"];
          found.assignment_prompt =
            typeof prompt === "string" && prompt.trim() !== "" ? prompt.trim() : null;
        }
        if ("excludedReferenceText" in args) {
          const reference = args["excludedReferenceText"];
          found.excluded_reference_text =
            typeof reference === "string" && reference.trim() !== "" ? reference.trim() : null;
        }
        if ("excludeCommonText" in args && typeof args["excludeCommonText"] === "boolean") {
          found.exclude_common_text = args["excludeCommonText"];
        }
        found.updated_at = now();
        return Promise.resolve({ ...found });
      }
      case "delete_session": {
        const index = db.sessions.findIndex((s) => s.id === args["id"]);
        if (index === -1) {
          return fail("not_found", "session not found");
        }
        const [removed] = db.sessions.splice(index, 1);
        db.students = db.students.filter((st) => st.session_id !== removed?.id);
        return Promise.resolve(null);
      }
      case "list_students":
        return Promise.resolve(db.students.filter((st) => st.session_id === args["sessionId"]));
      case "add_student": {
        const session = db.sessions.find((s) => s.id === args["sessionId"]);
        if (!session) {
          return fail("not_found", "session not found");
        }
        const displayName = String(args["displayName"] ?? "").trim();
        if (displayName === "") {
          return fail("validation", "student name must not be empty");
        }
        const student: Student = {
          id: nextId("stud"),
          session_id: session.id,
          display_name: displayName,
          created_at: now(),
        };
        db.students.push(student);
        return Promise.resolve(student);
      }
      case "remove_student": {
        const index = db.students.findIndex((st) => st.id === args["id"]);
        if (index === -1) {
          return fail("not_found", "student not found");
        }
        db.students.splice(index, 1);
        return Promise.resolve(null);
      }
      case "save_text_submission": {
        const student = db.students.find((st) => st.id === args["studentId"]);
        if (!student) {
          return fail("not_found", "student not found");
        }
        const text = String(args["text"] ?? "").replace(/\r\n/g, "\n").replace(/\r/g, "\n");
        if (text.trim() === "") {
          return fail("validation", "submission text must not be empty");
        }
        const filename = args["filename"];
        const sourceType = args["sourceType"] as Submission["source_type"];
        const existing = db.submissions.find((s) => s.student_id === student.id);
        if (existing) {
          existing.original_text = text;
          existing.source_type = sourceType;
          existing.source_filename = typeof filename === "string" ? filename : null;
          existing.content_sha256 = `fake-sha-${text.length}`;
          existing.updated_at = now();
          return Promise.resolve({ ...existing });
        }
        const submission: Submission = {
          id: nextId("sub"),
          student_id: student.id,
          session_id: student.session_id,
          source_type: sourceType,
          status: "draft",
          original_text: text,
          source_filename: typeof filename === "string" ? filename : null,
          content_sha256: `fake-sha-${text.length}`,
          created_at: now(),
          updated_at: now(),
        };
        db.submissions.push(submission);
        return Promise.resolve(submission);
      }
      case "get_submission": {
        const found = db.submissions.find((s) => s.student_id === args["studentId"]);
        return found
          ? Promise.resolve(found)
          : fail("not_found", "no submission for this student yet");
      }
      case "save_file_submission": {
        const student = db.students.find((st) => st.id === args["studentId"]);
        if (!student) {
          return fail("not_found", "student not found");
        }
        const filename = String(args["filename"] ?? "");
        const ext = filename.includes(".") ? (filename.split(".").pop() ?? "").toLowerCase() : "";
        if (!["txt", "md", "markdown", "text", "pdf", "docx"].includes(ext)) {
          return fail("validation", "unsupported file type (use TXT, Markdown, PDF, or DOCX)");
        }
        const bytes = (args["bytes"] as number[] | undefined) ?? [];
        // Mirror the real size cap: oversized uploads never reach storage.
        if (bytes.length > MAX_UPLOAD_BYTES) {
          return fail(
            "validation",
            `file is too large (max ${MAX_UPLOAD_BYTES / 1_000_000} MB)`,
          );
        }
        let text: string;
        try {
          text = new TextDecoder("utf-8", { fatal: true }).decode(new Uint8Array(bytes));
        } catch {
          return fail("validation", "file is not valid UTF-8 text");
        }
        if (text.trim() === "") {
          if (ext === "pdf") {
            return fail(
              "validation",
              "this PDF contains scanned pages; scanned/image-only PDFs are unsupported. Upload a digital PDF, DOCX, TXT, or Markdown file. Nothing was saved.",
            );
          }
          return fail("validation", "submission text must not be empty");
        }
        const source =
          ext === "pdf"
            ? "pdf_digital"
            : ext === "docx"
              ? "docx_file"
              : ext === "md" || ext === "markdown"
                ? "markdown_file"
                : "txt_file";
        const existing = db.submissions.find((s) => s.student_id === student.id);
        const normalized = text.replace(/\r\n/g, "\n").replace(/\r/g, "\n");
        if (existing) {
          existing.original_text = normalized;
          existing.source_type = source as Submission["source_type"];
          existing.source_filename = filename;
          existing.updated_at = now();
          return Promise.resolve({
            Digital: { text: normalized, pages: 1, source, filename },
          });
        }
        const submission: Submission = {
          id: nextId("sub"),
          student_id: student.id,
          session_id: student.session_id,
          source_type: source as Submission["source_type"],
          status: "draft",
          original_text: normalized,
          source_filename: filename,
          content_sha256: `fake-sha-${normalized.length}`,
          created_at: now(),
          updated_at: now(),
        };
        db.submissions.push(submission);
        return Promise.resolve({
          Digital: {
            text: normalized,
            pages: 1,
            source,
            filename,
          },
        });
      }
      case "list_submissions":
        return Promise.resolve(db.submissions.filter((s) => s.session_id === args["sessionId"]));
      case "get_session_analysis":
        return Promise.resolve(savedAnalyses.get(String(args["sessionId"])) ?? null);
      case "analyze_session": {
        const result = fakeAnalyze(db, String(args["sessionId"]));
        savedAnalyses.set(String(args["sessionId"]), result);
        return Promise.resolve(result);
      }
      default:
        return fail("unknown", `unexpected command ${cmd}`);
    }
  };
  return { invoke, db };
}

interface Word {
  word: string;
  start: number;
  end: number;
}

function wordsWithOffsets(text: string): Word[] {
  const out: Word[] = [];
  const re = /[\p{L}\p{N}]+/gu;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text)) !== null) {
    out.push({ word: m[0].toLowerCase(), start: m.index, end: m.index + m[0].length });
  }
  return out;
}

/**
 * Simplified exact analysis for unit tests: greedy longest common word runs
 * (min 8) with union coverage. Mirrors the real engine's shape, not its
 * Winnowing internals.
 */
export function fakeAnalyze(db: FakeDb, sessionId: string) {
  const MIN_RUN = 8;
  const students = db.students.filter((s) => s.session_id === sessionId);
  const docs = new Map<string, Word[]>();
  for (const st of students) {
    const sub = db.submissions.find((s) => s.student_id === st.id);
    if (sub && sub.original_text.trim() !== "") {
      docs.set(st.id, wordsWithOffsets(sub.original_text));
    }
  }
  const unionLen = (ranges: Array<[number, number]>): number => {
    const sorted = [...ranges].sort((x, y) => x[0] - y[0] || x[1] - y[1]);
    let total = 0;
    let cur: [number, number] | null = null;
    for (const [s, e] of sorted) {
      if (!cur) {
        cur = [s, e];
      } else if (s <= cur[1]) {
        cur[1] = Math.max(cur[1], e);
      } else {
        total += cur[1] - cur[0];
        cur = [s, e];
      }
    }
    if (cur) total += cur[1] - cur[0];
    return total;
  };

  const commonRuns = (a: Word[], b: Word[], minRun: number) => {
    const usedA = new Array<boolean>(a.length).fill(false);
    const usedB = new Array<boolean>(b.length).fill(false);
    const runs: Array<[number, number, number, number]> = [];
    for (;;) {
      // Longest common substring over unused words.
      const prev = new Array<number>(b.length + 1).fill(0);
      let bestLen = 0;
      let bestA = 0;
      let bestB = 0;
      for (let i = 1; i <= a.length; i++) {
        const cur = new Array<number>(b.length + 1).fill(0);
        for (let j = 1; j <= b.length; j++) {
          if (!usedA[i - 1] && !usedB[j - 1] && a[i - 1]?.word === b[j - 1]?.word) {
            cur[j] = (prev[j - 1] ?? 0) + 1;
            if ((cur[j] ?? 0) > bestLen) {
              bestLen = cur[j] ?? 0;
              bestA = i - bestLen;
              bestB = j - bestLen;
            }
          }
        }
        for (let j = 0; j <= b.length; j++) prev[j] = cur[j] ?? 0;
      }
      if (bestLen < minRun) break;
      runs.push([bestA, bestA + bestLen, bestB, bestB + bestLen]);
      for (let k = 0; k < bestLen; k++) {
        usedA[bestA + k] = true;
        usedB[bestB + k] = true;
      }
    }
    return runs;
  };

  const subtract = (
    range: [number, number],
    cuts: Array<[number, number]>,
  ): Array<[number, number]> => {
    let kept: Array<[number, number]> = [range];
    for (const [cs, ce] of cuts) {
      const next: Array<[number, number]> = [];
      for (const [s, e] of kept) {
        if (cs <= s && ce >= e) continue;
        if (ce <= s || cs >= e) {
          next.push([s, e]);
          continue;
        }
        if (cs > s) next.push([s, cs]);
        if (ce < e) next.push([ce, e]);
      }
      kept = next;
    }
    return kept;
  };

  // Prompt exclusion mirrors the real engine (min 5 words, reason Prompt).
  const session = db.sessions.find((s) => s.id === sessionId);
  const promptWords =
    session?.assignment_prompt && session.assignment_prompt.trim() !== ""
      ? wordsWithOffsets(session.assignment_prompt)
      : [];
  const promptSpans = new Map<string, Array<[number, number]>>();
  if (promptWords.length >= 5) {
    for (const [id, words] of docs) {
      const hits = commonRuns(words, promptWords, 5);
      promptSpans.set(
        id,
        hits.map(([as, ae]) => [as, ae]),
      );
    }
  }

  const ids = [...docs.keys()];
  const pairs = [];
  const spans = new Map<string, Array<[number, number]>>();
  for (let i = 0; i < ids.length; i++) {
    for (let j = i + 1; j < ids.length; j++) {
      const aId = ids[i] as string;
      const bId = ids[j] as string;
      const a = docs.get(aId) ?? [];
      const b = docs.get(bId) ?? [];
      const runs = commonRuns(a, b, MIN_RUN);
      const passages = [];
      const excluded = [];
      for (const [as, ae, bs, be] of runs) {
        const offset = bs - as;
        const keptA = subtract([as, ae], promptSpans.get(aId) ?? []);
        const keptB = subtract([bs, be], promptSpans.get(bId) ?? []);
        for (const [s, e] of keptA) {
          const mappedS = s + offset;
          const mappedE = e + offset;
          // Keep only the part both sides agree is reportable.
          const overlap = keptB.some(([ks, ke]) => mappedS < ke && mappedE > ks);
          if (!overlap) continue;
          passages.push({
            kind: "exact",
            identity: null,
            // The fake has no document-frequency metadata; component tests cover the common
            // badge with hand-built passages (see PassageCard tests).
            common_text: false,
            a_token_start: s,
            a_token_end: e,
            b_token_start: mappedS,
            b_token_end: mappedE,
            a_char_start: a[s]?.start ?? 0,
            a_char_end: a[e - 1]?.end ?? 0,
            b_char_start: b[mappedS]?.start ?? 0,
            b_char_end: b[mappedE - 1]?.end ?? 0,
            tokens: e - s,
          });
          spans.set(aId, [...(spans.get(aId) ?? []), [s, e]]);
          spans.set(bId, [...(spans.get(bId) ?? []), [mappedS, mappedE]]);
        }
        for (const [side, doc, docSpans, lo, hi] of [
          ["a", a, promptSpans.get(aId) ?? [], as, ae] as const,
          ["b", b, promptSpans.get(bId) ?? [], bs, be] as const,
        ]) {
          for (const [ps, pe] of docSpans) {
            const s = Math.max(ps, lo);
            const e = Math.min(pe, hi);
            if (s < e) {
              excluded.push({
                side,
                token_start: s,
                token_end: e,
                char_start: doc[s]?.start ?? 0,
                char_end: doc[e - 1]?.end ?? 0,
                reason: "Prompt",
                tokens: e - s,
              });
            }
          }
        }
      }
      pairs.push({
        a_student_id: aId,
        b_student_id: bId,
        runs: passages.map((p) => [p.a_token_start, p.a_token_end] as [number, number]),
        bruns: passages.map((p) => [p.b_token_start, p.b_token_end] as [number, number]),
        passages,
        excluded,
        aLen: a.length,
        bLen: b.length,
      });
    }
  }

  // Eligible text per doc: total minus prompt-covered words.
  const eligibleOf = (id: string): number => {
    const total = docs.get(id)?.length ?? 0;
    return total - unionLen(promptSpans.get(id) ?? []);
  };

  // Per-pair coverage restricted to that pair's own runs, over eligible text.
  const withCoverage = pairs.map((p) => {
    const aRanges: Array<[number, number]> = p.runs;
    const bRanges: Array<[number, number]> = p.bruns;
    const eligibleA = eligibleOf(p.a_student_id);
    const eligibleB = eligibleOf(p.b_student_id);
    return {
      a_student_id: p.a_student_id,
      b_student_id: p.b_student_id,
      coverage_a: eligibleA === 0 ? null : (unionLen(aRanges) / eligibleA) * 100,
      coverage_b: eligibleB === 0 ? null : (unionLen(bRanges) / eligibleB) * 100,
      exact_coverage_a: eligibleA === 0 ? null : (unionLen(aRanges) / eligibleA) * 100,
      exact_coverage_b: eligibleB === 0 ? null : (unionLen(bRanges) / eligibleB) * 100,
      modified_coverage_a: eligibleA === 0 ? null : 0,
      modified_coverage_b: eligibleB === 0 ? null : 0,
      passages: p.passages,
      excluded: p.excluded,
    };
  });

  const per_student = students.map((st) => {
    const total = docs.get(st.id)?.length ?? 0;
    const eligible = eligibleOf(st.id);
    const matched = Math.min(unionLen(spans.get(st.id) ?? []), eligible);
    return {
      student_id: st.id,
      coverage: eligible === 0 ? null : (matched / eligible) * 100,
      exact_coverage: eligible === 0 ? null : (matched / eligible) * 100,
      modified_coverage: eligible === 0 ? null : 0,
      matched_tokens: matched,
      total_tokens: total,
      eligible_tokens: eligible,
    };
  });

  return {
    fingerprint_version: 1,
    normalization_version: 1,
    common_text_version: 1,
    modified_version: 1,
    common_text_applied: false,
    prompt_applied: promptWords.length >= 5,
    pairs: withCoverage,
    per_student,
  };
}

export function renderRouteAt(path: string) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const testRouter = createAppRouter(path);
  return render(
    <QueryClientProvider client={client}>
      <RouterProvider router={testRouter} />
    </QueryClientProvider>,
  );
}

export function sessionFixture(overrides?: Partial<Session>): Session {
  return {
    id: "sess-1",
    name: "Biology Assignment 1",
    subject: "Biology",
    status: "draft",
    assignment_prompt: null,
    excluded_reference_text: null,
    exclude_common_text: true,
    created_at: "2026-09-04T10:00:00.000Z",
    updated_at: "2026-09-04T10:00:00.000Z",
    ...overrides,
  };
}

export function studentFixture(overrides?: Partial<Student>): Student {
  return {
    id: "stud-1",
    session_id: "sess-1",
    display_name: "Amit",
    created_at: "2026-09-04T10:00:00.000Z",
    ...overrides,
  };
}

export function submissionFixture(overrides?: Partial<Submission>): Submission {
  return {
    id: "sub-1",
    student_id: "stud-1",
    session_id: "sess-1",
    source_type: "pasted_text",
    status: "draft",
    original_text: "Photosynthesis converts light energy.",
    source_filename: null,
    content_sha256: "fake-sha-39",
    created_at: "2026-09-04T10:00:00.000Z",
    updated_at: "2026-09-04T10:00:00.000Z",
    ...overrides,
  };
}
