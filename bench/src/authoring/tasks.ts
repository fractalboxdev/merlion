/**
 * The `authoring` corpus (specs/benchmark.md#authoring): diagram tasks stated
 * in prose beside the graph they mean, and the recorded model outputs scored
 * against it.
 *
 * The task file is the ground truth. A task's prose quotes every node label
 * exactly as the reference spells it, so following the prose scores full node
 * fidelity in either output format and the comparison measures the format, not
 * the model's invention. Edge labels are scored apart from topology, because
 * the prose names a condition without fixing its wording.
 */
import { FileSystem } from "@effect/platform";
import { Effect, Schema } from "effect";
import { join } from "node:path";
import { BENCH_DIR } from "../paths.ts";

export const AUTHORING_DIR = join(BENCH_DIR, "corpus", "authoring");
export const TASKS_FILE = join(AUTHORING_DIR, "tasks.json");
export const OUTPUTS_DIR = join(AUTHORING_DIR, "outputs");

export const TaskSize = Schema.Literal("small", "medium", "large");
export type TaskSize = typeof TaskSize.Type;

export const FORMATS = ["mermaid", "svg"] as const;
export const Format = Schema.Literal(...FORMATS);
export type Format = typeof Format.Type;

export class RefNode extends Schema.Class<RefNode>("RefNode")({
  id: Schema.String,
  label: Schema.String,
}) {}

export class RefEdge extends Schema.Class<RefEdge>("RefEdge")({
  from: Schema.String,
  to: Schema.String,
  label: Schema.String,
}) {}

export class RefGraph extends Schema.Class<RefGraph>("RefGraph")({
  nodes: Schema.Array(RefNode),
  edges: Schema.Array(RefEdge),
}) {}

export class Task extends Schema.Class<Task>("Task")({
  name: Schema.String,
  size: TaskSize,
  /** The diagram stated in prose; the only thing a model is shown. */
  prompt: Schema.String,
  reference: RefGraph,
}) {}

export class TaskFile extends Schema.Class<TaskFile>("TaskFile")({
  corpus: Schema.Literal("authoring"),
  version: Schema.Number,
  licence: Schema.String,
  note: Schema.String,
  tasks: Schema.Array(Task),
}) {}

export const loadTasks = Effect.gen(function* () {
  const fs = yield* FileSystem.FileSystem;
  const text = yield* fs.readFileString(TASKS_FILE);
  return yield* Schema.decodeUnknown(Schema.parseJson(TaskFile))(text);
});

/** Canonical Mermaid for a reference graph: what a perfect model would write. */
export const referenceMermaid = (g: RefGraph): string => {
  const lines = ["flowchart TD"];
  for (const n of g.nodes) lines.push(`  ${n.id}[${n.label}]`);
  for (const e of g.edges) {
    lines.push(e.label === "" ? `  ${e.from} --> ${e.to}` : `  ${e.from} -->|${e.label}| ${e.to}`);
  }
  return `${lines.join("\n")}\n`;
};

// ---------------------------------------------------------------------------
// Recorded model output.

export class Usage extends Schema.Class<Usage>("Usage")({
  /** Every token the model emitted, reasoning included, or null when the provider reported none. */
  outputTokens: Schema.NullOr(Schema.Number),
  inputTokens: Schema.NullOr(Schema.Number),
  /**
   * Of those, the ones spent thinking rather than answering. Absent for a
   * provider that does not separate them, and for answers recorded before a
   * reasoning model was first measured.
   */
  reasoningTokens: Schema.optional(Schema.NullOr(Schema.Number)),
}) {}

export class Output extends Schema.Class<Output>("Output")({
  task: Schema.String,
  format: Format,
  model: Schema.String,
  /** ISO date the output was generated; the corpus is a record, not a live call. */
  generated: Schema.String,
  /** Everything between the fences, exactly as the model wrote it. */
  text: Schema.String,
  /** Null when the model answered with no fenced block of the requested language. */
  extracted: Schema.NullOr(Schema.String),
  usage: Usage,
}) {}

export class OutputFile extends Schema.Class<OutputFile>("OutputFile")({
  corpus: Schema.Literal("authoring"),
  model: Schema.String,
  provider: Schema.String,
  generated: Schema.String,
  tasksVersion: Schema.Number,
  outputs: Schema.Array(Output),
}) {}
