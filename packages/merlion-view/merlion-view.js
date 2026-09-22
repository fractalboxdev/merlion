// <merlion-view>: pan, zoom and fullscreen for any inline SVG (specs/viewer.md).
// Without JavaScript, or before upgrade, the SVG inside renders as a static diagram.
import {
  IDENTITY,
  zoomAt,
  panBy,
  wheelFactor,
  pinchFactor,
  keyView,
  semanticLimit,
  needsControls,
  clampView,
  fitsBox,
  viewBoxSize,
  transformOf,
  panIntent,
  resetsOnDoubleClick,
  isTap,
  place,
} from "./zoom.js";

// Loading the module outside a browser (SSR, tests) defines the class and registers nothing.
const Base = globalThis.HTMLElement ?? class {};

const icon = (d) =>
  `<svg viewBox="0 0 16 16" aria-hidden="true" focusable="false"><path d="${d}"/></svg>`;

// [action, aria-label, icon path]
const BUTTONS = [
  ["+", "Zoom in", "M8 3v10M3 8h10"],
  ["-", "Zoom out", "M3 8h10"],
  ["0", "Reset zoom", "M3.5 8a4.5 4.5 0 1 0 1.3-3.2M4.5 2v3h3"],
  ["f", "Fullscreen", "M2 6V2h4m4 0h4v4m0 4v4h-4m-4 0H2v-4"],
  // Shown while an extension hides part of the drawing (host attribute data-merlion-hidden).
  ["s", "Show all", "M1.5 8S4 3.5 8 3.5 14.5 8 14.5 8 12 12.5 8 12.5 1.5 8 1.5 8ZM8 6a2 2 0 1 0 0 4 2 2 0 0 0 0-4"],
];

// Controls fade in on hover or focus and stay visible on touch devices (`hover: none`).
const CSS = `:host{display:grid;align-items:center;justify-items:center;position:relative;overflow:hidden;touch-action:pan-x pan-y}
:host([zoomed]){touch-action:none}
:host([pannable]){cursor:grab}
:host([controls="always"]) .c{opacity:1}
:host(:focus-visible){outline:2px solid var(--merlion-accent,#0969da);outline-offset:2px}
.c{position:absolute;top:6px;right:6px;display:flex;gap:4px;opacity:.4;transition:opacity .15s}
.c[hidden],:host(:not([data-merlion-hidden])) [data-a=s]{display:none}
:host(:hover) .c,:host(:focus-within) .c{opacity:1}
@media (hover:none){.c{opacity:1}}
@media (prefers-reduced-motion:reduce){.c{transition:none}}
button{display:grid;place-items:center;width:28px;height:28px;padding:0;border:1px solid var(--merlion-border,#d0d7de);border-radius:6px;background:var(--merlion-bg,#fff);color:var(--merlion-fg,#1f2328);cursor:pointer}
button:focus-visible{outline:2px solid var(--merlion-accent,#0969da);outline-offset:1px}
button svg{width:16px;height:16px;fill:none;stroke:currentColor;stroke-width:1.5;stroke-linecap:round;stroke-linejoin:round}
.sr{position:absolute;clip-path:inset(50%)}`;

// The popover's style is inline (CSSOM, allowed under a strict CSP), because in fullscreen it
// lives in the light-DOM dialog, out of reach of the shadow style.
const POP =
  "position:fixed;z-index:9;box-sizing:border-box;padding:6px 10px;overflow:auto;pointer-events:none;text-align:left;overflow-wrap:anywhere;font:13px/1.45 system-ui,sans-serif;border:1px solid #8886;border-radius:6px;background:var(--merlion-bg,#fff);color:var(--merlion-fg,#1f2328)";

const reducedMotion = () => globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches;

const isButton = (e) => e.composedPath().some((n) => n.localName === "button");

// What a pointer is over (specs/viewer.md#gestures): label text, a node or edge, or background.
// Merlion classes first, then mermaid's, so the rules hold for any wrapped SVG.
const hitOf = (t) =>
  t.closest?.("text,foreignObject")
    ? "text"
    : t.closest?.(".merlion-node,.merlion-edge,.node,.edgePath,.edgeLabel")
      ? "shape"
      : "bg";

const selected = () => !!globalThis.getSelection?.()?.toString();

