use std::collections::HashMap;

use avian2d::prelude::*;
use bevy::math::{Vec2, Vec3, Vec3Swizzles};
use bevy::prelude::*;

use crate::lyon_compat::{GeometryBuilder, ShapeBundle, shapes};
use crate::objects::{ColorComponent, SizeComponent};
use crate::ui::SceneState;
use crate::{make_fill, systems};

const DEFAULT_FADE_TIME: f32 = 1.5;
const MIN_SAMPLE_DISTANCE: f32 = 1.0e-4;

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
}

fn update_tracer_trails(
    mut commands: Commands,
    physics: Res<Time<Physics>>,
    scene_state: Res<SceneState>,
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
            commands.entity(root_entity).despawn_children();
            roots_by_tracer.insert(root.tracer, root_entity);
        } else {
            commands.entity(root_entity).despawn();
        }
    }

    let is_running = !physics.is_paused();
    let dt = physics.delta_secs();

    for (entity, transform, mut tracer, color, size) in &mut tracers {
        let root = roots_by_tracer.remove(&entity).unwrap_or_else(|| {
            commands
                .spawn((
                    TracerTrailRoot { tracer: entity },
                    Transform::default(),
                    Visibility::Visible,
                    ViewVisibility::default(),
                    ChildOf(scene_state.scene),
                ))
                .id()
        });

        if is_running {
            for sample in &mut tracer.samples {
                sample.age += dt;
            }
        }

        let fade_time = tracer.fade_time.max(f32::EPSILON);
        tracer.samples.retain(|sample| sample.age <= fade_time);

        let pos = transform.translation_vec3a().xy();
        if is_running
            && tracer
                .samples
                .last()
                .is_none_or(|sample| sample.pos.distance(pos) >= MIN_SAMPLE_DISTANCE)
        {
            tracer.samples.push(TracerSample { pos, age: 0.0 });
        }

        let z = transform.translation_vec3a().z - 0.1;
        for segment in tracer.samples.windows(2) {
            let [start, end] = segment else {
                continue;
            };
            let Some(poly) = trail_segment(start.pos, end.pos, size.0) else {
                continue;
            };
            let age = start.age.max(end.age);
            let alpha = (1.0 - age / fade_time).clamp(0.0, 1.0);
            commands.spawn((
                ShapeBundle::new(
                    GeometryBuilder::build_as(&poly),
                    Transform::from_translation(Vec3::new(0.0, 0.0, z)),
                    Visibility::Inherited,
                ),
                make_fill(crate::hsva_to_rgba(bevy_egui::egui::ecolor::Hsva {
                    a: color.0.a * alpha,
                    ..color.0
                })),
                ChildOf(root),
            ));
        }
    }
}

fn trail_segment(start: Vec2, end: Vec2, diameter: f32) -> Option<shapes::Polygon> {
    let delta = end - start;
    if delta.length_squared() <= f32::EPSILON {
        return None;
    }

    let dir = delta.normalize();
    let normal = dir.perp() * diameter * 0.5;
    Some(shapes::Polygon {
        points: vec![start + normal, start - normal, end - normal, end + normal],
        closed: true,
    })
}
