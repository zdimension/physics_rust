use crate::{List, Value, eval::Evaluator, parse::Number, parse::Span};

use super::{BuiltinNamespace, BuiltinSpec, string_argument};

const MEMBERS: &[BuiltinSpec] = &[
    BuiltinSpec::new("length", 1, length),
    BuiltinSpec::new("split", 2, split),
    BuiltinSpec::new("str2list", 1, str2list),
];

pub(super) const NAMESPACE: BuiltinNamespace = BuiltinNamespace::new("string", MEMBERS);

fn length(
    _evaluator: &mut Evaluator<'_, '_>,
    arguments: &[Value],
    call_span: Span,
) -> Result<Value, String> {
    let length = match &arguments[0] {
        Value::Str(string) => string.chars().count(),
        Value::List(list) => list.as_slice().len(),
        value => {
            return Err(format!(
                "Builtin string.length expected a string or list, got {value} at {call_span:?}"
            ));
        }
    };
    let length = i32::try_from(length).map_err(|_| {
        format!("Builtin string.length result does not fit in an integer at {call_span:?}")
    })?;
    Ok(Value::Number(Number::Int(length)))
}

fn split(
    _evaluator: &mut Evaluator<'_, '_>,
    arguments: &[Value],
    call_span: Span,
) -> Result<Value, String> {
    let string = string_argument("string.split", arguments, 0, call_span)?;
    let delimiter = string_argument("string.split", arguments, 1, call_span)?;
    let mut characters = delimiter.chars();
    let Some(delimiter) = characters.next() else {
        return Err(format!(
            "Builtin string.split delimiter must be exactly one character at {call_span:?}"
        ));
    };
    if characters.next().is_some() {
        return Err(format!(
            "Builtin string.split delimiter must be exactly one character at {call_span:?}"
        ));
    }

    Ok(Value::List(List::new(
        string.split(delimiter).map(|part| Value::Str(part.into())),
    )))
}

fn str2list(
    _evaluator: &mut Evaluator<'_, '_>,
    arguments: &[Value],
    call_span: Span,
) -> Result<Value, String> {
    let string = string_argument("string.str2list", arguments, 0, call_span)?;
    Ok(Value::List(List::new(string.chars().map(|character| {
        Value::Str(character.to_string().into())
    }))))
}
