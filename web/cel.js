// The boundary with cel_playground.wasm (see src/abi.rs), for the page and
// for tests/smoke.test.mjs alike: strings in, an outcome object out.
//
// A trap (a panic, as cel-rust aborts on wasm32) leaves the instance unusable:
// it is dropped, the outcome reports it, and the next call gets a fresh
// instance of the same compiled module.

const encoder = new TextEncoder();
const decoder = new TextDecoder();

/**
 * @param {WebAssembly.Module} module the compiled cel_playground.wasm
 * @returns {Promise<{version: string, commit: string, evaluate(expression: string, input: string): Promise<object>}>}
 */
export async function evaluator(module) {
  let instance = null;
  let instantiating = null;
  let panic = null;

  const read = (ptr, len) =>
    decoder.decode(new Uint8Array(instance.exports.memory.buffer, ptr, len));

  function ready() {
    if (instance) return Promise.resolve(instance);
    instantiating ??= WebAssembly.instantiate(module, {
      playground: {
        panicked(ptr, len) {
          panic = read(ptr, len);
        },
      },
    }).then((created) => {
      instance = created;
      instantiating = null;
      return created;
    });
    return instantiating;
  }

  function write(text) {
    const bytes = encoder.encode(text);
    const ptr = instance.exports.alloc(bytes.length);
    new Uint8Array(instance.exports.memory.buffer, ptr, bytes.length).set(bytes);
    return [ptr, bytes.length];
  }

  /** Calls `name` with `texts`; the string it returns, or `{trap}`. */
  async function call(name, ...texts) {
    const current = await ready();
    panic = null;
    const buffers = [];
    try {
      for (const text of texts) buffers.push(write(text));
      const len = current.exports[name](...buffers.flat());
      return { text: read(current.exports.output_ptr(), len) };
    } catch (error) {
      if (!(error instanceof WebAssembly.RuntimeError)) throw error;
      instance = null;
      return { trap: panic ?? `${error.message} (no panic message)`, panicked: panic !== null };
    } finally {
      if (instance === current) {
        for (const [ptr, len] of buffers) current.exports.dealloc(ptr, len);
      }
    }
  }

  const version = (await call("cel_version")).text ?? "unknown";
  const commit = (await call("playground_commit")).text ?? "unknown";

  async function outcome(name, ...texts) {
    const result = await call(name, ...texts);
    if (result.trap !== undefined) {
      return { ok: false, stage: result.panicked ? "panic" : "trap", error: result.trap };
    }
    return JSON.parse(result.text);
  }

  return {
    version,
    /** The playground commit the module was built from, or "unknown". */
    commit,
    evaluate: (expression, input) => outcome("evaluate", expression, input),
    /** For tests: a panic, as the outcome of evaluating would report it. */
    panicForTests: () => outcome("panic_for_tests"),
  };
}
