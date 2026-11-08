//! The tokens of an expression, for highlighting it as it's typed, parsable
//! or not.
//!
//! It mirrors the lexer rules of the grammar cel 0.15 parses with
//! (`src/parser/gen/CEL.g4`), whose lexer isn't public: longest match first,
//! so `.5` is a number but `1.` is a number then a dot, and `0X1` is `0` then
//! the identifier `X1`. Where that lexer would fail, on an unterminated string
//! or a stray character, it still returns a token, so the rest highlights.

/// What a token is, as the page styles it: `tok-<name>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Comment,
    String,
    Bytes,
    Number,
    /// `true`, `false` and `null`.
    Constant,
    /// `in`.
    Keyword,
    Identifier,
    Operator,
    Punctuation,
    /// A character no token starts with.
    Invalid,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Comment => "comment",
            Kind::String => "string",
            Kind::Bytes => "bytes",
            Kind::Number => "number",
            Kind::Constant => "constant",
            Kind::Keyword => "keyword",
            Kind::Identifier => "identifier",
            Kind::Operator => "operator",
            Kind::Punctuation => "punctuation",
            Kind::Invalid => "invalid",
        }
    }
}

/// A token, from `start` up to `end`, in UTF-16 code units, as JavaScript
/// indexes strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    pub start: usize,
    pub end: usize,
    pub kind: Kind,
}

/// The tokens of `source` but whitespace, in order.
pub fn tokens(source: &str) -> Vec<Token> {
    let chars: Vec<char> = source.chars().collect();
    let mut utf16 = Vec::with_capacity(chars.len() + 1);
    utf16.push(0);
    for c in &chars {
        utf16.push(utf16.last().unwrap() + c.len_utf16());
    }

    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let start = i;
        let (kind, end) = token(&chars, i);
        i = end;
        if let Some(kind) = kind {
            tokens.push(Token {
                start: utf16[start],
                end: utf16[end],
                kind,
            });
        }
    }
    tokens
}

/// The token at `i`, `None` for whitespace, and the index it ends at.
fn token(chars: &[char], i: usize) -> (Option<Kind>, usize) {
    let at = |j: usize| chars.get(j).copied();
    let is_quote = |c: Option<char>| matches!(c, Some('"' | '\''));
    let one = |kind| (Some(kind), i + 1);
    let two = |kind| (Some(kind), i + 2);

    match chars[i] {
        '\t' | '\n' | '\r' | ' ' | '\u{c}' => {
            let end = (i..chars.len())
                .find(|&j| !matches!(chars[j], '\t' | '\n' | '\r' | ' ' | '\u{c}'))
                .unwrap_or(chars.len());
            (None, end)
        }
        '/' if at(i + 1) == Some('/') => {
            let end = (i..chars.len())
                .find(|&j| chars[j] == '\n')
                .unwrap_or(chars.len());
            (Some(Kind::Comment), end)
        }
        '"' | '\'' => (Some(Kind::String), string_end(chars, i, false)),
        'r' | 'R' if is_quote(at(i + 1)) => (Some(Kind::String), string_end(chars, i + 1, true)),
        'b' | 'B' if is_quote(at(i + 1)) => (Some(Kind::Bytes), string_end(chars, i + 1, false)),
        'b' | 'B' if matches!(at(i + 1), Some('r' | 'R')) && is_quote(at(i + 2)) => {
            (Some(Kind::Bytes), string_end(chars, i + 2, true))
        }
        'a'..='z' | 'A'..='Z' | '_' => {
            let end = (i..chars.len())
                .find(|&j| !(chars[j].is_ascii_alphanumeric() || chars[j] == '_'))
                .unwrap_or(chars.len());
            let word: String = chars[i..end].iter().collect();
            let kind = match word.as_str() {
                "true" | "false" | "null" => Kind::Constant,
                "in" => Kind::Keyword,
                _ => Kind::Identifier,
            };
            (Some(kind), end)
        }
        '`' => {
            let quoted = |c: char| c.is_ascii_alphanumeric() || "_.-/ ".contains(c);
            let end = (i + 1..chars.len())
                .find(|&j| !quoted(chars[j]))
                .unwrap_or(chars.len());
            if end > i + 1 && at(end) == Some('`') {
                (Some(Kind::Identifier), end + 1)
            } else {
                one(Kind::Invalid)
            }
        }
        '0'..='9' => (Some(Kind::Number), number_end(chars, i)),
        '.' if at(i + 1).is_some_and(|c| c.is_ascii_digit()) => {
            (Some(Kind::Number), number_end(chars, i))
        }
        '=' if at(i + 1) == Some('=') => two(Kind::Operator),
        '!' | '<' | '>' if at(i + 1) == Some('=') => two(Kind::Operator),
        '&' if at(i + 1) == Some('&') => two(Kind::Operator),
        '|' if at(i + 1) == Some('|') => two(Kind::Operator),
        '!' | '<' | '>' | '+' | '-' | '*' | '/' | '%' | '?' | ':' => one(Kind::Operator),
        '(' | ')' | '[' | ']' | '{' | '}' | '.' | ',' => one(Kind::Punctuation),
        _ => one(Kind::Invalid),
    }
}

