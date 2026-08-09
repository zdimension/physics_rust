use std::collections::HashMap;

use ::thyme::{
    Function, Host, HostError, HostErrorKind, IntrinsicId, List, NativeObjectId, Object,
    PropertyId, ResolvedProperty, Runtime, Symbol, Value, parse::Number,
};
use avian2d::prelude::{
    AngularVelocity, Collider, ColliderDensity, CollisionLayers, Gravity, LinearVelocity, Physics,
    PhysicsTime, Position, Restitution, RigidBody, Rotation,
};
use bevy::{
    app::AppExit,
    ecs::world::World,
    math::{EulerRot, Quat, Vec2, Vec3},
    prelude::{ChildOf, Entity, GlobalTransform, Time, Transform},
};
use bevy_egui::egui::ecolor::Hsva;

use crate::{
    config::AppConfig,
    lyon_compat::Shape,
    objects::{
        ColorComponent, MotorComponent,
        attraction::{Attraction, AttractionFalloff},
        laser::LaserSettings,
        phy_obj::{
            CircleVisual, FreeformObject, RefractiveIndex, set_box_geometry, set_circle_geometry,
        },
        spring::{SpringEndHandle, SpringObject},
        thruster::ThrusterSettings,
        tracer::TracerSettings,
    },
    tools::{
        add_object::AttachmentKind, r#move::attachment_local_position,
        rotate::attachment_local_rotation,
    },
    ui::GravitySetting,
};

type Getter = fn(&World, Option<Entity>) -> Result<Value, HostError>;
type Setter = fn(&mut World, Option<Entity>, &Value) -> Result<(), HostError>;
type Method = fn(&mut World, Option<Entity>, &[Value]) -> Result<Value, HostError>;
type Applies = fn(&World, Entity) -> bool;

enum NativeMember {
    Property {
        name: &'static str,
        applies: Option<Applies>,
        get: Getter,
        set: Option<Setter>,
    },
    #[allow(dead_code)]
    Event { name: &'static str },
    Method {
        name: &'static str,
        arity: usize,
        call: Method,
    },
}

impl NativeMember {
    fn name(&self) -> &'static str {
        match self {
            Self::Property { name, .. } | Self::Event { name } | Self::Method { name, .. } => name,
        }
    }
}

struct NativeClass {
    name: &'static str,
    members: &'static [NativeMember],
}

macro_rules! native_class {
    ($name:ident = $display:literal, [$($member:expr),* $(,)?]) => {
        static $name: NativeClass = NativeClass {
            name: $display,
            members: &[$($member),*],
        };
    };
}

macro_rules! native_property {
    ($name:literal, bool, $get:expr, $set:expr) => {
        NativeMember::Property {
            name: $name,
            applies: None,
            get: |world, entity| Ok(Value::Bool(($get)(world, entity))),
            set: Some(|world, entity, value| {
                let Value::Bool(value) = value else {
                    return Err(type_error($name, "bool"));
                };
                ($set)(world, entity, *value)
            }),
        }
    };
    ($name:literal, float, $get:expr, $set:expr) => {
        NativeMember::Property {
            name: $name,
            applies: None,
            get: |world, entity| Ok(Value::Number(Number::Float(($get)(world, entity)))),
            set: Some(|world, entity, value| {
                let Value::Number(value) = value else {
                    return Err(type_error($name, "number"));
                };
                ($set)(world, entity, value.to_f32_lossy())
            }),
        }
    };
    ($name:literal, int, $get:expr, $set:expr) => {
        NativeMember::Property {
            name: $name,
            applies: None,
            get: |world, entity| Ok(Value::Number(Number::Int(($get)(world, entity)))),
            set: Some(|world, entity, value| {
                let Value::Number(Number::Int(value)) = value else {
                    return Err(type_error($name, "int"));
                };
                ($set)(world, entity, *value)
            }),
        }
    };
}

macro_rules! scene_property {
    ($name:literal, $applies:expr, $get:path) => {
        NativeMember::Property {
            name: $name,
            applies: Some($applies),
            get: |world, entity| $get(world, scene_entity(entity)?),
            set: None,
        }
    };
    ($name:literal, $applies:expr, $get:path, $set:path) => {
        NativeMember::Property {
            name: $name,
            applies: Some($applies),
            get: |world, entity| $get(world, scene_entity(entity)?),
            set: Some(|world, entity, value| $set(world, scene_entity(entity)?, value)),
        }
    };
}

macro_rules! scene_component_property {
    ($name:literal, $component:ty, $get:path) => {
        scene_property!(
            $name,
            |world: &World, entity| world.get::<$component>(entity).is_some(),
            $get
        )
    };
    ($name:literal, $component:ty, $get:path, $set:path) => {
        scene_property!(
            $name,
            |world: &World, entity| world.get::<$component>(entity).is_some(),
            $get,
            $set
        )
    };
}

macro_rules! scene_component_value {
    ($name:literal, $component:ty, $field:tt, bool) => {
        NativeMember::Property {
            name: $name,
            applies: Some(|world, entity| world.get::<$component>(entity).is_some()),
            get: |world, entity| {
                Ok(Value::Bool(
                    world
                        .get::<$component>(scene_entity(entity)?)
                        .ok_or_else(|| object_error($name))?
                        .$field,
                ))
            },
            set: Some(|world, entity, value| {
                let Value::Bool(value) = value else {
                    return Err(type_error($name, "bool"));
                };
                world
                    .get_mut::<$component>(scene_entity(entity)?)
                    .ok_or_else(|| object_error($name))?
                    .$field = *value;
                Ok(())
            }),
        }
    };
    ($name:literal, $component:ty, $field:tt, float) => {
        NativeMember::Property {
            name: $name,
            applies: Some(|world, entity| world.get::<$component>(entity).is_some()),
            get: |world, entity| {
                Ok(Value::from(
                    world
                        .get::<$component>(scene_entity(entity)?)
                        .ok_or_else(|| object_error($name))?
                        .$field,
                ))
            },
            set: Some(|world, entity, value| {
                world
                    .get_mut::<$component>(scene_entity(entity)?)
                    .ok_or_else(|| object_error($name))?
                    .$field = number(value, $name)?;
                Ok(())
            }),
        }
    };
}

