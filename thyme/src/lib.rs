use chumsky::input::*;
use chumsky::pratt::*;
use chumsky::prelude::*;
use dumpster::{Trace, TraceWith, Visitor, unsync::Gc};
use logos::Logos;
use std::borrow::{Borrow, Cow};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::{self, Debug, Display};
use std::rc::Rc;

/// A Thyme number. Algodoo keeps integers and floats as distinct runtime types.
#[derive(Copy, Clone, Debug, PartialEq, Trace)]
pub enum Number {
    Int(i32),
    Float(f32),
}

/// An internable, cheaply cloned Thyme identifier.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Trace)]
pub struct Symbol(Rc<str>);

impl Symbol {
    pub fn new(value: impl AsRef<str>) -> Self {
        Self(Rc::from(value.as_ref()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for Symbol {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for Symbol {
    fn from(value: String) -> Self {
        Self(Rc::from(value))
    }
}

impl AsRef<str> for Symbol {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for Symbol {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

macro_rules! opaque_id {
    ($name:ident) => {
        #[repr(transparent)]
        #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Trace)]
        pub struct $name(u64);

        impl $name {
            pub const fn from_raw(raw: u64) -> Self {
                Self(raw)
            }

            pub const fn into_raw(self) -> u64 {
                self.0
            }
        }
    };
}

opaque_id!(NativeObjectId);
opaque_id!(IntrinsicId);
opaque_id!(PropertyId);

/// An immutable Thyme list, shared by the garbage collector.
#[derive(Clone, Trace)]
pub struct List(Gc<[Value]>);

impl List {
    pub fn new(values: impl IntoIterator<Item = Value>) -> Self {
        Self(values.into_iter().collect())
    }

    pub fn as_slice(&self) -> &[Value] {
        &self.0
    }
}

impl Default for List {
    fn default() -> Self {
        Self::new([])
    }
}

impl From<Vec<Value>> for List {
    fn from(values: Vec<Value>) -> Self {
        Self(Gc::from(values))
    }
}

impl Debug for List {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("List").field("len", &self.0.len()).finish()
    }
}

/// A cloneable handle to a dynamic Thyme object.
#[derive(Clone, Trace)]
pub struct Object(Gc<ClassObject>);

impl Object {
    pub fn new() -> Self {
        Self(Gc::new(ClassObject {
            fields: RefCell::new(HashMap::new()),
            native: None,
        }))
    }

    pub fn native(id: NativeObjectId) -> Self {
        Self(Gc::new(ClassObject {
            fields: RefCell::new(HashMap::new()),
            native: Some(id),
        }))
    }

    pub fn native_id(&self) -> Option<NativeObjectId> {
        self.0.native
    }

    pub fn field(&self, name: &str) -> Option<Value> {
        self.0.fields.borrow().get(name).cloned()
    }

    pub fn set_field(&self, name: impl Into<Symbol>, value: Value) -> Option<Value> {
        self.0.fields.borrow_mut().insert(name.into(), value)
    }

    pub fn remove_field(&self, name: &str) -> Option<Value> {
        self.0.fields.borrow_mut().remove(name)
    }
}

impl Default for Object {
    fn default() -> Self {
        Self::new()
    }
}

impl Debug for Object {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Object")
            .field("native", &self.native_id())
            .finish_non_exhaustive()
    }
}

/// A cloneable handle to either a user function or a host-provided intrinsic.
#[derive(Clone, Trace)]
pub struct Function(Gc<FunctionValue>);

impl Function {
    pub fn intrinsic(id: IntrinsicId, name: impl AsRef<str>, arity: usize) -> Self {
        Self(Gc::new(FunctionValue::Intrinsic(IntrinsicFunction {
            name: Rc::from(name.as_ref()),
            arity,
            id,
        })))
    }

    pub fn name(&self) -> Option<&str> {
        match &*self.0 {
            FunctionValue::User(_) => None,
            FunctionValue::Intrinsic(intrinsic) => Some(&intrinsic.name),
        }
    }

    pub fn arity(&self) -> usize {
        match &*self.0 {
            FunctionValue::User(user) => user.definition.params.len(),
            FunctionValue::Intrinsic(intrinsic) => intrinsic.arity,
        }
    }

    pub fn intrinsic_id(&self) -> Option<IntrinsicId> {
        match &*self.0 {
            FunctionValue::User(_) => None,
            FunctionValue::Intrinsic(intrinsic) => Some(intrinsic.id),
        }
    }
}

impl Debug for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &*self.0 {
            FunctionValue::User(user) => f
                .debug_struct("Function")
                .field("kind", &"user")
                .field("arity", &user.definition.params.len())
                .finish(),
            FunctionValue::Intrinsic(intrinsic) => f
                .debug_struct("Function")
                .field("kind", &"intrinsic")
                .field("name", &intrinsic.name)
                .field("arity", &intrinsic.arity)
                .field("id", &intrinsic.id)
                .finish(),
        }
    }
}

/// A value visible to Thyme programs and host implementations.
#[derive(Clone, Trace)]
pub enum Value {
    Null,
    Void,
    Bool(bool),
    Number(Number),
    Str(Rc<str>),
    List(List),
    Object(Object),
    Function(Function),
}

impl Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => f.write_str("Null"),
            Self::Void => f.write_str("Void"),
            Self::Bool(value) => f.debug_tuple("Bool").field(value).finish(),
            Self::Number(value) => f.debug_tuple("Number").field(value).finish(),
            Self::Str(value) => f.debug_tuple("Str").field(value).finish(),
            Self::List(value) => Debug::fmt(value, f),
            Self::Object(value) => Debug::fmt(value, f),
            Self::Function(value) => Debug::fmt(value, f),
        }
    }
}

