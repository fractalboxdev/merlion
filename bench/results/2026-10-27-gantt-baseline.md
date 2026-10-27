# Gantt baseline — 2026-10-27

Pinned Mermaid mermaid@12.0.0 (98a0945418c76238f15df2afaddbba4272656c3b); Chromium 145.0.7632.6; 64 of 64 retained unique sources. See `bench/NOTICES.md` for provenance.

Compatibility compares task IDs, visible labels and marks, sections, status flags, and resolved start/end timestamps. Merlion marks and timestamped axis grid lines must follow one increasing affine time scale, including milestone midpoint and vertical-marker start positions. Ordinary task width is the projected duration with a 2 px visibility minimum. Mermaid runs in UTC. This is schedule/content compatibility, not pixel equivalence or proof of axis-label/layout equivalence.

Time-only date formats normalize each renderer's common midnight origin. Day-of-month formats without month/year normalize the common first-month origin, preserving day-of-month and elapsed durations; crossing unequal calendar months can still differ. These matches establish relative calendar/time equivalence, not absolute-calendar equivalence. Mermaid's current-day defaults and today marker remain clock-dependent; Merlion's deterministic omissions are documented in specs/gantt.md.

| Measure | Result |
|---|---|
| Matched / all retained sources | 92.2% (59/64) |
| Absolute UTC matches | 50 |
| Time-only normalized matches | 8 |
| Day-of-month normalized matches | 1 |
| Merlion rendered | 59/64 |
| Mermaid rendered | 64/64 |
| Merlion fits 720 px | 54/64 |

Invalid, unsupported and clock-dependent sources stay in the denominator. Both-rejected cases are rejection agreement and earn no successful-render credit. The fixture named `should-fail-rendering` remains classified as an upstream failure fixture even if the pinned renderer accepts it through browser date fallbacks. Reference today-marker classifications describe a live decoration outside the schedule metric; omission is not claimed as marker parity. A limited run is exploratory and cannot establish corpus coverage.

