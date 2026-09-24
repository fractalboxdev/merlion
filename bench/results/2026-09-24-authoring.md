# Authoring: Mermaid or SVG

18 tasks (corpus version 1), each asked of every model twice — once as a Mermaid flowchart, once as a standalone SVG — with the same description and the same readability requirement. Scored 2026-09-24. Method: [specs/benchmark.md](../../specs/benchmark.md#authoring).

## Reader ceiling

Merlion's own drawing of each task's declared graph, read back through the same geometry recovery every hand-written SVG is read through. The graph is known to be right, so what this loses is the reader's error, not the drawing's.

| Node F1 | Edge F1 | Labels overflowing their shape | Shape overlaps | Clipped |
|---|---|---|---|---|
| 1.000 | 1.000 | 0.0% | 0 | 0 |

## claude-sonnet-5

Answers recorded 2026-09-23 through `claude-cli`.

| | Mermaid | SVG | SVG ÷ Mermaid |
|---|---|---|---|
| Answers holding a diagram that draws | 100.0% | 100.0% | |
| Mean output tokens | 215 | 12096 | 56.3× |
| Mean source bytes | 426 | 4034 | 9.5× |
| Node F1 against the declared graph | 1.000 | 0.918 | |
| Edge F1 against the declared graph | 1.000 | 1.000 | |
| Labelled edges drawn with the right label | 58.8% | 29.4% | |
| Labels overflowing their shape | 0.0% | 0.4% | |
| Labels touching no shape (a title, or a chipless edge label) | 0 | 16 | |
| Shape overlaps | 0 | 4 | |
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
| long-labels | small | svg | yes | 0.91 | 1.00 | 0/7 | 1 | 6229 |
| oauth-code | medium | mermaid | yes | 1.00 | 1.00 | 0/10 | 0 | 144 |
| oauth-code | medium | svg | yes | 0.89 | 1.00 | 0/12 | 0 | 5237 |
| build-cache | medium | mermaid | yes | 1.00 | 1.00 | 0/14 | 0 | 164 |
| build-cache | medium | svg | yes | 1.00 | 1.00 | 0/14 | 0 | 6682 |
| support-triage | medium | mermaid | yes | 1.00 | 1.00 | 0/15 | 0 | 173 |
| support-triage | medium | svg | yes | 0.87 | 1.00 | 1/15 | 0 | 12987 |
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
| Answers holding a diagram that draws | 94.4% | 100.0% | |
| Mean output tokens | 819 | 7569 | 9.2× |
| — of those, the diagram itself | 124 | 1656 | 13.4× |
| Mean source bytes | 402 | 3687 | 9.2× |
| Node F1 against the declared graph | 0.988 | 0.870 | |
| Edge F1 against the declared graph | 0.966 | 0.870 | |
| Labelled edges drawn with the right label | 35.9% | 14.7% | |
| Labels overflowing their shape | 0.0% | 9.5% | |
| Labels touching no shape (a title, or a chipless edge label) | 0 | 53 | |
| Shape overlaps | 0 | 2 | |
| Ink outside the viewBox | 0 | 0 | |
| Edges with an endpoint at no shape | 0 | 3 | |
| Mermaid answers mermaid 12.0.0 itself parses | 94.4% | — | |

### Per task

| Task | Size | Format | Drawn | Node F1 | Edge F1 | Overflowing | Overlaps | Output tokens |
|---|---|---|---|---|---|---|---|---|
| ci-pipeline | small | mermaid | yes | 1.00 | 1.00 | 0/10 | 0 | 468 |
| ci-pipeline | small | svg | yes | 0.86 | 1.00 | 2/10 | 0 | 6047 |
| password-reset | small | mermaid | yes | 1.00 | 1.00 | 0/8 | 0 | 451 |
| password-reset | small | svg | yes | 0.86 | 1.00 | 0/8 | 0 | 3996 |
| order-fulfilment | small | mermaid | yes | 1.00 | 1.00 | 0/8 | 0 | 461 |
| order-fulfilment | small | svg | yes | 0.83 | 0.60 | 2/8 | 0 | 2938 |
| cache-read | small | mermaid | yes | 1.00 | 1.00 | 0/7 | 0 | 607 |
| cache-read | small | svg | yes | 0.91 | 1.00 | 0/7 | 0 | 7087 |
| long-labels | small | mermaid | yes | 1.00 | 1.00 | 0/7 | 0 | 541 |
| long-labels | small | svg | yes | 0.91 | 1.00 | 0/7 | 0 | 9413 |
| oauth-code | medium | mermaid | yes | 1.00 | 1.00 | 0/10 | 0 | 799 |
| oauth-code | medium | svg | yes | 1.00 | 1.00 | 2/10 | 0 | 8487 |
| build-cache | medium | mermaid | yes | 1.00 | 1.00 | 0/14 | 0 | 884 |
| build-cache | medium | svg | yes | 0.91 | 1.00 | 0/14 | 0 | 9826 |
| support-triage | medium | mermaid | yes | 1.00 | 1.00 | 0/15 | 0 | 1570 |
| support-triage | medium | svg | yes | 0.87 | 0.50 | 1/15 | 0 | 6647 |
| request-lifecycle | medium | mermaid | yes | 1.00 | 1.00 | 0/15 | 0 | 770 |
| request-lifecycle | medium | svg | yes | 0.85 | 1.00 | 2/15 | 0 | 8261 |
| release-train | medium | mermaid | yes | 1.00 | 1.00 | 0/13 | 0 | 730 |
| release-train | medium | svg | yes | 0.70 | 0.71 | 0/13 | 0 | 5435 |
| data-ingest | medium | mermaid | yes | 1.00 | 1.00 | 0/12 | 0 | 606 |
| data-ingest | medium | svg | yes | 0.95 | 1.00 | 1/12 | 0 | 5070 |
| compiler-stages | medium | mermaid | no (exit 1: no JSON on stdout; <stdin>:5:18: error E002 expected a link, `&`, `;` or a newline, found `r`) | — | — | — | — | 696 |
| compiler-stages | medium | svg | yes | 0.77 | 0.67 | 2/16 | 0 | 5843 |
| k8s-rollout | large | mermaid | yes | 1.00 | 1.00 | 0/20 | 0 | 1155 |
| k8s-rollout | large | svg | yes | 0.94 | 1.00 | 3/20 | 0 | 9345 |
| payment-settlement | large | mermaid | yes | 1.00 | 1.00 | 0/22 | 0 | 1328 |
| payment-settlement | large | svg | yes | 0.85 | 1.00 | 2/22 | 0 | 8800 |
| incident-response | large | mermaid | yes | 1.00 | 1.00 | 0/20 | 0 | 1072 |
| incident-response | large | svg | yes | 0.90 | 0.94 | 2/20 | 0 | 10554 |
| content-moderation | large | mermaid | yes | 0.80 | 0.42 | 0/21 | 0 | 821 |
| content-moderation | large | svg | yes | 0.87 | 0.42 | 3/21 | 0 | 14049 |
| warehouse-etl | large | mermaid | yes | 1.00 | 1.00 | 0/17 | 0 | 964 |
| warehouse-etl | large | svg | yes | 0.84 | 0.82 | 0/17 | 2 | 5068 |
| onboarding-funnel | large | mermaid | yes | 1.00 | 1.00 | 0/18 | 0 | 817 |
| onboarding-funnel | large | svg | yes | 0.85 | 1.00 | 2/18 | 0 | 9369 |

