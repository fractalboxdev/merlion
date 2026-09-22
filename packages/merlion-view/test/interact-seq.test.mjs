// Pure sequence logic behind @fractalbox/merlion-view/interact (specs/sequence.md#interaction), no DOM.
import { test } from "node:test";
import assert from "node:assert/strict";
import { messageLines, lineOf, messageTargets, barOwners, spanSet } from "../interact-seq.js";

// The outline of crates/merlion-render/tests/fixtures/sequence/checkout-order.mmd.
const desc = [
  "Sequence diagram. 4 participants, 6 messages.",
  "Participants: Customer, Web app (Web), API gateway (API), Bank.",
  "1. Customer → Web app: Place order",
  "2. Web app → API gateway: POST /orders",
  "loop Every minute:",
  "  3. API gateway → Bank: Authorise payment",
  "  4. Bank --> API gateway: Approved",
  "alt bank declines:",
  "  5. Bank --> Web app: Declined",
  "else bank approves:",
  "  6. Web app --> Customer: Order confirmed",
  "Note over Customer,Bank: One order, one transaction",
];

test("message lines: the numbered outline lines in source order, indented ones included", () => {
  const L = messageLines(desc, 6);
  assert.deepEqual(L, [
    "1. Customer → Web app: Place order",
    "2. Web app → API gateway: POST /orders",
    "3. API gateway → Bank: Authorise payment",
    "4. Bank --> API gateway: Approved",
    "5. Bank --> Web app: Declined",
    "6. Web app --> Customer: Order confirmed",
  ]);
});

test("message lines: an autonumber with decimals still numbers a line", () => {
  const L = messageLines(["Sequence diagram. 2 participants, 2 messages.", "1.05. A → B: one", "1.15. B → A: two"], 2);
  assert.deepEqual(L, ["1.05. A → B: one", "1.15. B → A: two"]);
});

test("message lines: a count the outline does not match yields none", () => {
  // accDescr replaces the outline, so no line is a message's.
  assert.deepEqual(messageLines(["A hand-written description."], 6), []);
  // A fragment heading or a note is never a message line, so a short outline stays a mismatch.
  assert.deepEqual(messageLines(desc, 7), []);
});

test("each message takes the line its data-merlion-index numbers; an outline with none gives none", () => {
  const L = messageLines(desc, 6);
  assert.deepEqual(lineOf(L, [3, 0]), ["4. Bank --> API gateway: Approved", "1. Customer → Web app: Place order"]);
  assert.deepEqual(lineOf([], [0, 1]), [undefined, undefined]);
});

test("the messages join the walk in the order the outline numbers them, not in draw order", () => {
  assert.deepEqual(messageTargets([2, 0, 1]), [{ e: 1 }, { e: 2 }, { e: 0 }]);
  assert.deepEqual(messageTargets([]), []);
});

test("an activation bar joins the elements its participant lights, and none when it names no participant", () => {
  const gateway = { els: ["g"] };
  const byId = new Map([["Gateway", gateway]]);
  assert.deepEqual(barOwners(["Gateway", "Ghost"], byId), [gateway.els, undefined]);
});

// crates/merlion-render/tests/fixtures/sequence/api-retry-backoff.mmd, in draw order.
const retry = [
  { from: "Client", to: "Gateway" },
  { from: "Gateway", to: "Upstream" },
  { from: "Upstream", to: "Gateway" },
  { from: "Upstream", to: "Gateway" },
  { from: "Gateway", to: "Gateway" },
  { from: "Gateway", to: "Client" },
  { from: "Gateway", to: "Client" },
];
const retryNodes = ["Client", "Gateway", "Upstream"].map((id) => ({ id }));

test("a fragment lights the messages in the rows it spans and the participants they name", () => {
  // `loop`, data-merlion-span="1 4": every message but the first and the last two.
  assert.deepEqual(spanSet(false, 1, 4, retryNodes, retry, [0, 1, 2, 3, 4, 5, 6]), {
    nodes: ["Gateway", "Upstream"],
    edges: [1, 2, 3, 4],
  });
  // `break`, one row, one message, and a self-message names one participant twice.
  assert.deepEqual(spanSet(false, 5, 5, retryNodes, retry, [0, 1, 2, 3, 4, 5, 6]), { nodes: ["Gateway", "Client"], edges: [5] });
  assert.deepEqual(spanSet(false, 4, 4, retryNodes, retry, [0, 1, 2, 3, 4, 5, 6]), { nodes: ["Gateway"], edges: [4] });
});

test("a fragment reads the span as message indices, never as edge positions", () => {
  // Draw order and source order differ: the edge at position 0 is message 4.
  assert.deepEqual(spanSet(false, 0, 0, retryNodes, retry, [4, 0, 1, 2, 3, 5, 6]), { nodes: ["Gateway", "Upstream"], edges: [1] });
});

// crates/merlion-render/tests/fixtures/sequence/notes-and-boxes.mmd.
const ride = [
  { from: "Rider", to: "App" },
  { from: "App", to: "Dispatch" },
  { from: "Dispatch", to: "Driver" },
  { from: "Driver", to: "Dispatch" },
  { from: "Dispatch", to: "App" },
];
const rideNodes = ["Rider", "App", "Dispatch", "Driver"].map((id) => ({ id }));

test("a box lights the columns it spans and only the messages with both ends among them", () => {
  assert.deepEqual(spanSet(true, 0, 1, rideNodes, ride, [0, 1, 2, 3, 4]), { nodes: ["Rider", "App"], edges: [0] });
  assert.deepEqual(spanSet(true, 2, 3, rideNodes, ride, [0, 1, 2, 3, 4]), { nodes: ["Dispatch", "Driver"], edges: [2, 3] });
  // A box of one column: the participant, and any message it sends to itself.
  assert.deepEqual(spanSet(true, 3, 3, rideNodes, ride, [0, 1, 2, 3, 4]), { nodes: ["Driver"], edges: [] });
});