| Source | Outcome | Classification | Comparison / error |
|---|---|---|---|
| `demos-gantt-01` | matched | clock-dependent reference today marker | absolute UTC |
| `demos-gantt-02` | matched | clock-dependent reference today marker | absolute UTC |
| `demos-gantt-03` | candidate rejected | clock-dependent reference today marker | Merlion: parse: E002 invalid task duration or end date `milestore` |
| `demos-gantt-04` | matched | clock-dependent reference today marker | absolute UTC |
| `demos-gantt-05` | matched | clock-dependent reference today marker | absolute UTC |
| `demos-gantt-06` | candidate rejected |  | Merlion: parse: E002 invalid start date `08-08-09-01:00` for dateFormat `Z` |
| `demos-gantt-07` | candidate rejected | clock-dependent reference today marker | Merlion: parse: E002 invalid start date `08-08-09-01:00` for dateFormat `Z` |
| `demos-gantt-08` | matched | clock-dependent reference today marker | absolute UTC |
| `demos-gantt-09` | matched | clock-dependent reference today marker | absolute UTC |
| `demos-gantt-10` | matched | clock-dependent reference today marker; time-of-day (day origin normalized) | time-of-day (day origin normalized) |
| `docs-gantt-01` | matched | clock-dependent reference today marker | absolute UTC |
| `docs-gantt-02` | matched | clock-dependent reference today marker | absolute UTC |
| `docs-gantt-03` | matched | clock-dependent reference today marker | absolute UTC |
| `docs-gantt-04` | matched | clock-dependent reference today marker | absolute UTC |
| `docs-gantt-05` | matched | clock-dependent reference today marker; time-of-day (day origin normalized) | time-of-day (day origin normalized) |
| `docs-gantt-06` | matched | clock-dependent reference today marker; time-of-day (day origin normalized) | time-of-day (day origin normalized) |
| `docs-gantt-07` | matched | clock-dependent reference today marker | absolute UTC |
| `docs-gantt-08` | matched | clock-dependent reference today marker | absolute UTC |
| `docs-gantt-09` | matched | clock-dependent reference today marker | absolute UTC |
| `docs-gantt-10` | matched | clock-dependent reference today marker | absolute UTC |
| `docs-gantt-11` | candidate rejected | clock-dependent reference today marker | Merlion: parse: E011 unterminated quoted scalar |
| `e2e-handle-multiline-section-titles-with-different-line-breaks` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-multiple-dependencies-syntax` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-default-to-showing-today-marker` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-fail-rendering-a-gantt-chart-for-issue-1060-with-invalid-date` | matched | clock-dependent reference today marker; upstream failure fixture | absolute UTC |
| `e2e-should-handle-dates-with-year-typo-like-202-instead-of-2024-issue-5496` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-handle-milliseconds` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-handle-multiple-dependencies-syntax-with-after-and-until` | matched |  | absolute UTC |
| `e2e-should-handle-numeric-timestamps-with-dateformat-x` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-handle-numeric-timestamps-with-dateformat-x-2` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-handle-seconds-only-format-with-tickinterval-issue-5496` | candidate rejected | clock-dependent reference today marker | Merlion: parse: E002 invalid start date `0` for dateFormat `ss` |
| `e2e-should-hide-today-marker` | matched |  | absolute UTC |
| `e2e-should-render-a-gantt-chart` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-render-a-gantt-diagram-excluding-a-specific-date-in-yyyy-mm-dd-hh-mm-ss-format` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-render-a-gantt-diagram-excluding-friday-and-saturday` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-render-a-gantt-diagram-excluding-friday-and-saturday-in-yyyy-mm-dd-hh-mm-ss-format` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-render-a-gantt-diagram-excluding-saturday-and-sunday` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-render-a-gantt-diagram-excluding-saturday-and-sunday-in-yyyy-mm-dd-hh-mm-ss-format` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-render-a-gantt-diagram-with-a-vert-tag` | matched | clock-dependent reference today marker; time-of-day (day origin normalized) | time-of-day (day origin normalized) |
| `e2e-should-render-a-gantt-diagram-with-data-labels-at-the-top-when-topaxis-is-true` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-render-a-gantt-diagram-with-tick-is-1-day` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-render-a-gantt-diagram-with-tick-is-1-day-and-topaxis-is-true` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-render-a-gantt-diagram-with-tick-is-1-month` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-render-a-gantt-diagram-with-tick-is-1-week` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-render-a-gantt-diagram-with-tick-is-1-week-with-the-day-starting-on-monday` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-render-a-gantt-diagram-with-tick-is-15-minutes` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-render-a-gantt-diagram-with-tick-is-2-milliseconds` | matched | clock-dependent reference today marker; time-of-day (day origin normalized) | time-of-day (day origin normalized) |
| `e2e-should-render-a-gantt-diagram-with-tick-is-2-seconds` | matched | clock-dependent reference today marker; time-of-day (day origin normalized) | time-of-day (day origin normalized) |
| `e2e-should-render-a-gantt-diagram-with-tick-is-6-hours` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-render-a-gantt-diagram-with-very-large-intervals-skipping-excludes-if-interval-5-years` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-render-done-task-inside-text-readable-in-dark-mode` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-render-done-task-outside-text-readable-in-dark-mode-regression-7433` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-render-done-tasks-with-readable-text-in-dark-mode` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-render-multi-line-excludes-issue-6270` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-render-only-the-day-when-using-dateformat-d` | matched | clock-dependent reference today marker; day-of-month (month origin normalized) | day-of-month (month origin normalized) |
| `e2e-should-render-vertical-markers-without-creating-extra-rows-and-show-their-labels` | matched | clock-dependent reference today marker; time-of-day (day origin normalized) | time-of-day (day origin normalized) |
| `e2e-should-render-when-compact-is-true` | matched | clock-dependent reference today marker; time-of-day (day origin normalized) | time-of-day (day origin normalized) |
| `e2e-should-render-when-theres-a-hashtag-in-a-section-is-true` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-render-when-theres-a-hashtag-in-the-task-data` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-render-when-theres-a-hashtag-in-the-title` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-render-when-theres-a-semicolon-in-a-section-is-true` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-render-when-theres-a-semicolon-in-the-task-data` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-render-when-theres-a-semicolon-in-the-title` | matched | clock-dependent reference today marker | absolute UTC |
| `e2e-should-style-today-marker` | matched | clock-dependent reference today marker | absolute UTC |
