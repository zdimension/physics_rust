use crate::lyon_compat::GeometryBuilder;
use crate::lyon_compat::ShapeBundle;
use crate::lyon_compat::shapes;
use crate::mouse::select::SelectUnderMouseEvent;
use crate::mouse_tracking::MainCamera;
use crate::objects::hinge::{FixObject, HingeObject};
use crate::objects::laser::LaserBundle;
use crate::objects::phy_obj::PhysicalObject;
use crate::objects::tracer::TracerObject;
use crate::objects::{ColorComponent, MotorComponent, SettingComponent, SizeComponent, SpriteOnly};
use crate::palette::PaletteConfig;
use crate::rng::RngComponent;
use crate::ui::SceneState;
use crate::ui::images::AppIcons;
use crate::update_from::UpdateFrom;
use crate::{BORDER_THICKNESS, InvTransformPoint};
use avian2d::{math::*, prelude::*};
use bevy::log::info;
use bevy::math::{Vec2, Vec3, Vec3Swizzles};
use bevy::prelude::*;

const VIRTUAL_LAYER: u32 = 1 << 31;

static VIRTUAL_LAYER_OBJ: CollisionLayers =
    CollisionLayers::from_bits(VIRTUAL_LAYER, VIRTUAL_LAYER);

pub fn query_only_real() -> SpatialQueryFilter {
    SpatialQueryFilter::from_mask(0xffff_ffff ^ VIRTUAL_LAYER)
}

#[derive(Debug, Clone, Message)]
pub enum AddHingeEvent {
    Mouse(Vec2),
    AddCenter(Entity),
}

#[derive(Debug, Clone, Message)]
pub enum AddObjectEvent {
    Hinge(AddHingeEvent),
    Fix(Vec2),
    Circle { center: Vec2, radius: f32 },
    Box { pos: Vec2, size: Vec2 },
    Laser(Vec2),
    Tracer(Vec2),
    Polygon { pos: Vec2, points: Vec<Vec2> },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Component)]
pub enum AttachmentKind {
    Fix,
    Hinge,
    Laser,
    Tracer,
}

#[derive(Copy, Clone, Debug, Default, Component)]
pub struct AttachmentLinks {
    pub(crate) joint: Option<Entity>,
    pub(crate) sky_anchor: Option<Entity>,
}

#[derive(Copy, Clone, Debug, Component)]
pub struct AttachmentJoint {
    pub(crate) visual: Entity,
}

#[derive(Copy, Clone, Debug, Message)]
pub struct PlaceAttachmentEvent {
    pub entity: Entity,
    pub pos: Vec2,
}

const DEFAULT_OBJ_SIZE: f32 = 66.0;

#[derive(Copy, Clone)]
struct BodyHit {
    entity: Entity,
    local_pos: Vec2,
    z: f32,
}

#[derive(Copy, Clone)]
struct AttachmentPlacement {
    body1: BodyHit,
    body2: Option<BodyHit>,
    pos: Vec2,
}

#[derive(Copy, Clone)]
enum LaserPlacement {
    Body(AttachmentPlacement),
    Sky { pos: Vec2 },
}

