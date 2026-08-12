use super::*;

native_class!(
    pub(super) GUI = "GUI",
    [
        native_property!(
            "scale",
            float,
            |world: &World, _| world.resource::<AppConfig>().ui_scale,
            |world: &mut World, _, scale: f32| {
                if !scale.is_finite() || scale <= 0.0 {
                    return Err(HostError::new(
                        HostErrorKind::InvalidType,
                        "scale must be finite and positive",
                    ));
                }
                world.resource_mut::<AppConfig>().ui_scale = scale;
                Ok(())
            }
        ),
        resource_property!("cursor", AppConfig, tool_cursor, bool),
        NativeMember::Property {
            name: "angleColor",
            applies: None,
            get: |world, _| Ok(color_value(world.resource::<AppConfig>().angle_color)),
            set: Some(|world, _, value| {
                let [r, g, b, a] = float_list(value, "angleColor")?;
                world.resource_mut::<AppConfig>().angle_color = Color::srgba(r, g, b, a);
                Ok(())
            }),
        },
        resource_property!(
            "allowDrawSelect",
            SelectionConfig,
            select_by_encircling,
            bool
        ),
    ]
);
