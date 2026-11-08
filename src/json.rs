//! What the input highlights: the tokens of its JSON, its keys, and where it
//! isn't the JSON object the expression needs.
//!
//! The tokens follow JSON's grammar strictly, as serde_json parses it: `01` is
//! two numbers, `1.` a number and a stray `.`, and `NaN` or `'s'` aren't
//! tokens. Like the expression's, an unterminated string or a stray character
//! still makes a token, so the rest highlights.

use serde_json::{error::Category, json, Value as Json};

use crate::highlight::Class;
use crate::tokens::{Kind, Token};

/// What to highlight, as JSON, in the shape of the expression's (see
/// `highlight::to_json`): `{"tokens": [[start, end, kind]...], "marks": [[start,
/// end, class]...], "errors": [[start, end]...]}`, in UTF-16 code units. A
/// top-level key is marked a variable, a nested one a field; the error is on
/// the token serde_json fails at, on the first one if the input isn't an
/// object, or at the end (`start == end`) if the input ends early.
pub fn to_json(input: &str) -> String {
    let tokens = tokens(input);
    let marks = keys(input, &tokens);
    let errors = error(input, &tokens);
    json!({
        "tokens": tokens.iter().map(|t| json!([t.start, t.end, t.kind.name()])).collect::<Vec<_>>(),
        "marks": marks
            .iter()
            .map(|(t, class)| json!([t.start, t.end, class.name()]))
            .collect::<Vec<_>>(),
        "errors": errors.iter().map(|(start, end)| json!([start, end])).collect::<Vec<_>>(),
    })
    .to_string()
}

/// The tokens of `input` but whitespace, in order.
fn tokens(input: &str) -> Vec<Token> {
    let chars: Vec<char> = input.chars().collect();
    let mut utf16 = Vec::with_capacity(chars.len() + 1);
    utf16.push(0);
    for c in &chars {
        utf16.push(utf16.last().unwrap() + c.len_utf16());
    }

    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let (kind, end) = token(&chars, i);
        if let Some(kind) = kind {
            tokens.push(Token {
                start: utf16[i],
                end: utf16[end],
                kind,
            });
        }
        i = end;
    }
    tokens
}

/// The token at `i`, `None` for whitespace, and the index it ends at.
fn token(chars: &[char], i: usize) -> (Option<Kind>, usize) {
    let is_digit = |j: usize| chars.get(j).is_some_and(char::is_ascii_digit);
    let digits_from = |j: usize| {
        (j..chars.len())
            .find(|&k| !is_digit(k))
            .unwrap_or(chars.len())
    };

    match chars[i] {
        ' ' | '\t' | '\n' | '\r' => {
            let end = (i..chars.len())
                .find(|&j| !matches!(chars[j], ' ' | '\t' | '\n' | '\r'))
                .unwrap_or(chars.len());
            (None, end)
        }
        '{' | '}' | '[' | ']' | ':' | ',' => (Some(Kind::Punctuation), i + 1),
        // Up to the closing quote; unterminated, up to the end of the line,
        // which a string can't span
        '"' => {
            let mut j = i + 1;
            loop {
                match chars.get(j) {
                    Some('"') => return (Some(Kind::String), j + 1),
                    Some('\\') if chars.get(j + 1).is_some_and(|&c| c != '\n') => j += 2,
                    Some('\n') | None => return (Some(Kind::Invalid), j),
                    Some(_) => j += 1,
                }
            }
        }
        // -?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?
        '-' | '0'..='9' => {
            let first = if chars[i] == '-' { i + 1 } else { i };
            let mut end = match chars.get(first) {
                Some('0') => first + 1,
                Some('1'..='9') => digits_from(first),
                _ => return (Some(Kind::Invalid), i + 1),
            };
            if chars.get(end) == Some(&'.') && is_digit(end + 1) {
                end = digits_from(end + 1);
            }
            if matches!(chars.get(end), Some('e' | 'E')) {
                let sign = usize::from(matches!(chars.get(end + 1), Some('+' | '-')));
                if is_digit(end + 1 + sign) {
                    end = digits_from(end + 1 + sign);
                }
            }
            (Some(Kind::Number), end)
        }
        // A word is a token, if not a valid one: `True` is wrong as a whole
        'a'..='z' | 'A'..='Z' | '_' => {
            let end = (i..chars.len())
                .find(|&j| !(chars[j].is_ascii_alphanumeric() || chars[j] == '_'))
                .unwrap_or(chars.len());
            let word: String = chars[i..end].iter().collect();
            let kind = match word.as_str() {
                "true" | "false" | "null" => Kind::Constant,
                _ => Kind::Invalid,
            };
            (Some(kind), end)
        }
        _ => (Some(Kind::Invalid), i + 1),
    }
}

