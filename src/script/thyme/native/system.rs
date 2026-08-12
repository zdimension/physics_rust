use super::*;

native_class!(
    pub(super) SYSTEM = "System",
    [
        native_method!("exit", 0, |world: &mut World, _, _, _| {
            world.write_message(AppExit::Success);
            Ok(Value::Void)
        }),
        native_method!("time", 0, |world: &mut World, _, _, _| {
            Ok(Value::Number(Number::Float(
                world.resource::<Time>().elapsed_secs(),
            )))
        }),
    ]
);