macro_rules! native_method {
    ($name:literal, $arity:expr, $call:expr) => {
        NativeMember::Method {
            name: $name,
            arity: $arity,
            call: $call,
        }
    };
}

#[allow(unused_macros)]
macro_rules! native_event {
    ($name:literal) => {
        NativeMember::Event { name: $name }
    };
}

fn type_error(name: &str, expected: &str) -> HostError {
    HostError::new(
        HostErrorKind::InvalidType,
        format!("{name} expects {expected}"),
    )
}

fn sync_gravity(world: &mut World) {
    let settings = *world.resource::<GravitySetting>();
    world.resource_mut::<Gravity>().0 = if settings.enabled {
        Vec2::from_angle(settings.direction) * settings.strength
    } else {
        Vec2::ZERO
    };
}

fn scene_entity(entity: Option<Entity>) -> Result<Entity, HostError> {
    entity.ok_or_else(|| HostError::new(HostErrorKind::UnknownObject, "missing scene object"))
}

fn object_error(name: &str) -> HostError {
    HostError::new(HostErrorKind::UnknownObject, format!("missing {name}"))
}

fn number(value: &Value, name: &str) -> Result<f32, HostError> {
    let Value::Number(value) = value else {
        return Err(type_error(name, "number"));
    };
    Ok(value.to_f32_lossy())
}

fn float_list<const N: usize>(value: &Value, name: &str) -> Result<[f32; N], HostError> {
    let Value::List(values) = value else {
        return Err(type_error(name, "list"));
    };
    let Ok(values) = <&[Value; N]>::try_from(values.as_slice()) else {
        return Err(type_error(name, &format!("{N}-item list")));
    };
    let mut result = [0.0; N];
    for (result, value) in result.iter_mut().zip(values) {
        *result = number(value, name)?;
    }
    Ok(result)
}

fn floats(values: impl IntoIterator<Item = f32>) -> Value {
    Value::List(List::new(values.into_iter().map(Value::from)))
}

fn has_angle(world: &World, entity: Entity) -> bool {
    world.get::<RigidBody>(entity).is_some() && world.get::<Rotation>(entity).is_some()
}

fn has_rotation(world: &World, entity: Entity) -> bool {
    world.get::<AttachmentKind>(entity).is_some()
}

fn get_world_rotation(world: &World, entity: Entity) -> Result<Value, HostError> {
    let attachment = world.get::<AttachmentKind>(entity).is_some();
    let angle = if attachment {
        world
            .get::<GlobalTransform>(entity)
            .map(|transform| transform.rotation().to_euler(EulerRot::XYZ).2)
    } else {
        world
            .get::<Rotation>(entity)
            .map(|rotation| rotation.as_radians())
            .or_else(|| {
                world
                    .get::<GlobalTransform>(entity)
                    .map(|transform| transform.rotation().to_euler(EulerRot::XYZ).2)
            })
    }
    .ok_or_else(|| object_error("rotation"))?;
    Ok(Value::from(angle))
}

fn set_world_rotation(
    world: &mut World,
    entity: Entity,
    value: &Value,
    name: &str,
) -> Result<(), HostError> {
    let angle = number(value, name)?;
    let attachment = world.get::<AttachmentKind>(entity).is_some();
    if let Some(mut rotation) = world.get_mut::<Rotation>(entity) {
        *rotation = Rotation::radians(angle);
    }
    if !attachment {
        world
            .get_mut::<Transform>(entity)
            .ok_or_else(|| object_error(name))?
            .rotation = Quat::from_rotation_z(angle);
        return Ok(());
    }
    let parent = world
        .get::<ChildOf>(entity)
        .and_then(|parent| world.get::<GlobalTransform>(parent.parent()));
    let local_rotation = attachment_local_rotation(parent, angle);
    let mut transform = world
        .get_mut::<Transform>(entity)
        .ok_or_else(|| object_error(name))?;
    transform.rotation = local_rotation;
    if let Some(mut settings) = world.get_mut::<ThrusterSettings>(entity)
        && !settings.follow_geometry_rotation
    {
        settings.fixed_angle = angle;
    }
    Ok(())
}

fn set_angle(world: &mut World, entity: Entity, value: &Value) -> Result<(), HostError> {
    set_world_rotation(world, entity, value, "angle")
}

fn set_rotation(world: &mut World, entity: Entity, value: &Value) -> Result<(), HostError> {
    set_world_rotation(world, entity, value, "rotation")
}

fn has_area(world: &World, entity: Entity) -> bool {
    world.get::<RigidBody>(entity).is_some() && world.get::<Collider>(entity).is_some()
}

fn get_area(world: &World, entity: Entity) -> Result<Value, HostError> {
    let collider = world
        .get::<Collider>(entity)
        .ok_or_else(|| object_error("area"))?;
    Ok(Value::from(collider.shape().mass_properties(1.0).mass()))
}

fn has_collision_set(world: &World, entity: Entity) -> bool {
    world.get::<RigidBody>(entity).is_some() && world.get::<CollisionLayers>(entity).is_some()
}

fn get_collision_set(world: &World, entity: Entity) -> Result<Value, HostError> {
    let layers = world
        .get::<CollisionLayers>(entity)
        .ok_or_else(|| object_error("collideSet"))?;
    Ok(Value::from(layers.memberships.0 as i32))
}

fn set_collision_set(world: &mut World, entity: Entity, value: &Value) -> Result<(), HostError> {
    let Value::Number(Number::Int(value)) = value else {
        return Err(type_error("collideSet", "int"));
    };
    world
        .entity_mut(entity)
        .insert(CollisionLayers::from_bits(*value as u32, *value as u32));
    Ok(())
}

fn get_color_hsva(world: &World, entity: Entity) -> Result<Value, HostError> {
    let color = world
        .get::<ColorComponent>(entity)
        .ok_or_else(|| object_error("colorHSVA"))?
        .0;
    Ok(floats([color.h * 360.0, color.s, color.v, color.a]))
}

fn set_color_hsva(world: &mut World, entity: Entity, value: &Value) -> Result<(), HostError> {
    let [h, s, v, a] = float_list(value, "colorHSVA")?;
    world
        .entity_mut(entity)
        .insert(ColorComponent(Hsva::new(h / 360.0, s, v, a)));
    Ok(())
}

