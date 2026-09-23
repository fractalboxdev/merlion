// Pure state logic behind @fractalboxdev/merlion-view/interact (specs/state.md#interaction), no DOM.
import { test } from "node:test";
import assert from "node:assert/strict";
import { assign, declOrder, display, held, noteOwner, walkOf } from "../interact-state.js";

// The outline of crates/merlion-render/tests/fixtures/state/keyboard-concurrency.mmd.
const keyboard = [
  "State diagram, top to bottom. 12 states, 11 transitions.",
  "start → Active",
  "Active → end",
  "Active: start → NumLockOff",
  "Active: NumLockOff → NumLockOn [EvNumLockPressed]",
  "Active: NumLockOn → NumLockOff [EvNumLockPressed]",
  "Active: start → CapsLockOff",
  "Active: CapsLockOff → CapsLockOn [EvCapsLockPressed]",
  "Active: CapsLockOn → CapsLockOff [EvCapsLockPressed]",
  "Active: start → ScrollLockOff",
  "Active: ScrollLockOff → ScrollLockOn [EvScrollLockPressed]",
  "Active: ScrollLockOn → ScrollLockOff [EvScrollLockPressed]",
];

test("declaration order comes from the node ids the core numbers, not from draw order", () => {
  // keyboard-concurrency draws the regions first and the root's start and end last.
  const drawn = ["m1-n1", "m1-n2", "m1-n3", "m1-n4", "m1-n0", "m1-n10"];
  assert.deepEqual(declOrder(drawn), [4, 0, 1, 2, 3, 5]);
});

test("declaration order: a node group without the core's id keeps document order, after the numbered ones", () => {
  assert.deepEqual(declOrder(["m1-n2", "", "m1-n0"]), [2, 0, 1]);
});

test("a state reads as the outline names it: start, end, a kind in parentheses, else its label", () => {
  assert.equal(display("root_start", "start", ""), "start");
  assert.equal(display("Active_r0_end", "end", ""), "end");
  assert.equal(display("if_state", "choice", ""), "if_state (choice)");
  assert.equal(display("fork_state", "fork", ""), "fork_state (fork)");
  assert.equal(display("join_state", "join", ""), "join_state (join)");
  assert.equal(display("Draft", "simple", "Waiting for the author"), "Waiting for the author");
  // A state described by nothing but its id labels with the id, and so reads as the id.
  assert.equal(display("Idle", "simple", ""), "Idle");
  assert.equal(display("First", "composite", "First"), "First");
});

test("the walk puts each composite before the first state it holds, as the outline heads its members", () => {
  const seq = [
    { id: "root_start", cl: [] },
    { id: "Active_r0_start", cl: ["Active"] },
    { id: "NumLockOff", cl: ["Active"] },
    { id: "Deep", cl: ["Active", "Inner"] },
    { id: "root_end", cl: [] },
  ];
  assert.deepEqual(walkOf(seq), [
    { n: "root_start" },
    { c: "Active" },
    { n: "Active_r0_start" },
    { n: "NumLockOff" },
    { c: "Inner" },
    { n: "Deep" },
    { n: "root_end" },
  ]);
});

test("the walk names a cluster the SVG leaves unnamed no target", () => {
  assert.deepEqual(walkOf([{ id: "A", cl: [undefined] }]), [{ n: "A" }]);
});

test("each target takes the next outline line that names it, so three regions read as three starts", () => {
  const keys = [
    "start",
    "Active",
    "Active: start",
    "Active: NumLockOff",
    "Active: NumLockOn",
    "Active: start",
    "Active: CapsLockOff",
    "Active: CapsLockOn",
    "Active: start",
    "Active: ScrollLockOff",
    "Active: ScrollLockOn",
    "end",
  ];
  assert.deepEqual(assign(keys, keyboard), [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, -1]);
});

test("a name that prefixes a longer one takes only its own line", () => {
  const desc = ["State diagram, top to bottom. 2 states, 1 transition.", "Lock screen → Lock", "Lock"];
  assert.deepEqual(assign(["Lock", "Lock screen"], desc), [2, -1]);
  assert.deepEqual(assign(["Lock screen", "Lock"], desc), [1, 2]);
});

test("a state the outline gives no line keeps the scan where it was", () => {
  // `root_end` prints no line; the states declared after it still find theirs.
  const desc = ["State diagram, top to bottom. 3 states, 2 transitions.", "start → A", "A → end"];
  assert.deepEqual(assign(["start", "end", "A"], desc), [1, -1, 2]);
});

// crates/merlion-render/tests/fixtures/state/keyboard-concurrency.mmd, in transition order.
const transitions = [
  { from: "root_start", to: "Active_r0_start" },
  { from: "Active_r0_start", to: "NumLockOff" },
  { from: "NumLockOff", to: "NumLockOn" },
  { from: "NumLockOn", to: "NumLockOff" },
  { from: "Active_r1_start", to: "CapsLockOff" },
  { from: "Active_r0_start", to: "root_end" },
];

test("a note on a composite travels with the member it is drawn beside, the first one declared", () => {
  const cruising = { id: "Cruising", cl: ["Moving"] };
  const seq = [
    { id: "Idle", cl: [] },
    cruising,
    { id: "Level", cl: ["Moving"] },
  ];
  // A note on a simple state still joins that state's own node.
  assert.equal(noteOwner("Idle", seq), seq[0]);
  assert.equal(noteOwner("Moving", seq), cruising);
  // A note naming neither a state nor a composite joins nothing rather than throwing.
  assert.equal(noteOwner("Ghost", seq), undefined);
});

test("a composite lights what it holds: its states and the transitions between them", () => {
  assert.deepEqual(held(["Active_r0_start", "NumLockOff", "NumLockOn"], transitions), {
    nodes: ["Active_r0_start", "NumLockOff", "NumLockOn"],
    edges: [1, 2, 3],
  });
  // A transition crossing the boundary in either direction stays out of the set.
  assert.deepEqual(held(["NumLockOff"], transitions), { nodes: ["NumLockOff"], edges: [] });
  assert.deepEqual(held([], transitions), { nodes: [], edges: [] });
});