pub fn process_add_object(
    mut events: MessageReader<AddObjectEvent>,
    query: Query<(Entity, &GlobalTransform), (With<RigidBody>, Without<MainCamera>)>,
    images: Res<AppIcons>,
    mut commands: Commands,
    mut cameras: Query<&mut Transform, With<MainCamera>>,
    palette_config: Res<PaletteConfig>,
    mut z: ResMut<DepthSorter>,
    mut rng: Query<&mut RngComponent>,
    mut select_mouse: MessageWriter<SelectUnderMouseEvent>,
    sensor: Query<&Sensor>,
    scene_state: Res<SceneState>,
    spatial_query: SpatialQuery,
    fixes: Query<(&FixedJoint, &AttachmentJoint), With<FixObject>>,
) {
    let palette = &palette_config.current_palette;

    for ev in events.read() {
        use AddObjectEvent::*;
        match *ev {
            Box { pos, size } => {
                commands
                    .spawn(PhysicalObject::rect(size, z.pos(pos)))
                    .insert(ChildOf(scene_state.scene))
                    .insert(
                        ColorComponent(palette.get_color_hsva(&mut *rng.single_mut().unwrap()))
                            .update_from_this(),
                    )
                    .log_components();
            }
            Circle { center, radius } => {
                commands
                    .spawn(PhysicalObject::ball(radius, z.pos(center)))
                    .insert(ChildOf(scene_state.scene))
                    .insert(
                        ColorComponent(palette.get_color_hsva(&mut *rng.single_mut().unwrap()))
                            .update_from_this(),
                    )
                    .log_components();
            }
            Polygon { pos, ref points } => {
                commands
                    .spawn(PhysicalObject::poly(points.clone(), z.pos(pos)))
                    .insert(ChildOf(scene_state.scene))
                    .insert(
                        ColorComponent(palette.get_color_hsva(&mut *rng.single_mut().unwrap()))
                            .update_from_this(),
                    )
                    .log_components();
            }
            Fix(pos) => {
                let Some(placement) = attachment_placement(
                    pos,
                    AttachmentKind::Fix,
                    None,
                    &query,
                    &spatial_query,
                    &fixes,
                ) else {
                    continue;
                };

                if sensor.get(placement.body1.entity).is_ok() {
                    select_mouse.write(SelectUnderMouseEvent {
                        pos,
                        open_menu: false,
                    });
                    continue;
                }

                spawn_fix_attachment(
                    &mut commands,
                    placement,
                    &images,
                    palette.get_color_hsva_opaque(&mut *rng.single_mut().unwrap()),
                    palette.sky_color,
                    cameras.single_mut().unwrap().scale.x,
                    &mut z,
                    scene_state.scene,
                );
            }
            Hinge(ref ev) => {
                let Some(placement) = hinge_placement(ev, &query, &spatial_query, &fixes) else {
                    continue;
                };

                if sensor.get(placement.body1.entity).is_ok() {
                    info!("Add hinge on sensor; selecting");
                    select_mouse.write(SelectUnderMouseEvent {
                        pos: placement.pos,
                        open_menu: false,
                    });
                    continue;
                }

                spawn_hinge_attachment(
                    &mut commands,
                    placement,
                    &images,
                    palette.get_color_hsva_opaque(&mut *rng.single_mut().unwrap()),
                    palette.sky_color,
                    cameras.single_mut().unwrap().scale.x,
                    &mut z,
                    scene_state.scene,
                );
            }
            Laser(pos) => {
                let placement = laser_placement(pos, &query, &spatial_query);
                spawn_laser_attachment(
                    &mut commands,
                    placement,
                    &images,
                    palette.get_color_hsva_opaque(&mut *rng.single_mut().unwrap()),
                    cameras.single_mut().unwrap().scale.x,
                    &mut z,
                    scene_state.scene,
                );
            }
            Tracer(pos) => {
                let Some(placement) = single_body_placement(pos, &query, &spatial_query) else {
                    continue;
                };
                spawn_tracer_attachment(
                    &mut commands,
                    placement,
                    &images,
                    palette.get_color_hsva_opaque(&mut *rng.single_mut().unwrap()),
                    cameras.single_mut().unwrap().scale.x,
                    &mut z,
                );
            }
        }
    }
}

