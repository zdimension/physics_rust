use avian2d::prelude::*;
use bevy::math::{Vec2, Vec3};
use bevy::prelude::*;

use crate::lyon_compat::{shapes, Fill, GeometryBuilder, ShapeBundle, Stroke};
use crate::mouse::select;
use crate::objects::{ColorComponent, SettingComponent, SpriteOnly};
use crate::palette::{PaletteConfig, ToRgba};
use crate::tools::add_object::{query_only_real, DepthSorter};
use crate::ui::images::AppIcons;
use crate::ui::{EntitySelection, SelectionState};
use crate::update_from::UpdateFrom;
use crate::{make_stroke, InvTransformPoint, BORDER_THICKNESS};

const DEFAULT_SPRING_CONSTANT_PER_KG: f32 = 100.0;
const DEFAULT_DAMPING: f32 = 0.2;
const SPRING_UNIT_SCREEN_PX: f32 = 32.0;
const MIN_SPRING_UNITS: usize = 1;
const SPRING_VIRTUAL_LAYER: u32 = 1 << 31;

#[derive(Component, Copy, Clone, Debug)]
pub struct SpringObject {
    pub end_a: SpringEnd,
    pub end_b: SpringEnd,
    pub target_length: f32,
    pub spring_constant: f32,
    pub damping: f32,
    pub unit_size: f32,
    pub unit_count: usize,
}

#[derive(Copy, Clone, Debug)]
pub enum SpringEnd {
    Body { entity: Entity, local_anchor: Vec2 },
    Sky { world_anchor: Vec2 },
}

#[derive(Component, Copy, Clone, Debug)]
pub struct SpringPreview;

#[derive(Component, Copy, Clone, Debug)]
pub struct SpringEndHandle {
    pub spring: Entity,
    pub end: SpringEndIndex,
}

#[derive(Component, Copy, Clone, Debug)]
struct SpringUnit {
    index: usize,
}

#[derive(Component, Copy, Clone, Debug, PartialEq, Eq)]
pub enum SpringEndIndex {
    A,
    B,
}

#[derive(Component, Copy, Clone, Debug)]
struct SpringEndpointVisual {
    end: SpringEndIndex,
}

#[derive(Copy, Clone, Debug)]
pub struct SpringPlacementState {
    pub preview: Entity,
    pub start: SpringEnd,
}

#[derive(Message, Copy, Clone, Debug)]
pub struct UpdateSpringPreviewEvent {
    pub preview: Entity,
    pub end_pos: Vec2,
}

#[derive(Message, Copy, Clone, Debug)]
pub struct FinishSpringEvent {
    pub state: SpringPlacementState,
    pub end_pos: Vec2,
}

pub fn add_systems(app: &mut App) {
    app.add_message::<UpdateSpringPreviewEvent>()
        .add_message::<FinishSpringEvent>()
        .add_systems(
            Update,
            (
                update_spring_previews,
                finish_springs.after(update_spring_previews),
                update_spring_visuals.after(finish_springs),
            ),
        )
        .add_systems(
            PhysicsSchedule,
            apply_spring_forces
                .in_set(PhysicsStepSystems::BroadPhase)
                .after(crate::tools::drag::apply_drag_force),
        );
}

impl SpringObject {
    pub fn placement(
        end_a: SpringEnd,
        end_b: SpringEnd,
        unit_size: f32,
        current_length: f32,
    ) -> Self {
        Self {
            end_a,
            end_b,
            target_length: current_length.max(0.01),
            spring_constant: 100.0, // will be erased anyway
            damping: DEFAULT_DAMPING,
            unit_size,
            unit_count: unit_count_for(current_length, unit_size),
        }
    }

    pub fn potential_energy_for_length(&self, length: f32) -> f32 {
        let stretch = length - self.target_length;
        0.5 * self.spring_constant.max(0.0) * stretch * stretch
    }

    pub fn potential_energy(&self, bodies: &Query<(&Position, &Rotation)>) -> Option<f32> {
        let (point_a, point_b) = self.world_points(bodies)?;
        Some(self.potential_energy_for_length(point_a.distance(point_b)))
    }
}

impl SpringEnd {
    pub fn sky(point: Vec2) -> Self {
        Self::Sky {
            world_anchor: point,
        }
    }

    pub fn from_body(entity: Entity, transform: &GlobalTransform, point: Vec2) -> Self {
        Self::Body {
            entity,
            local_anchor: transform.to_local(point),
        }
    }

