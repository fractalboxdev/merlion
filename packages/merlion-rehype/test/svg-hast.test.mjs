// svgToHast against the HTML parser, over every diagram the core renders from the
// benchmark corpora and the core's fixtures, and the plugin's output through MDX.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { compile } from "@mdx-js/mdx";
import { fromHtml } from "hast-util-from-html";
import { toHtml } from "hast-util-to-html";
import { find, svg as svgSchema } from "property-information";
import { VFile } from "vfile";
import rehypeMerlion from "../index.js";
import { svgToHast } from "../svg-hast.js";
import { loadWasmRender } from "../wasm.js";

const REPO = new URL("../../../", import.meta.url).pathname;
const CORPORA = [
  "bench/corpus/compat",
  "bench/corpus/compat-sequence",
  "bench/corpus/compat-state",
  "crates/merlion-render/tests/fixtures",
];

const sources = (dir) =>
  readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    const p = join(dir, e.name);
    return e.isDirectory() ? sources(p) : e.name.endsWith(".mmd") ? [p] : [];
  });

/** The tree an HTML parser builds from `html`, without positions. */
const parsed = (html) =>
  JSON.parse(JSON.stringify(fromHtml(html, { fragment: true }), (k, v) => (k === "position" ? undefined : v)));

const walk = (node, visit) => {
  visit(node);
  for (const c of node.children ?? []) walk(c, visit);
};

test("every rendered corpus diagram converts to elements an HTML parser agrees with", async () => {
  const render = await loadWasmRender();
  let n = 0;
  for (const file of CORPORA.flatMap((d) => sources(join(REPO, d)))) {
    const { svg } = render(readFileSync(file, "utf8"), { width: 720, strict: false, idPrefix: "t" });
    if (typeof svg !== "string") continue;
    n++;
    const tree = svgToHast(svg);
    assert.ok(tree, `${file}: outside the grammar`);
    // Same document once serialised: the conversion drops or alters nothing.
    // The trailing newline after `</svg>` sits outside the element and is dropped.
    assert.deepEqual(parsed(toHtml(tree, { space: "svg" })), parsed(svg.trim()), file);
    // Property names are the canonical hast names for the SVG schema.
    walk(tree, (node) => {
      for (const key of Object.keys(node.properties ?? {})) {
        assert.equal(find(svgSchema, key).property, key, `${file}: <${node.tagName}> ${key}`);
      }
    });
  }
  assert.ok(n > 700, `rendered ${n} diagrams`);
});

test("rejects input outside the core's output grammar", () => {
  for (const bad of [
    "<svg><!-- c --></svg>",
    "<svg><![CDATA[x]]></svg>",
    "<svg><g></svg>",
    "<svg></g></svg>",
    "<svg x=1></svg>",
    "<svg x='1'></svg>",
    '<svg a="1" a="2"></svg>',
    "<svg>&nbsp;</svg>",
    "<svg>a & b</svg>",
    "<svg>&#0;</svg>",
    "<svg></svg><svg></svg>",
    "<g></g>",
    "text",
  ]) {
    assert.equal(svgToHast(bad), null, bad);
  }
});

test("decodes the entities the serializer writes and splits class", () => {
  const tree = svgToHast('<svg class="a  b"><text data-merlion-id="x&quot;y">&lt;&amp;&gt;&#39;&#x1F600;</text></svg>\n');
  assert.deepEqual(tree.properties, { className: ["a", "b"] });
  assert.deepEqual(tree.children[0].properties, { dataMerlionId: 'x"y' });
  assert.equal(tree.children[0].children[0].value, "<&>'😀");
});

test("the plugin's figure compiles as MDX", async () => {
  const doc = "# Doc\n\n```mermaid\nflowchart LR\n  a[Start] --> b{Ok?}\n```\n\n<Note id=\"n\" />\n";
  const out = await compile(new VFile({ path: "/site/page.mdx", cwd: "/site", value: doc }), {
    rehypePlugins: [[rehypeMerlion, { fontCss: true }]],
  });
  const js = String(out);
  assert.match(js, /_jsx\(_components\.svg|"svg"/);
  assert.match(js, /merlion-view/);
});
