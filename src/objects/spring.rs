use super::{body::world_point, phy_obj::PhysicalGeometry};
use avian2d::prelude::*;
use bevy::math::{Vec2, Vec3};
use bevy::prelude::*;

use crate::InvTransformPoint;
use crate::mouse::select;
use crate::objects::{ColorComponent, SettingComponent, SpriteOnly};
use crate::palette::{PaletteConfig, ToRgba};
use crate::tools::add_object::DepthSorter;
use crate::ui::images::AppIcons;
use crate::update_from::UpdateFrom;

const DEFAULT_SPRING_CONSTANT_PER_KG: f32 = 100.0;
const DEFAULT_DAMPING: f32 = 0.2;
const SPRING_UNIT_SCREEN_PX: f32 = 46.0;
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
    colliders: &Query<(Entity, &Collider, &GlobalTransform), Without<ColliderDisabled>>,
    bodies: &Query<&GlobalTransform, With<PhysicalGeometry>>,
    point: Vec2,
) -> Option<Entity> {
    select::colliders_under_point(point, colliders).find(|ent| bodies.contains(*ent))
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
        Collider::rectangle(current_length.max(unit_size), spring_thickness(unit_size)),
        Sensor,
        non_interacting_virtual_layers(),
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
                Sprite {
                    image: images.spring_attachment.clone(),
                    custom_size: Some(Vec2::ONE),
                    ..Default::default()
                },
                Transform::from_translation(
                    Vec3::Z * endpoint_local_z(end, spring_z, endpoint_a_z, endpoint_b_z),
                )
                .with_scale(endpoint_scale),
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
    colliders: Query<(Entity, &Collider, &GlobalTransform), Without<ColliderDisabled>>,
    bodies: Query<&GlobalTransform, With<PhysicalGeometry>>,
    body_positions: Query<(&Position, &Rotation)>,
    body_masses: Query<&ColliderMassProperties>,
    mut springs: Query<&mut SpringObject, With<SpringPreview>>,
    mut commands: Commands,
) {
    for event in events.read() {
        let Ok(mut spring) = springs.get_mut(event.state.preview) else {
            continue;
        };
        let end_body = pick_body_at(&colliders, &bodies, event.end_pos);
        let start_body = event.state.start.body();
        let end_b = match (start_body, end_body) {
            (None, None) => {
                commands.entity(event.state.preview).despawn();
                continue;
            }
            (Some(start), Some(end)) if start == end => SpringEnd::sky(event.end_pos),
            (_, Some(end)) => {
                let Ok(transform) = bodies.get(end) else {
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
    body_transforms: Query<(&Position, &Rotation)>,
    color_sources: Query<(Option<&ChildOf>, Option<Ref<ColorComponent>>)>,
    mut springs: Query<
        (
            Entity,
            &SpringObject,
            &Children,
            &mut Transform,
            &mut Collider,
        ),
        (Without<SpringUnit>, Without<SpringEndpointVisual>),
    >,
    mut units: Query<
        (Entity, &SpringUnit, &mut Transform, &mut Sprite),
        (Without<SpringObject>, Without<SpringEndpointVisual>),
    >,
    mut endpoints: Query<
        (Entity, &SpringEndpointVisual, &mut Transform, &mut Sprite),
        (Without<SpringObject>, Without<SpringUnit>),
    >,
    changed_springs: Query<(), Changed<SpringObject>>,
    changed_bodies: Query<(), Or<(Changed<Position>, Changed<Rotation>)>>,
    changed_colors: Query<(), Changed<ColorComponent>>,
    added_units: Query<(), Added<SpringUnit>>,
    added_endpoints: Query<(), Added<SpringEndpointVisual>>,
) {
    if changed_springs.is_empty()
        && changed_bodies.is_empty()
        && changed_colors.is_empty()
        && added_units.is_empty()
        && added_endpoints.is_empty()
        && !palette.is_changed()
    {
        return;
    }

    for (spring_entity, spring, children, mut transform, mut collider) in &mut springs {
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
        *collider = Collider::rectangle(safe_length, thickness);

        let unit_len = safe_length / spring.unit_count.max(1) as f32;
        let mut unit_count = 0;
        for child in children.iter() {
            if let Ok((unit_entity, unit, mut unit_transform, mut sprite)) = units.get_mut(child) {
                unit_count += 1;
                if unit.index >= spring.unit_count {
                    commands.entity(unit_entity).despawn();
                } else {
                    unit_transform.translation = Vec3::new(
                        -safe_length * 0.5 + unit_len * (unit.index as f32 + 0.5),
                        0.0,
                        0.01,
                    );
                    unit_transform.rotation = Quat::IDENTITY;
                    unit_transform.scale = Vec3::ONE;
                    sprite.custom_size = Some(Vec2::new(unit_len, spring.unit_size));
                }
            }

            if let Ok((endpoint_entity, endpoint, mut endpoint_transform, mut sprite)) =
                endpoints.get_mut(child)
            {
                let local_x = match endpoint.end {
                    SpringEndIndex::A => -safe_length * 0.5,
                    SpringEndIndex::B => safe_length * 0.5,
                };
                let translation = Vec3::new(local_x, 0.0, endpoint_transform.translation.z);
                let color = endpoint_color(
                    match endpoint.end {
                        SpringEndIndex::A => spring.end_a,
                        SpringEndIndex::B => spring.end_b,
                    },
                    endpoint_entity,
                    &color_sources,
                    &palette,
                );
                if endpoint_transform.translation != translation {
                    endpoint_transform.translation = translation;
                }
                if sprite.color != color {
                    sprite.color = color;
                }
            }
        }

        if unit_count < spring.unit_count {
            commands.entity(spring_entity).with_children(|builder| {
                for index in unit_count..spring.unit_count {
                    spawn_unit(builder, &images, spring_entity, index, spring.unit_size);
                }
            });
        }
    }
}

pub(crate) fn apply_spring_forces(
    springs: Query<&SpringObject, Without<SpringPreview>>,
    geometries: Query<
        (&Position, &Rotation, &ColliderOf),
        (With<PhysicalGeometry>, Without<RigidBodyDisabled>),
    >,
    mut bodies: ParamSet<(
        Query<RigidBodyQueryReadOnly, (Without<PhysicalGeometry>, Without<RigidBodyDisabled>)>,
        Query<Forces, (Without<PhysicalGeometry>, Without<RigidBodyDisabled>)>,
    )>,
) {
    let mut applications = Vec::new();
    {
        let body_query = bodies.p0();
        for spring in &springs {
            let Some(a) = SpringBodyPoint::from_end(spring.end_a, &geometries, &body_query) else {
                continue;
            };
            let Some(b) = SpringBodyPoint::from_end(spring.end_b, &geometries, &body_query) else {
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

    let mut forces = bodies.p1();
    for (entity, force, point) in applications {
        if let Ok(mut body_forces) = forces.get_mut(entity) {
            body_forces.apply_force_at_point(force, point);
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
        geometries: &Query<
            (&Position, &Rotation, &ColliderOf),
            (With<PhysicalGeometry>, Without<RigidBodyDisabled>),
        >,
        bodies: &Query<
            RigidBodyQueryReadOnly,
            (Without<PhysicalGeometry>, Without<RigidBodyDisabled>),
        >,
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
                let (position, rotation, link) = geometries.get(entity).ok()?;
                let body = bodies.get(link.body).ok()?;
                let point = world_point((position.0, *rotation), local_anchor);
                let center_of_mass =
                    world_point((body.position.0, *body.rotation), body.center_of_mass.0);
                Some(Self {
                    entity: Some(link.body),
                    point,
                    velocity: body.velocity_at_point(point - center_of_mass),
                    mass: body.mass().value(),
                })
            }
        }
    }
}

impl SpringObject {
    pub(crate) fn world_points(
        &self,
        bodies: &Query<(&Position, &Rotation)>,
    ) -> Option<(Vec2, Vec2)> {
        Some((self.end_a.world_pos(bodies)?, self.end_b.world_pos(bodies)?))
    }
}

impl SpringEnd {
    pub(crate) fn world_pos(self, bodies: &Query<(&Position, &Rotation)>) -> Option<Vec2> {
        match self {
            Self::Body {
                entity,
                local_anchor,
            } => {
                let (position, rotation) = bodies.get(entity).ok()?;
                Some(world_point((position.0, *rotation), local_anchor))
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
    unit_size * 0.6
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
        }
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
