//! The tree cel parses an expression into, macros expanded, for the AST
//! explorer: each node with what it is, where cel positions it, and whether
//! that is its own text or the call of the macro that made it.

use std::collections::HashMap;
use std::sync::Arc;

use cel::common::ast::{EntryExpr, Expr, IdedExpr, LiteralValue, SourceInfo};
use cel::Value;
use serde_json::{json, Value as Json};

use crate::tokens::{self, Kind, Token};

/// The tree of `expression` as JSON: `{"ok": true, "root": <node>}`, or
/// `{"ok": false, "error": "<why it doesn't parse>"}`. A node is
/// `{"kind", "detail", "id", "start", "end", "text", "from", "role",
/// "children"}`: `start` and `end` in UTF-16 code units, `from` the macro (or
/// `"parser"`) that made it, `null` when it's text of the expression.
pub fn to_json(expression: &str) -> String {
    let env = crate::ENV.with(Arc::clone);
    match env.compile(expression) {
        Ok(program) => {
            let source = Source::new(expression, program.source_info());
            json!({ "ok": true, "root": source.node(program.expression(), None) })
        }
        Err(errors) => json!({ "ok": false, "error": errors.to_string() }),
    }
    .to_string()
}

struct Source<'a> {
    text: &'a str,
    info: &'a SourceInfo,
    /// UTF-16 offsets, by UTF-8 offset, at char boundaries.
    utf16: Vec<usize>,
    /// The text, in UTF-16 code units.
    units: Vec<u16>,
    tokens: Vec<Token>,
    /// Which token starts at a UTF-16 offset.
    starts: HashMap<usize, usize>,
}

impl<'a> Source<'a> {
    fn new(text: &'a str, info: &'a SourceInfo) -> Self {
        let mut utf16 = vec![0; text.len() + 1];
        let mut units = 0;
        for (byte, c) in text.char_indices() {
            utf16[byte] = units;
            units += c.len_utf16();
        }
        utf16[text.len()] = units;
        let tokens = tokens::tokens(text);
        let starts = tokens
            .iter()
            .enumerate()
            .map(|(i, t)| (t.start, i))
            .collect();
        Source {
            text,
            info,
            utf16,
            units: text.encode_utf16().collect(),
            tokens,
            starts,
        }
    }

    /// The UTF-16 range of node `id`: cel positions nodes at a token, in UTF-8
    /// offsets, its last byte included.
    fn range(&self, id: u64) -> Option<(usize, usize)> {
        let (start, stop) = self.info.offset_for(id)?;
        let (start, stop) = (start as usize, stop as usize);
        let end = (stop + 1..=self.text.len()).find(|&b| self.text.is_char_boundary(b))?;
        self.text
            .is_char_boundary(start)
            .then(|| (self.utf16[start], self.utf16[end]))
    }

    fn token_at(&self, start: usize) -> Option<(usize, &Token)> {
        let index = *self.starts.get(&start)?;
        Some((index, &self.tokens[index]))
    }

    fn token_text(&self, token: &Token) -> String {
        String::from_utf16_lossy(&self.units[token.start..token.end])
    }

    /// Whether the token at `start` is the node's own text, or `None` when it
    /// isn't, then what made the node: the macro whose call's `(` that is,
    /// or the parser.
    fn made_by(&self, expr: &Expr, start: usize) -> Option<String> {
        let Some((index, token)) = self.token_at(start) else {
            return Some("parser".to_owned());
        };
        let text = self.token_text(token);
        let previous = index
            .checked_sub(1)
            .map(|i| &self.tokens[i])
            .filter(|t| t.kind == Kind::Identifier)
            .map(|t| self.token_text(t));
        let own = match expr {
            Expr::Ident(name) => token.kind == Kind::Identifier && &text == name,
            Expr::Literal(_) => matches!(
                token.kind,
                Kind::Number | Kind::String | Kind::Bytes | Kind::Constant
            ),
            Expr::Call(call) => match operator(&call.func_name) {
                Some(symbol) => text == symbol,
                None => text == "(" && previous.as_deref() == Some(call.func_name.as_str()),
            },
            Expr::Select(_) => text == ".",
            Expr::List(_) => text == "[",
            Expr::Map(_) | Expr::Struct(_) => text == "{",
            Expr::Comprehension(_) | Expr::Unspecified => false,
        };
        if own {
            None
        } else if text == "(" {
            Some(previous.unwrap_or_else(|| "parser".to_owned()))
        } else {
            Some("parser".to_owned())
        }
    }

