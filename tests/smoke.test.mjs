// The wasm boundary, against the built module: `node --test tests/`.
// PLAYGROUND_WASM overrides where the module is read from.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { evaluator } from "../web/cel.js";

const wasm =
  process.env.PLAYGROUND_WASM ??
  new URL("../target/wasm32-unknown-unknown/release/cel_playground.wasm", import.meta.url);
const cel = await evaluator(await WebAssembly.compile(await readFile(wasm)));

test("reports the cel version", () => {
  assert.match(cel.version, /^\d+\.\d+\.\d+/);
});

test("reports the commit it was built from", () => {
  assert.match(cel.commit, /^([0-9a-f]{40}|unknown)$/);
});

test("evaluates with the input's keys as variables", async () => {
  assert.deepEqual(await cel.evaluate("n + 1", '{"n": 1}'), { ok: true, value: "2" });
});

test("passes text through as UTF-8", async () => {
  assert.deepEqual(await cel.evaluate("s + ' 👋'", '{"s": "héllo"}'), {
    ok: true,
    value: '"héllo 👋"',
  });
});

test("reports errors by stage", async () => {
  assert.equal((await cel.evaluate("1 +", "{}")).stage, "parse");
  assert.equal((await cel.evaluate("1", "[]")).stage, "input");
  assert.equal((await cel.evaluate("missing", "{}")).stage, "evaluation");
});

test("every example evaluates", async () => {
  const examples = JSON.parse(await readFile(new URL("../web/examples.json", import.meta.url)));
  for (const { name, expression, input } of examples) {
    const outcome = await cel.evaluate(expression, JSON.stringify(input, null, 2));
    assert.equal(outcome.ok, true, `${name}: ${JSON.stringify(outcome)}`);
  }
});

test("deep nesting is a parse error, not a stack overflow", async () => {
  const deep = "[".repeat(200) + "]".repeat(200);
  const outcome = await cel.evaluate(deep, "");
  assert.equal(outcome.stage, "parse");
  assert.match(outcome.error, /Recursion limit/);
});

test("reports a panic, and recovers from it", async () => {
  const outcome = await cel.panicForTests();
  assert.equal(outcome.ok, false);
  assert.equal(outcome.stage, "panic");
  assert.match(outcome.error, /panic_for_tests was called/);
  assert.deepEqual(await cel.evaluate("1 + 1", ""), { ok: true, value: "2" });
});