pub fn process_place_attachment(
    mut events: MessageReader<PlaceAttachmentEvent>,
    mut commands: Commands,
    mut attachments: Query<(&AttachmentKind, Option<&AttachmentLinks>, &mut Transform)>,
    bodies: Query<(Entity, &GlobalTransform), (With<RigidBody>, Without<MainCamera>)>,
    spatial_query: SpatialQuery,
    fixes: Query<(&FixedJoint, &AttachmentJoint), With<FixObject>>,
    mut z: ResMut<DepthSorter>,
    scene_state: Res<SceneState>,
    palette_config: Res<PaletteConfig>,
    mut attachment_colors: Query<(
        Entity,
        &ChildOf,
        Option<&HingeBodyColor>,
        Option<&AttachmentSupportColor>,
        Option<&mut Sprite>,
    )>,
) {
    for event in events.read().copied() {
        let Ok((kind, links, mut transform)) = attachments.get_mut(event.entity) else {
            continue;
        };
        clear_attachment_links(&mut commands, links.copied());

        if *kind == AttachmentKind::Laser {
            let placement = laser_placement(event.pos, &bodies, &spatial_query);
            let current_scale = transform.scale.x;
            match placement {
                LaserPlacement::Body(placement) => {
                    *transform = attachment_transform(placement, current_scale, z.next());
                    commands
                        .entity(event.entity)
                        .insert(ChildOf(placement.body1.entity));
                }
                LaserPlacement::Sky { pos } => {
                    *transform = sky_attachment_transform(pos, current_scale, z.next());
                    commands
                        .entity(event.entity)
                        .insert(ChildOf(scene_state.scene));
                }
            }
            commands
                .entity(event.entity)
                .insert(AttachmentLinks::default());
            continue;
        }

        if *kind == AttachmentKind::Tracer {
            let Some(placement) = single_body_placement(event.pos, &bodies, &spatial_query) else {
                commands.entity(event.entity).despawn();
                continue;
            };
            let current_scale = transform.scale.x;
            *transform = attachment_transform(placement, current_scale, z.next());
            commands
                .entity(event.entity)
                .insert(ChildOf(placement.body1.entity))
                .insert(AttachmentLinks::default());
            continue;
        }

        let Some(placement) = attachment_placement(
            event.pos,
            *kind,
            Some(event.entity),
            &bodies,
            &spatial_query,
            &fixes,
        ) else {
            commands.entity(event.entity).despawn();
            continue;
        };

        let current_scale = transform.scale.x;
        *transform = attachment_transform(placement, current_scale, z.next());
        commands
            .entity(event.entity)
            .insert(ChildOf(placement.body1.entity));

        let links = match *kind {
            AttachmentKind::Fix => {
                update_attachment_color_sources(
                    event.entity,
                    placement.body1.entity,
                    placement.body2.map(|body| body.entity),
                    palette_config.current_palette.sky_color,
                    &mut commands,
                    &mut attachment_colors,
                );
                spawn_fix_joint(&mut commands, event.entity, placement, scene_state.scene)
            }
            AttachmentKind::Hinge => {
                update_attachment_color_sources(
                    event.entity,
                    placement.body1.entity,
                    placement.body2.map(|body| body.entity),
                    palette_config.current_palette.sky_color,
                    &mut commands,
                    &mut attachment_colors,
                );
                spawn_hinge_joint(&mut commands, event.entity, placement, scene_state.scene)
            }
            AttachmentKind::Laser => AttachmentLinks::default(),
            AttachmentKind::Tracer => AttachmentLinks::default(),
        };
        commands.entity(event.entity).insert(links);
    }
}

fn attachment_placement(
    pos: Vec2,
    kind: AttachmentKind,
    current: Option<Entity>,
    bodies: &Query<(Entity, &GlobalTransform), (With<RigidBody>, Without<MainCamera>)>,
    spatial_query: &SpatialQuery,
    fixes: &Query<(&FixedJoint, &AttachmentJoint), With<FixObject>>,
) -> Option<AttachmentPlacement> {
    let mut hits = body_hits_at(pos, bodies, spatial_query, None);
    let body1 = hits.next()?;
    let mut body2 = hits.next();

    if kind == AttachmentKind::Fix
        && body2.is_some_and(|body| duplicate_fix_exists(body1.entity, body.entity, current, fixes))
    {
        body2 = None;
    }

    Some(AttachmentPlacement { body1, body2, pos })
}

