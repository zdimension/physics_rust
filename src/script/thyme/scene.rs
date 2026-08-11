use std::{
    borrow::Cow,
    io::{Cursor, Read},
    path::Path,
};

use ::thyme::parse::{Expr, Spanned, parse_thyme, read_auto_encoding};
use avian2d::prelude::{Physics, PhysicsTime, Position, Rotation};
use bevy::prelude::{Children, Resource, Time, Transform, Vec2, World};

use super::{Console, PendingEvents, ScriptEngine, with_script_engine};
use crate::{
    default_camera_transform,
    grid::GridSettings,
    mouse::select::SelectionConfig,
    mouse_tracking::MainCameraEntity,
    objects::{air::AirSettings, gravity::GravitySetting},
    tools::{add_object::DepthSorter, drag::DragConfig, gear::GearSettings},
    ui::{PointerToolState, SceneState, selection_overlay::OverlayState},
};

struct SceneFile {
    name: String,
    bytes: Vec<u8>,
    origin: Option<Vec2>,
}

enum Pending {
    File(SceneFile),
    New,
}

#[derive(Default, Resource)]
pub(crate) struct PendingScene(Option<Pending>);

pub(crate) fn queue_bytes(world: &mut World, name: impl Into<String>, bytes: Vec<u8>) {
    world.resource_mut::<PendingScene>().0 = Some(Pending::File(SceneFile {
        name: name.into(),
        bytes,
        origin: None,
    }));
}

pub(crate) fn queue_import_bytes(
    world: &mut World,
    name: impl Into<String>,
    bytes: Vec<u8>,
    origin: Vec2,
) {
    world.resource_mut::<PendingScene>().0 = Some(Pending::File(SceneFile {
        name: name.into(),
        bytes,
        origin: Some(origin),
    }));
}

fn read_path(path: impl AsRef<Path>) -> Result<Vec<u8>, String> {
    #[cfg(target_arch = "wasm32")]
    return Err(format!(
        "cannot open {path}: filesystem paths are unavailable"
    ));

    #[cfg(not(target_arch = "wasm32"))]
    {
        std::fs::read(path.as_ref())
            .map_err(|error| format!("cannot open {}: {error}", path.as_ref().display()))
    }
}

pub(crate) fn queue_path(
    world: &mut World,
    path: impl AsRef<Path>,
    import: bool,
) -> Result<(), String> {
    let name = path
        .as_ref()
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "scene".to_string());
    let bytes = read_path(path)?;
    if import {
        let origin = {
            let mouse = world.resource::<crate::mouse_tracking::MousePosWorld>();
            Vec2::new(mouse.x, mouse.y)
        };
        queue_import_bytes(world, name, bytes, origin);
    } else {
        queue_bytes(world, name, bytes);
    }
    Ok(())
}

pub(crate) fn queue_new(world: &mut World) {
    world.resource_mut::<PendingScene>().0 = Some(Pending::New);
}

fn is_zip(bytes: &[u8]) -> bool {
    [b"PK\x03\x04", b"PK\x05\x06", b"PK\x07\x08"]
        .iter()
        .any(|magic| bytes.starts_with(*magic))
}

fn source_bytes(bytes: &[u8]) -> Result<Cow<'_, [u8]>, String> {
    if is_zip(bytes) {
        let mut archive =
            zip::ZipArchive::new(Cursor::new(bytes)).map_err(|error| error.to_string())?;
        let mut file = archive
            .by_name("scene.phn")
            .map_err(|_| "zip has no root scene.phn".to_owned())?;
        let mut source = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut source)
            .map_err(|error| error.to_string())?;
        Ok(Cow::Owned(source))
    } else {
        Ok(Cow::Borrowed(bytes))
    }
}

pub(super) fn read_source(path: &str) -> Result<String, String> {
    let bytes = read_path(path)?;
    let bytes = source_bytes(&bytes)?;
    Ok(read_auto_encoding(&bytes).into_owned())
}

