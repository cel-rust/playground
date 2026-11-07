//! Values as CEL literals: `1`, `1u`, `1.0`, `b"\x00"`, `timestamp("...")`,
//! so the result keeps the types JSON would lose, and reads back as CEL.

use std::fmt::Write;

use cel::objects::{Key, OptionalValue};
use cel::Value;
use chrono::{SecondsFormat, Utc};

/// Lists and maps longer than this, once indented, are broken one element per
/// line.
const WIDTH: usize = 72;

/// `value` as a CEL literal, or as Rust's `Debug` of it for what CEL has no
/// literal for (functions, structs, opaque values other than optionals).
pub fn render(value: &Value) -> String {
    let mut out = String::new();
    pretty(&mut out, value, 0);
    out
}

fn pretty(out: &mut String, value: &Value, indent: usize) {
    let flat = flat(value);
    if indent + flat.len() <= WIDTH {
        out.push_str(&flat);
        return;
    }
    let pad = |out: &mut String, indent: usize| out.extend(std::iter::repeat_n(' ', indent));
    match value {
        Value::List(items) if !items.is_empty() => {
            out.push_str("[\n");
            for item in items.iter() {
                pad(out, indent + 2);
                pretty(out, item, indent + 2);
                out.push_str(",\n");
            }
            pad(out, indent);
            out.push(']');
        }
        Value::Map(map) if !map.map.is_empty() => {
            out.push_str("{\n");
            for (key, value) in sorted(map) {
                pad(out, indent + 2);
                write_key(out, key);
                out.push_str(": ");
                pretty(out, value, indent + 2);
                out.push_str(",\n");
            }
            pad(out, indent);
            out.push('}');
        }
        _ => out.push_str(&flat),
    }
}

fn flat(value: &Value) -> String {
    let mut out = String::new();
    write_flat(&mut out, value);
    out
}

fn write_flat(out: &mut String, value: &Value) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => write!(out, "{b}").unwrap(),
        Value::Int(i) => write!(out, "{i}").unwrap(),
        Value::UInt(u) => write!(out, "{u}u").unwrap(),
        Value::Float(f) => write_double(out, *f),
        Value::String(s) => write_string(out, s),
        Value::Bytes(b) => write_bytes(out, b),
        Value::Duration(d) => {
            out.push_str("duration(\"");
            if *d < chrono::Duration::zero() {
                out.push('-');
            }
            write!(out, "{}", d.num_seconds().unsigned_abs()).unwrap();
            let nanos = d.subsec_nanos().unsigned_abs();
            if nanos > 0 {
                let fraction = format!("{nanos:09}");
                write!(out, ".{}", fraction.trim_end_matches('0')).unwrap();
            }
            out.push_str("s\")");
        }
        Value::Timestamp(t) => write!(
            out,
            "timestamp(\"{}\")",
            t.with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::AutoSi, true)
        )
        .unwrap(),
        Value::List(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_flat(out, item);
            }
            out.push(']');
        }
        Value::Map(map) => {
            out.push('{');
            for (i, (key, value)) in sorted(map).into_iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_key(out, key);
                out.push_str(": ");
                write_flat(out, value);
            }
            out.push('}');
        }
        Value::Opaque(opaque) => match opaque.as_ref().downcast_ref::<OptionalValue>() {
            Some(optional) => match optional.value() {
                Some(value) => {
                    out.push_str("optional.of(");
                    write_flat(out, value);
                    out.push(')');
                }
                None => out.push_str("optional.none()"),
            },
            None => write!(out, "{value:?}").unwrap(),
        },
        Value::Function(..) | Value::Struct(_) => write!(out, "{value:?}").unwrap(),
    }
}

/// A map's entries, in a stable order: maps are hashed.
fn sorted(map: &cel::objects::Map) -> Vec<(&Key, &Value)> {
    let mut entries: Vec<_> = map.map.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    entries
}

