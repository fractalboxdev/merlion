/**
 * `bench run`: renders a corpus with each selected renderer, extracts the
 * drawn graph and records metrics per diagram (specs/benchmark.md). A failed
 * render is a result, never an abort.
 */
import { Command, FileSystem, Path } from "@effect/platform";
import { Console, Effect, Option, Schema, String as Str } from "effect";
import { pathToFileURL } from "node:url";
import { compareGraphs, displacement, layoutMetrics } from "./metrics/metrics.ts";
import { COMPAT_DIR, EDITS_DIR, MERLION_WASM_DIR, REPO_DIR, RESULTS_DIR } from "./paths.ts";
import { MerlionLive } from "./renderers/merlion.ts";
import { MermaidDagreLive, MermaidElkLive } from "./renderers/mermaid.ts";
import { failed, Renderer, type RendererName } from "./renderers/Renderer.ts";
import {
  type Determinism,
  type DiagramResult,
  EditPairFile,
  type EditResult,
  Manifest,
  type RendererResult,
  ResultsFile,
} from "./schema.ts";
import { type ExtractedGraph, extractSvg } from "./svg/extract.ts";

/** Compatibility is measured against this renderer's drawing of the same source. */
export const REFERENCE: RendererName = "mermaid-dagre";

export interface RunOptions {
  readonly renderers: readonly RendererName[];
  readonly corpus: "compat";
  readonly limit: Option.Option<number>;
  readonly outSvgs: boolean;
  readonly edits: boolean;
}

interface Diagram {
  readonly name: string;
  readonly source: string;
}

const layerFor = (name: RendererName) =>
  name === "merlion" ? MerlionLive : name === "mermaid-dagre" ? MermaidDagreLive : MermaidElkLive;

const loadCorpus = (limit: Option.Option<number>) =>
  Effect.gen(function* () {
    const fs = yield* FileSystem.FileSystem;
    const path = yield* Path.Path;
    const manifestText = yield* fs.readFileString(path.join(COMPAT_DIR, "manifest.json"));
    const manifest = yield* Schema.decodeUnknown(Schema.parseJson(Manifest))(manifestText);
    const entries = Option.match(limit, { onNone: () => manifest.diagrams, onSome: (n) => manifest.diagrams.slice(0, n) });
    const diagrams = yield* Effect.forEach(entries, (e) =>
      fs.readFileString(path.join(COMPAT_DIR, `${e.name}.mmd`)).pipe(Effect.map((source): Diagram => ({ name: e.name, source }))),
    );
    const names = new Set(diagrams.map((d) => d.name));
    const editFiles = (yield* fs.readDirectory(EDITS_DIR).pipe(Effect.orElseSucceed(() => [] as string[])))
      .filter((f) => f.endsWith(".json"))
      .sort();
    const edits = yield* Effect.forEach(editFiles, (f) =>
      fs.readFileString(path.join(EDITS_DIR, f)).pipe(Effect.flatMap(Schema.decodeUnknown(Schema.parseJson(EditPairFile)))),
    );
    return { manifest, diagrams, edits: edits.filter((e) => names.has(e.source)) };
  });

const runRenderer = (
  diagrams: readonly Diagram[],
  edits: readonly EditPairFile[],
  refGraphs: ReadonlyMap<string, ExtractedGraph>,
  opts: RunOptions,
) =>
  Effect.gen(function* () {
    const r = yield* Renderer;
    const fs = yield* FileSystem.FileSystem;
    const path = yield* Path.Path;
    const svgDir = path.join(RESULTS_DIR, "svgs", r.name);
    if (opts.outSvgs) yield* fs.makeDirectory(svgDir, { recursive: true });

    const graphs = new Map<string, ExtractedGraph>();
    const svgs = new Map<string, string>();
    const results: DiagramResult[] = [];
    const batch = r.renderBatch === undefined ? undefined : yield* r.renderBatch(diagrams);
    for (const [k, d] of diagrams.entries()) {
      const out =
        batch === undefined ? yield* r.render(d.source) : (batch.get(d.name) ?? failed("missing from batch render"));
      let metrics: DiagramResult["metrics"] = null;
      let compat: DiagramResult["compat"] = null;
      if (out.svg !== null) {
        const g = extractSvg(out.svg);
        graphs.set(d.name, g);
        svgs.set(d.name, out.svg);
        metrics = layoutMetrics(g);
        const ref = refGraphs.get(d.name);
        if (ref !== undefined) compat = compareGraphs(ref, g);
        if (opts.outSvgs) yield* fs.writeFileString(path.join(svgDir, `${d.name}.svg`), out.svg);
      }
      results.push({
        name: d.name,
        ok: out.svg !== null,
        error: out.error,
        ms: out.ms,
        fuelUsed: out.fuelUsed,
        svgBytes: out.svg === null ? 0 : Buffer.byteLength(out.svg, "utf8"),
        metrics,
        compat,
      });
      if ((k + 1) % 25 === 0 || k + 1 === diagrams.length) {
        const ok = results.filter((x) => x.ok).length;
        yield* Console.log(`  ${r.name}: ${k + 1}/${diagrams.length} (${ok} rendered)`);
      }
    }

    const editResults: EditResult[] = [];
    if (opts.edits) {
      for (const e of edits) {
        const before = yield* r.render(e.before);
        const after = before.svg === null ? before : yield* r.render(e.after, { hint: before.svg });
        if (before.svg === null || after.svg === null) {
          editResults.push({ name: e.name, kind: e.kind, ok: false, error: after.error ?? before.error, matched: 0, displacements: [] });
          continue;
        }
        const d = displacement(extractSvg(before.svg), extractSvg(after.svg));
        editResults.push({ name: e.name, kind: e.kind, ok: true, error: null, matched: d.matched, displacements: d.values });
      }
      yield* Console.log(`  ${r.name}: ${editResults.filter((x) => x.ok).length}/${edits.length} edit pairs rendered`);
    }
    const result: RendererResult = { renderer: r.name, version: r.version, diagrams: results, edits: editResults };
    return { result, graphs, svgs };
  });

