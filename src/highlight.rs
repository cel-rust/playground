//! What the editor highlights: the tokens of the expression, what its parsed
//! tree says of its identifiers, and where it doesn't parse.
//!
//! An identifier the tree has a node for, at its own token, is a variable (in
//! the input, or missing from it), a name a macro binds, a namespace or a type.
//! Others are told by where they are: a function or macro before a `(`, a
//! field after a `.`. While the expression doesn't parse, only the latter.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use cel::common::ast::{EntryExpr, Expr, IdedExpr};
use cel::{Context, Env, ExecutionError, ParseErrors};
use serde_json::{json, Value as Json};

use crate::ast::Source;
use crate::tokens::{self, Kind, Token};

/// What the tree, or the place of a token, says an identifier is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// A variable the input has.
    Variable,
    /// A variable the input doesn't have: evaluating fails on it.
    Missing,
    /// A name a macro binds, where it's declared and where it's used.
    Bound,
    Function,
    Macro,
    Field,
    /// The first part of a namespaced function's name: `base64` in
    /// `base64.encode(b)`.
    Namespace,
    Type,
}

impl Class {
    pub fn name(self) -> &'static str {
        match self {
            Class::Variable => "variable",
            Class::Missing => "missing",
            Class::Bound => "bound",
            Class::Function => "function",
            Class::Macro => "macro",
            Class::Field => "field",
            Class::Namespace => "namespace",
            Class::Type => "type",
        }
    }
}

/// What to highlight, as JSON: `{"tokens": [[start, end, kind]...], "marks":
/// [[start, end, class]...], "errors": [[start, end]...]}`, in UTF-16 code
/// units. A mark is on an identifier token; an error on the token cel reports
/// it at, or at the end (`start == end`).
pub fn to_json(expression: &str, input: &str) -> String {
    let env = crate::ENV.with(Arc::clone);
    let tokens = tokens::tokens(expression);
    let (classes, errors) = match env.compile(expression) {
        Ok(program) => {
            let source = Source::new(expression, program.source_info());
            let mut walk = Walk {
                env: &env,
                source: &source,
                keys: input_keys(input),
                scope: Vec::new(),
                classes: HashMap::new(),
            };
            walk.node(program.expression());
            let mut classes = walk.classes;
            by_place(&source.units, &tokens, true, &mut classes);
            (classes, Vec::new())
        }
        Err(errors) => {
            let units: Vec<u16> = expression.encode_utf16().collect();
            let mut classes = HashMap::new();
            by_place(&units, &tokens, false, &mut classes);
            (classes, error_ranges(expression, &tokens, &errors))
        }
    };

    let mut marks: Vec<_> = classes.into_iter().collect();
    marks.sort_by_key(|(index, _)| *index);
    json!({
        "tokens": tokens.iter().map(|t| json!([t.start, t.end, t.kind.name()])).collect::<Vec<_>>(),
        "marks": marks
            .iter()
            .map(|(index, class)| json!([tokens[*index].start, tokens[*index].end, class.name()]))
            .collect::<Vec<_>>(),
        "errors": errors.iter().map(|(start, end)| json!([start, end])).collect::<Vec<_>>(),
    })
    .to_string()
}

/// The input's top-level keys; `None` when it isn't a JSON object, so nothing
/// can be said to be missing from it. Blank input has none.
fn input_keys(input: &str) -> Option<HashSet<String>> {
    if input.trim().is_empty() {
        return Some(HashSet::new());
    }
    match serde_json::from_str(input) {
        Ok(Json::Object(object)) => Some(object.keys().cloned().collect()),
        _ => None,
    }
}

struct Walk<'a> {
    env: &'a Arc<Env>,
    source: &'a Source<'a>,
    keys: Option<HashSet<String>>,
    /// The names the enclosing comprehensions bind.
    scope: Vec<String>,
    /// By token index.
    classes: HashMap<usize, Class>,
}

