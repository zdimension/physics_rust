use crate::BORDER_THICKNESS;
use crate::lyon_compat::GeometryBuilder;
use crate::lyon_compat::ShapeBundle;
use crate::lyon_compat::shapes;
use crate::mouse::select::{SelectUnderMouseEvent, SelectionMode};
use crate::mouse_tracking::MainCamera;
use crate::objects::axle::{
    AxleObject, AxleVisual, FixObject, HINGE_MOTOR_VISUAL_DIAMETER, HingeMotorDirection,
    HingeMotorRing, hinge_selection_radius,
};
use crate::objects::laser::{LaserSettings, LaserVisual};
use crate::objects::phy_obj::{FreeformObject, PhysicalObject};
use crate::objects::plane::spawn_plane;
use crate::objects::thruster::{ThrusterInner, ThrusterSettings};
use crate::objects::tracer::{TracerObject, TracerSettings, TracerVisual};
use crate::objects::{ColorComponent, MotorComponent, SettingComponent, SpriteOnly};
use crate::palette::PaletteConfig;
use crate::rng::RngComponent;
use crate::tools::gear::{GearOutline, GearSettings};
use crate::ui::SceneState;
use crate::ui::images::AppIcons;
use crate::update_from::UpdateFrom;
use avian2d::prelude::*;
use bevy::log::info;
use bevy::math::{Vec2, Vec3};
use bevy::prelude::*;

const VIRTUAL_LAYER: u32 = 1 << 31;

static VIRTUAL_LAYER_OBJ: CollisionLayers =
    CollisionLayers::from_bits(VIRTUAL_LAYER, VIRTUAL_LAYER);

pub fn query_only_real() -> SpatialQueryFilter {
    SpatialQueryFilter::from_mask(0xffff_ffff ^ VIRTUAL_LAYER)
}

#[derive(Debug, Clone, Message)]
pub enum AddAxleEvent {
    Mouse(Vec2),
    AddCenter(Entity),
}

#[derive(Debug, Clone, Message)]
pub enum AddObjectEvent {
    Axle(AddAxleEvent),
    Fix(Vec2),
    Circle {
        center: Vec2,
        radius: f32,
    },
    Gear {
        center: Vec2,
        radius: f32,
        settings: GearSettings,
    },
    Plane {
        point: Vec2,
        outward_normal: Vec2,
        color: bevy_egui::egui::ecolor::Hsva,
    },
    Box {
        pos: Vec2,
        size: Vec2,
    },
    Laser(Vec2),
    Thruster(Vec2),
    Tracer(Vec2),
    CenterThruster(Entity),
    CenterTracer(Entity),
    Polygon {
        pos: Vec2,
        points: Vec<Vec2>,
    },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Component)]
pub enum AttachmentKind {
    Fix,
    Axle,
    Laser,
    Thruster,
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

type BodyQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static GlobalTransform,
        &'static Position,
        &'static Rotation,
        &'static Collider,
    ),
    (With<RigidBody>, Without<MainCamera>),
>;