    fn body(self) -> Option<Entity> {
        match self {
            Self::Body { entity, .. } => Some(entity),
            Self::Sky { .. } => None,
        }
    }
}

pub fn unit_size_for_camera(camera: &Transform) -> f32 {
    (camera.scale.x * SPRING_UNIT_SCREEN_PX).max(0.01)
}

pub fn pick_body_at(
    spatial_query: &SpatialQuery,
    bodies: &Query<(&GlobalTransform, Option<&RigidBody>)>,
    point: Vec2,
) -> Option<Entity> {
    select::find_under_mouse(spatial_query, point, query_only_real(), |ent| {
        bodies
            .get(ent)
            .map(|(transform, _)| transform.translation_vec3a().z)
            .unwrap_or(f32::NEG_INFINITY)
    })
    .find(|ent| bodies.get(*ent).is_ok_and(|(_, body)| body.is_some()))
}

pub fn spawn_spring(
    commands: &mut Commands,
    scene: Entity,
    images: &AppIcons,
    color: ColorComponent,
    end_a: SpringEnd,
    end_b: SpringEnd,
    unit_size: f32,
    z: &mut DepthSorter,
    preview: bool,
) -> Entity {
    let current_length = end_a
        .preview_world_pos()
        .distance(end_b.preview_world_pos());
    let spring = SpringObject::placement(end_a, end_b, unit_size, current_length);
    let endpoint_scale = Vec3::splat(endpoint_diameter(unit_size));
    let spring_z = z.next();
    let endpoint_a_z = z.next();
    let endpoint_b_z = z.next();

    let mut entity = commands.spawn((
        spring,
        color.update_from_this(),
        Transform::from_translation(Vec3::new(0.0, 0.0, spring_z)),
        Visibility::Inherited,
    ));
    if preview {
        entity.insert(SpringPreview);
    }
    let spring_entity = entity.id();
    entity.insert(ChildOf(scene)).with_children(|builder| {
        for end in [SpringEndIndex::A, SpringEndIndex::B] {
            builder.spawn((
                SpringEndpointVisual { end },
                SpringEndHandle {
                    spring: spring_entity,
                    end,
                },
                ShapeBundle::new(
                    GeometryBuilder::build_as(&shapes::Circle {
                        radius: 0.5,
                        ..Default::default()
                    }),
                    Transform::from_translation(Vec3::Z * endpoint_local_z(end, spring_z, endpoint_a_z, endpoint_b_z))
                        .with_scale(endpoint_scale),
                    Visibility::Inherited,
                ),
                crate::make_fill(Color::WHITE),
                make_stroke(Color::BLACK, BORDER_THICKNESS),
                SpriteOnly,
                Collider::circle(0.5),
                Sensor,
                non_interacting_virtual_layers(),
            ));
        }

        for index in 0..spring.unit_count {
            spawn_unit(builder, images, spring_entity, index, unit_size);
        }
    });

    spring_entity
}

fn spawn_unit(
    builder: &mut ChildSpawnerCommands,
    images: &AppIcons,
    spring: Entity,
    index: usize,
    unit_size: f32,
) {
    builder.spawn((
        SpringUnit { index },
        Sprite {
            image: images.spring.clone(),
            custom_size: Some(Vec2::splat(unit_size)),
            ..Default::default()
        },
        Transform::default(),
        UpdateFrom::<ColorComponent>::entity(spring),
    ));
}

pub fn find_spring_under_point(
    point: Vec2,
    springs: &Query<(Entity, &SpringObject, &Transform)>,
    bodies: &Query<(&Position, &Rotation)>,
) -> Option<(Entity, f32)> {
    let mut best_hit = None;

    for (spring_entity, spring, transform) in springs {
        let Some((point_a, point_b)) = spring.world_points(bodies) else {
            continue;
        };

        let z = transform.translation.z;
        let spring_radius = spring_thickness(spring.unit_size) * 0.5;
        if point_to_segment_distance(point, point_a, point_b) <= spring_radius {
            best_hit = higher_hit(best_hit, (spring_entity, z));
        }

    }

    best_hit
}

fn higher_hit(current: Option<(Entity, f32)>, candidate: (Entity, f32)) -> Option<(Entity, f32)> {
    match current {
        Some(current) if current.1 > candidate.1 => Some(current),
        _ => Some(candidate),
    }
}

