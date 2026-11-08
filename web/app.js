// The page: highlights and evaluates as you type, keeps its state in the URL
// fragment, and links to cel-rust's bug form with what it evaluated.

import { evaluator } from "./cel.js";

const $ = (id) => document.getElementById(id);
const expression = $("expression");
const highlight = $("highlight");
const editor = expression.closest(".editor");
const input = $("input");
const output = $("output");
const outputPane = output.closest(".pane");
const status = $("status");
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

// --- The URL fragment: `#s=` the deflated JSON of {e, i}, base64url ---

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
    const { e, i } = JSON.parse(new TextDecoder().decode(json));
    return typeof e === "string" && typeof i === "string" ? { e, i } : null;
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

function repro(link, withSources) {
  const lines = [`// Evaluated in the playground: ${link}`];
  if (withSources) {
    lines.push(
      `let expression = ${rawString(expression.value)};`,
      "// The top-level keys of this JSON object are the variables.",
      `let input = ${rawString(input.value)};`,
    );
  } else {
    lines.push("// The expression and input are too long for a link: they are in the one above.");
  }
  return lines.join("\n");
}

function issueUrl(cel, outcome) {
  const url = new URL(ISSUES);
  url.searchParams.set("template", "bug_report.yml");
  url.searchParams.set("version", cel.version);
  const actual = outcome.ok ? outcome.value : `${STAGES[outcome.stage] ?? outcome.stage}: ${outcome.error}`;
  url.searchParams.set("actual", actual.slice(0, 2000));
  for (const withSources of [true, false]) {
    url.searchParams.set("repro", repro(location.href, withSources));
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

// --- Highlighting: the expression's tokens, behind its transparent textarea ---

function renderTokens(text, tokens) {
  const fragment = document.createDocumentFragment();
  let at = 0;
  for (const [start, end, kind] of tokens) {
    if (start > at) fragment.append(text.slice(at, start));
    const span = document.createElement("span");
    span.className = `tok-${kind}`;
    span.textContent = text.slice(start, end);
    fragment.append(span);
    at = end;
  }
  // A <pre> doesn't show the empty line a final newline starts; a textarea
  // puts the caret on it, so give that line something.
  fragment.append(text.slice(at) + (text.endsWith("\n") ? " " : ""));
  highlight.replaceChildren(fragment);
  syncScroll();
}

function syncScroll() {
  highlight.scrollTop = expression.scrollTop;
  highlight.scrollLeft = expression.scrollLeft;
}

// --- Evaluation ---

function show(state, text, label) {
  outputPane.dataset.state = state;
  output.textContent = text;
  status.textContent = label ? `· ${label}` : "";
}

async function compile() {
  const url = new URL("cel_playground.wasm", import.meta.url);
  try {
    return await WebAssembly.compileStreaming(fetch(url));
  } catch {
    // compileStreaming needs `application/wasm`, which some servers don't send
    const response = await fetch(url);
    if (!response.ok) throw new Error(`${url}: ${response.status} ${response.statusText}`);
    return WebAssembly.compile(await response.arrayBuffer());
  }
}

async function main() {
  const loadedExamples = fetch(new URL("examples.json", import.meta.url)).then((r) => r.json());
  let cel;
  try {
    cel = await evaluator(await compile());
  } catch (error) {
    show("error", String(error), "could not load cel-rust");
    return;
  }
  $("version").textContent = `cel-rust ${cel.version}`;
  showCommit(cel.commit);

  let sequence = 0;
  async function run() {
    const id = ++sequence;
    const state = { e: expression.value, i: input.value };
    let outcome;
    if (state.e.trim() === "") {
      outcome = null;
      show("pending", "Type an expression above.");
    } else {
      outcome = await cel.evaluate(state.e, state.i);
      if (id !== sequence) return;
      if (outcome.ok) show("ok", outcome.value);
      else show("error", outcome.error, STAGES[outcome.stage] ?? outcome.stage);
    }
    const hash = `#s=${await encodeState(state)}`;
    if (id !== sequence) return;
    history.replaceState(null, "", hash);
    report.href = outcome ? issueUrl(cel, outcome) : `${ISSUES}?template=bug_report.yml`;
  }

  // On every change, unlike evaluating: the typed text is transparent until
  // its tokens are drawn.
  let highlighting = 0;
  async function rehighlight() {
    const id = ++highlighting;
    const text = expression.value;
    const tokens = await cel.tokens(text);
    if (id !== highlighting) return;
    if (tokens) {
      renderTokens(text, tokens);
      editor.classList.add("highlighting");
    } else {
      // The instance trapped: show the textarea's own text until it's back
      editor.classList.remove("highlighting");
    }
  }
  expression.addEventListener("input", rehighlight);
  expression.addEventListener("scroll", syncScroll);

  let timer;
  const schedule = () => {
    clearTimeout(timer);
    timer = setTimeout(run, DEBOUNCE_MS);
  };
  const runNow = () => {
    clearTimeout(timer);
    return run();
  };
  expression.addEventListener("input", schedule);
  input.addEventListener("input", schedule);
  document.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      runNow();
    }
  });

  copyLink.addEventListener("click", async () => {
    await runNow();
    try {
      await navigator.clipboard.writeText(location.href);
      copyLink.textContent = "Copied";
      setTimeout(() => (copyLink.textContent = "Copy link"), 1500);
    } catch {
      prompt("Copy this link:", location.href);
    }
  });

  const list = await loadedExamples.catch(() => []);
  for (const [index, example] of list.entries()) {
    examples.add(new Option(example.name, String(index)));
  }
  const load = (example) => {
    expression.value = example.expression;
    input.value = JSON.stringify(example.input, null, 2);
    rehighlight();
  };
  examples.addEventListener("change", () => {
    const example = list[Number(examples.value)];
    examples.selectedIndex = 0;
    if (example) {
      load(example);
      runNow();
    }
  });

  const state = await decodeState(location.hash);
  if (state) {
    expression.value = state.e;
    input.value = state.i;
    rehighlight();
  } else if (list.length > 0) {
    load(list[0]);
  }
  runNow();
}

main();
