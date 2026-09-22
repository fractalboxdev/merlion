// Pure sequence logic behind @fractalboxdev/merlion-view/interact (specs/sequence.md#interaction), no DOM.
import { test } from "node:test";
import assert from "node:assert/strict";
import { messageLines, lineOf, messageTargets, barOwners } from "../interact-seq.js";

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