fn point_to_segment_distance(point: Vec2, start: Vec2, end: Vec2) -> f32 {
    let segment = end - start;
    let length_squared = segment.length_squared();
    if length_squared <= f32::EPSILON {
        return point.distance(start);
    }

    let t = ((point - start).dot(segment) / length_squared).clamp(0.0, 1.0);
    point.distance(start + segment * t)
}

fn update_spring_previews(
    mut events: MessageReader<UpdateSpringPreviewEvent>,
    mut springs: Query<&mut SpringObject, With<SpringPreview>>,
    bodies: Query<(&Position, &Rotation)>,
) {
    for event in events.read() {
        let Ok(mut spring) = springs.get_mut(event.preview) else {
            continue;
        };
        spring.end_b = SpringEnd::sky(event.end_pos);
        let Some((a, b)) = spring.world_points(&bodies) else {
            continue;
        };
        let length = a.distance(b);
        spring.target_length = length.max(0.01);
        spring.unit_count = unit_count_for(length, spring.unit_size);
    }
}

fn finish_springs(
    mut events: MessageReader<FinishSpringEvent>,
    spatial_query: SpatialQuery,
    bodies: Query<(&GlobalTransform, Option<&RigidBody>)>,
    body_positions: Query<(&Position, &Rotation)>,
    body_masses: Query<&ColliderMassProperties>,
    mut springs: Query<&mut SpringObject, With<SpringPreview>>,
    mut commands: Commands,
) {
    for event in events.read() {
        let Ok(mut spring) = springs.get_mut(event.state.preview) else {
            continue;
        };
        let end_body = pick_body_at(&spatial_query, &bodies, event.end_pos);
        let start_body = event.state.start.body();
        let end_b = match (start_body, end_body) {
            (None, None) => {
                commands.entity(event.state.preview).despawn();
                continue;
            }
            (Some(start), Some(end)) if start == end => SpringEnd::sky(event.end_pos),
            (_, Some(end)) => {
                let Ok((transform, _)) = bodies.get(end) else {
                    commands.entity(event.state.preview).despawn();
                    continue;
                };
                SpringEnd::from_body(end, transform, event.end_pos)
            }
            (_, None) => SpringEnd::sky(event.end_pos),
        };

        spring.end_a = event.state.start;
        spring.end_b = end_b;
        let Some((a, b)) = spring.world_points(&body_positions) else {
            commands.entity(event.state.preview).despawn();
            continue;
        };
        let length = a.distance(b);
        spring.target_length = length.max(0.01);
        spring.spring_constant =
            default_spring_constant_for_ends(spring.end_a, spring.end_b, &body_masses)
                .expect("should not happen");
        spring.unit_count = unit_count_for(length, spring.unit_size);
        commands
            .entity(event.state.preview)
            .remove::<SpringPreview>();
    }
}

