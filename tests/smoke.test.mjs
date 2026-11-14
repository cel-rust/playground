// The wasm boundary, against each module build.sh built, one per version of
// cel-rust: `./build.sh && node --test tests/`. PLAYGROUND_DIST overrides
// where they're read from, dist/.

import { describe, test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { evaluator } from "../web/cel.js";
import { moduleFile } from "../web/versions.js";

const dist = process.env.PLAYGROUND_DIST
  ? pathToFileURL(`${resolve(process.env.PLAYGROUND_DIST)}/`)
  : new URL("../dist/", import.meta.url);
const versions = JSON.parse(await readFile(new URL("versions.json", dist)));

test("lists each version once, newest first", () => {
  assert.ok(versions.length > 0);
  const numbers = (version) => version.split(".").map((n) => parseInt(n, 10));
  const newer = (a, b) => {
    const [x, y] = [numbers(a), numbers(b)];
    const at = x.findIndex((n, i) => n !== y[i]);
    return at !== -1 && x[at] > y[at];
  };
  for (const [i, version] of versions.entries()) {
    if (i > 0) assert.ok(newer(versions[i - 1], version), `${versions[i - 1]} before ${version}`);
  }
});

/**
 * The examples the page offers that a version can't run, by its line: they
 * need what came after it (see `EXAMPLES_BEYOND` in src/lib.rs).
 */
const EXAMPLES_BEYOND = {
  "0.14": ["Strings extension", "Lists extension", "Math extension", "Base64 encoders", "Timestamps and durations"],
};

for (const version of versions) {
  const cel = await evaluator(await WebAssembly.compile(await readFile(new URL(moduleFile(version), dist))));
  // cel 0.14 doesn't say where the nodes of what it parses are
  const { positioned } = await cel.ast("x");
  describe(`cel-rust ${version}`, () => suite(cel, version, positioned));
}

function suite(cel, version, positioned) {
  test("reports the version it's listed as", () => {
    assert.equal(cel.version, version);
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

  test("highlights, in the string's own indices", async () => {
    assert.deepEqual(await cel.highlight("a + 1", '{"a": 1}'), {
      tokens: [
        [0, 1, "identifier"],
        [2, 3, "operator"],
        [4, 5, "number"],
      ],
      // Without positions, only what places tell: nothing here
      marks: positioned ? [[0, 1, "variable"]] : [],
      errors: [],
    });
    const source = "'👋' + x";
    const { tokens, marks } = await cel.highlight(source, "{}");
    assert.deepEqual(
      tokens.map(([start, end, kind]) => [source.slice(start, end), kind]),
      [
        ["'👋'", "string"],
        ["+", "operator"],
        ["x", "identifier"],
      ],
    );
    assert.deepEqual(marks, positioned ? [[7, 8, "missing"]] : []);
    assert.deepEqual((await cel.highlight("f(a.b)", "")).marks, [
      [0, 1, "function"],
      ...(positioned ? [[2, 3, "missing"]] : []),
      [4, 5, "field"],
    ]);
    assert.deepEqual((await cel.highlight("x +", "")).errors, [[3, 3]]);
  });

  test("highlights the input, in the string's own indices", async () => {
    const source = '{"👋": [1, true], "a": {"b": null}}';
    const { tokens, marks, errors } = await cel.highlightInput(source);
    const text = ([start, end, name]) => [source.slice(start, end), name];
    assert.deepEqual(tokens.slice(0, 6).map(text), [
      ["{", "bracket"],
      ['"👋"', "string"],
      [":", "delimiter"],
      ["[", "bracket"],
      ["1", "number"],
      [",", "delimiter"],
    ]);
    assert.deepEqual(marks.map(text), [
      ['"👋"', "variable"],
      ['"a"', "variable"],
      ['"b"', "field"],
    ]);
    assert.deepEqual(errors, []);
    assert.deepEqual((await cel.highlightInput('{"👋": x}')).errors, [[7, 8]]);
    assert.deepEqual((await cel.highlightInput('{"a": 1')).errors, [[7, 7]]);
  });

  test("parses into a tree, positioned in the string's own indices", async () => {
    const result = await cel.ast("'👋' + x");
    assert.equal(result.ok, true);
    assert.equal(result.positioned, positioned);
    const { root } = result;
    const x = root.children[1];
    if (positioned) {
      assert.deepEqual([root.kind, root.detail, root.text, root.from], ["Call", "_+_", "+", null]);
      assert.deepEqual([x.kind, x.detail, x.start, x.end], ["Ident", "x", 7, 8]);
    } else {
      assert.deepEqual([root.kind, root.detail, root.text, root.from], ["Call", "_+_", null, null]);
      assert.deepEqual([x.kind, x.detail, x.start, x.end], ["Ident", "x", null, null]);
    }
    assert.equal((await cel.ast("1 +")).ok, false);
  });

  test("reports errors by stage", async () => {
    assert.equal((await cel.evaluate("1 +", "{}")).stage, "parse");
    assert.equal((await cel.evaluate("1", "[]")).stage, "input");
    assert.equal((await cel.evaluate("missing", "{}")).stage, "evaluation");
  });

  test("every example evaluates, but those that need a later version", async () => {
    const examples = JSON.parse(await readFile(new URL("../web/examples.json", import.meta.url)));
    const beyond = EXAMPLES_BEYOND[version.split(".").slice(0, 2).join(".")] ?? [];
    for (const { name, expression, input } of examples) {
      const outcome = await cel.evaluate(expression, JSON.stringify(input, null, 2));
      if (beyond.includes(name)) {
        assert.equal(outcome.stage, "evaluation", `${name}: ${JSON.stringify(outcome)}`);
      } else {
        assert.equal(outcome.ok, true, `${name}: ${JSON.stringify(outcome)}`);
      }
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
}