fn write_key(out: &mut String, key: &Key) {
    match key {
        Key::Int(i) => write!(out, "{i}").unwrap(),
        Key::Uint(u) => write!(out, "{u}u").unwrap(),
        Key::Bool(b) => write!(out, "{b}").unwrap(),
        Key::String(s) => write_string(out, s),
    }
}

/// `1.0`, `1e300`, `-0.0`: Rust's `Debug` of an `f64` is a CEL double
/// literal, when it is finite.
fn write_double(out: &mut String, f: f64) {
    if f.is_nan() {
        out.push_str(r#"double("NaN")"#);
    } else if f.is_infinite() {
        let sign = if f < 0.0 { "-" } else { "" };
        write!(out, r#"double("{sign}Infinity")"#).unwrap();
    } else {
        write!(out, "{f:?}").unwrap();
    }
}

fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => write!(out, "\\u{:04x}", c as u32).unwrap(),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn write_bytes(out: &mut String, bytes: &[u8]) {
    out.push_str("b\"");
    for &b in bytes {
        match b {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x20..=0x7e => out.push(b as char),
            b => write!(out, "\\x{b:02x}").unwrap(),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use crate::evaluate;

    /// The result of `expression`, rendered.
    fn rendered(expression: &str) -> String {
        let outcome: serde_json::Value = serde_json::from_str(&evaluate(expression, "")).unwrap();
        assert_eq!(outcome["ok"], true, "{expression}: {outcome}");
        outcome["value"].as_str().unwrap().to_owned()
    }

    #[test]
    fn scalars() {
        assert_eq!(rendered("null"), "null");
        assert_eq!(rendered("true"), "true");
        assert_eq!(rendered("-1"), "-1");
        assert_eq!(rendered("1u"), "1u");
        assert_eq!(rendered("1.0"), "1.0");
        assert_eq!(rendered("1e300"), "1e300");
        assert_eq!(rendered("double('NaN')"), r#"double("NaN")"#);
        assert_eq!(rendered("-double('Infinity')"), r#"double("-Infinity")"#);
    }

    #[test]
    fn strings_and_bytes_are_escaped() {
        assert_eq!(rendered(r#"'a"b\\c\n\u0001é'"#), r#""a\"b\\c\n\u0001é""#);
        assert_eq!(rendered(r#"b'a"\x00\xff'"#), r#"b"a\"\x00\xff""#);
    }

    #[test]
    fn time() {
        assert_eq!(
            rendered("timestamp('2026-11-07T10:00:00+02:00')"),
            r#"timestamp("2026-11-07T08:00:00Z")"#
        );
        assert_eq!(
            rendered("timestamp('2026-11-07T08:00:00.25Z')"),
            r#"timestamp("2026-11-07T08:00:00.250Z")"#
        );
        assert_eq!(rendered("duration('1h1.5s')"), r#"duration("3601.5s")"#);
        assert_eq!(rendered("duration('-1.5s')"), r#"duration("-1.5s")"#);
        assert_eq!(rendered("duration('0s')"), r#"duration("0s")"#);
    }

    #[test]
    fn containers() {
        assert_eq!(rendered("[]"), "[]");
        assert_eq!(rendered("{}"), "{}");
        assert_eq!(
            rendered("{'b': [1u, 2.0], 'a': {true: null}, 2: 'x', 1: 'y'}"),
            r#"{1: "y", 2: "x", "a": {true: null}, "b": [1u, 2.0]}"#
        );
    }

    #[test]
    fn optionals() {
        assert_eq!(rendered("optional.of([1])"), "optional.of([1])");
        assert_eq!(rendered("optional.none()"), "optional.none()");
    }

    #[test]
    fn long_containers_are_broken_up() {
        let list = (0..30)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let expected = (0..30).map(|i| format!("    {i},\n")).collect::<String>();
        assert_eq!(
            rendered(&format!("{{'k': [{list}]}}")),
            format!("{{\n  \"k\": [\n{expected}  ],\n}}")
        );
    }
}
