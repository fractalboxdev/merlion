import { NodeContext } from "@effect/platform-node";
import { Effect } from "effect";
import { readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { generate } from "../src/authoring/generate.ts";
import { OUTPUTS_DIR } from "../src/authoring/tasks.ts";

/**
 * `--resume` writes the corpus file by replacing it, so anything it fails to
 * carry through is deleted. The bug this guards against turned 35 recorded
 * answers into 3, by rebuilding the output list from only the tasks the run
 * walked.
 */

const LABEL = "resume-regression-fixture";
const target = join(OUTPUTS_DIR, `${LABEL}.json`);

/** A port nothing listens on, so every call fails and no answer is replaced. */
const DEAD_ENDPOINT = "http://127.0.0.1:9/v1";

const answer = (task: string, format: "mermaid" | "svg") => ({
  task,
  format,
  model: "fixture-model",
  generated: "2026-09-24",
  text: format === "mermaid" ? "flowchart TD\n  a[A] --> b[B]\n" : '<svg xmlns="http://www.w3.org/2000/svg"></svg>',
  extracted: format === "mermaid" ? "flowchart TD\n  a[A] --> b[B]" : '<svg xmlns="http://www.w3.org/2000/svg"></svg>',
  error: null,
  truncated: false,
  usage: { outputTokens: 10, inputTokens: null, reasoningTokens: null },
});

const run = (opts: { limit: number | undefined }) =>
  Effect.runPromise(
    generate({ provider: "openai-compatible", model: "fixture-model", label: LABEL, limit: opts.limit, resume: true })
      .pipe(Effect.provide(NodeContext.layer)),
  );

let previousBaseUrl: string | undefined;

beforeEach(() => {
  previousBaseUrl = process.env["OPENAI_BASE_URL"];
  process.env["OPENAI_BASE_URL"] = DEAD_ENDPOINT;
});

afterEach(() => {
  if (previousBaseUrl === undefined) delete process.env["OPENAI_BASE_URL"];
  else process.env["OPENAI_BASE_URL"] = previousBaseUrl;
  rmSync(target, { force: true });
});

const writeFixture = (tasks: readonly string[], gap: string): void => {
  const outputs = tasks.flatMap((t) =>
    (["mermaid", "svg"] as const).filter((f) => `${t}/${f}` !== gap).map((f) => answer(t, f))
  );
  writeFileSync(
    target,
    JSON.stringify(
      { corpus: "authoring", model: "fixture-model", provider: "openai-compatible", generated: "2026-09-24", tasksVersion: 1, outputs },
      null,
      2,
    ),
  );
};

const recorded = (): Array<{ task: string; format: string; error: string | null }> =>
  (JSON.parse(readFileSync(target, "utf8")) as { outputs: Array<{ task: string; format: string; error: string | null }> }).outputs;

/** Each re-asked call fails three times with a widening gap before giving up. */
const TIMEOUT = 60_000;

describe("generate --resume", () => {
  it("keeps every recorded answer when the re-asked call fails", { timeout: TIMEOUT }, async () => {
    writeFixture(["ci-pipeline", "password-reset"], "password-reset/mermaid");
    expect(recorded()).toHaveLength(3);
    await run({ limit: 2 });
    const after = recorded();
    // The three kept answers survive; the gap is recorded as a failed call.
    expect(after.filter((o) => o.error === null)).toHaveLength(3);
    expect(after.filter((o) => o.error !== null && o.error !== undefined)).toHaveLength(1);
  });

  it("does not drop answers for tasks outside --limit", { timeout: TIMEOUT }, async () => {
    // `k8s-rollout` is the thirteenth task, well outside a limit of 1.
    writeFixture(["ci-pipeline", "k8s-rollout"], "ci-pipeline/mermaid");
    await run({ limit: 1 });
    const after = recorded();
    expect(after.filter((o) => o.task === "k8s-rollout")).toHaveLength(2);
  });

  it("refuses a file recorded from a different model", { timeout: TIMEOUT }, async () => {
    writeFixture(["ci-pipeline"], "");
    await expect(
      Effect.runPromise(
        generate({ provider: "openai-compatible", model: "another-model", label: LABEL, limit: 1, resume: true })
          .pipe(Effect.provide(NodeContext.layer)),
      ),
    ).rejects.toThrow(/fixture-model/);
  });
});
