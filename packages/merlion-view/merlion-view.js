// <merlion-view>: pan, zoom and fullscreen for any inline SVG (specs/viewer.md).
// Without JavaScript, or before upgrade, the SVG inside renders as a static diagram.
import {
  IDENTITY,
  zoomAt,
  panBy,
  wheelFactor,
  pinchFactor,
  keyView,
  labelPx,
  rankLimit,
  needsControls,
  viewBoxSize,
  transformOf,
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
];

// Controls fade in on hover or focus and stay visible on touch devices (`hover: none`).
const CSS = `:host{display:block;position:relative;overflow:hidden;touch-action:pan-x pan-y}
:host([zoomed]){touch-action:none;cursor:grab}
:host(:focus-visible){outline:2px solid var(--merlion-accent,#0969da);outline-offset:2px}
.c{position:absolute;top:6px;right:6px;display:flex;gap:4px;opacity:0;transition:opacity .15s}
.c[hidden]{display:none}
:host(:hover) .c,:host(:focus-within) .c{opacity:1}
@media (hover:none){.c{opacity:1}}
@media (prefers-reduced-motion:reduce){.c{transition:none}}
button{display:grid;place-items:center;width:28px;height:28px;padding:0;border:1px solid var(--merlion-border,#d0d7de);border-radius:6px;background:var(--merlion-bg,#fff);color:var(--merlion-fg,#1f2328);cursor:pointer}
button:focus-visible{outline:2px solid var(--merlion-accent,#0969da);outline-offset:1px}
button svg{width:16px;height:16px;fill:none;stroke:currentColor;stroke-width:1.5;stroke-linecap:round;stroke-linejoin:round}`;

const reducedMotion = () => globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches;

const isButton = (e) => e.composedPath().some((n) => n.localName === "button");

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

  constructor() {
    super();
    const root = this.attachShadow({ mode: "open" });
    root.innerHTML =
      `<style>${CSS}</style><slot></slot><div class="c" part="controls" hidden>` +
      BUTTONS.map(
        ([a, label, d]) =>
          `<button type="button" data-a="${a}" aria-label="${label}" title="${label}">${icon(d)}</button>`,
      ).join("") +
      `</div>`;
    this.#ctl = root.querySelector(".c");
    this.#ctl.addEventListener("click", (e) => {
      const a = e.target.closest?.("button")?.dataset.a;
      if (a === "f") this.#fullscreen();
      else if (a) this.#set(keyView(a, this.#v, this.#box()), true);
    });
    root.querySelector("slot").addEventListener("slotchange", () => this.#adopt());

    this.addEventListener("wheel", this.#wheel, { passive: false });
    this.addEventListener("pointerdown", this.#down);
    this.addEventListener("pointermove", this.#move);
    this.addEventListener("pointerup", this.#up);
    this.addEventListener("pointercancel", this.#up);
    this.addEventListener("dblclick", (e) => isButton(e) || this.#set(IDENTITY, true));
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
    this.#adopt();
  }

  disconnectedCallback() {
    this.#ro?.disconnect();
  }

  static get observedAttributes() {
    return ["controls"];
  }

  attributeChangedCallback() {
    this.#refresh();
  }

  /** Current view { s, x, y }. */
  get view() {
    return this.#v;
  }

  /** Reset to fit. */
  reset() {
    this.#set(IDENTITY, true);
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
  }

  #refresh() {
    const vb = viewBoxSize(this.#svg?.getAttribute("viewBox"));
    this.#ctl.hidden = !this.#svg || !needsControls(vb?.w, this.clientWidth, this.getAttribute("controls"));
    this.#semantic();
  }

  #set(v, animate) {
    const svg = this.#svg;
    if (!svg || !v) return;
    this.#v = v;
    // CSS transform only: the SVG is never re-rasterised, so text stays vector-sharp.
    svg.style.transition = animate && !reducedMotion() ? "transform .2s ease-out" : "";
    svg.style.transform = transformOf(v);
    this.toggleAttribute("zoomed", v !== IDENTITY && (v.s !== 1 || v.x !== 0 || v.y !== 0));
    this.#semantic();
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
      if (fit > 0) limit = rankLimit(labelPx(fs, fit, this.#v.s));
    }
    this.classList.toggle("merlion-zoomed-out", limit !== null);
    if (limit === null) this.removeAttribute("data-merlion-rank-limit");
    else this.setAttribute("data-merlion-rank-limit", limit);
  }

  // Client coordinates → coordinates relative to the SVG's untransformed top-left.
  #local(cx, cy) {
    const r = this.#svg.getBoundingClientRect();
    return [cx - (r.left - this.#v.x), cy - (r.top - this.#v.y)];
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

  #down = (e) => {
    if (!this.#svg || isButton(e) || (e.pointerType === "mouse" && e.button !== 0)) return;
    this.#pts.set(e.pointerId, [e.clientX, e.clientY]);
    if (e.pointerType === "mouse") {
      e.preventDefault(); // no text selection while dragging
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
    const q = [e.clientX, e.clientY];
    this.#pts.set(e.pointerId, q);
    if (this.#pts.size === 1) {
      // One finger pans only when zoomed in; otherwise the page scrolls.
      if (e.pointerType !== "touch" || this.hasAttribute("zoomed")) {
        this.#set(panBy(this.#v, q[0] - p[0], q[1] - p[1]));
      }
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
    if (!this.#svg || e.ctrlKey || e.metaKey || e.altKey) return;
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
