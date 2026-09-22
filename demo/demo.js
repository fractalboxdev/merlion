// Demo page: theme toggle, gallery of pre-rendered SVG, and a live editor over
// @fractalboxdev/merlion-wasm. Every missing piece degrades to a visible message.

const $ = (sel) => document.querySelector(sel);
const notices = $("#notices");

const notice = (html) => {
  const p = document.createElement("p");
  p.className = "notice";
  p.innerHTML = html;
  notices.append(p);
};

const h = (tag, props = {}, ...children) => {
  const el = document.createElement(tag);
  Object.assign(el, props);
  el.append(...children);
  return el;
};

// ---- Theme -----------------------------------------------------------------

const themeBtn = $("#theme");
const applyTheme = (t) => {
  document.documentElement.dataset.theme = t;
  themeBtn.setAttribute("aria-pressed", String(t === "dark"));
  themeBtn.querySelector(".theme-label").textContent = t === "dark" ? "Dark" : "Light";
};
applyTheme(document.documentElement.dataset.theme === "dark" ? "dark" : "light");
themeBtn.addEventListener("click", () => {
  const next = document.documentElement.dataset.theme === "dark" ? "light" : "dark";
  applyTheme(next);
  try {
    localStorage.setItem("merlion-demo-theme", next);
  } catch {
    // Storage unavailable (private window): the choice lasts for this page view.
  }
});

// ---- Stylesheet and viewer -------------------------------------------------

const themesLink = $("#themes-css");
const themesMissing = () =>
  notice(
    "<strong>merlion-themes.css</strong> did not load (<code>packages/merlion-themes/merlion-themes.css</code>). Diagrams use their built-in light colours and the demo's fallback dark tokens.",
  );
// A 404 stylesheet still yields an (empty) CSSStyleSheet in some browsers, so ask the server.
fetch(themesLink.href, { method: "HEAD" })
  .then((r) => r.ok || themesMissing())
  .catch(themesMissing);

import("../packages/merlion-view/merlion-view.js").catch(() =>
  notice(
    "<strong>&lt;merlion-view&gt;</strong> did not load (<code>packages/merlion-view/merlion-view.js</code>). Diagrams render static, without zoom.",
  ),
);

// ---- SVG insertion ---------------------------------------------------------

// Parse as XML so a malformed file fails visibly instead of half-rendering.
const parseSvg = (text) => {
  const doc = new DOMParser().parseFromString(text, "image/svg+xml");
  const svg = doc.documentElement;
  if (svg.localName !== "svg" || doc.querySelector("parsererror")) return null;
  return document.importNode(svg, true);
};

const placeholder = (html) => h("p", { className: "placeholder", innerHTML: html });

// ---- Renderer --------------------------------------------------------------

const wasmReady = (async () => {
  try {
    const mod = await import("../packages/merlion-wasm/index.js");
    await mod.init();
    return mod;
  } catch (err) {
    console.warn("merlion-wasm unavailable:", err);
    return null;
  }
})();

// ---- Gallery ---------------------------------------------------------------

const cards = $("#cards");
const roleCards = $("#role-cards");

/** Standalone renders with a theme baked in, shown as images so no page CSS reaches them. */
const bakedFigures = (baked) =>
  h(
    "div",
    { className: "baked" },
    ...baked.map((b) =>
      h(
        "figure",
        { className: `baked-${b.theme}` },
        h("img", { src: b.file, alt: `The same diagram rendered standalone with the ${b.theme} theme baked in` }),
        h("figcaption", { textContent: `Standalone, ${b.theme} theme baked in` }),
      ),
    ),
  );

