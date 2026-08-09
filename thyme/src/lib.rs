use dumpster::{Trace, TraceWith, Visitor, unsync::Gc};
use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::{self, Debug, Display};
use std::rc::Rc;

use crate::parse::{Number, UserFunctionDef};

mod builtins;
pub mod eval;
pub mod parse;

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
        self.0
            .fields
            .borrow()
            .get(name)
            .map(|slot| slot.value.clone())
    }

    pub fn set_field(&self, name: impl Into<Symbol>, value: Value) -> Option<Value> {
        let name = name.into();
        let mut fields = self.0.fields.borrow_mut();
        if let Some(slot) = fields.get_mut(&name) {
            return Some(std::mem::replace(&mut slot.value, value));
        }
        fields.insert(name, ValueSlot::writable(value));
        None
    }

    pub fn remove_field(&self, name: &str) -> Option<Value> {
        self.0
            .fields
            .borrow_mut()
            .remove(name)
            .map(|slot| slot.value)
    }

    pub(crate) fn define_read_only_field(&self, name: impl Into<Symbol>, value: Value) {
        self.0
            .fields
            .borrow_mut()
            .insert(name.into(), ValueSlot::read_only(value));
    }

    pub(crate) fn field_is_read_only(&self, name: &str) -> bool {
        self.0
            .fields
            .borrow()
            .get(name)
            .is_some_and(|slot| slot.read_only)
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

/// A cloneable handle to a user function, Thyme builtin, or host-provided intrinsic.
#[derive(Clone, Trace)]
pub struct Function(Gc<FunctionValue>);

impl Function {
    pub(crate) fn builtin(
        name: impl AsRef<str>,
        arity: usize,
        callback: builtins::BuiltinCallback,
    ) -> Self {
        Self(Gc::new(FunctionValue::Builtin(
            builtins::BuiltinFunction::new(name, arity, callback),
        )))
    }

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
            FunctionValue::Builtin(builtin) => Some(builtin.name()),
            FunctionValue::Intrinsic(intrinsic) => Some(&intrinsic.name),
        }
    }

    pub fn arity(&self) -> usize {
        match &*self.0 {
            FunctionValue::User(user) => user.definition.params.len(),
            FunctionValue::Builtin(builtin) => builtin.arity(),
            FunctionValue::Intrinsic(intrinsic) => intrinsic.arity,
        }
    }

    pub fn intrinsic_id(&self) -> Option<IntrinsicId> {
        match &*self.0 {
            FunctionValue::User(_) | FunctionValue::Builtin(_) => None,
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
            FunctionValue::Builtin(builtin) => f
                .debug_struct("Function")
                .field("kind", &"builtin")
                .field("name", &builtin.name())
                .field("arity", &builtin.arity())
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
    Undefined,
    Bool(bool),
    Number(Number),
    Str(Rc<str>),
    List(List),
    Object(Object),
    Function(Function),
}

impl Value {
    /*fn thyme_display(&self) -> Cow<'_, str> {
        match self {
            Self::Null => "null".into(),
            Self::Void => "void".into(),
            Self::Undefined => "undefined".into(),
            Self::Bool(value) => value.to_string().into(),
            Self::Number(n) => n.thyme_display(),
            Self::Str(value) => value.as_ref().into(),
            Self::List(list) => {
                let mut result = String::from("[");
                for (i, value) in list.0.iter().enumerate() {
                    if i != 0 {
                        result.push_str(", ");
                    }
                    result.push_str(&value.thyme_display());
                }
                result.push(']');
                result.into()
            }
            Self::Object(object) => match object.native_id() {
                Some(id) => format!("native({})", id.into_raw()).into(),
                None => "object".into(),
            },
            Self::Function(function) => match &*function.0 {
                FunctionValue::Intrinsic(intrinsic) => format!("intrinsic function with {} arguments (id {})", function.arity(), intrinsic.id.into_raw()).into(),
                FunctionValue::User(def) => def.definition.to_string().into(),
            },
        }
    }*/
}

impl Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => f.write_str("null"),
            Self::Void => f.write_str("void"),
            Self::Undefined => f.write_str("undefined"),
            Self::Bool(value) => Display::fmt(value, f),
            Self::Number(n) => Display::fmt(n, f),
            Self::Str(value) => Display::fmt(value, f),
            Self::List(list) => {
                f.write_str("[")?;
                for (i, value) in list.0.iter().enumerate() {
                    if i != 0 {
                        f.write_str(", ")?;
                    }
                    Display::fmt(value, f)?;
                }
                f.write_str("]")
            }
            Self::Object(object) => match object.native_id() {
                Some(id) => write!(f, "native({})", id.into_raw()),
                None => f.write_str("object"),
            },
            Self::Function(function) => match &*function.0 {
                FunctionValue::Builtin(builtin) => {
                    write!(f, "builtin function {}", builtin.name())
                }
                FunctionValue::Intrinsic(intrinsic) => write!(
                    f,
                    "intrinsic function with {} arguments (id {})",
                    function.arity(),
                    intrinsic.id.into_raw()
                ),
                FunctionValue::User(def) => {
                    def.definition.pretty(&mut parse::PrettyPrinter::new(f))
                }
            },
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Null, Self::Null)
            | (Self::Void, Self::Void)
            | (Self::Undefined, Self::Undefined) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => a == b,
            (Self::Str(a), Self::Str(b)) => a == b,
            (Self::List(a), Self::List(b)) => a.as_slice() == b.as_slice(),
            (Self::Object(a), Self::Object(b)) => Gc::ptr_eq(&a.0, &b.0),
            (Self::Function(a), Self::Function(b)) => Gc::ptr_eq(&a.0, &b.0),
            _ => false,
        }
    }
}