// Every connected host, so an extension registered late reaches hosts already on the page.
const hosts = new Set();
const exts = [];

// Rules for the slotted SVG, which the shadow style cannot reach: one constructed sheet, adopted
// by each host's root (document or shadow root) and extended through MerlionView.style. Adopted
// sheets apply under a CSP without 'unsafe-inline'. Label text keeps the I-beam over a pannable
// drawing, since a drag on text selects it (specs/viewer.md#gestures).
let lightCss = "merlion-view :is(text,foreignObject){cursor:text}";
const light = globalThis.CSSStyleSheet && new CSSStyleSheet();
light?.replaceSync(lightCss);

export class MerlionView extends Base {
  #v = IDENTITY;
  #svg = null;
  #ctl;
  #dlg = null;
  #home = null; // the node the SVG sat before when it moved into the dialog
  #style = null; // the SVG's own style attribute, restored on dialog close
  #pts = new Map(); // active pointers: id → [clientX, clientY]
  #gesture = 1; // last Safari gesture scale
  #ro = null;
  #offs = []; // what each extension returned for the current SVG
  #pop; // popover for extensions (tip)
  #anchor = null; // the element the popover describes
  #live; // polite live region for extensions (say)
  #at = []; // where the last primary pointer went down

  /**
   * Extension hook (specs/interaction.md#loading): `fn(host, svg)` runs whenever a host adopts
   * an SVG and returns a cleanup function, called when the SVG changes or the host disconnects.
   * A `view` method on that function is called after every view change.
   */
  static extend(fn) {
    exts.push(fn);
    for (const h of hosts) h.#hook();
  }

  /** Add rules for the SVG inside every host (they live in the host's root, not its shadow). */
  static style(css) {
    light?.replaceSync((lightCss += css));
  }