    fn node(&self, e: &IdedExpr, role: Option<&str>) -> Json {
        let (kind, detail, children): (&str, String, Vec<Json>) = match &e.expr {
            Expr::Unspecified => ("Unspecified", String::new(), vec![]),
            Expr::Call(call) => {
                let mut children = Vec::new();
                if let Some(target) = &call.target {
                    children.push(self.node(target, Some("target")));
                }
                children.extend(call.args.iter().map(|a| self.node(a, None)));
                ("Call", call.func_name.clone(), children)
            }
            Expr::Comprehension(c) => {
                let vars = match &c.iter_var2 {
                    Some(second) => format!("{}, {second}", c.iter_var),
                    None => c.iter_var.clone(),
                };
                let children = [
                    ("iter_range", &c.iter_range),
                    ("accu_init", &c.accu_init),
                    ("loop_condition", &c.loop_cond),
                    ("loop_step", &c.loop_step),
                    ("result", &c.result),
                ]
                .into_iter()
                .map(|(role, child)| self.node(child, Some(role)))
                .collect();
                (
                    "Comprehension",
                    format!("{vars} · accu {}", c.accu_var),
                    children,
                )
            }
            Expr::Ident(name) => ("Ident", name.clone(), vec![]),
            Expr::Literal(value) => ("Literal", crate::literal::render(&literal(value)), vec![]),
            Expr::List(list) => {
                let children = list
                    .elements
                    .iter()
                    .enumerate()
                    .map(|(i, element)| {
                        let optional = list.optional_indices.contains(&i);
                        self.node(element, optional.then_some("optional"))
                    })
                    .collect();
                ("List", String::new(), children)
            }
            Expr::Map(map) => {
                let children = map
                    .entries
                    .iter()
                    .map(|e| self.entry(e.id, &e.expr))
                    .collect();
                ("Map", String::new(), children)
            }
            Expr::Struct(s) => {
                let children = s
                    .entries
                    .iter()
                    .map(|e| self.entry(e.id, &e.expr))
                    .collect();
                ("Struct", s.type_name.clone(), children)
            }
            Expr::Select(select) => {
                let detail = if select.test {
                    format!("has .{}", select.field)
                } else {
                    format!(".{}", select.field)
                };
                ("Select", detail, vec![self.node(&select.operand, None)])
            }
        };
        let range = self.range(e.id);
        let from = range.map_or(Some("parser".to_owned()), |(start, _)| {
            self.made_by(&e.expr, start)
        });
        self.json(e.id, kind, detail, range, from, role, children)
    }

    fn entry(&self, id: u64, entry: &EntryExpr) -> Json {
        let range = self.range(id);
        let own = range
            .and_then(|(start, _)| self.token_at(start))
            .is_some_and(|(_, t)| self.token_text(t) == ":");
        let from = (!own).then(|| "parser".to_owned());
        let (kind, detail, children) = match entry {
            EntryExpr::MapEntry(e) => (
                "Entry",
                if e.optional { "?" } else { "" }.to_owned(),
                vec![
                    self.node(&e.key, Some("key")),
                    self.node(&e.value, Some("value")),
                ],
            ),
            EntryExpr::StructField(f) => (
                "Field",
                format!("{}{}", if f.optional { "?" } else { "" }, f.field),
                vec![self.node(&f.value, None)],
            ),
        };
        self.json(id, kind, detail, range, from, None, children)
    }

    #[allow(clippy::too_many_arguments)]
    fn json(
        &self,
        id: u64,
        kind: &str,
        detail: String,
        range: Option<(usize, usize)>,
        from: Option<String>,
        role: Option<&str>,
        children: Vec<Json>,
    ) -> Json {
        let text = range.map(|(start, end)| String::from_utf16_lossy(&self.units[start..end]));
        json!({
            "kind": kind,
            "detail": detail,
            "id": id,
            "start": range.map(|r| r.0),
            "end": range.map(|r| r.1),
            "text": text,
            "from": from,
            "role": role,
            "children": children,
        })
    }
}

/// The symbol an operator's node is positioned at.
fn operator(function: &str) -> Option<&'static str> {
    Some(match function {
        "_+_" => "+",
        "_-_" | "-_" => "-",
        "_*_" => "*",
        "_/_" => "/",
        "_%_" => "%",
        "_==_" => "==",
        "_!=_" => "!=",
        "_<_" => "<",
        "_<=_" => "<=",
        "_>_" => ">",
        "_>=_" => ">=",
        "_&&_" => "&&",
        "_||_" => "||",
        "!_" => "!",
        "_[_]" | "_[?_]" => "[",
        "_?_:_" => "?",
        "@in" => "in",
        "_?._" => ".",
        _ => return None,
    })
}

