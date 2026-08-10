use std::collections::HashMap;

use dumpster::unsync::Gc;

use crate::{
    Environment, Function, FunctionValue, Host, List, NativeObjectId, Object, PropertyId, Runtime,
    Symbol, UserFunction, Value,
    parse::{AssignmentKind, AssignmentTarget, BinaryOp, Expr, Literal, Number, Span, UnaryOp},
};

pub struct Evaluator<'runtime, 'host> {
    pub runtime: &'runtime Runtime,
    pub host: &'host mut dyn Host,
}

enum ResolvedMember {
    Native {
        object: NativeObjectId,
        property: PropertyId,
        writable: bool,
        name: Symbol,
    },
    Dynamic {
        object: Object,
        name: Symbol,
    },
}

impl ResolvedMember {
    fn exists(&self) -> bool {
        match self {
            Self::Native { .. } => true,
            Self::Dynamic { object, name } => object.field_symbol(name).is_some(),
        }
    }
}

impl<'runtime, 'host> Evaluator<'runtime, 'host> {
    /// if the value is a zero-parameter function, call it and return the result, otherwise return the value as-is
    fn collapse(&mut self, value: Value) -> Result<Value, String> {
        if let Value::Function(function) = &value {
            if function.arity() == 0 {
                return self.call_function(function, &[], (0..0).into());
            }
        }
        Ok(value)
    }

    fn span_suffix(span: Option<Span>) -> String {
        span.map(|span| format!(" at {span:?}")).unwrap_or_default()
    }

    fn resolve_member(
        &mut self,
        object: Object,
        name: &Symbol,
        span: Option<Span>,
    ) -> Result<ResolvedMember, String> {
        if let Some(object_id) = object.native_id() {
            let property = self
                .host
                .resolve_property(object_id, name)
                .map_err(|error| {
                    format!(
                        "Failed to resolve native property {name}{}: {error}",
                        Self::span_suffix(span)
                    )
                })?;
            if let Some(property) = property {
                return Ok(ResolvedMember::Native {
                    object: object_id,
                    property: property.id(),
                    writable: property.is_writable(),
                    name: name.clone(),
                });
            }
        }

        Ok(ResolvedMember::Dynamic {
            object,
            name: name.clone(),
        })
    }

    fn read_member(
        &mut self,
        member: &ResolvedMember,
        span: Option<Span>,
    ) -> Result<Option<Value>, String> {
        match member {
            ResolvedMember::Native {
                object,
                property,
                name,
                writable: _,
            } => self
                .host
                .get_property(*object, *property)
                .map(Some)
                .map_err(|error| {
                    format!(
                        "Failed to get native property {name}{}: {error}",
                        Self::span_suffix(span)
                    )
                }),
            ResolvedMember::Dynamic { object, name } => Ok(object.field_symbol(name)),
        }
    }

    fn write_member(
        &mut self,
        member: ResolvedMember,
        value: Value,
        span: Option<Span>,
    ) -> Result<(), String> {
        match member {
            ResolvedMember::Native {
                object,
                property,
                writable,
                name,
            } => {
                if !writable {
                    return Err(format!(
                        "Cannot set read-only member {name}{}",
                        Self::span_suffix(span)
                    ));
                }
                self.runtime
                    .assign_native_property(self.host, object, property, value)
                    .map_err(|error| {
                        format!(
                            "Failed to set native property {name}{}: {error}",
                            Self::span_suffix(span)
                        )
                    })
            }
            ResolvedMember::Dynamic { object, name } => {
                if object.field_is_read_only(&name) {
                    return Err(format!(
                        "Cannot set read-only member {name}{}",
                        Self::span_suffix(span)
                    ));
                }
                object.set_field(name, value);
                Ok(())
            }
        }
    }

    fn read_symbol(
        &mut self,
        env: &Gc<Environment>,
        name: &Symbol,
    ) -> Result<Option<Value>, String> {
        let mut scope = Some(env.clone());
        while let Some(current) = scope {
            if let Some(value) = current.local(name) {
                return Ok(Some(value));
            }
            if let Some(receiver) = current.receiver() {
                let member = self.resolve_member(receiver, name, None)?;
                if let Some(value) = self.read_member(&member, None)? {
                    return Ok(Some(value));
                }
            }
            scope = current.parent();
        }
        Ok(None)
    }

    fn assign_symbol(
        &mut self,
        env: &Gc<Environment>,
        name: &Symbol,
        kind: AssignmentKind,
        value: Value,
        span: Span,
    ) -> Result<(), String> {
        if kind == AssignmentKind::Declare {
            if env.contains_local(name) {
                if env.local_is_read_only(name) {
                    return Err(format!("Cannot set read-only binding {name} at {span:?}"));
                }
                env.declare(name.clone(), value);
                return Ok(());
            }
            if env.initializes_receiver()
                && let Some(receiver) = env.receiver()
            {
                let member = self.resolve_member(receiver, name, Some(span))?;
                if member.exists() {
                    return self.write_member(member, value, Some(span));
                }
            }
            env.declare(name.clone(), value);
            return Ok(());
        }

        let mut scope = Some(env.clone());
        while let Some(current) = scope {
            if current.contains_local(name) {
                if current.local_is_read_only(name) {
                    return Err(format!("Cannot set read-only binding {name} at {span:?}"));
                }
                current.declare(name.clone(), value);
                return Ok(());
            }
            if let Some(receiver) = current.receiver() {
                let member = self.resolve_member(receiver, name, Some(span))?;
                if member.exists() {
                    return self.write_member(member, value, Some(span));
                }
                current.declare(name.clone(), value);
                return Ok(());
            }
            scope = current.parent();
        }

        env.declare(name.clone(), value);
        Ok(())
    }

