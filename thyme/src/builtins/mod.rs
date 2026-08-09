use std::rc::Rc;

use dumpster::{TraceWith, Visitor};

use crate::{Environment, Function, Object, Value, eval::Evaluator, parse::Span};

mod core;
mod math;
mod rand;
mod set;
mod string;

pub(crate) type BuiltinCallback = for<'runtime, 'host> fn(
    &mut Evaluator<'runtime, 'host>,
    &[Value],
    Span,
) -> Result<Value, String>;

pub(crate) struct BuiltinFunction {
    name: Rc<str>,
    arity: usize,
    callback: BuiltinCallback,
}

impl BuiltinFunction {
    pub(crate) fn new(name: impl AsRef<str>, arity: usize, callback: BuiltinCallback) -> Self {
        Self {
            name: Rc::from(name.as_ref()),
            arity,
            callback,
        }
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn arity(&self) -> usize {
        self.arity
    }

    pub(crate) fn callback(&self) -> BuiltinCallback {
        self.callback
    }
}

// SAFETY: A builtin contains only an `Rc<str>`, numeric metadata, and a plain function pointer.
// Builtin callbacks cannot capture garbage-collected values, so there are no `Gc` edges to visit.
unsafe impl<V: Visitor> TraceWith<V> for BuiltinFunction {
    fn accept(&self, _visitor: &mut V) -> Result<(), ()> {
        Ok(())
    }
}

#[derive(Copy, Clone)]
pub(super) struct BuiltinSpec {
    name: &'static str,
    arity: usize,
    callback: BuiltinCallback,
}

impl BuiltinSpec {
    pub(super) const fn new(name: &'static str, arity: usize, callback: BuiltinCallback) -> Self {
        Self {
            name,
            arity,
            callback,
        }
    }
}

#[derive(Copy, Clone)]
pub(super) struct BuiltinNamespace {
    name: &'static str,
    members: &'static [BuiltinSpec],
}

impl BuiltinNamespace {
    pub(super) const fn new(name: &'static str, members: &'static [BuiltinSpec]) -> Self {
        Self { name, members }
    }
}

const NAMESPACES: &[BuiltinNamespace] = &[
    string::NAMESPACE,
    set::NAMESPACE,
    math::NAMESPACE,
    rand::NAMESPACE,
];

pub(crate) fn install(environment: &Environment) {
    for builtin in core::GLOBALS {
        environment.define_read_only(
            builtin.name,
            Value::Function(Function::builtin(
                builtin.name,
                builtin.arity,
                builtin.callback,
            )),
        );
    }

    for namespace in NAMESPACES {
        let object = Object::new();
        for builtin in namespace.members {
            let qualified_name = format!("{}.{}", namespace.name, builtin.name);
            object.define_read_only_field(
                builtin.name,
                Value::Function(Function::builtin(
                    qualified_name,
                    builtin.arity,
                    builtin.callback,
                )),
            );
        }
        environment.define_read_only(namespace.name, Value::Object(object));
    }
}
