/**
 * Merlion through its native CLI (`target/release/merlion`, built with
 * `cargo build --release -p merlion-cli`), specs/integrations.md#cli.
 *
 * A corpus renders in one process with `merlion render --batch in -o out
 * --json-summary`: one JSON line per file with the render time the CLI
 * measures around the core call, so time excludes process start-up. A hinted
 * render (edit pairs) runs one process per diagram, `merlion render --json
 * --hint <prev.svg>`, with the source on stdin; its time includes start-up.
 */
import { Command, CommandExecutor, FileSystem, Path } from "@effect/platform";
import { Duration, Effect, Layer, Option, Schema, Stream, String as Str } from "effect";
import { MERLION_BIN } from "../paths.ts";
import { MerlionJson } from "../schema.ts";
import { type BatchDiagram, failed, RENDER_TIMEOUT, type RenderOpts, type RenderOutcome, Renderer } from "./Renderer.ts";

const collect = (s: Stream.Stream<Uint8Array, unknown>) =>
  s.pipe(Stream.decodeText(), Stream.runFold("", (acc, chunk) => acc + chunk));

const decodeJson = Schema.decodeUnknown(Schema.parseJson(MerlionJson));

/** One line of `merlion render --batch --json-summary`. */
const BatchLine = Schema.Struct({
  file: Schema.String,
  ok: Schema.Boolean,
  micros: Schema.Number,
  fuel_used: Schema.Number,
  diagnostics: Schema.Array(Schema.Struct({ severity: Schema.String, code: Schema.String, message: Schema.String })),
  error: Schema.NullOr(Schema.Struct({ kind: Schema.String, header: Schema.optional(Schema.String), what: Schema.optional(Schema.String) })),
});
const decodeBatchLine = Schema.decodeUnknownOption(Schema.parseJson(BatchLine));

export interface BatchEntry {
  readonly ok: boolean;
  readonly ms: number;
  readonly fuelUsed: number;
  readonly error: string | null;
}

/**
 * Parses `--json-summary` output into entries keyed by file stem. The error
 * text is the error kind followed by the first error diagnostic, else the
 * kind's detail. Lines that are not summary JSON are skipped.
 */
export const parseBatchSummary = (stdout: string): Map<string, BatchEntry> => {
  const out = new Map<string, BatchEntry>();
  for (const raw of stdout.split("\n")) {
    const line = Option.getOrUndefined(decodeBatchLine(raw));
    if (line === undefined) continue;
    const base = line.file.split("/").pop() ?? line.file;
    const stem = base.endsWith(".mmd") ? base.slice(0, -4) : base;
    const errorText = Option.match(Option.fromNullable(line.error), {
      onNone: () => (line.ok ? null : "no SVG"),
      onSome: (e) => {
        const diag = line.diagnostics.find((d) => d.severity === "error");
        const detail = diag !== undefined ? `${diag.code} ${diag.message}` : (e.header ?? e.what);
        return detail === undefined ? e.kind : `${e.kind}: ${detail}`;
      },
    });
    out.set(stem, { ok: line.ok, ms: line.micros / 1000, fuelUsed: line.fuel_used, error: errorText });
  }
  return out;
};

