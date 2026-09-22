/**
 * Sequence-diagram extraction from rendered SVG (specs/benchmark.md#output,
 * specs/sequence.md#svg-output).
 *
 * A sequence drawing carries none of the flowchart graph: there is nothing to
 * route and nothing to cross, so the comparable content is the participant
 * labels, the messages and their labels, the notes and the fragments. Merlion
 * output is read through its data attributes, mermaid output through its class
 * names, with `htmlLabels: false` so every label is a `<text>`.
 *
 * mermaid draws each participant twice, once at the top and once at the foot,
 * and a `create`d or `destroy`ed one only once; the column's x identifies it in
 * either case, so the extractor keys participants by x and keeps the first.
 */
import { type Box, boxOfPoints, type Point, unionBoxes } from "./geom.ts";
import { parsePath } from "./path.ts";
import { findAll, hasClass, parseXml, type XmlElement } from "./xml.ts";
import { type Flavor, labelText } from "./extract.ts";

export interface SequenceParticipant {
  readonly label: string;
  /** Centre of the column; identifies a participant across its two drawn copies. */
  readonly x: number;
  readonly box: Box | null;
}

export interface SequenceMessage {
  readonly label: string;
  /** The label's drawn lines, longest first for the width estimate. */
  readonly lines: readonly string[];
  /** Anchor of the label text, and whether the renderer centres it there. */
  readonly anchor: Point | null;
  readonly centred: boolean;
  /** Background box behind the label; `null` when the renderer draws none. */
  readonly labelBox: Box | null;
}

export interface SequenceNote {
  readonly text: string;
  readonly box: Box | null;
}

export interface SequenceFragment {
  /** The kind keyword drawn in the corner tab, lower-cased: `alt`, `loop`, `opt`, … */
  readonly kind: string;
  /** The header label beside the tab, without mermaid's surrounding brackets. */
  readonly label: string;
}

export interface SequenceDrawing {
  readonly flavor: Flavor;
  readonly viewBox: Box | null;
  readonly participants: readonly SequenceParticipant[];
  readonly messages: readonly SequenceMessage[];
  readonly notes: readonly SequenceNote[];
  readonly fragments: readonly SequenceFragment[];
  /** Font size the renderer draws labels at, used to estimate an unboxed label. */
  readonly fontSize: number;
}

const EMPTY: SequenceDrawing = {
  flavor: "unknown",
  viewBox: null,
  participants: [],
  messages: [],
  notes: [],
  fragments: [],
  fontSize: 14,
};

const num = (s: string | undefined): number | null => {
  if (s === undefined) return null;
  const v = Number.parseFloat(s);
  return Number.isFinite(v) ? v : null;
};

const viewBoxOf = (root: XmlElement): Box | null => {
  const parts = (root.attrs["viewBox"] ?? "").trim().split(/[\s,]+/).map(Number);
  if (parts.length !== 4 || parts.some((p) => !Number.isFinite(p))) return null;
  return { x: parts[0]!, y: parts[1]!, w: parts[2]!, h: parts[3]! };
};

const rectBox = (el: XmlElement): Box | null => {
  const [x, y, w, h] = [num(el.attrs["x"]), num(el.attrs["y"]), num(el.attrs["width"]), num(el.attrs["height"])];
  return x === null || y === null || w === null || h === null ? null : { x, y, w, h };
};

const pathBox = (el: XmlElement): Box | null =>
  boxOfPoints(parsePath(el.attrs["d"] ?? "").flatMap((sp) => sp.points));

/** Bounding box of one drawn shape, whatever element draws it. */
const shapeBox = (el: XmlElement): Box | null => {
  switch (el.name) {
    case "rect":
      return rectBox(el);
    case "path":
      return pathBox(el);
    case "circle": {
      const [cx, cy, r] = [num(el.attrs["cx"]), num(el.attrs["cy"]), num(el.attrs["r"])];
      return cx === null || cy === null || r === null ? null : { x: cx - r, y: cy - r, w: 2 * r, h: 2 * r };
    }
    case "line": {
      const p = [num(el.attrs["x1"]), num(el.attrs["y1"]), num(el.attrs["x2"]), num(el.attrs["y2"])];
      return p.some((v) => v === null) ? null : boxOfPoints([{ x: p[0]!, y: p[1]! }, { x: p[2]!, y: p[3]! }]);
    }
    default:
      return null;
  }
};

