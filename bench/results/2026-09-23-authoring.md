# Authoring: Mermaid or SVG

18 tasks (corpus version 1), each asked of every model twice — once as a Mermaid flowchart, once as a standalone SVG — with the same description and the same readability requirement. Scored 2026-09-23. Method: [specs/benchmark.md](../../specs/benchmark.md#authoring).

## Reader ceiling

Merlion's own drawing of each task's declared graph, read back through the same geometry recovery every hand-written SVG is read through. The graph is known to be right, so what this loses is the reader's error, not the drawing's.

| Node F1 | Edge F1 | Labels overflowing their shape | Shape overlaps | Clipped |
|---|---|---|---|---|
| 1.000 | 1.000 | 0.0% | 0 | 0 |

## sonnet

Answers recorded 2026-09-23 through `claude-cli`.

| | Mermaid | SVG | SVG ÷ Mermaid |
|---|---|---|---|
| Answers holding a diagram that draws | 100.0% | 100.0% | |
| Mean output tokens | 215 | 12096 | 56.3× |
| Mean source bytes | 426 | 4034 | 9.5× |
| Node F1 against the declared graph | 1.000 | 0.918 | |
| Edge F1 against the declared graph | 1.000 | 1.000 | |
| Labelled edges drawn with the right label | 58.8% | 29.4% | |
| Labels overflowing their shape | 0.0% | 0.0% | |
| Shape overlaps | 0 | 3 | |
| Ink outside the viewBox | 0 | 0 | |
| Edges with an endpoint at no shape | 0 | 0 | |
| Mermaid answers mermaid 12.0.0 itself parses | 100.0% | — | |

### Per task

| Task | Size | Format | Drawn | Node F1 | Edge F1 | Overflowing | Overlaps | Output tokens |
|---|---|---|---|---|---|---|---|---|
| ci-pipeline | small | mermaid | yes | 1.00 | 1.00 | 0/10 | 0 | 122 |
| ci-pipeline | small | svg | yes | 1.00 | 1.00 | 0/10 | 0 | 7231 |
| password-reset | small | mermaid | yes | 1.00 | 1.00 | 0/8 | 0 | 98 |
| password-reset | small | svg | yes | 0.92 | 1.00 | 0/8 | 0 | 4226 |
| order-fulfilment | small | mermaid | yes | 1.00 | 1.00 | 0/8 | 0 | 97 |
| order-fulfilment | small | svg | yes | 0.86 | 1.00 | 0/8 | 0 | 2212 |
| cache-read | small | mermaid | yes | 1.00 | 1.00 | 0/7 | 0 | 89 |
| cache-read | small | svg | yes | 0.91 | 1.00 | 0/7 | 0 | 3385 |
| long-labels | small | mermaid | yes | 1.00 | 1.00 | 0/7 | 0 | 129 |
| long-labels | small | svg | yes | 0.91 | 1.00 | 0/7 | 0 | 6229 |
| oauth-code | medium | mermaid | yes | 1.00 | 1.00 | 0/10 | 0 | 144 |
| oauth-code | medium | svg | yes | 0.89 | 1.00 | 0/12 | 0 | 5237 |
| build-cache | medium | mermaid | yes | 1.00 | 1.00 | 0/14 | 0 | 164 |
| build-cache | medium | svg | yes | 1.00 | 1.00 | 0/14 | 0 | 6682 |
| support-triage | medium | mermaid | yes | 1.00 | 1.00 | 0/15 | 0 | 173 |
| support-triage | medium | svg | yes | 0.87 | 1.00 | 0/15 | 0 | 12987 |
| request-lifecycle | medium | mermaid | yes | 1.00 | 1.00 | 0/15 | 0 | 248 |
| request-lifecycle | medium | svg | yes | 0.92 | 1.00 | 0/15 | 1 | 9127 |
| release-train | medium | mermaid | yes | 1.00 | 1.00 | 0/13 | 0 | 162 |
| release-train | medium | svg | yes | 0.86 | 1.00 | 0/14 | 0 | 10664 |
| data-ingest | medium | mermaid | yes | 1.00 | 1.00 | 0/12 | 0 | 255 |
| data-ingest | medium | svg | yes | 0.91 | 1.00 | 0/12 | 0 | 7251 |
| compiler-stages | medium | mermaid | yes | 1.00 | 1.00 | 0/16 | 0 | 276 |
| compiler-stages | medium | svg | yes | 0.86 | 1.00 | 0/16 | 0 | 13633 |
| k8s-rollout | large | mermaid | yes | 1.00 | 1.00 | 0/20 | 0 | 271 |
| k8s-rollout | large | svg | yes | 0.91 | 1.00 | 0/21 | 0 | 14763 |
| payment-settlement | large | mermaid | yes | 1.00 | 1.00 | 0/22 | 0 | 432 |
| payment-settlement | large | svg | yes | 1.00 | 1.00 | 0/22 | 0 | 23029 |
| incident-response | large | mermaid | yes | 1.00 | 1.00 | 0/20 | 0 | 254 |
| incident-response | large | svg | yes | 0.93 | 1.00 | 0/21 | 0 | 16775 |
| content-moderation | large | mermaid | yes | 1.00 | 1.00 | 0/26 | 0 | 302 |
| content-moderation | large | svg | yes | 0.94 | 1.00 | 0/19 | 0 | 40987 |
| warehouse-etl | large | mermaid | yes | 1.00 | 1.00 | 0/17 | 0 | 425 |
| warehouse-etl | large | svg | yes | 1.00 | 1.00 | 0/17 | 0 | 10074 |
| onboarding-funnel | large | mermaid | yes | 1.00 | 1.00 | 0/18 | 0 | 229 |
| onboarding-funnel | large | svg | yes | 0.85 | 1.00 | 0/19 | 2 | 23238 |

## google/gemma-4-31b

Answers recorded 2026-09-23 through `openai-compatible`.

| | Mermaid | SVG | SVG ÷ Mermaid |
|---|---|---|---|
| Answers holding a diagram that draws | 100.0% | 100.0% | |
| Mean output tokens | 538 | 4195 | 7.8× |
| Mean source bytes | 189 | 2606 | 13.8× |
| Node F1 against the declared graph | 1.000 | 0.857 | |
| Edge F1 against the declared graph | 1.000 | 1.000 | |
| Labelled edges drawn with the right label | 0.0% | 50.0% | |
| Labels overflowing their shape | 0.0% | 0.0% | |
| Shape overlaps | 0 | 0 | |
| Ink outside the viewBox | 0 | 0 | |
| Edges with an endpoint at no shape | 0 | 0 | |
| Mermaid answers mermaid 12.0.0 itself parses | 100.0% | — | |

### Per task

| Task | Size | Format | Drawn | Node F1 | Edge F1 | Overflowing | Overlaps | Output tokens |
|---|---|---|---|---|---|---|---|---|
| ci-pipeline | small | mermaid | yes | 1.00 | 1.00 | 0/10 | 0 | 538 |
| ci-pipeline | small | svg | yes | 0.86 | 1.00 | 0/10 | 0 | 4195 |

