# Third-party notices

Merlion is MIT-licensed (`LICENSE`). The works below are incorporated under their own licences (specs/licensing.md).

## Inter

- **Project:** [Inter](https://github.com/rsms/inter) 4.1
- **Licence:** SIL Open Font License 1.1, no Reserved Font Name
- **Copyright:** © 2016 The Inter Project Authors
- **Incorporated as:** metric tables generated from `Inter-Regular.ttf` and `Inter-SemiBold.ttf` (`crates/merlion-render/src/text/tables.rs`), and WOFF2 subsets of both weights (`crates/merlion-render/assets/`). The subsets are Modified Versions under the OFL; the licence text ships beside them in `crates/merlion-render/assets/OFL.txt` and inside every SVG rendered with `font: "embed"`.

```text
Copyright (c) 2016 The Inter Project Authors (https://github.com/rsms/inter)

This Font Software is licensed under the SIL Open Font License, Version 1.1.
```

The full licence text is in [`tools/fontgen/OFL.txt`](tools/fontgen/OFL.txt).
