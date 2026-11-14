//! What the playground asks of cel, whichever version it builds with: what
//! differs between versions is here, the rest of the code is the same.
//!
//! cel 0.14 has no extensions, parses without an environment, and doesn't say
//! where the nodes of what parses are: [`Program::source_info`] is `None`, so
//! what needs those positions does without them.

use cel::common::ast::{IdedExpr, SourceInfo};
use cel::{Context, Env, ParseErrors, ResolveResult};

/// The standard library, with optionals, and every extension the version has.
pub fn environment() -> Env {
    #[cfg(not(feature = "cel-0-14"))]
    {
        use cel::extensions;
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
    #[cfg(feature = "cel-0-14")]
    Env::stdlib()
}

/// An expression `env` parsed, ready to evaluate.
pub struct Program {
    #[cfg(not(feature = "cel-0-14"))]
    program: cel::Program,
    #[cfg(feature = "cel-0-14")]
    expression: IdedExpr,
}

/// Parses `source` as `env` would, its optional syntax (`a.?b`) included.
#[cfg_attr(feature = "cel-0-14", allow(unused_variables))]
pub fn compile(env: &Env, source: &str) -> Result<Program, ParseErrors> {
    #[cfg(not(feature = "cel-0-14"))]
    return env.compile(source).map(|program| Program { program });
    // What `cel::Program::compile` does, but for the optional syntax
    #[cfg(feature = "cel-0-14")]
    return cel::parser::Parser::new()
        .enable_optional_syntax(true)
        .parse(source)
        .map(|expression| Program { expression });
}

impl Program {
    pub fn expression(&self) -> &IdedExpr {
        #[cfg(not(feature = "cel-0-14"))]
        return self.program.expression();
        #[cfg(feature = "cel-0-14")]
        return &self.expression;
    }

    /// Where cel positions each node; `None` where it doesn't say.
    pub fn source_info(&self) -> Option<&SourceInfo> {
        #[cfg(not(feature = "cel-0-14"))]
        return Some(self.program.source_info());
        #[cfg(feature = "cel-0-14")]
        return None;
    }

    pub fn execute(&self, context: &Context) -> ResolveResult {
        #[cfg(not(feature = "cel-0-14"))]
        return self.program.execute(context);
        #[cfg(feature = "cel-0-14")]
        return context.resolve(&self.expression);
    }
}

/// Whether `name` is a type `env` knows. Only asked of what cel positions,
/// which 0.14 doesn't.
#[cfg_attr(feature = "cel-0-14", allow(unused_variables))]
pub fn is_type(env: &Env, name: &str) -> bool {
    #[cfg(not(feature = "cel-0-14"))]
    return env.types().find_type(name).is_some();
    #[cfg(feature = "cel-0-14")]
    return false;
}