/** Bounding box of an element's own shape, else of every shape it contains. */
const subtreeBox = (el: XmlElement): Box | null => {
  const own = shapeBox(el);
  if (own !== null) return own;
  const boxes = findAll(el, (e) => e !== el).flatMap((e) => {
    const b = shapeBox(e);
    return b === null ? [] : [b];
  });
  return unionBoxes(boxes);
};

/** Text of an element, with `<tspan>` lines joined by a space and whitespace collapsed. */
const label = (el: XmlElement | undefined): string => (el === undefined ? "" : labelText([el]));

/** The first `font-size` an inline style or attribute declares, else `fallback`. */
const fontSizeOf = (els: readonly XmlElement[], fallback: number): number => {
  for (const el of els) {
    const inline = /font-size:\s*([\d.]+)px/.exec(el.attrs["style"] ?? "");
    if (inline !== null) return Number.parseFloat(inline[1]!);
    const attr = num(el.attrs["font-size"]);
    if (attr !== null) return attr;
  }
  return fallback;
};

// ---------------------------------------------------------------------------
// Merlion

const merlionDrawing = (root: XmlElement): SequenceDrawing => {
  const participants = findAll(root, (e) => hasClass(e, "merlion-participant")).map((g) => {
    const life = findAll(g, (e) => hasClass(e, "merlion-lifeline"))[0];
    const shape = findAll(g, (e) => hasClass(e, "merlion-shape") && !hasClass(e, "merlion-participant-foot"))[0];
    return {
      label: label(findAll(g, (e) => hasClass(e, "merlion-label"))[0]),
      x: life === undefined ? 0 : (num(life.attrs["x1"]) ?? 0),
      box: shape === undefined ? null : pathBox(shape),
    };
  });

  const messages = findAll(root, (e) => hasClass(e, "merlion-message")).map((g) => {
    const bg = findAll(g, (e) => hasClass(e, "merlion-edge-label-bg"))[0];
    const text = findAll(g, (e) => hasClass(e, "merlion-edge-text"))[0];
    const spans = text === undefined ? [] : findAll(text, (e) => e.name === "tspan");
    const first = spans[0];
    return {
      label: label(text),
      lines: spans.map((s) => label(s)),
      anchor: first === undefined ? null : { x: num(first.attrs["x"]) ?? 0, y: num(first.attrs["y"]) ?? 0 },
      centred: false,
      labelBox: bg === undefined ? null : rectBox(bg),
    };
  });

  const notes = findAll(root, (e) => hasClass(e, "merlion-note")).map((g) => {
    const box = findAll(g, (e) => hasClass(e, "merlion-note-box"))[0];
    return {
      text: label(findAll(g, (e) => hasClass(e, "merlion-label"))[0]),
      box: box === undefined ? null : rectBox(box),
    };
  });

  const fragments = findAll(root, (e) => hasClass(e, "merlion-fragment")).map((g) => ({
    kind: (g.attrs["data-merlion-kind"] ?? label(findAll(g, (e) => hasClass(e, "merlion-cluster-title"))[0])).toLowerCase(),
    label: label(findAll(g, (e) => hasClass(e, "merlion-fragment-label"))[0]),
  }));

  return {
    flavor: "merlion",
    viewBox: viewBoxOf(root),
    participants,
    messages,
    notes,
    fragments,
    fontSize: fontSizeOf(findAll(root, (e) => hasClass(e, "merlion-diagram")), 14),
  };
};

// ---------------------------------------------------------------------------
// mermaid

/** `[is sick]` is how mermaid writes a fragment's header label; the brackets are its own. */
const unbracket = (s: string): string => (/^\[.*\]$/.test(s) ? s.slice(1, -1).trim() : s);

