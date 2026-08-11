use crate::BORDER_WIDTH_PX;
use crate::lyon_compat::GeometryBuilder;
use crate::lyon_compat::ShapeBundle;
use crate::lyon_compat::shapes;
use crate::mouse::select::{SelectUnderMouseEvent, SelectionMode};
use crate::mouse_tracking::MainCamera;
use crate::objects::axle::{
    AxleObject, AxleVisual, FixObject, HINGE_MOTOR_VISUAL_DIAMETER, HingeMotorDirection,
    HingeMotorRing, JointGeometry, hinge_selection_radius,
};
use crate::objects::body::{BodyTransform, local_point, world_point};
use crate::objects::laser::{LaserSettings, LaserVisual};
use crate::objects::phy_obj::{FreeformObject, PhysicalGeometry, PhysicalObject};
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
use bevy::ecs::world::CommandQueue;
use bevy::log::info;
use bevy::math::{Vec2, Vec3};
use bevy::prelude::*;
use bevy_egui::egui::ecolor::Hsva;

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
        angle: f32,
        settings: GearSettings,
    },
    Plane {
        point: Vec2,
        outward_normal: Vec2,
        color: Hsva,
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
        &'static ColliderOf,
        &'static BodyTransform,
    ),
    (With<PhysicalGeometry>, Without<MainCamera>),
>;

#[derive(Copy, Clone)]
struct BodyHit {
    entity: Entity,
    body: Entity,
    local_pos: Vec2,
    body_local_pos: Vec2,
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

#[derive(Copy, Clone)]
struct AttachmentSpawnContext<'a> {
    images: &'a AppIcons,
    color: Hsva,
    sky_color: Color,
    camera_scale: f32,
    camera_rotation: Quat,
    z: f32,
    scene: Entity,
    sky: Entity,
}

fn random_color(world: &mut World) -> Hsva {
    let palette = world.resource::<PaletteConfig>().current_palette;
    palette.get_color_hsva(
        &mut *world
            .query::<&mut RngComponent>()
            .single_mut(world)
            .unwrap(),
    )
}

pub(crate) fn spawn_default_box(world: &mut World) -> Entity {
    let color = random_color(world);
    let scene = world.resource::<SceneState>().scene;
    let pos = world.resource_mut::<DepthSorter>().pos(-Vec2::splat(0.5));
    let mut queue = CommandQueue::default();
    let entity =
        PhysicalObject::rect(Vec2::ONE, pos).spawn(&mut Commands::new(&mut queue, world), scene);
    queue.apply(world);
    world
        .entity_mut(entity)
        .insert(ColorComponent(color).update_from_this());
    entity
}

pub(crate) fn spawn_default_circle(world: &mut World) -> Entity {
    let color = random_color(world);
    let scene = world.resource::<SceneState>().scene;
    let pos = world.resource_mut::<DepthSorter>().pos(-Vec2::splat(0.5));
    let mut queue = CommandQueue::default();
    let entity = PhysicalObject::ball(1.0, pos).spawn(&mut Commands::new(&mut queue, world), scene);
    queue.apply(world);
    world
        .entity_mut(entity)
        .insert(ColorComponent(color).update_from_this());
    entity
}

pub(crate) fn spawn_default_plane(world: &mut World, point: Vec2) -> Entity {
    let color = random_color(world);
    let state = world.resource::<SceneState>();
    let (scene, sky) = (state.scene, state.sky);
    let mut queue = CommandQueue::default();
    let entity = spawn_plane(
        &mut Commands::new(&mut queue, world),
        scene,
        sky,
        point,
        Vec2::Y,
        color,
    );
    queue.apply(world);
    entity
}

