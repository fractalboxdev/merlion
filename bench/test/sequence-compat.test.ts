import { describe, expect, it } from "vitest";
import { sequenceCompat, sequenceMetrics } from "../src/metrics/sequence.ts";
import type { SequenceDrawing } from "../src/svg/sequence.ts";

const drawing = (o: Partial<SequenceDrawing>): SequenceDrawing => ({
  flavor: "merlion",
  viewBox: { x: 0, y: 0, w: 400, h: 300 },
  participants: [],
  messages: [],
  notes: [],
  fragments: [],
  fontSize: 14,
  ...o,
});

const parts = (...labels: string[]) => labels.map((label, k) => ({ label, x: k * 100, box: null }));
const msgs = (...labels: string[]) =>
  labels.map((label) => ({ label, lines: label === "" ? [] : [label], anchor: null, centred: false, labelBox: null }));
const notes = (...texts: string[]) => texts.map((text) => ({ text, box: null }));

describe("sequenceCompat", () => {
  it("passes when the participants, the message count, the message labels and the notes agree", () => {
    const ref = drawing({ participants: parts("Alice", "Bob"), messages: msgs("Hello", ""), notes: notes("a note") });
    const cand = drawing({ participants: parts("Bob", "Alice"), messages: msgs("", "Hello"), notes: notes("a note") });
    const c = sequenceCompat(ref, cand);
    expect(c.pass).toBe(true);
    expect(c.participants).toBe(true);
    expect(c.messageCount).toBe(true);
    expect(c.messageLabels).toBe(true);
    expect(c.notes).toBe(true);
  });

  it("names the participant labels that differ", () => {
    const ref = drawing({ participants: parts("Alice", "Bob", "Carol") });
    const cand = drawing({ participants: parts("Alice", "Bob", "Dave") });
    const c = sequenceCompat(ref, cand);
    expect(c.pass).toBe(false);
    expect(c.missingParticipants).toEqual(["Carol"]);
    expect(c.extraParticipants).toEqual(["Dave"]);
  });

  it("compares labels with whitespace removed, since the two wrap at different points", () => {
    const ref = drawing({ messages: msgs("declined (insufficient funds)") });
    const cand = drawing({ messages: msgs("declined\n(insufficient funds)") });
    expect(sequenceCompat(ref, cand).messageLabels).toBe(true);
  });

  it("reports a message-count difference without claiming the labels match", () => {
    const ref = drawing({ messages: msgs("a", "b") });
    const cand = drawing({ messages: msgs("a") });
    const c = sequenceCompat(ref, cand);
    expect(c.messageCount).toBe(false);
    expect(c.refMessages).toBe(2);
    expect(c.candMessages).toBe(1);
    expect(c.pass).toBe(false);
  });

  it("ignores a rect fragment, which mermaid draws without a kind tab", () => {
    const ref = drawing({ fragments: [{ kind: "par", label: "refund" }] });
    const cand = drawing({ fragments: [{ kind: "rect", label: "" }, { kind: "par", label: "refund" }] });
    expect(sequenceCompat(ref, cand).fragments).toBe(true);
  });

  it("fails on an empty candidate drawing, so a renderer that draws nothing never passes", () => {
    const ref = drawing({ participants: parts("Alice", "Bob"), messages: msgs("Hello") });
    expect(sequenceCompat(ref, drawing({})).pass).toBe(false);
  });
});

describe("sequenceMetrics", () => {
  it("reads the drawn size and counts", () => {
    const d = drawing({
      viewBox: { x: 0, y: 0, w: 700, h: 400 },
      participants: parts("A", "B"),
      messages: msgs("x"),
      notes: notes("n"),
      fragments: [{ kind: "alt", label: "" }],
    });
    const m = sequenceMetrics(d);
    expect(m).toMatchObject({
      participants: 2,
      messages: 1,
      notes: 1,
      fragments: 1,
      width: 700,
      height: 400,
      area: 280000,
      fits720: true,
    });
  });

  it("marks a drawing wider than 720 px as not fitting", () => {
    expect(sequenceMetrics(drawing({ viewBox: { x: 0, y: 0, w: 721, h: 100 } })).fits720).toBe(false);
  });

  it("counts a message label overlapping a participant box", () => {
    const d = drawing({
      participants: [{ label: "A", x: 50, box: { x: 0, y: 0, w: 100, h: 40 } }],
      messages: [{ label: "long", lines: ["long"], anchor: { x: 10, y: 20 }, centred: false, labelBox: null }],
    });
    expect(sequenceMetrics(d).labelOverlaps).toBe(1);
  });
});
