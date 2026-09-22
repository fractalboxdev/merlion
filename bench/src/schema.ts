/** Boundary types: files on disk and renderer output. */
import { Schema } from "effect";

/** The mermaid release the `compat` corpus and the mermaid baselines are pinned to. */
export const MERMAID_REPO = "mermaid-js/mermaid";
export const MERMAID_TAG = "mermaid@12.0.0";
/** Commit the `mermaid@12.0.0` tag points to. */
export const MERMAID_COMMIT = "98a0945418c76238f15df2afaddbba4272656c3b";

export class ManifestEntry extends Schema.Class<ManifestEntry>("ManifestEntry")({
  name: Schema.String,
  /** Repository path of the file the diagram was extracted from. */
  source: Schema.String,
  /** 1-based index of the diagram among the corpus's diagrams in `source`. */
  block: Schema.Number,
  /** Git blob sha of `source` at `commit`. */
  sourceBlob: Schema.String,
  /** sha256 of the `.mmd` file contents. */
  sha256: Schema.String,
}) {}

export class Manifest extends Schema.Class<Manifest>("Manifest")({
  corpus: Schema.Literal("compat", "compat-sequence"),
  repo: Schema.String,
  tag: Schema.String,
  commit: Schema.String,
  licence: Schema.String,
  diagrams: Schema.Array(ManifestEntry),
}) {}

export const EditKind = Schema.Literal("add-node", "add-edge", "remove-edge", "rename-label");

export class EditPairFile extends Schema.Class<EditPairFile>("EditPairFile")({
  name: Schema.String,
  source: Schema.String,
  kind: EditKind,
  before: Schema.String,
  after: Schema.String,
}) {}

/** stdout of `merlion render --json` (specs/integrations.md#cli). */
export const MerlionJson = Schema.Struct({
  svg: Schema.NullOr(Schema.String),
  outline: Schema.optional(Schema.NullOr(Schema.String)),
  diagnostics: Schema.optional(Schema.Array(Schema.Unknown)),
  fuel_used: Schema.optional(Schema.NullOr(Schema.Number)),
  error: Schema.optional(Schema.NullOr(Schema.String)),
});
export type MerlionJson = typeof MerlionJson.Type;

// ---------------------------------------------------------------------------
// Results file.

const NullableNumber = Schema.NullOr(Schema.Number);

export const LayoutMetricsSchema = Schema.Struct({
  nodes: Schema.Number,
  edges: Schema.Number,
  crossings: Schema.Number,
  maxCrossingsPerEdge: Schema.Number,
  bends: Schema.Number,
  edgeLength: Schema.Number,
  width: Schema.Number,
  height: Schema.Number,
  area: Schema.Number,
  aspectRatio: NullableNumber,
  labelOverlaps: Schema.Number,
  stress: NullableNumber,
  fits720: Schema.Boolean,
});

export const CompatSchema = Schema.Struct({
  pass: Schema.Boolean,
  nodeLabels: Schema.Boolean,
  edgeCount: Schema.Boolean,
  edgeLabels: Schema.Boolean,
  missingNodeLabels: Schema.Array(Schema.String),
  extraNodeLabels: Schema.Array(Schema.String),
  refEdges: Schema.Number,
  candEdges: Schema.Number,
});

export const DiagramResult = Schema.Struct({
  name: Schema.String,
  ok: Schema.Boolean,
  error: Schema.NullOr(Schema.String),
  ms: Schema.Number,
  fuelUsed: NullableNumber,
  svgBytes: Schema.Number,
  metrics: Schema.NullOr(LayoutMetricsSchema),
  /** Against the reference renderer's drawing of the same source; null when either failed. */
  compat: Schema.NullOr(CompatSchema),
});
export type DiagramResult = typeof DiagramResult.Type;

export const EditResult = Schema.Struct({
  name: Schema.String,
  kind: EditKind,
  ok: Schema.Boolean,
  error: Schema.NullOr(Schema.String),
  matched: Schema.Number,
  /** Per surviving node displacement, as a fraction of the diagonal before the edit. */
  displacements: Schema.Array(Schema.Number),
});
export type EditResult = typeof EditResult.Type;

export const RendererResult = Schema.Struct({
  renderer: Schema.String,
  version: Schema.String,
  diagrams: Schema.Array(DiagramResult),
  edits: Schema.Array(EditResult),
});
export type RendererResult = typeof RendererResult.Type;

export const Determinism = Schema.Struct({
  status: Schema.Literal("skipped", "ran"),
  reason: Schema.NullOr(Schema.String),
  compared: Schema.Number,
  identical: Schema.Number,
  differing: Schema.Array(Schema.String),
});
export type Determinism = typeof Determinism.Type;

export const ResultsFile = Schema.Struct({
  date: Schema.String,
  commit: Schema.String,
  corpus: Schema.String,
  corpusCommit: Schema.String,
  diagrams: Schema.Number,
  editPairs: Schema.Number,
  reference: Schema.String,
  renderers: Schema.Array(RendererResult),
  determinism: Determinism,
});
export type ResultsFile = typeof ResultsFile.Type;
