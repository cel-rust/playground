// The page's settings: chosen in the menu behind the gear, kept in this
// browser's localStorage, and applied again on the next visit.
//
// index.html applies them from the same key before the page draws, so the
// page doesn't flash: keep the two in step (tests/settings.test.mjs checks).

export const STORAGE_KEY = "cel-playground.settings";

/**
 * Each setting, by the name its inputs in the menu have: the values it takes,
 * its default first. One applies as an attribute of the page, `data-<name>`
 * (`data-code-theme` for `codeTheme`), which the stylesheet styles by, and
 * the page's script asks for it (see `setUpSettings`). One that's `on` or
 * `off` is a switch in the menu, the others a choice of radios.
 */
export const SETTINGS = {
  /** The page's colours: the system's, light or dark. */
  theme: { values: ["system", "light", "dark"] },
  /** The code's; one with a light and a dark variant follows the page's. */
  codeTheme: { values: ["github", "catppuccin", "solarized", "dracula", "tokyonight-moon"] },
  /** Evaluating as you type; off, Ctrl+Enter or Run does. */
  autoRun: { values: ["on", "off"] },
  /** Highlighting the expression, the input and the output. */
  highlighting: { values: ["on", "off"] },
  /** What only who knows CEL needs: for now, the AST explorer. */
  advanced: { values: ["off", "on"] },
};

/** Whether `name` is a switch, `on` or `off`. */
export const isSwitch = (name) => SETTINGS[name].values.every((v) => v === "on" || v === "off");

const apply = (name, value) => (document.documentElement.dataset[name] = value);

/**
 * The settings `storage` keeps: each the default where it keeps none, or
 * one the setting doesn't take.
 */
export function load(storage) {
  let stored = null;
  try {
    stored = JSON.parse(storage.getItem(STORAGE_KEY));
  } catch {
    // Unreadable, or no storage: the defaults
  }
  return Object.fromEntries(
    Object.entries(SETTINGS).map(([name, { values }]) => {
      const value = stored?.[name];
      return [name, values.includes(value) ? value : values[0]];
    }),
  );
}

/** Keeps `settings` in `storage`; whether it could. */
export function save(storage, settings) {
  try {
    storage.setItem(STORAGE_KEY, JSON.stringify(settings));
    return true;
  } catch {
    return false;
  }
}

/** localStorage, or null where the browser blocks it. */
export function localStorageOrNull() {
  try {
    return globalThis.localStorage ?? null;
  } catch {
    return null;
  }
}

/**
 * Sets up the menu, a popover opened by `toggle`: its inputs show the
 * settings, and changing one applies it and keeps it. Without storage, a
 * change lasts until the page is left.
 *
 * Returns what the page's script asks: `get(name)`, a setting's value, and
 * `on(name, listener)`, which calls `listener(value)` when it changes.
 */
export function setUpSettings(menu, toggle, storage = localStorageOrNull()) {
  const store = storage ?? { getItem: () => null, setItem() {} };
  const settings = load(store);
  const changes = new EventTarget();
  for (const name of Object.keys(SETTINGS)) {
    // What index.html applied, unless it isn't a value the setting takes
    apply(name, settings[name]);
    for (const input of menu.querySelectorAll(`input[name="${name}"]`)) {
      const switched = isSwitch(name);
      if (switched) input.checked = settings[name] === "on";
      else input.checked = input.value === settings[name];
      input.addEventListener("change", () => {
        const value = switched ? (input.checked ? "on" : "off") : input.value;
        settings[name] = value;
        apply(name, value);
        save(store, settings);
        changes.dispatchEvent(new CustomEvent(name, { detail: value }));
      });
    }
  }

  // Each code theme's sample, in its colours
  const sample = menu.querySelector("#code-sample").content;
  for (const target of menu.querySelectorAll(".code-sample")) {
    target.append(sample.cloneNode(true));
  }

  // Under the gear, right-aligned with it but within the page, which on a
  // phone has the gear on the left: in the top layer, out of the page's
  // flow, so it's placed by hand, and again as the gear moves (a phone's
  // address bar or keyboard resizes the page)
  const place = () => {
    const gear = toggle.getBoundingClientRect();
    const page = document.documentElement.clientWidth;
    // Its width, computed from the stylesheet's though it's not shown yet
    const width = Math.min(parseFloat(getComputedStyle(menu).width) || 256, page - 16);
    menu.style.top = `${gear.bottom + 6}px`;
    menu.style.maxHeight = `${document.documentElement.clientHeight - gear.bottom - 14}px`;
    menu.style.left = `${Math.max(8, Math.min(gear.right - width, page - 8 - width))}px`;
  };
  menu.addEventListener("beforetoggle", (event) => {
    if (event.newState === "open") place();
  });
  const follow = () => menu.matches(":popover-open") && place();
  addEventListener("resize", follow);
  addEventListener("scroll", follow, { passive: true });

  return {
    get: (name) => settings[name],
    on: (name, listener) => changes.addEventListener(name, (event) => listener(event.detail)),
  };
}