impl Walk<'_> {
    /// The token a node is at, its index, and what made the node (`None`: the
    /// node is that token's).
    fn at(&self, e: &IdedExpr) -> Option<(usize, Option<String>)> {
        let (start, _) = self.source.range(e.id)?;
        let (index, _) = self.source.token_at(start)?;
        Some((index, self.source.made_by(&e.expr, start)))
    }

    fn text(&self, index: usize) -> Option<String> {
        self.source
            .tokens
            .get(index)
            .map(|t| self.source.token_text(t))
    }

    fn is_identifier(&self, index: usize) -> bool {
        self.source
            .tokens
            .get(index)
            .is_some_and(|t| t.kind == Kind::Identifier)
    }

    /// A variable, or a type, unless a comprehension binds it.
    fn reference(&self, name: &str) -> Class {
        if self.scope.iter().any(|bound| bound == name) {
            Class::Bound
        } else if self.keys.as_ref().is_some_and(|keys| keys.contains(name)) {
            Class::Variable
        } else if self.env.types().find_type(name).is_some() {
            Class::Type
        } else if self.keys.is_some() {
            Class::Missing
        } else {
            Class::Variable
        }
    }

    fn node(&mut self, e: &IdedExpr) {
        let at = self.at(e);
        // A macro's call: its name before the `(` it made the node at
        if let Some((paren, Some(made_by))) = &at {
            if self.text(*paren).as_deref() == Some("(")
                && paren.checked_sub(1).is_some_and(|name| {
                    self.is_identifier(name) && self.text(name).as_deref() == Some(made_by)
                })
            {
                self.classes.insert(paren - 1, Class::Macro);
            }
        }

        match &e.expr {
            Expr::Ident(name) => match at {
                Some((index, None)) => {
                    let class = self.reference(name);
                    self.classes.insert(index, class);
                }
                // A macro that copies its receiver, as optMap does: that
                // receiver is `name . macro (`
                Some((paren, Some(_))) if paren >= 3 => {
                    let receiver = paren - 3;
                    if self.text(paren - 2).as_deref() == Some(".")
                        && self.is_identifier(receiver)
                        && self.text(receiver).as_deref() == Some(name.as_str())
                    {
                        let class = self.reference(name);
                        self.classes.entry(receiver).or_insert(class);
                    }
                }
                _ => {}
            },
            Expr::Call(call) => {
                if let Some(target) = &call.target {
                    // `base64.encode(b)`: `base64` names no variable, but
                    // with `encode`, a function
                    let namespace = match (&target.expr, self.at(target)) {
                        (Expr::Ident(name), Some((index, None)))
                            if !self.scope.contains(name)
                                && is_namespace(self.env, name, &call.func_name) =>
                        {
                            self.classes.insert(index, Class::Namespace);
                            true
                        }
                        _ => false,
                    };
                    if !namespace {
                        self.node(target);
                    }
                }
                for arg in &call.args {
                    self.node(arg);
                }
            }
            Expr::Comprehension(c) => {
                // Where the macro declares what it binds: `map(x, ...)`
                if let Some((paren, _)) = at {
                    if let Some(declared) = self.text(paren + 1) {
                        let names = [Some(&c.iter_var), c.iter_var2.as_ref(), Some(&c.accu_var)];
                        if self.is_identifier(paren + 1) && names.contains(&Some(&declared)) {
                            self.classes.insert(paren + 1, Class::Bound);
                        }
                    }
                }
                self.node(&c.iter_range);
                self.node(&c.accu_init);
                let outer = self.scope.len();
                // As in cel-go's checker: the accumulator is in scope of the
                // result, the iteration variables of the loop only
                self.scope.push(c.accu_var.clone());
                self.node(&c.result);
                self.scope.push(c.iter_var.clone());
                self.scope.extend(c.iter_var2.clone());
                self.node(&c.loop_cond);
                self.node(&c.loop_step);
                self.scope.truncate(outer);
            }
            Expr::List(list) => list.elements.iter().for_each(|x| self.node(x)),
            Expr::Select(select) => self.node(&select.operand),
            Expr::Map(map) => map.entries.iter().for_each(|en| self.entry(&en.expr)),
            Expr::Struct(s) => s.entries.iter().for_each(|en| self.entry(&en.expr)),
            Expr::Literal(_) | Expr::Unspecified => {}
        }
    }

    fn entry(&mut self, entry: &EntryExpr) {
        match entry {
            EntryExpr::MapEntry(e) => {
                self.node(&e.key);
                self.node(&e.value);
            }
            EntryExpr::StructField(f) => self.node(&f.value),
        }
    }
}

