// Which version of cel-rust runs what a link asks for: `node --test tests/`.

import { test } from "node:test";
import assert from "node:assert/strict";
import { moduleFile, pick } from "../web/versions.js";

const VERSIONS = ["0.16.1", "0.15.0"];

test("the newest, unless asked for another", () => {
  assert.equal(pick(VERSIONS, null), "0.16.1");
  assert.equal(pick(VERSIONS, undefined), "0.16.1");
  assert.equal(pick(VERSIONS, 16), "0.16.1");
});

test("the version asked for, when on offer", () => {
  assert.equal(pick(VERSIONS, "0.15.0"), "0.15.0");
  assert.equal(pick(VERSIONS, "0.16.1"), "0.16.1");
});

test("else the one of its line", () => {
  assert.equal(pick(VERSIONS, "0.16.0"), "0.16.1");
  assert.equal(pick(VERSIONS, "0.15.2"), "0.15.0");
});

test("else the newest", () => {
  assert.equal(pick(VERSIONS, "0.14.0"), "0.16.1");
  assert.equal(pick(VERSIONS, "0.1"), "0.16.1");
  assert.equal(pick(VERSIONS, ""), "0.16.1");
});

test("each version's module is a file of its own", () => {
  assert.equal(moduleFile("0.16.1"), "cel-0.16.1.wasm");
});
