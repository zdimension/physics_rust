use std::f32::consts::TAU;

use rand::random;
use rand_distr::{Distribution, StandardNormal};

use crate::{List, Value, eval::Evaluator, parse::Span};

use super::{BuiltinNamespace, BuiltinSpec};

const MEMBERS: &[BuiltinSpec] = &[
    BuiltinSpec::new("boolean", 0, boolean),
    BuiltinSpec::new("normal", 0, normal),
    BuiltinSpec::new("normal2D", 0, normal_2d),
    BuiltinSpec::new("uniform01", 0, uniform_01),
    BuiltinSpec::new("direction2D", 0, direction_2d),
];

pub(super) const NAMESPACE: BuiltinNamespace = BuiltinNamespace::new("rand", MEMBERS);

fn normal_value() -> f32 {
    StandardNormal.sample(&mut rand::rng())
}

fn boolean(_: &mut Evaluator<'_, '_>, _: &[Value], _: Span) -> Result<Value, String> {
    Ok(Value::Bool(random()))
}

fn normal(_: &mut Evaluator<'_, '_>, _: &[Value], _: Span) -> Result<Value, String> {
    Ok(Value::from(normal_value()))
}

fn normal_2d(_: &mut Evaluator<'_, '_>, _: &[Value], _: Span) -> Result<Value, String> {
    Ok(Value::List(List::new([
        Value::from(normal_value()),
        Value::from(normal_value()),
    ])))
}

fn uniform_01(_: &mut Evaluator<'_, '_>, _: &[Value], _: Span) -> Result<Value, String> {
    Ok(Value::from(random::<f32>()))
}

fn direction_2d(_: &mut Evaluator<'_, '_>, _: &[Value], _: Span) -> Result<Value, String> {
    let angle = TAU * random::<f32>();
    Ok(Value::List(List::new([
        Value::from(angle.cos()),
        Value::from(angle.sin()),
    ])))
}
