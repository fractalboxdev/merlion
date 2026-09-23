/**
 * A small, tolerant XML tokenizer for SVG output.
 *
 * The benchmark reads SVG from several renderers, some of which emit HTML-ish
 * markup, so the parser never throws: unknown constructs become text,
 * mismatched end tags close back to the nearest matching open element (or are
 * ignored), and unclosed elements close at end of input.
 */

export interface XmlElement {
  readonly name: string;
  readonly attrs: Readonly<Record<string, string>>;
  readonly children: Array<XmlElement | string>;
}

const RAW_TEXT = new Set(["style", "script"]);

const NAMED_ENTITIES: Readonly<Record<string, string>> = {
  amp: "&",
  lt: "<",
  gt: ">",
  quot: '"',
  apos: "'",
  nbsp: " ",
};

/** Decodes the five XML entities, `&nbsp;` and numeric character references. */
export const decodeEntities = (s: string): string =>
  s.replace(/&(#x[0-9a-fA-F]+|#[0-9]+|[a-zA-Z]+);/g, (whole, body: string) => {
    if (body.startsWith("#x") || body.startsWith("#X")) {
      const cp = Number.parseInt(body.slice(2), 16);
      return cp > 0 && cp <= 0x10ffff ? String.fromCodePoint(cp) : whole;
    }
    if (body.startsWith("#")) {
      const cp = Number.parseInt(body.slice(1), 10);
      return cp > 0 && cp <= 0x10ffff ? String.fromCodePoint(cp) : whole;
    }
    return NAMED_ENTITIES[body] ?? whole;
  });

const ATTR_RE = /([^\s=/>]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+)))?/g;

const parseAttrs = (s: string): Record<string, string> => {
  const attrs: Record<string, string> = {};
  for (const m of s.matchAll(ATTR_RE)) {
    const name = m[1];
    if (name === undefined) continue;
    attrs[name] = decodeEntities(m[2] ?? m[3] ?? m[4] ?? "");
  }
  return attrs;
};

/**
 * Where a start or end tag ends, skipping `>` inside quoted values.
 *
 * A tag that reaches an unquoted `<` before its `>` was never closed. Browsers
 * recover by ending it there and reading the `<` as the next tag, and so does
 * this: a model that writes `</defs` instead of `</defs>` still produces a
 * drawing every renderer shows, so the benchmark reads it the way a renderer
 * does rather than losing the rest of the document inside the unclosed element.
 */
const findTagEnd = (src: string, from: number): { end: number; terminated: boolean } => {
  let quote: string | null = null;
  for (let k = from; k < src.length; k++) {
    const ch = src[k];
    if (quote !== null) {
      if (ch === quote) quote = null;
    } else if (ch === '"' || ch === "'") {
      quote = ch;
    } else if (ch === ">") {
      return { end: k, terminated: true };
    } else if (ch === "<") {
      return { end: k, terminated: false };
    }
  }
  return { end: src.length, terminated: false };
};

/**
 * Parses `src` into an element tree. Returns the first top-level element when
 * there is one, else a synthetic `#document` element holding whatever was found.
 */
export const parseXml = (src: string): XmlElement => {
  const doc: XmlElement = { name: "#document", attrs: {}, children: [] };
  const stack: XmlElement[] = [doc];
  const top = (): XmlElement => stack[stack.length - 1] ?? doc;
  const pushText = (t: string): void => {
    if (t.length > 0) top().children.push(t);
  };

  let i = 0;
  const n = src.length;
  while (i < n) {
    const lt = src.indexOf("<", i);
    if (lt < 0) {
      pushText(decodeEntities(src.slice(i)));
      break;
    }
    if (lt > i) pushText(decodeEntities(src.slice(i, lt)));

    if (src.startsWith("<!--", lt)) {
      const end = src.indexOf("-->", lt + 4);
      i = end < 0 ? n : end + 3;
      continue;
    }
    if (src.startsWith("<![CDATA[", lt)) {
      const end = src.indexOf("]]>", lt + 9);
      pushText(src.slice(lt + 9, end < 0 ? n : end));
      i = end < 0 ? n : end + 3;
      continue;
    }
    if (src.startsWith("<?", lt) || src.startsWith("<!", lt)) {
      const end = src.indexOf(">", lt + 2);
      i = end < 0 ? n : end + 1;
      continue;
    }

    if (!/[A-Za-z_:/]/.test(src[lt + 1] ?? "")) {
      // "<" not followed by a name: literal text (e.g. "a < b's").
      pushText("<");
      i = lt + 1;
      continue;
    }
    // The character after "<" is a name start, checked above, so the tag has at
    // least one character of content and `gt` is always past it.
    const { end: gt, terminated } = findTagEnd(src, lt + 1);
    const inner = src.slice(lt + 1, gt);
    /** An unterminated tag ends at the `<` that follows it, which stays unread. */
    const next = terminated ? gt + 1 : gt;

    if (inner.startsWith("/")) {
      const name = inner.slice(1).trim();
      // Close back to the nearest open element with this name; ignore stray end tags.
      for (let k = stack.length - 1; k > 0; k--) {
        if (stack[k]?.name === name) {
          stack.length = k;
          break;
        }
      }
      i = next;
      continue;
    }

    const nameMatch = /^([^\s/>]+)/.exec(inner);
    if (nameMatch === null || nameMatch[1] === undefined) {
      // "<" not followed by a name: literal text.
      pushText("<");
      i = lt + 1;
      continue;
    }
    const name = nameMatch[1];
    const selfClosing = inner.endsWith("/");
    const attrSrc = inner.slice(name.length, selfClosing ? -1 : undefined);
    const el: XmlElement = { name, attrs: parseAttrs(attrSrc), children: [] };
    top().children.push(el);
    i = next;
    if (selfClosing) continue;

    if (RAW_TEXT.has(name)) {
      const close = src.indexOf(`</${name}`, i);
      const end = close < 0 ? n : close;
      if (end > i) el.children.push(src.slice(i, end));
      const closeGt = close < 0 ? -1 : src.indexOf(">", close);
      i = closeGt < 0 ? n : closeGt + 1;
      continue;
    }
    stack.push(el);
  }

  const first = doc.children.find((c): c is XmlElement => typeof c !== "string");
  return first ?? doc;
};

/** Concatenated text of an element and its descendants, in document order. */
export const textContent = (el: XmlElement): string => {
  let out = "";
  const walk = (e: XmlElement): void => {
    for (const c of e.children) {
      if (typeof c === "string") out += c;
      else walk(c);
    }
  };
  walk(el);
  return out;
};

export const childElements = (el: XmlElement): XmlElement[] =>
  el.children.filter((c): c is XmlElement => typeof c !== "string");

/** Every descendant (excluding `el`) matching `pred`, in document order. */
export const findAll = (el: XmlElement, pred: (e: XmlElement) => boolean): XmlElement[] => {
  const out: XmlElement[] = [];
  // Iterative depth-first walk: SVG input is untrusted and may nest deeply.
  const stack: XmlElement[] = [...childElements(el)].reverse();
  while (stack.length > 0) {
    const e = stack.pop();
    if (e === undefined) break;
    if (pred(e)) out.push(e);
    const kids = childElements(e);
    for (let k = kids.length - 1; k >= 0; k--) {
      const kid = kids[k];
      if (kid !== undefined) stack.push(kid);
    }
  }
  return out;
};

export const classList = (el: XmlElement): string[] =>
  (el.attrs["class"] ?? "").split(/\s+/).filter((c) => c.length > 0);

export const hasClass = (el: XmlElement, cls: string): boolean => classList(el).includes(cls);