fn get_color(world: &World, entity: Entity) -> Result<Value, HostError> {
    let color = world
        .get::<ColorComponent>(entity)
        .ok_or_else(|| object_error("color"))?
        .0;
    Ok(floats(color.to_rgba_unmultiplied()))
}

fn set_color(world: &mut World, entity: Entity, value: &Value) -> Result<(), HostError> {
    let [r, g, b, a] = float_list(value, "color")?;
    world
        .entity_mut(entity)
        .insert(ColorComponent(Hsva::from_rgba_unmultiplied(r, g, b, a)));
    Ok(())
}

fn get_attraction_type(world: &World, entity: Entity) -> Result<Value, HostError> {
    let falloff = world
        .get::<Attraction>(entity)
        .ok_or_else(|| object_error("attractionType"))?
        .falloff;
    Ok(Value::from(match falloff {
        AttractionFalloff::Linear => 1,
        AttractionFalloff::Quadratic => 2,
    }))
}

fn set_attraction_type(world: &mut World, entity: Entity, value: &Value) -> Result<(), HostError> {
    let Value::Number(Number::Int(value)) = value else {
        return Err(type_error("attractionType", "1 or 2"));
    };
    world
        .get_mut::<Attraction>(entity)
        .ok_or_else(|| object_error("attractionType"))?
        .falloff = match *value {
        1 => AttractionFalloff::Linear,
        2 => AttractionFalloff::Quadratic,
        _ => return Err(type_error("attractionType", "1 or 2")),
    };
    Ok(())
}

fn is_box(world: &World, entity: Entity) -> bool {
    world
        .get::<CircleVisual>(entity)
        .is_some_and(|circle| circle.0 == 0.0)
        && world.get::<FreeformObject>(entity).is_none()
        && world
            .get::<Collider>(entity)
            .is_some_and(|collider| collider.shape().as_cuboid().is_some())
}

fn get_box_size(world: &World, entity: Entity) -> Result<Value, HostError> {
    let size = world
        .get::<Collider>(entity)
        .and_then(|collider| collider.shape().as_cuboid())
        .map(|cuboid| cuboid.half_extents * 2.0)
        .ok_or_else(|| object_error("size"))?;
    Ok(floats(size.to_array()))
}

fn set_box_size(world: &mut World, entity: Entity, value: &Value) -> Result<(), HostError> {
    let size = Vec2::from_array(float_list(value, "size")?);
    let mut query = world.query::<(&mut Collider, &mut Shape)>();
    let (mut collider, mut shape) = query
        .get_mut(world, entity)
        .map_err(|_| object_error("size"))?;
    set_box_geometry(&mut collider, &mut shape, size);
    Ok(())
}

fn has_radius(world: &World, entity: Entity) -> bool {
    world
        .get::<CircleVisual>(entity)
        .is_some_and(|circle| circle.0 > 0.0)
}

fn get_radius(world: &World, entity: Entity) -> Result<Value, HostError> {
    Ok(Value::from(
        world
            .get::<CircleVisual>(entity)
            .ok_or_else(|| object_error("radius"))?
            .0,
    ))
}

fn set_radius(world: &mut World, entity: Entity, value: &Value) -> Result<(), HostError> {
    let radius = number(value, "radius")?;
    let mut query = world.query::<(&mut Collider, &mut Shape, &mut CircleVisual)>();
    let (mut collider, mut shape, mut circle) = query
        .get_mut(world, entity)
        .map_err(|_| object_error("radius"))?;
    set_circle_geometry(&mut collider, &mut shape, &mut circle, radius);
    Ok(())
}

fn has_pos(world: &World, entity: Entity) -> bool {
    world.get::<Position>(entity).is_some() || world.get::<AttachmentKind>(entity).is_some()
}

fn get_pos(world: &World, entity: Entity) -> Result<Value, HostError> {
    let attachment = world.get::<AttachmentKind>(entity).is_some();
    let pos = if attachment {
        world
            .get::<GlobalTransform>(entity)
            .map(|transform| transform.translation().truncate())
    } else {
        world.get::<Position>(entity).map(|pos| pos.0).or_else(|| {
            world
                .get::<GlobalTransform>(entity)
                .map(|transform| transform.translation().truncate())
        })
    }
    .ok_or_else(|| object_error("pos"))?;
    Ok(floats(pos.to_array()))
}

fn set_pos(world: &mut World, entity: Entity, value: &Value) -> Result<(), HostError> {
    let pos = Vec2::from_array(float_list(value, "pos")?);
    if let Some(mut position) = world.get_mut::<Position>(entity) {
        position.0 = pos;
    }
    let attachment = world.get::<AttachmentKind>(entity).is_some();
    if !attachment {
        let mut transform = world
            .get_mut::<Transform>(entity)
            .ok_or_else(|| object_error("pos"))?;
        transform.translation.x = pos.x;
        transform.translation.y = pos.y;
        return Ok(());
    }
    let parent = world
        .get::<ChildOf>(entity)
        .and_then(|parent| world.get::<GlobalTransform>(parent.parent()));
    let local = attachment_local_position(parent, pos);
    let mut transform = world
        .get_mut::<Transform>(entity)
        .ok_or_else(|| object_error("pos"))?;
    transform.translation.x = local.x;
    transform.translation.y = local.y;
    Ok(())
}

fn has_size(world: &World, entity: Entity) -> bool {
    is_box(world, entity)
        || world.get::<LaserSettings>(entity).is_some()
        || world.get::<TracerSettings>(entity).is_some()
        || world.get::<SpringObject>(entity).is_some()
        || world.get::<SpringEndHandle>(entity).is_some()
        || world.get::<MotorComponent>(entity).is_some()
}

fn get_size(world: &World, entity: Entity) -> Result<Value, HostError> {
    if is_box(world, entity) {
        return get_box_size(world, entity);
    }
    let size = world
        .get::<LaserSettings>(entity)
        .map(|v| v.size)
        .or_else(|| world.get::<TracerSettings>(entity).map(|v| v.diameter))
        .or_else(|| world.get::<SpringObject>(entity).map(|v| v.unit_size))
        .or_else(|| world.get::<Transform>(entity).map(|v| v.scale.x))
        .ok_or_else(|| object_error("size"))?;
    Ok(Value::from(size))
}

