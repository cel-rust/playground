// The page's settings: chosen in the menu behind the gear, kept in this
// browser's localStorage, and applied again on the next visit.
//
// index.html applies the theme from the same key before the page draws, so
// it doesn't flash: keep the two in step (tests/settings.test.mjs checks).

export const STORAGE_KEY = "cel-playground.settings";

/**
 * Each setting, by the name its inputs in the menu have: the values it takes,
 * its default first, and how one applies.
 */
export const SETTINGS = {
  /** The page's colours; the code's are tokyonight moon's either way. */
  theme: {
    values: ["system", "light", "dark"],
    apply(value) {
      const root = document.documentElement;
      if (value === "system") delete root.dataset.theme;
      else root.dataset.theme = value;
    },
  },
};

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
function localStorageOrNull() {
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
 */
export function setUpSettings(menu, toggle, storage = localStorageOrNull()) {
  const store = storage ?? { getItem: () => null, setItem() {} };
  const settings = load(store);
  for (const [name, setting] of Object.entries(SETTINGS)) {
    setting.apply(settings[name]);
    for (const input of menu.querySelectorAll(`input[name="${name}"]`)) {
      input.checked = input.value === settings[name];
      input.addEventListener("change", () => {
        settings[name] = input.value;
        setting.apply(input.value);
        save(store, settings);
      });
    }
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
    menu.style.left = `${Math.max(8, Math.min(gear.right - width, page - 8 - width))}px`;
  };
  menu.addEventListener("beforetoggle", (event) => {
    if (event.newState === "open") place();
  });
  const follow = () => menu.matches(":popover-open") && place();
  addEventListener("resize", follow);
  addEventListener("scroll", follow, { passive: true });
}
