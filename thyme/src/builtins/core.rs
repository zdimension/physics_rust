use crate::{Object, Value, eval::Evaluator, parse::Span};

use super::BuiltinSpec;

pub(super) const GLOBALS: &[BuiltinSpec] = &[BuiltinSpec::new("alloc", 0, alloc)];

fn alloc(
    _evaluator: &mut Evaluator<'_, '_>,
    _arguments: &[Value],
    _call_span: Span,
) -> Result<Value, String> {
    Ok(Value::Object(Object::new()))
}
