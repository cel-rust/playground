// The page: highlights and evaluates as you type, or when asked (see the
// settings), with the version of cel-rust chosen (see versions.js), keeps its
// state in the URL fragment and what you last wrote in the browser (see
// draft.js), and links to cel-rust's bug form with what it evaluated.

import { evaluator } from "./cel.js";
import { loadDraft, saveDraft } from "./draft.js";
import { setUpSettings } from "./settings.js";
import { moduleFile, pick } from "./versions.js";

const $ = (id) => document.getElementById(id);
const settings = setUpSettings($("settings"), $("settings-toggle"));
const versionSelect = $("version");
const expression = $("expression");
const input = $("input");
const output = $("output");
const outputPane = output.closest(".pane");
const status = $("status");
const evaluatedBy = $("evaluated-by");
const examples = $("examples");
const copyLink = $("copy-link");
const report = $("report");

const STAGES = {
  parse: "parse error",
  input: "input error",
  evaluation: "evaluation error",
  panic: "cel-rust panicked: that's a bug, please report it",
  trap: "the WebAssembly instance trapped: please report it",
};
const ISSUES = "https://github.com/cel-rust/cel-rust/issues/new";
const MAX_URL = 8000;
const DEBOUNCE_MS = 150;
// Past this many characters, text isn't highlighted: a 20,000 character input
// takes ~3 ms to highlight, on every keystroke, an output ~45 ms, mostly
// parsing it back.
const HIGHLIGHT_LIMIT = 20_000;

// --- The URL fragment: `#s=` the deflated JSON of {e, i, a?, v?}, base64url:
// expression, input, whether the AST explorer is open, and the version of
// cel-rust, which links made before there was a choice don't have ---

