/**
 * The two prompts (specs/benchmark.md#authoring).
 *
 * Everything but the output format is held equal, including the quality
 * requirement. Asking only the SVG side for a readable drawing would be a
 * loaded comparison; asking both is the point, because the requirement is what
 * a layout engine satisfies for the Mermaid side and what the model has to
 * satisfy itself for the SVG side.
 */
import type { Format, Task } from "./tasks.ts";

/** Held equal across formats: what the diagram has to show and how good it has to be. */
const common = (task: Task): string =>
  `Draw this diagram.

${task.prompt}

The drawing must be readable: every label sits inside the shape it names, no two shapes overlap, and every connection the description states is drawn as an arrow between the right two shapes, with the condition on it where the description gives one.`;

const FORMAT_INSTRUCTION: Record<Format, string> = {
  mermaid:
    "Answer with a Mermaid flowchart and nothing else: one ```mermaid fenced block, no explanation before or after it.",
  svg:
    "Answer with an SVG and nothing else: one ```svg fenced block, no explanation before or after it. The SVG is standalone — an `xmlns`, a `viewBox`, no external stylesheet, font file or image.",
};

export const promptFor = (task: Task, format: Format): string => `${common(task)}\n\n${FORMAT_INSTRUCTION[format]}\n`;

const FENCE_LANGUAGES: Record<Format, readonly string[]> = {
  mermaid: ["mermaid", "mmd"],
  svg: ["svg", "xml", "html"],
};

/**
 * The diagram inside a model's answer: the first fenced block in one of the
 * format's languages, else the first fenced block of any language whose body
 * looks like the format, else the whole answer when it is the format unfenced.
 */
export const extractAnswer = (text: string, format: Format): string | null => {
  const blocks = [...text.matchAll(/```([A-Za-z0-9_+-]*)[^\S\n]*\n([\s\S]*?)```/g)].map((m) => ({
    lang: (m[1] ?? "").toLowerCase(),
    body: (m[2] ?? "").trim(),
  }));
  const looksRight = (body: string): boolean =>
    format === "svg" ? /<svg[\s>]/i.test(body) : /^\s*(flowchart|graph)\b/im.test(body);

  const tagged = blocks.find((b) => FENCE_LANGUAGES[format].includes(b.lang) && b.body !== "");
  if (tagged !== undefined) return trimToFormat(tagged.body, format);
  const guessed = blocks.find((b) => looksRight(b.body));
  if (guessed !== undefined) return trimToFormat(guessed.body, format);
  const bare = text.trim();
  return bare !== "" && looksRight(bare) ? trimToFormat(bare, format) : null;
};

/** An SVG answer may carry a prologue or trailing prose; keep the root element. */
const trimToFormat = (body: string, format: Format): string => {
  if (format !== "svg") return body;
  const start = body.search(/<svg[\s>]/i);
  const end = body.lastIndexOf("</svg>");
  return start >= 0 && end > start ? body.slice(start, end + "</svg>".length) : body;
};
