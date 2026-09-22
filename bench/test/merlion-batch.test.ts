import { existsSync } from "node:fs";
import { NodeContext } from "@effect/platform-node";
import { Effect } from "effect";
import { describe, expect, it } from "vitest";
import { MERLION_BIN } from "../src/paths.ts";
import { MerlionLive, parseBatchSummary } from "../src/renderers/merlion.ts";
import { Renderer } from "../src/renderers/Renderer.ts";

const ok = `{"file":"in/x.mmd","ok":true,"micros":264,"fuel_used":47,"svg_bytes":4680,"diagnostics":[{"severity":"repair","code":"R005","line":2,"column":1,"byte_start":13,"byte_end":14,"message":"node \`a\` is never declared","fix":null}],"error":null}`;
const bad = `{"file":"in/y.mmd","ok":false,"micros":25,"fuel_used":0,"svg_bytes":0,"diagnostics":[{"severity":"error","code":"E002","line":2,"column":5,"byte_start":17,"byte_end":17,"message":"expected a node id, found end of line","fix":null}],"error":{"kind":"parse"}}`;

describe("parseBatchSummary", () => {
  it("keys each line by file stem with in-process milliseconds and fuel", () => {
    const m = parseBatchSummary(`${ok}\n${bad}\n`);
    expect(m.get("x")).toEqual({ ok: true, ms: 0.264, fuelUsed: 47, error: null });
    expect(m.get("y")).toEqual({ ok: false, ms: 0.025, fuelUsed: 0, error: "parse: E002 expected a node id, found end of line" });
  });

  it("names the error kind when no error diagnostic is present", () => {
    const line = `{"file":"in/z.mmd","ok":false,"micros":3,"fuel_used":0,"svg_bytes":0,"diagnostics":[],"error":{"kind":"unsupported_diagram","header":"pie"}}`;
    expect(parseBatchSummary(line).get("z")?.error).toBe("unsupported_diagram: pie");
  });

  it("skips lines that are not summary JSON", () => {
    expect(parseBatchSummary("garbage\n\n").size).toBe(0);
  });
});

describe("MerlionLive", () => {
  it.skipIf(!existsSync(MERLION_BIN))("renders with a hint and in a batch", async () => {
    const program = Effect.gen(function* () {
      const r = yield* Renderer;
      const before = yield* r.render("flowchart LR\n  a --> b\n");
      const after = yield* r.render("flowchart LR\n  a --> b\n  b --> c\n", { hint: before.svg ?? "" });
      const batch = yield* r.renderBatch!([{ name: "one", source: "flowchart TB\n  x --> y\n" }]);
      return { before, after, batch };
    }).pipe(Effect.provide(MerlionLive), Effect.provide(NodeContext.layer));
    const { before, after, batch } = await Effect.runPromise(program);
    expect(before.error).toBeNull();
    expect(after.error).toBeNull();
    expect(after.svg).toContain("merlion-node");
    expect(batch.get("one")?.svg).toContain("merlion-node");
  });
});
