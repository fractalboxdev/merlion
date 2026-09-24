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
import { Context, Effect, Layer, Schema } from "effect";
import { chromium } from "playwright";
import { MerlionLive } from "../renderers/merlion.ts";
import { MermaidDagreLive } from "../renderers/mermaid.ts";
import { Renderer, type RendererApi } from "../renderers/Renderer.ts";
import { extractSvg } from "../svg/extract.ts";
import { extractGeneric } from "../svg/generic.ts";
import { fidelity, type Fidelity } from "./compare.ts";
import { type Legibility, measure, preparePage } from "./legibility.ts";
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
  unplaced: Schema.Number,
});

export const Row = Schema.Struct({
  task: Schema.String,
  size: Schema.String,
  format: Schema.String,
  /** A drawing exists: the answer held the requested format and it renders. */
  drawn: Schema.Boolean,
  /** The provider never returned an answer, so this row judges no model output. */
  callFailed: Schema.Boolean,
  /** The answer stopped at the token cap; a fragment is not a drawing. */
  truncated: Schema.Boolean,
  error: Schema.NullOr(Schema.String),
  outputTokens: Schema.NullOr(Schema.Number),
  /** Of those, the ones spent thinking rather than answering; null where the provider does not separate them. */
  reasoningTokens: Schema.NullOr(Schema.Number),
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

/**
 * Whether mermaid itself parses a source, which separates Merlion's tolerance
 * from validity. The layer is built once by the caller: building it per answer
 * launches a browser and reloads the mermaid bundle for every row.
 */
const mermaidAccepts = (mermaid: RendererApi, source: string) =>
  mermaid.render(source).pipe(Effect.map((out) => out.svg !== null), Effect.orElseSucceed(() => false));

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
    // One mermaid host for the whole run, not one per answer.
    const mermaid = Context.get(yield* Layer.build(MermaidDagreLive), Renderer);

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
        legibility: yield* measure(page, out.svg).pipe(
          Effect.orElseFail(() => new Error(`the legibility probe failed on the reference drawing of ${task.name}`)),
          Effect.orDie,
        ),
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
      if (of.tasksVersion !== file.version) {
        return yield* Effect.die(
          new Error(
            `${name} holds answers to task file version ${of.tasksVersion}, but tasks.json is version ${file.version}. Re-generate it, or restore the task file.`,
          ),
        );
      }
      const rows: Row[] = [];
      for (const o of of.outputs) {
        const task = tasks.get(o.task);
        if (task === undefined) {
          return yield* Effect.die(
            new Error(`${name} holds an answer to "${o.task}", which tasks.json no longer defines. Re-generate it, or restore the task.`),
          );
        }
        rows.push(
          yield* scoreOne(
            page,
            task,
            o.format,
            o.extracted ?? extractAnswer(o.text, o.format),
            o.usage.outputTokens,
            o.usage.reasoningTokens ?? null,
            o.error ?? null,
            o.truncated ?? false,
            mermaid,
          ),
        );
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
  reasoningTokens: number | null,
  callError: string | null,
  truncated: boolean,
  mermaid: RendererApi,
) =>
  Effect.gen(function* () {
    const base = {
      task: task.name,
      size: task.size,
      format,
      outputTokens,
      reasoningTokens,
      // Real UTF-8 bytes: `String.length` counts UTF-16 code units, which is
      // a different number for any diagram with an arrow glyph or an accent,
      // and this is the figure the cost comparison is reported on.
      sourceBytes: answer === null ? 0 : Buffer.byteLength(answer, "utf8"),
      callFailed: callError !== null,
      truncated,
    };
    // A fragment cut off at the cap is not a worse drawing, it is half a file;
    // scoring it for fidelity would read the model's budget, not its answer.
    if (truncated) {
      return { ...base, drawn: false, error: "the answer stopped at the token cap", mermaidParsed: null, fidelity: null, legibility: null };
    }
    if (callError !== null) {
      return { ...base, drawn: false, error: callError, mermaidParsed: null, fidelity: null, legibility: null };
    }
    if (answer === null) {
      return { ...base, drawn: false, error: "no diagram in the answer", mermaidParsed: null, fidelity: null, legibility: null };
    }
    const mermaidParsed = format === "mermaid" ? yield* mermaidAccepts(mermaid, answer) : null;
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
      // A probe that cannot measure a drawing reports nothing rather than a
      // clean sheet: `EMPTY` is a perfect score, and a crash must never read
      // as one.
      legibility: yield* measure(page, svg).pipe(
        Effect.tapError((e) => Effect.logWarning(`${task.name}/${format}: legibility not measured (${e.message})`)),
        Effect.orElseSucceed(() => null),
      ),
    };
  });

export type { Fidelity, Legibility };