fn update_spring_visuals(
    mut commands: Commands,
    images: Res<AppIcons>,
    palette: Res<PaletteConfig>,
    selection_state: Res<SelectionState>,
    body_transforms: Query<(&Position, &Rotation)>,
    color_sources: Query<(Option<&ChildOf>, Option<Ref<ColorComponent>>)>,
    mut springs: Query<
        (Entity, &SpringObject, &mut Transform),
        (Without<SpringUnit>, Without<SpringEndpointVisual>),
    >,
    unit_count_query: Query<(Entity, &SpringUnit, &ChildOf)>,
    mut units: Query<
        (&SpringUnit, &ChildOf, &mut Transform, &mut Sprite),
        (Without<SpringObject>, Without<SpringEndpointVisual>),
    >,
    mut endpoints: Query<
        (
            Entity,
            &SpringEndpointVisual,
            &ChildOf,
            &mut Transform,
            &mut Fill,
            &mut Stroke,
        ),
        (Without<SpringObject>, Without<SpringUnit>),
    >,
) {
    for (spring_entity, spring, mut transform) in &mut springs {
        let Some((point_a, point_b)) = spring.world_points(&body_transforms) else {
            commands.entity(spring_entity).despawn();
            continue;
        };
        let delta = point_b - point_a;
        let length = delta.length();
        let thickness = spring_thickness(spring.unit_size);
        let safe_length = length.max(thickness);
        let angle = if length > f32::EPSILON {
            delta.y.atan2(delta.x)
        } else {
            transform.rotation.to_euler(EulerRot::XYZ).2
        };

        transform.translation = ((point_a + point_b) * 0.5).extend(transform.translation.z);
        transform.rotation = Quat::from_rotation_z(angle);

        let existing = unit_count_query
            .iter()
            .filter(|(_, _, parent)| parent.parent() == spring_entity)
            .collect::<Vec<_>>();
        if existing.len() < spring.unit_count {
            commands.entity(spring_entity).with_children(|builder| {
                for index in existing.len()..spring.unit_count {
                    spawn_unit(builder, &images, spring_entity, index, spring.unit_size);
                }
            });
        } else if existing.len() > spring.unit_count {
            for (entity, unit, _) in existing {
                if unit.index >= spring.unit_count {
                    commands.entity(entity).despawn();
                }
            }
        }

        let unit_len = safe_length / spring.unit_count.max(1) as f32;
        for (unit, parent, mut unit_transform, mut sprite) in &mut units {
            if parent.parent() != spring_entity {
                continue;
            }
            unit_transform.translation = Vec3::new(
                -safe_length * 0.5 + unit_len * (unit.index as f32 + 0.5),
                0.0,
                0.01,
            );
            unit_transform.rotation = Quat::IDENTITY;
            unit_transform.scale = Vec3::ONE;
            sprite.custom_size = Some(Vec2::new(unit_len, spring.unit_size));
        }

        for (endpoint_entity, endpoint, parent, mut endpoint_transform, mut fill, mut stroke) in
            &mut endpoints
        {
            if parent.parent() != spring_entity {
                continue;
            }
            let local_x = match endpoint.end {
                SpringEndIndex::A => -safe_length * 0.5,
                SpringEndIndex::B => safe_length * 0.5,
            };
            endpoint_transform.translation =
                Vec3::new(local_x, 0.0, endpoint_transform.translation.z);
            endpoint_transform.scale = Vec3::splat(endpoint_diameter(spring.unit_size));
            fill.color = endpoint_color(
                match endpoint.end {
                    SpringEndIndex::A => spring.end_a,
                    SpringEndIndex::B => spring.end_b,
                },
                endpoint_entity,
                &color_sources,
                &palette,
            );
            stroke.color = if selection_state.selected_entity
                == Some(EntitySelection {
                    entity: endpoint_entity,
                })
                || selection_state.selected_entity
                    == Some(EntitySelection {
                        entity: spring_entity,
                    }) {
                Color::WHITE
            } else {
                Color::BLACK
            };
        }
    }
}

fn apply_spring_forces(
    springs: Query<&SpringObject, Without<SpringPreview>>,
    mut bodies: ParamSet<(
        Query<RigidBodyQueryReadOnly, Without<RigidBodyDisabled>>,
        Query<Forces, Without<RigidBodyDisabled>>,
    )>,
) {
    let mut applications = Vec::new();
    {
        let body_query = bodies.p0();
        for spring in &springs {
            let Some(a) = SpringBodyPoint::from_end(spring.end_a, &body_query) else {
                continue;
            };
            let Some(b) = SpringBodyPoint::from_end(spring.end_b, &body_query) else {
                continue;
            };
            let delta = b.point - a.point;
            let distance = delta.length();
            if distance <= f32::EPSILON {
                continue;
            }
            let direction = delta / distance;
            let stretch = distance - spring.target_length;
            let relative_velocity = (b.velocity - a.velocity).dot(direction);
            let damping_coefficient =
                spring.damping.max(0.0) * critical_damping(spring.spring_constant, a.mass, b.mass);
            let magnitude =
                spring.spring_constant.max(0.0) * stretch + damping_coefficient * relative_velocity;
            let force_on_a = direction * magnitude;
            if let Some(entity) = a.entity {
                applications.push((entity, force_on_a, a.point));
            }
            if let Some(entity) = b.entity {
                applications.push((entity, -force_on_a, b.point));
            }
        }
    }

    let mut force_query = bodies.p1();
    for (entity, force, point) in applications {
        if let Ok(mut forces) = force_query.get_mut(entity) {
            forces.apply_force_at_point(force, point);
        }
    }
}

struct SpringBodyPoint {
    entity: Option<Entity>,
    point: Vec2,
    velocity: Vec2,
    mass: f32,
}

