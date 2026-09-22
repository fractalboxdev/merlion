/**
 * `bench fetch`: downloads the `compat`, `compat-sequence` and `compat-state` corpora
 * from the mermaid repository at the pinned commit and derives the `edits` corpus from
 * the flowcharts (specs/benchmark.md#corpora, specs/licensing.md rule 5).
 */
import { FileSystem, HttpClient, HttpClientRequest, Path } from "@effect/platform";
import { createHash } from "node:crypto";
import { Config, Console, Effect, Option, Redacted, Schema } from "effect";
import { COMPAT_DIR, COMPAT_SEQUENCE_DIR, COMPAT_STATE_DIR, EDITS_DIR } from "../paths.ts";
import { EditPairFile, Manifest, ManifestEntry, MERMAID_COMMIT, MERMAID_REPO, MERMAID_TAG } from "../schema.ts";
import { makeEdits } from "./edits.ts";
import { type DiagramKind, extractDiagrams, selectSourcePaths, sourceSlug } from "./sources.ts";

/** Diagrams the `edits` corpus is derived from: the first eligible compat diagrams by name. */
export const EDIT_SOURCES = 30;

const GitTree = Schema.Struct({
  truncated: Schema.Boolean,
  tree: Schema.Array(Schema.Struct({ path: Schema.String, type: Schema.String, sha: Schema.String })),
});

export class FetchError extends Schema.TaggedError<FetchError>()("FetchError", { message: Schema.String }) {}

const sha256 = (s: string): string => createHash("sha256").update(s, "utf8").digest("hex");

export const fetchCorpus = Effect.gen(function* () {
  const fs = yield* FileSystem.FileSystem;
  const path = yield* Path.Path;
  const token = yield* Config.option(Config.redacted("GITHUB_TOKEN"));
  const client = (yield* HttpClient.HttpClient).pipe(HttpClient.filterStatusOk);

  const treeUrl = `https://api.github.com/repos/${MERMAID_REPO}/git/trees/${MERMAID_COMMIT}?recursive=1`;
  const treeReq = HttpClientRequest.get(treeUrl).pipe(
    HttpClientRequest.setHeader("accept", "application/vnd.github+json"),
    (r) => Option.match(token, { onNone: () => r, onSome: (t) => HttpClientRequest.bearerToken(r, Redacted.value(t)) }),
  );
  const treeJson = yield* client.execute(treeReq).pipe(Effect.flatMap((r) => r.json));
  const tree = yield* Schema.decodeUnknown(GitTree)(treeJson);
  if (tree.truncated) return yield* new FetchError({ message: "GitHub returned a truncated tree listing" });
  const blobs = new Map(tree.tree.filter((t) => t.type === "blob").map((t) => [t.path, t.sha]));

  const fetchText = (src: string) =>
    client
      .get(`https://raw.githubusercontent.com/${MERMAID_REPO}/${MERMAID_COMMIT}/${src}`)
      .pipe(
        Effect.flatMap((r) => r.text),
        Effect.map((text) => [src, text] as const),
      );

  /** Extracts one kind, de-duplicated by content (first occurrence wins, sources sorted), named. */
  const collect = (kind: DiagramKind, contents: ReadonlyArray<readonly [string, string]>) => {
    const seen = new Set<string>();
    const names = new Set<string>();
    const out: Array<{ entry: ManifestEntry; text: string }> = [];
    for (const [src, content] of contents) {
      extractDiagrams(src, content, kind).forEach((text, k) => {
        const hash = sha256(text);
        if (seen.has(hash)) return;
        seen.add(hash);
        const base = src.endsWith(".mmd") ? sourceSlug(src) : `${sourceSlug(src)}-${String(k + 1).padStart(2, "0")}`;
        let name = base;
        for (let n = 2; names.has(name); n++) name = `${base}-${n}`;
        names.add(name);
        out.push({
          entry: new ManifestEntry({ name, source: src, block: k + 1, sourceBlob: blobs.get(src) ?? "", sha256: hash }),
          text,
        });
      });
    }
    out.sort((a, b) => (a.entry.name < b.entry.name ? -1 : a.entry.name > b.entry.name ? 1 : 0));
    return out;
  };

  /** Rewrites a corpus directory from scratch so removed diagrams do not linger. */
  const write = (
    corpus: "compat" | "compat-sequence" | "compat-state",
    dir: string,
    es: ReadonlyArray<{ entry: ManifestEntry; text: string }>,
  ) =>
    Effect.gen(function* () {
      yield* fs.remove(dir, { recursive: true }).pipe(Effect.ignore);
      yield* fs.makeDirectory(dir, { recursive: true });
      for (const { entry, text } of es) yield* fs.writeFileString(path.join(dir, `${entry.name}.mmd`), `${text}\n`);
      const manifest = new Manifest({
        corpus,
        repo: `https://github.com/${MERMAID_REPO}`,
        tag: MERMAID_TAG,
        commit: MERMAID_COMMIT,
        licence: "MIT (c) 2014 - 2022 Knut Sveidqvist; see bench/NOTICES.md",
        diagrams: es.map((e) => e.entry),
      });
      const encoded = yield* Schema.encode(Manifest)(manifest);
      yield* fs.writeFileString(path.join(dir, "manifest.json"), `${JSON.stringify(encoded, null, 2)}\n`);
      yield* Console.log(`${corpus}: ${es.length} diagrams written to ${path.relative(process.cwd(), dir) || "."}`);
    });

  const sources = selectSourcePaths([...blobs.keys()]);
  yield* Console.log(`${sources.length} flowchart source files at ${MERMAID_TAG} (${MERMAID_COMMIT.slice(0, 12)})`);
  const contents = yield* Effect.forEach(sources, fetchText, { concurrency: 8 });
  const entries = collect("flowchart", contents);
  yield* write("compat", COMPAT_DIR, entries);

  const seqSources = selectSourcePaths([...blobs.keys()], "sequence");
  yield* Console.log(`${seqSources.length} sequence source files`);
  const seqContents = yield* Effect.forEach(seqSources, fetchText, { concurrency: 8 });
  const seqEntries = collect("sequence", seqContents);
  yield* write("compat-sequence", COMPAT_SEQUENCE_DIR, seqEntries);

  const stateSources = selectSourcePaths([...blobs.keys()], "state");
  yield* Console.log(`${stateSources.length} state source files`);
  const stateContents = yield* Effect.forEach(stateSources, fetchText, { concurrency: 8 });
  const stateEntries = collect("state", stateContents);
  yield* write("compat-state", COMPAT_STATE_DIR, stateEntries);

  // Edits: pairs from the first EDIT_SOURCES diagrams that yield at least three edit kinds.
  yield* fs.remove(EDITS_DIR, { recursive: true }).pipe(Effect.ignore);
  yield* fs.makeDirectory(EDITS_DIR, { recursive: true });
  let used = 0;
  let pairs = 0;
  for (const { entry, text } of entries) {
    if (used >= EDIT_SOURCES) break;
    const edits = makeEdits(entry.name, text);
    if (edits.length < 3) continue;
    used++;
    for (const e of edits) {
      const encodedPair = yield* Schema.encode(EditPairFile)(new EditPairFile(e));
      yield* fs.writeFileString(path.join(EDITS_DIR, `${e.name}.json`), `${JSON.stringify(encodedPair, null, 2)}\n`);
      pairs++;
    }
  }
  yield* Console.log(`edits: ${pairs} pairs from ${used} diagrams`);
  return {
    diagrams: entries.length,
    sequenceDiagrams: seqEntries.length,
    stateDiagrams: stateEntries.length,
    editPairs: pairs,
  };
});
