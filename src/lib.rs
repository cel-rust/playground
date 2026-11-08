//! The CEL playground: cel-rust compiled to WebAssembly, evaluating an
//! expression against variables given as a JSON object.
//!
//! [`evaluate`] does the work; [`abi`] exposes it to JavaScript as raw
//! exports, strings in and a JSON outcome out (see `web/cel.js`).

mod abi;
mod ast;
mod highlight;
mod input;
mod json;
mod literal;
mod tokens;

use std::sync::Arc;

use cel::{extensions, Context, Env, Value};
use serde_json::json;

/// The version of `cel` this build runs, as resolved in `Cargo.lock`.
pub const CEL_VERSION: &str = env!("CEL_VERSION");

/// The playground commit this was built from, or `unknown`.
pub const PLAYGROUND_COMMIT: &str = env!("PLAYGROUND_COMMIT");

thread_local! {
    /// Built once: the standard library, with optionals, and every extension.
    static ENV: Arc<Env> = Arc::new(environment());
}

fn environment() -> Env {
    let mut env = Env::stdlib();
    for extension in [
        extensions::strings,
        extensions::lists,
        extensions::math,
        extensions::encoders,
    ] {
        env.add_extension(extension)
            .expect("the extensions declare distinct overloads");
    }
    env
}

/// Evaluates `expression` with the top-level keys of the JSON object `input`
/// as its variables, and returns the outcome as JSON: either
/// `{"ok": true, "value": "<the result, as a CEL literal>"}` or
/// `{"ok": false, "stage": "parse" | "input" | "evaluation", "error": "<message>"}`.
pub fn evaluate(expression: &str, input: &str) -> String {
    match run(expression, input) {
        Ok(value) => json!({ "ok": true, "value": literal::render(&value) }),
        Err((stage, error)) => json!({ "ok": false, "stage": stage, "error": error }),
    }
    .to_string()
}

fn run(expression: &str, input: &str) -> Result<Value, (&'static str, String)> {
    let env = ENV.with(Arc::clone);
    let program = env
        .compile(expression)
        .map_err(|error| ("parse", error.to_string()))?;
    let variables = input::variables(input).map_err(|error| ("input", error))?;
    let mut context = Context::with_env(env);
    for (name, value) in variables {
        context.add_variable_from_value(name, value);
    }
    program
        .execute(&context)
        .map_err(|error| ("evaluation", error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value as Json;

    fn outcome(expression: &str, input: &str) -> Json {
        serde_json::from_str(&evaluate(expression, input)).unwrap()
    }

    fn value(expression: &str, input: &str) -> String {
        let outcome = outcome(expression, input);
        assert_eq!(outcome["ok"], true, "{expression}: {outcome}");
        outcome["value"].as_str().unwrap().to_owned()
    }

    fn failure(expression: &str, input: &str) -> (String, String) {
        let outcome = outcome(expression, input);
        assert_eq!(outcome["ok"], false, "{expression}: {outcome}");
        (
            outcome["stage"].as_str().unwrap().to_owned(),
            outcome["error"].as_str().unwrap().to_owned(),
        )
    }

    #[test]
    fn evaluates_against_the_inputs_keys() {
        assert_eq!(value("a + b", r#"{"a": 1, "b": 2}"#), "3");
        assert_eq!(value("1 + 1", ""), "2");
        assert_eq!(value("1 + 1", " \n "), "2");
    }

    #[test]
    fn reports_each_stage() {
        assert_eq!(failure("1 +", "{}").0, "parse");
        assert_eq!(failure("1", "[1]").0, "input");
        assert_eq!(failure("1", "{").0, "input");
        assert_eq!(failure("missing", "{}").0, "evaluation");
        // The expression is checked before the input
        assert_eq!(failure("1 +", "[1]").0, "parse");
    }

    #[test]
    fn enables_the_extensions() {
        assert_eq!(value("'TacoCat'.lowerAscii()", ""), r#""tacocat""#);
        assert_eq!(value("[3, 1, 2].sort()", ""), "[1, 2, 3]");
        assert_eq!(value("math.greatest(1, 5, 3)", ""), "5");
        assert_eq!(value("base64.encode(b'hi')", ""), r#""aGk=""#);
        assert_eq!(value("optional.of(1).orValue(2)", ""), "1");
    }

    #[test]
    fn examples_evaluate() {
        let examples: Json = serde_json::from_str(include_str!("../web/examples.json")).unwrap();
        for example in examples.as_array().unwrap() {
            let expression = example["expression"].as_str().unwrap();
            let input = serde_json::to_string_pretty(&example["input"]).unwrap();
            value(expression, &input);
        }
    }
}