/// What the tree didn't say, told by where an identifier is: a function before
/// a `(`, a field after a `.` (or `.?`) or before the `:` of a struct literal,
/// a type before a `{`, and, in what parsed, a namespace before a `.`: the tree
/// has no node for it. While typing, that `.` is more likely a variable's.
fn by_place(units: &[u16], tokens: &[Token], parsed: bool, classes: &mut HashMap<usize, Class>) {
    let text = |i: usize| {
        tokens
            .get(i)
            .map(|t| String::from_utf16_lossy(&units[t.start..t.end]))
    };
    for (i, token) in tokens.iter().enumerate() {
        if token.kind != Kind::Identifier || classes.contains_key(&i) {
            continue;
        }
        let previous = i.checked_sub(1).and_then(text);
        let selected = previous.as_deref() == Some(".")
            || (previous.as_deref() == Some("?")
                && i.checked_sub(2).and_then(text).as_deref() == Some("."));
        let class = match text(i + 1).as_deref() {
            Some("(") => Class::Function,
            _ if selected => Class::Field,
            Some("{") => Class::Type,
            Some(":") => Class::Field,
            Some(".") if parsed => Class::Namespace,
            _ => continue,
        };
        classes.insert(i, class);
    }
}

thread_local! {
    static NAMESPACES: RefCell<HashMap<(String, String), bool>> = RefCell::new(HashMap::new());
}

/// Whether `name.function` names a function, as cel resolves a call on an
/// identifier: it calls `name.function`, or reads the variable `name`. The
/// environment can't be asked, so this evaluates `name.function()` with no
/// variables, and sees whether it reads `name`.
fn is_namespace(env: &Arc<Env>, name: &str, function: &str) -> bool {
    let key = (name.to_owned(), function.to_owned());
    if let Some(known) = NAMESPACES.with(|cache| cache.borrow().get(&key).copied()) {
        return known;
    }
    let namespace = match env.compile(&format!("{name}.{function}()")) {
        Ok(program) => !matches!(
            program.execute(&Context::with_env(Arc::clone(env))),
            Err(ExecutionError::UndeclaredReference(read)) if read.as_str() == name
        ),
        Err(_) => false,
    };
    NAMESPACES.with(|cache| cache.borrow_mut().insert(key, namespace));
    namespace
}

