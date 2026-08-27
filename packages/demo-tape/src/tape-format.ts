// Tape format v1 — a frozen external contract OWNED BY mysteryshopper, not
// kineto. Transcribed verbatim (field names, types, comments) from the
// canonical implementation:
//
//   ~/personal/repos/mysteryshopper/src/tape.ts
//
// which was produced by "Task 2: Tape contract v1" in
//   ~/personal/repos/mysteryshopper/docs/superpowers/plans/2026-08-25-mysteryshopper-v1.md
// and is referenced as the frozen contract in
//   ~/personal/repos/mysteryshopper/docs/superpowers/specs/2026-08-25-mysteryshopper-design.md
//   (§7: "The tape format is a contract, not an implementation detail.").
//
// A tape directory is one `actions.jsonl` file (line 1: a `TapeHeader` JSON
// object; every subsequent line: one `TapeFrame` JSON object) plus one
// screenshot file per frame, named by `TapeFrame.frame` (mysteryshopper's
// `frameName()` produces `step-${String(step).padStart(2, '0')}.jpg`, e.g.
// "step-01.jpg").
//
// kineto's tape adapter (adapter.ts) only READS this format — it must
// never diverge from mysteryshopper's tape.ts. If that file changes
// upstream, this transcription needs a matching update.

/** First line of `actions.jsonl`. */
export interface TapeHeader {
  v: 1;
  url: string;
  task: string;
  viewport: { width: number; height: number };
  started_at: string;
}

/** One JSON-per-line record after the header, one per step. */
export interface TapeFrame {
  t_ms: number;
  step: number;
  narration: string;
  action: string;
  /** Screenshot filename for this frame, e.g. "step-01.jpg". */
  frame: string;
}
