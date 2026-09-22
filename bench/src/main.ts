/**
 * Entry point: `pnpm bench <fetch|run|report>`.
 *
 *   pnpm bench fetch
 *   pnpm bench run [--renderers merlion,mermaid-dagre,mermaid-elk] [--corpus compat] [--limit N] [--out-svgs] [--no-edits]
 *   pnpm bench report [--input results/<file>.json] [--out <file>.md]
 *   pnpm bench determinism [--font link|embed|system]
 */
import { Command, Options } from "@effect/cli";
import { FetchHttpClient } from "@effect/platform";
import { NodeContext, NodeRuntime } from "@effect/platform-node";
import { Effect, Layer, Schema } from "effect";
import { fetchCorpus } from "./corpus/fetch.ts";
import { determinism, FONT_MODES } from "./determinism.ts";
import { RENDERER_NAMES, type RendererName } from "./renderers/Renderer.ts";
import { report } from "./report.ts";
import { run } from "./run.ts";

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
const determinismCmd = Command.make("determinism", { font }, (o) => determinism(o.font).pipe(Effect.asVoid)).pipe(
  Command.withDescription("Check native (CLI --batch) and WASM output are byte-identical over the compat corpus"),
);

const bench = Command.make("bench").pipe(Command.withSubcommands([fetchCmd, runCmd, reportCmd, determinismCmd]));

const cli = Command.run(bench, { name: "merlion-bench", version: "0.0.0" });

cli(process.argv).pipe(
  Effect.provide(Layer.mergeAll(NodeContext.layer, FetchHttpClient.layer)),
  NodeRuntime.runMain,
);
