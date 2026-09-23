/**
 * `bench authoring score` (specs/benchmark.md#authoring).
 *
 * Reads the recorded answers in `corpus/authoring/outputs/`, turns each into a
 * drawing — a Mermaid answer through Merlion, an SVG answer as written — and
 * scores every drawing the same way: does it exist, does it show the graph the
 * task declared, and can it be read.
 *
 * Every run also scores the ceiling: Merlion's own drawing of each task's
 * reference graph, read back through the same geometry recovery that a
 * hand-written SVG is read through. Geometry recovery loses information even
 * on a drawing built from a known graph, and the ceiling says how much, so the
 * SVG column is never credited or blamed for the reader's own error.
 */
import { FileSystem, Path } from "@effect/platform";
import { Effect, Schema } from "effect";
import { chromium } from "playwright";
import { MerlionLive } from "../renderers/merlion.ts";
import { MermaidDagreLive } from "../renderers/mermaid.ts";
import { Renderer } from "../renderers/Renderer.ts";
import { extractSvg } from "../svg/extract.ts";
import { extractGeneric } from "../svg/generic.ts";
import { fidelity, type Fidelity } from "./compare.ts";
import { EMPTY, type Legibility, measure, preparePage } from "./legibility.ts";
import { extractAnswer } from "./prompt.ts";
import { renderReport } from "./report.ts";
import { type Format, loadTasks, OutputFile, OUTPUTS_DIR, referenceMermaid, type Task } from "./tasks.ts";
import { RESULTS_DIR } from "../paths.ts";

const FidelitySchema = Schema.Struct({
  refNodes: Schema.Number,
  gotNodes: Schema.Number,
  nodesMatched: Schema.Number,
  nodeF1: Schema.Number,
  refEdges: Schema.Number,
  gotEdges: Schema.Number,
  edgesMatched: Schema.Number,
  edgeF1: Schema.Number,
  labelledEdges: Schema.Number,
  edgeLabelsMatched: Schema.Number,
  danglingEdges: Schema.Number,
});

const LegibilitySchema = Schema.Struct({
  labels: Schema.Number,
  overflowing: Schema.Number,
  worstOverflow: Schema.Number,
  clipped: Schema.Number,
  shapeOverlaps: Schema.Number,
});

export const Row = Schema.Struct({
  task: Schema.String,
  size: Schema.String,
  format: Schema.String,
  /** A drawing exists: the answer held the requested format and it renders. */
  drawn: Schema.Boolean,
  error: Schema.NullOr(Schema.String),
  outputTokens: Schema.NullOr(Schema.Number),
  /** Bytes of the diagram source the model wrote, before rendering. */
  sourceBytes: Schema.Number,
  /** A Mermaid answer only: whether mermaid 12.0.0 itself parses it. */
  mermaidParsed: Schema.NullOr(Schema.Boolean),
  fidelity: Schema.NullOr(FidelitySchema),
  legibility: Schema.NullOr(LegibilitySchema),
});
export type Row = typeof Row.Type;

export const CeilingRow = Schema.Struct({
  task: Schema.String,
  fidelity: FidelitySchema,
  legibility: LegibilitySchema,
});

export const ModelResult = Schema.Struct({
  model: Schema.String,
  provider: Schema.String,
  generated: Schema.String,
  rows: Schema.Array(Row),
});

export const AuthoringResults = Schema.Struct({
  date: Schema.String,
  tasksVersion: Schema.Number,
  tasks: Schema.Number,
  /** Merlion's drawing of each reference graph, read through geometry recovery. */
  ceiling: Schema.Array(CeilingRow),
  models: Schema.Array(ModelResult),
});
export type AuthoringResults = typeof AuthoringResults.Type;

// ---------------------------------------------------------------------------

const renderMermaid = (source: string) =>
  Effect.gen(function* () {
    const r = yield* Renderer;
    return yield* r.render(source);
  }).pipe(Effect.provide(MerlionLive));

const mermaidAccepts = (source: string) =>
  Effect.gen(function* () {
    const r = yield* Renderer;
    const out = yield* r.render(source);
    return out.svg !== null;
  }).pipe(Effect.provide(MermaidDagreLive), Effect.orElseSucceed(() => false));

/** The drawing a model's answer produces, and why there is none when there is none. */
const draw = (answer: string, format: Format) =>
  format === "svg"
    ? Effect.succeed(/<svg[\s>]/i.test(answer) ? { svg: answer, error: null } : { svg: null, error: "no <svg> element" })
    : renderMermaid(answer).pipe(Effect.map((o) => ({ svg: o.svg, error: o.error })));

