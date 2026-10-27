// Real-browser regressions for XML descriptions and native detail-panel scrolling.
import { after, before, test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const repo = new URL("../../", import.meta.url);
const binary = process.env.MERLION_BIN ?? fileURLToPath(new URL("target/release/merlion", repo));
const modules = new Map(["merlion-view.js", "zoom.js", "interact.js", "interact-details.js", "interact-model.js", "interact-seq.js", "interact-state.js"].map((name) => [
  `/${name}`, readFileSync(new URL(`packages/merlion-view/${name}`, repo), "utf8"),
]));
const render = (source) => {
  const result = JSON.parse(execFileSync(binary, ["render", "--json", "--id-prefix", "viewer-test"], { input: source, encoding: "utf8" }));
  assert.equal(result.error, null, JSON.stringify(result.diagnostics));
  return result.svg;
};
let browser;
before(async () => {
  browser = await chromium.launch(process.env.MERLION_BENCH_CHROMIUM ? { executablePath: process.env.MERLION_BENCH_CHROMIUM } : {});
});
after(async () => { await browser?.close(); });

const open = async (svg, wide = false) => {
  const page = await browser.newPage({ viewport: { width: 1100, height: 700 }, reducedMotion: "reduce" });
  page.setDefaultTimeout(5000);
  await page.route("**/*", async (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path === "/") return route.fulfill({ contentType: "text/html", body: '<!doctype html><style>merlion-view{display:block;width:440px;height:260px;margin:70px;--merlion-bg:white}</style><script type="module">import "/interact.js";window.ready=true;</script>' });
    if (modules.has(path)) return route.fulfill({ contentType: "application/javascript", body: modules.get(path) });
    return route.abort();
  });
  await page.goto("http://merlion.test/");
  await page.waitForFunction(() => window.ready);
  const parsed = await page.evaluate(({ svg, wide }) => {
    const xml = new DOMParser().parseFromString(svg, "image/svg+xml");
    const error = xml.querySelector("parsererror")?.textContent;
    if (error) return { error };
    const node = xml.querySelector("[data-merlion-description]");
    const description = node?.getAttribute("data-merlion-description");
    const host = document.createElement("merlion-view");
    host.setAttribute("controls", "always");
    const drawing = document.importNode(xml.documentElement, true);
    drawing.style.cssText = `width:${wide ? 1000 : 400}px;max-width:none;flex:none`;
    host.append(drawing);
    document.body.append(host);
    return { description };
  }, { svg, wide });
  assert.equal(parsed.error, undefined);
  await page.waitForFunction(() => document.querySelector("merlion-view")?.hasAttribute("data-merlion-interactive"));
  return { page, description: parsed.description };
};

for (const header of ["flowchart LR\nA[Alpha] --> B[Beta]", 'stateDiagram-v2\nstate "Alpha" as A\nA --> B']) {
  test(`${header.split("\n")[0]} descriptions survive an XML round trip`, async () => {
    const svg = render(`${header}\nclick A href "/details" "First line.<br/>Second line."\n`);
    const { page, description } = await open(svg);
    try {
      assert.equal(description, "First line.\nSecond line.");
      await page.locator('[data-merlion-id="A"]').click();
      await page.getByRole("button", { name: "Close details" }).waitFor();
      assert.equal(await page.getByRole("region", { name: "Alpha details" }).locator("div").nth(1).textContent(), "First line.\nSecond line.");
    } finally { await page.close(); }
  });
}

for (const fullscreen of [false, true]) {
  test(`detail-panel arrow keys scroll natively${fullscreen ? " in fullscreen" : ""} and Escape closes details`, async () => {
    const details = Array.from({ length: 70 }, (_, i) => `Explanation paragraph ${i + 1}.`).join("<br/>");
    const { page } = await open(render(`flowchart LR\nA[Alpha] --> B[Beta]\nclick A href "/details" "${details}"\n`), true);
    try {
      if (fullscreen) await page.getByRole("button", { name: "Fullscreen", exact: true }).click();
      await page.locator('[data-merlion-id="A"]').click();
      const close = page.getByRole("button", { name: "Close details" });
      await close.waitFor();
      await close.focus();
      const before = await page.evaluate(() => {
        const host = document.querySelector("merlion-view");
        const pop = host.shadowRoot.querySelector('[part="tooltip"]') ?? host.querySelector('[part="tooltip"]');
        pop.scrollTop = pop.scrollHeight;
        return { scroll: pop.scrollTop, transform: host.querySelector("svg").style.transform, panel: { height: pop.clientHeight, content: pop.scrollHeight, css: pop.style.cssText, text: pop.textContent.length } };
      });
      assert.ok(before.scroll > 0, `the explanation genuinely overflows its panel: ${JSON.stringify(before.panel)}`);
      await page.keyboard.press("ArrowUp");
      await page.waitForFunction((scroll) => {
        const host = document.querySelector("merlion-view");
        const pop = host.shadowRoot.querySelector('[part="tooltip"]') ?? host.querySelector('[part="tooltip"]');
        return pop.scrollTop < scroll;
      }, before.scroll, { timeout: 3000 });
      assert.equal(await page.locator("merlion-view svg").first().evaluate((svg) => svg.style.transform), before.transform, "scrolling the panel never pans the drawing");
      await page.keyboard.press("Escape");
      await close.waitFor({ state: "detached" });
      if (fullscreen) assert.equal(await page.locator("dialog").evaluate((dialog) => dialog.open), true);
      else assert.equal(await page.locator("merlion-view").evaluate((host) => document.activeElement === host), true);
    } finally { await page.close(); }
  });
}