fn laser_placement(
    pos: Vec2,
    bodies: &Query<(Entity, &GlobalTransform), (With<RigidBody>, Without<MainCamera>)>,
    spatial_query: &SpatialQuery,
) -> LaserPlacement {
    single_body_placement(pos, bodies, spatial_query)
        .map(LaserPlacement::Body)
        .unwrap_or(LaserPlacement::Sky { pos })
}

fn single_body_placement(
    pos: Vec2,
    bodies: &Query<(Entity, &GlobalTransform), (With<RigidBody>, Without<MainCamera>)>,
    spatial_query: &SpatialQuery,
) -> Option<AttachmentPlacement> {
    body_hits_at(pos, bodies, spatial_query, None)
        .next()
        .map(|body1| AttachmentPlacement {
            body1,
            body2: None,
            pos,
        })
}

fn hinge_placement(
    event: &AddHingeEvent,
    bodies: &Query<(Entity, &GlobalTransform), (With<RigidBody>, Without<MainCamera>)>,
    spatial_query: &SpatialQuery,
    fixes: &Query<(&FixedJoint, &AttachmentJoint), With<FixObject>>,
) -> Option<AttachmentPlacement> {
    match *event {
        AddHingeEvent::Mouse(pos) => attachment_placement(
            pos,
            AttachmentKind::Hinge,
            None,
            bodies,
            spatial_query,
            fixes,
        ),
        AddHingeEvent::AddCenter(entity) => {
            let Ok((_, transform)) = bodies.get(entity) else {
                info!("Can't find transform for entity (add center axle)");
                return None;
            };
            let pos = transform.translation_vec3a().xy();
            let body1 = BodyHit {
                entity,
                local_pos: Vec2::ZERO,
                z: transform.translation_vec3a().z,
            };
            let body2 = body_hits_at(pos, bodies, spatial_query, Some(entity)).next();
            Some(AttachmentPlacement { body1, body2, pos })
        }
    }
}

fn body_hits_at<'a>(
    pos: Vec2,
    bodies: &'a Query<(Entity, &GlobalTransform), (With<RigidBody>, Without<MainCamera>)>,
    spatial_query: &SpatialQuery,
    exclude: Option<Entity>,
) -> impl Iterator<Item = BodyHit> + 'a {
    let mut hits = Vec::new();
    spatial_query.point_intersections_callback(pos, &query_only_real(), |entity| {
        if Some(entity) == exclude {
            return true;
        }
        if let Ok((entity, transform)) = bodies.get(entity) {
            hits.push(BodyHit {
                entity,
                local_pos: transform.to_local(pos),
                z: transform.translation_vec3a().z,
            });
        }
        true
    });
    hits.sort_by(|a, b| a.z.total_cmp(&b.z));
    hits.into_iter().rev()
}

fn duplicate_fix_exists(
    body1: Entity,
    body2: Entity,
    current: Option<Entity>,
    fixes: &Query<(&FixedJoint, &AttachmentJoint), With<FixObject>>,
) -> bool {
    fixes.iter().any(|(joint, link)| {
        Some(link.visual) != current && joint.body1 == body1 && joint.body2 == body2
    })
}

fn attachment_transform(placement: AttachmentPlacement, scale: f32, z: f32) -> Transform {
    Transform::from_translation(placement.body1.local_pos.extend(z - placement.body1.z))
        .with_scale(Vec3::new(scale, scale, 1.0))
}

fn sky_attachment_transform(pos: Vec2, scale: f32, z: f32) -> Transform {
    Transform::from_translation(pos.extend(z)).with_scale(Vec3::new(scale, scale, 1.0))
}

pub(crate) fn despawn_attachment_links(commands: &mut Commands, links: Option<&AttachmentLinks>) {
    let Some(links) = links else {
        return;
    };
    if let Some(joint) = links.joint {
        commands.entity(joint).despawn();
    }
    if let Some(sky_anchor) = links.sky_anchor {
        commands.entity(sky_anchor).despawn();
    }
}

