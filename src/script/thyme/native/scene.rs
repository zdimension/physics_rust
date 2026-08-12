use super::*;

fn entity_by_id(
    world: &mut World,
    registry: &mut NativeRegistry,
    arguments: &[Value],
    name: &str,
) -> Result<Value, HostError> {
    let Value::Number(Number::Int(id)) = &arguments[0] else {
        return Err(type_error(name, "int"));
    };
    let Some(entity) = real_entity(world, *id) else {
        return Ok(Value::Null);
    };
    let object = registry.ensure_entity(entity);
    Ok(Value::Object(registry.instance(object)?.object.clone()))
}

fn queue_scene(world: &mut World, arguments: &[Value], import: bool) -> Result<Value, HostError> {
    let Value::Str(path) = &arguments[0] else {
        return Err(type_error("scene path", "string"));
    };
    super::super::scene::queue_path(world, path.as_ref(), import)
        .map_err(|error| HostError::new(HostErrorKind::Other, error))?;
    Ok(Value::Void)
}

pub(super) fn real_entity(world: &World, id: i32) -> Option<Entity> {
    let raw = Entity::from_raw_u32(id as u32)?;
    let entity = world.entities().resolve_from_index(raw.index());
    world.get_entity(entity).is_ok().then_some(entity)
}

native_class!(
    pub(super) SCENE = "Scene",
    [
        native_method!("entityByID", 1, |world, registry, _, arguments| {
            entity_by_id(world, registry, arguments, "entityByID")
        }),
        native_method!("entityByGeomID", 1, |world, registry, _, arguments| {
            entity_by_id(world, registry, arguments, "entityByGeomID")
        }),
        native_method!("Open", 1, |world, _, _, arguments| {
            queue_scene(world, arguments, false)
        }),
        native_method!("loadScene", 1, |world, _, _, arguments| {
            queue_scene(world, arguments, false)
        }),
        native_method!("importPhunlet", 1, |world, _, _, arguments| {
            queue_scene(world, arguments, true)
        }),
        native_method!("Clear", 0, |world: &mut World, _, _, _| {
            super::super::scene::clear(world);
            Ok(Value::Void)
        }),
        native_method!("New", 0, |world: &mut World, _, _, _| {
            super::super::scene::queue_new(world);
            Ok(Value::Void)
        }),
        native_builder_method!("addBox", spawn_default_box),
        native_builder_method!("addCircle", spawn_default_circle),
        native_host_method!("addFixjoint", 1, add_fixjoint),
        native_host_method!("addHinge", 1, add_hinge),
        native_host_method!("addPolygon", 1, add_polygon),
    ]
);

native_class!(
    pub(super) CAMERA = "Camera",
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
