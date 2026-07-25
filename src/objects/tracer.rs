use std::collections::HashMap;

use avian2d::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::sprite_render::AlphaMode2d;

use crate::objects::{ColorComponent, SizeComponent};
use crate::ui::SceneState;
use crate::{hsva_to_rgba, systems};

const DEFAULT_FADE_TIME: f32 = 1.5;
const MIN_SAMPLE_DISTANCE: f32 = 1.0e-4;
const SAMPLE_DISTANCE_DIAMETER_FACTOR: f32 = 0.1;
const TRAIL_Z_OFFSET: f32 = -0.1;

systems!(update_tracer_trails);

#[derive(Component)]
pub struct TracerObject {
    pub fade_time: f32,
    samples: Vec<TracerSample>,
}

impl Default for TracerObject {
    fn default() -> Self {
        Self {
            fade_time: DEFAULT_FADE_TIME,
            samples: Vec::new(),
        }
    }
}

#[derive(Clone, Copy)]
struct TracerSample {
    pos: Vec2,
    age: f32,
}

#[derive(Component)]
struct TracerTrailRoot {
    tracer: Entity,
    mesh: Handle<Mesh>,
    material: Handle<ColorMaterial>,
}

fn update_tracer_trails(
    mut commands: Commands,
    physics: Res<Time<Physics>>,
    scene_state: Res<SceneState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut tracers: Query<(
        Entity,
        &GlobalTransform,
        &mut TracerObject,
        &ColorComponent,
        &SizeComponent,
    )>,
    roots: Query<(Entity, &TracerTrailRoot)>,
) {
    let mut roots_by_tracer = HashMap::new();
    for (root_entity, root) in &roots {
        if tracers.contains(root.tracer) {
            roots_by_tracer.insert(root.tracer, (root_entity, root.mesh.clone()));
        } else {
            meshes.remove(root.mesh.id());
            materials.remove(root.material.id());
            commands.entity(root_entity).despawn();
        }
    }

    let is_running = !physics.is_paused();
    let dt = physics.delta_secs();

    for (entity, transform, mut tracer, color, size) in &mut tracers {
        let (root, mesh) = roots_by_tracer.remove(&entity).map_or_else(
            || spawn_trail_root(&mut commands, &mut meshes, &mut materials, scene_state.scene, entity),
            |(root, mesh)| (root, mesh),
        );

        if is_running {
            for sample in &mut tracer.samples {
                sample.age += dt;
            }
        }

        let fade_time = tracer.fade_time.max(f32::EPSILON);
        tracer.samples.retain(|sample| sample.age <= fade_time);

        let pos = transform.translation().truncate();
        let min_sample_distance = (size.0 * SAMPLE_DISTANCE_DIAMETER_FACTOR).max(MIN_SAMPLE_DISTANCE);
        if is_running
            && tracer
                .samples
                .last()
                .is_none_or(|sample| sample.pos.distance(pos) >= min_sample_distance)
        {
            tracer.samples.push(TracerSample { pos, age: 0.0 });
        }

        commands.entity(root).insert(Transform::from_translation(
            Vec3::Z * (transform.translation().z + TRAIL_Z_OFFSET),
        ));

        if let Some(mut mesh) = meshes.get_mut(&mesh) {
            *mesh = build_trail_mesh(&tracer.samples, size.0, color, fade_time);
        }
    }
}

fn spawn_trail_root(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    scene: Entity,
    tracer: Entity,
) -> (Entity, Handle<Mesh>) {
    let mesh = meshes.add(empty_trail_mesh());
    let material = materials.add(ColorMaterial {
        color: Color::WHITE,
        alpha_mode: AlphaMode2d::Blend,
        ..Default::default()
    });
    let root = commands
        .spawn((
            TracerTrailRoot {
                tracer,
                mesh: mesh.clone(),
                material: material.clone(),
            },
            Mesh2d(mesh.clone()),
            MeshMaterial2d(material),
            Transform::default(),
            Visibility::Visible,
            ChildOf(scene),
        ))
        .id();

    (root, mesh)
}

fn build_trail_mesh(
    samples: &[TracerSample],
    diameter: f32,
    color: &ColorComponent,
    fade_time: f32,
) -> Mesh {
    if samples.len() < 2 {
        return empty_trail_mesh();
    }

    let mut positions = Vec::with_capacity(samples.len() * 2);
    let mut uvs = Vec::with_capacity(samples.len() * 2);
    let mut colors = Vec::with_capacity(samples.len() * 2);
    let half_width = diameter * 0.5;

    for index in 0..samples.len() {
        let sample = samples[index];
        let Some(normal) = sample_normal(samples, index) else {
            continue;
        };
        let alpha = (1.0 - sample.age / fade_time).clamp(0.0, 1.0);
        let vertex_color = hsva_to_rgba(bevy_egui::egui::ecolor::Hsva {
            a: color.0.a * alpha,
            ..color.0
        })
        .to_linear()
        .to_f32_array();
        let offset = normal * half_width;

        positions.push((sample.pos + offset).extend(0.0).to_array());
        positions.push((sample.pos - offset).extend(0.0).to_array());
        uvs.push([0.0, index as f32]);
        uvs.push([1.0, index as f32]);
        colors.push(vertex_color);
        colors.push(vertex_color);
    }

    if positions.len() < 4 {
        return empty_trail_mesh();
    }

    let mut indices = Vec::with_capacity((positions.len() / 2 - 1) * 6);
    for index in 0..(positions.len() / 2 - 1) as u32 {
        let left0 = index * 2;
        let right0 = left0 + 1;
        let left1 = left0 + 2;
        let right1 = left0 + 3;
        indices.extend_from_slice(&[left0, right0, left1, right0, right1, left1]);
    }

    let mut mesh = empty_trail_mesh();
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn empty_trail_mesh() -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<[f32; 3]>::new());
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, Vec::<[f32; 2]>::new());
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, Vec::<[f32; 4]>::new());
    mesh.insert_indices(Indices::U32(Vec::new()));
    mesh
}

fn sample_normal(samples: &[TracerSample], index: usize) -> Option<Vec2> {
    let prev_dir = (index > 0)
        .then(|| samples[index].pos - samples[index - 1].pos)
        .and_then(|delta| delta.try_normalize());
    let next_dir = (index + 1 < samples.len())
        .then(|| samples[index + 1].pos - samples[index].pos)
        .and_then(|delta| delta.try_normalize());

    let tangent = match (prev_dir, next_dir) {
        (Some(prev), Some(next)) => (prev + next).try_normalize().unwrap_or(next),
        (Some(prev), None) => prev,
        (None, Some(next)) => next,
        (None, None) => return None,
    };
    Some(tangent.perp())
}