    pub fn eval_expr(&mut self, expr: &Expr, env: &Gc<Environment>) -> Result<Value, String> {
        Ok(match expr {
            Expr::Error => return Err("Cannot evaluate an error expression".to_string()),
            Expr::Parenthesized(inner) => self.eval_expr(&inner.0, env)?,
            Expr::Value(Literal::Null) => Value::Null,
            Expr::Value(Literal::Bool(b)) => Value::Bool(*b),
            Expr::Value(Literal::Number(num)) => Value::Number(*num),
            Expr::Value(Literal::Str(s)) => Value::Str(s.clone()),
            Expr::List(list) => Value::List(List(
                list.iter()
                    .map(|(expr, _)| self.eval_expr(expr, env))
                    .collect::<Result<Gc<[_]>, _>>()?,
            )),
            Expr::Symbol(sym) => match self.read_symbol(env, sym)? {
                Some(value) => self.collapse(value)?,
                None => Value::Undefined,
            },
            Expr::Member(object, (member, member_span)) => {
                let object_value = self.eval_expr(&object.0, env)?;
                let Value::Object(object) = object_value else {
                    return Err(format!(
                        "Cannot access member {member} of non-object value {object_value} at \
                         {member_span:?}"
                    ));
                };

                let resolved = self.resolve_member(object, member, Some(*member_span))?;
                let value = self
                    .read_member(&resolved, Some(*member_span))?
                    .unwrap_or(Value::Undefined);

                self.collapse(value)?
            }
            Expr::Call(function_expr, (argument_exprs, call_span)) => {
                let function_value = self.eval_expr(&function_expr.0, env)?;
                let arguments = argument_exprs
                    .iter()
                    .map(|(argument, _)| self.eval_expr(argument, env))
                    .collect::<Result<Vec<_>, _>>()?;
                match function_value {
                    Value::Function(function) => {
                        self.call_function(&function, &arguments, *call_span)?
                    }
                    Value::List(list) => {
                        let [index] = &arguments[..] else {
                            return Err(format!(
                                "List index expects exactly 1 argument (got {}) at {call_span:?}",
                                arguments.len()
                            ));
                        };
                        self.apply_unary(
                            |index| match index {
                                Value::Number(Number::Int(i)) => {
                                    let i = if i < 0 {
                                        list.0.len().wrapping_add(i as isize as usize)
                                    } else {
                                        i as usize
                                    };
                                    if i < list.0.len() {
                                        Ok(list.0[i].clone())
                                    } else {
                                        Err(format!(
                                            "List index {i} out of bounds (length {})",
                                            list.0.len()
                                        ))
                                    }
                                }
                                _ => Err(format!(
                                    "Cannot use non-integer value {index} as a list index"
                                )),
                            },
                            index.clone(),
                        )?
                    }
                    _ => {
                        return Err(format!(
                            "Cannot call non-function value {function_value} at {call_span:?}: {}",
                            expr.pretty_print()
                        ));
                    }
                }
            }
            Expr::Ternary(cond, then_expr, else_expr) => {
                let cond_value = self.eval_expr(&cond.0, env)?;
                let Value::Bool(cond_bool) = cond_value else {
                    return Err(format!(
                        "Cannot use non-boolean value {cond_value} as a condition"
                    ));
                };
                let res = if cond_bool {
                    self.eval_expr(&then_expr.0, env)?
                } else {
                    self.eval_expr(&else_expr.0, env)?
                };
                self.collapse(res)?
            }
            Expr::Func(definition) => {
                Value::Function(Function(Gc::new(FunctionValue::User(UserFunction {
                    definition: definition.clone(),
                    env: env.clone(),
                }))))
            }
            Expr::Seq(expressions) => {
                let mut result = Value::Void;
                for expression in expressions {
                    result = self.eval_expr(&expression.0, env)?;
                }
                result
            }
            Expr::Unary(op, expr) => {
                let value = self.eval_expr(&expr.0, env)?;
                self.apply_unary(
                    |value| match op {
                        UnaryOp::Neg => match value {
                            Value::Number(Number::Int(num)) => {
                                Ok(Value::Number(Number::Int(num.wrapping_neg())))
                            }
                            Value::Number(Number::Float(num)) => {
                                Ok(Value::Number(Number::Float(-num)))
                            }
                            _ => Err(format!("Cannot negate non-number value {value}")),
                        },
                        UnaryOp::Pos => match value {
                            Value::Number(num) => Ok(Value::Number(num)),
                            _ => Err(format!("Cannot posivate non-number value {value}")),
                        },
                        UnaryOp::Not => match value {
                            Value::Bool(b) => Ok(Value::Bool(!b)),
                            _ => Err(format!("Cannot invert non-boolean value {value}")),
                        },
                    },
                    value,
                )?
            }
            Expr::Assignment(target, kind, right) => {
                let right_value = self.eval_expr(&right.0, env)?;
                match target {
                    AssignmentTarget::Name((name, span)) => {
                        self.assign_symbol(env, name, *kind, right_value.clone(), *span)?
                    }
                    AssignmentTarget::Member(object_expr, (member, member_span)) => {
                        let object_value = self.eval_expr(&object_expr.0, env)?;
                        let Value::Object(object) = object_value else {
                            return Err(format!(
                                "Cannot assign to member {member} of non-object value \
                                 {object_value} at {member_span:?}"
                            ));
                        };
                        let resolved = self.resolve_member(object, member, Some(*member_span))?;
                        self.write_member(resolved, right_value.clone(), Some(*member_span))?;
                    }
                }
                right_value
            }
            Expr::With(object_expr, body) => {
                let object_value = self.eval_expr(&object_expr.0, env)?;
                let Value::Object(object) = object_value else {
                    return Err(format!(
                        "Cannot use '->' with non-object value {object_value} at {:?}",
                        object_expr.1
                    ));
                };
                let with_environment = Gc::new(Environment::child(
                    env.clone(),
                    HashMap::new(),
                    Some(object.clone()),
                    false,
                ));
                self.eval_expr(&body.0, &with_environment)?;
                Value::Object(object)
            }
            Expr::Binary(left, op, right) => {
                let right_value = self.eval_expr(&right.0, env)?;
                let left_value = self.eval_expr(&left.0, env)?;
                let left_value = self.collapse(left_value)?;
                let right_value = self.collapse(right_value)?;

                let (int, float, other): (
                    fn(i32, i32) -> Result<Value, String>,
                    fn(f32, f32) -> Value,
                    Option<fn(Value, Value) -> Result<Value, String>>,
                ) = match *op {
                    // Algodoo always performs float exponentiation
                    BinaryOp::Pow => (
                        |l, r| Ok((l as f32).powf(r as f32).into()),
                        |l, r| l.powf(r).into(),
                        None,
                    ),

                    BinaryOp::Mul => (
                        |l, r| Ok(l.wrapping_mul(r).into()),
                        |l, r| (l * r).into(),
                        None,
                    ),
                    BinaryOp::Div => (
                        |l, r| {
                            if r == 0 {
                                Err("Division by zero".to_string())
                            } else {
                                Ok((l / r).into())
                            }
                        },
                        |l, r| (l / r).into(),
                        None,
                    ),
                    BinaryOp::Mod => (
                        |l, r| {
                            if r == 0 {
                                Err("Modulus by zero".to_string())
                            } else {
                                Ok((l % r).into())
                            }
                        },
                        |l, r| (l % r).into(),
                        None,
                    ),

                    BinaryOp::Add => (
                        |l, r| Ok(l.wrapping_add(r).into()),
                        |l, r| (l + r).into(),
                        Some(|l, r| match (l, r) {
                            (Value::Str(ls), Value::Str(rs)) => {
                                Ok(Value::Str(format!("{ls}{rs}").into()))
                            }
                            (l, r) => Err(format!("Cannot add values {l} and {r}")),
                        }),
                    ),
                    BinaryOp::Sub => (
                        |l, r| Ok(l.wrapping_sub(r).into()),
                        |l, r| (l - r).into(),
                        None,
                    ),

                    BinaryOp::ListConcat => match (left_value, right_value) {
                        (Value::List(left_list), Value::List(right_list)) => {
                            let new_list = left_list
                                .0
                                .iter()
                                .chain(right_list.0.iter())
                                .cloned()
                                .collect();
                            return Ok(Value::List(List(new_list)));
                        }
                        (l, r) => {
                            return Err(format!("Cannot concatenate non-list values {l} and {r}"));
                        }
                    },

                    BinaryOp::Less => (
                        |l, r| Ok((l < r).into()),
                        |l, r| (l < r).into(),
                        Some(|l, r| match (l, r) {
                            (Value::Str(ls), Value::Str(rs)) => Ok((ls < rs).into()),
                            (l, r) => Err(format!("Cannot compare values {l} and {r}")),
                        }),
                    ),
                    BinaryOp::LessEq => (
                        |l, r| Ok((l <= r).into()),
                        |l, r| (l <= r).into(),
                        Some(|l, r| match (l, r) {
                            (Value::Str(ls), Value::Str(rs)) => Ok((ls <= rs).into()),
                            (l, r) => Err(format!("Cannot compare values {l} and {r}")),
                        }),
                    ),
                    BinaryOp::Greater => (
                        |l, r| Ok((l > r).into()),
                        |l, r| (l > r).into(),
                        Some(|l, r| match (l, r) {
                            (Value::Str(ls), Value::Str(rs)) => Ok((ls > rs).into()),
                            (l, r) => Err(format!("Cannot compare values {l} and {r}")),
                        }),
                    ),
                    BinaryOp::GreaterEq => (
                        |l, r| Ok((l >= r).into()),
                        |l, r| (l >= r).into(),
                        Some(|l, r| match (l, r) {
                            (Value::Str(ls), Value::Str(rs)) => Ok((ls >= rs).into()),
                            (l, r) => Err(format!("Cannot compare values {l} and {r}")),
                        }),
                    ),

                    BinaryOp::Eq => return Ok((left_value == right_value).into()),
                    BinaryOp::NotEq => return Ok((left_value != right_value).into()),

                    BinaryOp::Range => {
                        // the operation is defined with a recursive Thyme function in thyme.cfg so we can
                        // be lazy here
                        return Ok(match (left_value, right_value) {
                            (Value::Number(Number::Int(l)), Value::Number(r)) => {
                                let range = l..=r.floor();
                                Value::List(List(range.map(|n| Value::Number(n.into())).collect()))
                            }
                            (Value::Number(Number::Float(l)), Value::Number(r)) => {
                                let count = (r.to_f32_lossy() - l).floor() as i32;
                                let range = 0..=count;
                                Value::List(List(
                                    range
                                        .map(|n| Value::Number((l + n as f32).into()))
                                        .collect(),
                                ))
                            }
                            (l, r) => {
                                return Err(format!(
                                    "Cannot create a range from non-number values {l} and {r}"
                                ));
                            }
                        });
                    }

                    BinaryOp::And => {
                        return self.apply_binary(
                            |l, r| match (l, r) {
                                (Value::Bool(lb), Value::Bool(rb)) => Ok(Value::Bool(lb && rb)),
                                (l, r) => Err(format!(
                                    "Cannot apply logical AND to non-boolean values {l} and {r}"
                                )),
                            },
                            left_value,
                            right_value,
                        );
                    }
                    BinaryOp::Or => {
                        return self.apply_binary(
                            |l, r| match (l, r) {
                                (Value::Bool(lb), Value::Bool(rb)) => Ok(Value::Bool(lb || rb)),
                                (l, r) => Err(format!(
                                    "Cannot apply logical OR to non-boolean values {l} and {r}"
                                )),
                            },
                            left_value,
                            right_value,
                        );
                    }
                };

                self.apply_binary(
                    move |left_value, right_value| {
                        Ok(match (left_value, right_value) {
                            (Value::Number(left_num), Value::Number(right_num)) => {
                                use Number::*;
                                match (left_num, right_num) {
                                    (Int(l), Int(r)) => int(l, r)?,
                                    (Int(l), Float(r)) => float(l as f32, r),
                                    (Float(l), Int(r)) => float(l, r as f32),
                                    (Float(l), Float(r)) => float(l, r),
                                }
                            }
                            (left, right) => match other {
                                Some(handler) => handler(left, right)?,
                                None => {
                                    return Err(format!(
                                        "Cannot apply binary operation to values {left} and {right}"
                                    ));
                                }
                            },
                        })
                    },
                    left_value,
                    right_value,
                )?
            }
        })
    }