impl<T: Into<Number>> From<T> for Value {
    fn from(value: T) -> Self {
        Self::Number(value.into())
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => f.write_str("Null"),
            Self::Void => f.write_str("Void"),
            Self::Undefined => f.write_str("Undefined"),
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
    fields: RefCell<HashMap<Symbol, ValueSlot>>,
    native: Option<NativeObjectId>,
}

#[derive(Trace)]
struct ValueSlot {
    value: Value,
    read_only: bool,
}

impl ValueSlot {
    fn writable(value: Value) -> Self {
        Self {
            value,
            read_only: false,
        }
    }

    fn read_only(value: Value) -> Self {
        Self {
            value,
            read_only: true,
        }
    }
}

#[derive(Trace)]
pub struct Environment {
    parent: Option<Gc<Environment>>,
    bindings: RefCell<HashMap<Symbol, ValueSlot>>,
    receiver: Option<Object>,
}

impl Environment {
    pub fn new_root() -> Self {
        let environment = Self {
            parent: None,
            bindings: RefCell::new(HashMap::new()),
            receiver: None,
        };
        builtins::install(&environment);
        environment
    }

    pub(crate) fn child(
        parent: Gc<Environment>,
        bindings: HashMap<Symbol, Value>,
        receiver: Option<Object>,
    ) -> Self {
        Self {
            parent: Some(parent),
            bindings: RefCell::new(
                bindings
                    .into_iter()
                    .map(|(name, value)| (name, ValueSlot::writable(value)))
                    .collect(),
            ),
            receiver,
        }
    }

    pub(crate) fn local(&self, name: &str) -> Option<Value> {
        self.bindings
            .borrow()
            .get(name)
            .map(|slot| slot.value.clone())
    }

    pub(crate) fn contains_local(&self, name: &str) -> bool {
        self.bindings.borrow().contains_key(name)
    }

    pub(crate) fn local_is_read_only(&self, name: &str) -> bool {
        self.bindings
            .borrow()
            .get(name)
            .is_some_and(|slot| slot.read_only)
    }

    pub(crate) fn parent(&self) -> Option<Gc<Environment>> {
        self.parent.clone()
    }

    pub(crate) fn receiver(&self) -> Option<Object> {
        self.receiver.clone()
    }

    /// Creates or replaces the binding for a name in this environment.
    pub fn declare(&self, name: impl Into<Symbol>, value: Value) -> Option<Value> {
        let name = name.into();
        let mut bindings = self.bindings.borrow_mut();
        if let Some(slot) = bindings.get_mut(&name) {
            return Some(std::mem::replace(&mut slot.value, value));
        }
        bindings.insert(name, ValueSlot::writable(value));
        None
    }

    pub(crate) fn define_read_only(&self, name: impl Into<Symbol>, value: Value) {
        self.bindings
            .borrow_mut()
            .insert(name.into(), ValueSlot::read_only(value));
    }
}

#[cfg(test)]
thread_local! {
    static ENVIRONMENT_DROPS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
impl Drop for Environment {
    fn drop(&mut self) {
        ENVIRONMENT_DROPS.with(|cell| cell.set(cell.get() + 1));
    }
}

#[derive(Trace)]
enum FunctionValue {
    User(UserFunction),
    Builtin(builtins::BuiltinFunction),
    Intrinsic(IntrinsicFunction),
}

#[derive(Trace)]
struct IntrinsicFunction {
    name: Rc<str>,
    arity: usize,
    id: IntrinsicId,
}

pub struct UserFunction {
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
            globals: Gc::new(Environment::new_root()),
            native_bindings: RefCell::new(HashMap::new()),
        }
    }

    pub fn global(&self, name: &str) -> Option<Value> {
        self.globals.local(name)
    }

    pub fn set_global(&self, name: impl Into<Symbol>, value: Value) -> Option<Value> {
        self.globals.declare(name, value)
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

#[cfg(test)]
mod tests {
    use chumsky::span::Span;

    use crate::parse::Expr;

    use super::*;

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
        let environment = Gc::new(Environment::new_root());
        let function = Function(Gc::new(FunctionValue::User(UserFunction {
            definition: UserFunctionDef {
                params: Rc::from([]),
                body: Rc::new((Expr::Error, Span::new((), 0..0))),
            },
            env: environment.clone(),
        })));
        environment.declare("cycle", Value::Function(function.clone()));

        drop(function);
        drop(environment);
        dumpster::unsync::collect();

        assert!(ENVIRONMENT_DROPS.get() > drops_before);
    }
}
