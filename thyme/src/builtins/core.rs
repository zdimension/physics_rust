use crate::{Object, Value, eval::Evaluator, parse::Span};

use super::BuiltinSpec;

pub(super) const GLOBALS: &[BuiltinSpec] = &[
    BuiltinSpec::new("alloc", 0, alloc),
    BuiltinSpec::new("eval", 1, eval),
    BuiltinSpec::new("geval", 1, geval),
];

fn alloc(
    _evaluator: &mut Evaluator<'_, '_>,
    _arguments: &[Value],
    _call_span: Span,
) -> Result<Value, String> {
    Ok(Value::Object(Object::new()))
}

fn source<'a>(arguments: &'a [Value], name: &str) -> Result<&'a str, String> {
    let Value::Str(source) = &arguments[0] else {
        return Err(format!("{name}: expected string"));
    };
    Ok(source)
}

fn eval(
    evaluator: &mut Evaluator<'_, '_>,
    arguments: &[Value],
    _call_span: Span,
) -> Result<Value, String> {
    evaluator.eval_source(source(arguments, "eval")?)
}

pub(super) fn geval(
    evaluator: &mut Evaluator<'_, '_>,
    arguments: &[Value],
    _call_span: Span,
) -> Result<Value, String> {
    evaluator.eval_global_source(source(arguments, "geval")?)
}
