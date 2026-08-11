use crate::{Value, eval::Evaluator, parse::Span};

use super::{BuiltinNamespace, BuiltinSpec, core};

const MEMBERS: &[BuiltinSpec] = &[
    BuiltinSpec::new("ExecuteCode", 1, core::geval),
    BuiltinSpec::new("ExecuteFile", 1, execute_file),
];

pub(super) const NAMESPACE: BuiltinNamespace = BuiltinNamespace::new("Reflection", MEMBERS);

fn execute_file(
    evaluator: &mut Evaluator<'_, '_>,
    arguments: &[Value],
    _call_span: Span,
) -> Result<Value, String> {
    let Value::Str(path) = &arguments[0] else {
        return Err("Reflection.ExecuteFile: expected string".into());
    };
    let source = evaluator
        .host
        .read_source(path)
        .map_err(|error| error.to_string())?;
    evaluator.eval_global_source(&source)
}
