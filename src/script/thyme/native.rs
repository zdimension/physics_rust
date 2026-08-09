use std::collections::HashMap;

use ::thyme::{
    Function, Host, HostError, HostErrorKind, IntrinsicId, NativeObjectId, Object, PropertyId,
    ResolvedProperty, Runtime, Symbol, Value, parse::Number,
};
use avian2d::prelude::{Gravity, Physics, PhysicsTime};
use bevy::{
    app::AppExit,
    ecs::world::World,
    math::Vec2,
    prelude::{Entity, Time},
};

use crate::{config::AppConfig, ui::GravitySetting};

type Getter = fn(&World, Option<Entity>) -> Result<Value, HostError>;
type Setter = fn(&mut World, Option<Entity>, &Value) -> Result<(), HostError>;
type Method = fn(&mut World, Option<Entity>, &[Value]) -> Result<Value, HostError>;

enum NativeMember {
    Property {
        name: &'static str,
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

static CLASSES: &[&NativeClass] = &[&SYSTEM, &GUI, &SIM];

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
            globals: Vec::new(),
            next_id: 0,
        };
        for class in 0..CLASSES.len() {
            registry.register_global(runtime, class);
        }
        registry
    }

    fn register_global(&mut self, runtime: &Runtime, class: usize) {
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
        runtime.define_read_only_global(definition.name, Value::Object(object.clone()));
        self.instances.insert(
            id,
            NativeInstance {
                class,
                entity: None,
                object,
            },
        );
        self.globals.push(id);
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
    pub(crate) fn eval(&self, world: &mut World, source: &str) -> Result<Value, String> {
        let mut host = WorldHost {
            world,
            registry: &self.registry,
        };
        self.runtime.eval(&mut host, source)
    }

    pub(crate) fn evaluate_bindings(&self, world: &mut World) -> Vec<String> {
        let mut host = WorldHost {
            world,
            registry: &self.registry,
        };
        let mut errors = Vec::new();
        for &object in &self.registry.globals {
            for error in self.runtime.evaluate_property_bindings(&mut host, object) {
                errors.push(format!(
                    "{}.{}: {}",
                    self.registry.classes[self.registry.instance(object).unwrap().class]
                        .definition
                        .name,
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
        Ok(class.properties.get(name).map(|&property| {
            let NativeMember::Property { set, .. } =
                &class.definition.members[property.into_raw() as usize]
            else {
                unreachable!()
            };
            ResolvedProperty::new(property, set.is_some())
        }))
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
        let engine = ScriptEngine::default();
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
        let engine = ScriptEngine::default();
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
        let engine = ScriptEngine::default();
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
}
