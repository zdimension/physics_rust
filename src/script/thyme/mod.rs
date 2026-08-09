use std::fmt::Write;

use ::thyme::{
    Function, Host, HostError, HostErrorKind, IntrinsicId, NativeObjectId, Object, PropertyId,
    Runtime, Value, parse::Number,
};
use bevy::app::AppExit;
use bevy::ecs::message::MessageWriter;

const EXIT: IntrinsicId = IntrinsicId::from_raw(0);
const TIME: IntrinsicId = IntrinsicId::from_raw(1);

pub(crate) struct Console {
    pub(crate) open: bool,
    pub(crate) input: String,
    pub(crate) output: String,
    runtime: Runtime,
}

impl Default for Console {
    fn default() -> Self {
        let runtime = Runtime::new();
        let system = Object::new();
        system.set_field(
            "exit",
            Value::Function(Function::intrinsic(EXIT, "System.exit", 0)),
        );
        system.set_field(
            "time",
            Value::Function(Function::intrinsic(TIME, "System.time", 0)),
        );
        runtime.set_global("System", Value::Object(system));
        Self {
            open: false,
            input: String::new(),
            output: String::new(),
            runtime,
        }
    }
}

impl Console {
    pub(crate) fn run(&mut self, time: f32, exit: &mut MessageWriter<AppExit>) {
        let source = std::mem::take(&mut self.input);
        let source = source.trim();
        if source.is_empty() {
            return;
        }
        if !self.output.is_empty() {
            self.output.push('\n');
        }
        writeln!(self.output, "> {source}").unwrap();
        let mut host = SystemHost { time, exit };
        match self.runtime.eval(&mut host, source) {
            Ok(value) => write!(self.output, "{value}").unwrap(),
            Err(error) => write!(self.output, "ERROR: {error}").unwrap(),
        }
    }
}

struct SystemHost<'a, 'w> {
    time: f32,
    exit: &'a mut MessageWriter<'w, AppExit>,
}

impl Host for SystemHost<'_, '_> {
    fn resolve_property(
        &mut self,
        _object: NativeObjectId,
        _name: &str,
    ) -> Result<Option<PropertyId>, HostError> {
        Ok(None)
    }

    fn get_property(
        &mut self,
        _object: NativeObjectId,
        _property: PropertyId,
    ) -> Result<Value, HostError> {
        Err(HostError::new(HostErrorKind::Other, "no native objects"))
    }

    fn set_property(
        &mut self,
        _object: NativeObjectId,
        _property: PropertyId,
        _value: &Value,
    ) -> Result<(), HostError> {
        Err(HostError::new(HostErrorKind::Other, "no native objects"))
    }

    fn call_intrinsic(
        &mut self,
        intrinsic: IntrinsicId,
        _arguments: &[Value],
    ) -> Result<Value, HostError> {
        match intrinsic {
            EXIT => {
                self.exit.write(AppExit::Success);
                Ok(Value::Void)
            }
            TIME => Ok(Value::Number(Number::Float(self.time))),
            _ => Err(HostError::new(
                HostErrorKind::Intrinsic,
                "unknown intrinsic",
            )),
        }
    }
}