#[derive(Copy, Clone)]
struct BodyHit {
    entity: Entity,
    local_pos: Vec2,
    z: f32,
    rotation: Quat,
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
    query: BodyQuery,
    images: Res<AppIcons>,
    mut commands: Commands,
    cameras: Query<&Transform, With<MainCamera>>,
    palette_config: Res<PaletteConfig>,
    mut z: ResMut<DepthSorter>,
    mut rng: Query<&mut RngComponent>,
    mut select_mouse: MessageWriter<SelectUnderMouseEvent>,
    sensor: Query<&Sensor>,
    scene_state: Res<SceneState>,
    spatial_query: SpatialQuery,
    fixes: Query<(&FixedJoint, &AttachmentJoint), With<FixObject>>,
    body_colors: Query<&ColorComponent>,
) {
    let palette = &palette_config.current_palette;
    let camera = cameras.single().unwrap();
    let camera_scale = camera.scale.x;
    let camera_rotation = camera.rotation;

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
            Gear {
                center,
                radius,
                settings,
            } => {
                let Some(outline) = GearOutline::from_radius(radius, settings) else {
                    continue;
                };
                let gear_pos = z.pos(center);
                let Some(object) = PhysicalObject::freeform_path(outline.path(), gear_pos) else {
                    continue;
                };
                let entity = commands
                    .spawn(object)
                    .insert(FreeformObject)
                    .insert(ChildOf(scene_state.scene))
                    .insert(
                        ColorComponent(palette.get_color_hsva(&mut *rng.single_mut().unwrap()))
                            .update_from_this(),
                    )
                    .log_components()
                    .id();
                let placement = AttachmentPlacement {
                    body1: BodyHit {
                        entity,
                        local_pos: Vec2::ZERO,
                        z: gear_pos.z,
                        rotation: Quat::IDENTITY,
                    },
                    body2: body_hits_at(center, &query, &spatial_query, None).next(),
                    pos: center,
                };
                spawn_axle_attachment(
                    &mut commands,
                    placement,
                    &images,
                    palette.get_color_hsva_opaque(&mut *rng.single_mut().unwrap()),
                    palette.sky_color,
                    camera_scale,
                    camera_rotation,
                    &mut z,
                    scene_state.scene,
                );
            }
            Plane {
                point,
                outward_normal,
                color,
            } => {
                spawn_plane(
                    &mut commands,
                    scene_state.scene,
                    point,
                    outward_normal,
                    color,
                );
            }
            Polygon { pos, ref points } => {
                let Some(object) = PhysicalObject::freeform(points, z.pos(pos)) else {
                    continue;
                };
                commands
                    .spawn(object)
                    .insert(FreeformObject)
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
                        mode: SelectionMode::Replace,
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
                    camera_scale,
                    camera_rotation,
                    &mut z,
                    scene_state.scene,
                );
            }
            Axle(ref ev) => {
                let Some(placement) = axle_placement(ev, &query, &spatial_query, &fixes) else {
                    continue;
                };

                if sensor.get(placement.body1.entity).is_ok() {
                    info!("Add axle on sensor; selecting");
                    select_mouse.write(SelectUnderMouseEvent {
                        pos: placement.pos,
                        mode: SelectionMode::Replace,
                        open_menu: false,
                    });
                    continue;
                }

                spawn_axle_attachment(
                    &mut commands,
                    placement,
                    &images,
                    palette.get_color_hsva_opaque(&mut *rng.single_mut().unwrap()),
                    palette.sky_color,
                    camera_scale,
                    camera_rotation,
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
                    camera_scale,
                    camera_rotation,
                    &mut z,
                    scene_state.scene,
                );
            }
            Thruster(pos) => {
                let Some(placement) = single_body_placement(pos, &query, &spatial_query) else {
                    continue;
                };
                spawn_thruster_attachment(
                    &mut commands,
                    placement,
                    &images,
                    camera_scale,
                    camera_rotation,
                    &mut z,
                    false,
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
                    camera_scale,
                    camera_rotation,
                    &mut z,
                    false,
                );
            }
            CenterThruster(entity) => {
                let Some(placement) = center_placement(entity, &query) else {
                    continue;
                };
                spawn_thruster_attachment(
                    &mut commands,
                    placement,
                    &images,
                    camera_scale,
                    camera_rotation,
                    &mut z,
                    true,
                );
            }
            CenterTracer(entity) => {
                let Some(placement) = center_placement(entity, &query) else {
                    continue;
                };
                let color = body_colors
                    .get(entity)
                    .map(|color| color.0)
                    .unwrap_or_else(|_| {
                        palette.get_color_hsva_opaque(&mut *rng.single_mut().unwrap())
                    });
                spawn_tracer_attachment(
                    &mut commands,
                    placement,
                    &images,
                    color,
                    camera_scale,
                    camera_rotation,
                    &mut z,
                    true,
                );
            }
        }
    }
}

