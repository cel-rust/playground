// The settings as localStorage keeps them: `node --test tests/`.

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

test("each setting defaults to its first value", () => {
  assert.deepEqual(defaults(), { theme: "system" });
});

test("keeps what it saves", () => {
  const storage = new Memory();
  assert.equal(save(storage, { theme: "dark" }), true);
  assert.deepEqual(load(storage), { theme: "dark" });
});

test("ignores what it can't take", () => {
  for (const stored of ["{", "null", "1", '"dark"', "[]", '{"theme": "blue"}', '{"theme": 1}']) {
    const storage = new Memory();
    storage.setItem(STORAGE_KEY, stored);
    assert.deepEqual(load(storage), defaults(), stored);
  }
  const storage = new Memory();
  storage.setItem(STORAGE_KEY, '{"theme": "light", "gone": true}');
  assert.deepEqual(load(storage), { theme: "light" });
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

test("index.html applies the same theme before the page draws", async () => {
  const html = await readFile(new URL("../web/index.html", import.meta.url), "utf8");
  assert.ok(html.includes(`localStorage.getItem("${STORAGE_KEY}")`));
  // It sets the themes other than the default, which the stylesheet styles
  assert.deepEqual(SETTINGS.theme.values, ["system", "light", "dark"]);
  assert.ok(html.includes('if (theme === "light" || theme === "dark")'));
  const css = await readFile(new URL("../web/style.css", import.meta.url), "utf8");
  assert.ok(css.includes(':root:not([data-theme="light"])'));
  assert.ok(css.includes(':root[data-theme="dark"]'));
});