fn set_size(world: &mut World, entity: Entity, value: &Value) -> Result<(), HostError> {
    if is_box(world, entity) {
        return set_box_size(world, entity, value);
    }
    let size = number(value, "size")?;
    if let Some(mut settings) = world.get_mut::<LaserSettings>(entity) {
        settings.size = size;
    } else if let Some(mut settings) = world.get_mut::<TracerSettings>(entity) {
        settings.diameter = size;
    } else if let Some(mut spring) = world.get_mut::<SpringObject>(entity) {
        spring.unit_size = size;
    } else {
        let mut transform = world
            .get_mut::<Transform>(entity)
            .ok_or_else(|| object_error("size"))?;
        transform.scale = Vec3::new(size, size, transform.scale.z);
    }
    Ok(())
}

fn get_vel(world: &World, entity: Entity) -> Result<Value, HostError> {
    let vel = world
        .get::<LinearVelocity>(entity)
        .ok_or_else(|| object_error("vel"))?;
    Ok(floats(vel.0.to_array()))
}

fn set_vel(world: &mut World, entity: Entity, value: &Value) -> Result<(), HostError> {
    world
        .get_mut::<LinearVelocity>(entity)
        .ok_or_else(|| object_error("vel"))?
        .0 = Vec2::from_array(float_list(value, "vel")?);
    Ok(())
}

fn get_z_order(world: &World, entity: Entity) -> Result<Value, HostError> {
    let z = world
        .get::<GlobalTransform>(entity)
        .map(|transform| transform.translation().z)
        .or_else(|| {
            world
                .get::<Transform>(entity)
                .map(|transform| transform.translation.z)
        })
        .ok_or_else(|| object_error("zOrder"))?;
    Ok(Value::from(z))
}

fn set_z_order(world: &mut World, entity: Entity, value: &Value) -> Result<(), HostError> {
    let z = number(value, "zOrder")?;
    let current = world.get::<GlobalTransform>(entity).map_or_else(
        || world.get::<Transform>(entity).unwrap().translation.z,
        |t| t.translation().z,
    );
    world
        .get_mut::<Transform>(entity)
        .ok_or_else(|| object_error("zOrder"))?
        .translation
        .z += z - current;
    Ok(())
}

native_class!(
    SYSTEM = "System",
    [
        native_method!("exit", 0, |world: &mut World, _, _| {
            world.write_message(AppExit::Success);
            Ok(Value::Void)
        }),
        native_method!("time", 0, |world: &mut World, _, _| {
            Ok(Value::Number(Number::Float(
                world.resource::<Time>().elapsed_secs(),
            )))
        }),
    ]
);

native_class!(
    GUI = "GUI",
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
        native_property!(
            "cursor",
            bool,
            |world: &World, _| world.resource::<AppConfig>().tool_cursor,
            |world: &mut World, _, cursor: bool| {
                world.resource_mut::<AppConfig>().tool_cursor = cursor;
                Ok(())
            }
        ),
    ]
);

native_class!(
    SIM = "Sim",
    [
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
                sync_gravity(world);
                Ok(())
            }
        ),
        native_property!(
            "gravitySwitch",
            bool,
            |world: &World, _| world.resource::<GravitySetting>().enabled,
            |world: &mut World, _, enabled: bool| {
                world.resource_mut::<GravitySetting>().enabled = enabled;
                sync_gravity(world);
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
                sync_gravity(world);
                Ok(())
            }
        ),
    ]
);

native_class!(
    SCENE_OBJECT = "SceneObject",
    [
        scene_property!("angle", has_angle, get_world_rotation, set_angle),
        scene_component_value!("angvel", AngularVelocity, 0, float),
        scene_property!("area", has_area, get_area),
        scene_component_value!("attraction", Attraction, strength, float),
        scene_component_property!(
            "attractionType",
            Attraction,
            get_attraction_type,
            set_attraction_type
        ),
        scene_property!(
            "collideSet",
            has_collision_set,
            get_collision_set,
            set_collision_set
        ),
        scene_component_property!("color", ColorComponent, get_color, set_color),
        scene_component_property!("colorHSVA", ColorComponent, get_color_hsva, set_color_hsva),
        scene_component_value!("constant", SpringObject, spring_constant, float),
        scene_component_value!("dampingFactor", SpringObject, damping, float),
        scene_component_value!("density", ColliderDensity, 0, float),
        scene_component_value!("fadeDist", LaserSettings, fade_distance, float),
        scene_component_value!("force", ThrusterSettings, force, float),
        scene_component_value!("length", SpringObject, target_length, float),
        scene_component_value!("motor", MotorComponent, enabled, bool),
        scene_component_value!("motorSpeed", MotorComponent, vel, float),
        scene_component_value!("motorTorque", MotorComponent, torque, float),
        scene_property!("pos", has_pos, get_pos, set_pos),
        scene_property!("radius", has_radius, get_radius, set_radius),
        scene_component_value!("restitution", Restitution, coefficient, float),
        scene_component_value!("refractiveIndex", RefractiveIndex, 0, float),
        scene_property!("rotation", has_rotation, get_world_rotation, set_rotation),
        scene_property!("size", has_size, get_size, set_size),
        scene_component_property!("vel", LinearVelocity, get_vel, set_vel),
        scene_component_property!("zOrder", Transform, get_z_order, set_z_order),
    ]
);

static CLASSES: &[&NativeClass] = &[&SYSTEM, &GUI, &SIM, &SCENE_OBJECT];
const GLOBAL_CLASS_COUNT: usize = 3;
const SCENE_CLASS: usize = 3;

struct RegisteredClass {
    definition: &'static NativeClass,
    properties: HashMap<Symbol, PropertyId>,
}

struct NativeInstance {
    class: usize,
    entity: Option<Entity>,
    #[allow(dead_code)]
    object: Object,
}

struct NativeRegistry {
    classes: Vec<RegisteredClass>,
    instances: HashMap<NativeObjectId, NativeInstance>,
    entities: HashMap<Entity, NativeObjectId>,
    globals: Vec<NativeObjectId>,
    next_id: u64,
}