/** Scores every recorded answer and writes the JSON and Markdown summaries into `dir` (default `results/`). */
export const score = (outDir?: string) =>
  Effect.gen(function* () {
    const fs = yield* FileSystem.FileSystem;
    const path = yield* Path.Path;
    const file = yield* loadTasks;
    const tasks = new Map(file.tasks.map((t) => [t.name, t]));

    const browser = yield* Effect.acquireRelease(
      Effect.promise(() => chromium.launch()),
      (b) => Effect.promise(() => b.close()),
    );
    const page = yield* Effect.promise(async () => {
      const p = await browser.newPage();
      await p.setContent("<!doctype html><html><head><meta charset='utf-8'></head><body></body></html>");
      return p;
    });
    yield* preparePage(page);

    // The ceiling: Merlion's drawing of each declared graph, read back the way
    // a hand-written SVG is read.
    const ceiling: Array<typeof CeilingRow.Type> = [];
    for (const task of file.tasks) {
      const out = yield* renderMermaid(referenceMermaid(task.reference));
      if (out.svg === null) {
        return yield* Effect.die(new Error(`the reference graph of ${task.name} does not render: ${out.error ?? "?"}`));
      }
      ceiling.push({
        task: task.name,
        fidelity: fidelity(task.reference, extractGeneric(out.svg)),
        legibility: yield* measure(page, out.svg).pipe(Effect.orElseSucceed(() => EMPTY)),
      });
    }

    const exists = yield* fs.exists(OUTPUTS_DIR);
    const files = exists
      ? (yield* fs.readDirectory(OUTPUTS_DIR)).filter((f) => f.endsWith(".json")).sort()
      : [];
    const models: Array<typeof ModelResult.Type> = [];
    for (const name of files) {
      const text = yield* fs.readFileString(path.join(OUTPUTS_DIR, name));
      const of = yield* Schema.decodeUnknown(Schema.parseJson(OutputFile))(text);
      const rows: Row[] = [];
      for (const o of of.outputs) {
        const task = tasks.get(o.task);
        if (task === undefined) continue;
        rows.push(yield* scoreOne(page, task, o.format, o.extracted ?? extractAnswer(o.text, o.format), o.usage.outputTokens));
      }
      models.push({ model: of.model, provider: of.provider, generated: of.generated, rows });
    }

    const results: AuthoringResults = {
      date: new Date().toISOString().slice(0, 10),
      tasksVersion: file.version,
      tasks: file.tasks.length,
      ceiling,
      models,
    };
    const dir = outDir ?? RESULTS_DIR;
    yield* fs.makeDirectory(dir, { recursive: true });
    const json = path.join(dir, `${results.date}-authoring.json`);
    const md = path.join(dir, `${results.date}-authoring.md`);
    yield* fs.writeFileString(json, `${JSON.stringify(results, null, 2)}\n`);
    yield* fs.writeFileString(md, `${renderReport(results)}\n`);
    yield* Effect.log(`authoring: ${models.length} model file(s), ${file.tasks.length} tasks → ${md}`);
    return results;
  }).pipe(Effect.scoped);

const scoreOne = (
  page: Parameters<typeof measure>[0],
  task: Task,
  format: Format,
  answer: string | null,
  outputTokens: number | null,
) =>
  Effect.gen(function* () {
    const base = { task: task.name, size: task.size, format, outputTokens, sourceBytes: answer?.length ?? 0 };
    if (answer === null) {
      return { ...base, drawn: false, error: "no diagram in the answer", mermaidParsed: null, fidelity: null, legibility: null };
    }
    const mermaidParsed = format === "mermaid" ? yield* mermaidAccepts(answer) : null;
    const { svg, error } = yield* draw(answer, format);
    if (svg === null) {
      return { ...base, drawn: false, error: error ?? "no drawing", mermaidParsed, fidelity: null, legibility: null };
    }
    // A Mermaid answer is read through Merlion's own data attributes, which
    // state the graph; an SVG answer has to be recovered from geometry.
    const graph = format === "mermaid" ? extractSvg(svg) : extractGeneric(svg);
    return {
      ...base,
      drawn: true,
      error: null,
      mermaidParsed,
      fidelity: fidelity(task.reference, graph),
      legibility: yield* measure(page, svg).pipe(Effect.orElseSucceed(() => EMPTY)),
    };
  });

export type { Fidelity, Legibility };