fn joint_placement(world: &World, geometry: JointGeometry) -> AttachmentPlacement {
    let hit = |entity, local_pos| {
        let rotation = *world.get::<Rotation>(entity).unwrap();
        let link = world.get::<ColliderOf>(entity).unwrap();
        let local = world.get::<BodyTransform>(entity).unwrap();
        BodyHit {
            entity,
            body: link.body,
            local_pos,
            body_local_pos: local.transform_point(local_pos),
            z: world.get::<Transform>(entity).unwrap().translation.z,
            rotation: Quat::from_rotation_z(rotation.as_radians()),
        }
    };
    let (body1, body2, pos) = match geometry.geoms {
        [Some(geom0), geom1] => (
            hit(geom0, geometry.positions[0]),
            geom1.map(|entity| hit(entity, geometry.positions[1])),
            if geom1.is_none() {
                geometry.positions[1]
            } else {
                world_point(
                    (
                        world.get::<Position>(geom0).unwrap().0,
                        *world.get::<Rotation>(geom0).unwrap(),
                    ),
                    geometry.positions[0],
                )
            },
        ),
        [None, Some(geom1)] => (
            hit(geom1, geometry.positions[1]),
            None,
            geometry.positions[0],
        ),
        [None, None] => unreachable!(),
    };
    AttachmentPlacement { body1, body2, pos }
}

fn spawn_joint_visual(
    world: &mut World,
    placement: AttachmentPlacement,
    kind: AttachmentKind,
) -> Entity {
    let images = world.resource::<AppIcons>().clone();
    let palette = world.resource::<PaletteConfig>().current_palette;
    let color = palette.get_color_hsva_opaque(
        &mut *world
            .query::<&mut RngComponent>()
            .single_mut(world)
            .unwrap(),
    );
    let camera = *world
        .query_filtered::<&Transform, With<MainCamera>>()
        .single(world)
        .unwrap();
    let z = world.resource_mut::<DepthSorter>().next();
    let scene = world.resource::<SceneState>();
    let context = AttachmentSpawnContext {
        images: &images,
        color,
        sky_color: palette.sky_color,
        camera_scale: camera.scale.x,
        camera_rotation: camera.rotation,
        z,
        scene: scene.scene,
        sky: scene.sky,
    };
    let mut queue = CommandQueue::default();
    let commands = &mut Commands::new(&mut queue, world);
    let visual = match kind {
        AttachmentKind::Axle => spawn_axle_visual(commands, placement, context),
        AttachmentKind::Fix => spawn_fix_visual(commands, placement, context),
        _ => unreachable!(),
    };
    queue.apply(world);
    visual
}

pub(crate) fn spawn_pending_joint(world: &mut World, kind: AttachmentKind) -> Entity {
    let sky = world.resource::<SceneState>().sky;
    spawn_joint_visual(
        world,
        AttachmentPlacement {
            body1: BodyHit {
                entity: sky,
                body: sky,
                local_pos: Vec2::ZERO,
                body_local_pos: Vec2::ZERO,
                z: 0.0,
                rotation: Quat::IDENTITY,
            },
            body2: None,
            pos: Vec2::ZERO,
        },
        kind,
    )
}

pub(crate) fn configure_joint(world: &mut World, visual: Entity, geometry: JointGeometry) {
    if world.get::<JointGeometry>(visual) == Some(&geometry) {
        return;
    }
    rebuild_joint(world, visual, geometry);
}

fn rebuild_joint(world: &mut World, visual: Entity, geometry: JointGeometry) {
    let placement = joint_placement(world, geometry);
    let kind = *world.get::<AttachmentKind>(visual).unwrap();
    let scene = world.resource::<SceneState>().scene;
    let sky = world.resource::<SceneState>().sky;
    let transform = *world.get::<Transform>(visual).unwrap();
    let (rotation, z) = world.get::<JointGeometry>(visual).map_or(
        (transform.rotation, transform.translation.z),
        |_| {
            let transform = world.get::<GlobalTransform>(visual).unwrap();
            (transform.rotation(), transform.translation().z)
        },
    );
    if let Some(joint) = world
        .get::<AttachmentLinks>(visual)
        .and_then(|links| links.joint)
    {
        world.despawn(joint);
    }
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, world);
    commands.entity(visual).insert((
        attachment_pose(placement, z)
            .with_rotation(placement.body1.rotation.inverse() * rotation)
            .with_scale(transform.scale),
        ChildOf(placement.body1.entity),
        geometry,
    ));
    let links = spawn_joint(&mut commands, visual, placement, scene, sky, kind);
    commands.entity(visual).insert(links);
    queue.apply(world);

    let children = world
        .get::<Children>(visual)
        .map(|children| children.iter().collect::<Vec<_>>())
        .unwrap_or_default();
    let sky_color = world.resource::<PaletteConfig>().current_palette.sky_color;
    for child in children {
        if world.get::<AxleBodyColor>(child).is_some() {
            world
                .entity_mut(child)
                .insert(UpdateFrom::<ColorComponent>::entity(placement.body1.entity));
        } else if world.get::<AttachmentSupportColor>(child).is_some() {
            if let Some(body2) = placement.body2 {
                world
                    .entity_mut(child)
                    .insert(UpdateFrom::<ColorComponent>::entity(body2.entity));
            } else {
                world
                    .entity_mut(child)
                    .remove::<UpdateFrom<ColorComponent>>();
                world.get_mut::<Sprite>(child).unwrap().color = sky_color;
            }
        }
    }
}

