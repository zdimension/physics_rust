use crate::{List, Value, eval::Evaluator, parse::Span};

use super::{BuiltinNamespace, BuiltinSpec};

const MEMBERS: &[BuiltinSpec] = &[
    BuiltinSpec::new("insert", 2, insert),
    BuiltinSpec::new("merge", 2, merge),
];

pub(super) const NAMESPACE: BuiltinNamespace = BuiltinNamespace::new("set", MEMBERS);

fn kind(value: &Value) -> Option<bool> {
    match value {
        Value::Number(_) => Some(true),
        Value::Str(_) => Some(false),
        _ => None,
    }
}

fn set_kind(set: &[Value]) -> Result<Option<bool>, ()> {
    let Some(expected) = set.first().and_then(kind) else {
        return if set.is_empty() { Ok(None) } else { Err(()) };
    };
    set.iter()
        .all(|value| kind(value) == Some(expected))
        .then_some(Some(expected))
        .ok_or(())
}

fn insert(
    _evaluator: &mut Evaluator<'_, '_>,
    arguments: &[Value],
    _call_span: Span,
) -> Result<Value, String> {
    let (Value::List(set), Some(value_kind)) = (&arguments[0], kind(&arguments[1])) else {
        return Err("set.insert: expected set and number or string".into());
    };
    let Ok(set_kind) = set_kind(set.as_slice()) else {
        return Err("set.insert: invalid set".into());
    };
    if set_kind.is_some_and(|kind| kind != value_kind) {
        return Err("set.insert: incompatible types".into());
    }
    let mut result = set.as_slice().to_vec();
    if !result.contains(&arguments[1]) {
        result.push(arguments[1].clone());
    }
    Ok(Value::List(List::from(result)))
}

fn merge(
    _evaluator: &mut Evaluator<'_, '_>,
    arguments: &[Value],
    _call_span: Span,
) -> Result<Value, String> {
    let (Value::List(a), Value::List(b)) = (&arguments[0], &arguments[1]) else {
        return Err("set.merge: expected two sets".into());
    };
    let (Ok(a_kind), Ok(b_kind)) = (set_kind(a.as_slice()), set_kind(b.as_slice())) else {
        return Err("set.merge: invalid set".into());
    };
    if a_kind.zip(b_kind).is_some_and(|(a, b)| a != b) {
        return Err("set.merge: incompatible types".into());
    }

    let mut result = a.as_slice().to_vec();
    for value in b.as_slice() {
        if !result.contains(value) {
            result.push(value.clone());
        }
    }
    Ok(Value::List(List::from(result)))
}