/// Each key, a string before a `:` in an object, as a variable if it's one of
/// the top-level object's, as a field if nested. By place only, so they keep
/// their colour while the input doesn't parse.
fn keys(input: &str, tokens: &[Token]) -> Vec<(Token, Class)> {
    let units: Vec<u16> = input.encode_utf16().collect();
    let text = |t: &Token| String::from_utf16_lossy(&units[t.start..t.end]);
    let mut open = Vec::new();
    let mut keys = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        match (token.kind, text(token).as_str()) {
            (Kind::Punctuation, "{" | "[") => open.push(text(token)),
            (Kind::Punctuation, "}" | "]") => {
                open.pop();
            }
            (Kind::String, _)
                if open.last().is_some_and(|o| o == "{")
                    && tokens.get(index + 1).is_some_and(|next| text(next) == ":") =>
            {
                let class = if open.len() == 1 {
                    Class::Variable
                } else {
                    Class::Field
                };
                keys.push((*token, class));
            }
            _ => {}
        }
    }
    keys
}

/// Where the input fails to be what `input::variables` takes: blank, or a
/// JSON object.
fn error(input: &str, tokens: &[Token]) -> Option<(usize, usize)> {
    if input.trim().is_empty() {
        return None;
    }
    let error = match serde_json::from_str::<Json>(input) {
        Ok(Json::Object(_)) => return None,
        Ok(_) => return tokens.first().map(|t| (t.start, t.end)),
        Err(error) => error,
    };
    let len = input.encode_utf16().count();
    if error.classify() == Category::Eof {
        return Some((len, len));
    }
    // serde_json counts the column in bytes, up to and including the one it
    // fails at; at column 0, it fails at the line's start
    let line_start: usize = input
        .split_inclusive('\n')
        .take(error.line().saturating_sub(1))
        .map(str::len)
        .sum();
    let mut byte = (line_start + error.column())
        .saturating_sub(1)
        .min(input.len());
    while !input.is_char_boundary(byte) {
        byte -= 1;
    }
    let at = input[..byte].encode_utf16().count();
    // serde_json can't read past a token that isn't JSON's, but may fail just
    // after it (at the `}` of `tru}`): the first such token, if it's not past
    // there. Else the token it's in; between tokens, the one before.
    let token = tokens
        .iter()
        .find(|t| t.kind == Kind::Invalid && t.start <= at)
        .or_else(|| tokens.iter().find(|t| t.start <= at && at < t.end))
        .or_else(|| tokens.iter().rev().find(|t| t.end <= at));
    Some(match token {
        Some(t) => (t.start, t.end),
        None => (at, (at + 1).min(len)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Highlighted {
        units: Vec<u16>,
        result: Json,
    }

    fn highlight(input: &str) -> Highlighted {
        Highlighted {
            units: input.encode_utf16().collect(),
            result: serde_json::from_str(&to_json(input)).unwrap(),
        }
    }

    impl Highlighted {
        fn text(&self, start: &Json, end: &Json) -> String {
            let (start, end) = (
                start.as_u64().unwrap() as usize,
                end.as_u64().unwrap() as usize,
            );
            String::from_utf16(&self.units[start..end]).unwrap()
        }

        /// Each entry of `field` as its text, then its kind or class, if it
        /// has one.
        fn list(&self, field: &str) -> Vec<String> {
            self.result[field]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| match e.get(2) {
                    Some(name) => format!("{}:{}", self.text(&e[0], &e[1]), name.as_str().unwrap()),
                    None => self.text(&e[0], &e[1]),
                })
                .collect()
        }

        fn errors_at(&self) -> Vec<(u64, u64)> {
            self.result["errors"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| (e[0].as_u64().unwrap(), e[1].as_u64().unwrap()))
                .collect()
        }
    }

    #[test]
    fn every_kind() {
        assert_eq!(
            highlight(r#"{"a": [1, -2.5e+3, true, null, "s\"\\"]}"#).list("tokens"),
            [
                "{:punctuation",
                r#""a":string"#,
                "::punctuation",
                "[:punctuation",
                "1:number",
                ",:punctuation",
                "-2.5e+3:number",
                ",:punctuation",
                "true:constant",
                ",:punctuation",
                "null:constant",
                ",:punctuation",
                r#""s\"\\":string"#,
                "]:punctuation",
                "}:punctuation",
            ]
        );
    }

    #[test]
    fn strictly_jsons_tokens() {
        let tokens = |input| highlight(input).list("tokens");
        assert_eq!(tokens("01"), ["0:number", "1:number"]);
        assert_eq!(tokens("1."), ["1:number", ".:invalid"]);
        assert_eq!(tokens("1e"), ["1:number", "e:invalid"]);
        assert_eq!(tokens("-"), ["-:invalid"]);
        assert_eq!(tokens("+1"), ["+:invalid", "1:number"]);
        assert_eq!(tokens("NaN True"), ["NaN:invalid", "True:invalid"]);
        assert_eq!(tokens("'s'"), ["':invalid", "s:invalid", "':invalid"]);
        assert_eq!(tokens("// c"), ["/:invalid", "/:invalid", "c:invalid"]);
        // Unterminated: up to the end of the line
        assert_eq!(tokens("\"ab\n1"), ["\"ab:invalid", "1:number"]);
        assert_eq!(tokens("\"a\\"), ["\"a\\:invalid"]);
    }

    #[test]
    fn tokens_cover_all_but_whitespace() {
        for input in [
            "",
            "\"",
            "\"\\",
            "\"\\\n",
            "-",
            "1e+",
            "{\"a\": [1, 2.5]}\r\n",
            "é 👋 \u{0}",
        ] {
            let units: Vec<u16> = input.encode_utf16().collect();
            let mut at = 0;
            for t in tokens(input) {
                assert!(at <= t.start && t.start < t.end, "{input:?}: {t:?}");
                let gap = String::from_utf16(&units[at..t.start]).unwrap();
                assert!(
                    gap.chars().all(|c| " \t\n\r".contains(c)),
                    "{input:?}: {gap:?}"
                );
                at = t.end;
            }
            let rest = String::from_utf16(&units[at..]).unwrap();
            assert!(rest.chars().all(|c| " \t\n\r".contains(c)), "{input:?}");
        }
    }

    #[test]
    fn top_level_keys_are_variables() {
        assert_eq!(
            highlight(r#"{"a": {"b": [{"c": "d"}]}, "e": "f:"}"#).list("marks"),
            [
                r#""a":variable"#,
                r#""b":field"#,
                r#""c":field"#,
                r#""e":variable"#
            ]
        );
        // In what doesn't parse yet, too
        assert_eq!(
            highlight(r#"{"a": {"b": "#).list("marks"),
            [r#""a":variable"#, r#""b":field"#]
        );
        // Not an object, so no variables
        assert_eq!(highlight(r#"[{"a": 1}]"#).list("marks"), [r#""a":field"#]);
    }

    #[test]
    fn errors_are_at_their_token() {
        let errors = |input| highlight(input).list("errors");
        assert_eq!(errors(r#"{"a" 1}"#), ["1"]);
        assert_eq!(errors(r#"{"a": 1,}"#), ["}"]);
        assert_eq!(errors(r#"{"a": tru}"#), ["tru"]);
        assert_eq!(errors(r#"{"a": 1.}"#), ["."]);
        assert_eq!(errors(r#"{"a": -}"#), ["-"]);
        assert_eq!(errors(r#"{"a": 01}"#), ["1"]);
        assert_eq!(errors("{\"a\": 1,\n \"b\": x}"), ["x"]);
        assert_eq!(errors(r#"{"a": "\q"}"#), [r#""\q""#]);
        // A string can't span lines: the line it starts on
        assert_eq!(errors("{\"a\": \"b\n\"}"), ["\"b"]);
        assert_eq!(errors("{\"a\": 1} 2"), ["2"]);
        // Not an object: its first token
        assert_eq!(errors("[1]"), ["["]);
        assert_eq!(errors(" 1"), ["1"]);
        for valid in ["", " \n", "{}", r#"{"a": [1, {"b": null}]}"#] {
            assert_eq!(errors(valid), Vec::<String>::new(), "{valid:?}");
        }
    }

    #[test]
    fn errors_at_the_end_are_after_it() {
        for input in ["{", r#"{"a": 1"#, r#"{"a": "b"#, "{\"a\": [\n"] {
            let len = input.encode_utf16().count() as u64;
            assert_eq!(highlight(input).errors_at(), [(len, len)], "{input:?}");
        }
    }

    #[test]
    fn errors_are_in_utf16() {
        let h = highlight(r#"{"é👋": "👋" x}"#);
        assert_eq!(h.list("errors"), ["x"]);
        assert_eq!(h.errors_at(), [(13, 14)]);
        assert_eq!(highlight("{\"é\": 1,\n\"👋\" 2}").list("errors"), ["2"]);
    }

    #[test]
    fn errors_when_and_only_when_the_input_is_refused() {
        for input in [
            "",
            "{}",
            "[]",
            "null",
            "{",
            r#"{"a": 1,}"#,
            r#"{"a": 1, "a": 2}"#,
            "{\"a\": \"\u{1}\"}",
            &format!("{}{}", "[".repeat(200), "]".repeat(200)),
            &format!(r#"{{"a": {}{}}}"#, "[".repeat(200), "]".repeat(200)),
        ] {
            assert_eq!(
                highlight(input).list("errors").is_empty(),
                crate::input::variables(input).is_ok(),
                "{input:?}"
            );
        }
    }

    #[test]
    fn examples() {
        let examples: Json = serde_json::from_str(include_str!("../web/examples.json")).unwrap();
        for example in examples.as_array().unwrap() {
            let input = serde_json::to_string_pretty(&example["input"]).unwrap();
            let h = highlight(&input);
            assert_eq!(h.errors_at(), [], "{input}");
            assert!(
                !h.list("tokens").iter().any(|t| t.ends_with(":invalid")),
                "{input}"
            );
        }
    }
}