interface WasmModule {
  readonly initSync: (bytes: Uint8Array) => void;
  readonly render: (source: string, options?: unknown) => { svg: string | null };
}

class DeterminismSkipped extends Schema.TaggedError<DeterminismSkipped>()("DeterminismSkipped", {
  reason: Schema.String,
}) {}

const skipped = (reason: string): Determinism => ({ status: "skipped", reason, compared: 0, identical: 0, differing: [] });

/**
 * Native vs WASM byte identity (specs/architecture.md#determinism), when the
 * WASM package has been built; skipped otherwise.
 */
const checkDeterminism = (diagrams: readonly Diagram[], native: ReadonlyMap<string, string> | undefined) =>
  Effect.gen(function* () {
    const fs = yield* FileSystem.FileSystem;
    const path = yield* Path.Path;
    if (native === undefined) return yield* new DeterminismSkipped({ reason: "merlion was not among the renderers" });
    const wasmPath = path.join(MERLION_WASM_DIR, "merlion.wasm");
    const indexPath = path.join(MERLION_WASM_DIR, "index.js");
    if (!(yield* fs.exists(wasmPath).pipe(Effect.orElseSucceed(() => false)))) {
      return yield* new DeterminismSkipped({ reason: "packages/merlion-wasm/merlion.wasm not built" });
    }
    const wasm = yield* Effect.tryPromise({
      try: async () => (await import(pathToFileURL(indexPath).href)) as WasmModule,
      catch: (e) => new DeterminismSkipped({ reason: `cannot load packages/merlion-wasm/index.js: ${String(e)}` }),
    });
    const bytes = yield* fs.readFile(wasmPath).pipe(Effect.mapError((e) => new DeterminismSkipped({ reason: String(e) })));
    yield* Effect.try({
      try: () => wasm.initSync(bytes),
      catch: (e) => new DeterminismSkipped({ reason: `initSync failed: ${String(e)}` }),
    });
    let compared = 0;
    let identical = 0;
    const differing: string[] = [];
    for (const d of diagrams) {
      const n = native.get(d.name);
      if (n === undefined) continue;
      const w = yield* Effect.try(() => wasm.render(d.source).svg).pipe(Effect.orElseSucceed(() => null));
      compared++;
      if (w === n) identical++;
      else differing.push(d.name);
    }
    return { status: "ran", reason: null, compared, identical, differing } satisfies Determinism;
  }).pipe(Effect.catchTag("DeterminismSkipped", (e) => Effect.succeed(skipped(e.reason))));

const gitCommit = Command.make("git", "rev-parse", "--short", "HEAD").pipe(
  Command.workingDirectory(REPO_DIR),
  Command.string,
  Effect.map(Str.trim),
  Effect.orElseSucceed(() => "unknown"),
);

export const run = (opts: RunOptions) =>
  Effect.gen(function* () {
    const fs = yield* FileSystem.FileSystem;
    const path = yield* Path.Path;
    const { manifest, diagrams, edits } = yield* loadCorpus(opts.limit);
    yield* Console.log(`corpus ${opts.corpus}: ${diagrams.length} diagrams, ${opts.edits ? edits.length : 0} edit pairs`);

    // The reference renders first so every other renderer can be compared against it.
    const order = [...opts.renderers].sort((a, b) => (a === REFERENCE ? -1 : b === REFERENCE ? 1 : 0));
    let refGraphs = new Map<string, ExtractedGraph>();
    let merlionSvgs: Map<string, string> | undefined;
    const results: RendererResult[] = [];
    for (const name of order) {
      yield* Console.log(`rendering with ${name}`);
      const out = yield* runRenderer(diagrams, edits, refGraphs, opts).pipe(
        Effect.provide(layerFor(name)),
        Effect.catchAll((e) =>
          // A renderer that cannot start (e.g. no browser) records every diagram as failed.
          Console.log(`  ${name} unavailable: ${String(e)}`).pipe(
            Effect.as({
              result: {
                renderer: name,
                version: "unavailable",
                diagrams: diagrams.map(
                  (d): DiagramResult => ({ name: d.name, ok: false, error: `renderer unavailable: ${String(e)}`, ms: 0, fuelUsed: null, svgBytes: 0, metrics: null, compat: null }),
                ),
                edits: [],
              } satisfies RendererResult,
              graphs: new Map<string, ExtractedGraph>(),
              svgs: new Map<string, string>(),
            }),
          ),
        ),
      );
      if (name === REFERENCE) refGraphs = out.graphs;
      if (name === "merlion") merlionSvgs = out.svgs;
      results.push(out.result);
    }

    const determinism = yield* checkDeterminism(diagrams, merlionSvgs);
    const date = new Date().toISOString().slice(0, 10);
    const commit = yield* gitCommit;
    const file: ResultsFile = {
      date,
      commit,
      corpus: opts.corpus,
      corpusCommit: manifest.commit,
      diagrams: diagrams.length,
      editPairs: opts.edits ? edits.length : 0,
      reference: REFERENCE,
      renderers: results,
      determinism,
    };
    const encoded = yield* Schema.encode(ResultsFile)(file);
    yield* fs.makeDirectory(RESULTS_DIR, { recursive: true });
    const out = path.join(RESULTS_DIR, `${date}-${commit}.json`);
    yield* fs.writeFileString(out, `${JSON.stringify(encoded)}\n`);
    yield* Console.log(`results: ${path.relative(process.cwd(), out)}`);
    return out;
  });

