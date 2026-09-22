import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { COMPAT_SEQUENCE_DIR } from "../src/paths.ts";
import { extractSequence } from "../src/svg/sequence.ts";

const merlion = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 300 200" class="merlion merlion-sequence">
<g class="merlion-diagram" font-size="14">
<g class="merlion-cluster merlion-fragment" data-merlion-kind="alt" data-merlion-index="0"><rect class="merlion-cluster-box" x="8" y="60" width="284" height="80"/><text class="merlion-cluster-title"><tspan x="12" y="72">alt</tspan></text><text class="merlion-fragment-label"><tspan x="40" y="72">is sick</tspan></text></g>
<g class="merlion-node merlion-participant" data-merlion-id="Alice"><line class="merlion-lifeline" x1="50" y1="40" x2="50" y2="160"/><path class="merlion-shape" d="M10 8H90V40H10Z"/><text class="merlion-label"><tspan x="30" y="29">Alice</tspan></text></g>
<g class="merlion-node merlion-participant" data-merlion-id="Bob"><line class="merlion-lifeline" x1="220" y1="40" x2="220" y2="160"/><path class="merlion-shape" d="M180 8H260V40H180Z"/><text class="merlion-label"><tspan x="200" y="29">Bob</tspan></text></g>
<g class="merlion-note" data-merlion-from="Alice" data-merlion-to="Alice" data-merlion-placement="over"><rect class="merlion-note-box" x="20" y="45" width="60" height="20"/><text class="merlion-label"><tspan x="24" y="59">a note</tspan></text></g>
<g class="merlion-edge merlion-message" data-merlion-from="Alice" data-merlion-to="Bob" data-merlion-index="0"><path class="merlion-edge-path" d="M50 100L220 100"/><g class="merlion-edge-label"><rect class="merlion-edge-label-bg" x="110" y="80" width="50" height="20"/><text class="merlion-edge-text"><tspan x="114" y="94">Hello</tspan></text></g></g>
<g class="merlion-edge merlion-message" data-merlion-from="Bob" data-merlion-to="Alice" data-merlion-index="1"><path class="merlion-edge-path" d="M220 130L50 130"/></g>
</g></svg>`;

const mermaid = `<svg id="d0" width="100%" xmlns="http://www.w3.org/2000/svg" viewBox="-50 -10 850 697" aria-roledescription="sequence">
<rect x="0" y="0" width="150" height="74" class="actor actor-top" name="Alice"/>
<rect x="200" y="0" width="150" height="74" class="actor actor-top" name="Bob"/>
<text x="75" y="37" class="actor actor-box" style="font-size: 16px;"><tspan x="75">Alice</tspan></text>
<text x="275" y="37" class="actor actor-box" style="font-size: 16px;"><tspan x="275">Bob</tspan></text>
<text x="75" y="609" class="actor actor-box" style="font-size: 16px;"><tspan x="75">Alice</tspan></text>
<text x="275" y="609" class="actor actor-box" style="font-size: 16px;"><tspan x="275">Bob</tspan></text>
<polygon points="64,100 114,100 114,128 105,135 64,135" class="labelBox"/>
<text x="89" y="120" class="labelText" style="font-size: 16px;">alt</text>
<text x="200" y="118" class="loopText" style="font-size: 16px;"><tspan x="200">[is sick]</tspan></text>
<rect x="20" y="80" width="60" height="20" class="note"/>
<text x="50" y="85" class="noteText" style="font-size: 16px;"><tspan x="50">a note</tspan></text>
<line x1="75" y1="160" x2="275" y2="160" class="messageLine0"/>
<text x="175" y="145" class="messageText" style="font-size: 16px;">Hello</text>
<line x1="275" y1="200" x2="75" y2="200" class="messageLine1"/>
</svg>`;

describe("extractSequence", () => {
  it("reads a Merlion drawing through its data attributes", () => {
    const d = extractSequence(merlion);
    expect(d.flavor).toBe("merlion");
    expect(d.viewBox).toEqual({ x: 0, y: 0, w: 300, h: 200 });
    expect(d.participants.map((p) => p.label)).toEqual(["Alice", "Bob"]);
    expect(d.participants[0]!.box).toEqual({ x: 10, y: 8, w: 80, h: 32 });
    expect(d.messages.map((m) => m.label)).toEqual(["Hello", ""]);
    expect(d.notes.map((n) => n.text)).toEqual(["a note"]);
    expect(d.fragments).toEqual([{ kind: "alt", label: "is sick" }]);
  });

  it("reads a mermaid drawing through its class names", () => {
    const d = extractSequence(mermaid);
    expect(d.flavor).toBe("mermaid");
    expect(d.viewBox).toEqual({ x: -50, y: -10, w: 850, h: 697 });
    // The top and bottom copies of one actor are one participant.
    expect(d.participants.map((p) => p.label)).toEqual(["Alice", "Bob"]);
    expect(d.participants[0]!.box).toEqual({ x: 0, y: 0, w: 150, h: 74 });
    // Two message lines, one of them unlabelled.
    expect(d.messages.length).toBe(2);
    expect(d.messages.map((m) => m.label).filter((l) => l !== "")).toEqual(["Hello"]);
    expect(d.notes.map((n) => n.text)).toEqual(["a note"]);
    // mermaid brackets a fragment's header label.
    expect(d.fragments).toEqual([{ kind: "alt", label: "is sick" }]);
  });

  it("reads a mermaid note drawn over several lines as one note", () => {
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 200" aria-roledescription="sequence">
<rect x="20" y="80" width="200" height="40" class="note"/>
<text x="120" y="85" class="noteText" style="font-size: 16px;"><tspan x="120">The pickup pin is</tspan></text>
<text x="120" y="102" class="noteText" style="font-size: 16px;"><tspan x="120">dragged before sending</tspan></text>
<rect x="250" y="80" width="120" height="20" class="note"/>
<text x="310" y="85" class="noteText" style="font-size: 16px;"><tspan x="310">held for 15s</tspan></text>
</svg>`;
    const d = extractSequence(svg);
    expect(d.notes.map((n) => n.text)).toEqual(["The pickup pin is dragged before sending", "held for 15s"]);
  });

  it("joins the lines mermaid wraps one message label over", () => {
    // mermaid draws one `messageText` per wrapped line, above the line it labels.
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 300" aria-roledescription="sequence">
<text x="175" y="89" class="messageText" style="font-size: 16px;"><tspan x="175">Hello John,</tspan></text>
<text x="175" y="106" class="messageText" style="font-size: 16px;"><tspan x="175">how are you?</tspan></text>
<line x1="75" y1="138" x2="275" y2="138" class="messageLine0"/>
<text x="175" y="153" class="messageText" style="font-size: 16px;"><tspan x="175">Fine</tspan></text>
<line x1="275" y1="202" x2="75" y2="202" class="messageLine1"/>
</svg>`;
    const d = extractSequence(svg);
    expect(d.messages.map((m) => m.label)).toEqual(["Hello John, how are you?", "Fine"]);
    expect(d.messages[0]!.lines).toEqual(["Hello John,", "how are you?"]);
  });

  it("leaves a message mermaid draws unlabelled empty", () => {
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 300" aria-roledescription="sequence">
<line x1="75" y1="100" x2="275" y2="100" class="messageLine0"/>
<text x="175" y="145" class="messageText" style="font-size: 16px;"><tspan x="175">second</tspan></text>
<line x1="275" y1="180" x2="75" y2="180" class="messageLine1"/>
</svg>`;
    const d = extractSequence(svg);
    expect(d.messages.map((m) => m.label)).toEqual(["", "second"]);
  });

  it("joins the lines mermaid draws for one multi-line participant label", () => {
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 700 200" aria-roledescription="sequence">
<rect x="550" y="0" width="150" height="74" class="actor actor-top" name="John"/>
<text x="619" y="46" class="actor actor-box" style="font-size: 16px;"><tspan x="619">John</tspan></text>
<text x="619" y="46" class="actor actor-box" style="font-size: 16px;"><tspan x="619">Second Line</tspan></text>
<text x="619" y="169" class="actor actor-box" style="font-size: 16px;"><tspan x="619">John</tspan></text>
<text x="619" y="169" class="actor actor-box" style="font-size: 16px;"><tspan x="619">Second Line</tspan></text>
<text x="75" y="46" class="actor actor-box" style="font-size: 16px;"><tspan x="75">Alice</tspan></text>
</svg>`;
    const d = extractSequence(svg);
    expect(d.participants.map((p) => p.label)).toEqual(["Alice", "John Second Line"]);
  });

  it("ignores mermaid's hidden actor popup menus", () => {
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 200" aria-roledescription="sequence">
<rect x="0" y="0" width="150" height="74" class="actor actor-top" name="Runner"/>
<text x="75" y="37" class="actor actor-box" style="font-size: 16px;"><tspan x="75">Runner</tspan></text>
<g id="a_popup" class="actorPopupMenu" display="none">
<rect class="actorPopupMenuPanel actor actor-bottom" x="0" y="74" width="150" height="50"/>
<text x="10" y="104" class="actor" style="font-size: 16px;"><tspan x="10">Logs</tspan></text>
</g>
</svg>`;
    const d = extractSequence(svg);
    expect(d.participants.map((p) => p.label)).toEqual(["Runner"]);
    expect(d.participants[0]!.box).toEqual({ x: 0, y: 0, w: 150, h: 74 });
  });

  it("reports an unknown flavor rather than guessing", () => {
    const d = extractSequence(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"></svg>`);
    expect(d.flavor).toBe("unknown");
    expect(d.participants).toEqual([]);
  });
});

describe("the sequence corpus", () => {
  it("is pinned by a manifest at the same commit as compat", () => {
    const m = JSON.parse(readFileSync(join(COMPAT_SEQUENCE_DIR, "manifest.json"), "utf8")) as {
      corpus: string;
      commit: string;
      diagrams: Array<{ name: string; source: string; sha256: string }>;
    };
    expect(m.corpus).toBe("compat-sequence");
    expect(m.commit).toBe("98a0945418c76238f15df2afaddbba4272656c3b");
    expect(m.diagrams.length).toBeGreaterThan(100);
    for (const d of m.diagrams) expect(d.sha256).toMatch(/^[0-9a-f]{64}$/);
  });
});
