//! The JSON input, as the variables of the expression.

use std::collections::HashMap;
use std::sync::Arc;

use cel::objects::{Key, Map};
use cel::Value;
use serde_json::Value as Json;

/// The top-level keys of the JSON object `input`, with their values. Blank
/// input declares no variables.
pub fn variables(input: &str) -> Result<Vec<(String, Value)>, String> {
    if input.trim().is_empty() {
        return Ok(Vec::new());
    }
    match serde_json::from_str(input) {
        Ok(Json::Object(object)) => Ok(object.into_iter().map(|(k, v)| (k, value(v))).collect()),
        Ok(other) => Err(format!(
            "the input must be a JSON object, whose keys are the variables, not {}",
            kind(&other)
        )),
        Err(error) => Err(format!("invalid JSON: {error}")),
    }
}

/// A JSON value as CEL's: an integral number is an `int`, or a `uint` past
/// `i64::MAX`, and any other number a `double`. `1` is an `int`, `1.0` a
/// `double`, as in CEL itself.
fn value(json: Json) -> Value {
    match json {
        Json::Null => Value::Null,
        Json::Bool(b) => Value::Bool(b),
        Json::Number(n) => match (n.as_i64(), n.as_u64()) {
            (Some(i), _) => Value::Int(i),
            (None, Some(u)) => Value::UInt(u),
            (None, None) => Value::Float(n.as_f64().unwrap_or(f64::NAN)),
        },
        Json::String(s) => Value::String(Arc::new(s)),
        Json::Array(items) => Value::List(Arc::new(items.into_iter().map(value).collect())),
        Json::Object(object) => Value::Map(Map {
            map: Arc::new(
                object
                    .into_iter()
                    .map(|(k, v)| (Key::String(Arc::new(k)), value(v)))
                    .collect::<HashMap<_, _>>(),
            ),
        }),
    }
}

fn kind(json: &Json) -> &'static str {
    match json {
        Json::Null => "null",
        Json::Bool(_) => "a boolean",
        Json::Number(_) => "a number",
        Json::String(_) => "a string",
        Json::Array(_) => "an array",
        Json::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variable(json: &str) -> Value {
        let mut variables = variables(&format!(r#"{{"v": {json}}}"#)).unwrap();
        variables.pop().unwrap().1
    }

    #[test]
    fn numbers() {
        assert_eq!(variable("1"), Value::Int(1));
        assert_eq!(variable("-1"), Value::Int(-1));
        assert_eq!(variable("9223372036854775807"), Value::Int(i64::MAX));
        assert_eq!(variable("9223372036854775808"), Value::UInt(1 << 63));
        assert_eq!(variable("1.0"), Value::Float(1.0));
        assert_eq!(variable("1e3"), Value::Float(1000.0));
    }

    #[test]
    fn nested() {
        let Value::Map(map) = variable(r#"{"a": [true, null, "s"]}"#) else {
            panic!("not a map");
        };
        assert_eq!(
            map.map.get(&Key::String(Arc::new("a".into()))),
            Some(&Value::List(Arc::new(vec![
                Value::Bool(true),
                Value::Null,
                Value::String(Arc::new("s".into()))
            ])))
        );
    }

    #[test]
    fn only_objects() {
        assert!(variables("[]").unwrap_err().contains("not an array"));
        assert!(variables("1").unwrap_err().contains("not a number"));
        assert!(variables("{").unwrap_err().starts_with("invalid JSON"));
    }
}
