# CEL playground

Evaluate [CEL](https://cel.dev) expressions against JSON in the browser, with
[cel-rust](https://github.com/cel-rust/cel-rust) compiled to WebAssembly.

**Try it at <https://cel-rust.github.io/playground/>**

Three panes: the expression, the JSON input, and the output, which shows the
result or the error. It evaluates as you type; `Ctrl`/`Cmd`+`Enter` evaluates
right away.

- **Environment:** `Env::stdlib()`, with optional values, plus the strings,
  lists, math and encoders extensions.
- **Highlighting:** the expression is highlighted as you type, whether it
  parses or not.
- **AST explorer:** the "AST" strip right of the editor opens the tree cel
  parses the expression into, macros expanded. What a macro or the parser made,
  rather than text of the expression, is faded.
- **Input:** a JSON object, whose top-level keys are the variables. Integral
  numbers are `int`s (`uint`s past `i64::MAX`), other numbers `double`s.
- **Output:** the result as a CEL literal (`1u`, `1.0`, `b"\x00"`,
  `timestamp("...")`, `optional.none()`), so its type shows.
- **Sharing:** the expression and input live in the URL fragment, as does
  whether the AST explorer is open, so a link reproduces them. "Report a bug" opens cel-rust's bug form with them filled in.

## Running it locally

Needs Rust with the `wasm32-unknown-unknown` target (`rustup target add
wasm32-unknown-unknown`); no other tooling.

```sh
cargo run --example localhost            # then open http://localhost:8000
cargo run --example localhost -- 8080    # on another port
```

It builds the wasm module and serves `web/` as it is on disk, so changes to the
page show on reload; restart it after changing the Rust code. It only listens on
127.0.0.1. Opening `web/index.html` as a file doesn't work: browsers don't load
the scripts and the module from `file://` URLs.

`./build.sh` builds what gets deployed into `dist/`: the same files.

## Testing

```sh
cargo test                                       # conversions, rendering, examples
./build.sh && node --test tests/smoke.test.mjs   # the wasm, through web/cel.js
```

The smoke test needs [Node.js](https://nodejs.org) (tested with 22), and
nothing from npm.

## How it fits together

- `src/lib.rs`: `evaluate(expression, input)`, returning the outcome as JSON.
- `src/tokens.rs`: the tokens to highlight, positioned in UTF-16 as JavaScript
  indexes strings. cel 0.15's lexer isn't public, so this mirrors its rules
  (`CEL.g4`); a test checks it against the positions of what cel parses. The
  page draws them in a `<pre>` behind the expression's transparent textarea.
- `src/ast.rs`: the parsed tree for the explorer. cel positions each node at
  one token; a node whose token doesn't spell it (an identifier its name, an
  operator its symbol, a call its name before a `(`) was made by the macro
  whose call's `(` it sits on, or by the parser.
- `src/abi.rs`: the raw exports `web/cel.js` calls. No wasm-bindgen: strings
  go through buffers from `alloc`, the outcome comes back at `output_ptr`.
- `web/cel.js`: the JavaScript side of that, shared by the page and the smoke
  test. A panic aborts the wasm instance; the message is reported and the
  next evaluation gets a fresh instance.
- `web/examples.json`: the examples, also evaluated by both test suites.
- `examples/localhost.rs`: the local server, std only; it serves the files
  directly in `web/` and the module, nothing else.

## Deploying

`.github/workflows/pages.yml` tests, builds and deploys `dist/` to GitHub Pages
on every push to `main`. In the repository settings, set Pages' source to
"GitHub Actions". Dependabot opens a PR when cel is released; merging it
deploys the new version.