/// Where the string whose opening quote is at `i` ends: after its closing
/// quote, or, unterminated, at the end of its line (the end of the source for
/// a triple-quoted one). Raw strings have no escapes.
fn string_end(chars: &[char], i: usize, raw: bool) -> usize {
    let quote = chars[i];
    let is_quote = |j: usize| chars.get(j) == Some(&quote);
    let triple = is_quote(i + 1) && is_quote(i + 2);
    let mut j = if triple { i + 3 } else { i + 1 };
    while j < chars.len() {
        match chars[j] {
            '\\' if !raw => {
                // The escaped character, unless the line ends there
                j += 1;
                if triple || !matches!(chars.get(j), Some('\n' | '\r')) {
                    j += 1;
                }
            }
            '\n' | '\r' if !triple => return j,
            c if c == quote && (!triple || (is_quote(j + 1) && is_quote(j + 2))) => {
                return j + if triple { 3 } else { 1 };
            }
            _ => j += 1,
        }
    }
    chars.len()
}

/// Where the number at `i` ends, as the longest of `NUM_FLOAT`, `NUM_INT`
/// and `NUM_UINT` matches.
fn number_end(chars: &[char], i: usize) -> usize {
    let at = |j: usize| chars.get(j).copied();
    let digits_from = |j: usize, digit: fn(&char) -> bool| {
        (j..chars.len())
            .find(|&k| !digit(&chars[k]))
            .unwrap_or(chars.len())
    };
    let unsigned = |j: usize| j + usize::from(matches!(at(j), Some('u' | 'U')));

    if at(i) == Some('0')
        && at(i + 1) == Some('x')
        && at(i + 2).is_some_and(|c| c.is_ascii_hexdigit())
    {
        return unsigned(digits_from(i + 2, char::is_ascii_hexdigit));
    }
    let mut j = digits_from(i, char::is_ascii_digit);
    let mut float = false;
    if at(j) == Some('.') && at(j + 1).is_some_and(|c| c.is_ascii_digit()) {
        j = digits_from(j + 1, char::is_ascii_digit);
        float = true;
    }
    if matches!(at(j), Some('e' | 'E')) {
        let sign = usize::from(matches!(at(j + 1), Some('+' | '-')));
        if at(j + 1 + sign).is_some_and(|c| c.is_ascii_digit()) {
            j = digits_from(j + 1 + sign, char::is_ascii_digit);
            float = true;
        }
    }
    if float {
        j
    } else {
        unsigned(j)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each token as its text and kind name.
    fn lexed(source: &str) -> Vec<(String, &'static str)> {
        let units: Vec<u16> = source.encode_utf16().collect();
        tokens(source)
            .into_iter()
            .map(|t| {
                let text = String::from_utf16(&units[t.start..t.end]).unwrap();
                (text, t.kind.name())
            })
            .collect()
    }

    fn kinds(source: &str) -> Vec<&'static str> {
        lexed(source).into_iter().map(|(_, kind)| kind).collect()
    }

    fn texts(source: &str) -> Vec<String> {
        lexed(source).into_iter().map(|(text, _)| text).collect()
    }

    #[test]
    fn every_kind() {
        assert_eq!(
            lexed("x in [1, 'a', b'b'] && !true // c"),
            [
                ("x", "identifier"),
                ("in", "keyword"),
                ("[", "punctuation"),
                ("1", "number"),
                (",", "punctuation"),
                ("'a'", "string"),
                (",", "punctuation"),
                ("b'b'", "bytes"),
                ("]", "punctuation"),
                ("&&", "operator"),
                ("!", "operator"),
                ("true", "constant"),
                ("// c", "comment"),
            ]
            .map(|(text, kind)| (text.to_owned(), kind))
        );
    }

    #[test]
    fn positions_are_utf16() {
        let tokens = tokens("'é👋' + x");
        // ' é 👋(2 units) ' then + at 6, x at 8
        assert_eq!(
            tokens.iter().map(|t| (t.start, t.end)).collect::<Vec<_>>(),
            [(0, 5), (6, 7), (8, 9)]
        );
    }

    #[test]
    fn operators() {
        assert_eq!(
            texts("a==b!=c<=d>=e<f>g&&h||!i+-*/%?:"),
            [
                "a", "==", "b", "!=", "c", "<=", "d", ">=", "e", "<", "f", ">", "g", "&&", "h",
                "||", "!", "i", "+", "-", "*", "/", "%", "?", ":"
            ]
        );
        assert_eq!(
            kinds("a.?b"),
            ["identifier", "punctuation", "operator", "identifier"]
        );
        assert_eq!(kinds("a = b"), ["identifier", "invalid", "identifier"]);
        assert_eq!(
            kinds("a & b | c"),
            [
                "identifier",
                "invalid",
                "identifier",
                "invalid",
                "identifier"
            ]
        );
    }

    #[test]
    fn numbers_take_the_longest_match() {
        for number in [
            "1", "1u", "1U", "1.5", ".5", "1e3", "1E+3", "1.5e-3", ".5e1", "0x1F", "0x1fu",
        ] {
            assert_eq!(lexed(number), [(number.to_owned(), "number")], "{number}");
        }
        assert_eq!(texts("1."), ["1", "."]);
        assert_eq!(texts("1.5u"), ["1.5", "u"]);
        assert_eq!(texts("1e"), ["1", "e"]);
        assert_eq!(texts("0x"), ["0", "x"]);
        assert_eq!(texts("0X1"), ["0", "X1"]);
        assert_eq!(texts("a.5"), ["a", ".5"]);
        assert_eq!(texts("-1"), ["-", "1"]);
    }

    #[test]
    fn strings() {
        for string in [
            r#""a\"b""#,
            r"'a\'b'",
            "''",
            r"'\\'",
            "'''one\n'two'\n'''",
            r#""""a"b""""#,
            r"r'\'",
            r#"R"\""#,
            "r'''a\\'''",
            "'é👋'",
        ] {
            assert_eq!(lexed(string), [(string.to_owned(), "string")], "{string}");
        }
        for bytes in [r"b'\xff'", "B''", r"br'\'", r#"BR"""a""""#] {
            assert_eq!(lexed(bytes), [(bytes.to_owned(), "bytes")], "{bytes}");
        }
        // Only `b` then `r` is a prefix
        assert_eq!(kinds("rb'x'"), ["identifier", "string"]);
    }

    #[test]
    fn unterminated_strings_end_with_their_line() {
        assert_eq!(texts("'abc\nx"), ["'abc", "x"]);
        assert_eq!(texts("'abc\\\nx"), ["'abc\\", "x"]);
        assert_eq!(texts("'''abc\nx"), ["'''abc\nx"]);
        assert_eq!(texts("r'abc"), ["r'abc"]);
    }

    #[test]
    fn comments_end_with_their_line() {
        assert_eq!(texts("a // b\nc"), ["a", "// b", "c"]);
        assert_eq!(kinds("a / b"), ["identifier", "operator", "identifier"]);
    }

    #[test]
    fn identifiers() {
        assert_eq!(kinds("_a1 if trueish nullable"), ["identifier"; 4]);
        assert_eq!(
            lexed("a.`b-c.d`"),
            [
                ("a".to_owned(), "identifier"),
                (".".to_owned(), "punctuation"),
                ("`b-c.d`".to_owned(), "identifier"),
            ]
        );
        assert_eq!(kinds("``"), ["invalid", "invalid"]);
        assert_eq!(kinds("`a"), ["invalid", "identifier"]);
        assert_eq!(kinds("é # $"), ["invalid"; 3]);
    }

    /// cel's own lexer isn't public, but the identifiers and literals of what
    /// it parses are positioned on its tokens: each must be one of ours.
    #[test]
    fn matches_cels_lexer_on_identifiers_and_literals() {
        use cel::common::ast::{EntryExpr, Expr, IdedExpr};

        let examples: serde_json::Value =
            serde_json::from_str(include_str!("../web/examples.json")).unwrap();
        let mut sources: Vec<String> = examples
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["expression"].as_str().unwrap().to_owned())
            .collect();
        sources.extend(
            [
                "'é👋' + x",
                "b'\\xff' + 0x1Fu + 1u",
                "1.5e3 + .5 + 2e-1",
                "[r'\\', '''a\n'b'\n''', \"é\", R\"\"\"x\"\"\"]",
                "{'k': true, 'n': null} // done",
                // cel 0.15 lexes `escaped` identifiers but doesn't parse them
                "a.b.c && !false",
                "-1 + -2.5",
            ]
            .map(str::to_owned),
        );

        fn nodes<'e>(e: &'e IdedExpr, out: &mut Vec<&'e IdedExpr>) {
            out.push(e);
            match &e.expr {
                Expr::Call(c) => {
                    c.target.iter().for_each(|t| nodes(t, out));
                    c.args.iter().for_each(|a| nodes(a, out));
                }
                Expr::Comprehension(c) => {
                    for x in [
                        &c.iter_range,
                        &c.accu_init,
                        &c.loop_cond,
                        &c.loop_step,
                        &c.result,
                    ] {
                        nodes(x, out);
                    }
                }
                Expr::List(l) => l.elements.iter().for_each(|x| nodes(x, out)),
                Expr::Select(s) => nodes(&s.operand, out),
                Expr::Map(m) => m.entries.iter().for_each(|en| match &en.expr {
                    EntryExpr::MapEntry(me) => {
                        nodes(&me.key, out);
                        nodes(&me.value, out);
                    }
                    EntryExpr::StructField(f) => nodes(&f.value, out),
                }),
                _ => {}
            }
        }

        let env = crate::ENV.with(std::sync::Arc::clone);
        for source in &sources {
            let program = env.compile(source).unwrap();
            let ours: Vec<_> = tokens(source)
                .into_iter()
                .map(|t| (t.start, t.end, t.kind))
                .collect();
            let mut all = Vec::new();
            nodes(program.expression(), &mut all);
            let mut checked = 0;
            for node in all {
                // cel's positions: UTF-8 byte offsets, the last one included
                let (start, stop) = program.source_info().offset_for(node.id).unwrap();
                let (start, stop) = (start as usize, stop as usize);
                let end = stop + source[stop..].chars().next().unwrap().len_utf8();
                let text = &source[start..end];
                let kind = match &node.expr {
                    Expr::Ident(name) if text == name => Kind::Identifier,
                    // Nodes a macro made are at its `(`, or the `.` of `.?`
                    Expr::Literal(_) if text != "(" && text != "." => match text {
                        "true" | "false" | "null" => Kind::Constant,
                        _ if text.starts_with(['b', 'B']) => Kind::Bytes,
                        _ if text.starts_with(['\'', '"', 'r', 'R']) => Kind::String,
                        _ => Kind::Number,
                    },
                    _ => continue,
                };
                let utf16 = |byte: usize| source[..byte].encode_utf16().count();
                let theirs = (utf16(start), utf16(end), kind);
                assert!(ours.contains(&theirs), "{source:?}: {text:?} at {theirs:?}");
                checked += 1;
            }
            assert!(checked > 0, "{source:?}: nothing checked");
        }
    }

    /// Whatever the source, the tokens are in order, don't overlap, and only
    /// whitespace is left between them.
    #[test]
    fn tokens_cover_all_but_whitespace() {
        let examples: serde_json::Value =
            serde_json::from_str(include_str!("../web/examples.json")).unwrap();
        let mut sources: Vec<String> = examples
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["expression"].as_str().unwrap().to_owned())
            .collect();
        sources.extend(
            [
                "",
                " ",
                "\n",
                "'",
                "\"\"\"",
                "b",
                "br",
                "r",
                "`",
                "0x",
                ".",
                "//",
                "a\r\nb",
                "'\\",
                "'''\\",
                "\u{c}x",
                "🦀 + 'é'",
                "x.`a b`.y",
                "1..2",
                "@x",
            ]
            .map(str::to_owned),
        );
        for source in sources {
            let units: Vec<u16> = source.encode_utf16().collect();
            let mut at = 0;
            for token in tokens(&source) {
                assert!(
                    at <= token.start && token.start < token.end,
                    "{source:?}: {token:?}"
                );
                let gap = String::from_utf16(&units[at..token.start]).unwrap();
                assert!(
                    gap.chars().all(|c| "\t\n\r \u{c}".contains(c)),
                    "{source:?}: {gap:?}"
                );
                at = token.end;
            }
            let rest = String::from_utf16(&units[at..]).unwrap();
            assert!(
                rest.chars().all(|c| "\t\n\r \u{c}".contains(c)),
                "{source:?}: {rest:?}"
            );
        }
    }
}
