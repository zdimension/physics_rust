use super::*;

native_class!(
    pub(super) SIM = "Sim",
    [
        native_read_only!("time", |world: &World, _| {
            Ok(Value::from(
                world.resource::<Time<Physics>>().elapsed_secs(),
            ))
        }),
        native_property!(
            "timeFactor",
            float,
            |world: &World, _| world.resource::<Time<Physics>>().relative_speed(),
            |world: &mut World, _, value: f32| {
                if !value.is_finite() || value < 0.0 {
                    return Err(type_error("timeFactor", "non-negative number"));
                }
                world
                    .resource_mut::<Time<Physics>>()
                    .set_relative_speed(value);
                Ok(())
            }
        ),
        native_property!(
            "running",
            bool,
            |world: &World, _| !world.resource::<Time<Physics>>().is_paused(),
            |world: &mut World, _, running: bool| {
                let mut time = world.resource_mut::<Time<Physics>>();
                if running {
                    time.unpause();
                } else {
                    time.pause();
                }
                Ok(())
            }
        ),
        native_property!(
            "gravityStrength",
            float,
            |world: &World, _| world.resource::<GravitySetting>().strength,
            |world: &mut World, _, strength: f32| {
                world.resource_mut::<GravitySetting>().strength = strength;
                Ok(())
            }
        ),
        native_property!(
            "gravitySwitch",
            bool,
            |world: &World, _| world.resource::<GravitySetting>().enabled,
            |world: &mut World, _, enabled: bool| {
                world.resource_mut::<GravitySetting>().enabled = enabled;
                Ok(())
            }
        ),
        native_property!(
            "gravityAngleOffset",
            float,
            |world: &World, _| world.resource::<GravitySetting>().direction
                + std::f32::consts::FRAC_PI_2,
            |world: &mut World, _, offset: f32| {
                world.resource_mut::<GravitySetting>().direction =
                    offset - std::f32::consts::FRAC_PI_2;
                Ok(())
            }
        ),
        resource_property!("airFrictionLinear", AirSettings, linear_term, float),
        resource_property!("airFrictionQuadratic", AirSettings, quadratic_term, float),
        resource_property!("airFrictionMultiplier", AirSettings, multiplier, float),
        resource_property!("airSwitch", AirSettings, enabled, bool),
        resource_property!("windAngle", AirSettings, wind_direction, float),
        resource_property!("windStrength", AirSettings, wind_speed, float),
    ]
);
