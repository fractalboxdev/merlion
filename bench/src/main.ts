/**
 * Entry point: `pnpm bench <fetch|run|report>`.
 *
 *   pnpm bench fetch
 *   pnpm bench run [--renderers merlion,mermaid-dagre,mermaid-elk] [--corpus compat] [--limit N] [--out-svgs] [--no-edits]
 *   pnpm bench report [--input results/<file>.json] [--out <file>.md]
 *   pnpm bench determinism [--corpus compat|compat-sequence|compat-state|sequence|state] [--font link|embed|system]
 *   pnpm bench parity [--limit N] [--require-rsvg]
 *   pnpm bench sequence [--limit N] [--out-svgs]
 *   pnpm bench state [--limit N] [--out-svgs]
 *   pnpm bench authoring generate --provider <p> --model <m> [--label <name>] [--limit N] [--resume]
 *   pnpm bench authoring score [--out <dir>]
 */
import { Command, Options } from "@effect/cli";
import { FetchHttpClient } from "@effect/platform";
import { NodeContext, NodeRuntime } from "@effect/platform-node";
import { Effect, Layer, Option, Schema } from "effect";
import { generate } from "./authoring/generate.ts";
import { PROVIDERS } from "./authoring/provider.ts";
import { score } from "./authoring/score.ts";
import { fetchCorpus } from "./corpus/fetch.ts";
import { CORPUS_NAMES, determinism, FONT_MODES } from "./determinism.ts";
import { parity } from "./parity/gate.ts";
import { RENDERER_NAMES, type RendererName } from "./renderers/Renderer.ts";
import { report } from "./report.ts";
import { run } from "./run.ts";
import { sequenceRun } from "./sequence-run.ts";
import { stateRun } from "./state-run.ts";

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
  Options.withDescription("A mermaid corpus (compat, compat-sequence, compat-state) or a core fixture corpus (sequence, state)"),
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

const stateLimit = Options.integer("limit").pipe(Options.optional, Options.withDescription("Render only the first N diagrams"));
const stateOutSvgs = Options.boolean("out-svgs").pipe(Options.withDescription("Write every SVG to results/svgs-state/<renderer>/"));
const stateCmd = Command.make("state", { limit: stateLimit, outSvgs: stateOutSvgs }, (o) =>
  stateRun({ limit: o.limit, outSvgs: o.outSvgs }).pipe(Effect.asVoid),
).pipe(Command.withDescription("Render the compat-state corpus with Merlion and mermaid and write the baseline"));

const provider = Options.choice("provider", PROVIDERS).pipe(
  Options.withDefault("claude-cli" as const),
  Options.withDescription("anthropic (ANTHROPIC_API_KEY) or claude-cli (the local `claude` CLI, no key)"),
);
const model = Options.text("model").pipe(Options.withDescription("Model id, as the provider names it"));
const label = Options.text("label").pipe(
  Options.optional,
  Options.withDescription("File stem under corpus/authoring/outputs/ (default: the model id)"),
);
const authoringLimit = Options.integer("limit").pipe(Options.optional, Options.withDescription("Ask only the first N tasks"));
const resume = Options.boolean("resume").pipe(
  Options.withDescription("Keep the answers already recorded and re-ask only the ones missing, failed or truncated"),
);
const generateCmd = Command.make("generate", { provider, model, label, authoringLimit, resume }, (o) =>
  generate({
    provider: o.provider,
    model: o.model,
    label: Option.getOrUndefined(o.label),
    limit: Option.getOrUndefined(o.authoringLimit),
    resume: o.resume,
  }).pipe(Effect.asVoid),
).pipe(Command.withDescription("Ask one model for every authoring task in both formats and record the answers"));

const scoreOut = Options.directory("out").pipe(
  Options.optional,
  Options.withDescription("Where to write the summaries (default: results/)"),
);
const scoreCmd = Command.make("score", { scoreOut }, (o) => score(Option.getOrUndefined(o.scoreOut)).pipe(Effect.asVoid)).pipe(
  Command.withDescription("Score every recorded answer against the task's declared graph and write the summary"),
);

const authoringCmd = Command.make("authoring").pipe(
  Command.withDescription("Mermaid or SVG: which output format serves a model better"),
  Command.withSubcommands([generateCmd, scoreCmd]),
);

const bench = Command.make("bench").pipe(
  Command.withSubcommands([fetchCmd, runCmd, reportCmd, determinismCmd, parityCmd, sequenceCmd, stateCmd, authoringCmd]),
);

const cli = Command.run(bench, { name: "merlion-bench", version: "0.0.0" });

cli(process.argv).pipe(
  Effect.provide(Layer.mergeAll(NodeContext.layer, FetchHttpClient.layer)),
  NodeRuntime.runMain,
);
