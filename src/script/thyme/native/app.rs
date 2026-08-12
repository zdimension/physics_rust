use super::*;

native_class!(
    pub(super) APP = "App",
    [
        native_read_only!("mousePos", |world: &World, _| {
            let pos = world.resource::<MousePosWorld>();
            Ok(floats([pos.x, pos.y]))
        }),
        resource_property!("laserWidth", AppConfig, laser_width, float),
        NativeMember::Property {
            name: "polytoolPreviewColor",
            applies: None,
            get: |world, _| Ok(color_value(
                world.resource::<AppConfig>().polytool_preview_color
            )),
            set: Some(|world, _, value| {
                let [r, g, b, a] = float_list(value, "polytoolPreviewColor")?;
                world.resource_mut::<AppConfig>().polytool_preview_color = Color::srgba(r, g, b, a);
                Ok(())
            }),
        },
        resource_property!("enableScriptMenu", AppConfig, enable_script_menu, bool),
        resource_property!("drawScaleIndicator", AppConfig, draw_scale_indicator, bool),
    ]
);

native_class!(
    pub(super) APP_GRID = "Grid",
    [
        native_property!(
            "base",
            int,
            |world: &World, _| world.resource::<GridSettings>().base as i32,
            |world: &mut World, _, value| {
                world.resource_mut::<GridSettings>().base = int_at_least_two(value, "base")?;
                Ok(())
            }
        ),
        resource_property!("grid", GridSettings, enabled, bool),
        native_property!(
            "numAxes",
            int,
            |world: &World, _| world.resource::<GridSettings>().axes as i32,
            |world: &mut World, _, value| {
                world.resource_mut::<GridSettings>().axes = int_at_least_two(value, "numAxes")?;
                Ok(())
            }
        ),
        resource_property!("opacity", GridSettings, opacity, float),
        resource_property!("snap", GridSettings, snap, bool),
    ]
);
