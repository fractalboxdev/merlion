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
import { request as httpRequest } from "node:http";
import { request as httpsRequest } from "node:https";

export const PROVIDERS = ["anthropic", "claude-cli", "openai-compatible"] as const;
export const Provider = Schema.Literal(...PROVIDERS);
export type Provider = typeof Provider.Type;

export interface Answer {
  readonly text: string;
  /** The model the provider actually ran, which an alias like `sonnet` does not name. */
  readonly model: string | null;
  /** The provider stopped at the token cap, so the answer is a fragment. */
  readonly truncated: boolean;
  /** Every token the model emitted, reasoning included; this is what a caller pays for. */
  readonly outputTokens: number | null;
  readonly inputTokens: number | null;
  /** Of those, the ones spent thinking rather than answering, where the provider separates them. */
  readonly reasoningTokens: number | null;
  /**
   * Assistant turns the provider took to answer, where it runs an agent rather
   * than a single completion. Above one, the token counts cover the run.
   */
  readonly turns: number | null;
}

export class ProviderError extends Schema.TaggedError<ProviderError>()("ProviderError", {
  provider: Schema.String,
  message: Schema.String,
}) {}

/** Sampling is off: the same prompt gives the same answer as far as the provider allows. */
const TEMPERATURE = 0;
/** Generous enough that an SVG answer, and a reasoning model's thinking before it, both fit. */
const MAX_TOKENS = 32768;
/** LM Studio and every other OpenAI-compatible server, when `OPENAI_BASE_URL` is unset. */
const DEFAULT_OPENAI_BASE_URL = "http://127.0.0.1:1234/v1";

// ---------------------------------------------------------------------------

const AnthropicResponse = Schema.Struct({
  model: Schema.optional(Schema.String),
  stop_reason: Schema.optional(Schema.NullOr(Schema.String)),
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
      model: parsed.model ?? null,
      truncated: parsed.stop_reason === "max_tokens",
      outputTokens: parsed.usage?.output_tokens ?? null,
      inputTokens: parsed.usage?.input_tokens ?? null,
      reasoningTokens: null,
      turns: 1,
    };
  });

// ---------------------------------------------------------------------------

const ClaudeCliResponse = Schema.Struct({
  result: Schema.optional(Schema.String),
  is_error: Schema.optional(Schema.Boolean),
  stop_reason: Schema.optional(Schema.NullOr(Schema.String)),
  num_turns: Schema.optional(Schema.Number),
  modelUsage: Schema.optional(Schema.Record({ key: Schema.String, value: Schema.Unknown })),
  usage: Schema.optional(Schema.Struct({
    input_tokens: Schema.optional(Schema.Number),
    output_tokens: Schema.optional(Schema.Number),
    output_tokens_details: Schema.optional(Schema.Struct({
      thinking_tokens: Schema.optional(Schema.Number),
    })),
  })),
});

/**
 * The `claude` CLI in print mode, which needs no API key.
 *
 * Its counts are for the whole agentic run, not for the answer: the input side
 * carries the CLI's own system prompt and tool definitions, and the output side
 * sums every assistant turn the run took, while `result` holds only the last
 * one. A run that took several turns reports several times the tokens its
 * answer contains, so this provider's output count measures what the run cost
 * and not what the diagram cost, and the benchmark compares sizes instead.
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
      // The CLI reports the resolved id as the sole key of `modelUsage`.
      model: Object.keys(parsed.modelUsage ?? {})[0] ?? null,
      truncated: parsed.stop_reason === "max_tokens",
      outputTokens: parsed.usage?.output_tokens ?? null,
      inputTokens: null,
      reasoningTokens: parsed.usage?.output_tokens_details?.thinking_tokens ?? null,
      turns: parsed.num_turns ?? null,
    };
  });

// ---------------------------------------------------------------------------

const OpenAiResponse = Schema.Struct({
  model: Schema.optional(Schema.String),
  choices: Schema.Array(Schema.Struct({
    message: Schema.Struct({
      content: Schema.NullOr(Schema.String),
      /** A reasoning model puts its thinking here and keeps `content` for the answer. */
      reasoning_content: Schema.optional(Schema.NullOr(Schema.String)),
    }),
    finish_reason: Schema.optional(Schema.NullOr(Schema.String)),
  })),
  usage: Schema.optional(Schema.Struct({
    prompt_tokens: Schema.optional(Schema.Number),
    completion_tokens: Schema.optional(Schema.Number),
    completion_tokens_details: Schema.optional(Schema.Struct({
      reasoning_tokens: Schema.optional(Schema.Number),
    })),
  })),
});