impl NativeRegistry {
    fn new(runtime: &Runtime) -> Self {
        let classes = CLASSES
            .iter()
            .map(|definition| RegisteredClass {
                definition,
                properties: definition
                    .members
                    .iter()
                    .enumerate()
                    .filter_map(|(index, member)| match member {
                        NativeMember::Property { name, .. } => {
                            Some((Symbol::from(*name), PropertyId::from_raw(index as u64)))
                        }
                        _ => None,
                    })
                    .collect(),
            })
            .collect();
        let mut registry = Self {
            classes,
            instances: HashMap::new(),
            entities: HashMap::new(),
            globals: Vec::new(),
            next_id: 0,
        };
        for class in 0..GLOBAL_CLASS_COUNT {
            registry.register_global(runtime, class);
        }
        registry
    }

    fn register_global(&mut self, runtime: &Runtime, class: usize) {
        let (id, object) = self.register_instance(class, None);
        let definition = self.classes[class].definition;
        runtime.define_read_only_global(definition.name, Value::Object(object));
        self.globals.push(id);
    }

    fn register_instance(
        &mut self,
        class: usize,
        entity: Option<Entity>,
    ) -> (NativeObjectId, Object) {
        let id = NativeObjectId::from_raw(self.next_id);
        self.next_id += 1;
        let definition = self.classes[class].definition;
        let object = Object::native(id);
        for (member, descriptor) in definition.members.iter().enumerate() {
            match descriptor {
                NativeMember::Event { name } => {
                    object.set_field(*name, Value::Undefined);
                }
                NativeMember::Method { name, arity, .. } => {
                    let intrinsic = IntrinsicId::from_raw(((class as u64) << 32) | member as u64);
                    object.define_read_only_field(
                        *name,
                        Value::Function(Function::method(
                            id,
                            intrinsic,
                            format!("{}.{}", definition.name, name),
                            *arity,
                        )),
                    );
                }
                NativeMember::Property { .. } => {}
            }
        }
        self.instances.insert(
            id,
            NativeInstance {
                class,
                entity,
                object: object.clone(),
            },
        );
        (id, object)
    }

    fn ensure_entity(&mut self, entity: Entity) -> NativeObjectId {
        if let Some(&id) = self.entities.get(&entity) {
            return id;
        }
        let (id, _) = self.register_instance(SCENE_CLASS, Some(entity));
        self.entities.insert(entity, id);
        id
    }

    fn instance(&self, id: NativeObjectId) -> Result<&NativeInstance, HostError> {
        self.instances
            .get(&id)
            .ok_or_else(|| HostError::new(HostErrorKind::UnknownObject, "unknown object"))
    }

    fn member(
        &self,
        object: NativeObjectId,
        property: PropertyId,
    ) -> Result<(&NativeInstance, &NativeMember), HostError> {
        let instance = self.instance(object)?;
        let member = self.classes[instance.class]
            .definition
            .members
            .get(property.into_raw() as usize)
            .ok_or_else(|| HostError::new(HostErrorKind::UnknownProperty, "unknown property"))?;
        Ok((instance, member))
    }

    fn property_name(&self, object: NativeObjectId, property: PropertyId) -> &str {
        self.member(object, property)
            .map(|(_, member)| member.name())
            .unwrap_or("?")
    }
}

pub(crate) struct SceneProperty {
    pub(crate) name: &'static str,
    pub(crate) value: String,
    pub(crate) read_only: bool,
}

pub(crate) struct ScriptEngine {
    runtime: Runtime,
    registry: NativeRegistry,
}

impl Default for ScriptEngine {
    fn default() -> Self {
        let runtime = Runtime::new();
        let registry = NativeRegistry::new(&runtime);
        Self { runtime, registry }
    }
}

impl ScriptEngine {
    pub(crate) fn eval(&mut self, world: &mut World, source: &str) -> Result<Value, String> {
        let mut host = WorldHost {
            world,
            registry: &self.registry,
        };
        self.runtime.eval(&mut host, source)
    }

    pub(crate) fn selection_properties(
        &mut self,
        world: &mut World,
        entities: &[Entity],
    ) -> Vec<SceneProperty> {
        let objects = entities
            .iter()
            .copied()
            .filter(|entity| world.get_entity(*entity).is_ok())
            .map(|entity| (entity, self.registry.ensure_entity(entity)))
            .collect::<Vec<_>>();
        let mut result = Vec::new();
        for (index, member) in SCENE_OBJECT.members.iter().enumerate() {
            let NativeMember::Property {
                name,
                applies: Some(applies),
                get,
                set,
            } = member
            else {
                continue;
            };
            let mut values = objects.iter().filter_map(|&(entity, object)| {
                applies(world, entity).then(|| {
                    self.runtime
                        .property_binding(object, PropertyId::from_raw(index as u64))
                        .map(Value::Function)
                        .unwrap_or_else(|| get(world, Some(entity)).unwrap_or(Value::Undefined))
                })
            });
            let Some(value) = values.next() else { continue };
            let mixed = values.any(|other| other != value);
            result.push(SceneProperty {
                name,
                value: if mixed { "?".into() } else { value.to_string() },
                read_only: set.is_none(),
            });
        }
        result
    }

