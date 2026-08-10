use std::{collections::HashMap, rc::Rc};

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
    prelude::{ChildOf, Color, Entity, GlobalTransform, Time, Transform},
};
use bevy_egui::egui::ecolor::Hsva;

use super::Console;
use crate::{
    config::AppConfig,
    grid::GridSettings,
    lyon_compat::Shape,
    mouse::select::SelectionConfig,
    mouse_tracking::{MainCamera, MousePosWorld},
    objects::{
        ColorComponent, MotorComponent,
        air::AirSettings,
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
        add_object::{AttachmentKind, spawn_default_box},
        drag::DragConfig,
        gear::GearSettings,
        r#move::attachment_local_position,
        rotate::attachment_local_rotation,
    },
    ui::GravitySetting,
};

type Getter = fn(&World, Option<Entity>) -> Result<Value, HostError>;
type Setter = fn(&mut World, Option<Entity>, &Value) -> Result<(), HostError>;
type Method = fn(&mut WorldHost<'_>, Option<Entity>, &[Value]) -> Result<Value, HostError>;
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
            call: |host, entity, arguments| {
                ($call)(&mut *host.world, &mut *host.registry, entity, arguments)
            },
        }
    };
}

macro_rules! native_builder_method {
    ($name:literal, $spawn:path) => {
        NativeMember::Method {
            name: $name,
            arity: 1,
            call: |host, _, arguments| {
                let Value::Function(builder) = &arguments[0] else {
                    return Err(type_error($name, "zero-argument function"));
                };
                if builder.arity() != 0 {
                    return Err(type_error($name, "zero-argument function"));
                }
                let entity = $spawn(host.world);
                let id = host.registry.ensure_entity(entity);
                let object = host.registry.instance(id)?.object.clone();
                let runtime = host.runtime;
                runtime
                    .call_function_with_receiver(host, builder, &[], object.clone())
                    .map_err(|error| HostError::new(HostErrorKind::Intrinsic, error))?;
                Ok(Value::Object(object))
            },
        }
    };
}

macro_rules! native_read_only {
    ($name:literal, $get:expr) => {
        NativeMember::Property {
            name: $name,
            applies: None,
            get: $get,
            set: None,
        }
    };
}

