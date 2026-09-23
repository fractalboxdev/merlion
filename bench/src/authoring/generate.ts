/**
 * `bench authoring generate` (specs/benchmark.md#authoring).
 *
 * Asks one model for every task in both formats and records the answers in
 * `corpus/authoring/outputs/<model>.json`. Scoring never calls a model, so a
 * recorded file is the corpus: the same answers are re-scored whenever the
 * extractor or a metric changes, and the numbers move only when Merlion does.
 */
import { FileSystem, Path } from "@effect/platform";
import { Effect, Option } from "effect";
import { ask, type Provider } from "./provider.ts";
import { extractAnswer, promptFor } from "./prompt.ts";
import { FORMATS, loadTasks, type Output, OUTPUTS_DIR } from "./tasks.ts";

export interface GenerateOpts {
  readonly provider: Provider;
  readonly model: string;
  /** File stem under corpus/authoring/outputs/; defaults to the model name. */
  readonly label: string | undefined;
  readonly limit: number | undefined;
}

const slug = (s: string): string => s.toLowerCase().replace(/[^a-z0-9.-]+/g, "-").replace(/^-|-$/g, "");

export const generate = (opts: GenerateOpts) =>
  Effect.gen(function* () {
    const fs = yield* FileSystem.FileSystem;
    const path = yield* Path.Path;
    const file = yield* loadTasks;
    const tasks = opts.limit === undefined ? file.tasks : file.tasks.slice(0, opts.limit);
    const generated = new Date().toISOString().slice(0, 10);

    const outputs: Output[] = [];
    for (const task of tasks) {
      for (const format of FORMATS) {
        const answer = yield* ask(opts.provider, opts.model, promptFor(task, format)).pipe(
          Effect.tapError((e) => Effect.logWarning(`${task.name}/${format}: ${e.message}`)),
          Effect.option,
        );
        const got = Option.getOrNull(answer);
        if (got === null) continue;
        const { text, outputTokens, inputTokens } = got;
        outputs.push({
          task: task.name,
          format,
          model: opts.model,
          generated,
          text,
          extracted: extractAnswer(text, format),
          usage: { outputTokens, inputTokens },
        });
        yield* Effect.log(`${task.name}/${format}: ${outputTokens ?? "?"} output tokens`);
      }
    }

    yield* fs.makeDirectory(OUTPUTS_DIR, { recursive: true });
    const target = path.join(OUTPUTS_DIR, `${slug(opts.label ?? opts.model)}.json`);
    const body = {
      corpus: "authoring" as const,
      model: opts.model,
      provider: opts.provider,
      generated,
      tasksVersion: file.version,
      outputs,
    };
    yield* fs.writeFileString(target, `${JSON.stringify(body, null, 2)}\n`);
    yield* Effect.log(`recorded ${outputs.length} answers → ${target}`);
    return target;
  });