fn literal(value: &LiteralValue) -> Value {
    match value {
        LiteralValue::Boolean(b) => Value::Bool(*b.inner()),
        LiteralValue::Bytes(b) => Value::Bytes(Arc::new(b.inner().to_vec())),
        LiteralValue::Double(d) => Value::Float(*d.inner()),
        LiteralValue::Int(i) => Value::Int(*i.inner()),
        LiteralValue::Null => Value::Null,
        LiteralValue::String(s) => Value::String(Arc::new(s.inner().to_owned())),
        LiteralValue::UInt(u) => Value::UInt(*u.inner()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(expression: &str) -> Json {
        let result: Json = serde_json::from_str(&to_json(expression)).unwrap();
        assert_eq!(result["ok"], true, "{expression}: {result}");
        result["root"].clone()
    }

    /// Each node, depth first, as `kind detail [text] <from>`.
    fn lines(expression: &str) -> Vec<String> {
        fn walk(node: &Json, out: &mut Vec<String>) {
            let mut line = format!(
                "{} {}",
                node["kind"].as_str().unwrap(),
                node["detail"].as_str().unwrap()
            );
            if let Some(role) = node["role"].as_str() {
                line = format!("{role}: {line}");
            }
            if let Some(text) = node["text"].as_str() {
                line.push_str(&format!(" [{text}]"));
            }
            if let Some(from) = node["from"].as_str() {
                line.push_str(&format!(" <{from}>"));
            }
            out.push(line.trim().replace("  ", " "));
            for child in node["children"].as_array().unwrap() {
                walk(child, out);
            }
        }
        let mut out = Vec::new();
        walk(&tree(expression), &mut out);
        out
    }

    #[test]
    fn nodes_of_the_expression_are_its_own_text() {
        assert_eq!(
            lines("a.b + f(1u, 'é👋') in [x]"),
            [
                "Call @in [in]",
                "Call _+_ [+]",
                "Select .b [.]",
                "Ident a [a]",
                "Call f [(]",
                "Literal 1u [1u]",
                "Literal \"é👋\" ['é👋']",
                "List [[]",
                "Ident x [x]",
            ]
        );
    }

    #[test]
    fn positions_are_utf16() {
        let root = tree("'é👋' + x");
        let x = &root["children"][1];
        assert_eq!((x["start"].as_u64(), x["end"].as_u64()), (Some(8), Some(9)));
    }

    #[test]
    fn what_macros_make_is_theirs() {
        assert_eq!(
            lines("[1].map(x, x + y)"),
            [
                "Comprehension x · accu @result [(] <map>",
                "iter_range: List [[]",
                "Literal 1 [1]",
                "accu_init: List [(] <map>",
                "loop_condition: Literal true [(] <map>",
                "loop_step: Call _+_ [(] <map>",
                "Ident @result [(] <map>",
                "List [(] <map>",
                "Call _+_ [+]",
                "Ident x [x]",
                "Ident y [y]",
                "result: Ident @result [(] <map>",
            ]
        );
        assert_eq!(
            lines("has(a.b)"),
            ["Select has .b [(] <has>", "Ident a [a]"]
        );
        let opt_map = lines("o.optMap(v, v + 1)");
        assert_eq!(opt_map[0], "Call _?_:_ [(] <optMap>");
        assert!(
            opt_map.contains(&"target: Ident o [(] <optMap>".to_owned()),
            "{opt_map:#?}"
        );
        assert!(opt_map.contains(&"Ident v [v]".to_owned()), "{opt_map:#?}");
    }

    #[test]
    fn what_the_parser_makes_is_its() {
        assert_eq!(
            lines("a.?b"),
            ["Call _?._ [.]", "Ident a [a]", "Literal \"b\" [.] <parser>"]
        );
        // Folded into the literal, positioned at its digits
        assert_eq!(lines("-1"), ["Literal -1 [1]"]);
    }

    #[test]
    fn entries_and_optionals() {
        assert_eq!(
            lines("{?'k': [?v, 1]}"),
            [
                "Map [{]",
                "Entry ? [:]",
                "key: Literal \"k\" ['k']",
                "value: List [[]",
                "optional: Ident v [v]",
                "Literal 1 [1]",
            ]
        );
        assert_eq!(
            lines("Msg{f: 1}"),
            ["Struct Msg [{]", "Field f [:]", "Literal 1 [1]"]
        );
    }

    #[test]
    fn parse_errors() {
        let result: Json = serde_json::from_str(&to_json("1 +")).unwrap();
        assert_eq!(result["ok"], false);
        assert!(result["error"].as_str().unwrap().contains("Syntax error"));
    }

    #[test]
    fn examples() {
        let examples: Json = serde_json::from_str(include_str!("../web/examples.json")).unwrap();
        for example in examples.as_array().unwrap() {
            tree(example["expression"].as_str().unwrap());
        }
    }
}