#[derive(Trace)]
struct ClassObject {
    fields: RefCell<HashMap<Symbol, Value>>,
    native: Option<NativeObjectId>,
}

#[derive(Trace)]
struct Environment {
    parent: Option<Gc<Environment>>,
    bindings: RefCell<HashMap<Symbol, Value>>,
}

#[cfg(test)]
thread_local! {
    static ENVIRONMENT_DROPS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
impl Drop for Environment {
    fn drop(&mut self) {
        ENVIRONMENT_DROPS.set(ENVIRONMENT_DROPS.get() + 1);
    }
}

#[derive(Trace)]
enum FunctionValue {
    User(UserFunction),
    Intrinsic(IntrinsicFunction),
}

#[derive(Trace)]
struct IntrinsicFunction {
    name: Rc<str>,
    arity: usize,
    id: IntrinsicId,
}

#[derive(Clone, Debug)]
struct UserFunctionDef {
    params: Rc<[Symbol]>,
    body: Rc<Spanned<Expr>>,
}

struct UserFunction {
    definition: UserFunctionDef,
    env: Gc<Environment>,
}

// SAFETY: `UserFunctionDef` contains no `Gc` pointers. The captured environment is the only field
// that participates in the collector's object graph.
unsafe impl<V: Visitor> TraceWith<V> for UserFunction {
    fn accept(&self, visitor: &mut V) -> Result<(), ()> {
        self.env.accept(visitor)
    }
}

/// The category of a failure reported by the game-side host.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HostErrorKind {
    UnknownObject,
    UnknownProperty,
    InvalidType,
    Intrinsic,
    Other,
}

/// A structured error crossing the host/runtime boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostError {
    kind: HostErrorKind,
    message: Rc<str>,
}

impl HostError {
    pub fn new(kind: HostErrorKind, message: impl AsRef<str>) -> Self {
        Self {
            kind,
            message: Rc::from(message.as_ref()),
        }
    }

    pub fn kind(&self) -> HostErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for HostError {}

/// The Bevy-independent interface implemented by the embedding application.
///
/// This trait is intentionally neither `Send` nor `Sync`: a Thyme runtime is confined to one
/// thread, and the host may query a thread-local game world.
pub trait Host {
    fn resolve_property(
        &mut self,
        object: NativeObjectId,
        name: &str,
    ) -> Result<Option<PropertyId>, HostError>;

    fn get_property(
        &mut self,
        object: NativeObjectId,
        property: PropertyId,
    ) -> Result<Value, HostError>;

    fn set_property(
        &mut self,
        object: NativeObjectId,
        property: PropertyId,
        value: &Value,
    ) -> Result<(), HostError>;

    fn call_intrinsic(
        &mut self,
        intrinsic: IntrinsicId,
        arguments: &[Value],
    ) -> Result<Value, HostError>;
}

/// The roots and host-property bindings owned by one Thyme interpreter.
///
/// The embedding application controls object evaluation order. In particular, the Bevy adapter
/// can visit objects in z-order and request each object's bindings separately. Properties within
/// one object are stored in a `HashMap`, so their evaluation order is intentionally unspecified.
#[derive(Trace)]
pub struct Runtime {
    globals: Gc<Environment>,
    native_bindings: RefCell<HashMap<NativeObjectId, HashMap<PropertyId, Function>>>,
}

impl Runtime {
    pub fn new() -> Self {
        Self {
            globals: Gc::new(Environment {
                parent: None,
                bindings: RefCell::new(HashMap::new()),
            }),
            native_bindings: RefCell::new(HashMap::new()),
        }
    }

    pub fn global(&self, name: &str) -> Option<Value> {
        self.globals.bindings.borrow().get(name).cloned()
    }

    pub fn set_global(&self, name: impl Into<Symbol>, value: Value) -> Option<Value> {
        self.globals
            .bindings
            .borrow_mut()
            .insert(name.into(), value)
    }

    /// Creates or replaces the function bound to a native property.
    pub fn bind_property(
        &self,
        object: NativeObjectId,
        property: PropertyId,
        function: Function,
    ) -> Option<Function> {
        self.native_bindings
            .borrow_mut()
            .entry(object)
            .or_default()
            .insert(property, function)
    }

    pub fn property_binding(
        &self,
        object: NativeObjectId,
        property: PropertyId,
    ) -> Option<Function> {
        self.native_bindings
            .borrow()
            .get(&object)
            .and_then(|properties| properties.get(&property))
            .cloned()
    }