async function pipe(bytes, transform) {
  const stream = new Blob([bytes]).stream().pipeThrough(transform);
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

function toBase64Url(bytes) {
  let binary = "";
  for (let i = 0; i < bytes.length; i += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
}

function fromBase64Url(text) {
  const binary = atob(text.replaceAll("-", "+").replaceAll("_", "/"));
  return Uint8Array.from(binary, (c) => c.charCodeAt(0));
}

async function encodeState(state) {
  const json = new TextEncoder().encode(JSON.stringify(state));
  return toBase64Url(await pipe(json, new CompressionStream("deflate-raw")));
}

async function decodeState(hash) {
  const data = new URLSearchParams(hash.replace(/^#/, "")).get("s");
  if (!data) return null;
  try {
    const json = await pipe(fromBase64Url(data), new DecompressionStream("deflate-raw"));
    const { e, i, a, v } = JSON.parse(new TextDecoder().decode(json));
    if (typeof e !== "string" || typeof i !== "string") return null;
    return { e, i, a: a === true, v: typeof v === "string" ? v : null };
  } catch (error) {
    console.warn("ignoring the state in the URL:", error);
    return null;
  }
}

// --- The bug report link ---

/** `text` as a Rust raw string literal. */
function rawString(text) {
  let hashes = "#";
  while (text.includes(`"${hashes}`)) hashes += "#";
  return `r${hashes}"${text}"${hashes}`;
}

function repro(link, state, withSources) {
  const lines = [`// Evaluated in the playground: ${link}`];
  if (withSources) {
    lines.push(
      `let expression = ${rawString(state.e)};`,
      "// The top-level keys of this JSON object are the variables.",
      `let input = ${rawString(state.i)};`,
    );
  } else {
    lines.push("// The expression and input are too long for a link: they are in the one above.");
  }
  return lines.join("\n");
}

/** The bug form, filled in with `state`, its `link`, and what evaluating it came to. */
function issueUrl(cel, outcome, state, link) {
  const url = new URL(ISSUES);
  url.searchParams.set("template", "bug_report.yml");
  url.searchParams.set("version", cel.version);
  const actual = outcome.ok ? outcome.value : `${STAGES[outcome.stage] ?? outcome.stage}: ${outcome.error}`;
  url.searchParams.set("actual", actual.slice(0, 2000));
  for (const withSources of [true, false]) {
    url.searchParams.set("repro", repro(link, state, withSources));
    if (url.href.length <= MAX_URL) return url.href;
  }
  url.searchParams.set("repro", "// Paste the playground link here: it is too long to fill in.");
  return url.href;
}

/** The commit the page was built from, after the source link. */
function showCommit(commit) {
  const span = $("commit");
  if (/^[0-9a-f]{40}$/.test(commit)) {
    const link = document.createElement("a");
    link.href = `https://github.com/cel-rust/playground/commit/${commit}`;
    link.textContent = commit.slice(0, 7);
    span.append(" at ", link);
  } else {
    span.append(` (${commit})`);
  }
}

// --- Highlighting: tokens drawn in a <pre>, behind a transparent textarea ---

/**
 * `text` as spans, one per token, each with what's said of it (`sem-*`) and
 * whether an error is reported there; `hovered` is a range to highlight.
 */
function spans(text, { tokens, marks, errors }, hovered = null) {
  const classes = new Map(marks.map(([start, end, name]) => [`${start}:${end}`, `sem-${name}`]));
  const failing = new Set(errors.map(([start, end]) => `${start}:${end}`));
  const fragment = document.createDocumentFragment();
  let at = 0;
  for (const [start, end, kind] of tokens) {
    if (start > at) fragment.append(text.slice(at, start));
    const span = document.createElement("span");
    const key = `${start}:${end}`;
    span.className = [
      `tok-${kind}`,
      classes.get(key),
      failing.has(key) && "sem-error",
      hovered && hovered.start === start && hovered.end === end && "hovered",
    ]
      .filter(Boolean)
      .join(" ");
    span.textContent = text.slice(start, end);
    fragment.append(span);
    at = end;
  }
  // A <pre> doesn't show the empty line a final newline starts; a textarea
  // puts the caret on it, so give that line something.
  fragment.append(text.slice(at) + (text.endsWith("\n") ? " " : ""));
  // An error at the end, where something is missing: underline a space
  if (errors.some(([start, end]) => start === end && start >= text.length)) {
    const missing = document.createElement("span");
    missing.className = "sem-error";
    missing.textContent = " ";
    fragment.append(missing);
  }
  return fragment;
}

/** The highlighting of `textarea`, drawn in the <pre> behind it. */
function overlay(textarea) {
  const editor = textarea.closest(".editor");
  const pre = editor.querySelector("pre");
  /** What's drawn: the text, and what to highlight in it. */
  let drawn = null;
  let hovered = null;
  const syncScroll = () => {
    pre.scrollTop = textarea.scrollTop;
    pre.scrollLeft = textarea.scrollLeft;
  };
  const draw = () => {
    pre.replaceChildren(spans(drawn.text, drawn.result, hovered));
    syncScroll();
  };
  textarea.addEventListener("scroll", syncScroll);
  return {
    /** Draws what to highlight in `text`; `null` shows the textarea's own text. */
    show(text, result) {
      drawn = result && { text, result };
      if (drawn) draw();
      else pre.replaceChildren();
      editor.classList.toggle("highlighting", drawn !== null);
    },
    /** Highlights `range`, as the AST explorer's pointer moves. */
    hover(range) {
      if (hovered?.start === range?.start && hovered?.end === range?.end) return;
      hovered = range;
      if (drawn?.text === textarea.value) draw();
    },
  };
}

const expressionView = overlay(expression);
const inputView = overlay(input);

// --- The AST explorer: the tree cel parses the expression into ---

const astPane = $("ast");
const astToggle = $("ast-toggle");
const astTree = $("ast-tree");
/** The subtrees folded by hand, by their path from the root. */
const folded = new Set();

function setAstOpen(open) {
  astPane.dataset.open = String(open);
  astToggle.setAttribute("aria-expanded", String(open));
  astTree.hidden = !open;
}

const astOpen = () => astPane.dataset.open === "true";

function astNote(text) {
  const note = document.createElement("p");
  note.className = "ast-note";
  note.textContent = text;
  return note;
}

/** One node and its children: `<details>` when it has some, a row if not. */
function astNode(node, path) {
  const row = [];
  const part = (className, text) => {
    const span = document.createElement("span");
    span.className = className;
    span.textContent = text;
    row.push(span, " ");
  };
  if (node.role) part("ast-role", `${node.role}:`);
  part("ast-kind", node.kind);
  if (node.detail) part("ast-detail", node.detail);
  if (node.text !== null && node.text !== node.detail) part("ast-text", node.text);
  part("ast-id", `#${node.id}`);

  let element;
  let line;
  if (node.children.length === 0) {
    element = line = document.createElement("div");
    element.className = "ast-leaf";
    element.append(...row);
  } else {
    element = document.createElement("details");
    element.open = !folded.has(path);
    line = document.createElement("summary");
    line.append(...row);
    element.append(line, ...node.children.map((child, i) => astNode(child, `${path}/${i}`)));
    element.addEventListener("toggle", () => {
      if (element.open) folded.delete(path);
      else folded.add(path);
    });
  }
  if (node.start !== null) {
    line.dataset.start = node.start;
    line.dataset.end = node.end;
  }
  if (node.from) {
    line.classList.add("ast-made");
    line.title =
      node.from === "parser"
        ? "Made by the parser, not text of the expression"
        : `Made by the ${node.from}() macro: positioned at its call`;
  }
  return element;
}

// Pointing at a node, or focusing it, highlights where cel positions it
const rangeOf = (target) => {
  const line = target instanceof Element ? target.closest("[data-start]") : null;
  return line ? { start: Number(line.dataset.start), end: Number(line.dataset.end) } : null;
};
astTree.addEventListener("mouseover", (event) => expressionView.hover(rangeOf(event.target)));
astTree.addEventListener("mouseleave", () => expressionView.hover(null));
astTree.addEventListener("focusin", (event) => expressionView.hover(rangeOf(event.target)));
astTree.addEventListener("focusout", () => expressionView.hover(null));

function renderAst(text, result) {
  if (text.trim() === "") {
    astTree.classList.remove("stale");
    astTree.replaceChildren(astNote("Type an expression to see its tree."));
  } else if (result === null) {
    astTree.replaceChildren(astNote("The WebAssembly instance trapped: no tree."));
  } else if (!result.ok) {
    // Keep the last tree that parsed, faded, rather than flicker while typing
    const root = astTree.querySelector(".ast-root");
    astTree.classList.add("stale");
    astTree.replaceChildren(astNote("Doesn't parse: this is the last tree that did."), ...(root ? [root] : []));
  } else {
    const root = astNode(result.root, "0");
    root.classList.add("ast-root");
    astTree.classList.remove("stale");
    // As cel-rust 0.14's: no node is at text of the expression
    const unpositioned = result.positioned === false;
    astTree.replaceChildren(
      ...(unpositioned ? [astNote("This version of cel-rust doesn't say where nodes are in the expression.")] : []),
      root,
    );
  }
}

// --- Evaluation ---

/**
 * Shows `text` in the output, highlighted if `result` says how, and the
 * `version` of cel-rust it came from, if it came from one.
 */
function show(state, text, label, result = null, version = null) {
  outputPane.dataset.state = state;
  if (result) output.replaceChildren(spans(text, result));
  else output.textContent = text;
  status.textContent = label ? `· ${label}` : "";
  evaluatedBy.textContent = version ? `by cel-rust ${version}` : "";
}

async function compile(file) {
  const url = new URL(file, import.meta.url);
  try {
    return await WebAssembly.compileStreaming(fetch(url));
  } catch {
    // compileStreaming needs `application/wasm`, which some servers don't send
    const response = await fetch(url);
    if (!response.ok) throw new Error(`${url}: ${response.status} ${response.statusText}`);
    return WebAssembly.compile(await response.arrayBuffer());
  }
}

/** The versions of cel-rust on offer, newest first. */
async function loadVersions() {
  const url = new URL("versions.json", import.meta.url);
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url}: ${response.status} ${response.statusText}`);
  const versions = await response.json();
  if (!Array.isArray(versions) || versions.length === 0) throw new Error(`${url}: no versions`);
  return versions;
}

/** Each version's evaluator, its module fetched and compiled once. */
const evaluators = new Map();
function evaluatorFor(version) {
  let loading = evaluators.get(version);
  if (!loading) {
    loading = compile(moduleFile(version)).then(evaluator);
    // Asked for again, it's fetched again
    loading.catch(() => evaluators.delete(version));
    evaluators.set(version, loading);
  }
  return loading;
}

async function main() {
  const loadedExamples = fetch(new URL("examples.json", import.meta.url)).then((r) => r.json());
  /** What's on offer; the version that runs what's there, and its evaluator. */
  let versions, version, cel;
  try {
    versions = await loadVersions();
    // A link's own, so as not to load the newest first
    version = pick(versions, (await decodeState(location.hash))?.v);
    cel = await evaluatorFor(version);
  } catch (error) {
    show("error", String(error), "could not load cel-rust");
    return;
  }
  versionSelect.replaceChildren(...versions.map((each) => new Option(each)));
  versionSelect.value = version;
  versionSelect.disabled = false;
  showCommit(cel.commit);

  // Only in advanced mode, while it's open, and not on every keystroke
  const astShown = () => settings.get("advanced") === "on" && astOpen();
  let astSequence = 0;
  async function refreshAst() {
    if (!astShown()) return;
    const id = ++astSequence;
    const text = expression.value;
    const result = text.trim() === "" ? null : await cel.ast(text);
    if (id !== astSequence) return;
    renderAst(text, result);
  }

  /**
   * Highlights `textarea` with `highlighter` on every change, unlike
   * evaluating: the typed text is transparent until its tokens are drawn.
   * Returns what redraws it.
   */
  function highlighting(textarea, view, highlighter) {
    let sequence = 0;
    async function refresh() {
      const id = ++sequence;
      const text = textarea.value;
      // Off, too long to redraw on each keystroke, or the instance trapped:
      // the textarea shows its own text
      const off = settings.get("highlighting") === "off" || text.length > HIGHLIGHT_LIMIT;
      const result = off ? null : await highlighter(text);
      if (id === sequence) view.show(text, result);
    }
    textarea.addEventListener("input", refresh);
    return refresh;
  }
  const rehighlight = highlighting(expression, expressionView, (text) => cel.highlight(text, input.value));
  const rehighlightInput = highlighting(input, inputView, (text) => cel.highlightInput(text));

  /**
   * The value, a CEL literal, highlighted as an expression: of no variables
   * (`null` isn't an object, so none is missing), and not underlined where it
   * doesn't parse back, as past cel's nesting limit.
   */
  async function highlightValue(value) {
    if (settings.get("highlighting") === "off" || value.length > HIGHLIGHT_LIMIT) return null;
    const result = await cel.highlight(value, "null");
    return result && { ...result, errors: [] };
  }

  const current = () => ({ e: expression.value, i: input.value, ...(astOpen() && { a: true }), v: version });

  /**
   * Where what's in the editors comes from: an `example` as loaded, a `link`
   * as opened, or a `draft`, what someone wrote or edited, which the browser
   * keeps for the next visit.
   */
  let origin = "example";
  const keepDraft = () => origin === "draft" && saveDraft(current());

  // The address bar, in step with what's there, evaluated or not; but not
  // with an example as loaded, so that reloading resumes the draft
  let saving = 0;
  async function save(state) {
    const id = ++saving;
    const hash = `#s=${await encodeState(state)}`;
    if (id === saving) history.replaceState(null, "", origin === "example" ? location.pathname + location.search : hash);
    return hash;
  }
  const linkTo = async (state) => new URL(`#s=${await encodeState(state)}`, location.href).href;

  /** What was last evaluated, and what it came to; null before anything is. */
  let evaluated = null;
  const isStale = () =>
    evaluated !== null &&
    (evaluated.state.e !== expression.value || evaluated.state.i !== input.value || evaluated.state.v !== version);

  /** What evaluating came to, and the version that evaluated it. */
  async function display({ state, outcome }) {
    if (outcome === null) {
      show("pending", "Type an expression above.");
    } else if (outcome.ok) {
      show("ok", outcome.value, null, await highlightValue(outcome.value), state.v);
    } else {
      show("error", outcome.error, STAGES[outcome.stage] ?? outcome.stage, null, state.v);
    }
  }

  let sequence = 0;
  async function evaluate() {
    const id = ++sequence;
    const state = current();
    // The version of `state`, should another be chosen meanwhile
    const evaluating = cel;
    const outcome = state.e.trim() === "" ? null : await evaluating.evaluate(state.e, state.i);
    if (id !== sequence) return;
    evaluated = { state, outcome };
    await display(evaluated);
    outputPane.dataset.stale = String(isStale());
    const hash = await save(state);
    if (id !== sequence) return;
    const link = new URL(hash, location.href).href;
    report.href = outcome ? issueUrl(evaluating, outcome, state, link) : `${ISSUES}?template=bug_report.yml`;
  }

  /** What an edit leads to, and, with auto run on or when asked, evaluating. */
  async function update(evaluating) {
    clearTimeout(timer);
    keepDraft();
    refreshAst();
    // The input decides which variables are missing
    rehighlight();
    outputPane.dataset.stale = String(isStale());
    await (evaluating ? evaluate() : save(current()));
  }

  let timer;
  const autoRun = () => settings.get("autoRun") === "on";
  const schedule = () => {
    clearTimeout(timer);
    timer = setTimeout(() => update(autoRun()), DEBOUNCE_MS);
  };
  /** Whatever is scheduled, now. */
  const flush = () => update(autoRun());
  const runNow = () => update(true);

  // Editing makes what's there the draft, an example or a link included
  const edited = () => {
    origin = "draft";
    schedule();
  };
  expression.addEventListener("input", edited);
  input.addEventListener("input", edited);
  // Within the debounce of the last keystroke, or with auto run off
  addEventListener("pagehide", keepDraft);
  document.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      runNow();
    }
  });
  $("run").addEventListener("click", runNow);

  astToggle.addEventListener("click", () => {
    setAstOpen(!astOpen());
    // Draws the tree, and keeps the URL in step
    flush();
  });

  // Of what's there, whether the address bar has it or not (an example)
  copyLink.addEventListener("click", async () => {
    await flush();
    const link = await linkTo(current());
    try {
      await navigator.clipboard.writeText(link);
      copyLink.textContent = "Copied";
      setTimeout(() => (copyLink.textContent = "Copy link"), 1500);
    } catch {
      prompt("Copy this link:", link);
    }
  });

  settings.on("autoRun", (value) => {
    if (value === "on" && (evaluated === null || isStale())) runNow();
  });
  settings.on("highlighting", () => {
    rehighlight();
    rehighlightInput();
    if (evaluated) display(evaluated);
  });
  settings.on("advanced", refreshAst);

  const list = await loadedExamples.catch(() => []);
  for (const [index, example] of list.entries()) {
    examples.add(new Option(example.name, String(index)));
  }
  const load = (example) => {
    origin = "example";
    expression.value = example.expression;
    input.value = JSON.stringify(example.input, null, 2);
    rehighlight();
    rehighlightInput();
  };
  examples.addEventListener("change", () => {
    const example = list[Number(examples.value)];
    examples.selectedIndex = 0;
    if (example) {
      load(example);
      flush();
    }
  });

  /**
   * Runs what's there with `wanted`, or the version that stands in for it
   * (see versions.js), once its module loads: as an edit would, highlighted
   * again, and evaluated with auto run on. The last version asked for wins.
   */
  let switching = 0;
  async function use(wanted) {
    const id = ++switching;
    const next = pick(versions, wanted);
    versionSelect.value = next;
    versionSelect.setAttribute("aria-busy", String(next !== version));
    if (next === version) return;
    let loaded;
    try {
      loaded = await evaluatorFor(next);
    } catch (error) {
      if (id !== switching) return;
      versionSelect.value = version;
      show("error", String(error), `could not load cel-rust ${next}`);
      return;
    } finally {
      if (id === switching) versionSelect.setAttribute("aria-busy", "false");
    }
    if (id !== switching) return;
    version = next;
    cel = loaded;
    rehighlightInput();
    await flush();
  }
  versionSelect.addEventListener("change", () => use(versionSelect.value));

  // A link made before there was a choice of version keeps the one chosen
  const open = (state) => {
    origin = "link";
    expression.value = state.e;
    input.value = state.i;
    setAstOpen(state.a);
    if (state.v !== null) use(state.v);
  };
  // A link opened in this tab, as pasted into its address bar: only the
  // fragment changes, so the page isn't loaded again
  addEventListener("hashchange", async () => {
    // Before it's replaced, the last keystrokes of a draft
    keepDraft();
    const hash = location.hash;
    const state = await decodeState(hash);
    if (state === null || location.hash !== hash) return;
    open(state);
    rehighlight();
    rehighlightInput();
    flush();
  });

  // A link first, then what was last written, then the first example
  const state = await decodeState(location.hash);
  const draft = state ? null : loadDraft();
  if (state) {
    open(state);
  } else if (draft) {
    origin = "draft";
    expression.value = draft.e;
    input.value = draft.i;
  } else if (list.length > 0) {
    load(list[0]);
  }
  rehighlight();
  rehighlightInput();
  if (!autoRun()) show("pending", "Ctrl+Enter, or Run, evaluates the expression.");
  flush();
}

main();