    pub(crate) fn set_selection_property(
        &mut self,
        world: &mut World,
        entities: &[Entity],
        name: &str,
        source: &str,
    ) -> Result<(), String> {
        let property = *self.registry.classes[SCENE_CLASS]
            .properties
            .get(&Symbol::new(name))
            .ok_or_else(|| format!("unknown property {name}"))?;
        let NativeMember::Property { applies, set, .. } =
            &SCENE_OBJECT.members[property.into_raw() as usize]
        else {
            unreachable!()
        };
        let applies = applies.unwrap();
        if set.is_none() {
            return Err(format!("{name} is read-only"));
        }
        let targets = entities
            .iter()
            .copied()
            .filter(|entity| world.get_entity(*entity).is_ok() && applies(world, *entity))
            .map(|entity| self.registry.ensure_entity(entity))
            .collect::<Vec<_>>();
        if targets.is_empty() {
            return Err(format!("no selected object has {name}"));
        }
        let value = self.eval(world, source)?;
        let mut host = WorldHost {
            world,
            registry: &self.registry,
        };
        for object in targets {
            self.runtime
                .assign_native_property(&mut host, object, property, value.clone())
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub(crate) fn evaluate_bindings(&mut self, world: &mut World) -> Vec<String> {
        let removed = self
            .registry
            .entities
            .iter()
            .filter_map(|(&entity, &object)| {
                world
                    .get_entity(entity)
                    .is_err()
                    .then_some((entity, object))
            })
            .collect::<Vec<_>>();
        for (entity, object) in removed {
            self.registry.entities.remove(&entity);
            self.registry.instances.remove(&object);
            self.runtime.unbind_object(object);
        }
        let mut objects = self.registry.globals.clone();
        let mut scene_objects = self
            .registry
            .entities
            .iter()
            .map(|(&entity, &object)| {
                let z = world
                    .get::<GlobalTransform>(entity)
                    .map_or(0.0, |transform| transform.translation().z);
                (z, object)
            })
            .collect::<Vec<_>>();
        scene_objects.sort_by(|a, b| a.0.total_cmp(&b.0));
        objects.extend(scene_objects.into_iter().map(|(_, object)| object));
        let mut host = WorldHost {
            world,
            registry: &self.registry,
        };
        let mut errors = Vec::new();
        for object in objects {
            for error in self.runtime.evaluate_property_bindings(&mut host, object) {
                let instance = self.registry.instance(object).unwrap();
                errors.push(format!(
                    "{}.{}: {}",
                    instance.entity.map_or(
                        self.registry.classes[instance.class]
                            .definition
                            .name
                            .to_owned(),
                        |entity| format!("{entity:?}")
                    ),
                    self.registry.property_name(object, error.property),
                    error.message
                ));
            }
        }
        errors
    }
}

struct WorldHost<'a> {
    world: &'a mut World,
    registry: &'a NativeRegistry,
}

impl Host for WorldHost<'_> {
    fn resolve_property(
        &mut self,
        object: NativeObjectId,
        name: &Symbol,
    ) -> Result<Option<ResolvedProperty>, HostError> {
        let instance = self.registry.instance(object)?;
        let class = &self.registry.classes[instance.class];
        let Some(&property) = class.properties.get(name) else {
            return Ok(None);
        };
        let NativeMember::Property { applies, set, .. } =
            &class.definition.members[property.into_raw() as usize]
        else {
            unreachable!()
        };
        if let (Some(applies), Some(entity)) = (applies, instance.entity)
            && !applies(self.world, entity)
        {
            return Ok(None);
        }
        Ok(Some(ResolvedProperty::new(property, set.is_some())))
    }

    fn get_property(
        &mut self,
        object: NativeObjectId,
        property: PropertyId,
    ) -> Result<Value, HostError> {
        let (instance, member) = self.registry.member(object, property)?;
        let NativeMember::Property { get, .. } = member else {
            return Err(HostError::new(
                HostErrorKind::UnknownProperty,
                "not a property",
            ));
        };
        get(self.world, instance.entity)
    }

    fn set_property(
        &mut self,
        object: NativeObjectId,
        property: PropertyId,
        value: &Value,
    ) -> Result<(), HostError> {
        let (instance, member) = self.registry.member(object, property)?;
        let NativeMember::Property { name, set, .. } = member else {
            return Err(HostError::new(
                HostErrorKind::UnknownProperty,
                "not a property",
            ));
        };
        let Some(set) = set else {
            return Err(HostError::new(
                HostErrorKind::Other,
                format!("{name} is read-only"),
            ));
        };
        set(self.world, instance.entity, value)
    }

    fn call_intrinsic(
        &mut self,
        receiver: Option<NativeObjectId>,
        intrinsic: IntrinsicId,
        arguments: &[Value],
    ) -> Result<Value, HostError> {
        let receiver = receiver
            .ok_or_else(|| HostError::new(HostErrorKind::Intrinsic, "missing method receiver"))?;
        let instance = self.registry.instance(receiver)?;
        let class = (intrinsic.into_raw() >> 32) as usize;
        let member = intrinsic.into_raw() as u32 as usize;
        if instance.class != class {
            return Err(HostError::new(
                HostErrorKind::Intrinsic,
                "invalid method receiver",
            ));
        }
        let Some(NativeMember::Method { call, .. }) = self
            .registry
            .classes
            .get(class)
            .and_then(|class| class.definition.members.get(member))
        else {
            return Err(HostError::new(HostErrorKind::Intrinsic, "unknown method"));
        };
        call(self.world, instance.entity, arguments)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::phy_obj::PhysicalObject;
    use crate::objects::spring::{SpringEnd, SpringEndIndex};

    fn world() -> World {
        let mut world = World::new();
        world.insert_resource(AppConfig::default());
        world.insert_resource(GravitySetting::default());
        world.insert_resource(Gravity(Vec2::NEG_Y * 9.81));
        let mut physics = Time::<Physics>::default();
        physics.pause();
        world.insert_resource(physics);
        world.insert_resource(Time::<()>::default());
        world
    }

    #[test]
    fn registry_preserves_names_and_resolves_case_insensitively() {
        let engine = ScriptEngine::default();
        let gui = engine.registry.globals[1];
        let mut world = world();
        let mut host = WorldHost {
            world: &mut world,
            registry: &engine.registry,
        };
        let property = host
            .resolve_property(gui, &Symbol::from("ScAlE"))
            .unwrap()
            .unwrap();

        assert_eq!(engine.registry.classes[1].definition.name, "GUI");
        assert_eq!(engine.registry.property_name(gui, property.id()), "scale");
        assert!(property.is_writable());
        assert_ne!(engine.registry.globals[0], engine.registry.globals[1]);
        assert_eq!(
            engine.runtime.global("gui"),
            Some(Value::Object(
                engine.registry.instance(gui).unwrap().object.clone()
            ))
        );
    }

    #[test]
    fn gui_properties_accept_ints_for_floats_and_protect_globals() {
        let mut engine = ScriptEngine::default();
        let mut world = world();

        engine.eval(&mut world, "gui.SCALE = 2").unwrap();
        assert_eq!(world.resource::<AppConfig>().ui_scale, 2.0);
        assert!(engine.eval(&mut world, "GUI.scale = 0").is_err());
        assert!(engine.eval(&mut world, "GUI = 1").is_err());
        assert!(engine.eval(&mut world, "System.time = 1").is_err());
        assert!(matches!(
            engine.eval(&mut world, "system.TIME").unwrap(),
            Value::Number(Number::Float(_))
        ));
    }

    #[test]
    fn sim_properties_share_toolbar_and_physics_state() {
        let mut engine = ScriptEngine::default();
        let mut world = world();

        engine.eval(&mut world, "Sim.running = true").unwrap();
        assert!(!world.resource::<Time<Physics>>().is_paused());

        engine.eval(&mut world, "Sim.gravityStrength = 5").unwrap();
        assert_eq!(world.resource::<GravitySetting>().strength, 5.0);
        assert!((world.resource::<Gravity>().0 - Vec2::NEG_Y * 5.0).length() < 1e-5);

        engine
            .eval(
                &mut world,
                &format!("Sim.gravityAngleOffset = {}", std::f32::consts::FRAC_PI_2),
            )
            .unwrap();
        assert!((world.resource::<Gravity>().0 - Vec2::X * 5.0).length() < 1e-5);

        engine
            .eval(&mut world, "Sim.gravitySwitch = false")
            .unwrap();
        engine.eval(&mut world, "Sim.gravityStrength = 7").unwrap();
        assert_eq!(world.resource::<Gravity>().0, Vec2::ZERO);
        assert_eq!(world.resource::<GravitySetting>().strength, 7.0);
    }

    #[test]
    fn render_bindings_apply_and_remove_invalid_results() {
        let mut engine = ScriptEngine::default();
        let mut world = world();

        engine.eval(&mut world, "GUI.scale = { 2 }").unwrap();
        assert!(engine.evaluate_bindings(&mut world).is_empty());
        assert_eq!(world.resource::<AppConfig>().ui_scale, 2.0);

        engine.eval(&mut world, "GUI.scale = { 0 }").unwrap();
        let errors = engine.evaluate_bindings(&mut world);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("GUI.scale"));
        assert_eq!(engine.runtime.binding_count(), 0);
    }