fn clear_attachment_links(commands: &mut Commands, links: Option<AttachmentLinks>) {
    despawn_attachment_links(commands, links.as_ref());
}

fn spawn_fix_attachment(
    commands: &mut Commands,
    placement: AttachmentPlacement,
    images: &AppIcons,
    color: bevy_egui::egui::ecolor::Hsva,
    sky_color: Color,
    camera_scale: f32,
    z: &mut DepthSorter,
    scene: Entity,
) -> Entity {
    let scale = camera_scale * DEFAULT_OBJ_SIZE;
    let visual = commands
        .spawn((
            ShapeBundle::new(
                GeometryBuilder::build_as(&shapes::Circle {
                    radius: 0.5 * 1.1,
                    ..Default::default()
                }),
                attachment_transform(placement, scale, z.next()),
                Visibility::Inherited,
            ),
            crate::make_stroke(Color::srgba(0.0, 0.0, 0.0, 0.0), BORDER_THICKNESS),
            SpriteOnly,
            Collider::circle(0.5),
            VIRTUAL_LAYER_OBJ,
            Sensor,
            AttachmentKind::Fix,
            ColorComponent(color).update_from_this(),
            ChildOf(placement.body1.entity),
        ))
        .with_children(|builder| {
            builder.spawn((
                Sprite {
                    image: images.fixjoint_outer.clone(),
                    ..Default::default()
                },
                Transform::from_scale(Vec3::new(1.0 / 256.0, 1.0 / 256.0, 1.0)),
                UpdateFrom::<ColorComponent>::This,
            ));
            let mut inner = builder.spawn((
                AttachmentSupportColor,
                Sprite {
                    image: images.fixjoint_inner.clone(),
                    color: sky_color,
                    ..Default::default()
                },
                Transform::from_scale(Vec3::new(1.0 / 256.0, 1.0 / 256.0, 1.0)),
            ));
            if let Some(body2) = placement.body2 {
                inner.insert(UpdateFrom::<ColorComponent>::entity(body2.entity));
            }
        })
        .id();
    let links = spawn_fix_joint(commands, visual, placement, scene);
    commands.entity(visual).insert(links);
    visual
}

fn spawn_hinge_attachment(
    commands: &mut Commands,
    placement: AttachmentPlacement,
    images: &AppIcons,
    color: bevy_egui::egui::ecolor::Hsva,
    sky_color: Color,
    camera_scale: f32,
    z: &mut DepthSorter,
    scene: Entity,
) -> Entity {
    let scale = camera_scale * DEFAULT_OBJ_SIZE;
    const IMAGE_SCALE: f32 = 1.0 / 256.0;
    const IMAGE_SCALE_VEC: Vec3 = Vec3::new(IMAGE_SCALE, IMAGE_SCALE, 1.0);

    let visual = commands
        .spawn((
            ShapeBundle::new(
                GeometryBuilder::build_as(&shapes::Circle {
                    radius: 0.5 * 1.1,
                    ..Default::default()
                }),
                attachment_transform(placement, scale, z.next()),
                Visibility::Inherited,
            ),
            crate::make_stroke(Color::srgba(0.0, 0.0, 0.0, 0.0), BORDER_THICKNESS),
            SpriteOnly,
            Collider::circle(0.5),
            VIRTUAL_LAYER_OBJ,
            Sensor,
            AttachmentKind::Hinge,
            ColorComponent(color).update_from_this(),
            MotorComponent::default(),
            ChildOf(placement.body1.entity),
        ))
        .with_children(|builder| {
            builder.spawn((
                HingeBodyColor,
                Sprite {
                    image: images.hinge_balls.clone(),
                    ..Default::default()
                },
                Transform::from_scale(IMAGE_SCALE_VEC),
                UpdateFrom::<ColorComponent>::entity(placement.body1.entity),
            ));
            builder.spawn((
                Sprite {
                    image: images.hinge_background.clone(),
                    ..Default::default()
                },
                Transform::from_scale(IMAGE_SCALE_VEC),
                UpdateFrom::<ColorComponent>::This,
            ));
            let mut inner = builder.spawn((
                AttachmentSupportColor,
                Sprite {
                    image: images.hinge_inner.clone(),
                    color: sky_color,
                    ..Default::default()
                },
                Transform::from_scale(IMAGE_SCALE_VEC),
            ));
            if let Some(body2) = placement.body2 {
                inner.insert(UpdateFrom::<ColorComponent>::entity(body2.entity));
            }
        })
        .id();
    let links = spawn_hinge_joint(commands, visual, placement, scene);
    commands.entity(visual).insert(links);
    visual
}