    /// applies a handler to a value. If the value is a list, then recursively applies the handler to each element.
    pub(crate) fn apply_unary(
        &mut self,
        handler: impl FnOnce(Value) -> Result<Value, String> + Copy,
        value: Value,
    ) -> Result<Value, String> {
        match value {
            Value::List(list) => {
                let new_list = list
                    .0
                    .iter()
                    .map(|element| self.apply_unary(handler, element.clone()))
                    .collect::<Result<Gc<[_]>, _>>()?;
                Ok(Value::List(List(new_list)))
            }
            value => handler(value),
        }
    }

    fn apply_binary(
        &mut self,
        handler: impl FnOnce(Value, Value) -> Result<Value, String> + Copy,
        left: Value,
        right: Value,
    ) -> Result<Value, String> {
        match (left, right) {
            (Value::List(left_list), Value::List(right_list)) => {
                if left_list.0.len() != right_list.0.len() {
                    return Err(format!(
                        "Cannot apply binary operation to lists of different lengths: {} and {}",
                        left_list.0.len(),
                        right_list.0.len()
                    ));
                }
                let new_list = left_list
                    .0
                    .iter()
                    .zip(right_list.0.iter())
                    .map(|(left_element, right_element)| {
                        self.apply_binary(handler, left_element.clone(), right_element.clone())
                    })
                    .collect::<Result<Gc<[_]>, _>>()?;
                Ok(Value::List(List(new_list)))
            }
            (left, right) => handler(left, right),
        }
    }