macro_rules! resource_property {
    ($name:literal, $resource:ty, $field:ident, $kind:ident) => {
        native_property!(
            $name,
            $kind,
            |world: &World, _| world.resource::<$resource>().$field,
            |world: &mut World, _, value| {
                world.resource_mut::<$resource>().$field = value;
                Ok(())
            }
        )
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

fn int_at_least_two(value: i32, name: &str) -> Result<u32, HostError> {
    u32::try_from(value)
        .ok()
        .filter(|value| *value >= 2)
        .ok_or_else(|| type_error(name, "int >= 2"))
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

fn color_value(color: Color) -> Value {
    let color = color.to_srgba();
    floats([color.red, color.green, color.blue, color.alpha])
}

fn main_camera(world: &World) -> Result<Entity, HostError> {
    world
        .iter_entities()
        .find(|entity| entity.contains::<MainCamera>())
        .map(|entity| entity.id())
        .ok_or_else(|| object_error("Camera"))
}

fn get_camera_pan(world: &World, _: Option<Entity>) -> Result<Value, HostError> {
    let pan = world
        .get::<Transform>(main_camera(world)?)
        .ok_or_else(|| object_error("Camera.pan"))?
        .translation
        .truncate();
    Ok(floats(pan.to_array()))
}

fn set_camera_pan(world: &mut World, _: Option<Entity>, value: &Value) -> Result<(), HostError> {
    let pan = Vec2::from_array(float_list(value, "pan")?);
    let camera = main_camera(world)?;
    let mut transform = world
        .get_mut::<Transform>(camera)
        .ok_or_else(|| object_error("Camera.pan"))?;
    transform.translation.x = pan.x;
    transform.translation.y = pan.y;
    Ok(())
}

fn get_camera_zoom(world: &World, _: Option<Entity>) -> Result<Value, HostError> {
    let scale = world
        .get::<Transform>(main_camera(world)?)
        .ok_or_else(|| object_error("Camera.zoom"))?
        .scale
        .x
        .abs();
    Ok(Value::from(scale.recip()))
}

fn set_camera_zoom(world: &mut World, _: Option<Entity>, value: &Value) -> Result<(), HostError> {
    let zoom = number(value, "zoom")?;
    if !zoom.is_finite() || zoom <= 0.0 {
        return Err(type_error("zoom", "positive number"));
    }
    let camera = main_camera(world)?;
    let mut transform = world
        .get_mut::<Transform>(camera)
        .ok_or_else(|| object_error("Camera.zoom"))?;
    transform.scale.x = zoom.recip();
    transform.scale.y = zoom.recip();
    Ok(())
}

fn get_camera_rotation(world: &World, _: Option<Entity>) -> Result<Value, HostError> {
    let rotation = world
        .get::<Transform>(main_camera(world)?)
        .ok_or_else(|| object_error("Camera.rotation"))?
        .rotation
        .to_euler(EulerRot::XYZ)
        .2;
    Ok(Value::from(rotation))
}

fn set_camera_rotation(
    world: &mut World,
    _: Option<Entity>,
    value: &Value,
) -> Result<(), HostError> {
    let rotation = number(value, "rotation")?;
    world
        .get_mut::<Transform>(main_camera(world)?)
        .ok_or_else(|| object_error("Camera.rotation"))?
        .rotation = Quat::from_rotation_z(rotation);
    Ok(())
}

fn event_object(this: Object) -> Object {
    let event = Object::new();
    event.set_field("handled", Value::Bool(false));
    event.set_field("this", Value::Object(this));
    event
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

fn get_entity_id(_: &World, entity: Entity) -> Result<Value, HostError> {
    Ok(Value::from(entity.index_u32() as i32))
}

#[derive(Copy, Clone)]
enum LoadId {
    Entity,
    Geometry,
}

fn load_id(member: &NativeMember) -> Option<LoadId> {
    match member.name() {
        "entityID" => Some(LoadId::Entity),
        "geomID" => Some(LoadId::Geometry),
        _ => None,
    }
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

native_class!(
    APP = "App",
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
    APP_GRID = "Grid",
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

native_class!(
    SIM = "Sim",
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
        resource_property!("airFrictionLinear", AirSettings, linear_term, float),
        resource_property!("airFrictionQuadratic", AirSettings, quadratic_term, float),
        resource_property!("airFrictionMultiplier", AirSettings, multiplier, float),
        resource_property!("airSwitch", AirSettings, enabled, bool),
        resource_property!("windAngle", AirSettings, wind_direction, float),
        resource_property!("windStrength", AirSettings, wind_speed, float),
    ]
);

native_class!(
    CONSOLE = "Console",
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

fn entity_by_id(
    world: &mut World,
    registry: &mut NativeRegistry,
    arguments: &[Value],
    name: &str,
) -> Result<Value, HostError> {
    let Value::Number(Number::Int(id)) = &arguments[0] else {
        return Err(type_error(name, "int"));
    };
    let Some(raw) = Entity::from_raw_u32(*id as u32) else {
        return Ok(Value::Null);
    };
    let entity = world.entities().resolve_from_index(raw.index());
    if world.get_entity(entity).is_err() {
        return Ok(Value::Null);
    }
    let object = registry.ensure_entity(entity);
    Ok(Value::Object(registry.instance(object)?.object.clone()))
}

native_class!(
    SCENE = "Scene",
    [
        native_method!("entityByID", 1, |world, registry, _, arguments| {
            entity_by_id(world, registry, arguments, "entityByID")
        }),
        native_method!("entityByGeomID", 1, |world, registry, _, arguments| {
            entity_by_id(world, registry, arguments, "entityByGeomID")
        }),
        native_builder_method!("addBox", spawn_default_box),
    ]
);

native_class!(
    CAMERA = "Camera",
    [
        NativeMember::Property {
            name: "pan",
            applies: None,
            get: get_camera_pan,
            set: Some(set_camera_pan),
        },
        NativeMember::Property {
            name: "zoom",
            applies: None,
            get: get_camera_zoom,
            set: Some(set_camera_zoom),
        },
        NativeMember::Property {
            name: "rotation",
            applies: None,
            get: get_camera_rotation,
            set: Some(set_camera_rotation),
        },
    ]
);

native_class!(TOOLS = "Tools", []);

native_class!(
    DRAG_TOOL = "DragTool",
    [
        resource_property!("centerOfMass", DragConfig, drag_center_of_mass, bool),
        resource_property!("maxForce", DragConfig, max_force, float),
        resource_property!("strength", DragConfig, strength, float),
    ]
);

native_class!(
    GEAR_TOOL = "GearTool",
    [
        resource_property!("cogSize", GearSettings, teeth_size, float),
        resource_property!("inside", GearSettings, internal, bool),
        resource_property!("outside", GearSettings, external, bool),
        resource_property!("thickness", GearSettings, hollow_thickness, float),
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
        scene_property!("entityID", |_, _| true, get_entity_id),
        scene_property!("geomID", has_area, get_entity_id),
        scene_component_value!("fadeDist", LaserSettings, fade_distance, float),
        scene_component_value!("force", ThrusterSettings, force, float),
        scene_component_value!("length", SpringObject, target_length, float),
        scene_component_value!("motor", MotorComponent, enabled, bool),
        scene_component_value!("motorSpeed", MotorComponent, vel, float),
        scene_component_value!("motorTorque", MotorComponent, torque, float),
        native_event!("onClick"),
        native_event!("onKey"),
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

static CLASSES: &[&NativeClass] = &[
    &SYSTEM,
    &GUI,
    &SIM,
    &CONSOLE,
    &APP,
    &APP_GRID,
    &SCENE,
    &CAMERA,
    &TOOLS,
    &DRAG_TOOL,
    &GEAR_TOOL,
    &SCENE_OBJECT,
];
const APP_CLASS: usize = 4;
const GUI_CLASS: usize = 1;
const GRID_CLASS: usize = 5;
const SCENE_NAMESPACE_CLASS: usize = 6;
const CAMERA_CLASS: usize = 7;
const TOOLS_CLASS: usize = 8;
const DRAG_TOOL_CLASS: usize = 9;
const GEAR_TOOL_CLASS: usize = 10;
const SCENE_CLASS: usize = 11;

struct RegisteredClass {
    definition: &'static NativeClass,
    properties: HashMap<Symbol, PropertyId>,
    events: HashMap<Symbol, usize>,
}

struct NativeInstance {
    class: usize,
    entity: Option<Entity>,
    load_entity_id: Option<i32>,
    load_geom_id: Option<i32>,
    #[allow(dead_code)]
    object: Object,
}

struct NativeRegistry {
    classes: Vec<RegisteredClass>,
    instances: HashMap<NativeObjectId, NativeInstance>,
    entities: HashMap<Entity, NativeObjectId>,
    globals: Vec<NativeObjectId>,
    loading_scene: bool,
    load_entities: HashMap<i32, Entity>,
    load_geometries: HashMap<i32, Entity>,
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
                events: definition
                    .members
                    .iter()
                    .enumerate()
                    .filter_map(|(index, member)| match member {
                        NativeMember::Event { name } => Some((Symbol::from(*name), index)),
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
            loading_scene: false,
            load_entities: HashMap::new(),
            load_geometries: HashMap::new(),
            next_id: 0,
        };
        registry.register_global(runtime, 0);
        let gui = registry.register_global(runtime, GUI_CLASS);
        registry.register_global(runtime, 2);
        registry.register_global(runtime, 3);
        let app = registry.register_global(runtime, APP_CLASS);
        let scene = registry.register_global(runtime, SCENE_NAMESPACE_CLASS);
        let tools = registry.register_global(runtime, TOOLS_CLASS);
        let grid = registry.register_namespace(GRID_CLASS);
        let camera = registry.register_namespace(CAMERA_CLASS);
        let drag = registry.register_namespace(DRAG_TOOL_CLASS);
        let gear = registry.register_namespace(GEAR_TOOL_CLASS);
        app.define_read_only_field("Grid", Value::Object(grid));
        app.define_read_only_field("GUI", Value::Object(gui));
        scene.define_read_only_field("Camera", Value::Object(camera));
        tools.define_read_only_field("DragTool", Value::Object(drag));
        tools.define_read_only_field("GearTool", Value::Object(gear));
        registry
    }

    fn register_global(&mut self, runtime: &Runtime, class: usize) -> Object {
        let (id, object) = self.register_instance(class, None);
        let definition = self.classes[class].definition;
        runtime.define_read_only_global(definition.name, Value::Object(object.clone()));
        self.globals.push(id);
        object
    }

    fn register_namespace(&mut self, class: usize) -> Object {
        let (id, object) = self.register_instance(class, None);
        self.globals.push(id);
        object
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
                load_entity_id: None,
                load_geom_id: None,
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

    fn instance_mut(&mut self, id: NativeObjectId) -> Result<&mut NativeInstance, HostError> {
        self.instances
            .get_mut(&id)
            .ok_or_else(|| HostError::new(HostErrorKind::UnknownObject, "unknown object"))
    }

    fn begin_scene_load(&mut self) {
        self.loading_scene = true;
        self.load_entities.clear();
        self.load_geometries.clear();
    }

    fn end_scene_load(&mut self) {
        self.loading_scene = false;
        self.load_entities.clear();
        self.load_geometries.clear();
        for instance in self.instances.values_mut() {
            instance.load_entity_id = None;
            instance.load_geom_id = None;
        }
    }

    fn set_load_id(
        &mut self,
        object: NativeObjectId,
        kind: LoadId,
        id: i32,
    ) -> Result<(), HostError> {
        let instance = self.instance_mut(object)?;
        let entity = instance.entity.ok_or_else(|| object_error("ID"))?;
        let old = match kind {
            LoadId::Entity => instance.load_entity_id.replace(id),
            LoadId::Geometry => instance.load_geom_id.replace(id),
        };
        let ids = match kind {
            LoadId::Entity => &mut self.load_entities,
            LoadId::Geometry => &mut self.load_geometries,
        };
        if let Some(old) = old
            && ids.get(&old) == Some(&entity)
        {
            ids.remove(&old);
        }
        ids.insert(id, entity);
        Ok(())
    }

    #[allow(dead_code)]
    fn load_entity(&self, kind: LoadId, id: i32) -> Option<Entity> {
        match kind {
            LoadId::Entity => &self.load_entities,
            LoadId::Geometry => &self.load_geometries,
        }
        .get(&id)
        .copied()
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
            runtime: &self.runtime,
            world,
            registry: &mut self.registry,
        };
        self.runtime.eval(&mut host, source)
    }

    #[allow(dead_code)]
    pub(crate) fn eval_scene(&mut self, world: &mut World, source: &str) -> Result<Value, String> {
        self.registry.begin_scene_load();
        let result = self.eval(world, source);
        self.registry.end_scene_load();
        result
    }

    fn event_handler(&self, object: NativeObjectId, name: &str) -> Option<(Object, Function)> {
        let object = self.registry.instance(object).ok()?.object.clone();
        let Value::Function(function) = object.field(name)? else {
            return None;
        };
        (function.arity() == 1).then_some((object, function))
    }

    fn call_event(
        &mut self,
        world: &mut World,
        function: &Function,
        event: Object,
    ) -> Result<Value, String> {
        let mut host = WorldHost {
            runtime: &self.runtime,
            world,
            registry: &mut self.registry,
        };
        self.runtime
            .call_function(&mut host, function, &[Value::Object(event)])
    }

    pub(crate) fn dispatch_click(
        &mut self,
        world: &mut World,
        entity: Entity,
        pos: Vec2,
    ) -> Option<String> {
        let id = self.registry.ensure_entity(entity);
        let (object, function) = self.event_handler(id, "onClick")?;
        let event = event_object(object);
        event.set_field("pos", floats(pos.to_array()));
        self.call_event(world, &function, event)
            .err()
            .map(|error| format!("{entity:?}.onClick: {error}"))
    }

    pub(crate) fn dispatch_key(
        &mut self,
        world: &mut World,
        pressed: bool,
        key_code: &str,
        key_char: Option<&str>,
    ) -> Vec<String> {
        let handlers = self
            .registry
            .entities
            .iter()
            .filter(|(entity, _)| world.get_entity(**entity).is_ok())
            .filter_map(|(&entity, &id)| {
                self.event_handler(id, "onKey")
                    .map(|(object, function)| (entity, object, function))
            })
            .collect::<Vec<_>>();
        handlers
            .into_iter()
            .filter_map(|(entity, object, function)| {
                let event = event_object(object);
                event.set_field("pressed", Value::Bool(pressed));
                event.set_field("keyCode", Value::Str(Rc::from(key_code)));
                if let Some(key_char) = key_char {
                    event.set_field("keyChar", Value::Str(Rc::from(key_char)));
                }
                self.call_event(world, &function, event)
                    .err()
                    .map(|error| format!("{entity:?}.onKey: {error}"))
            })
            .collect()
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
            let (name, read_only, values) = match member {
                NativeMember::Property {
                    name,
                    applies: Some(applies),
                    get,
                    set,
                } => (
                    *name,
                    set.is_none(),
                    objects
                        .iter()
                        .filter_map(|&(entity, object)| {
                            applies(world, entity).then(|| {
                                self.runtime
                                    .property_binding(object, PropertyId::from_raw(index as u64))
                                    .map(Value::Function)
                                    .unwrap_or_else(|| {
                                        get(world, Some(entity)).unwrap_or(Value::Undefined)
                                    })
                            })
                        })
                        .collect::<Vec<_>>(),
                ),
                NativeMember::Event { name } => (
                    *name,
                    false,
                    objects
                        .iter()
                        .map(|&(_, object)| {
                            self.registry
                                .instance(object)
                                .unwrap()
                                .object
                                .field(name)
                                .unwrap_or(Value::Undefined)
                        })
                        .collect(),
                ),
                _ => continue,
            };
            let Some((value, rest)) = values.split_first() else {
                continue;
            };
            let mixed = rest.iter().any(|other| other != value);
            result.push(SceneProperty {
                name,
                value: if mixed { "?".into() } else { value.to_string() },
                read_only,
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
        let symbol = Symbol::new(name);
        if let Some(&event) = self.registry.classes[SCENE_CLASS].events.get(&symbol) {
            let NativeMember::Event { name } = &SCENE_OBJECT.members[event] else {
                unreachable!()
            };
            let targets = entities
                .iter()
                .copied()
                .filter(|entity| world.get_entity(*entity).is_ok())
                .collect::<Vec<_>>();
            if targets.is_empty() {
                return Err(format!("no selected object has {name}"));
            }
            let value = self.eval(world, source)?;
            for entity in targets {
                let object = self.registry.ensure_entity(entity);
                self.registry
                    .instance(object)
                    .unwrap()
                    .object
                    .set_field(*name, value.clone());
            }
            return Ok(());
        }
        let property = *self.registry.classes[SCENE_CLASS]
            .properties
            .get(&symbol)
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
            runtime: &self.runtime,
            world,
            registry: &mut self.registry,
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
            runtime: &self.runtime,
            world,
            registry: &mut self.registry,
        };
        let mut errors = Vec::new();
        for object in objects {
            for error in self.runtime.evaluate_property_bindings(&mut host, object) {
                let instance = host.registry.instance(object).unwrap();
                errors.push(format!(
                    "{}.{}: {}",
                    instance.entity.map_or(
                        host.registry.classes[instance.class]
                            .definition
                            .name
                            .to_owned(),
                        |entity| format!("{entity:?}")
                    ),
                    host.registry.property_name(object, error.property),
                    error.message
                ));
            }
        }
        errors
    }
}

struct WorldHost<'a> {
    runtime: &'a Runtime,
    world: &'a mut World,
    registry: &'a mut NativeRegistry,
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
        let member = &class.definition.members[property.into_raw() as usize];
        let NativeMember::Property { applies, set, .. } = member else {
            unreachable!()
        };
        if let (Some(applies), Some(entity)) = (applies, instance.entity)
            && !applies(self.world, entity)
        {
            return Ok(None);
        }
        Ok(Some(ResolvedProperty::new(
            property,
            set.is_some() || self.registry.loading_scene && load_id(member).is_some(),
        )))
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
        if self.registry.loading_scene
            && let Some(kind) = load_id(member)
        {
            let id = match kind {
                LoadId::Entity => instance.load_entity_id,
                LoadId::Geometry => instance.load_geom_id,
            };
            return Ok(id.map(Value::from).unwrap_or(Value::Undefined));
        }
        get(self.world, instance.entity)
    }

    fn set_property(
        &mut self,
        object: NativeObjectId,
        property: PropertyId,
        value: &Value,
    ) -> Result<(), HostError> {
        let (entity, kind, name, set) = {
            let (instance, member) = self.registry.member(object, property)?;
            let NativeMember::Property { name, set, .. } = member else {
                return Err(HostError::new(
                    HostErrorKind::UnknownProperty,
                    "not a property",
                ));
            };
            (instance.entity, load_id(member), *name, *set)
        };
        if self.registry.loading_scene
            && let Some(kind) = kind
        {
            let Value::Number(Number::Int(id)) = value else {
                return Err(type_error(name, "int"));
            };
            return self.registry.set_load_id(object, kind, *id);
        }
        let Some(set) = set else {
            return Err(HostError::new(
                HostErrorKind::Other,
                format!("{name} is read-only"),
            ));
        };
        set(self.world, entity, value)
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
        let entity = instance.entity;
        let Some(NativeMember::Method { call, .. }) = self
            .registry
            .classes
            .get(class)
            .and_then(|class| class.definition.members.get(member))
        else {
            return Err(HostError::new(HostErrorKind::Intrinsic, "unknown method"));
        };
        let call = *call;
        call(self, entity, arguments)
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
        world.insert_resource(Console::default());
        world.insert_resource(GravitySetting::default());
        world.insert_resource(Gravity(Vec2::NEG_Y * 9.81));
        world.insert_resource(GridSettings::default());
        world.insert_resource(AirSettings::default());
        world.insert_resource(DragConfig::default());
        world.insert_resource(GearSettings::default());
        world.insert_resource(SelectionConfig::default());
        world.insert_resource(MousePosWorld::default());
        let mut physics = Time::<Physics>::default();
        physics.pause();
        world.insert_resource(physics);
        world.insert_resource(Time::<()>::default());
        world.spawn((MainCamera, Transform::default()));
        world
    }

    fn scene_world() -> World {
        let mut world = world();
        let scene = world.spawn_empty().id();
        world.insert_resource(crate::ui::SceneState { scene });
        world.insert_resource(crate::tools::add_object::DepthSorter::default());
        world.insert_resource(crate::palette::PaletteConfig {
            palettes: Default::default(),
            current_palette: Default::default(),
        });
        world.spawn(crate::rng::RngComponent::default());
        world
    }

    #[test]
    fn registry_preserves_names_and_resolves_case_insensitively() {
        let mut engine = ScriptEngine::default();
        let gui = engine.registry.globals[1];
        let mut world = world();
        let mut host = WorldHost {
            runtime: &engine.runtime,
            world: &mut world,
            registry: &mut engine.registry,
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
    fn app_grid_and_gui_properties_share_their_ui_resources() {
        let mut engine = ScriptEngine::default();
        let mut world = world();

        engine
            .eval(
                &mut world,
                "App.Grid.base = 256; App.Grid.numAxes = 5; App.Grid.grid = true; \
                 App.Grid.opacity = 0.4; App.Grid.snap = false",
            )
            .unwrap();
        let grid = world.resource::<GridSettings>();
        assert_eq!(grid.base, 256);
        assert_eq!(grid.axes, 5);
        assert!(grid.enabled);
        assert_eq!(grid.opacity, 0.4);
        assert!(!grid.snap);
        assert!(engine.eval(&mut world, "App.Grid.base = 1").is_err());
        assert!(engine.eval(&mut world, "App.Grid.numAxes = 1").is_err());

        engine
            .eval(
                &mut world,
                "App.laserWidth = 0.3; App.polytoolPreviewColor = [0.1, 0.2, 0.3, 0.4]; \
                 App.enableScriptMenu = false; App.drawScaleIndicator = false; \
                 App.GUI.angleColor = [1, 0, 0.5, 0.75]; App.GUI.allowDrawSelect = false",
            )
            .unwrap();
        let config = world.resource::<AppConfig>();
        assert_eq!(config.laser_width, 0.3);
        assert!(!config.enable_script_menu);
        assert!(!config.draw_scale_indicator);
        let preview = config.polytool_preview_color.to_srgba();
        assert_eq!(
            [preview.red, preview.green, preview.blue, preview.alpha],
            [0.1, 0.2, 0.3, 0.4]
        );
        let angle = config.angle_color.to_srgba();
        assert_eq!(
            [angle.red, angle.green, angle.blue, angle.alpha],
            [1.0, 0.0, 0.5, 0.75]
        );
        assert!(!world.resource::<SelectionConfig>().select_by_encircling);
        assert_eq!(
            engine.eval(&mut world, "App.mousePos").unwrap().to_string(),
            "[0, 0]"
        );
        assert!(engine.eval(&mut world, "App.mousePos = [1, 2]").is_err());
        assert!(engine.eval(&mut world, "App.Grid = 1").is_err());
    }

    #[test]
    fn sim_time_and_air_properties_share_physics_state() {
        let mut engine = ScriptEngine::default();
        let mut world = world();
        world
            .resource_mut::<Time<Physics>>()
            .advance_by(std::time::Duration::from_secs_f32(2.0));

        assert_eq!(
            engine.eval(&mut world, "Sim.time").unwrap().to_string(),
            "2"
        );
        engine
            .eval(
                &mut world,
                "Sim.timeFactor = 2.5; Sim.airFrictionLinear = 1; \
                 Sim.airFrictionQuadratic = 2; Sim.airFrictionMultiplier = 3; \
                 Sim.airSwitch = false; Sim.windAngle = 0.7; Sim.windStrength = 8",
            )
            .unwrap();
        assert_eq!(world.resource::<Time<Physics>>().relative_speed(), 2.5);
        let air = world.resource::<AirSettings>();
        assert_eq!(air.linear_term, 1.0);
        assert_eq!(air.quadratic_term, 2.0);
        assert_eq!(air.multiplier, 3.0);
        assert!(!air.enabled);
        assert_eq!(air.wind_direction, 0.7);
        assert_eq!(air.wind_speed, 8.0);
        assert!(engine.eval(&mut world, "Sim.timeFactor = -1").is_err());
        assert!(engine.eval(&mut world, "Sim.time = 0").is_err());
    }

    #[test]
    fn camera_and_tool_properties_share_their_live_state() {
        let mut engine = ScriptEngine::default();
        let mut world = world();

        engine
            .eval(
                &mut world,
                "Scene.Camera.pan = [3, 4]; Scene.Camera.zoom = 200; Scene.Camera.rotation = 0.5; \
                 Tools.DragTool.centerOfMass = true; Tools.DragTool.maxForce = 12; \
                 Tools.DragTool.strength = 34; Tools.GearTool.cogSize = 0.6; \
                 Tools.GearTool.inside = true; Tools.GearTool.outside = false; \
                 Tools.GearTool.thickness = 0.8",
            )
            .unwrap();
        let camera = world
            .iter_entities()
            .find(|entity| entity.contains::<MainCamera>())
            .unwrap();
        let transform = camera.get::<Transform>().unwrap();
        assert_eq!(transform.translation.truncate(), Vec2::new(3.0, 4.0));
        assert_eq!(transform.scale.truncate(), Vec2::splat(0.005));
        assert!((transform.rotation.to_euler(EulerRot::XYZ).2 - 0.5).abs() < 1.0e-6);
        let drag = world.resource::<DragConfig>();
        assert!(drag.drag_center_of_mass);
        assert_eq!(drag.max_force, 12.0);
        assert_eq!(drag.strength, 34.0);
        let gear = world.resource::<GearSettings>();
        assert_eq!(gear.teeth_size, 0.6);
        assert!(gear.internal);
        assert!(!gear.external);
        assert_eq!(gear.hollow_thickness, 0.8);
        assert!(engine.eval(&mut world, "Scene.Camera.zoom = 0").is_err());
        assert!(engine.eval(&mut world, "Tools.DragTool = 1").is_err());
    }

    #[test]
    fn console_methods_print_values_and_clear_output() {
        let mut engine = ScriptEngine::default();
        let mut world = world();

        assert_eq!(
            engine
                .eval(&mut world, "Console.print([1, true, \"hi\"])")
                .unwrap(),
            Value::Void
        );
        assert_eq!(world.resource::<Console>().output, "[1, true, hi]");
        engine.eval(&mut world, "console.clear").unwrap();
        assert!(world.resource::<Console>().output.is_empty());
        assert!(engine.eval(&mut world, "Console = 1").is_err());
    }

    #[test]
    fn scene_add_box_uses_defaults_and_runs_its_builder_on_the_box() {
        let mut engine = ScriptEngine::default();
        let mut world = scene_world();
        let scene = world.resource::<crate::ui::SceneState>().scene;

        let Value::Object(default_box) = engine.eval(&mut world, "Scene.addBox {}").unwrap() else {
            panic!("expected box");
        };
        let entity = engine
            .registry
            .instance(default_box.native_id().unwrap())
            .unwrap()
            .entity
            .unwrap();
        assert_eq!(world.get::<Position>(entity).unwrap().0, Vec2::ZERO);
        assert_eq!(get_box_size(&world, entity).unwrap().to_string(), "[1, 1]");
        assert_eq!(world.get::<ColliderDensity>(entity).unwrap().0, 2.0);
        assert_eq!(world.get::<ChildOf>(entity).unwrap().parent(), scene);

        let Value::Object(configured_box) = engine
            .eval(
                &mut world,
                "Scene.addBox { pos = [10.0, 15.0]; size = [2, 4]; density = 3 }",
            )
            .unwrap()
        else {
            panic!("expected box");
        };
        let entity = engine
            .registry
            .instance(configured_box.native_id().unwrap())
            .unwrap()
            .entity
            .unwrap();
        assert_eq!(
            world.get::<Position>(entity).unwrap().0,
            Vec2::new(10.0, 15.0)
        );
        assert_eq!(get_box_size(&world, entity).unwrap().to_string(), "[2, 4]");
        assert_eq!(world.get::<ColliderDensity>(entity).unwrap().0, 3.0);

        assert!(engine.eval(&mut world, "Scene.addBox 1").is_err());
        assert!(engine.eval(&mut world, "Scene.addBox ((x) => {})").is_err());
        assert!(
            engine
                .eval(&mut world, "Scene.addBox { entityID = 123 }")
                .is_err()
        );
        assert!(
            engine
                .eval(&mut world, "Scene.addBox { geomID = 123 }")
                .is_err()
        );
    }

    #[test]
    fn scene_loading_temporarily_accepts_script_entity_and_geometry_ids() {
        let mut engine = ScriptEngine::default();
        let mut world = scene_world();

        let Value::Object(object) = engine
            .eval_scene(
                &mut world,
                "Scene.addBox { entityID = 12; geomID = 34; \
                 Console.print([entityID, geomID, pos]) }",
            )
            .unwrap()
        else {
            panic!("expected box");
        };
        assert_eq!(world.resource::<Console>().output, "[12, 34, [0, 0]]");

        let entity = engine
            .registry
            .instance(object.native_id().unwrap())
            .unwrap()
            .entity
            .unwrap();
        let real_id = Value::from(entity.index_u32() as i32);
        assert_eq!(
            engine.eval(&mut world, "entityID").unwrap(),
            Value::Undefined
        );
        let id = entity.index_u32() as i32;
        assert_eq!(
            engine
                .eval(&mut world, &format!("Scene.entityByID({id}).entityID"))
                .unwrap(),
            real_id
        );
        assert_eq!(
            engine
                .eval(&mut world, &format!("Scene.entityByGeomID({id}).geomID"))
                .unwrap(),
            real_id
        );

        engine.registry.begin_scene_load();
        let first = engine
            .eval(&mut world, "Scene.addBox { geomID = 123 }")
            .unwrap();
        let Value::Object(first) = first else {
            unreachable!()
        };
        let first = engine
            .registry
            .instance(first.native_id().unwrap())
            .unwrap()
            .entity
            .unwrap();
        let second = engine
            .eval(&mut world, "Scene.addBox { geomID = 123 }")
            .unwrap();
        let Value::Object(second) = second else {
            unreachable!()
        };
        let second = engine
            .registry
            .instance(second.native_id().unwrap())
            .unwrap()
            .entity
            .unwrap();
        assert_ne!(first, second);
        assert_eq!(
            engine.registry.load_entity(LoadId::Geometry, 123),
            Some(second)
        );
        engine.registry.end_scene_load();
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
    fn entity_ids_round_trip_through_scene_lookup() {
        let mut engine = ScriptEngine::default();
        let mut world = world();
        let entity = world.spawn_empty().id();
        let id = entity.index_u32() as i32;

        let property = engine
            .selection_properties(&mut world, &[entity])
            .into_iter()
            .find(|property| property.name == "entityID")
            .unwrap();
        assert_eq!(property.value, id.to_string());
        assert!(property.read_only);

        let source = format!("Scene.entityByID({id})");
        let first = engine.eval(&mut world, &source).unwrap();
        let second = engine.eval(&mut world, &source).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            engine
                .eval(&mut world, &format!("({source}).entityID"))
                .unwrap(),
            Value::from(id)
        );
        assert!(
            engine
                .eval(&mut world, &format!("({source}).entityID = 0"))
                .is_err()
        );
        assert_eq!(
            engine.eval(&mut world, "Scene.entityByID(-1)").unwrap(),
            Value::Null
        );
        assert!(engine.eval(&mut world, "Scene.entityByID(1.0)").is_err());

        world.despawn(entity);
        assert_eq!(engine.eval(&mut world, &source).unwrap(), Value::Null);
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

    #[test]
    fn scene_events_call_only_one_argument_functions() {
        let mut engine = ScriptEngine::default();
        let mut world = world();
        let clicked = world
            .spawn((
                Position::default(),
                Transform::default(),
                GlobalTransform::default(),
            ))
            .id();
        let other = world.spawn_empty().id();

        let properties = engine.selection_properties(&mut world, &[clicked]);
        assert!(properties.iter().any(|property| property.name == "onClick"));
        assert!(properties.iter().any(|property| property.name == "onKey"));
        engine
            .set_selection_property(
                &mut world,
                &[clicked],
                "onClick",
                "(e) => { e.this.pos = e.pos; e.this.wasHandled = e.handled; e.handled = true }",
            )
            .unwrap();
        assert_eq!(
            engine.dispatch_click(&mut world, clicked, Vec2::new(3.0, 4.0)),
            None
        );
        assert_eq!(
            world.get::<Position>(clicked).unwrap().0,
            Vec2::new(3.0, 4.0)
        );
        assert_eq!(
            engine.dispatch_click(&mut world, clicked, Vec2::new(3.0, 4.0)),
            None
        );
        let clicked_object = &engine
            .registry
            .instance(engine.registry.entities[&clicked])
            .unwrap()
            .object;
        assert_eq!(clicked_object.field("wasHandled"), Some(Value::Bool(false)));

        engine
            .set_selection_property(
                &mut world,
                &[clicked],
                "onKey",
                "(e) => { e.this.lastPressed = e.pressed; e.this.lastCode = e.keyCode; e.this.lastChar = e.keyChar }",
            )
            .unwrap();
        engine
            .set_selection_property(&mut world, &[other], "onKey", "(e, unused) => { 1 }")
            .unwrap();
        assert!(
            engine
                .dispatch_key(&mut world, true, "a", Some("A"))
                .is_empty()
        );
        let object = &engine
            .registry
            .instance(engine.registry.entities[&clicked])
            .unwrap()
            .object;
        assert_eq!(object.field("lastPressed"), Some(Value::Bool(true)));
        assert_eq!(object.field("lastCode"), Some(Value::Str(Rc::from("a"))));
        assert_eq!(object.field("lastChar"), Some(Value::Str(Rc::from("A"))));
        let other = &engine
            .registry
            .instance(engine.registry.entities[&other])
            .unwrap()
            .object;
        assert_eq!(other.field("lastCode"), None);
    }
}