fn spawn_laser_attachment(
    commands: &mut Commands,
    placement: LaserPlacement,
    images: &AppIcons,
    color: bevy_egui::egui::ecolor::Hsva,
    camera_scale: f32,
    z: &mut DepthSorter,
    scene: Entity,
) -> Entity {
    let scale = camera_scale * DEFAULT_OBJ_SIZE;
    let (transform, parent) = match placement {
        LaserPlacement::Body(placement) => (
            attachment_transform(placement, scale, z.next()),
            placement.body1.entity,
        ),
        LaserPlacement::Sky { pos } => (sky_attachment_transform(pos, scale, z.next()), scene),
    };
    commands
        .spawn((
            ShapeBundle::new(
                GeometryBuilder::build_as(&shapes::Rectangle {
                    extents: Vec2::new(1.0, 0.5) * 1.1,
                    ..Default::default()
                }),
                transform,
                Visibility::Inherited,
            ),
            crate::make_stroke(Color::srgba(0.0, 0.0, 0.0, 0.0), BORDER_THICKNESS),
            LaserBundle {
                fade_distance: 10.0,
            },
            ColorComponent(color).update_from_this(),
            Collider::rectangle(0.5, 0.25),
            VIRTUAL_LAYER_OBJ,
            SizeComponent(scale),
            Sensor,
            AttachmentKind::Laser,
            AttachmentLinks::default(),
            UpdateFrom::<SizeComponent>::This,
            ChildOf(parent),
        ))
        .with_child((
            Sprite {
                image: images.laserpen.clone(),
                ..Default::default()
            },
            Transform::from_scale(Vec3::new(1.0 / 256.0, 1.0 / 256.0, 1.0)),
            UpdateFrom::<ColorComponent>::This,
        ))
        .id()
}

fn spawn_tracer_attachment(
    commands: &mut Commands,
    placement: AttachmentPlacement,
    images: &AppIcons,
    color: bevy_egui::egui::ecolor::Hsva,
    camera_scale: f32,
    z: &mut DepthSorter,
) -> Entity {
    let scale = camera_scale * DEFAULT_OBJ_SIZE;
    commands
        .spawn((
            ShapeBundle::new(
                GeometryBuilder::build_as(&shapes::Circle {
                    radius: 0.5 * 1.1,
                    ..Default::default()
                }),
                attachment_transform(placement, scale, z.next()),
                Visibility::Inherited,
            ),
            crate::make_stroke(Color::srgba(0.0, 0.0, 0.0, 0.0), BORDER_THICKNESS),
            TracerObject::default(),
            ColorComponent(color).update_from_this(),
            Collider::circle(0.5),
            VIRTUAL_LAYER_OBJ,
            SizeComponent(scale),
            Sensor,
            SpriteOnly,
            AttachmentKind::Tracer,
            AttachmentLinks::default(),
            UpdateFrom::<SizeComponent>::This,
            ChildOf(placement.body1.entity),
        ))
        .with_child((
            Sprite {
                image: images.tracer.clone(),
                custom_size: Some(Vec2::ONE),
                ..Default::default()
            },
            Transform::default(),
            UpdateFrom::<ColorComponent>::This,
        ))
        .id()
}

