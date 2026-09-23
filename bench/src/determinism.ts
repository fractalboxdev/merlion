/**
 * `bench determinism`: native and WASM output are byte-identical for the same source
 * and options (specs/architecture.md#determinism).
 *
 * Renders every diagram of a corpus twice: natively in one `merlion render --batch` process
 * (into `target/determinism/<corpus>/<font>/`, with `--no-hint` so earlier output never acts
 * as a layout hint), and through `packages/merlion-wasm` loaded with `initSync`. Fails when
 * any diagram differs, including one that renders on one target and fails on the other.
 *
 * One corpus per diagram type on each side: `compat`, `compat-sequence` and `compat-state`
 * are the mermaid sources of the benchmark, and `sequence` and `state` are the core's own
 * fixtures.
 */
import { Command, FileSystem, Path } from "@effect/platform";
import { Console, Effect, Schema } from "effect";
import { pathToFileURL } from "node:url";
import {
  COMPAT_DIR,
  COMPAT_SEQUENCE_DIR,
  COMPAT_STATE_DIR,
  MERLION_BIN,
  MERLION_WASM_DIR,
  REPO_DIR,
  SEQUENCE_DIR,
  STATE_DIR,
} from "./paths.ts";

export const FONT_MODES = ["link", "embed", "system"] as const;
export type FontMode = (typeof FONT_MODES)[number];

/** The directories the check renders, one per corpus name. */
export const CORPORA = {
  compat: COMPAT_DIR,
  "compat-sequence": COMPAT_SEQUENCE_DIR,
  "compat-state": COMPAT_STATE_DIR,
  sequence: SEQUENCE_DIR,
  state: STATE_DIR,
} as const;
export const CORPUS_NAMES = Object.keys(CORPORA) as [Corpus, ...Corpus[]];
export type Corpus = keyof typeof CORPORA;

export interface Comparison {
  readonly compared: number;
  readonly identical: number;
  /** Diagrams neither target rendered (both returned no SVG). */
  readonly bothFailed: number;
  readonly differing: readonly string[];
}

/** Compares the two targets' SVG per diagram name; `null` is a failed render. */
export const compareRenders = (
  names: readonly string[],
  native: ReadonlyMap<string, string | null>,
  wasm: ReadonlyMap<string, string | null>,
): Comparison => {
  let identical = 0;
  let bothFailed = 0;
  const differing: string[] = [];
  for (const name of names) {
    const n = native.get(name);
    const w = wasm.get(name);
    if (n === undefined || w === undefined) differing.push(name);
    else if (n === null && w === null) bothFailed++;
    else if (n === w) identical++;
    else differing.push(name);
  }
  return { compared: names.length, identical, bothFailed, differing };
};

export const summaryLine = (c: Comparison, corpus: Corpus, font: FontMode): string =>
  `determinism (${corpus}, font ${font}): ${c.identical}/${c.compared} byte-identical, ${c.bothFailed} failed on both targets, ${c.differing.length} differing`;

export class DeterminismFailed extends Schema.TaggedError<DeterminismFailed>()("DeterminismFailed", {
  message: Schema.String,
}) {}

interface WasmModule {
  readonly initSync: (bytes: Uint8Array) => void;
  readonly render: (source: string, options?: { font?: FontMode }) => { svg: string | null };
}

export const determinism = (corpus: Corpus, font: FontMode) =>
  Effect.gen(function* () {
    const fs = yield* FileSystem.FileSystem;
    const path = yield* Path.Path;
    const fail = (message: string) => new DeterminismFailed({ message });

    const wasmPath = path.join(MERLION_WASM_DIR, "merlion.wasm");
    if (!(yield* fs.exists(MERLION_BIN))) return yield* fail("target/release/merlion not built (cargo build --release -p merlion-cli)");
    if (!(yield* fs.exists(wasmPath))) return yield* fail("packages/merlion-wasm/merlion.wasm not built (sh packages/merlion-wasm/scripts/build-wasm.sh)");

    const dir = CORPORA[corpus];
    const files = (yield* fs.readDirectory(dir)).filter((f) => f.endsWith(".mmd")).sort();
    const names = files.map((f) => f.slice(0, -".mmd".length));

    // The CLI writes only inside its working directory, so the output lives in the repo's target/.
    const outDir = path.join(REPO_DIR, "target", "determinism", corpus, font);
    yield* fs.remove(outDir, { recursive: true }).pipe(Effect.ignore);
    yield* fs.makeDirectory(outDir, { recursive: true });
    // Exit code 1 only means some diagrams failed to render; failures are compared too.
    yield* Command.make(MERLION_BIN, "render", "--batch", dir, "-o", path.relative(REPO_DIR, outDir), "--no-hint", "--font", font).pipe(
      Command.workingDirectory(REPO_DIR),
      Command.stderr("inherit"),
      Command.exitCode,
    );
    const native = new Map<string, string | null>();
    for (const name of names) {
      const svg = path.join(outDir, `${name}.svg`);
      native.set(name, (yield* fs.exists(svg)) ? yield* fs.readFileString(svg) : null);
    }

    const wasm = yield* Effect.tryPromise({
      try: async () => (await import(pathToFileURL(path.join(MERLION_WASM_DIR, "index.js")).href)) as WasmModule,
      catch: (e) => fail(`cannot load packages/merlion-wasm/index.js: ${String(e)}`),
    });
    const bytes = yield* fs.readFile(wasmPath);
    yield* Effect.try({ try: () => wasm.initSync(bytes), catch: (e) => fail(`initSync failed: ${String(e)}`) });
    const viaWasm = new Map<string, string | null>();
    for (const [i, name] of names.entries()) {
      const source = yield* fs.readFileString(path.join(dir, files[i] ?? ""));
      viaWasm.set(name, wasm.render(source, { font }).svg);
    }

    const result = compareRenders(names, native, viaWasm);
    yield* Console.log(summaryLine(result, corpus, font));
    if (result.differing.length > 0) return yield* fail(`differing: ${result.differing.join(", ")}`);
    if (result.identical === 0) return yield* fail("no diagram rendered on both targets");
    return result;
  });