    pub fn call_function(
        &mut self,
        function: &Function,
        arguments: &[Value],
        call_span: Span,
    ) -> Result<Value, String> {
        self.call_function_with_receiver(function, arguments, None, false, call_span)
    }

    pub fn call_function_with_receiver(
        &mut self,
        function: &Function,
        arguments: &[Value],
        receiver: Option<Object>,
        initialize_receiver: bool,
        call_span: Span,
    ) -> Result<Value, String> {
        match &*function.0 {
            FunctionValue::Builtin(builtin) => {
                if arguments.len() != builtin.arity() {
                    return Err(format!(
                        "Builtin {} expected {} arguments, got {} at {call_span:?}",
                        builtin.name(),
                        builtin.arity(),
                        arguments.len()
                    ));
                }

                (builtin.callback())(self, arguments, call_span)
            }
            FunctionValue::Intrinsic(intrinsic) => {
                if arguments.len() != intrinsic.arity {
                    return Err(format!(
                        "Intrinsic {} expected {} arguments, got {} at {call_span:?}",
                        intrinsic.name,
                        intrinsic.arity,
                        arguments.len()
                    ));
                }

                self.host
                    .call_intrinsic(intrinsic.receiver, intrinsic.id, arguments)
                    .map_err(|error| {
                        format!(
                            "Intrinsic {} failed at {call_span:?}: {error}",
                            intrinsic.name
                        )
                    })
            }
            FunctionValue::User(user) => {
                let definition = user.definition.clone();
                let captured_environment = user.env.clone();

                if arguments.len() != definition.params.len() {
                    return Err(format!(
                        "Function expected {} arguments, got {} at {call_span:?}",
                        definition.params.len(),
                        arguments.len()
                    ));
                }

                let bindings: HashMap<Symbol, Value> = definition
                    .params
                    .iter()
                    .cloned()
                    .zip(arguments.iter().cloned())
                    .collect();
                let call_environment = Gc::new(Environment::child(
                    captured_environment,
                    bindings,
                    receiver,
                    initialize_receiver,
                ));

                self.eval_expr(&definition.body.0, &call_environment)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use crate::{
        HostError, IntrinsicId, NativeObjectId, PropertyId, ResolvedProperty,
        parse::{Spanned, UserFunctionDef},
    };

    use super::*;

    struct TestHost {
        calls: usize,
    }

    struct MemberHost {
        resolutions: usize,
        gets: usize,
        sets: Vec<Value>,
    }

    impl Host for TestHost {
        fn resolve_property(
            &mut self,
            _object: NativeObjectId,
            _name: &Symbol,
        ) -> Result<Option<ResolvedProperty>, HostError> {
            unreachable!()
        }

        fn get_property(
            &mut self,
            _object: NativeObjectId,
            _property: PropertyId,
        ) -> Result<Value, HostError> {
            unreachable!()
        }

        fn set_property(
            &mut self,
            _object: NativeObjectId,
            _property: PropertyId,
            _value: &Value,
        ) -> Result<(), HostError> {
            unreachable!()
        }

        fn call_intrinsic(
            &mut self,
            _receiver: Option<NativeObjectId>,
            _intrinsic: IntrinsicId,
            arguments: &[Value],
        ) -> Result<Value, HostError> {
            self.calls += 1;
            Ok(arguments.first().cloned().unwrap_or(Value::Null))
        }
    }

    impl Host for MemberHost {
        fn resolve_property(
            &mut self,
            object: NativeObjectId,
            name: &Symbol,
        ) -> Result<Option<ResolvedProperty>, HostError> {
            assert_eq!(object, NativeObjectId::from_raw(10));
            self.resolutions += 1;
            Ok(match name.as_str() {
                "native" => Some(ResolvedProperty::new(PropertyId::from_raw(20), true)),
                "readonly" => Some(ResolvedProperty::new(PropertyId::from_raw(21), false)),
                _ => None,
            })
        }

        fn get_property(
            &mut self,
            object: NativeObjectId,
            property: PropertyId,
        ) -> Result<Value, HostError> {
            assert_eq!(object, NativeObjectId::from_raw(10));
            assert_eq!(property, PropertyId::from_raw(20));
            self.gets += 1;
            Ok(Value::Bool(true))
        }

        fn set_property(
            &mut self,
            object: NativeObjectId,
            property: PropertyId,
            value: &Value,
        ) -> Result<(), HostError> {
            assert_eq!(object, NativeObjectId::from_raw(10));
            assert_eq!(property, PropertyId::from_raw(20));
            self.sets.push(value.clone());
            Ok(())
        }

        fn call_intrinsic(
            &mut self,
            _receiver: Option<NativeObjectId>,
            _intrinsic: IntrinsicId,
            _arguments: &[Value],
        ) -> Result<Value, HostError> {
            unreachable!()
        }
    }

    fn empty_environment() -> Gc<Environment> {
        Gc::new(Environment::new_root())
    }

    fn eval_source(
        evaluator: &mut Evaluator<'_, '_>,
        environment: &Gc<Environment>,
        source: &str,
    ) -> Value {
        let (expression, _) = crate::parse::parse_thyme(source)
            .into_result()
            .unwrap_or_else(|errors| panic!("parse errors for {source:?}: {errors:#?}"));
        evaluator
            .eval_expr(&expression, environment)
            .unwrap_or_else(|error| panic!("evaluation error for {source:?}: {error}"))
    }

    fn eval_source_error(
        evaluator: &mut Evaluator<'_, '_>,
        environment: &Gc<Environment>,
        source: &str,
    ) -> String {
        let (expression, _) = crate::parse::parse_thyme(source)
            .into_result()
            .unwrap_or_else(|errors| panic!("parse errors for {source:?}: {errors:#?}"));
        evaluator.eval_expr(&expression, environment).unwrap_err()
    }

    fn echo_builtin(
        evaluator: &mut Evaluator<'_, '_>,
        arguments: &[Value],
        _call_span: Span,
    ) -> Result<Value, String> {
        assert!(evaluator.runtime.global("alloc").is_some());
        Ok(arguments[0].clone())
    }

    #[test]
    fn generic_builtin_descriptors_dispatch_without_the_host() {
        let runtime = Runtime::new();
        let mut host = TestHost { calls: 0 };
        let mut evaluator = Evaluator {
            runtime: &runtime,
            host: &mut host,
        };
        let function = Function::builtin("test.echo", 1, echo_builtin);
        let span: Span = (4..9).into();

        assert_eq!(function.name(), Some("test.echo"));
        assert_eq!(function.arity(), 1);
        assert_eq!(
            evaluator
                .call_function(&function, &[Value::Bool(true)], span)
                .unwrap(),
            Value::Bool(true)
        );
        let error = evaluator.call_function(&function, &[], span).unwrap_err();
        assert!(error.contains("Builtin test.echo expected 1 arguments, got 0"));
        drop(evaluator);
        assert_eq!(host.calls, 0);

        drop(function);
        dumpster::unsync::collect();
    }

    #[test]
    fn alloc_returns_a_fresh_empty_object_without_calling_the_host() {
        let runtime = Runtime::new();
        let mut host = TestHost { calls: 0 };
        let environment = empty_environment();
        let mut evaluator = Evaluator {
            runtime: &runtime,
            host: &mut host,
        };

        let Value::Object(first) = eval_source(&mut evaluator, &environment, "alloc") else {
            panic!("alloc did not return an object");
        };
        let Value::Object(second) = eval_source(&mut evaluator, &environment, "alloc") else {
            panic!("alloc did not return an object");
        };

        assert_ne!(Value::Object(first.clone()), Value::Object(second));
        assert_eq!(first.field("anything"), None);
        drop(evaluator);
        assert_eq!(host.calls, 0);
    }

    #[test]
    fn string_builtins_handle_unicode_strings_and_lists() {
        let runtime = Runtime::new();
        let mut host = TestHost { calls: 0 };
        let environment = empty_environment();
        let mut evaluator = Evaluator {
            runtime: &runtime,
            host: &mut host,
        };

        assert_eq!(
            eval_source(&mut evaluator, &environment, "string.length(\"é🙂\")"),
            Value::Number(Number::Int(2))
        );
        assert_eq!(
            eval_source(&mut evaluator, &environment, "string.length([1, 2, 3])"),
            Value::Number(Number::Int(3))
        );
        assert_eq!(
            eval_source(
                &mut evaluator,
                &environment,
                "string.split(\"a🙂b🙂\", \"🙂\")"
            ),
            Value::List(List::new([
                Value::Str("a".into()),
                Value::Str("b".into()),
                Value::Str("".into()),
            ]))
        );
        assert_eq!(
            eval_source(&mut evaluator, &environment, "string.str2list(\"é🙂\")"),
            Value::List(List::new(
                [Value::Str("é".into()), Value::Str("🙂".into()),]
            ))
        );
        drop(evaluator);
        assert_eq!(host.calls, 0);
    }

    #[test]
    fn string_builtins_report_type_delimiter_and_arity_errors() {
        let runtime = Runtime::new();
        let mut host = TestHost { calls: 0 };
        let environment = empty_environment();
        let mut evaluator = Evaluator {
            runtime: &runtime,
            host: &mut host,
        };

        let error = eval_source_error(&mut evaluator, &environment, "string.length(12)");
        assert!(error.contains("expected string or list"), "{error}");

        for delimiter in ["", "ab"] {
            let source = format!("string.split(\"abc\", \"{delimiter}\")");
            let error = eval_source_error(&mut evaluator, &environment, &source);
            assert!(error.contains("delimiter must be one character"), "{error}");
        }

        let error = eval_source_error(&mut evaluator, &environment, "string.str2list([1])");
        assert!(error.contains("expected string"), "{error}");

        let error = eval_source_error(&mut evaluator, &environment, "string.length()");
        assert!(error.contains("expected 1 arguments, got 0"), "{error}");
    }

    #[test]
    fn builtin_slots_are_read_only_but_can_be_shadowed_and_extended() {
        let runtime = Runtime::new();
        let mut host = TestHost { calls: 0 };
        let environment = empty_environment();
        let mut evaluator = Evaluator {
            runtime: &runtime,
            host: &mut host,
        };

        for source in [
            "alloc = 123",
            "alloc := 123",
            "string = 123",
            "string.split = 123",
            "string.str2list := 123",
            "set.insert = 123",
            "math.toBool := 123",
        ] {
            let error = eval_source_error(&mut evaluator, &environment, source);
            assert!(error.contains("Cannot set read-only"), "{source}: {error}");
        }

        assert_eq!(
            eval_source(
                &mut evaluator,
                &environment,
                "shadow := { alloc := 7; alloc }; shadow"
            ),
            Value::Number(Number::Int(7))
        );
        assert_eq!(
            eval_source(
                &mut evaluator,
                &environment,
                "string.custom = 12; string.custom"
            ),
            Value::Number(Number::Int(12))
        );
        assert_eq!(
            eval_source(&mut evaluator, &environment, "string.length(\"ok\")"),
            Value::Number(Number::Int(2))
        );
        assert_eq!(
            eval_source(
                &mut evaluator,
                &environment,
                "string.list2str([1, true, \"x\", [2]])"
            ),
            Value::Str("1truex[2]".into())
        );
    }

    #[test]
    fn symbol_and_dynamic_member_lookup_is_case_insensitive() {
        let runtime = Runtime::new();
        let mut host = TestHost { calls: 0 };
        let environment = empty_environment();
        let object = Object::new();
        environment.declare("Target", Value::Object(object.clone()));
        environment.declare(
            "Echo",
            Value::Function(Function::intrinsic(IntrinsicId::from_raw(6), "Echo", 1)),
        );
        let mut evaluator = Evaluator {
            runtime: &runtime,
            host: &mut host,
        };

        assert_eq!(
            eval_source(&mut evaluator, &environment, "a = 5; A = 6; a"),
            Value::Number(Number::Int(6))
        );
        assert_eq!(
            eval_source(
                &mut evaluator,
                &environment,
                "TARGET.SomeField = 9; target.somefield"
            ),
            Value::Number(Number::Int(9))
        );
        assert_eq!(
            object.field("SOMEFIELD"),
            Some(Value::Number(Number::Int(9)))
        );
        assert_eq!(
            eval_source(&mut evaluator, &environment, "STRING.LIST2STR([1])"),
            Value::Str("1".into())
        );
        assert_eq!(
            eval_source(&mut evaluator, &environment, "eCHO(true)"),
            Value::Bool(true)
        );
        drop(evaluator);
        assert_eq!(host.calls, 1);
    }

    #[test]
    fn set_builtins_use_thyme_number_equality_and_preserve_order() {
        let runtime = Runtime::new();
        let mut host = TestHost { calls: 0 };
        let environment = empty_environment();
        let mut evaluator = Evaluator {
            runtime: &runtime,
            host: &mut host,
        };

        assert_eq!(
            eval_source(
                &mut evaluator,
                &environment,
                "set.insert([2147483647], 2147483647.5)"
            ),
            Value::List(List::new([Value::Number(Number::Int(i32::MAX))]))
        );
        assert_eq!(
            eval_source(
                &mut evaluator,
                &environment,
                "set.insert([2147483647.0], 2147483648)"
            ),
            Value::List(List::new([Value::Number(Number::Float(2147483648.0))]))
        );
        assert_eq!(
            eval_source(
                &mut evaluator,
                &environment,
                "set.merge([1, 1], [1.0, 2, 2])"
            ),
            Value::List(List::new([1.into(), 1.into(), 2.into()]))
        );
        assert_eq!(
            eval_source(
                &mut evaluator,
                &environment,
                "set.merge([], [\"a\", \"a\", \"b\"])"
            ),
            Value::List(List::new([Value::Str("a".into()), Value::Str("b".into()),]))
        );
        assert!(
            eval_source_error(&mut evaluator, &environment, "set.insert([\"a\"], 1)")
                .contains("incompatible types")
        );
        assert!(
            eval_source_error(&mut evaluator, &environment, "set.merge([1], [\"a\"])")
                .contains("incompatible types")
        );
    }

    #[test]
    fn math_to_bool_maps_nested_lists() {
        let runtime = Runtime::new();
        let mut host = TestHost { calls: 0 };
        let environment = empty_environment();
        let mut evaluator = Evaluator {
            runtime: &runtime,
            host: &mut host,
        };

        assert_eq!(
            eval_source(
                &mut evaluator,
                &environment,
                "math.toBool([2, [1, 0], -0.0, \"FALSE\"])"
            ),
            Value::List(List::new([
                Value::Bool(true),
                Value::List(List::new([Value::Bool(true), Value::Bool(false)])),
                Value::Bool(false),
                Value::Bool(false),
            ]))
        );
        assert_eq!(
            eval_source(&mut evaluator, &environment, "math.toBool(\"TrUe\")"),
            Value::Bool(true)
        );
        assert!(
            eval_source_error(&mut evaluator, &environment, "math.toBool(\"yes\")")
                .contains("invalid value")
        );
    }

    #[test]
    fn math_float_functions_map_nested_lists() {
        let runtime = Runtime::new();
        let mut host = TestHost { calls: 0 };
        let environment = empty_environment();
        let mut evaluator = Evaluator {
            runtime: &runtime,
            host: &mut host,
        };

        for (name, expected) in [
            ("acos", 1.0_f32.acos()),
            ("asin", 1.0_f32.asin()),
            ("atan", 1.0_f32.atan()),
            ("cos", 1.0_f32.cos()),
            ("log", 1.0_f32.ln()),
            ("log10", 1.0_f32.log10()),
            ("sin", 1.0_f32.sin()),
            ("sqrt", 1.0_f32.sqrt()),
            ("tan", 1.0_f32.tan()),
        ] {
            assert_eq!(
                eval_source(&mut evaluator, &environment, &format!("math.{name}(1.0)")),
                Value::Number(Number::Float(expected))
            );
        }
        assert_eq!(
            eval_source(&mut evaluator, &environment, "math.sqrt([4, [9.0]])"),
            Value::List(List::new([
                Value::Number(Number::Float(2.0)),
                Value::List(List::new([Value::Number(Number::Float(3.0))])),
            ]))
        );
        assert_eq!(
            eval_source(&mut evaluator, &environment, "math.atan2(1, 0.0)"),
            Value::Number(Number::Float(1.0_f32.atan2(0.0)))
        );
        assert_eq!(
            eval_source(&mut evaluator, &environment, "math.sin(1)"),
            Value::Number(Number::Float(1.0_f32.sin()))
        );
        assert!(
            eval_source_error(&mut evaluator, &environment, "math.atan2([1.0], [0.0])")
                .contains("expected floats")
        );
    }

    #[test]
    fn color_builtins_convert_float_triplets_and_preserve_alpha() {
        let runtime = Runtime::new();
        let mut host = TestHost { calls: 0 };
        let environment = empty_environment();
        let mut evaluator = Evaluator {
            runtime: &runtime,
            host: &mut host,
        };

        for (source, expected) in [
            ("math.HSL2RGB([0, 1, 0.5])", [1.0, 0.0, 0.0]),
            ("math.HSV2RGB([120.0, 1.0, 1.0])", [0.0, 1.0, 0.0]),
            ("math.RGB2HSV([1.0, 0.0, 0.0])", [0.0, 1.0, 1.0]),
        ] {
            assert_eq!(
                eval_source(&mut evaluator, &environment, source),
                Value::List(List::new(
                    expected.map(|value| Value::Number(Number::Float(value)))
                ))
            );
        }
        assert_eq!(
            eval_source(
                &mut evaluator,
                &environment,
                "math.RGB2HSL([0.0, 0.0, 1.0, 0.25])"
            ),
            Value::List(List::new(
                [240.0, 1.0, 0.5, 0.25].map(|value| Value::Number(Number::Float(value)))
            ))
        );
        assert!(
            eval_source_error(&mut evaluator, &environment, "math.HSL2RGB([0, true, 1])")
                .contains("expected float[3 or 4]")
        );
    }

    #[test]
    fn intrinsic_calls_check_arity_and_dispatch_to_the_host() {
        let runtime = Runtime::new();
        let mut host = TestHost { calls: 0 };
        let mut evaluator = Evaluator {
            runtime: &runtime,
            host: &mut host,
        };
        let function = Function::intrinsic(IntrinsicId::from_raw(4), "identity", 1);
        let span: Span = (3..8).into();

        let result = evaluator
            .call_function(&function, &[Value::Bool(true)], span)
            .unwrap();
        assert!(matches!(result, Value::Bool(true)));

        let error = evaluator.call_function(&function, &[], span).unwrap_err();
        assert!(error.contains("expected 1 arguments, got 0"));
        drop(evaluator);
        assert_eq!(host.calls, 1);
    }

    #[test]
    fn native_methods_pass_their_receiver_to_the_host() {
        struct ReceiverHost(NativeObjectId);

        impl Host for ReceiverHost {
            fn resolve_property(
                &mut self,
                _object: NativeObjectId,
                _name: &Symbol,
            ) -> Result<Option<ResolvedProperty>, HostError> {
                unreachable!()
            }

            fn get_property(
                &mut self,
                _object: NativeObjectId,
                _property: PropertyId,
            ) -> Result<Value, HostError> {
                unreachable!()
            }

            fn set_property(
                &mut self,
                _object: NativeObjectId,
                _property: PropertyId,
                _value: &Value,
            ) -> Result<(), HostError> {
                unreachable!()
            }

            fn call_intrinsic(
                &mut self,
                receiver: Option<NativeObjectId>,
                _intrinsic: IntrinsicId,
                _arguments: &[Value],
            ) -> Result<Value, HostError> {
                assert_eq!(receiver, Some(self.0));
                Ok(Value::Bool(true))
            }
        }

        let runtime = Runtime::new();
        let object = NativeObjectId::from_raw(42);
        let mut host = ReceiverHost(object);
        let mut evaluator = Evaluator {
            runtime: &runtime,
            host: &mut host,
        };
        assert_eq!(
            evaluator
                .call_function(
                    &Function::method(object, IntrinsicId::from_raw(7), "method", 0),
                    &[],
                    (0..0).into(),
                )
                .unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn runtime_eval_uses_its_persistent_global_environment() {
        let runtime = Runtime::new();
        let mut host = TestHost { calls: 0 };

        runtime.eval(&mut host, "Answer = 41").unwrap();
        assert_eq!(
            runtime.eval(&mut host, "answer + 1").unwrap(),
            Value::Number(Number::Int(42))
        );
    }

    #[test]
    fn collapse_calls_zero_argument_host_intrinsics() {
        let runtime = Runtime::new();
        let mut host = TestHost { calls: 0 };
        let environment = empty_environment();
        environment.declare(
            "host_value",
            Value::Function(Function::intrinsic(
                IntrinsicId::from_raw(5),
                "host_value",
                0,
            )),
        );
        let mut evaluator = Evaluator {
            runtime: &runtime,
            host: &mut host,
        };

        assert_eq!(
            eval_source(&mut evaluator, &environment, "host_value"),
            Value::Null
        );
        drop(evaluator);
        assert_eq!(host.calls, 1);
    }

    #[test]
    fn user_calls_bind_arguments_over_the_captured_environment() {
        let runtime = Runtime::new();
        let mut host = TestHost { calls: 0 };
        let span: Span = (0..1).into();
        let body: Spanned<Expr> = (Expr::Symbol(Symbol::from("argument")), span);
        let function = Function(Gc::new(FunctionValue::User(UserFunction {
            definition: UserFunctionDef {
                params: Rc::from([Symbol::from("argument")]),
                body: Rc::new(body),
            },
            env: empty_environment(),
        })));
        let mut evaluator = Evaluator {
            runtime: &runtime,
            host: &mut host,
        };

        let result = evaluator
            .call_function(&function, &[Value::Bool(true)], span)
            .unwrap();
        assert!(matches!(result, Value::Bool(true)));
    }

    #[test]
    fn initializers_declare_existing_members_but_keep_missing_names_local() {
        let runtime = Runtime::new();
        let mut host = TestHost { calls: 0 };
        let Value::Function(builder) = runtime
            .eval(&mut host, "{ value := 2; missing := 3 }")
            .unwrap()
        else {
            panic!("expected function");
        };
        let object = Object::new();
        object.set_field("value", Value::from(1));

        runtime
            .call_initializer(&mut host, &builder, &[], object.clone())
            .unwrap();

        assert_eq!(object.field("value"), Some(Value::from(2)));
        assert_eq!(object.field("missing"), None);
    }

    #[test]
    fn member_reads_native_properties_then_fall_back_to_dynamic_fields() {
        let runtime = Runtime::new();
        let mut host = MemberHost {
            resolutions: 0,
            gets: 0,
            sets: Vec::new(),
        };
        let span: Span = (0..6).into();
        let object = crate::Object::native(NativeObjectId::from_raw(10));
        object.set_field("dynamic", Value::Bool(false));
        let environment = Gc::new(Environment::child(
            Gc::new(Environment::new_root()),
            HashMap::from([(Symbol::from("object"), Value::Object(object))]),
            None,
            false,
        ));
        let mut evaluator = Evaluator {
            runtime: &runtime,
            host: &mut host,
        };

        let native = Expr::Member(
            Box::new((Expr::Symbol(Symbol::from("object")), span)),
            (Symbol::from("native"), span),
        );
        assert!(matches!(
            evaluator.eval_expr(&native, &environment).unwrap(),
            Value::Bool(true)
        ));

        let dynamic = Expr::Member(
            Box::new((Expr::Symbol(Symbol::from("object")), span)),
            (Symbol::from("dynamic"), span),
        );
        assert!(matches!(
            evaluator.eval_expr(&dynamic, &environment).unwrap(),
            Value::Bool(false)
        ));

        drop(evaluator);
        assert_eq!(host.resolutions, 2);
        assert_eq!(host.gets, 1);
        assert!(host.sets.is_empty());
    }

    #[test]
    fn with_scope_only_routes_existing_members_to_the_receiver() {
        let runtime = Runtime::new();
        let mut host = TestHost { calls: 0 };
        let environment = empty_environment();
        let object = Object::new();
        object.set_field("x", Value::Undefined);
        object.set_field("copy", Value::Undefined);
        object.set_field("closure", Value::Undefined);
        object.set_field("captured", Value::Undefined);
        environment.declare("object", Value::Object(object.clone()));
        environment.declare("x", Value::Number(Number::Int(99)));
        environment.declare("y", Value::Number(Number::Int(100)));
        environment.declare("global_value", Value::Number(Number::Int(11)));
        let mut evaluator = Evaluator {
            runtime: &runtime,
            host: &mut host,
        };

        eval_source(
            &mut evaluator,
            &environment,
            "object -> { \
                x = 5; \
                x := 6; \
                y := 7; \
                y = 8; \
                copy = global_value; \
                closure = { captured = y }; \
                missing = 12; \
            }",
        );
        eval_source(&mut evaluator, &environment, "object.closure");

        assert_eq!(object.field("x"), Some(Value::Number(Number::Int(5))));
        assert_eq!(object.field("copy"), Some(Value::Number(Number::Int(11))));
        assert_eq!(object.field("y"), None);
        assert_eq!(object.field("missing"), None);
        assert_eq!(
            object.field("captured"),
            Some(Value::Number(Number::Int(8)))
        );
        assert_eq!(
            environment.local(&Symbol::from("x")),
            Some(Value::Number(Number::Int(99)))
        );
        assert_eq!(
            environment.local(&Symbol::from("y")),
            Some(Value::Number(Number::Int(100)))
        );
    }

    #[test]
    fn with_scope_can_update_but_not_implicitly_create_receiver_fields() {
        let runtime = Runtime::new();
        let mut host = TestHost { calls: 0 };
        let environment = empty_environment();
        let object = Object::new();
        environment.declare("o", Value::Object(object.clone()));
        let mut evaluator = Evaluator {
            runtime: &runtime,
            host: &mut host,
        };

        eval_source(&mut evaluator, &environment, "o -> { foobar = 1 }");
        assert_eq!(object.field("foobar"), None);

        eval_source(
            &mut evaluator,
            &environment,
            "o.foobar = 123; o -> { foobar = 1 }",
        );
        assert_eq!(object.field("foobar"), Some(Value::from(1)));
    }

    #[test]
    fn explicit_member_assignments_share_native_and_dynamic_write_paths() {
        let runtime = Runtime::new();
        let mut host = MemberHost {
            resolutions: 0,
            gets: 0,
            sets: Vec::new(),
        };
        let environment = empty_environment();
        let object = Object::native(NativeObjectId::from_raw(10));
        environment.declare("object", Value::Object(object.clone()));
        let mut evaluator = Evaluator {
            runtime: &runtime,
            host: &mut host,
        };

        eval_source(
            &mut evaluator,
            &environment,
            "object.dynamic := 3; object.dynamic = 4; object.native = { true }",
        );
        assert_eq!(object.field("dynamic"), Some(Value::Number(Number::Int(4))));
        assert_eq!(runtime.binding_count(), 1);

        eval_source(
            &mut evaluator,
            &environment,
            "object.native := 12; object -> { native = 13; dynamic = 5; missing = 1; local := 7 }",
        );
        assert_eq!(runtime.binding_count(), 0);
        assert_eq!(object.field("dynamic"), Some(Value::from(5)));
        assert_eq!(object.field("missing"), None);
        assert!(
            eval_source_error(&mut evaluator, &environment, "object.readonly = { true }")
                .contains("read-only member readonly")
        );
        assert_eq!(runtime.binding_count(), 0);
        drop(evaluator);
        assert_eq!(
            host.sets,
            vec![
                Value::Number(Number::Int(12)),
                Value::Number(Number::Int(13))
            ]
        );
        assert_eq!(object.field("local"), None);
    }
}
