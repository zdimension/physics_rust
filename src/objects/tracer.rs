use std::collections::HashMap;

use avian2d::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::sprite_render::AlphaMode2d;

use crate::lyon_compat::{GeometryBuilder, Shape, shapes};
use crate::objects::ColorComponent;
use crate::ui::SceneState;
use crate::{hsva_to_rgba, systems};

const DEFAULT_FADE_TIME: f32 = 1.5;
const MIN_SAMPLE_DISTANCE: f32 = 1.0e-4;
const SAMPLE_DISTANCE_DIAMETER_FACTOR: f32 = 0.1;
const TRAIL_Z_OFFSET: f32 = -0.1;

systems!(update_tracer_trails, sync_tracer_size);

#[derive(Component, Copy, Clone, Debug)]
pub struct TracerSettings {
    pub(crate) diameter: f32,
    pub(crate) fade_time: f32,
}

impl Default for TracerSettings {
    fn default() -> Self {
        Self {
            diameter: 1.0,
            fade_time: DEFAULT_FADE_TIME,
        }
    }
}

#[derive(Component)]
pub struct TracerObject {
    samples: Vec<TracerSample>,
    dirty: bool,
}

impl Default for TracerObject {
    fn default() -> Self {
        Self {
            samples: Vec::new(),
            dirty: true,
        }
    }
}

impl TracerObject {
    pub fn clear_trail(&mut self) {
        self.samples.clear();
        self.dirty = true;
    }
}

#[derive(Component)]
pub(crate) struct TracerVisual;

#[derive(Clone, Copy)]
struct TracerSample {
    pos: Vec2,
    age: f32,
}

#[derive(Component)]
struct TracerTrailRoot {
    tracer: Entity,
    mesh: Handle<Mesh>,
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
        &TracerSettings,
        &ColorComponent,
    )>,
    roots: Query<(Entity, &TracerTrailRoot)>,
    changed_settings: Query<(), Changed<TracerSettings>>,
    changed_colors: Query<(), (With<TracerObject>, Changed<ColorComponent>)>,
    changed_transforms: Query<(), (With<TracerObject>, Changed<GlobalTransform>)>,
    mut removed_tracers: RemovedComponents<TracerObject>,
) {
    let tracer_removed = removed_tracers.read().next().is_some();
    let is_running = !physics.is_paused();
    let has_dirty_trail = tracers.iter().any(|(_, _, tracer, _, _)| tracer.dirty);
    if !is_running
        && !has_dirty_trail
        && changed_settings.is_empty()
        && changed_colors.is_empty()
        && changed_transforms.is_empty()
        && !tracer_removed
    {
        return;
    }

    let mut roots_by_tracer = HashMap::new();
    for (root_entity, root) in &roots {
        if tracers.contains(root.tracer) {
            roots_by_tracer.insert(root.tracer, (root_entity, root.mesh.clone()));
        } else {
            // Let handle drops release assets after the entity is gone.
            // Manually removing here can race with render-world extraction.
            commands.entity(root_entity).despawn();
        }
    }

    let dt = physics.delta_secs();

    for (entity, transform, mut tracer, settings, color) in &mut tracers {
        let (root, mesh) = roots_by_tracer.remove(&entity).map_or_else(
            || {
                spawn_trail_root(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    scene_state.scene,
                    entity,
                )
            },
            |(root, mesh)| (root, mesh),
        );

        if is_running {
            for sample in &mut tracer.samples {
                sample.age += dt;
            }
        }

        let fade_time = settings.fade_time.max(f32::EPSILON);
        tracer.samples.retain(|sample| sample.age <= fade_time);

        let pos = transform.translation().truncate();
        let min_sample_distance =
            (settings.diameter * SAMPLE_DISTANCE_DIAMETER_FACTOR).max(MIN_SAMPLE_DISTANCE);
        if is_running
            && tracer
                .samples
                .last()
                .is_none_or(|sample| sample.pos.distance(pos) >= min_sample_distance)
        {
            tracer.samples.push(TracerSample { pos, age: 0.0 });
        }

        let is_drawable = meshes.get_mut(&mesh).is_some_and(|mut mesh| {
            update_trail_mesh(
                &mut mesh,
                &tracer.samples,
                settings.diameter,
                color,
                fade_time,
            )
        });

        commands.entity(root).insert((
            Transform::from_translation(Vec3::Z * (transform.translation().z + TRAIL_Z_OFFSET)),
            if is_drawable {
                Visibility::Visible
            } else {
                Visibility::Hidden
            },
        ));
        tracer.dirty = false;
    }
}

fn sync_tracer_size(
    mut tracers: Query<
        (&TracerSettings, &Children, &mut Collider, &mut Shape),
        Changed<TracerSettings>,
    >,
    mut visuals: Query<&mut Sprite, With<TracerVisual>>,
) {
    for (settings, children, mut collider, mut shape) in &mut tracers {
        *collider = Collider::circle(settings.diameter * 0.5);
        shape.path = GeometryBuilder::build_as(&shapes::Circle {
            radius: settings.diameter * 0.55,
            ..Default::default()
        });

        for child in children.iter() {
            if let Ok(mut sprite) = visuals.get_mut(child) {
                sprite.custom_size = Some(Vec2::splat(settings.diameter));
            }
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
    let mesh = meshes.add(placeholder_trail_mesh());
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
            },
            Mesh2d(mesh.clone()),
            MeshMaterial2d(material),
            Transform::default(),
            Visibility::Hidden,
            ChildOf(scene),
        ))
        .id();

    (root, mesh)
}

fn update_trail_mesh(
    mesh: &mut Mesh,
    samples: &[TracerSample],
    diameter: f32,
    color: &ColorComponent,
    fade_time: f32,
) -> bool {
    let Some((positions, uvs, colors, indices)) =
        trail_mesh_data(samples, diameter, color, fade_time)
    else {
        return false;
    };

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    true
}

fn trail_mesh_data(
    samples: &[TracerSample],
    diameter: f32,
    color: &ColorComponent,
    fade_time: f32,
) -> Option<(Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<[f32; 4]>, Vec<u32>)> {
    if samples.len() < 2 {
        return None;
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
        return None;
    }

    let mut indices = Vec::with_capacity((positions.len() / 2 - 1) * 6);
    for index in 0..(positions.len() / 2 - 1) as u32 {
        let left0 = index * 2;
        let right0 = left0 + 1;
        let left1 = left0 + 2;
        let right1 = left0 + 3;
        indices.extend_from_slice(&[left0, right0, left1, right0, right1, left1]);
    }

    Some((positions, uvs, colors, indices))
}

fn placeholder_trail_mesh() -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0, 0.0, 0.0]; 4]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; 4]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0, 0.0, 0.0, 0.0]; 4]);
    mesh.insert_indices(Indices::U32(vec![0, 1, 2, 1, 3, 2]));
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