  constructor() {
    super();
    const root = this.attachShadow({ mode: "open" });
    root.innerHTML =
      `<style>${CSS}</style><slot></slot><div class="c" part="controls" hidden>` +
      BUTTONS.map(
        ([a, label, d]) =>
          `<button type="button" data-a="${a}" aria-label="${label}" title="${label}">${icon(d)}</button>`,
      ).join("") +
      `</div><div class="sr" role="status" aria-live="polite"></div>`;
    this.#ctl = root.querySelector(".c");
    this.#live = root.querySelector(".sr");
    const pop = (this.#pop = document.createElement("div"));
    pop.part = "tooltip";
    pop.ariaHidden = "true"; // the live region speaks the same text
    pop.style.cssText = POP;
    // A zoom animation moves the anchor; place the popover again once it ends.
    this.addEventListener("transitionend", (e) => e.target === this.#svg && this.#tip());
    this.#ctl.addEventListener("click", (e) => {
      const a = e.target.closest?.("button")?.dataset.a;
      if (a === "f") this.#fullscreen();
      else if (a === "s") this.dispatchEvent(new Event("merlion-show-all"));
      else if (a) this.#set(keyView(a, this.#v, this.#box()), true);
    });
    root.querySelector("slot").addEventListener("slotchange", () => this.#adopt());

    this.addEventListener("wheel", this.#wheel, { passive: false });
    this.addEventListener("pointerdown", this.#down);
    this.addEventListener("pointermove", this.#move);
    this.addEventListener("pointerup", this.#up);
    this.addEventListener("pointercancel", this.#up);
    this.addEventListener("dblclick", (e) => {
      if (!isButton(e) && resetsOnDoubleClick(hitOf(e.target), selected())) this.#set(IDENTITY, true);
    });
    this.addEventListener("keydown", this.#key);
    // Safari reports trackpad pinch as gesture events rather than ctrl+wheel.
    this.addEventListener("gesturestart", (e) => {
      e.preventDefault();
      this.#gesture = 1;
    });
    this.addEventListener("gesturechange", (e) => {
      e.preventDefault();
      const [x, y] = this.#local(e.clientX, e.clientY);
      this.#set(zoomAt(this.#v, pinchFactor(this.#gesture, e.scale), x, y));
      this.#gesture = e.scale;
    });
  }

  connectedCallback() {
    if (!this.hasAttribute("tabindex")) this.tabIndex = 0;
    if (!this.hasAttribute("role")) this.setAttribute("role", "group");
    this.#ro ??= new ResizeObserver(() => this.#refresh());
    this.#ro.observe(this);
    hosts.add(this);
    const root = this.getRootNode();
    if (!root.adoptedStyleSheets.includes(light)) root.adoptedStyleSheets = [...root.adoptedStyleSheets, light];
    this.#adopt();
    if (!this.#offs.length) this.#hook();
  }

  disconnectedCallback() {
    this.#ro?.disconnect();
    hosts.delete(this);
    this.#hook();
  }

  static get observedAttributes() {
    return ["controls", "interactive"];
  }

  attributeChangedCallback(name) {
    if (name === "interactive") this.#hook();
    else this.#refresh();
  }

  // Re-run the extensions against the current SVG; `interactive="off"` runs none. A Merlion
  // SVG loads the interaction module on first sight (specs/viewer.md#interaction).
  #hook() {
    for (const off of this.#offs) off?.();
    this.tip(null);
    const svg = this.isConnected && this.getAttribute("interactive") !== "off" ? this.#svg : null;
    this.#offs = svg ? exts.map((f) => f(this, svg)) : [];
    if (svg?.classList.contains("merlion") && !exts.length) import("./interact.js").catch(() => {});
  }

  /** Current view { s, x, y }. */
  get view() {
    return this.#v;
  }

  /** Reset to fit. */
  reset() {
    this.#set(IDENTITY, true);
  }

  /**
   * Show a popover next to `el`, an element of the drawing, filled by `build(popover)`; `null`
   * hides it. It stays inside the visible box and off `el` through every view change
   * (specs/interaction.md#placement).
   */
  tip(el, build) {
    this.#anchor = el;
    if (!el) return this.#pop.remove();
    this.#pop.replaceChildren();
    build(this.#pop);
    this.#tip();
  }

  /**
   * True when click `e` is a tap on the drawing: not on a viewer control, not the end of a drag
   * or a text selection, and not a double-click's second click (specs/viewer.md#gestures).
   */
  tap(e) {
    return !isButton(e) && isTap(e.clientX - this.#at[0], e.clientY - this.#at[1], e.detail, selected());
  }

  /** Read `text` to screen readers through a polite live region. */
  say(text) {
    this.#live.textContent = text;
  }

  // Place the popover: in the fullscreen dialog while it is open (the top layer covers the host).
  #tip() {
    const el = this.#anchor;
    if (!el) return;
    const p = this.#pop;
    const s = p.style;
    const box = this.#dlg?.open ? this.#dlg : this;
    (box === this ? this.shadowRoot : box).append(p);
    const b = box.getBoundingClientRect();
    s.left = s.top = 0;
    s.maxHeight = "";
    s.maxWidth = `${Math.min(320, b.width * 0.9)}px`;
    const q = place(el.getBoundingClientRect(), b, p.offsetWidth, p.offsetHeight);
    s.visibility = q ? "" : "hidden";
    if (!q) return;
    s.left = `${q.x}px`;
    s.top = `${q.y}px`;
    if (q.h) s.maxHeight = `${q.h}px`;
    if (q.w) s.maxWidth = `${q.w}px`;
  }

  // Pick up the SVG child, now or after it is inserted later.
  #adopt() {
    // While the SVG sits in the fullscreen dialog it is still ours.
    if (this.#dlg && this.#svg?.parentNode === this.#dlg) return;
    const svg = this.querySelector(":scope > svg");
    if (svg === this.#svg) return this.#refresh();
    this.#svg = svg;
    this.#v = IDENTITY;
    // Accessible name from the SVG's <title> (specs/viewer.md#behaviour).
    const title = svg?.querySelector(":scope > title")?.textContent.trim();
    if (title && !this.hasAttribute("aria-label")) this.setAttribute("aria-label", title);
    if (svg) svg.style.transformOrigin = "0 0";
    this.#refresh();
    this.#hook();
  }

  #refresh() {
    const vb = viewBoxSize(this.#svg?.getAttribute("viewBox"));
    this.#ctl.hidden = !this.#svg || !needsControls(vb?.w, this.clientWidth, this.getAttribute("controls"));
    this.#semantic();
    this.#pannable();
  }

  #set(v, animate) {
    const svg = this.#svg;
    if (!svg || !v) return;
    // Panning and zooming never push the drawing out of sight (specs/viewer.md#behaviour).
    if (v !== IDENTITY) v = clampView(v, this.#content(), this.#frame());
    // CSS transform only: the SVG is never re-rasterised, so text stays vector-sharp.
    svg.style.transition = animate && !reducedMotion() ? "transform .2s ease-out" : "";
    this.#v = v;
    svg.style.transform = transformOf(v);
    this.toggleAttribute("zoomed", v !== IDENTITY && (v.s !== 1 || v.x !== 0 || v.y !== 0));
    this.#semantic();
    this.#pannable();
  }

  // True while the drawing is larger than the box, so a background drag has something to reveal.
  #fits() {
    return fitsBox(this.#v, this.#content(), this.#frame());
  }

  #pannable() {
    this.toggleAttribute("pannable", !!this.#svg && !this.#fits());
    for (const off of this.#offs) off?.view?.();
    this.#tip();
  }

  // Semantic zoom (specs/viewer.md#semantic-zoom): only the host's class and attribute change.
  #semantic() {
    const svg = this.#svg;
    const vb = viewBoxSize(svg?.getAttribute("viewBox"));
    let limit = null;
    if (vb) {
      // CSS px per viewBox unit at fit; client sizes ignore the transform.
      const fit = Math.min(svg.clientWidth / vb.w, (svg.clientHeight || Infinity) / vb.h);
      const fs = parseFloat(getComputedStyle(svg).getPropertyValue("--merlion-font-size")) || 14;
      if (fit > 0) limit = semanticLimit(fs, fit, this.#v.s);
    }
    this.classList.toggle("merlion-zoomed-out", limit !== null);
    if (limit === null) this.removeAttribute("data-merlion-rank-limit");
    else this.setAttribute("data-merlion-rank-limit", limit);
  }

  // Client coordinates → coordinates relative to the SVG's untransformed top-left.
  // The on-screen box includes the transform as currently drawn, which lags `#v` while
  // a zoom animates; subtracting the drawn translation (not `#v`) keeps the result exact
  // mid-transition.
  #local(cx, cy) {
    const svg = this.#svg;
    const r = svg.getBoundingClientRect();
    const t = getComputedStyle(svg).transform;
    const m = t && t !== "none" && globalThis.DOMMatrixReadOnly ? new DOMMatrixReadOnly(t) : null;
    return [cx - (r.left - (m ? m.e : 0)), cy - (r.top - (m ? m.f : 0))];
  }

  // The SVG's untransformed size (layout size ignores the CSS transform).
  #content() {
    return { w: this.#svg.clientWidth, h: this.#svg.clientHeight };
  }

  // The visible box relative to the SVG's untransformed top-left corner.
  #frame() {
    const { cx, cy, w, h } = this.#box();
    return { x: cx - w / 2, y: cy - h / 2, w, h };
  }

  // Centre and size of the visible box (the host, or the dialog in fullscreen).
  #box() {
    const r = (this.#dlg?.open ? this.#dlg : this).getBoundingClientRect();
    const [cx, cy] = this.#local(r.left + r.width / 2, r.top + r.height / 2);
    return { cx, cy, w: r.width, h: r.height };
  }

  // Plain wheel scrolls the page; ctrl/⌘+wheel and trackpad pinch (which browsers
  // report as ctrl+wheel) zoom. In fullscreen the page cannot scroll, so any wheel zooms.
  #wheel = (e) => {
    if (!this.#svg || !(e.ctrlKey || e.metaKey || this.#dlg?.open)) return;
    e.preventDefault();
    const [x, y] = this.#local(e.clientX, e.clientY);
    this.#set(zoomAt(this.#v, wheelFactor(e.deltaY, e.deltaMode), x, y));
  };

  // A drag selects text natively unless it pans (specs/viewer.md#gestures): only a pan
  // suppresses the default action and captures the pointer. Touch pointers are tracked
  // for pinch whatever they start on.
  #down = (e) => {
    if (!this.#svg || isButton(e) || (e.pointerType === "mouse" && e.button !== 0)) return;
    this.#at = [e.clientX, e.clientY];
    const touch = e.pointerType === "touch";
    const hit = hitOf(e.target);
    // Shift+click on a node or edge is a command (path mode), never a selection extension.
    if (e.shiftKey && hit === "shape") e.preventDefault();
    const pan = panIntent(hit, e.pointerType, this.#fits(), this.hasAttribute("zoomed"));
    if (!pan && !touch) return;
    this.#pts.set(e.pointerId, [e.clientX, e.clientY, pan]);
    if (!pan) return;
    if (!touch) {
      e.preventDefault(); // no text selection while panning
      this.focus({ preventScroll: true });
    }
    try {
      this.setPointerCapture(e.pointerId);
    } catch {
      // Synthetic or already-released pointer: panning still works without capture.
    }
  };

  #move = (e) => {
    const p = this.#pts.get(e.pointerId);
    if (!p) return;
    const q = [e.clientX, e.clientY, p[2]];
    this.#pts.set(e.pointerId, q);
    if (this.#pts.size === 1) {
      if (p[2]) this.#set(panBy(this.#v, q[0] - p[0], q[1] - p[1]));
    } else if (this.#pts.size === 2) {
      let o;
      for (const [id, pt] of this.#pts) if (id !== e.pointerId) o = pt;
      const d0 = Math.hypot(p[0] - o[0], p[1] - o[1]);
      const d1 = Math.hypot(q[0] - o[0], q[1] - o[1]);
      const [x, y] = this.#local((q[0] + o[0]) / 2, (q[1] + o[1]) / 2);
      this.#set(zoomAt(this.#v, pinchFactor(d0, d1), x, y));
    }
  };

  #up = (e) => {
    this.#pts.delete(e.pointerId);
  };

  #key = (e) => {
    // An extension that consumes a key (arrows between nodes) prevents its default.
    if (!this.#svg || e.defaultPrevented || e.ctrlKey || e.metaKey || e.altKey) return;
    const v = keyView(e.key, this.#v, this.#box());
    if (!v) return;
    e.preventDefault();
    this.#set(v, true);
  };

  // Fullscreen moves (never clones) the SVG into a modal <dialog>, so its ids
  // stay unique on the page. The dialog is a light-DOM child of the host, so it
  // inherits the page's theme variables and the host's semantic-zoom attribute.
  #fullscreen() {
    const svg = this.#svg;
    if (!svg || typeof HTMLDialogElement === "undefined") return;
    let d = this.#dlg;
    if (!d) {
      d = this.#dlg = document.createElement("dialog");
      d.className = "merlion-view-dialog";
      d.style.cssText =
        "width:100vw;height:100vh;max-width:none;max-height:none;margin:0;padding:0;border:0;overflow:hidden;background:var(--merlion-bg,#fff);color:var(--merlion-fg,#1f2328)";
      const close = document.createElement("button");
      close.type = "button";
      close.setAttribute("aria-label", "Close fullscreen");
      close.textContent = "×";
      close.style.cssText =
        "position:absolute;top:12px;right:12px;width:36px;height:36px;font:24px/1 system-ui,sans-serif;border:1px solid var(--merlion-border,#d0d7de);border-radius:8px;background:var(--merlion-bg,#fff);color:inherit;cursor:pointer";
      close.addEventListener("click", () => this.#restore());
      d.append(close);
      // Esc fires `cancel` synchronously; `close` covers a dialog closed by script.
      // #restore is idempotent and closes the dialog itself.
      d.addEventListener("close", () => this.#restore());
      d.addEventListener("cancel", () => this.#restore());
      this.append(d);
    }
    d.setAttribute("aria-label", this.getAttribute("aria-label") || "Diagram");
    this.#home = svg.nextSibling;
    this.#style = svg.getAttribute("style");
    d.prepend(svg);
    svg.style.cssText += ";width:100%;height:100%;max-width:none;display:block";
    this.#v = IDENTITY;
    svg.style.transform = "";
    d.showModal();
    this.#semantic();
    this.#pannable();
  }

  #restore() {
    const svg = this.#svg;
    if (!svg || svg.parentNode !== this.#dlg) return;
    if (this.#dlg.open) this.#dlg.close();
    this.insertBefore(svg, this.#home?.parentNode === this ? this.#home : this.#dlg);
    if (this.#style === null) svg.removeAttribute("style");
    else svg.setAttribute("style", this.#style);
    svg.style.transformOrigin = "0 0";
    this.#set(IDENTITY);
    this.focus({ preventScroll: true });
  }
}

if (globalThis.customElements && !customElements.get("merlion-view")) {
  customElements.define("merlion-view", MerlionView);
}
