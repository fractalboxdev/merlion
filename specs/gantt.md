# Gantt diagrams

`gantt` renders a static SVG schedule through the shared core, CLI, WASM and Markdown adapters. The model stores UTC milliseconds and source spans; it does not read the clock, local timezone or browser state. Gantt uses its own timeline layout rather than the graph layout's ranks and routed edges.

## Reference and compatibility

The reference is Mermaid `mermaid@12.0.0`, commit `98a0945418c76238f15df2afaddbba4272656c3b`:

- [Gantt documentation](https://mermaid.js.org/syntax/gantt.html) describes authoring syntax. The pinned [syntax documentation](https://github.com/mermaid-js/mermaid/blob/98a0945418c76238f15df2afaddbba4272656c3b/packages/mermaid/src/docs/syntax/gantt.md) records that release's examples.
- [Grammar](https://github.com/mermaid-js/mermaid/blob/98a0945418c76238f15df2afaddbba4272656c3b/packages/mermaid/src/diagrams/gantt/parser/gantt.jison) defines directives, task metadata and comments.
- [Scheduling database](https://github.com/mermaid-js/mermaid/blob/98a0945418c76238f15df2afaddbba4272656c3b/packages/mermaid/src/diagrams/gantt/ganttDb.js) resolves starts, ends, exclusions and dependencies.
- [Renderer](https://github.com/mermaid-js/mermaid/blob/98a0945418c76238f15df2afaddbba4272656c3b/packages/mermaid/src/diagrams/gantt/ganttRenderer.js) positions milestone midpoints and vertical markers.

`bench/corpus/compat-gantt` contains 64 unique diagrams from the pinned demos, syntax documentation, rendering templates and Gantt `.mmd` fixtures. The source selection includes all 43 upstream Gantt fixture files, including the deliberately invalid date case. SHA-256 deduplication keeps the first source-path occurrence; the manifest records original paths, Git blobs and block indices. `bench/NOTICES.md` carries the MIT attribution. Selection depends on paths and diagram headers, never successful rendering.

`pnpm --dir bench bench fetch --kind gantt` refreshes only this corpus. `pnpm --dir bench bench gantt --out-svgs` compares every source with pinned Mermaid in headless Chromium, in UTC. `MERLION_BENCH_CHROMIUM` optionally selects an existing browser executable. Results include Markdown and JSON under `bench/results`.

`--min-compat PERCENT` gates the unrounded percentage against the complete retained corpus. It defaults to `0` for exploratory reports; a nonzero threshold rejects `--limit` and incomplete runs.

The schedule/content measure checks task IDs, visible labels and marks, section names, flags and resolved start/end timestamps. Marks must share an increasing affine time scale, with milestones at interval midpoints and vertical markers at starts. Invalid, rejected and unsupported sources remain in the denominator; two rejections earn no successful-render credit. A directives-only empty chart matches only with a valid SVG viewport. Time-only formats normalize the common day origin; day-of-month formats without a month/year normalize the common first-month origin. These modes report separately from absolute UTC matches and preserve elapsed durations; a span across unequal month lengths can still differ. The measure does not establish pixel equivalence or axis/layout equivalence. The roadmap's 90% target requires a complete run; a limited run cannot establish coverage.

## Scheduling contract

After leading flags, task metadata has these forms:

| Form | ID | Start | End |
|---|---|---|---|
| `Label : duration-or-end` | Generated | Previous declared task's end | Duration or absolute end |
| `Label : start, duration-or-end` | Generated | Absolute date or `after` | Duration or absolute end |
| `Label : id, start, duration-or-end` | Authored | Absolute date or `after` | Duration or absolute end |

Generated IDs are `task1`, `task2`, … and advance only for tasks without an authored ID. Sections do not reset the previous task. The first task requires an explicit start. Duplicate explicit IDs retain every declared task with `W016`; dependency lookup uses the last declaration with that ID, matching the pinned reference. Generated SVG element IDs remain unique. Unknown dependencies and cycles produce source-located errors rather than fabricated dates.

`after a b` starts at the latest end among its dependencies. `until a b` ends at the earliest dependency start. Forward references resolve before layout. Dependency counts and task counts use the shared resource limits.

Flags `done`, `active`, `crit`, `milestone` and `vert` precede the scheduling fields and remain independently represented. A milestone's visible instant is the midpoint of its start/end interval, including a nonzero duration. A `vert` task draws a vertical marker without consuming an ordinary task row.

Durations accept nonnegative decimal quantities followed by `ms`, `s`, `m`, `h`, `d`, `w`, `M` or `y`. A bare `0` also represents zero duration; a vertical marker's empty trailing end field represents zero duration. Other invalid durations remain errors. Months and years use calendar arithmetic with month-end clamping; days and weeks follow Mermaid's rounded day arithmetic. Sub-day units resolve to integer milliseconds. End times are exclusive unless `inclusiveEndDates` adds one day to an authored absolute end.

`excludes` merges weekends, English weekday names and explicit dates. The upstream example token `weekdays` excludes nothing in the reference and is ignored with `W016`; it does not mean Monday through Friday. `weekend friday` and `weekend saturday` select the weekend pair. Explicit `includes` dates override exclusions. Duration-derived ends extend through excluded days; explicit ISO `YYYY-MM-DD` ends bypass extension, matching the pinned scheduler's `manualEndTime` rule. Exclusion expansion is bounded to 10,000 added days. Exclusions shade the calendar as well as affecting duration resolution.

## Calendar and determinism

Set `dateFormat` and `inclusiveEndDates` before declaring tasks. A later change reports a source-located error, so earlier dates never retain a different interpretation from the diagram's final date settings.

`dateFormat` defaults to `YYYY-MM-DD`. The parser accepts calendar components, English month names, ordinal dates, time components, timezone offsets, bracketed literals, Unix seconds (`X`) and Unix milliseconds (`x`). Dates validate calendar ranges; unsupported or invalid forms produce `E002` with the source span. `R020` left-pads a one-to-four-digit year and unpadded month/day in an otherwise valid `YYYY-MM-DD` date, supplying the exact replacement; `202-12-01` means `0202-12-01`, never an inferred recent year. Strict rendering rejects that repair. Years outside 1–9999 and invalid calendar dates remain errors. No browser date fallback supplies dates. The calendar parser in `model/gantt/calendar.rs` is the token authority.

Omitted date components use the stable `1970-01-01` coordinate instead of Mermaid's current date. `I037` discloses this for partial/time-only formats. A time-only diagram's default axis uses `%H:%M`; explicit axis formats remain authored. Timezone offsets convert to UTC; host DST never changes task durations.

The pinned Mermaid scheduler treats numeric `X` starts as milliseconds despite documentation defining `X` as seconds; its end parser uses Day.js. Merlion follows the documented seconds semantics. A resulting reference mismatch remains visible in the benchmark.

The live today marker is omitted with `I038`. `todayMarker off` states this explicitly and emits no omission information. Styled today-marker directives validate their syntax and retain the same deterministic omission. No fixed date stands in for today. These information diagnostics do not fail strict Markdown builds.

## Directives and configuration

`title`, `section`, `dateFormat`, `axisFormat`, `tickInterval`, `topAxis`, `weekday`, `weekend`, `excludes`, `includes`, `inclusiveEndDates`, `displayMode`, `todayMarker`, `accTitle` and `accDescr` are Gantt directives. Standard shared preamble metadata remains available.

`tickInterval` accepts a positive integer followed by a singular unit such as `1week` or `15minute`. When an explicit interval would draw more than 512 ticks, the static axis uses an aligned integer multiple of the authored interval and reports the coarser cadence with `I039`; task dates remain unchanged. Ordinary explicit intervals retain their cadence, and automatic intervals adapt to the schedule's range. `weekday monday` controls weekly tick alignment. `axisFormat` uses supported D3/strftime-style conversions, including year/month/day names and numbers, time, milliseconds and epoch timestamps. Unsupported conversions report an error instead of a literal percent token.

Compact mode packs non-overlapping intervals within each section. YAML accepts top-level `displayMode: compact` as in pinned upstream examples, and `config.gantt` accepts `displayMode`, `topAxis`, `tickInterval` and `axisFormat`. Browser-specific layout settings do not control static SVG sizing; diagnostics expose ignored settings. Syntactically recognized Gantt `click … call` and `click … href` actions are omitted with `W016`; malformed action statements produce `E002`. No callback executes and no Gantt task link appears in static output.

## Model and SVG

`Diagram::Gantt` holds `Gantt`: shared metadata, sections, tasks, date/axis formats, tick interval, axis placement, compact mode, week start and exclusions. Each `Task` stores its ID, label, optional section index, start/end UTC milliseconds, flags, dependency indices and source span.

The root has `class="merlion merlion-gantt"`. Each task group has `merlion-gantt-task` and these escaped attributes:

| Attribute | Value |
|---|---|
| `data-merlion-id` | Authored or generated task ID |
| `data-merlion-start`, `data-merlion-end` | Signed integer UTC milliseconds |
| `data-merlion-section` | Section name, or empty for an unsectioned task |
| `data-merlion-flags` | Space-separated active/done/crit/milestone/vert flags |

Task marks use `merlion-shape`; visible labels use `merlion-gantt-task-text`. Ordinary task spans have a 2 px minimum visible width, even when the timeline projects their duration to less than a pixel; their start remains exact. Section backgrounds and headings, excluded-day shading, axis labels and grid lines have separate Gantt classes. CSS theme tokens and standalone literal paints follow the shared SVG contract. Labels reserve readable rows and sit outside short spans when necessary; a milestone remains a visible mark at zero duration.

Grid lines carry `data-merlion-time` with their UTC millisecond timestamp. The compatibility geometry check calibrates task positions against those drawn axis lines, including global shifts relative to the axis.

The accessible SVG includes title/description and a plain-text schedule outline. Authored `accDescr` replaces the SVG description without replacing the returned outline. Text and attribute escaping follows the common emitter. No script, external resource or runtime renderer is required.

## Validation

- Core fixtures cover explicit dates, durations, IDs, dependencies, sections, statuses, milestones, exclusions and time-only formats; negative cases assert source-located errors.
- SVG acceptance checks cover safe XML, labels/marks, fit, accessibility, escaping, deterministic bytes and theme output.
- Adapter acceptance exercises the same source through CLI, WASM, rehype and Astro.
- `pnpm --dir bench bench determinism --corpus gantt` checks core fixtures across native/WASM; `--corpus compat-gantt` retains successful and rejected upstream cases in the parity comparison.
- Benchmark comparison tests reject missing marks, missing/renamed tasks, changed sections, flags or schedule dates; they distinguish normalized time-only equivalence and rejection agreement.

Measured reports under `bench/results/*-gantt-baseline.{md,json}` state completed coverage and failures. They are the evidence for compatibility percentages; corpus presence and roadmap targets alone establish no pass rate.
