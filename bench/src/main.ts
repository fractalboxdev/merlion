/**
 * Entry point: `pnpm bench <fetch|run|report>`.
 *
 *   pnpm bench fetch
 *   pnpm bench run [--renderers merlion,mermaid-dagre,mermaid-elk] [--corpus compat] [--limit N] [--out-svgs] [--no-edits]
 *   pnpm bench report [--input results/<file>.json] [--out <file>.md]
 *   pnpm bench determinism [--corpus compat|sequence] [--font link|embed|system]
 *   pnpm bench parity [--limit N] [--require-rsvg]
 *   pnpm bench sequence [--limit N] [--out-svgs]
 */
import { Command, Options } from "@effect/cli";
import { FetchHttpClient } from "@effect/platform";
import { NodeContext, NodeRuntime } from "@effect/platform-node";
import { Effect, Layer, Schema } from "effect";
import { fetchCorpus } from "./corpus/fetch.ts";
import { CORPUS_NAMES, determinism, FONT_MODES } from "./determinism.ts";
import { parity } from "./parity/gate.ts";
import { RENDERER_NAMES, type RendererName } from "./renderers/Renderer.ts";
import { report } from "./report.ts";
import { run } from "./run.ts";
import { sequenceRun } from "./sequence-run.ts";

const fetchCmd = Command.make("fetch", {}, () => fetchCorpus.pipe(Effect.asVoid)).pipe(
  Command.withDescription("Download the compat corpus at the pinned mermaid commit and derive the edits corpus"),
);

const RendererList = Schema.transform(Schema.String, Schema.Array(Schema.Literal(...RENDERER_NAMES)), {
  strict: true,
  decode: (s) => s.split(",").map((x) => x.trim()).filter((x) => x.length > 0) as RendererName[],
  encode: (xs) => xs.join(","),
});

const renderers = Options.text("renderers").pipe(
  Options.withSchema(RendererList),
  Options.withDefault(RENDERER_NAMES as readonly RendererName[]),
  Options.withDescription(`Comma-separated subset of ${RENDERER_NAMES.join(", ")}`),
);
const corpus = Options.choice("corpus", ["compat"] as const).pipe(Options.withDefault("compat" as const));
const limit = Options.integer("limit").pipe(Options.optional, Options.withDescription("Render only the first N diagrams"));
const outSvgs = Options.boolean("out-svgs").pipe(Options.withDescription("Write every SVG to results/svgs/<renderer>/"));
const noEdits = Options.boolean("no-edits").pipe(Options.withDescription("Skip the edits corpus (stability)"));

const runCmd = Command.make("run", { renderers, corpus, limit, outSvgs, noEdits }, (o) =>
  run({ renderers: o.renderers, corpus: o.corpus, limit: o.limit, outSvgs: o.outSvgs, edits: !o.noEdits }).pipe(Effect.asVoid),
).pipe(Command.withDescription("Render the corpus with each renderer and record metrics"));

const input = Options.file("input").pipe(Options.optional, Options.withDescription("Results JSON (default: newest in results/)"));
const out = Options.file("out").pipe(Options.optional, Options.withDescription("Markdown output (default: next to the JSON)"));
const reportCmd = Command.make("report", { input, out }, (o) => report(o.input, o.out).pipe(Effect.asVoid)).pipe(
  Command.withDescription("Summarise a results file as Markdown"),
);

const font = Options.choice("font", FONT_MODES).pipe(Options.withDefault("link" as const));
const determinismCorpus = Options.choice("corpus", CORPUS_NAMES).pipe(
  Options.withDefault("compat" as const),
  Options.withDescription("compat (mermaid flowcharts) or sequence (the core's sequence fixtures)"),
);
const determinismCmd = Command.make("determinism", { corpus: determinismCorpus, font }, (o) =>
  determinism(o.corpus, o.font).pipe(Effect.asVoid),
).pipe(Command.withDescription("Check native (CLI --batch) and WASM output are byte-identical over a corpus"));

const parityLimit = Options.integer("limit").pipe(Options.optional, Options.withDescription("Check only the first N diagrams"));
const requireRsvg = Options.boolean("require-rsvg").pipe(Options.withDescription("Fail instead of skipping when rsvg-convert is not on PATH"));
const parityCmd = Command.make("parity", { limit: parityLimit, requireRsvg }, (o) =>
  parity({ limit: o.limit, requireRsvg: o.requireRsvg }).pipe(Effect.asVoid),
).pipe(
  Command.withDescription("Stylesheet parity: page CSS in Chromium vs baked SVG with no host CSS, without <style>, and through rsvg-convert"),
);

const seqLimit = Options.integer("limit").pipe(Options.optional, Options.withDescription("Render only the first N diagrams"));
const seqOutSvgs = Options.boolean("out-svgs").pipe(Options.withDescription("Write every SVG to results/svgs-sequence/<renderer>/"));
const sequenceCmd = Command.make("sequence", { limit: seqLimit, outSvgs: seqOutSvgs }, (o) =>
  sequenceRun({ limit: o.limit, outSvgs: o.outSvgs }).pipe(Effect.asVoid),
).pipe(Command.withDescription("Render the compat-sequence corpus with Merlion and mermaid and write the baseline"));

const bench = Command.make("bench").pipe(
  Command.withSubcommands([fetchCmd, runCmd, reportCmd, determinismCmd, parityCmd, sequenceCmd]),
);

const cli = Command.run(bench, { name: "merlion-bench", version: "0.0.0" });

cli(process.argv).pipe(
  Effect.provide(Layer.mergeAll(NodeContext.layer, FetchHttpClient.layer)),
  NodeRuntime.runMain,
);