    #[test]
    fn scene_properties_filter_mix_and_apply_to_every_compatible_object() {
        let mut engine = ScriptEngine::default();
        let mut world = world();
        let first = world
            .spawn((
                RigidBody::Dynamic,
                Collider::rectangle(2.0, 3.0),
                ColliderDensity(2.0),
                Position(Vec2::new(1.0, 2.0)),
                Rotation::default(),
                LinearVelocity(Vec2::ZERO),
                AngularVelocity(0.0),
                Transform::default(),
                GlobalTransform::default(),
                CollisionLayers::from_bits(3, 3),
                Restitution::new(0.5),
                RefractiveIndex(1.5),
                ColorComponent(Hsva::new(0.5, 0.6, 0.7, 0.8)),
                Attraction::default(),
            ))
            .id();
        let second = world
            .spawn((
                RigidBody::Dynamic,
                Collider::circle(1.0),
                ColliderDensity(2.0),
                Position(Vec2::new(9.0, 2.0)),
                Rotation::default(),
                LinearVelocity(Vec2::ZERO),
                AngularVelocity(0.0),
                Transform::default(),
                GlobalTransform::default(),
                CollisionLayers::from_bits(3, 3),
                Restitution::new(0.5),
                RefractiveIndex(1.5),
                ColorComponent(Hsva::new(0.5, 0.6, 0.7, 0.8)),
                Attraction::default(),
            ))
            .id();

        let properties = engine.selection_properties(&mut world, &[first, second]);
        assert_eq!(
            properties.iter().find(|p| p.name == "pos").unwrap().value,
            "?"
        );
        assert!(
            properties
                .iter()
                .find(|p| p.name == "area")
                .unwrap()
                .read_only
        );
        assert_eq!(
            properties
                .iter()
                .find(|p| p.name == "colorHSVA")
                .unwrap()
                .value,
            "[180, 0.6, 0.7, 0.8]"
        );
        assert!(properties.iter().any(|p| p.name == "angle"));
        assert!(!properties.iter().any(|p| p.name == "rotation"));

        engine
            .set_selection_property(&mut world, &[first, second], "pos", "[3, 4]")
            .unwrap();
        assert_eq!(world.get::<Position>(first).unwrap().0, Vec2::new(3.0, 4.0));
        assert_eq!(
            world.get::<Position>(second).unwrap().0,
            Vec2::new(3.0, 4.0)
        );
        engine
            .set_selection_property(&mut world, &[first, second], "angvel", "2")
            .unwrap();
        engine
            .set_selection_property(&mut world, &[first, second], "attractionType", "1")
            .unwrap();
        engine
            .set_selection_property(&mut world, &[first, second], "color", "[1, 0, 0, 1]")
            .unwrap();
        assert_eq!(world.get::<AngularVelocity>(first).unwrap().0, 2.0);
        assert_eq!(
            world.get::<Attraction>(second).unwrap().falloff,
            AttractionFalloff::Linear
        );
        let color = world.get::<ColorComponent>(first).unwrap().0;
        assert_eq!(color.to_rgba_unmultiplied(), [1.0, 0.0, 0.0, 1.0]);
        assert!(
            engine
                .set_selection_property(&mut world, &[first], "area", "10")
                .unwrap_err()
                .contains("read-only")
        );
    }

    #[test]
    fn boxes_and_circles_expose_their_geometry() {
        let mut engine = ScriptEngine::default();
        let mut world = world();
        let rectangle = world
            .spawn(PhysicalObject::rect(Vec2::new(2.0, 3.0), Vec3::ZERO))
            .id();
        let circle = world.spawn(PhysicalObject::ball(1.5, Vec3::ZERO)).id();

        let rectangle_props = engine.selection_properties(&mut world, &[rectangle]);
        assert_eq!(
            rectangle_props
                .iter()
                .find(|p| p.name == "size")
                .unwrap()
                .value,
            "[2, 3]"
        );
        assert!(!rectangle_props.iter().any(|p| p.name == "radius"));
        let circle_props = engine.selection_properties(&mut world, &[circle]);
        assert_eq!(
            circle_props
                .iter()
                .find(|p| p.name == "radius")
                .unwrap()
                .value,
            "1.5"
        );
        assert!(!circle_props.iter().any(|p| p.name == "size"));

        engine
            .set_selection_property(&mut world, &[rectangle], "size", "[4, 5]")
            .unwrap();
        engine
            .set_selection_property(&mut world, &[circle], "radius", "2")
            .unwrap();
        assert_eq!(
            world
                .get::<Collider>(rectangle)
                .unwrap()
                .shape()
                .as_cuboid()
                .unwrap()
                .half_extents,
            Vec2::new(2.0, 2.5)
        );
        assert_eq!(world.get::<CircleVisual>(circle).unwrap().0, 2.0);
    }