impl SpringBodyPoint {
    fn from_end(
        end: SpringEnd,
        bodies: &Query<RigidBodyQueryReadOnly, Without<RigidBodyDisabled>>,
    ) -> Option<Self> {
        match end {
            SpringEnd::Sky { world_anchor } => Some(Self {
                entity: None,
                point: world_anchor,
                velocity: Vec2::ZERO,
                mass: f32::INFINITY,
            }),
            SpringEnd::Body {
                entity,
                local_anchor,
            } => {
                let body = bodies.get(entity).ok()?;
                let point = body.position.0 + body.rotation * local_anchor;
                let center_of_mass = body.rotation * body.center_of_mass.0;
                Some(Self {
                    entity: Some(entity),
                    point,
                    velocity: body.velocity_at_point(point - (body.position.0 + center_of_mass)),
                    mass: body.mass().value(),
                })
            }
        }
    }
}

impl SpringObject {
    fn world_points(&self, bodies: &Query<(&Position, &Rotation)>) -> Option<(Vec2, Vec2)> {
        Some((self.end_a.world_pos(bodies)?, self.end_b.world_pos(bodies)?))
    }
}

impl SpringEnd {
    fn world_pos(self, bodies: &Query<(&Position, &Rotation)>) -> Option<Vec2> {
        match self {
            Self::Body {
                entity,
                local_anchor,
            } => {
                let (position, rotation) = bodies.get(entity).ok()?;
                Some(position.0 + *rotation * local_anchor)
            }
            Self::Sky { world_anchor } => Some(world_anchor),
        }
    }

    fn preview_world_pos(self) -> Vec2 {
        match self {
            Self::Body { local_anchor, .. } => local_anchor,
            Self::Sky { world_anchor } => world_anchor,
        }
    }
}

fn endpoint_color(
    end: SpringEnd,
    endpoint_entity: Entity,
    color_sources: &Query<(Option<&ChildOf>, Option<Ref<ColorComponent>>)>,
    palette: &PaletteConfig,
) -> Color {
    match end {
        SpringEnd::Body { entity, .. } => UpdateFrom::<ColorComponent>::entity(entity)
            .find_component(endpoint_entity, color_sources)
            .map(|(_, color)| color.to_rgba())
            .unwrap_or(palette.current_palette.sky_color),
        SpringEnd::Sky { .. } => palette.current_palette.sky_color,
    }
}

fn unit_count_for(length: f32, unit_size: f32) -> usize {
    ((length / unit_size).round() as usize).max(MIN_SPRING_UNITS)
}

fn spring_thickness(unit_size: f32) -> f32 {
    unit_size
}

fn endpoint_diameter(unit_size: f32) -> f32 {
    unit_size * 0.7
}

fn endpoint_local_z(
    end: SpringEndIndex,
    spring_z: f32,
    endpoint_a_z: f32,
    endpoint_b_z: f32,
) -> f32 {
    match end {
        SpringEndIndex::A => endpoint_a_z - spring_z,
        SpringEndIndex::B => endpoint_b_z - spring_z,
    }
}

fn critical_damping(k: f32, mass_a: f32, mass_b: f32) -> f32 {
    if k <= 0.0 {
        return 0.0;
    }
    let effective_mass = if mass_a.is_infinite() {
        mass_b
    } else if mass_b.is_infinite() {
        mass_a
    } else {
        mass_a * mass_b / (mass_a + mass_b)
    };
    if !effective_mass.is_finite() || effective_mass <= 0.0 {
        0.0
    } else {
        2.0 * (k * effective_mass).sqrt()
    }
}

fn default_spring_constant_for_ends(
    end_a: SpringEnd,
    end_b: SpringEnd,
    masses: &Query<&ColliderMassProperties>,
) -> Option<f32> {
    let mass_a = end_mass(end_a, masses);
    let mass_b = end_mass(end_b, masses);
    match (mass_a, mass_b) {
        (Some(a), Some(b)) => {
            // todo: this is just effective mass
            Some(DEFAULT_SPRING_CONSTANT_PER_KG * a * b / (a + b))
        },
        (Some(x), None) | (None, Some(x)) => Some(DEFAULT_SPRING_CONSTANT_PER_KG * x),
        _ => None,
    }
}

fn end_mass(end: SpringEnd, masses: &Query<&ColliderMassProperties>) -> Option<f32> {
    if let SpringEnd::Body { entity, .. } = end {
        masses.get(entity).ok().map(|mass| mass.mass)
    } else {
        None
    }
}

fn non_interacting_virtual_layers() -> CollisionLayers {
    CollisionLayers::from_bits(SPRING_VIRTUAL_LAYER, 0)
}
