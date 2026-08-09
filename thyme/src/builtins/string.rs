use crate::{List, Value, eval::Evaluator, parse::Number, parse::Span};

use super::{BuiltinNamespace, BuiltinSpec};

const MEMBERS: &[BuiltinSpec] = &[
    BuiltinSpec::new("length", 1, length),
    BuiltinSpec::new("split", 2, split),
    BuiltinSpec::new("str2list", 1, str2list),
    BuiltinSpec::new("list2str", 1, list2str),
];

pub(super) const NAMESPACE: BuiltinNamespace = BuiltinNamespace::new("string", MEMBERS);

fn length(
    _evaluator: &mut Evaluator<'_, '_>,
    arguments: &[Value],
    _call_span: Span,
) -> Result<Value, String> {
    let length = match &arguments[0] {
        Value::Str(string) => string.chars().count(),
        Value::List(list) => list.as_slice().len(),
        _ => return Err("string.length: expected string or list".into()),
    };
    let length = i32::try_from(length).map_err(|_| "string.length: result too large")?;
    Ok(Value::Number(Number::Int(length)))
}

fn split(
    _evaluator: &mut Evaluator<'_, '_>,
    arguments: &[Value],
    _call_span: Span,
) -> Result<Value, String> {
    let (Value::Str(string), Value::Str(delimiter)) = (&arguments[0], &arguments[1]) else {
        return Err("string.split: expected strings".into());
    };
    let mut chars = delimiter.chars();
    let (Some(delimiter), None) = (chars.next(), chars.next()) else {
        return Err("string.split: delimiter must be one character".into());
    };

    Ok(Value::List(List::new(
        string.split(delimiter).map(|part| Value::Str(part.into())),
    )))
}

fn str2list(
    _evaluator: &mut Evaluator<'_, '_>,
    arguments: &[Value],
    _call_span: Span,
) -> Result<Value, String> {
    let Value::Str(string) = &arguments[0] else {
        return Err("string.str2list: expected string".into());
    };
    Ok(Value::List(List::new(string.chars().map(|character| {
        Value::Str(character.to_string().into())
    }))))
}

fn list2str(
    _evaluator: &mut Evaluator<'_, '_>,
    arguments: &[Value],
    _call_span: Span,
) -> Result<Value, String> {
    let Value::List(list) = &arguments[0] else {
        return Err("string.list2str: expected list".into());
    };
    Ok(Value::Str(
        list.as_slice()
            .iter()
            .map(ToString::to_string)
            .collect::<String>()
            .into(),
    ))
}