fn parse_scene(bytes: Vec<u8>) -> Result<Spanned<Expr>, String> {
    let bytes = source_bytes(&bytes)?;
    let source = read_auto_encoding(&bytes);
    parse_thyme(&source).into_result().map_err(|errors| {
        errors
            .into_iter()
            .map(|error| error.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    })
}

pub(crate) fn clear(world: &mut World) {
    let state = world.resource::<SceneState>();
    let (scene, sky) = (state.scene, state.sky);
    let children = world
        .get::<Children>(scene)
        .map(|children| {
            children
                .iter()
                .copied()
                .filter(|&child| child != sky)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for child in children {
        world.despawn(child);
    }
}

pub(crate) fn reset(world: &mut World) {
    clear(world);
    let sky = world.resource::<SceneState>().sky;

    *world.get_mut::<Position>(sky).unwrap() = Position::default();
    *world.get_mut::<Rotation>(sky).unwrap() = Rotation::default();
    *world.get_mut::<Transform>(sky).unwrap() = Transform::default();
    let camera = world.resource::<MainCameraEntity>().0;
    *world.get_mut::<Transform>(camera).unwrap() = default_camera_transform();

    world.insert_resource(GridSettings::default());
    world.insert_resource(GravitySetting::default());
    world.insert_resource(AirSettings::default());
    world.insert_resource(DragConfig::default());
    world.insert_resource(GearSettings::default());
    world.insert_resource(SelectionConfig::default());
    world.insert_resource(DepthSorter::default());
    world.insert_resource(PendingEvents::default());
    world.insert_resource(PointerToolState::default());
    world.insert_resource(OverlayState::default());
    let mut physics = Time::<Physics>::default();
    physics.pause();
    world.insert_resource(physics);
    world.insert_non_send(ScriptEngine::default());
}

pub(crate) fn load_pending(world: &mut World) {
    let Some(pending) = world.resource_mut::<PendingScene>().0.take() else {
        return;
    };
    let file = match pending {
        Pending::New => {
            reset(world);
            crate::tools::add_object::spawn_default_plane(world, Vec2::ZERO);
            let camera = world.resource::<MainCameraEntity>().0;
            world.get_mut::<Transform>(camera).unwrap().translation.y = 2.0;
            return;
        }
        Pending::File(file) => file,
    };
    let program = match parse_scene(file.bytes) {
        Ok(program) => program,
        Err(error) => {
            world
                .resource_mut::<Console>()
                .push_line(format_args!("ERROR loading {}: {error}", file.name));
            return;
        }
    };

    if file.origin.is_none() {
        reset(world);
    }
    let errors = with_script_engine(world, |engine, world| {
        engine.eval_scene_statements(world, &program.0, file.origin.unwrap_or_default())
    });
    for error in errors {
        world
            .resource_mut::<Console>()
            .push_line(format_args!("ERROR loading {}: {error}", file.name));
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use ::thyme::parse::Expr;
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    use super::parse_scene;

    fn archive(name: &str, source: &str) -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file(
            name,
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
        )
        .unwrap();
        zip.write_all(source.as_bytes()).unwrap();
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn scene_source_is_detected_from_content() {
        let (plain, _) = parse_scene(b"a = 1".to_vec()).unwrap();
        let (bom, _) = parse_scene(b"\xef\xbb\xbfa = 1".to_vec()).unwrap();
        let (zipped, _) = parse_scene(archive("scene.phn", "a = 1")).unwrap();
        assert!(matches!(plain, Expr::Seq(_)));
        assert!(matches!(bom, Expr::Seq(_)));
        assert!(matches!(zipped, Expr::Seq(_)));

        assert!(parse_scene(archive("folder/scene.phn", "a = 1")).is_err());
        assert!(parse_scene(b"PK\x03\x04broken".to_vec()).is_err());
    }
}
