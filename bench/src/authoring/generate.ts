/**
 * `bench authoring generate` (specs/benchmark.md#authoring).
 *
 * Asks one model for every task in both formats and records the answers in
 * `corpus/authoring/outputs/<model>.json`. Scoring never calls a model, so a
 * recorded file is the corpus: the same answers are re-scored whenever the
 * extractor or a metric changes, and the numbers move only when Merlion does.
 */
import { FileSystem, Path } from "@effect/platform";
import { Effect, Either, Option, Schedule, Schema } from "effect";
import { ask, type Provider } from "./provider.ts";
import { extractAnswer, promptFor } from "./prompt.ts";
import { FORMATS, loadTasks, type Output, OutputFile, OUTPUTS_DIR } from "./tasks.ts";

export interface GenerateOpts {
  readonly provider: Provider;
  readonly model: string;
  /** File stem under corpus/authoring/outputs/; defaults to the model name. */
  readonly label: string | undefined;
  readonly limit: number | undefined;
  /** Keep the answers already recorded and ask only for the ones missing or failed. */
  readonly resume: boolean;
}

const slug = (s: string): string => s.toLowerCase().replace(/[^a-z0-9.-]+/g, "-").replace(/^-|-$/g, "");

/**
 * A local server drops a long connection now and then. Three tries with a
 * widening gap turns that into a delay rather than a hole in the corpus.
 */
const RETRY = Schedule.exponential("2 seconds").pipe(Schedule.intersect(Schedule.recurs(2)));

export const generate = (opts: GenerateOpts) =>
  Effect.gen(function* () {
    const fs = yield* FileSystem.FileSystem;
    const path = yield* Path.Path;
    const file = yield* loadTasks;
    const tasks = opts.limit === undefined ? file.tasks : file.tasks.slice(0, opts.limit);
    const generated = new Date().toISOString().slice(0, 10);

    const outputs: Output[] = [];
    /** What the provider says it ran, which an alias does not name. */
    const resolved = new Set<string>();
    yield* fs.makeDirectory(OUTPUTS_DIR, { recursive: true });
    const target = path.join(OUTPUTS_DIR, `${slug(opts.label ?? opts.model)}.json`);

    /**
     * Answers already recorded and worth keeping: `--resume` re-asks only the
     * rest. Every kept answer is carried into the new file, including ones for
     * tasks outside `--limit`, because writing the file is a replacement and a
     * kept answer left out of it is a deleted one.
     */
    const keep = new Map<string, Output>();
    if (opts.resume && (yield* fs.exists(target))) {
      const existing = yield* Schema.decodeUnknown(Schema.parseJson(OutputFile))(yield* fs.readFileString(target));
      // Resuming onto answers generated from a different task file, or by a
      // different model, would merge two corpora under one name.
      if (existing.tasksVersion !== file.version) {
        return yield* Effect.die(
          new Error(
            `${target} holds answers to task file version ${existing.tasksVersion}, but tasks.json is version ${file.version}. Generate afresh rather than resuming.`,
          ),
        );
      }
      const asked = existing.requestedModel ?? existing.model;
      if (asked !== opts.model && existing.model !== opts.model) {
        return yield* Effect.die(
          new Error(`${target} holds ${existing.model}'s answers, not ${opts.model}'s. Use a different --label.`),
        );
      }
      if (existing.provider !== opts.provider) {
        return yield* Effect.die(
          new Error(`${target} was recorded through ${existing.provider}, not ${opts.provider}. Use a different --label.`),
        );
      }
      for (const o of existing.outputs) {
        if (o.error !== null && o.error !== undefined) continue;
        if (o.truncated === true) continue;
        keep.set(`${o.task}/${o.format}`, o);
        if (o.model !== "") resolved.add(o.model);
      }
      yield* Effect.log(`resuming: ${keep.size} answers kept, the rest re-asked`);
    }

    // Written after every answer. A run is dozens of slow calls, and a crash
    // thirty answers in should cost the next one, not all thirty.
    const save = Effect.suspend(() => {
      const ran = resolved.size === 1 ? [...resolved][0] ?? opts.model : opts.model;
      return fs.writeFileString(
        target,
        `${
          JSON.stringify(
            {
              corpus: "authoring" as const,
              model: ran,
              ...(ran === opts.model ? {} : { requestedModel: opts.model }),
              provider: opts.provider,
              generated,
              tasksVersion: file.version,
              outputs,
            },
            null,
            2,
          )
        }\n`,
      );
    });

    for (const task of tasks) {
      for (const format of FORMATS) {
        const already = keep.get(`${task.name}/${format}`);
        if (already !== undefined) {
          outputs.push(already);
          keep.delete(`${task.name}/${format}`);
          continue;
        }
        const answer = yield* ask(opts.provider, opts.model, promptFor(task, format)).pipe(
          Effect.retry(RETRY),
          Effect.tapError((e) => Effect.logWarning(`${task.name}/${format}: ${e.message}`)),
          Effect.either,
        );
        const got = Either.getRight(answer);
        if (Option.isNone(got)) {
          // Recorded, not dropped, so the report can say a call never returned.
          outputs.push({
            task: task.name,
            format,
            model: opts.model,
            generated,
            text: "",
            extracted: null,
            error: Either.getLeft(answer).pipe(Option.map((e) => e.message), Option.getOrElse(() => "the call failed")),
            truncated: false,
            usage: { outputTokens: null, inputTokens: null, reasoningTokens: null },
          });
          yield* save;
          continue;
        }
        const { text, outputTokens, inputTokens, reasoningTokens, truncated } = got.value;
        if (got.value.model !== null) resolved.add(got.value.model);
        outputs.push({
          task: task.name,
          format,
          model: got.value.model ?? opts.model,
          generated,
          text,
          extracted: extractAnswer(text, format),
          error: null,
          truncated,
          usage: { outputTokens, inputTokens, reasoningTokens },
        });
        const thinking = reasoningTokens === null || reasoningTokens === undefined ? "" : ` (${reasoningTokens} reasoning)`;
        const clipped = truncated ? " — TRUNCATED at the token cap" : "";
        yield* save;
        yield* Effect.log(`${task.name}/${format}: ${outputTokens ?? "?"} output tokens${thinking}${clipped}`);
      }
    }

    // Answers this run never reached — a task outside `--limit`, or one the
    // task file no longer lists — are kept rather than dropped.
    for (const held of keep.values()) outputs.push(held);
    yield* save;
    yield* Effect.log(`recorded ${outputs.length} answers → ${target}`);
    return target;
  });
