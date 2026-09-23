/**
 * Where a model answer comes from (specs/benchmark.md#authoring).
 *
 * Generation is separate from scoring: `bench authoring generate` records
 * answers into `corpus/authoring/outputs/`, and every later `score` run reads
 * those files. A committed answer keeps the benchmark reproducible for anyone
 * without an account, and keeps a model's sampling out of the numbers a change
 * to Merlion is judged on.
 */
import { Command, CommandExecutor } from "@effect/platform";
import { Effect, Schema } from "effect";

export const PROVIDERS = ["anthropic", "claude-cli"] as const;
export const Provider = Schema.Literal(...PROVIDERS);
export type Provider = typeof Provider.Type;

export interface Answer {
  readonly text: string;
  readonly outputTokens: number | null;
  readonly inputTokens: number | null;
}

export class ProviderError extends Schema.TaggedError<ProviderError>()("ProviderError", {
  provider: Schema.String,
  message: Schema.String,
}) {}

/** Sampling is off: the same prompt gives the same answer as far as the provider allows. */
const TEMPERATURE = 0;
const MAX_TOKENS = 8192;

// ---------------------------------------------------------------------------

const AnthropicResponse = Schema.Struct({
  content: Schema.Array(Schema.Struct({ type: Schema.String, text: Schema.optional(Schema.String) })),
  usage: Schema.optional(Schema.Struct({
    input_tokens: Schema.optional(Schema.Number),
    output_tokens: Schema.optional(Schema.Number),
  })),
});

/** The Messages API, keyed by `ANTHROPIC_API_KEY`. Reports the model's own token counts. */
const anthropic = (model: string, prompt: string): Effect.Effect<Answer, ProviderError> =>
  Effect.gen(function* () {
    const key = process.env["ANTHROPIC_API_KEY"];
    if (key === undefined || key === "") {
      return yield* new ProviderError({ provider: "anthropic", message: "ANTHROPIC_API_KEY is not set" });
    }
    const body = yield* Effect.tryPromise({
      try: async () => {
        const res = await fetch("https://api.anthropic.com/v1/messages", {
          method: "POST",
          headers: { "content-type": "application/json", "x-api-key": key, "anthropic-version": "2023-06-01" },
          body: JSON.stringify({ model, max_tokens: MAX_TOKENS, temperature: TEMPERATURE, messages: [{ role: "user", content: prompt }] }),
        });
        if (!res.ok) throw new Error(`${res.status} ${await res.text()}`);
        return (await res.json()) as unknown;
      },
      catch: (e) => new ProviderError({ provider: "anthropic", message: e instanceof Error ? e.message : String(e) }),
    });
    const parsed = yield* Schema.decodeUnknown(AnthropicResponse)(body).pipe(
      Effect.mapError((e) => new ProviderError({ provider: "anthropic", message: String(e) })),
    );
    return {
      text: parsed.content.filter((c) => c.type === "text").map((c) => c.text ?? "").join(""),
      outputTokens: parsed.usage?.output_tokens ?? null,
      inputTokens: parsed.usage?.input_tokens ?? null,
    };
  });

// ---------------------------------------------------------------------------

const ClaudeCliResponse = Schema.Struct({
  result: Schema.optional(Schema.String),
  is_error: Schema.optional(Schema.Boolean),
  usage: Schema.optional(Schema.Struct({
    input_tokens: Schema.optional(Schema.Number),
    output_tokens: Schema.optional(Schema.Number),
  })),
});

/**
 * The `claude` CLI in print mode, which needs no API key. Its input token
 * count covers the CLI's own system prompt and tool definitions as well as the
 * task, so only the output count is comparable with the API provider's.
 */
const claudeCli = (
  model: string,
  prompt: string,
): Effect.Effect<Answer, ProviderError, CommandExecutor.CommandExecutor> =>
  Effect.gen(function* () {
    const executor = yield* CommandExecutor.CommandExecutor;
    const fail = (message: string) => new ProviderError({ provider: "claude-cli", message });
    const cmd = Command.make("claude", "-p", "--output-format", "json", "--model", model, prompt);
    const out = yield* executor.string(cmd).pipe(Effect.mapError((e) => fail(String(e))));
    const parsed = yield* Schema.decodeUnknown(Schema.parseJson(ClaudeCliResponse))(out.trim()).pipe(
      Effect.mapError((e) => fail(String(e))),
    );
    if (parsed.is_error === true) return yield* fail(parsed.result ?? "claude reported an error");
    return {
      text: parsed.result ?? "",
      outputTokens: parsed.usage?.output_tokens ?? null,
      inputTokens: null,
    };
  });

export const ask = (
  provider: Provider,
  model: string,
  prompt: string,
): Effect.Effect<Answer, ProviderError, CommandExecutor.CommandExecutor> =>
  provider === "anthropic" ? anthropic(model, prompt) : claudeCli(model, prompt);
