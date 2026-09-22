import { describe, expect, it } from "vitest";
import { parseXml, textContent, findAll, hasClass } from "../src/svg/xml.ts";

describe("parseXml", () => {
  it("builds a tree with attributes and decoded text", () => {
    const root = parseXml(
      `<?xml version="1.0"?><!-- c --><svg viewBox="0 0 10 10"><g class="a b" id='x'><text>A &amp; B &#65;&#x42;</text></g></svg>`,
    );
    expect(root.name).toBe("svg");
    expect(root.attrs["viewBox"]).toBe("0 0 10 10");
    const g = findAll(root, (e) => e.name === "g")[0]!;
    expect(g.attrs["id"]).toBe("x");
    expect(hasClass(g, "b")).toBe(true);
    expect(hasClass(g, "c")).toBe(false);
    expect(textContent(g)).toBe("A & B AB");
  });

  it("keeps style content raw even when it contains '>'", () => {
    const root = parseXml(`<svg><style>#a > .b { fill: red }</style><rect width="1"/></svg>`);
    expect(textContent(findAll(root, (e) => e.name === "style")[0]!)).toBe("#a > .b { fill: red }");
    expect(findAll(root, (e) => e.name === "rect")).toHaveLength(1);
  });

  it("tolerates unclosed and mismatched tags", () => {
    const root = parseXml(`<svg><g><rect width="2"><g></svg>`);
    expect(findAll(root, (e) => e.name === "rect")).toHaveLength(1);
    const broken = parseXml(`<svg><g></p></g><text>ok</text>`);
    expect(textContent(broken)).toBe("ok");
  });

  it("allows '>' inside quoted attribute values", () => {
    const root = parseXml(`<svg><g data-x="a>b" class='c'><rect/></g></svg>`);
    const g = findAll(root, (e) => e.name === "g")[0]!;
    expect(g.attrs["data-x"]).toBe("a>b");
    expect(g.attrs["class"]).toBe("c");
    expect(findAll(g, (e) => e.name === "rect")).toHaveLength(1);
  });

  it("handles CDATA, valueless attributes and garbage", () => {
    const root = parseXml(`<svg hidden><text><![CDATA[a<b]]></text></svg>`);
    expect(root.attrs["hidden"]).toBe("");
    expect(textContent(root)).toBe("a<b");
    expect(parseXml("not xml at all").children).toEqual(["not xml at all"]);
    expect(parseXml("").children).toEqual([]);
    expect(textContent(parseXml(`<svg><text>a < b's</text><text>c</text></svg>`))).toBe("a < b'sc");
  });
});