    /// Returns a stable snapshot of one object's bindings for this frame.
    ///
    /// The host chooses when to call this method and therefore controls object order. The order of
    /// entries in the returned vector is unspecified. Taking a snapshot permits the evaluator to
    /// remove a binding after an evaluation or setter failure without borrowing the binding table
    /// for the duration of user code.
    pub fn property_bindings_for(&self, object: NativeObjectId) -> Vec<(PropertyId, Function)> {
        self.native_bindings
            .borrow()
            .get(&object)
            .map(|properties| {
                properties
                    .iter()
                    .map(|(&property, function)| (property, function.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Applies the assignment behavior shared by all native properties.
    ///
    /// A function value creates or replaces a per-frame binding without calling the setter. Any
    /// other value first clears the old binding, then reaches the host setter. The host remains
    /// responsible for property-specific type checking.
    pub fn assign_native_property<H: Host + ?Sized>(
        &self,
        host: &mut H,
        object: NativeObjectId,
        property: PropertyId,
        value: Value,
    ) -> Result<(), HostError> {
        match value {
            Value::Function(function) => {
                self.bind_property(object, property, function);
                Ok(())
            }
            value => {
                self.unbind_property(object, property);
                host.set_property(object, property, &value)
            }
        }
    }

    /// Removes a binding, including after a future evaluation or host-setter failure.
    pub fn unbind_property(
        &self,
        object: NativeObjectId,
        property: PropertyId,
    ) -> Option<Function> {
        let mut bindings = self.native_bindings.borrow_mut();
        let removed = bindings
            .get_mut(&object)
            .and_then(|properties| properties.remove(&property));
        if bindings.get(&object).is_some_and(HashMap::is_empty) {
            bindings.remove(&object);
        }
        removed
    }

    pub fn binding_count(&self) -> usize {
        self.native_bindings
            .borrow()
            .values()
            .map(HashMap::len)
            .sum()
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Logos, Clone, Debug, PartialEq)]
#[logos(skip r"[ \t\r\n\f]+")]
#[logos(skip(r"//[^\r\n]*", allow_greedy = true))]
#[logos(skip r"/\*([^*]|\*[^/])*\*/")]
enum Token<'a> {
    Error,

    #[token("false", |_| false)]
    #[token("true", |_| true)]
    Bool(bool),

    #[token("null")]
    Null,

    #[token("=>")]
    LambdaArrow,

    #[token("{")]
    BraceOpen,

    #[token("}")]
    BraceClose,

    #[token("[")]
    BracketOpen,

    #[token("]")]
    BracketClose,

    #[token("(")]
    ParenOpen,

    #[token(")")]
    ParenClose,

    #[token(",")]
    Comma,

    #[token(";")]
    Semicolon,

    #[token(".")]
    Dot,

    #[regex(r"(>=|<=|==|!=|&&|\|\||\+\+|\.\.|:=|->)", priority = 3)]
    #[regex(r"[+\-*/%&|^!<>?=:]")]
    Op(&'a str),

    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*")]
    Ident(&'a str),

    #[regex(r"[0-9]+", |lex| match lex.slice().parse() {
        Ok(num) => Some(Number::Int(num)),
        Err(_) => Some(Number::Float(lex.slice().parse().unwrap())),
    }, priority = 3)]
    // * because Algodoo allows simply `0x` or `0b`.
    #[regex(r"0x[0-9a-fA-F]*", |lex| Number::Int(i32::from_str_radix(&lex.slice()[lex.slice().len().saturating_sub(8).max(2)..], 16).unwrap_or(0)), priority = 4)]
    #[regex(r"0b[01]*", |lex| Number::Int(i32::from_str_radix(&lex.slice()[lex.slice().len().saturating_sub(8).max(2)..], 2).unwrap_or(0)), priority = 4)]
    Int(Number),

    #[token("-inf", |_| f32::NEG_INFINITY)]
    #[token("+inf", |_| f32::INFINITY)]
    #[token("∞", |_| f32::INFINITY)]
    #[token("NaN", |_| f32::NAN)]
    #[regex(r"(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?", |lex| lex.slice().parse::<f32>().expect("float parse error: unexpected!"))]
    Float(f32),

    #[regex(r#""(?:[^"\\]|\\.)*""#, |lex| {
        let slice = lex.slice();
        if !slice.contains('\\') {
            Cow::Borrowed(&slice[1..slice.len() - 1])
        } else {
            let mut result = String::new();
            let mut chars = slice[1..slice.len() - 1].chars();
            while let Some(c) = chars.next() {
                if c == '\\' {
                    if let Some(escaped) = chars.next() {
                        match escaped {
                            'n' => result.push('\n'),
                            't' => result.push('\t'),
                            '\\' => result.push('\\'),
                            '"' => result.push('"'),
                            _ => {}
                        }
                    }
                } else {
                    result.push(c);
                }
            }
            Cow::Owned(result)
        }
    })]
    // Verbatim string literal, e.g. `@"C:\path\to\file.txt"`
    #[regex(r#"@"(?:[^"]|"")*""#, |lex| {
        let slice = lex.slice();
        let mut result = String::new();
        let mut chars = slice[2..slice.len() - 1].chars(); // this removes the leading @" and trailing "
        while let Some(c) = chars.next() {
            // because of the regex there can never be a lone double quote
            if c == '"' {
                if !matches!(chars.next(), Some('"')) {
                    unreachable!("unexpected lone double quote in verbatim string literal");
                }
                result.push('"');
            } else {
                result.push(c);
            }
        }
        Cow::Owned(result)
    })]
    Str(Cow<'a, str>),
}