pub(crate) fn rebuild_hinges(world: &mut World) {
    let hinges = world
        .query_filtered::<(Entity, &JointGeometry), With<AxleVisual>>()
        .iter(world)
        .map(|(entity, geometry)| (entity, *geometry))
        .collect::<Vec<_>>();
    for (entity, geometry) in hinges {
        rebuild_joint(world, entity, geometry);
    }
}

pub(crate) fn sync_axle_anchors(
    changed_geometries: Query<Entity, Changed<BodyTransform>>,
    geometries: Query<&BodyTransform>,
    visuals: Query<(Ref<JointGeometry>, &AttachmentLinks), With<AxleVisual>>,
    mut joints: Query<&mut RevoluteJoint>,
) {
    let changed = changed_geometries.iter().collect::<Vec<_>>();
    for (geometry, links) in &visuals {
        if !geometry.is_changed()
            && !geometry
                .geoms
                .iter()
                .flatten()
                .any(|entity| changed.contains(entity))
        {
            continue;
        }
        let Some(joint) = links.joint.and_then(|entity| joints.get_mut(entity).ok()) else {
            continue;
        };
        let anchor = |entity, pos| {
            geometries
                .get(entity)
                .ok()
                .map(|local| local.transform_point(pos))
        };
        let anchors = (|| {
            Some(match geometry.geoms {
                [Some(a), Some(b)] => [
                    anchor(a, geometry.positions[0])?,
                    anchor(b, geometry.positions[1])?,
                ],
                [Some(a), None] => [anchor(a, geometry.positions[0])?, geometry.positions[1]],
                [None, Some(b)] => [anchor(b, geometry.positions[1])?, geometry.positions[0]],
                [None, None] => return None,
            })
        })();
        let Some([a, b]) = anchors else {
            continue;
        };
        let mut joint = joint;
        joint.frame1.anchor = JointAnchor::Local(a);
        joint.frame2.anchor = JointAnchor::Local(b);
    }
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
    fixes: Query<(Entity, &JointGeometry), With<FixObject>>,
    body_colors: Query<&ColorComponent>,
) {
    let palette = &palette_config.current_palette;
    let camera = cameras.single().unwrap();
    let camera_scale = camera.scale.x;
    let camera_rotation = camera.rotation;
    let attachment_context = |z, color| AttachmentSpawnContext {
        images: &images,
        color,
        sky_color: palette.sky_color,
        camera_scale,
        camera_rotation,
        z,
        scene: scene_state.scene,
        sky: scene_state.sky,
    };

    for ev in events.read() {
        use AddObjectEvent::*;
        match *ev {
            Box { pos, size } => {
                let entity =
                    PhysicalObject::rect(size, z.pos(pos)).spawn(&mut commands, scene_state.scene);
                commands.entity(entity).insert(
                    ColorComponent(palette.get_color_hsva(&mut *rng.single_mut().unwrap()))
                        .update_from_this(),
                );
            }
            Circle { center, radius } => {
                let entity = PhysicalObject::ball(radius, z.pos(center))
                    .spawn(&mut commands, scene_state.scene);
                commands.entity(entity).insert(
                    ColorComponent(palette.get_color_hsva(&mut *rng.single_mut().unwrap()))
                        .update_from_this(),
                );
            }
            Gear {
                center,
                radius,
                angle,
                settings,
            } => {
                let Some(outline) = GearOutline::from_radius(radius, settings) else {
                    continue;
                };
                let gear_pos = z.pos(center);
                let collision_path = outline.path();
                let Some(object) = PhysicalObject::freeform_path(collision_path, gear_pos, angle)
                else {
                    continue;
                };
                let (body, entity) = object.spawn_with_body(&mut commands, scene_state.scene);
                commands.entity(entity).insert((
                    FreeformObject,
                    ColorComponent(palette.get_color_hsva(&mut *rng.single_mut().unwrap()))
                        .update_from_this(),
                ));
                let placement = AttachmentPlacement {
                    body1: BodyHit {
                        entity,
                        body,
                        local_pos: Vec2::ZERO,
                        body_local_pos: Vec2::ZERO,
                        z: gear_pos.z,
                        rotation: Quat::from_rotation_z(angle),
                    },
                    body2: body_hits_at(center, &query, &spatial_query, None).next(),
                    pos: center,
                };
                spawn_axle_attachment(
                    &mut commands,
                    placement,
                    attachment_context(
                        z.next(),
                        palette.get_color_hsva_opaque(&mut *rng.single_mut().unwrap()),
                    ),
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
                    scene_state.sky,
                    point,
                    outward_normal,
                    color,
                );
            }
            Polygon { pos, ref points } => {
                let Some(object) = PhysicalObject::freeform(points, z.pos(pos)) else {
                    continue;
                };
                let entity = object.spawn(&mut commands, scene_state.scene);
                commands.entity(entity).insert((
                    FreeformObject,
                    ColorComponent(palette.get_color_hsva(&mut *rng.single_mut().unwrap()))
                        .update_from_this(),
                ));
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
                    attachment_context(
                        z.next(),
                        palette.get_color_hsva_opaque(&mut *rng.single_mut().unwrap()),
                    ),
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
                    attachment_context(
                        z.next(),
                        palette.get_color_hsva_opaque(&mut *rng.single_mut().unwrap()),
                    ),
                );
            }
            Laser(pos) => {
                let placement = laser_placement(pos, &query, &spatial_query);
                spawn_laser_attachment(
                    &mut commands,
                    placement,
                    attachment_context(
                        z.next(),
                        palette.get_color_hsva_opaque(&mut *rng.single_mut().unwrap()),
                    ),
                );
            }
            Thruster(pos) => {
                let Some(placement) = single_body_placement(pos, &query, &spatial_query) else {
                    continue;
                };
                spawn_thruster_attachment(
                    &mut commands,
                    placement,
                    attachment_context(z.next(), Hsva::new(0.0, 0.0, 1.0, 1.0)),
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
                    attachment_context(
                        z.next(),
                        palette.get_color_hsva_opaque(&mut *rng.single_mut().unwrap()),
                    ),
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
                    attachment_context(z.next(), Hsva::new(0.0, 0.0, 1.0, 1.0)),
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
                    attachment_context(z.next(), color),
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
    fixes: Query<(Entity, &JointGeometry), With<FixObject>>,
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
            kind @ (AttachmentKind::Fix | AttachmentKind::Axle) => {
                update_attachment_color_sources(
                    event.entity,
                    placement.body1.entity,
                    placement.body2.map(|body| body.entity),
                    palette_config.current_palette.sky_color,
                    &mut commands,
                    &mut attachment_colors,
                );
                commands
                    .entity(event.entity)
                    .insert(joint_geometry(placement));
                spawn_joint(
                    &mut commands,
                    event.entity,
                    placement,
                    scene_state.scene,
                    scene_state.sky,
                    kind,
                )
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
    fixes: &Query<(Entity, &JointGeometry), With<FixObject>>,
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
    fixes: &Query<(Entity, &JointGeometry), With<FixObject>>,
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
    let Ok((_, transform, position, _rotation, _, link, local)) = bodies.get(entity) else {
        info!("Can't find physical object for centered attachment");
        return None;
    };
    Some(AttachmentPlacement {
        body1: BodyHit {
            entity,
            body: link.body,
            local_pos: Vec2::ZERO,
            body_local_pos: local.translation,
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
        if let Ok((entity, transform, position, rotation, _, link, local)) = bodies.get(entity) {
            hits.push(body_hit(
                entity, pos, transform, position, rotation, link, local,
            ));
        }
        true
    });

    for (entity, transform, position, rotation, collider, link, local) in bodies.iter() {
        if Some(entity) == exclude || hits.iter().any(|hit| hit.entity == entity) {
            continue;
        }
        if collider.contains_point(*position, *rotation, pos) {
            hits.push(body_hit(
                entity, pos, transform, position, rotation, link, local,
            ));
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
    link: &ColliderOf,
    local: &BodyTransform,
) -> BodyHit {
    let local_pos = local_point((position.0, *rotation), pos);
    BodyHit {
        entity,
        body: link.body,
        local_pos,
        body_local_pos: local.transform_point(local_pos),
        z: transform.translation_vec3a().z,
        rotation: transform.rotation(),
    }
}

fn duplicate_fix_exists(
    geom1: Entity,
    geom2: Entity,
    current: Option<Entity>,
    fixes: &Query<(Entity, &JointGeometry), With<FixObject>>,
) -> bool {
    fixes.iter().any(|(entity, joint)| {
        Some(entity) != current
            && (joint.geoms == [Some(geom1), Some(geom2)]
                || joint.geoms == [Some(geom2), Some(geom1)])
    })
}

fn attachment_transform(placement: AttachmentPlacement, scale: f32, z: f32) -> Transform {
    attachment_pose(placement, z).with_scale(Vec3::new(scale, scale, 1.0))
}

fn joint_geometry(placement: AttachmentPlacement) -> JointGeometry {
    JointGeometry {
        geoms: [
            Some(placement.body1.entity),
            placement.body2.map(|body| body.entity),
        ],
        positions: [
            placement.body1.local_pos,
            placement.body2.map_or(placement.pos, |body| body.local_pos),
        ],
    }
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
}

fn clear_attachment_links(commands: &mut Commands, links: Option<AttachmentLinks>) {
    despawn_attachment_links(commands, links.as_ref());
}

fn spawn_fix_visual(
    commands: &mut Commands,
    placement: AttachmentPlacement,
    context: AttachmentSpawnContext,
) -> Entity {
    let scale = context.camera_scale * DEFAULT_OBJ_SIZE;
    commands
        .spawn((
            ShapeBundle::new(
                GeometryBuilder::build_as(&shapes::Circle {
                    radius: 0.5 * 1.1,
                    ..Default::default()
                }),
                screen_aligned_attachment_transform(
                    placement,
                    scale,
                    context.z,
                    context.camera_rotation,
                ),
                Visibility::Inherited,
            ),
            crate::make_stroke(Color::srgba(0.0, 0.0, 0.0, 0.0), BORDER_WIDTH_PX),
            SpriteOnly,
            Collider::circle(0.5),
            VIRTUAL_LAYER_OBJ,
            Sensor,
            AttachmentKind::Fix,
            FixObject,
            ColorComponent(context.color).update_from_this(),
            ChildOf(placement.body1.entity),
        ))
        .with_children(|builder| {
            builder.spawn((
                Sprite {
                    image: context.images.fixjoint_outer.clone(),
                    ..Default::default()
                },
                Transform::from_scale(Vec3::new(1.0 / 256.0, 1.0 / 256.0, 1.0)),
                UpdateFrom::<ColorComponent>::This,
            ));
            let mut inner = builder.spawn((
                AttachmentSupportColor,
                Sprite {
                    image: context.images.fixjoint_inner.clone(),
                    color: context.sky_color,
                    ..Default::default()
                },
                Transform::from_scale(Vec3::new(1.0 / 256.0, 1.0 / 256.0, 1.0)),
            ));
            if let Some(body2) = placement.body2 {
                inner.insert(UpdateFrom::<ColorComponent>::entity(body2.entity));
            }
        })
        .id()
}

fn spawn_fix_attachment(
    commands: &mut Commands,
    placement: AttachmentPlacement,
    context: AttachmentSpawnContext,
) -> Entity {
    let visual = spawn_fix_visual(commands, placement, context);
    commands
        .entity(visual)
        .insert((joint_geometry(placement), AttachmentLinks::default()));
    visual
}

fn spawn_axle_visual(
    commands: &mut Commands,
    placement: AttachmentPlacement,
    context: AttachmentSpawnContext,
) -> Entity {
    let scale = context.camera_scale * DEFAULT_OBJ_SIZE;
    const IMAGE_SCALE: f32 = 1.0 / 256.0;
    const IMAGE_SCALE_VEC: Vec3 = Vec3::new(IMAGE_SCALE, IMAGE_SCALE, 1.0);

    commands
        .spawn((
            ShapeBundle::new(
                GeometryBuilder::build_as(&shapes::Circle {
                    radius: hinge_selection_radius(false),
                    ..Default::default()
                }),
                screen_aligned_attachment_transform(
                    placement,
                    scale,
                    context.z,
                    context.camera_rotation,
                ),
                Visibility::Inherited,
            ),
            crate::make_stroke(Color::srgba(0.0, 0.0, 0.0, 0.0), BORDER_WIDTH_PX),
            SpriteOnly,
            Collider::circle(0.5),
            VIRTUAL_LAYER_OBJ,
            Sensor,
            AttachmentKind::Axle,
            AxleVisual,
            ColorComponent(context.color).update_from_this(),
            MotorComponent::default(),
            ChildOf(placement.body1.entity),
        ))
        .with_children(|builder| {
            builder.spawn((
                HingeMotorRing,
                Sprite {
                    image: context.images.hinge_motor.clone(),
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
                    image: context.images.hinge_motor_ccw.clone(),
                    custom_size: Some(Vec2::splat(HINGE_MOTOR_VISUAL_DIAMETER)),
                    ..Default::default()
                },
                Transform::from_translation(Vec3::Z * -0.02),
                Visibility::Hidden,
            ));
            builder.spawn((
                HingeMotorDirection { reversed: false },
                Sprite {
                    image: context.images.hinge_motor_cw.clone(),
                    custom_size: Some(Vec2::splat(HINGE_MOTOR_VISUAL_DIAMETER)),
                    ..Default::default()
                },
                Transform::from_translation(Vec3::Z * -0.02),
                Visibility::Hidden,
            ));
            builder.spawn((
                AxleBodyColor,
                Sprite {
                    image: context.images.hinge_balls.clone(),
                    ..Default::default()
                },
                Transform::from_scale(IMAGE_SCALE_VEC),
                UpdateFrom::<ColorComponent>::entity(placement.body1.entity),
            ));
            builder.spawn((
                Sprite {
                    image: context.images.hinge_background.clone(),
                    ..Default::default()
                },
                Transform::from_scale(IMAGE_SCALE_VEC),
                UpdateFrom::<ColorComponent>::This,
            ));
            let mut inner = builder.spawn((
                AttachmentSupportColor,
                Sprite {
                    image: context.images.hinge_inner.clone(),
                    color: context.sky_color,
                    ..Default::default()
                },
                Transform::from_scale(IMAGE_SCALE_VEC),
            ));
            if let Some(body2) = placement.body2 {
                inner.insert(UpdateFrom::<ColorComponent>::entity(body2.entity));
            }
        })
        .id()
}

fn spawn_axle_attachment(
    commands: &mut Commands,
    placement: AttachmentPlacement,
    context: AttachmentSpawnContext,
) -> Entity {
    let visual = spawn_axle_visual(commands, placement, context);
    let links = spawn_axle_joint(commands, visual, placement, context.scene, context.sky);
    commands
        .entity(visual)
        .insert((joint_geometry(placement), links));
    visual
}

fn spawn_laser_attachment(
    commands: &mut Commands,
    placement: LaserPlacement,
    context: AttachmentSpawnContext,
) -> Entity {
    let scale = context.camera_scale * DEFAULT_OBJ_SIZE;
    let (transform, parent) = match placement {
        LaserPlacement::Body(placement) => (
            screen_aligned_attachment_pose(placement, context.z, context.camera_rotation),
            placement.body1.entity,
        ),
        LaserPlacement::Sky { pos } => (
            screen_aligned_sky_attachment_pose(pos, context.z, context.camera_rotation),
            context.scene,
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
            ColorComponent(context.color).update_from_this(),
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
                image: context.images.laserpen.clone(),
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
    context: AttachmentSpawnContext,
    align_with_body: bool,
) -> Entity {
    let scale = context.camera_scale * DEFAULT_OBJ_SIZE * 2.0;
    let sprite_scale = Vec3::new(scale / 256.0, scale / 256.0, 1.0);
    commands
        .spawn((
            if align_with_body {
                attachment_pose(placement, context.z)
            } else {
                screen_aligned_attachment_pose(placement, context.z, context.camera_rotation)
            },
            Visibility::Inherited,
            ThrusterSettings::default(),
            ColorComponent(context.color).update_from_this(),
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
                    image: context.images.thruster_inner.clone(),
                    ..Default::default()
                },
                Transform::from_scale(sprite_scale),
                UpdateFrom::<ColorComponent>::entity(placement.body1.entity),
            ));
            builder.spawn((
                Sprite {
                    image: context.images.thruster_thrust.clone(),
                    ..Default::default()
                },
                Transform::from_translation(Vec3::Z * 0.01).with_scale(sprite_scale),
            ));
            builder.spawn((
                Sprite {
                    image: context.images.thruster_outer.clone(),
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
    context: AttachmentSpawnContext,
    center_on_body: bool,
) -> Entity {
    let scale = context.camera_scale * DEFAULT_OBJ_SIZE;
    commands
        .spawn((
            ShapeBundle::new(
                GeometryBuilder::build_as(&shapes::Circle {
                    radius: scale * 0.55,
                    ..Default::default()
                }),
                if center_on_body {
                    attachment_pose(placement, context.z)
                } else {
                    screen_aligned_attachment_pose(placement, context.z, context.camera_rotation)
                },
                Visibility::Inherited,
            ),
            crate::make_stroke(Color::srgba(0.0, 0.0, 0.0, 0.0), BORDER_WIDTH_PX),
            TracerObject::default(),
            TracerSettings {
                diameter: scale,
                ..Default::default()
            },
            ColorComponent(context.color).update_from_this(),
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
                image: context.images.tracer.clone(),
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
    sky: Entity,
) -> AttachmentLinks {
    let (body2, anchor2) = body2_or_sky(placement, sky);
    if placement.body1.body == body2 {
        return AttachmentLinks::default();
    }
    let joint = RevoluteJoint::new(placement.body1.body, body2)
        .with_local_anchor1(placement.body1.body_local_pos)
        .with_local_anchor2(anchor2);
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

    AttachmentLinks { joint: Some(joint) }
}

fn spawn_joint(
    commands: &mut Commands,
    visual: Entity,
    placement: AttachmentPlacement,
    scene: Entity,
    sky: Entity,
    kind: AttachmentKind,
) -> AttachmentLinks {
    match kind {
        AttachmentKind::Axle => spawn_axle_joint(commands, visual, placement, scene, sky),
        AttachmentKind::Fix => AttachmentLinks::default(),
        _ => unreachable!(),
    }
}

fn body2_or_sky(placement: AttachmentPlacement, sky: Entity) -> (Entity, Vec2) {
    if let Some(body2) = placement.body2 {
        return (body2.body, body2.body_local_pos);
    }
    (sky, placement.pos)
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
    use bevy::ecs::system::RunSystemOnce;

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

    #[test]
    fn changed_geometry_pose_updates_only_the_axle_frames() {
        let mut world = World::new();
        let bodies = [world.spawn_empty().id(), world.spawn_empty().id()];
        let a = world
            .spawn(BodyTransform(ColliderTransform {
                translation: Vec2::X,
                rotation: Rotation::default(),
                scale: Vec2::ONE,
            }))
            .id();
        let b = world
            .spawn(BodyTransform(ColliderTransform {
                translation: Vec2::Y,
                rotation: Rotation::default(),
                scale: Vec2::ONE,
            }))
            .id();
        let joint = world.spawn(RevoluteJoint::new(bodies[0], bodies[1])).id();
        world.spawn((
            AxleVisual,
            JointGeometry {
                geoms: [Some(a), Some(b)],
                positions: [Vec2::X, Vec2::Y],
            },
            AttachmentLinks { joint: Some(joint) },
        ));

        world.run_system_once(sync_axle_anchors).unwrap();
        assert_eq!(
            world.get::<RevoluteJoint>(joint).unwrap().local_anchor1(),
            Some(Vec2::X * 2.0)
        );
        assert_eq!(
            world.get::<RevoluteJoint>(joint).unwrap().local_anchor2(),
            Some(Vec2::Y * 2.0)
        );

        world.clear_trackers();
        world.get_mut::<BodyTransform>(a).unwrap().translation = Vec2::splat(3.0);
        world.run_system_once(sync_axle_anchors).unwrap();
        assert_eq!(
            world.get::<RevoluteJoint>(joint).unwrap().local_anchor1(),
            Some(Vec2::new(4.0, 3.0))
        );
    }
}
