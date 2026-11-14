# CEL playground

Evaluate [CEL](https://cel.dev) expressions against JSON in the browser, with
[cel-rust](https://github.com/cel-rust/cel-rust) compiled to WebAssembly.

**Try it at <https://cel-rust.github.io/playground/>**

Three panes: the expression, the JSON input, and the output, which shows the
result or the error. It evaluates as you type; `Ctrl`/`Cmd`+`Enter` evaluates
right away. With auto run off in the settings, only that, or Run, evaluates.

- **Environment:** `Env::stdlib()`, with optional values, plus the strings,
  lists, math and encoders extensions.
- **Version:** cel-rust 0.16.0, the newest, or 0.15.0, chosen beside the
  title. Each is a build of its own: only the one chosen is downloaded, and
  switching runs what's there again with the other.
- **Highlighting:** the expression is highlighted as you type, whether it
  parses or not. Once it parses, its identifiers are coloured by what the
  parsed tree says they are: variables, variables missing from the input
  (underlined), names a macro binds, functions, macros, fields, namespaces and
  types. Parse errors are underlined where cel reports them. The input is
  highlighted too, its top-level keys as variables, with where its JSON breaks
  underlined; so is the output. Past 20,000 characters, text is left plain.
  The colours are a code theme's, chosen in the settings.
- **AST explorer**, with Advanced on in the settings: the "AST" strip right
  of the editor opens the tree cel
  parses the expression into, macros expanded. What a macro or the parser
  made, rather than text of the expression, is faded. Pointing at a node
  highlights the token cel positions it at.
- **Input:** a JSON object, whose top-level keys are the variables. Integral
  numbers are `int`s (`uint`s past `i64::MAX`), other numbers `double`s.
- **Output:** the result as a CEL literal (`1u`, `1.0`, `b"\x00"`,
  `timestamp("...")`, `optional.none()`), so its type shows.
- **Sharing:** the expression and input live in the URL fragment, as do the
  version of cel-rust and whether the AST explorer is open, so a link
  reproduces them. A link opens with its version, or the one of its line on
  offer (0.16.1 for 0.16.0), else the newest; one made before there was a
  choice opens with the newest, or, pasted into an open page, keeps the
  version chosen. "Report a bug with this CEL expression", over the editor,
  opens cel-rust's bug form with them filled in, the version, and what
  evaluating them came to.
- **Resuming:** what you last wrote or edited, expression and input, is kept
  in the browser: reloading, or coming back, resumes it. Loading an example
  doesn't replace it, until you edit the example; a link opens as it is.
- **Settings:** the gear opens them. Switches for auto run and syntax
  highlighting, both on, and for Advanced, off: what only who knows CEL
  needs, for now the AST explorer. A light or dark page, or the system's,
  and the code theme: GitHub (the default), Catppuccin or Solarized, which
  follow the page's light or dark, or Dracula or
  [Tokyo Night](https://github.com/folke/tokyonight.nvim) Moon, dark either
  way. Each theme's colours are its own project's, mapped to CEL's tokens in
  `web/style.css`. They're kept in the browser, for the next visit, not in
  links.

## Running it locally

Needs Rust with the `wasm32-unknown-unknown` target (`rustup target add
wasm32-unknown-unknown`); no other tooling.

```sh
cargo run --example localhost            # then open http://localhost:8000
cargo run --example localhost -- 8080    # on another port
```

It builds a wasm module for each version of cel and serves `web/` as it is on
disk, so changes to the page show on reload; restart it after changing the Rust
code. It only listens on 127.0.0.1. Opening `web/index.html` as a file doesn't
work: browsers don't load the scripts and the modules from `file://` URLs.

`./build.sh` builds what gets deployed into `dist/`: the same files.

## Testing

```sh
cargo test                                       # conversions, rendering, examples,
                                                 # with the newest cel
cargo test --no-default-features --features cel-0-15  # the same, with 0.15
./build.sh && node --test tests/*.test.mjs       # each version's wasm, through
                                                 # web/cel.js; the settings and
                                                 # draft, as kept; which version
                                                 # a link gets
```

CI runs `cargo test` with each version `./versions.sh` lists.

The JavaScript tests need [Node.js](https://nodejs.org) (tested with 22), and
nothing from npm.

## How it fits together

- `src/lib.rs`: `evaluate(expression, input)`, returning the outcome as JSON.
- `Cargo.toml`: each version of cel on offer is a `cel-0-N` dependency, of the
  0.N line, and the feature of the same name, which a build enables one of;
  `src/lib.rs` makes it the `cel` crate. `versions.sh` lists them, newest
  first, with the version `Cargo.lock` has of each.
- `src/tokens.rs`: the tokens to highlight, positioned in UTF-16 as JavaScript
  indexes strings. cel's lexer isn't public, in 0.15 or 0.16, so this mirrors
  its rules (`CEL.g4`); a test checks it against the positions of what each
  version parses. A word cel reserves is a keyword where its parser rejects it
  (`if`, but not `a.if`), which another test checks against cel's errors. The
  page draws them in a `<pre>` behind the expression's transparent textarea.
- `src/ast.rs`: the parsed tree for the explorer. cel positions each node at
  one token; a node whose token doesn't spell it (an identifier its name, an
  operator its symbol, a call its name before a `(`) was made by the macro
  whose call's `(` it sits on, or by the parser.
- `src/highlight.rs`: what the editor highlights, on every keystroke. Each
  identifier the tree has a node for is resolved as cel would: bound by an
  enclosing comprehension, a key of the input, a type, or missing. cel 0.15
  can't be asked whether `ns.f` is a function, so a call's target is a
  namespace when evaluating `ns.f()` with no variables doesn't read `ns`.
  Identifiers without a node are told by where they are: before a `(`, after
  a `.`.
- `src/json.rs`: what the input highlights: its tokens, strictly JSON's, its
  keys, and the token serde_json fails at. The output, a CEL literal, goes
  through `src/highlight.rs` as an expression of no variables.
- `src/abi.rs`: the raw exports `web/cel.js` calls. No wasm-bindgen: strings
  go through buffers from `alloc`, the outcome comes back at `output_ptr`.
- `web/cel.js`: the JavaScript side of that, shared by the page and the smoke
  test. A panic aborts the wasm instance; the message is reported and the
  next evaluation gets a fresh instance.
- `web/versions.js`: which version's module runs what a link asks for.
  `versions.json`, which `build.sh` writes, lists the versions; each module
  is `cel-<version>.wasm`.
- `web/draft.js`: what you last wrote, kept in localStorage. `app.js` keeps
  an example out of the address bar until it's edited, so reloading resumes
  the draft.
- `web/settings.js`: the settings behind the gear, kept in localStorage.
  `index.html` applies the theme from it before the page draws, so it doesn't
  flash.
- `web/examples.json`: the examples, also evaluated by both test suites.
- `examples/localhost.rs`: the local server, std only; it serves the files
  directly in `web/`, the modules and their `versions.json`, nothing else.

## Deploying

`.github/workflows/pages.yml` tests, builds and deploys `dist/` to GitHub Pages
on every push to `main`. In the repository settings, set Pages' source to
"GitHub Actions". Dependabot opens a PR when cel is released. A release of a
line on offer (0.16.1) merges as it is, and deploys in its line's place; for a
new line (0.17), offer another version instead, as below.

## Offering another version of cel

For 0.17, in `Cargo.toml`, add a `cel-0-17` dependency on `0.17.0` and a
`cel-0-17` feature enabling it, and make it the default; in `src/lib.rs`, add
it to the `extern crate`s and the `compile_error!`. `cargo build` locks it,
and `versions.sh`, `build.sh`, CI and the page pick it up from there. To stop
offering a version, remove it from the same places. Where versions differ, the
code tells them apart with `#[cfg(feature = "cel-0-N")]`.