impl<'a> Display for Token<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Token::Error => write!(f, "<error>"),
            Token::Bool(b) => write!(f, "{}", b),
            Token::Null => write!(f, "null"),
            Token::LambdaArrow => write!(f, "=>"),
            /*Token::Arrow => write!(f, "->"),
            Token::Declare => write!(f, ":="),
            Token::Assign => write!(f, "="),*/
            Token::BraceOpen => write!(f, "{{"),
            Token::BraceClose => write!(f, "}}"),
            Token::BracketOpen => write!(f, "["),
            Token::BracketClose => write!(f, "]"),
            Token::ParenOpen => write!(f, "("),
            Token::ParenClose => write!(f, ")"),
            Token::Comma => write!(f, ","),
            Token::Semicolon => write!(f, ";"),
            Token::Dot => write!(f, "."),
            Token::Op(op) => write!(f, "{}", op),
            Token::Ident(ident) => write!(f, "{}", ident),
            Token::Int(num) => match num {
                Number::Int(n) => write!(f, "{}", n),
                Number::Float(n) => write!(f, "{}", n),
            },
            Token::Float(flt) => write!(f, "{}", flt),
            Token::Str(s) => write!(f, "\"{}\"", s), // todo: escapes
        }
    }
}

fn read_auto_encoding(source: &[u8]) -> Cow<'_, str> {
    // try decoding utf8 then 1252
    match std::str::from_utf8(source) {
        Ok(s) => Cow::Borrowed(s),
        Err(_) => {
            let (cow, _, _) = encoding_rs::WINDOWS_1252.decode(source);
            cow
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_source(source: &str) -> Spanned<Expr> {
        let lexer = Token::lexer(source)
            .spanned()
            .map(|(token, span)| (token.unwrap_or(Token::Error), span.into()));
        let token_stream = Stream::from_iter(lexer)
            .map((0..source.len()).into(), |(token, span): (_, _)| {
                (token, span)
            });

        block_parser()
            .then_ignore(end())
            .parse(token_stream)
            .into_result()
            .unwrap_or_else(|errors| panic!("parse errors: {errors:#?}"))
    }

    fn single_expr(ast: &Spanned<Expr>) -> &Expr {
        match &ast.0 {
            Expr::Seq(exprs) if exprs.len() == 1 => &exprs[0].0,
            other => panic!("expected one expression, got {other:#?}"),
        }
    }

    fn assert_symbol(expr: &Spanned<Expr>, expected: &str) {
        match &expr.0 {
            Expr::Symbol(actual) => assert_eq!(actual.as_str(), expected),
            other => panic!("expected symbol {expected:?}, got {other:#?}"),
        }
    }

    fn assert_binary<'ast>(
        expr: &'ast Expr,
        expected: BinaryOp,
    ) -> (&'ast Spanned<Expr>, &'ast Spanned<Expr>) {
        match expr {
            Expr::Binary(lhs, actual, rhs) => {
                assert_eq!(*actual, expected);
                (lhs, rhs)
            }
            other => panic!("expected {expected:?}, got {other:#?}"),
        }
    }

    fn assert_unary(expr: &Expr, expected: UnaryOp) -> &Spanned<Expr> {
        match expr {
            Expr::Unary(actual, rhs) => {
                assert_eq!(*actual, expected);
                rhs
            }
            other => panic!("expected {expected:?}, got {other:#?}"),
        }
    }

    #[test]
    fn lexes_computer_phn_without_errors() {
        let source = read_auto_encoding(include_bytes!("../examples/computer.phn"));
        let mut lexer = Token::lexer(&source);

        while let Some(token) = lexer.next() {
            assert!(
                token.is_ok(),
                "lexer error at {:?}: {:?}",
                lexer.span(),
                lexer.slice()
            );
        }
    }

    #[test]
    fn parses_computer_phn_without_errors() {
        let source = read_auto_encoding(include_bytes!("../examples/computer.phn"));
        parse_source(&source);
    }

    fn parsed_number(source: &str) -> Number {
        let ast = parse_source(source);
        match single_expr(&ast) {
            Expr::Value(Literal::Number(number)) => *number,
            other => panic!("expected a number literal, got {other:#?}"),
        }
    }

    #[test]
    fn parsed_numbers_preserve_algodoo_integer_and_float_types() {
        assert_eq!(parsed_number("123"), Number::Int(123));
        assert!(matches!(parsed_number("2147483648"), Number::Float(value) if value.is_finite()));
        assert_eq!(parsed_number("0xff"), Number::Int(255));
        assert_eq!(parsed_number("0b101101"), Number::Int(45));
        assert_eq!(parsed_number("1.25"), Number::Float(1.25));
        assert_eq!(parsed_number("+inf"), Number::Float(f32::INFINITY));
        assert_eq!(parsed_number("-inf"), Number::Float(f32::NEG_INFINITY));
        assert!(matches!(parsed_number("NaN"), Number::Float(value) if value.is_nan()));
    }

    #[test]
    fn parsed_names_strings_and_parameters_are_owned() {
        let symbol = parse_source("some_name");
        match single_expr(&symbol) {
            Expr::Symbol(value) => assert_eq!(value.as_str(), "some_name"),
            other => panic!("expected symbol, got {other:#?}"),
        }

        let string = parse_source(r#""hello\nworld""#);
        match single_expr(&string) {
            Expr::Value(Literal::Str(value)) => assert_eq!(&**value, "hello\nworld"),
            other => panic!("expected string literal, got {other:#?}"),
        }

        let function = parse_source("(first, second) => { first }");
        match single_expr(&function) {
            Expr::Func(definition) => {
                assert_eq!(definition.params[0].as_str(), "first");
                assert_eq!(definition.params[1].as_str(), "second");
                let Expr::Seq(body) = &definition.body.0 else {
                    panic!(
                        "expected function body sequence, got {:#?}",
                        definition.body.0
                    );
                };
                assert_symbol(&body[0], "first");
            }
            other => panic!("expected function, got {other:#?}"),
        }
    }

    #[test]
    fn opaque_ids_round_trip_without_sharing_types() {
        let object = NativeObjectId::from_raw(7);
        let intrinsic = IntrinsicId::from_raw(7);
        let property = PropertyId::from_raw(7);

        assert_eq!(object.into_raw(), 7);
        assert_eq!(intrinsic.into_raw(), 7);
        assert_eq!(property.into_raw(), 7);
        assert_ne!(
            std::any::type_name_of_val(&object),
            std::any::type_name_of_val(&property)
        );
    }

    struct FakeHost {
        set: Option<(NativeObjectId, PropertyId, Value)>,
    }

    impl Host for FakeHost {
        fn resolve_property(
            &mut self,
            _object: NativeObjectId,
            name: &str,
        ) -> Result<Option<PropertyId>, HostError> {
            Ok((name == "position").then(|| PropertyId::from_raw(3)))
        }

        fn get_property(
            &mut self,
            _object: NativeObjectId,
            _property: PropertyId,
        ) -> Result<Value, HostError> {
            Ok(Value::Number(Number::Int(12)))
        }

        fn set_property(
            &mut self,
            object: NativeObjectId,
            property: PropertyId,
            value: &Value,
        ) -> Result<(), HostError> {
            self.set = Some((object, property, value.clone()));
            Ok(())
        }

        fn call_intrinsic(
            &mut self,
            _intrinsic: IntrinsicId,
            arguments: &[Value],
        ) -> Result<Value, HostError> {
            arguments
                .first()
                .cloned()
                .ok_or_else(|| HostError::new(HostErrorKind::Intrinsic, "expected one argument"))
        }
    }

    #[test]
    fn host_dispatch_preserves_number_types_and_unknown_properties_fall_back() {
        let object = NativeObjectId::from_raw(1);
        let property = PropertyId::from_raw(3);
        let mut host = FakeHost { set: None };

        assert_eq!(
            host.resolve_property(object, "position").unwrap(),
            Some(property)
        );
        assert_eq!(host.resolve_property(object, "dynamicField").unwrap(), None);
        assert!(matches!(
            host.get_property(object, property).unwrap(),
            Value::Number(Number::Int(12))
        ));

        let result = host
            .call_intrinsic(
                IntrinsicId::from_raw(5),
                &[Value::Number(Number::Float(2.5))],
            )
            .unwrap();
        assert!(matches!(result, Value::Number(Number::Float(2.5))));
    }

    #[test]
    fn runtime_centrally_replaces_and_clears_native_bindings() {
        let runtime = Runtime::new();
        let object = NativeObjectId::from_raw(10);
        let property = PropertyId::from_raw(20);
        let first = Function::intrinsic(IntrinsicId::from_raw(1), "first", 0);
        let second = Function::intrinsic(IntrinsicId::from_raw(2), "second", 0);

        assert!(runtime.bind_property(object, property, first).is_none());
        let replaced = runtime.bind_property(object, property, second).unwrap();
        assert_eq!(replaced.intrinsic_id(), Some(IntrinsicId::from_raw(1)));
        assert_eq!(runtime.binding_count(), 1);
        assert_eq!(
            runtime
                .property_binding(object, property)
                .unwrap()
                .intrinsic_id(),
            Some(IntrinsicId::from_raw(2))
        );
        let frame_snapshot = runtime.property_bindings_for(object);
        assert_eq!(frame_snapshot.len(), 1);
        assert_eq!(frame_snapshot[0].0, property);
        assert_eq!(
            frame_snapshot[0].1.intrinsic_id(),
            Some(IntrinsicId::from_raw(2))
        );
        assert!(
            runtime
                .property_bindings_for(NativeObjectId::from_raw(999))
                .is_empty()
        );

        let mut host = FakeHost { set: None };
        runtime
            .assign_native_property(
                &mut host,
                object,
                property,
                Value::Number(Number::Float(4.0)),
            )
            .unwrap();
        assert_eq!(runtime.binding_count(), 0);
        let (_, _, value) = host.set.unwrap();
        assert!(matches!(value, Value::Number(Number::Float(4.0))));
    }

    #[test]
    fn dumpster_collects_a_captured_environment_cycle() {
        let drops_before = ENVIRONMENT_DROPS.get();
        let environment = Gc::new(Environment {
            parent: None,
            bindings: RefCell::new(HashMap::new()),
        });
        let function = Function(Gc::new(FunctionValue::User(UserFunction {
            definition: UserFunctionDef {
                params: Rc::from([]),
                body: Rc::new((Expr::Error, Span::new((), 0..0))),
            },
            env: environment.clone(),
        })));
        environment
            .bindings
            .borrow_mut()
            .insert(Symbol::from("cycle"), Value::Function(function.clone()));

        drop(function);
        drop(environment);
        dumpster::unsync::collect();

        assert!(ENVIRONMENT_DROPS.get() > drops_before);
    }

    #[test]
    fn right_associative_operators_keep_their_ast_shape() {
        let assignment = parse_source("a = b := c");
        let Expr::Binary(a, BinaryOp::Assign, declaration) = single_expr(&assignment) else {
            panic!("expected assignment, got {:#?}", single_expr(&assignment));
        };
        assert_symbol(a, "a");
        let Expr::Binary(b, BinaryOp::Declare, c) = &declaration.0 else {
            panic!("expected declaration, got {:#?}", declaration.0);
        };
        assert_symbol(b, "b");
        assert_symbol(c, "c");

        let exponent = parse_source("a ^ b ^ c");
        let Expr::Binary(a, BinaryOp::Pow, rhs) = single_expr(&exponent) else {
            panic!("expected exponent, got {:#?}", single_expr(&exponent));
        };
        assert_symbol(a, "a");
        let Expr::Binary(b, BinaryOp::Pow, c) = &rhs.0 else {
            panic!("expected exponent, got {:#?}", rhs.0);
        };
        assert_symbol(b, "b");
        assert_symbol(c, "c");

        let ternary = parse_source("a ? b : c ? d : e");
        let Expr::Ternary(a, b, rhs) = single_expr(&ternary) else {
            panic!("expected ternary, got {:#?}", single_expr(&ternary));
        };
        assert_symbol(a, "a");
        assert_symbol(b, "b");
        let Expr::Ternary(c, d, e) = &rhs.0 else {
            panic!("expected ternary, got {:#?}", rhs.0);
        };
        assert_symbol(c, "c");
        assert_symbol(d, "d");
        assert_symbol(e, "e");
    }

    #[test]
    fn pratt_parser_uses_the_thyme_precedence_table() {
        let ast = parse_source("a || b && c .. d == e < f ++ g + h * -i ^ j");

        let (a, and) = assert_binary(single_expr(&ast), BinaryOp::Or);
        assert_symbol(a, "a");
        let (b, range) = assert_binary(&and.0, BinaryOp::And);
        assert_symbol(b, "b");
        let (c, equality) = assert_binary(&range.0, BinaryOp::Range);
        assert_symbol(c, "c");
        let (d, relation) = assert_binary(&equality.0, BinaryOp::Eq);
        assert_symbol(d, "d");
        let (e, concat) = assert_binary(&relation.0, BinaryOp::Less);
        assert_symbol(e, "e");
        let (f, sum) = assert_binary(&concat.0, BinaryOp::ListConcat);
        assert_symbol(f, "f");
        let (g, product) = assert_binary(&sum.0, BinaryOp::Add);
        assert_symbol(g, "g");
        let (h, negation) = assert_binary(&product.0, BinaryOp::Mul);
        assert_symbol(h, "h");
        let power = assert_unary(&negation.0, UnaryOp::Neg);
        let (i, j) = assert_binary(&power.0, BinaryOp::Pow);
        assert_symbol(i, "i");
        assert_symbol(j, "j");

        let subtraction = parse_source("a - b - c");
        let (lhs, c) = assert_binary(single_expr(&subtraction), BinaryOp::Sub);
        let (a, b) = assert_binary(&lhs.0, BinaryOp::Sub);
        assert_symbol(a, "a");
        assert_symbol(b, "b");
        assert_symbol(c, "c");
    }

    #[test]
    fn deeply_nested_functions_do_not_cause_exponential_backtracking() {
        let depth = 32;
        let source = format!("{}0{}", "{".repeat(depth), "}".repeat(depth));
        parse_source(&source);
    }
}

pub type Span = SimpleSpan;
pub type Spanned<T> = (T, Span);

// in increasing order of Algodoo precedence
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum BinaryOp {
    // < ->
    Assign,
    Declare,
    // < 0,
    ClassAssign,
    // 2 - <ternary goes here>
    // 3 - or
    Or,
    // 4 - and
    And,
    // 5 - range (declared in thyme.cfg using `infix`)
    Range,
    // 8 - eq comp
    Eq,
    NotEq,
    // 9 - rel comp
    Less,
    LessEq,
    Greater,
    GreaterEq,
    // 10 - list concat
    ListConcat,
    // 11 - addition
    Add,
    Sub,
    // 12 - multiplication
    Mul,
    Div,
    Mod,
    // 13 - <unary>
    // 14 - exponentiation (right associative)
    Pow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum UnaryOp {
    Not,
    Neg,
    Pos,
}

#[derive(Clone, Debug, PartialEq)]
enum Literal {
    Null,
    Bool(bool),
    Number(Number),
    Str(Rc<str>),
}

#[derive(Debug)]
enum Expr {
    Error,
    Value(Literal),
    List(Vec<Spanned<Self>>),
    Symbol(Symbol),
    /// a.b
    Member(Box<Spanned<Self>>, Spanned<Symbol>),
    /// f(a,b,c) or simply f a
    Call(Box<Spanned<Self>>, Spanned<Vec<Spanned<Self>>>),
    Binary(Box<Spanned<Self>>, BinaryOp, Box<Spanned<Self>>),
    Unary(UnaryOp, Box<Spanned<Self>>),
    // cond ? true_expr : false_expr
    Ternary(Box<Spanned<Self>>, Box<Spanned<Self>>, Box<Spanned<Self>>),
    /// (a,b,c)=>{...} or simply {...}
    Func(UserFunctionDef),
    /// a; b (optional trailing ;)
    Seq(Vec<Box<Spanned<Self>>>),
}

/*
block_inner
    = expression
      { ";" expression }
      [ ";" ]
    ;
     */
fn block_parser<'tokens, 'src: 'tokens, I>()
-> impl Parser<'tokens, I, Spanned<Expr>, extra::Err<Rich<'tokens, Token<'src>, Span>>> + Clone
where
    I: ValueInput<'tokens, Token = Token<'src>, Span = Span>,
{
    block_with_expr(expr_parser())
}

fn block_with_expr<'tokens, 'src: 'tokens, I, P>(
    expr: P,
) -> impl Parser<'tokens, I, Spanned<Expr>, extra::Err<Rich<'tokens, Token<'src>, Span>>> + Clone
where
    I: ValueInput<'tokens, Token = Token<'src>, Span = Span>,
    P: Parser<'tokens, I, Spanned<Expr>, extra::Err<Rich<'tokens, Token<'src>, Span>>> + Clone,
{
    expr.clone()
        .separated_by(just(Token::Semicolon))
        .allow_trailing()
        .collect::<Vec<_>>()
        .map_with(|stmts, e| {
            (
                Expr::Seq(stmts.into_iter().map(Box::new).collect()),
                e.span(),
            )
        })
}

fn expr_parser<'tokens, 'src: 'tokens, I>()
-> impl Parser<'tokens, I, Spanned<Expr>, extra::Err<Rich<'tokens, Token<'src>, Span>>> + Clone
where
    I: ValueInput<'tokens, Token = Token<'src>, Span = Span>,
{
    recursive(|expr| {
        let val = select! {
            Token::Null => Expr::Value(Literal::Null),
            Token::Bool(b) => Expr::Value(Literal::Bool(b)),
            Token::Int(n) => Expr::Value(Literal::Number(n)),
            Token::Float(f) => Expr::Value(Literal::Number(Number::Float(f))),
            Token::Str(s) => Expr::Value(Literal::Str(Rc::from(s.as_ref()))),
        }
        .labelled("value");

        let ident = select! { Token::Ident(ident) => Symbol::from(ident) }.labelled("identifier");

        let items = expr
            .clone()
            .separated_by(just(Token::Comma).or(just(Token::Semicolon)))
            .allow_trailing()
            .collect::<Vec<_>>();

        let list_brackets = items
            .clone()
            .map(Expr::List)
            .delimited_by(just(Token::BracketOpen), just(Token::BracketClose));

        let list_parentheses = items
            .clone()
            .map(Expr::List)
            .delimited_by(just(Token::ParenOpen), just(Token::ParenClose));

        let param_list = ident
            .clone()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>();

        /*
        zero_arg_function = "{" [ block_inner ] "}"
        arg_list = expression [ "," arg_list ]
        general_function = [ "(" [ arg_list ] ")" "=>" ]? zero_arg_function

         */
        let zero_arg_function = block_with_expr(expr.clone())
            .delimited_by(just(Token::BraceOpen), just(Token::BraceClose));

        let general_function = param_list
            .clone()
            .delimited_by(just(Token::ParenOpen), just(Token::ParenClose))
            .then_ignore(just(Token::LambdaArrow))
            .or_not()
            .then(zero_arg_function)
            .map(|(params, body)| {
                Expr::Func(UserFunctionDef {
                    params: Rc::from(params.unwrap_or_default()),
                    body: Rc::new(body),
                })
            });

        let atom = val
            .or(ident.map(Expr::Symbol))
            .or(list_brackets)
            .or(general_function)
            .or(expr
                .clone()
                .delimited_by(just(Token::ParenOpen), just(Token::ParenClose))
                .map_with(|expr, _e| expr.0))
            .or(list_parentheses)
            .map_with(|expr, e| (expr, e.span()))
            // Attempt to recover anything that looks like a parenthesised expression but contains errors
            .recover_with(via_parser(nested_delimiters(
                Token::ParenOpen,
                Token::ParenClose,
                [
                    (Token::BracketOpen, Token::BracketClose),
                    (Token::BraceOpen, Token::BraceClose),
                ],
                |span| (Expr::Error, span),
            )))
            // Attempt to recover anything that looks like a list but contains errors
            .recover_with(via_parser(nested_delimiters(
                Token::BracketOpen,
                Token::BracketClose,
                [
                    (Token::ParenOpen, Token::ParenClose),
                    (Token::BraceOpen, Token::BraceClose),
                ],
                |span| (Expr::Error, span),
            )))
            .boxed();

        // a.b
        let member_access = atom.clone().foldl_with(
            just(Token::Dot)
                .ignore_then(ident.clone())
                .map_with(|member, e| (member, e.span()))
                .repeated(),
            |object, member, e| (Expr::Member(Box::new(object), member), e.span()),
        );

        // f(x, y, z)
        let call_parenthesized = member_access.clone().foldl_with(
            items
                .delimited_by(just(Token::ParenOpen), just(Token::ParenClose))
                .map_with(|args, e| (args, e.span()))
                .repeated(),
            |f, args, e| (Expr::Call(Box::new(f), args), e.span()),
        );

        // f x; application binds more tightly than all infix operators.
        let call_unparenthesized = call_parenthesized.foldl_with(
            member_access
                .map_with(|arg, e| (vec![arg], e.span()))
                .repeated(),
            |f, args, e| (Expr::Call(Box::new(f), args), e.span()),
        );

        // These binding powers come directly from the table in thyme.md.
        let operator_expr = call_unparenthesized
            .pratt((
                infix(
                    right(14),
                    just(Token::Op("^")).to(BinaryOp::Pow),
                    |lhs, op, rhs, e| (Expr::Binary(Box::new(lhs), op, Box::new(rhs)), e.span()),
                ),
                prefix(
                    13,
                    select! {
                        Token::Op("!") => UnaryOp::Not,
                        Token::Op("-") => UnaryOp::Neg,
                        Token::Op("+") => UnaryOp::Pos,
                    },
                    |op, rhs, e| (Expr::Unary(op, Box::new(rhs)), e.span()),
                ),
                infix(
                    left(12),
                    select! {
                        Token::Op("*") => BinaryOp::Mul,
                        Token::Op("/") => BinaryOp::Div,
                        Token::Op("%") => BinaryOp::Mod,
                    },
                    |lhs, op, rhs, e| (Expr::Binary(Box::new(lhs), op, Box::new(rhs)), e.span()),
                ),
                infix(
                    left(11),
                    select! {
                        Token::Op("+") => BinaryOp::Add,
                        Token::Op("-") => BinaryOp::Sub,
                    },
                    |lhs, op, rhs, e| (Expr::Binary(Box::new(lhs), op, Box::new(rhs)), e.span()),
                ),
                infix(
                    left(10),
                    just(Token::Op("++")).to(BinaryOp::ListConcat),
                    |lhs, op, rhs, e| (Expr::Binary(Box::new(lhs), op, Box::new(rhs)), e.span()),
                ),
                infix(
                    left(9),
                    select! {
                        Token::Op("<") => BinaryOp::Less,
                        Token::Op("<=") => BinaryOp::LessEq,
                        Token::Op(">") => BinaryOp::Greater,
                        Token::Op(">=") => BinaryOp::GreaterEq,
                    },
                    |lhs, op, rhs, e| (Expr::Binary(Box::new(lhs), op, Box::new(rhs)), e.span()),
                ),
                infix(
                    left(8),
                    select! {
                        Token::Op("==") => BinaryOp::Eq,
                        Token::Op("!=") => BinaryOp::NotEq,
                    },
                    |lhs, op, rhs, e| (Expr::Binary(Box::new(lhs), op, Box::new(rhs)), e.span()),
                ),
                infix(
                    left(5),
                    just(Token::Op("..")).to(BinaryOp::Range),
                    |lhs, op, rhs, e| (Expr::Binary(Box::new(lhs), op, Box::new(rhs)), e.span()),
                ),
                infix(
                    left(4),
                    just(Token::Op("&&")).to(BinaryOp::And),
                    |lhs, op, rhs, e| (Expr::Binary(Box::new(lhs), op, Box::new(rhs)), e.span()),
                ),
                infix(
                    left(3),
                    just(Token::Op("||")).to(BinaryOp::Or),
                    |lhs, op, rhs, e| (Expr::Binary(Box::new(lhs), op, Box::new(rhs)), e.span()),
                ),
            ))
            .boxed();

        // ternary: a ? b : c
        // right associative: (true ? false : true ? 1 : 2) is parsed as (true ? false : (true ? 1 : 2))
        let ternary = operator_expr
            .then(
                just(Token::Op("?"))
                    .ignore_then(expr.clone())
                    .then_ignore(just(Token::Op(":")))
                    .then(expr.clone())
                    .or_not(),
            )
            .map_with(|(cond, branches), e| match branches {
                Some((true_expr, false_expr)) => (
                    Expr::Ternary(Box::new(cond), Box::new(true_expr), Box::new(false_expr)),
                    e.span(),
                ),
                None => cond,
            })
            .boxed();

        // class object assign: obj -> { foo = 5; bar = 6; }
        // right associative but Algodoo rejects it if there's more than one token on the left size (e.g. 1 + a->{2} breaks the parser)
        let class_assign = ternary
            .clone()
            .then(just(Token::Op("->")).ignore_then(expr.clone()).or_not())
            .map_with(|(obj, class_expr), e| match class_expr {
                Some(class_expr) => (
                    Expr::Binary(Box::new(obj), BinaryOp::ClassAssign, Box::new(class_expr)),
                    e.span(),
                ),
                None => obj,
            })
            .boxed();

        // assignment: a = b or a := b (right associative)
        let op = just(Token::Op("="))
            .to(BinaryOp::Assign)
            .or(just(Token::Op(":=")).to(BinaryOp::Declare));
        let assignment = class_assign
            .then(op.then(expr.clone()).or_not())
            .map_with(|(lhs, assignment), e| match assignment {
                Some((op, rhs)) => (Expr::Binary(Box::new(lhs), op, Box::new(rhs)), e.span()),
                None => lhs,
            })
            .boxed();

        assignment.labelled("expression").as_context()
    })
}