pub fn process_place_attachment(
    mut events: MessageReader<PlaceAttachmentEvent>,
    mut commands: Commands,
    mut attachments: Query<(
        &AttachmentKind,
        Option<&AttachmentLinks>,
        &GlobalTransform,
        &mut Transform,
    )>,
    bodies: BodyQuery,
    spatial_query: SpatialQuery,
    fixes: Query<(&FixedJoint, &AttachmentJoint), With<FixObject>>,
    mut z: ResMut<DepthSorter>,
    scene_state: Res<SceneState>,
    palette_config: Res<PaletteConfig>,
    mut attachment_colors: Query<(
        Entity,
        &ChildOf,
        Option<&AxleBodyColor>,
        Option<&ThrusterInner>,
        Option<&AttachmentSupportColor>,
        Option<&mut Sprite>,
    )>,
) {
    for event in events.read().copied() {
        let Ok((kind, links, global_transform, mut transform)) = attachments.get_mut(event.entity)
        else {
            continue;
        };
        let world_rotation = global_transform.rotation();
        clear_attachment_links(&mut commands, links.copied());

        if *kind == AttachmentKind::Laser {
            let placement = laser_placement(event.pos, &bodies, &spatial_query);
            match placement {
                LaserPlacement::Body(placement) => {
                    *transform = attachment_pose(placement, z.next());
                    transform.rotation = placement.body1.rotation.inverse() * world_rotation;
                    commands
                        .entity(event.entity)
                        .insert(ChildOf(placement.body1.entity));
                }
                LaserPlacement::Sky { pos } => {
                    *transform = sky_attachment_pose(pos, z.next());
                    transform.rotation = world_rotation;
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

        if *kind == AttachmentKind::Thruster {
            let Some(placement) = single_body_placement(event.pos, &bodies, &spatial_query) else {
                commands.entity(event.entity).despawn();
                continue;
            };
            *transform = attachment_pose(placement, z.next());
            transform.rotation = placement.body1.rotation.inverse() * world_rotation;
            commands
                .entity(event.entity)
                .insert(ChildOf(placement.body1.entity))
                .insert(AttachmentLinks::default());
            update_attachment_color_sources(
                event.entity,
                placement.body1.entity,
                None,
                palette_config.current_palette.sky_color,
                &mut commands,
                &mut attachment_colors,
            );
            continue;
        }

        if *kind == AttachmentKind::Tracer {
            let Some(placement) = single_body_placement(event.pos, &bodies, &spatial_query) else {
                commands.entity(event.entity).despawn();
                continue;
            };
            *transform = attachment_pose(placement, z.next());
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
            AttachmentKind::Axle => {
                update_attachment_color_sources(
                    event.entity,
                    placement.body1.entity,
                    placement.body2.map(|body| body.entity),
                    palette_config.current_palette.sky_color,
                    &mut commands,
                    &mut attachment_colors,
                );
                spawn_axle_joint(&mut commands, event.entity, placement, scene_state.scene)
            }
            AttachmentKind::Laser => AttachmentLinks::default(),
            AttachmentKind::Thruster => AttachmentLinks::default(),
            AttachmentKind::Tracer => AttachmentLinks::default(),
        };
        commands.entity(event.entity).insert(links);
    }
}

fn attachment_placement(
    pos: Vec2,
    kind: AttachmentKind,
    current: Option<Entity>,
    bodies: &BodyQuery,
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

fn laser_placement(pos: Vec2, bodies: &BodyQuery, spatial_query: &SpatialQuery) -> LaserPlacement {
    single_body_placement(pos, bodies, spatial_query)
        .map(LaserPlacement::Body)
        .unwrap_or(LaserPlacement::Sky { pos })
}

fn single_body_placement(
    pos: Vec2,
    bodies: &BodyQuery,
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

fn axle_placement(
    event: &AddAxleEvent,
    bodies: &BodyQuery,
    spatial_query: &SpatialQuery,
    fixes: &Query<(&FixedJoint, &AttachmentJoint), With<FixObject>>,
) -> Option<AttachmentPlacement> {
    match *event {
        AddAxleEvent::Mouse(pos) => attachment_placement(
            pos,
            AttachmentKind::Axle,
            None,
            bodies,
            spatial_query,
            fixes,
        ),
        AddAxleEvent::AddCenter(entity) => {
            let Some(placement) = center_placement(entity, bodies) else {
                info!("Can't find transform for entity (add center axle)");
                return None;
            };
            let body2 = body_hits_at(placement.pos, bodies, spatial_query, Some(entity)).next();
            Some(AttachmentPlacement { body2, ..placement })
        }
    }
}

fn center_placement(entity: Entity, bodies: &BodyQuery) -> Option<AttachmentPlacement> {
    let Ok((_, transform, position, _rotation, _)) = bodies.get(entity) else {
        info!("Can't find physical object for centered attachment");
        return None;
    };
    Some(AttachmentPlacement {
        body1: BodyHit {
            entity,
            local_pos: Vec2::ZERO,
            z: transform.translation_vec3a().z,
            rotation: transform.rotation(),
        },
        body2: None,
        pos: position.0,
    })
}

fn body_hits_at<'a>(
    pos: Vec2,
    bodies: &'a BodyQuery,
    spatial_query: &SpatialQuery,
    exclude: Option<Entity>,
) -> impl Iterator<Item = BodyHit> + 'a {
    let mut hits = Vec::new();
    spatial_query.point_intersections_callback(pos, &query_only_real(), |entity| {
        if Some(entity) == exclude {
            return true;
        }
        if let Ok((entity, transform, position, rotation, _)) = bodies.get(entity) {
            hits.push(body_hit(entity, pos, transform, position, rotation));
        }
        true
    });

    for (entity, transform, position, rotation, collider) in bodies.iter() {
        if Some(entity) == exclude || hits.iter().any(|hit| hit.entity == entity) {
            continue;
        }
        if collider.contains_point(*position, *rotation, pos) {
            hits.push(body_hit(entity, pos, transform, position, rotation));
        }
    }

    hits.sort_by(|a, b| a.z.total_cmp(&b.z));
    hits.into_iter().rev()
}

fn body_hit(
    entity: Entity,
    pos: Vec2,
    transform: &GlobalTransform,
    position: &Position,
    rotation: &Rotation,
) -> BodyHit {
    BodyHit {
        entity,
        local_pos: rotation.inverse() * (pos - position.0),
        z: transform.translation_vec3a().z,
        rotation: transform.rotation(),
    }
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
    attachment_pose(placement, z).with_scale(Vec3::new(scale, scale, 1.0))
}

fn attachment_pose(placement: AttachmentPlacement, z: f32) -> Transform {
    Transform::from_translation(placement.body1.local_pos.extend(z - placement.body1.z))
}

fn sky_attachment_pose(pos: Vec2, z: f32) -> Transform {
    Transform::from_translation(pos.extend(z))
}

fn screen_aligned_attachment_transform(
    placement: AttachmentPlacement,
    scale: f32,
    z: f32,
    camera_rotation: Quat,
) -> Transform {
    screen_aligned_attachment_pose(placement, z, camera_rotation)
        .with_scale(Vec3::new(scale, scale, 1.0))
}

fn screen_aligned_attachment_pose(
    placement: AttachmentPlacement,
    z: f32,
    camera_rotation: Quat,
) -> Transform {
    attachment_pose(placement, z).with_rotation(screen_aligned_local_rotation(
        placement.body1.rotation,
        camera_rotation,
    ))
}

fn screen_aligned_sky_attachment_pose(pos: Vec2, z: f32, camera_rotation: Quat) -> Transform {
    sky_attachment_pose(pos, z).with_rotation(camera_rotation)
}

fn screen_aligned_local_rotation(parent_rotation: Quat, camera_rotation: Quat) -> Quat {
    parent_rotation.inverse() * camera_rotation
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
    camera_rotation: Quat,
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
                screen_aligned_attachment_transform(placement, scale, z.next(), camera_rotation),
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

fn spawn_axle_attachment(
    commands: &mut Commands,
    placement: AttachmentPlacement,
    images: &AppIcons,
    color: bevy_egui::egui::ecolor::Hsva,
    sky_color: Color,
    camera_scale: f32,
    camera_rotation: Quat,
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
                    radius: hinge_selection_radius(false),
                    ..Default::default()
                }),
                screen_aligned_attachment_transform(placement, scale, z.next(), camera_rotation),
                Visibility::Inherited,
            ),
            crate::make_stroke(Color::srgba(0.0, 0.0, 0.0, 0.0), BORDER_THICKNESS),
            SpriteOnly,
            Collider::circle(0.5),
            VIRTUAL_LAYER_OBJ,
            Sensor,
            AttachmentKind::Axle,
            AxleVisual,
            ColorComponent(color).update_from_this(),
            MotorComponent::default(),
            ChildOf(placement.body1.entity),
        ))
        .with_children(|builder| {
            builder.spawn((
                HingeMotorRing,
                Sprite {
                    image: images.hinge_motor.clone(),
                    custom_size: Some(Vec2::splat(HINGE_MOTOR_VISUAL_DIAMETER)),
                    ..Default::default()
                },
                Transform::from_translation(Vec3::Z * -0.03),
                Visibility::Hidden,
                UpdateFrom::<ColorComponent>::This,
            ));
            builder.spawn((
                HingeMotorDirection { reversed: true },
                Sprite {
                    image: images.hinge_motor_ccw.clone(),
                    custom_size: Some(Vec2::splat(HINGE_MOTOR_VISUAL_DIAMETER)),
                    ..Default::default()
                },
                Transform::from_translation(Vec3::Z * -0.02),
                Visibility::Hidden,
            ));
            builder.spawn((
                HingeMotorDirection { reversed: false },
                Sprite {
                    image: images.hinge_motor_cw.clone(),
                    custom_size: Some(Vec2::splat(HINGE_MOTOR_VISUAL_DIAMETER)),
                    ..Default::default()
                },
                Transform::from_translation(Vec3::Z * -0.02),
                Visibility::Hidden,
            ));
            builder.spawn((
                AxleBodyColor,
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
    let links = spawn_axle_joint(commands, visual, placement, scene);
    commands.entity(visual).insert(links);
    visual
}

fn spawn_laser_attachment(
    commands: &mut Commands,
    placement: LaserPlacement,
    images: &AppIcons,
    color: bevy_egui::egui::ecolor::Hsva,
    camera_scale: f32,
    camera_rotation: Quat,
    z: &mut DepthSorter,
    scene: Entity,
) -> Entity {
    let scale = camera_scale * DEFAULT_OBJ_SIZE;
    let (transform, parent) = match placement {
        LaserPlacement::Body(placement) => (
            screen_aligned_attachment_pose(placement, z.next(), camera_rotation),
            placement.body1.entity,
        ),
        LaserPlacement::Sky { pos } => (
            screen_aligned_sky_attachment_pose(pos, z.next(), camera_rotation),
            scene,
        ),
    };
    commands
        .spawn((
            transform,
            Visibility::Inherited,
            LaserSettings {
                size: scale,
                fade_distance: 300.0,
            },
            ColorComponent(color).update_from_this(),
            Collider::rectangle(scale * 0.5, scale * 0.25),
            VIRTUAL_LAYER_OBJ,
            Sensor,
            AttachmentKind::Laser,
            AttachmentLinks::default(),
            ChildOf(parent),
        ))
        .with_child((
            LaserVisual,
            Sprite {
                image: images.laserpen.clone(),
                ..Default::default()
            },
            Transform::from_scale(Vec3::new(scale / 256.0, scale / 256.0, 1.0)),
            UpdateFrom::<ColorComponent>::This,
        ))
        .id()
}

fn spawn_thruster_attachment(
    commands: &mut Commands,
    placement: AttachmentPlacement,
    images: &AppIcons,
    camera_scale: f32,
    camera_rotation: Quat,
    z: &mut DepthSorter,
    align_with_body: bool,
) -> Entity {
    let scale = camera_scale * DEFAULT_OBJ_SIZE * 2.0;
    let sprite_scale = Vec3::new(scale / 256.0, scale / 256.0, 1.0);
    commands
        .spawn((
            if align_with_body {
                attachment_pose(placement, z.next())
            } else {
                screen_aligned_attachment_pose(placement, z.next(), camera_rotation)
            },
            Visibility::Inherited,
            ThrusterSettings::default(),
            ColorComponent(bevy_egui::egui::ecolor::Hsva::new(0.0, 0.0, 1.0, 1.0))
                .update_from_this(),
            Collider::rectangle(scale, scale * 0.5),
            VIRTUAL_LAYER_OBJ,
            Sensor,
            AttachmentKind::Thruster,
            AttachmentLinks::default(),
            ChildOf(placement.body1.entity),
        ))
        .with_children(|builder| {
            builder.spawn((
                ThrusterInner,
                Sprite {
                    image: images.thruster_inner.clone(),
                    ..Default::default()
                },
                Transform::from_scale(sprite_scale),
                UpdateFrom::<ColorComponent>::entity(placement.body1.entity),
            ));
            builder.spawn((
                Sprite {
                    image: images.thruster_thrust.clone(),
                    ..Default::default()
                },
                Transform::from_translation(Vec3::Z * 0.01).with_scale(sprite_scale),
            ));
            builder.spawn((
                Sprite {
                    image: images.thruster_outer.clone(),
                    ..Default::default()
                },
                Transform::from_translation(Vec3::Z * 0.02).with_scale(sprite_scale),
                UpdateFrom::<ColorComponent>::This,
            ));
        })
        .id()
}

fn spawn_tracer_attachment(
    commands: &mut Commands,
    placement: AttachmentPlacement,
    images: &AppIcons,
    color: bevy_egui::egui::ecolor::Hsva,
    camera_scale: f32,
    camera_rotation: Quat,
    z: &mut DepthSorter,
    center_on_body: bool,
) -> Entity {
    let scale = camera_scale * DEFAULT_OBJ_SIZE;
    commands
        .spawn((
            ShapeBundle::new(
                GeometryBuilder::build_as(&shapes::Circle {
                    radius: scale * 0.55,
                    ..Default::default()
                }),
                if center_on_body {
                    attachment_pose(placement, z.next())
                } else {
                    screen_aligned_attachment_pose(placement, z.next(), camera_rotation)
                },
                Visibility::Inherited,
            ),
            crate::make_stroke(Color::srgba(0.0, 0.0, 0.0, 0.0), BORDER_THICKNESS),
            TracerObject::default(),
            TracerSettings {
                diameter: scale,
                ..Default::default()
            },
            ColorComponent(color).update_from_this(),
            Collider::circle(scale * 0.5),
            VIRTUAL_LAYER_OBJ,
            Sensor,
            SpriteOnly,
            AttachmentKind::Tracer,
            AttachmentLinks::default(),
            ChildOf(placement.body1.entity),
        ))
        .with_child((
            TracerVisual,
            Sprite {
                image: images.tracer.clone(),
                custom_size: Some(Vec2::splat(scale)),
                ..Default::default()
            },
            Transform::default(),
            UpdateFrom::<ColorComponent>::This,
        ))
        .id()
}

fn spawn_axle_joint(
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
            AxleObject,
            AttachmentJoint { visual },
            JointCollisionDisabled,
            JointForces::new(),
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
pub(crate) struct AxleBodyColor;

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
        Option<&AxleBodyColor>,
        Option<&ThrusterInner>,
        Option<&AttachmentSupportColor>,
        Option<&mut Sprite>,
    )>,
) {
    for (entity, parent, is_body, is_thruster_inner, is_other, sprite) in colors.iter_mut() {
        if parent.parent() != visual {
            continue;
        }
        if is_body.is_some() || is_thruster_inner.is_some() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_attachment_starts_horizontal_on_a_rotated_camera() {
        let parent_rotation = Quat::from_rotation_z(45.0_f32.to_radians());
        let camera_rotation = Quat::from_rotation_z(-30.0_f32.to_radians());
        let local_rotation = screen_aligned_local_rotation(parent_rotation, camera_rotation);

        let screen_rotation = camera_rotation.inverse() * parent_rotation * local_rotation;

        assert!((screen_rotation * Vec3::X).distance(Vec3::X) < 1.0e-5);
    }

    #[test]
    fn sky_attachment_starts_horizontal_on_a_rotated_camera() {
        let camera_rotation = Quat::from_rotation_z(60.0_f32.to_radians());
        let transform =
            screen_aligned_sky_attachment_pose(Vec2::new(4.0, 7.0), 2.0, camera_rotation);

        let screen_rotation = camera_rotation.inverse() * transform.rotation;

        assert!((screen_rotation * Vec3::X).distance(Vec3::X) < 1.0e-5);
    }
}