/** POSTs JSON and reads the whole response, with no time limit on the body. */
const post = (url: string, key: string, payload: string): Promise<unknown> =>
  new Promise((resolve, reject) => {
    const target = new URL(url);
    const send = target.protocol === "https:" ? httpsRequest : httpRequest;
    const req = send(
      target,
      {
        method: "POST",
        headers: {
          "content-type": "application/json",
          "content-length": Buffer.byteLength(payload),
          authorization: `Bearer ${key}`,
        },
        // No socket, headers or body deadline: the caller decides when to stop.
        timeout: 0,
      },
      (res) => {
        let text = "";
        res.setEncoding("utf8");
        res.on("data", (chunk: string) => {
          text += chunk;
        });
        res.on("end", () => {
          const status = res.statusCode ?? 0;
          if (status < 200 || status >= 300) {
            reject(new Error(`${status} ${text.slice(0, 500)}`));
            return;
          }
          try {
            resolve(JSON.parse(text) as unknown);
          } catch (e) {
            reject(e instanceof Error ? e : new Error(String(e)));
          }
        });
      },
    );
    req.on("error", reject);
    req.write(payload);
    req.end();
  });

/**
 * Any OpenAI-compatible chat endpoint: a local LM Studio, Ollama or vLLM
 * server, or a hosted one. `OPENAI_BASE_URL` points at it and `OPENAI_API_KEY`
 * authenticates where the server asks for it; a local server usually does not.
 *
 * `completion_tokens` counts a reasoning model's thinking as well as its
 * answer, so the reasoning count is recorded beside it and the report states
 * the diagram's own cost apart from the thinking that preceded it.
 */
const openAiCompatible = (model: string, prompt: string): Effect.Effect<Answer, ProviderError> =>
  Effect.gen(function* () {
    const base = (process.env["OPENAI_BASE_URL"] ?? DEFAULT_OPENAI_BASE_URL).replace(/\/$/, "");
    const key = process.env["OPENAI_API_KEY"] ?? "not-needed";
    const fail = (message: string) => new ProviderError({ provider: "openai-compatible", message });
    // `node:http` rather than `fetch`: undici caps a response body at 300 s,
    // and a local model writing an SVG for a large diagram runs well past that,
    // so fetch failed on exactly the biggest diagrams and left holes where the
    // interesting answers should have been.
    const body = yield* Effect.tryPromise({
      try: () => post(`${base}/chat/completions`, key, JSON.stringify({
        model,
        max_tokens: MAX_TOKENS,
        temperature: TEMPERATURE,
        messages: [{ role: "user", content: prompt }],
      })),
      catch: (e) => fail(e instanceof Error ? e.message : String(e)),
    });
    const parsed = yield* Schema.decodeUnknown(OpenAiResponse)(body).pipe(Effect.mapError((e) => fail(String(e))));
    const choice = parsed.choices[0];
    if (choice === undefined) return yield* fail("the server returned no choice");
    // A model that spends its whole budget thinking answers with empty content.
    const text = choice.message.content ?? "";
    if (text.trim() === "" && choice.finish_reason === "length") {
      return yield* fail("the answer hit the token limit before any content was emitted");
    }
    return {
      text,
      model: parsed.model ?? null,
      truncated: choice.finish_reason === "length",
      outputTokens: parsed.usage?.completion_tokens ?? null,
      inputTokens: parsed.usage?.prompt_tokens ?? null,
      reasoningTokens: parsed.usage?.completion_tokens_details?.reasoning_tokens ?? null,
      turns: 1,
    };
  });

export const ask = (
  provider: Provider,
  model: string,
  prompt: string,
): Effect.Effect<Answer, ProviderError, CommandExecutor.CommandExecutor> => {
  switch (provider) {
    case "anthropic":
      return anthropic(model, prompt);
    case "openai-compatible":
      return openAiCompatible(model, prompt);
    default:
      return claudeCli(model, prompt);
  }
};