    #[test]
    fn attachments_expose_global_pos_and_rotation() {
        let mut engine = ScriptEngine::default();
        let mut world = world();
        let parent_transform = GlobalTransform::from(
            Transform::from_xyz(10.0, 20.0, 0.0)
                .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
        );
        let parent = world.spawn(parent_transform).id();
        let entity = world
            .spawn((
                AttachmentKind::Laser,
                Transform::default(),
                GlobalTransform::from(
                    Transform::from_xyz(12.0, 23.0, 1.0).with_rotation(Quat::from_rotation_z(0.4)),
                ),
                ChildOf(parent),
            ))
            .id();

        let properties = engine.selection_properties(&mut world, &[entity]);
        assert_eq!(
            properties.iter().find(|p| p.name == "pos").unwrap().value,
            "[12, 23]"
        );
        assert!(properties.iter().any(|p| p.name == "rotation"));
        assert!(!properties.iter().any(|p| p.name == "angle"));

        engine
            .set_selection_property(&mut world, &[entity], "pos", "[8, 25]")
            .unwrap();
        engine
            .set_selection_property(&mut world, &[entity], "rotation", "0.7")
            .unwrap();
        let transform = world.get::<Transform>(entity).unwrap();
        assert!(
            (transform.translation.truncate()
                - attachment_local_position(Some(&parent_transform), Vec2::new(8.0, 25.0)))
            .length()
                < 1.0e-5
        );
        let expected = attachment_local_rotation(Some(&parent_transform), 0.7);
        assert!((transform.rotation * Vec3::X - expected * Vec3::X).length() < 1.0e-5);
    }

    #[test]
    fn scene_property_bindings_are_evaluated_for_entity_instances() {
        let mut engine = ScriptEngine::default();
        let mut world = world();
        let entity = world
            .spawn((
                Position::default(),
                Transform::default(),
                GlobalTransform::default(),
            ))
            .id();

        engine
            .set_selection_property(&mut world, &[entity], "pos", "{ [2, 5] }")
            .unwrap();
        assert!(engine.evaluate_bindings(&mut world).is_empty());
        assert_eq!(
            world.get::<Position>(entity).unwrap().0,
            Vec2::new(2.0, 5.0)
        );
    }

    #[test]
    fn specialized_properties_only_apply_to_compatible_selected_objects() {
        let mut engine = ScriptEngine::default();
        let mut world = world();
        let spring = world
            .spawn(SpringObject {
                end_a: SpringEnd::sky(Vec2::ZERO),
                end_b: SpringEnd::sky(Vec2::X),
                target_length: 1.0,
                spring_constant: 100.0,
                damping: 0.2,
                unit_size: 0.5,
                unit_count: 2,
            })
            .id();
        let hinge = world
            .spawn((MotorComponent::default(), Transform::default()))
            .id();
        let names = engine
            .selection_properties(&mut world, &[spring, hinge])
            .into_iter()
            .map(|property| property.name)
            .collect::<Vec<_>>();
        for name in [
            "constant",
            "dampingFactor",
            "length",
            "motor",
            "motorSpeed",
            "motorTorque",
            "size",
        ] {
            assert!(names.contains(&name));
        }

        engine
            .set_selection_property(&mut world, &[spring, hinge], "constant", "250")
            .unwrap();
        engine
            .set_selection_property(&mut world, &[spring, hinge], "motorSpeed", "3")
            .unwrap();
        assert_eq!(
            world.get::<SpringObject>(spring).unwrap().spring_constant,
            250.0
        );
        assert_eq!(world.get::<MotorComponent>(hinge).unwrap().vel, 3.0);
    }

    #[test]
    fn attachment_specific_properties_are_component_driven() {
        let mut engine = ScriptEngine::default();
        let mut world = world();
        let laser = world
            .spawn((
                LaserSettings {
                    size: 1.0,
                    fade_distance: 300.0,
                },
                AttachmentKind::Laser,
                Transform::default(),
                GlobalTransform::default(),
            ))
            .id();
        let thruster = world.spawn(ThrusterSettings::default()).id();

        engine
            .set_selection_property(&mut world, &[laser, thruster], "fadeDist", "42")
            .unwrap();
        engine
            .set_selection_property(&mut world, &[laser, thruster], "force", "12")
            .unwrap();
        assert_eq!(
            world.get::<LaserSettings>(laser).unwrap().fade_distance,
            42.0
        );
        assert_eq!(world.get::<ThrusterSettings>(thruster).unwrap().force, 12.0);
    }

    #[test]
    fn spring_and_handle_sizes_are_independent() {
        let mut engine = ScriptEngine::default();
        let mut world = world();
        let spring = world
            .spawn(SpringObject {
                end_a: SpringEnd::sky(Vec2::ZERO),
                end_b: SpringEnd::sky(Vec2::X),
                target_length: 1.0,
                spring_constant: 100.0,
                damping: 0.2,
                unit_size: 0.5,
                unit_count: 2,
            })
            .id();
        let handle = world
            .spawn((
                SpringEndHandle {
                    spring,
                    end: SpringEndIndex::A,
                },
                Transform::from_scale(Vec3::splat(0.3)),
            ))
            .id();

        engine
            .set_selection_property(&mut world, &[spring], "size", "1")
            .unwrap();
        assert_eq!(world.get::<SpringObject>(spring).unwrap().unit_size, 1.0);
        assert_eq!(world.get::<Transform>(handle).unwrap().scale.x, 0.3);

        engine
            .set_selection_property(&mut world, &[handle], "size", "0.8")
            .unwrap();
        assert_eq!(world.get::<Transform>(handle).unwrap().scale.x, 0.8);
        assert_eq!(world.get::<SpringObject>(spring).unwrap().unit_size, 1.0);
    }
}
