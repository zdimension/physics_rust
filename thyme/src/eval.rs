use std::{cell::RefCell, collections::HashMap};

use dumpster::unsync::Gc;

use crate::{
    Environment, Function, FunctionValue, Host, List, Runtime, Symbol, UserFunction, Value,
    parse::{BinaryOp, Expr, Literal, Number, Span, UnaryOp},
};

pub struct Evaluator<'runtime, 'host> {
    pub runtime: &'runtime Runtime,
    pub host: &'host mut dyn Host,
}

impl<'runtime, 'host> Evaluator<'runtime, 'host> {
    /// if the value is a zero-parameter function, call it and return the result, otherwise return the value as-is
    fn collapse(&mut self, value: Value) -> Result<Value, String> {
        if let Value::Function(function) = &value {
            if let FunctionValue::User(user) = &*function.0 {
                if user.definition.params.is_empty() {
                    return self.call_function(function, &[], user.definition.body.1);
                }
            }
        }
        Ok(value)
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
            Expr::Symbol(sym) => match env.get(sym) {
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

                let value = if let Some(object_id) = object.native_id() {
                    match self
                        .host
                        .resolve_property(object_id, member.as_str())
                        .map_err(|error| {
                            format!(
                                "Failed to resolve native property {member} at {member_span:?}: \
                                 {error}"
                            )
                        })? {
                        Some(property_id) => self
                            .host
                            .get_property(object_id, property_id)
                            .map_err(|error| {
                                format!(
                                    "Failed to get native property {member} at {member_span:?}: \
                                     {error}"
                                )
                            })?,
                        None => object.field(member.as_str()).unwrap_or(Value::Undefined),
                    }
                } else {
                    object.field(member.as_str()).unwrap_or(Value::Undefined)
                };

                self.collapse(value)?
            }
            Expr::Call(function_expr, (argument_exprs, call_span)) => {
                let function_value = self.eval_expr(&function_expr.0, env)?;
                let Value::Function(function) = function_value else {
                    return Err(format!(
                        "Cannot call non-function value {function_value} at {call_span:?}"
                    ));
                };
                let arguments = argument_exprs
                    .iter()
                    .map(|(argument, _)| self.eval_expr(argument, env))
                    .collect::<Result<Vec<_>, _>>()?;
                self.call_function(&function, &arguments, *call_span)?
            }
            Expr::Ternary(cond, then_expr, else_expr) => {
                let cond_value = self.eval_expr(&cond.0, env)?;
                let Value::Bool(cond_bool) = cond_value else {
                    return Err(format!(
                        "Cannot use non-boolean value {cond_value} as a condition"
                    ));
                };
                if cond_bool {
                    self.eval_expr(&then_expr.0, env)?
                } else {
                    self.eval_expr(&else_expr.0, env)?
                }
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
            Expr::Binary(left, op, right) => {
                let left_value = self.eval_expr(&left.0, env)?;
                let right_value = self.eval_expr(&right.0, env)?;
                let (left_value, right_value) =
                    (self.collapse(left_value)?, self.collapse(right_value)?);

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

                    BinaryOp::ClassAssign => todo!(),

                    BinaryOp::Assign => todo!(),
                    BinaryOp::Declare => todo!(),
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
    fn apply_unary(
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
        match &*function.0 {
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
                    .call_intrinsic(intrinsic.id, arguments)
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
                let call_environment = Gc::new(Environment {
                    parent: Some(captured_environment),
                    bindings: RefCell::new(bindings),
                });

                self.eval_expr(&definition.body.0, &call_environment)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use crate::{
        HostError, IntrinsicId, NativeObjectId, PropertyId,
        parse::{Spanned, UserFunctionDef},
    };

    use super::*;

    struct TestHost {
        calls: usize,
    }

    struct MemberHost {
        resolutions: usize,
        gets: usize,
    }

    impl Host for TestHost {
        fn resolve_property(
            &mut self,
            _object: NativeObjectId,
            _name: &str,
        ) -> Result<Option<PropertyId>, HostError> {
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
            _intrinsic: IntrinsicId,
            arguments: &[Value],
        ) -> Result<Value, HostError> {
            self.calls += 1;
            Ok(arguments[0].clone())
        }
    }

    impl Host for MemberHost {
        fn resolve_property(
            &mut self,
            object: NativeObjectId,
            name: &str,
        ) -> Result<Option<PropertyId>, HostError> {
            assert_eq!(object, NativeObjectId::from_raw(10));
            self.resolutions += 1;
            Ok((name == "native").then(|| PropertyId::from_raw(20)))
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
            _object: NativeObjectId,
            _property: PropertyId,
            _value: &Value,
        ) -> Result<(), HostError> {
            unreachable!()
        }

        fn call_intrinsic(
            &mut self,
            _intrinsic: IntrinsicId,
            _arguments: &[Value],
        ) -> Result<Value, HostError> {
            unreachable!()
        }
    }

    fn empty_environment() -> Gc<Environment> {
        Gc::new(Environment {
            parent: None,
            bindings: RefCell::new(HashMap::new()),
        })
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
    fn member_reads_native_properties_then_fall_back_to_dynamic_fields() {
        let runtime = Runtime::new();
        let mut host = MemberHost {
            resolutions: 0,
            gets: 0,
        };
        let span: Span = (0..6).into();
        let object = crate::Object::native(NativeObjectId::from_raw(10));
        object.set_field("dynamic", Value::Bool(false));
        let environment = Gc::new(Environment {
            parent: None,
            bindings: RefCell::new(HashMap::from([(
                Symbol::from("object"),
                Value::Object(object),
            )])),
        });
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
    }
}