/// Where cel reports parse errors, by line and column of characters: the
/// token there, or the end of the expression.
fn error_ranges(expression: &str, tokens: &[Token], errors: &ParseErrors) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    for error in &errors.errors {
        let (line, column) = error.pos;
        if line < 1 || column < 1 {
            continue;
        }
        let (line, column) = (line as usize - 1, column as usize - 1);
        let mut offset = 0;
        let mut found = None;
        for (number, text) in expression.split('\n').enumerate() {
            if number == line {
                let chars: Vec<char> = text.chars().collect();
                let before: usize = chars.iter().take(column).map(|c| c.len_utf16()).sum();
                found = Some((offset + before, chars.get(column).map(|c| c.len_utf16())));
                break;
            }
            offset += text.encode_utf16().count() + 1;
        }
        let Some((start, width)) = found else {
            continue;
        };
        let range = match tokens.iter().find(|t| t.start <= start && start < t.end) {
            Some(token) => (token.start, token.end),
            None => (start, start + width.unwrap_or(0)),
        };
        if !ranges.contains(&range) {
            ranges.push(range);
        }
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Highlighted {
        units: Vec<u16>,
        result: Json,
    }

    fn highlight(expression: &str, input: &str) -> Highlighted {
        Highlighted {
            units: expression.encode_utf16().collect(),
            result: serde_json::from_str(&to_json(expression, input)).unwrap(),
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

        /// Each mark as `text:class`, in order.
        fn marks(&self) -> Vec<String> {
            self.result["marks"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| format!("{}:{}", self.text(&m[0], &m[1]), m[2].as_str().unwrap()))
                .collect()
        }

        fn errors(&self) -> Vec<String> {
            self.result["errors"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| self.text(&e[0], &e[1]))
                .collect()
        }
    }

    #[test]
    fn variables_fields_functions_and_macros() {
        let h = highlight(
            "request.method in ['GET'] && has(request.headers.auth) \
             && request.roles.exists(role, role in allowed) && size(x) > 0",
            r#"{"request": {}, "allowed": []}"#,
        );
        assert_eq!(
            h.marks(),
            [
                "request:variable",
                "method:field",
                "has:macro",
                "request:variable",
                "headers:field",
                "auth:field",
                "request:variable",
                "roles:field",
                "exists:macro",
                "role:bound",
                "role:bound",
                "allowed:variable",
                "size:function",
                "x:missing",
            ]
        );
    }

    #[test]
    fn namespaces_and_types() {
        assert_eq!(
            highlight(
                "base64.encode(b'x') == optional.of(1).value() && type(1) == int",
                ""
            )
            .marks(),
            [
                "base64:namespace",
                "encode:function",
                "optional:namespace",
                "of:function",
                "value:function",
                "type:function",
                "int:type",
            ]
        );
        // A macro in cel 0.15: no node for `math`
        assert_eq!(
            highlight("math.greatest([1])", "").marks(),
            ["math:namespace", "greatest:macro"]
        );
        // A variable spelled like a namespace, called a method of its own
        assert_eq!(
            highlight("base64.size()", r#"{"base64": "x"}"#).marks(),
            ["base64:variable", "size:function"]
        );
    }

    #[test]
    fn what_macros_bind_is_in_their_scope_only() {
        assert_eq!(
            highlight("[x].map(x, x) + [x]", r#"{"x": 1}"#).marks(),
            [
                "x:variable",
                "map:macro",
                "x:bound",
                "x:bound",
                "x:variable"
            ]
        );
        assert_eq!(
            highlight("o.optMap(v, v + 1)", r#"{"o": null}"#).marks(),
            ["o:variable", "optMap:macro", "v:bound", "v:bound"]
        );
        assert_eq!(
            highlight("o.optMap(v, v + w)", "").marks(),
            [
                "o:missing",
                "optMap:macro",
                "v:bound",
                "v:bound",
                "w:missing"
            ]
        );
    }

    #[test]
    fn without_an_object_as_input_nothing_is_missing() {
        assert_eq!(
            highlight("a + b", "[1]").marks(),
            ["a:variable", "b:variable"]
        );
        assert_eq!(
            highlight("a + b", "{").marks(),
            ["a:variable", "b:variable"]
        );
    }

    #[test]
    fn struct_literals() {
        assert_eq!(highlight("Msg{f: 1}", "").marks(), ["Msg:type", "f:field"]);
    }

    #[test]
    fn unparsable_keeps_what_places_tell() {
        let h = highlight("size(x) +", r#"{"x": []}"#);
        assert_eq!(h.marks(), ["size:function"]);
        assert_eq!(h.result["errors"], json!([[9, 9]]));
        assert_eq!(highlight("a.b(", "").marks(), ["b:function"]);
    }

    #[test]
    fn errors_are_at_their_token() {
        assert_eq!(highlight("x\n  + )", "").errors(), [")"]);
        assert_eq!(highlight("'é👋' + )", "").errors(), [")"]);
    }

    #[test]
    fn examples() {
        let examples: Json = serde_json::from_str(include_str!("../web/examples.json")).unwrap();
        for example in examples.as_array().unwrap() {
            let expression = example["expression"].as_str().unwrap();
            let input = example["input"].to_string();
            let h = highlight(expression, &input);
            assert_eq!(h.result["errors"], json!([]), "{expression}");
            // Every example has all its variables
            assert!(
                !h.marks().iter().any(|m| m.ends_with(":missing")),
                "{expression}: {:?}",
                h.marks()
            );
        }
    }
}