const mermaidDrawing = (root: XmlElement): SequenceDrawing => {
  // A `link` or `links` statement adds a hidden popup menu whose panel and entries
  // carry the actor classes; neither is a participant.
  const popup = (e: XmlElement): boolean => hasClass(e, "actorPopupMenu") || hasClass(e, "actorPopupMenuPanel");
  const shapes = findAll(
    root,
    (e) => !popup(e) && hasClass(e, "actor") && (hasClass(e, "actor-top") || hasClass(e, "actor-bottom")),
  );
  const boxAt = new Map<number, Box>();
  for (const s of shapes) {
    const b = subtreeBox(s);
    if (b === null) continue;
    const cx = Math.round(b.x + b.w / 2);
    if (!boxAt.has(cx)) boxAt.set(cx, b);
  }

  // One `<text>` per drawn line, every line of one label at the same anchor, and the
  // whole label once at the head and once at the foot: group by anchor, keep one per column.
  const byAnchor = new Map<string, { x: number; texts: XmlElement[] }>();
  for (const t of findAll(root, (e) => e.name === "text" && (hasClass(e, "actor-box") || hasClass(e, "actor-man")))) {
    const x = Math.round(num(t.attrs["x"]) ?? 0);
    const key = `${x}@${Math.round(num(t.attrs["y"]) ?? 0)}`;
    const slot = byAnchor.get(key);
    if (slot === undefined) byAnchor.set(key, { x, texts: [t] });
    else slot.texts.push(t);
  }
  const participants: SequenceParticipant[] = [];
  const seen = new Set<number>();
  for (const { x, texts } of byAnchor.values()) {
    if (seen.has(x)) continue;
    seen.add(x);
    participants.push({ label: labelText(texts), x, box: boxAt.get(x) ?? null });
  }
  participants.sort((a, b) => a.x - b.x);

  // One message per drawn line; the labels are a separate, possibly shorter list.
  const lines = findAll(root, (e) => hasClass(e, "messageLine0") || hasClass(e, "messageLine1"));
  const texts = findAll(root, (e) => hasClass(e, "messageText"));
  const messages: SequenceMessage[] = lines.map((_, k) => {
    const t = texts[k];
    const text = t === undefined ? "" : label(t);
    return {
      label: text,
      lines: text === "" ? [] : [text],
      anchor: t === undefined ? null : { x: num(t.attrs["x"]) ?? 0, y: num(t.attrs["y"]) ?? 0 },
      centred: true,
      labelBox: null,
    };
  });

  // mermaid draws one `noteText` per line, so a note's lines are the texts its box holds.
  const noteTexts = findAll(root, (e) => hasClass(e, "noteText"));
  const notes = findAll(root, (e) => hasClass(e, "note") && e.name === "rect").flatMap((r) => {
    const box = rectBox(r);
    if (box === null) return [];
    const inside = noteTexts.filter((t) => {
      const [x, y] = [num(t.attrs["x"]), num(t.attrs["y"])];
      return x !== null && y !== null && x >= box.x && x <= box.x + box.w && y >= box.y - 1 && y <= box.y + box.h + 1;
    });
    return [{ text: labelText(inside), box }];
  });

  const kinds = findAll(root, (e) => hasClass(e, "labelText")).map((t) => label(t).toLowerCase());
  const headers = findAll(root, (e) => hasClass(e, "loopText")).map((t) => unbracket(label(t)));
  const fragments = kinds.map((kind, k) => ({ kind, label: headers[k] ?? "" }));

  return {
    flavor: "mermaid",
    viewBox: viewBoxOf(root),
    participants,
    messages,
    notes,
    fragments,
    fontSize: fontSizeOf(findAll(root, (e) => hasClass(e, "messageText")), 16),
  };
};

/** True when the root is a mermaid sequence drawing. */
const isMermaidSequence = (root: XmlElement): boolean => root.attrs["aria-roledescription"] === "sequence";

/** True when the root is a Merlion sequence drawing. */
const isMerlionSequence = (root: XmlElement): boolean => hasClass(root, "merlion-sequence");

export const extractSequence = (svg: string): SequenceDrawing => {
  let root: XmlElement;
  try {
    root = parseXml(svg);
  } catch {
    return EMPTY;
  }
  if (isMerlionSequence(root)) return merlionDrawing(root);
  if (isMermaidSequence(root)) return mermaidDrawing(root);
  return EMPTY;
};

/** Average advance used to estimate a message label's box (em per character). */
const EM_PER_CHAR = 0.55;

/**
 * The boxes the overlap count runs over: the participant head boxes and note
 * boxes each renderer draws, plus one estimated box per message label. The
 * message label is estimated on both sides — mermaid draws no background box
 * behind one — so the two counts are measured the same way.
 */
export const labelBoxes = (d: SequenceDrawing): Box[] => {
  const out: Box[] = [];
  for (const p of d.participants) if (p.box !== null) out.push(p.box);
  for (const n of d.notes) if (n.box !== null) out.push(n.box);
  for (const m of d.messages) {
    if (m.anchor === null || m.lines.length === 0) continue;
    const chars = Math.max(...m.lines.map((l) => l.length));
    const w = chars * EM_PER_CHAR * d.fontSize;
    const h = m.lines.length * d.fontSize * 1.2;
    out.push({ x: m.anchor.x - (m.centred ? w / 2 : 0), y: m.anchor.y - d.fontSize, w, h });
  }
  return out;
};
