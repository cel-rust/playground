// What someone last wrote, as localStorage keeps it: `node --test tests/`.

import { test } from "node:test";
import assert from "node:assert/strict";
import { DRAFT_KEY, loadDraft, saveDraft } from "../web/draft.js";

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

test("keeps none until it saves one", () => {
  assert.equal(loadDraft(new Memory()), null);
});

test("keeps the expression and input it saves, and only those", () => {
  const storage = new Memory();
  assert.equal(saveDraft({ e: "x + 1", i: '{"x": 1}', a: true }, storage), true);
  assert.deepEqual(loadDraft(storage), { e: "x + 1", i: '{"x": 1}' });
  // Blank is something someone wrote too
  saveDraft({ e: "", i: "" }, storage);
  assert.deepEqual(loadDraft(storage), { e: "", i: "" });
});

test("ignores what it can't take", () => {
  for (const stored of ["{", "null", "1", '"x"', "[]", '{"e": "x"}', '{"e": 1, "i": ""}']) {
    const storage = new Memory();
    storage.setItem(DRAFT_KEY, stored);
    assert.equal(loadDraft(storage), null, stored);
  }
});

test("does without storage", () => {
  const full = {
    getItem() {
      throw new Error("blocked");
    },
    setItem() {
      throw new Error("quota");
    },
  };
  assert.equal(loadDraft(full), null);
  assert.equal(saveDraft({ e: "x", i: "" }, full), false);
  assert.equal(loadDraft(null), null);
  assert.equal(saveDraft({ e: "x", i: "" }, null), false);
});
