/**
 * Merlion through its native CLI (`target/release/merlion`, built with
 * `cargo build --release -p merlion-cli`), one process per diagram:
 * `merlion render --json [--hint <prev.svg>]` with the source on stdin and
 * `{svg, outline, diagnostics, fuel_used, error}` on stdout
 * (specs/integrations.md#cli). Time includes process start-up.
 */
import { Command, CommandExecutor, FileSystem, Path } from "@effect/platform";
import { Duration, Effect, Layer, Option, Schema, Stream, String as Str } from "effect";
import { MERLION_BIN } from "../paths.ts";
import { MerlionJson } from "../schema.ts";
import { failed, RENDER_TIMEOUT, type RenderOpts, type RenderOutcome, Renderer } from "./Renderer.ts";

const collect = (s: Stream.Stream<Uint8Array, unknown>) =>
  s.pipe(Stream.decodeText(), Stream.runFold("", (acc, chunk) => acc + chunk));

const decodeJson = Schema.decodeUnknown(Schema.parseJson(MerlionJson));

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
          const hintPath = path.join(tmp, "hint.svg");
          yield* fs.writeFileString(hintPath, opts.hint);
          args.push("--hint", hintPath);
        }
        const t0 = performance.now();
        const run = Effect.scoped(
          Effect.gen(function* () {
            const proc = yield* Command.make(MERLION_BIN, ...args).pipe(Command.feed(source), Command.start);
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

    return Renderer.of({ name: "merlion", version: `merlion ${version}`, render });
  }),
);
