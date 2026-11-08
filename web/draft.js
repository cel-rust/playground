// What someone last wrote: the expression and input as they last edited them,
// kept in this browser's localStorage, so that the page resumes them on the
// next visit. An example they load isn't theirs until they edit it, and a
// link they open isn't either (see app.js).

import { localStorageOrNull } from "./settings.js";

export const DRAFT_KEY = "cel-playground.draft";

/** The draft `storage` keeps, `{e, i}`, or null where it keeps none. */
export function loadDraft(storage = localStorageOrNull()) {
  try {
    const draft = JSON.parse(storage.getItem(DRAFT_KEY));
    if (typeof draft?.e === "string" && typeof draft?.i === "string") return { e: draft.e, i: draft.i };
  } catch {
    // Unreadable, or no storage
  }
  return null;
}

/** Keeps `draft`'s expression and input in `storage`; whether it could. */
export function saveDraft(draft, storage = localStorageOrNull()) {
  try {
    storage.setItem(DRAFT_KEY, JSON.stringify({ e: draft.e, i: draft.i }));
    return true;
  } catch {
    // No storage, or past its quota
    return false;
  }
}