export const MerlionLive = Layer.scoped(
  Renderer,
  Effect.gen(function* () {
    const fs = yield* FileSystem.FileSystem;
    const path = yield* Path.Path;
    const executor = yield* CommandExecutor.CommandExecutor;
    const present = yield* fs.exists(MERLION_BIN).pipe(Effect.orElseSucceed(() => false));
    const version = present
      ? yield* Command.make(MERLION_BIN, "--version").pipe(
          // Closed stdin: a CLI without --version must not wait for input.
          Command.feed(""),
          Command.string,
          Effect.timeout(RENDER_TIMEOUT),
          Effect.map((s) => Str.trim(s).split("\n")[0] ?? ""),
          Effect.map((s) => (s.startsWith("<") || s === "" ? "unknown" : s)),
          Effect.orElseSucceed(() => "unknown"),
        )
      : "missing";
    const tmp = yield* fs.makeTempDirectoryScoped({ prefix: "merlion-bench-" });

    const render = (source: string, opts?: RenderOpts): Effect.Effect<RenderOutcome> =>
      Effect.gen(function* () {
        if (!present) return failed(`${path.basename(MERLION_BIN)} not built (cargo build --release -p merlion-cli)`);
        const args = ["render", "--json"];
        if (opts?.hint !== undefined) {
          // The CLI reads a hint only from under its working directory, which is `tmp`.
          yield* fs.writeFileString(path.join(tmp, "hint.svg"), opts.hint);
          args.push("--hint", "hint.svg");
        }
        const t0 = performance.now();
        const run = Effect.scoped(
          Effect.gen(function* () {
            const proc = yield* Command.make(MERLION_BIN, ...args).pipe(
              Command.workingDirectory(tmp),
              Command.feed(source),
              Command.start,
            );
            const [stdout, stderr, code] = yield* Effect.all([collect(proc.stdout), collect(proc.stderr), proc.exitCode], {
              concurrency: 3,
            });
            return { stdout, stderr, code: Number(code) };
          }),
        );
        const outcome = yield* run.pipe(Effect.timeout(RENDER_TIMEOUT), Effect.option);
        if (Option.isNone(outcome)) return failed(`timeout after ${Duration.format(RENDER_TIMEOUT)}`, Duration.toMillis(RENDER_TIMEOUT));
        const { stdout, stderr, code } = outcome.value;
        const ms = performance.now() - t0;
        return yield* decodeJson(stdout).pipe(
          Effect.map(
            (j): RenderOutcome => ({
              svg: j.svg,
              error: j.svg === null ? (j.error ?? `exit ${code}`) : null,
              ms,
              fuelUsed: j.fuel_used ?? null,
            }),
          ),
          Effect.orElseSucceed(() => failed(`exit ${code}: no JSON on stdout${stderr.trim() === "" ? "" : `; ${stderr.trim().slice(0, 200)}`}`, ms)),
        );
      }).pipe(
        Effect.provideService(CommandExecutor.CommandExecutor, executor),
        Effect.catchAll((e) => Effect.succeed(failed(`spawn failed: ${String(e)}`))),
      );

    const renderBatch = (diagrams: readonly BatchDiagram[]): Effect.Effect<ReadonlyMap<string, RenderOutcome>> =>
      Effect.gen(function* () {
        const outcomes = new Map<string, RenderOutcome>();
        if (!present) {
          for (const d of diagrams) outcomes.set(d.name, failed(`${path.basename(MERLION_BIN)} not built (cargo build --release -p merlion-cli)`));
          return outcomes;
        }
        // The CLI writes only under its working directory, and reads an existing
        // output as a hint, so each batch runs in a fresh directory.
        const dir = yield* fs.makeTempDirectory({ directory: tmp, prefix: "batch-" });
        yield* fs.makeDirectory(path.join(dir, "in"));
        yield* fs.makeDirectory(path.join(dir, "out"));
        yield* Effect.forEach(diagrams, (d) => fs.writeFileString(path.join(dir, "in", `${d.name}.mmd`), d.source), { discard: true });
        const stdout = yield* Effect.scoped(
          Effect.gen(function* () {
            const proc = yield* Command.make(MERLION_BIN, "render", "--batch", "in", "-o", "out", "--json-summary").pipe(
              Command.workingDirectory(dir),
              Command.start,
            );
            const [text] = yield* Effect.all([collect(proc.stdout), collect(proc.stderr), proc.exitCode], { concurrency: 3 });
            return text;
          }),
        );
        const summary = parseBatchSummary(stdout);
        for (const d of diagrams) {
          const entry = summary.get(d.name);
          if (entry === undefined) {
            outcomes.set(d.name, failed("no summary line from merlion render --batch"));
            continue;
          }
          const svg = entry.ok
            ? yield* fs.readFileString(path.join(dir, "out", `${d.name}.svg`)).pipe(Effect.orElseSucceed(() => null))
            : null;
          outcomes.set(d.name, {
            svg,
            error: svg === null ? (entry.error ?? "SVG not written") : null,
            ms: entry.ms,
            fuelUsed: entry.fuelUsed,
          });
        }
        yield* fs.remove(dir, { recursive: true }).pipe(Effect.ignore);
        return outcomes;
      }).pipe(
        Effect.provideService(CommandExecutor.CommandExecutor, executor),
        Effect.catchAll((e) =>
          Effect.succeed(new Map(diagrams.map((d) => [d.name, failed(`batch failed: ${String(e)}`)] as const))),
        ),
      );

    return Renderer.of({ name: "merlion", version: `merlion ${version}`, render, renderBatch });
  }),
);