fn spawn_hinge_joint(
    commands: &mut Commands,
    visual: Entity,
    placement: AttachmentPlacement,
    scene: Entity,
) -> AttachmentLinks {
    let (body2, sky_anchor) = body2_or_sky_anchor(commands, placement, scene);
    let mut joint = RevoluteJoint::new(placement.body1.entity, body2)
        .with_local_anchor1(placement.body1.local_pos);
    if let Some(body2) = placement.body2 {
        joint = joint.with_local_anchor2(body2.local_pos);
    }
    let joint = commands
        .spawn((
            HingeObject,
            AttachmentJoint { visual },
            JointCollisionDisabled,
            UpdateFrom::<MotorComponent>::entity(visual),
            joint,
            ChildOf(scene),
        ))
        .id();

    AttachmentLinks {
        joint: Some(joint),
        sky_anchor,
    }
}

fn spawn_fix_joint(
    commands: &mut Commands,
    visual: Entity,
    placement: AttachmentPlacement,
    scene: Entity,
) -> AttachmentLinks {
    let (body2, sky_anchor) = body2_or_sky_anchor(commands, placement, scene);
    let mut joint = FixedJoint::new(placement.body1.entity, body2)
        .with_local_anchor1(placement.body1.local_pos)
        .with_basis(Rotation::default());
    if let Some(body2) = placement.body2 {
        joint = joint.with_local_anchor2(body2.local_pos);
    }
    let joint = commands
        .spawn((
            FixObject,
            AttachmentJoint { visual },
            JointCollisionDisabled,
            joint,
            ChildOf(scene),
        ))
        .id();

    AttachmentLinks {
        joint: Some(joint),
        sky_anchor,
    }
}

fn body2_or_sky_anchor(
    commands: &mut Commands,
    placement: AttachmentPlacement,
    scene: Entity,
) -> (Entity, Option<Entity>) {
    if let Some(body2) = placement.body2 {
        return (body2.entity, None);
    }

    let sky_anchor = commands
        .spawn((
            RigidBody::Kinematic,
            Position(placement.pos),
            Transform::from_translation(placement.pos.extend(0.0)),
            ChildOf(scene),
        ))
        .id();
    (sky_anchor, Some(sky_anchor))
}

#[derive(Component)]
pub(crate) struct HingeBodyColor;

#[derive(Component)]
pub(crate) struct AttachmentSupportColor;

fn update_attachment_color_sources(
    visual: Entity,
    body1: Entity,
    body2: Option<Entity>,
    sky_color: Color,
    commands: &mut Commands,
    colors: &mut Query<(
        Entity,
        &ChildOf,
        Option<&HingeBodyColor>,
        Option<&AttachmentSupportColor>,
        Option<&mut Sprite>,
    )>,
) {
    for (entity, parent, is_body, is_other, sprite) in colors.iter_mut() {
        if parent.parent() != visual {
            continue;
        }
        if is_body.is_some() {
            commands
                .entity(entity)
                .insert(UpdateFrom::<ColorComponent>::entity(body1));
        } else if is_other.is_some() {
            if let Some(body2) = body2 {
                commands
                    .entity(entity)
                    .insert(UpdateFrom::<ColorComponent>::entity(body2));
            } else {
                commands
                    .entity(entity)
                    .remove::<UpdateFrom<ColorComponent>>();
                if let Some(mut sprite) = sprite {
                    sprite.color = sky_color;
                }
            }
        }
    }
}

#[derive(Default, Resource)]
pub struct DepthSorter {
    current_depth: f32,
}

impl DepthSorter {
    pub fn next(&mut self) -> f32 {
        self.current_depth += 1.0;
        self.current_depth
    }

    pub fn pos(&mut self, pos: Vec2) -> Vec3 {
        pos.extend(self.next())
    }

    pub fn include(&mut self, z: f32) {
        self.current_depth = self.current_depth.max(z);
    }
}