const loadCard = async (entry, i, wasm) => {
  const view = document.createElement("merlion-view");
  const body = h("div", {}, view);
  const card = h("article", { className: "card" }, h("h3", { textContent: entry.title ?? entry.file }), body);
  if (Array.isArray(entry.baked) && entry.baked.length) {
    card.classList.add("wide");
    card.append(bakedFigures(entry.baked));
  }
  if (entry.source) {
    card.append(
      h(
        "details",
        {},
        h("summary", { textContent: "Diagram source" }),
        h("pre", {}, h("code", { className: "language-mermaid", textContent: entry.source })),
      ),
    );
  }
  (entry.origin === "roles" ? roleCards : cards).append(card);

  let svg = null;
  try {
    const res = await fetch(new URL(entry.file, location.href));
    if (res.ok) svg = parseSvg(await res.text());
  } catch {
    svg = null;
  }
  if (!svg && entry.source && wasm) {
    const out = wasm.render(entry.source, { idPrefix: `g${i + 1}` });
    svg = out.svg && parseSvg(out.svg);
    if (!svg) {
      const first = out.diagnostics?.[0];
      const text = `Render failed${first ? `: ${first.code} ${first.message}` : ""}.`;
      body.replaceChildren(h("p", { className: "placeholder", textContent: text }));
      return;
    }
  }
  if (!svg) {
    const hint = entry.source
      ? "Run <code>node demo/build-gallery.mjs</code> once <code>packages/merlion-wasm</code> is built."
      : "";
    body.replaceChildren(placeholder(`<code>demo/${entry.file}</code> is missing. ${hint}`));
    return;
  }
  view.append(svg);
  // Wide diagrams get the full row on two-column layouts.
  const w = Number(svg.getAttribute("viewBox")?.split(/[\s,]+/)[2]);
  if (w > 900) card.classList.add("wide");
};

(async () => {
  let entries;
  try {
    const res = await fetch("gallery.json");
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    entries = await res.json();
    if (!Array.isArray(entries)) throw new Error("not an array");
  } catch (err) {
    cards.append(
      h("p", { className: "placeholder", textContent: `demo/gallery.json could not be loaded (${err.message}).` }),
    );
    return;
  }
  if (entries.length === 0) cards.append(placeholder("The gallery is empty."));
  const wasm = await wasmReady;
  await Promise.all(entries.map((e, i) => loadCard(e, i, wasm)));
})();

// ---- Live editor -----------------------------------------------------------

const src = $("#src");
const live = $("#live");
const preview = $("#preview");
const empty = $("#preview-empty");
const status = $("#status");
const diagList = $("#diagnostics");
let previous = null; // last successful SVG, the next render's layout hint

// Select the diagnostic's position in the textarea (1-based line and column).
const jumpTo = (line, column) => {
  const lines = src.value.split("\n");
  let offset = 0;
  for (let i = 0; i < Math.min(line - 1, lines.length); i++) offset += lines[i].length + 1;
  offset += Math.max(0, column - 1);
  src.focus();
  src.setSelectionRange(offset, Math.min(src.value.length, offset + 1));
};

const showDiagnostics = (list) => {
  diagList.replaceChildren(
    ...list.map((d) => {
      // The @fractalboxdev/merlion-wasm Diagnostic: flat, 1-based, 0 without a location.
      const line = d.line ?? 0;
      const column = d.column ?? 0;
      const btn = h("button", { type: "button" });
      btn.append(
        h("span", { className: `sev-${d.severity}`, textContent: `${d.severity} ${d.code}` }),
        ` ${line}:${column} ${d.message}`,
      );
      if (line > 0) btn.addEventListener("click", () => jumpTo(line, column));
      return h("li", {}, btn);
    }),
  );
};

const renderLive = (wasm) => {
  const t0 = performance.now();
  const options = {
    idPrefix: "live",
    width: Math.max(240, Math.round(preview.clientWidth - 32)),
    ...(previous ? { hint: previous } : {}),
  };
  let out;
  try {
    out = wasm.render(src.value, options);
  } catch (err) {
    out = {
      svg: null,
      diagnostics: [
        { severity: "error", code: "E001", line: 0, column: 0, byteStart: 0, byteEnd: 0, message: String(err), fix: null },
      ],
    };
  }
  const ms = performance.now() - t0;
  showDiagnostics(out.diagnostics ?? []);
  const svg = out.svg && parseSvg(out.svg);
  if (svg) {
    previous = out.svg;
    live.replaceChildren(svg);
    empty.textContent = "";
    preview.classList.remove("stale");
    status.textContent = `${ms.toFixed(1)} ms · ${(out.svg.length / 1024).toFixed(1)} KB`;
  } else {
    preview.classList.add("stale");
    status.textContent = "Not rendered: see diagnostics";
    if (!previous) empty.textContent = "Nothing to show yet.";
  }
};

(async () => {
  const wasm = await wasmReady;
  if (!wasm) {
    empty.innerHTML =
      "The renderer is not available: <code>packages/merlion-wasm/index.js</code> did not load. Build the WASM package and reload.";
    status.textContent = "Renderer offline";
    return;
  }
  let timer = 0;
  src.addEventListener("input", () => {
    clearTimeout(timer);
    timer = setTimeout(() => renderLive(wasm), 150);
  });
  $("#fresh").addEventListener("click", () => {
    previous = null;
    renderLive(wasm);
  });
  renderLive(wasm);
})();
