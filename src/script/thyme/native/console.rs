use super::*;

native_class!(
    pub(super) CONSOLE = "Console",
    [
        native_method!(
            "print",
            1,
            |world: &mut World, _, _, arguments: &[Value]| {
                world.resource_mut::<Console>().push_line(&arguments[0]);
                Ok(Value::Void)
            }
        ),
        native_method!("clear", 0, |world: &mut World, _, _, _| {
            world.resource_mut::<Console>().output.clear();
            Ok(Value::Void)
        }),
    ]
);
