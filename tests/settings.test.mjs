// The settings as localStorage keeps them, and as the page applies them:
// `node --test tests/`.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { load, save, SETTINGS, STORAGE_KEY } from "../web/settings.js";

/** localStorage's methods, in memory. */
class Memory {
  #items = new Map();
  getItem(key) {
    return this.#items.get(key) ?? null;
  }
  setItem(key, value) {
    this.#items.set(key, String(value));
  }
}

const defaults = () => load(new Memory());
const web = (file) => readFile(new URL(`../web/${file}`, import.meta.url), "utf8");

test("each setting defaults to its first value", () => {
  assert.deepEqual(defaults(), { theme: "system", codeTheme: "github" });
});

test("keeps what it saves", () => {
  const storage = new Memory();
  assert.equal(save(storage, { theme: "dark", codeTheme: "dracula" }), true);
  assert.deepEqual(load(storage), { theme: "dark", codeTheme: "dracula" });
});

test("ignores what it can't take", () => {
  for (const stored of ["{", "null", "1", '"dark"', "[]", '{"theme": "blue"}', '{"theme": 1}']) {
    const storage = new Memory();
    storage.setItem(STORAGE_KEY, stored);
    assert.deepEqual(load(storage), defaults(), stored);
  }
  const storage = new Memory();
  storage.setItem(STORAGE_KEY, '{"theme": "light", "codeTheme": "monokai", "gone": true}');
  assert.deepEqual(load(storage), { theme: "light", codeTheme: "github" });
});

test("does without storage", () => {
  const blocked = {
    getItem() {
      throw new Error("blocked");
    },
    setItem() {
      throw new Error("blocked");
    },
  };
  assert.deepEqual(load(blocked), defaults());
  assert.equal(save(blocked, defaults()), false);
});

test("index.html applies the same settings before the page draws", async () => {
  const html = await web("index.html");
  assert.ok(html.includes(`localStorage.getItem("${STORAGE_KEY}")`));
  const names = html.match(/for \(const name of (\[[^\]]*\])\)/);
  assert.ok(names, "the loop over the settings' names");
  assert.deepEqual(JSON.parse(names[1]), Object.keys(SETTINGS));
});

test("the menu offers, and the stylesheet styles, each value", async () => {
  const [html, css] = await Promise.all([web("index.html"), web("style.css")]);
  for (const [name, { values }] of Object.entries(SETTINGS)) {
    for (const value of values) {
      assert.ok(html.includes(`name="${name}" value="${value}"`), `${name}: ${value} in the menu`);
    }
  }
  for (const value of SETTINGS.codeTheme.values) {
    assert.ok(css.includes(`[data-code-theme="${value}"] {`), `the ${value} code theme`);
  }
  assert.ok(css.includes(':root:not([data-theme="light"])'));
  assert.ok(css.includes(':root[data-theme="dark"]'));
});
