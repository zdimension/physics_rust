use crate::mouse::select;
use crate::mouse::select::SelectUnderMouseEvent;
use crate::objects::hinge::HingeObject;
use crate::objects::laser::LaserBundle;
use crate::objects::phy_obj::PhysicalObject;
use crate::objects::{ColorComponent, MotorComponent, SettingComponent, SizeComponent, SpriteOnly};
use crate::palette::PaletteConfig;
use crate::ui::images::AppIcons;
use crate::ui::windows::object::collisions::CollisionLayer;
use crate::ui::UiState;
use crate::update_from::UpdateFrom;
use crate::{InvTransformPoint, BORDER_THICKNESS};
use avian2d::{math::*, prelude::*};
use bevy::log::info;
use bevy::math::{Vec2, Vec3, Vec3Swizzles};
use bevy::prelude::*;
use crate::mouse_tracking::MainCamera;
use crate::lyon_compat::GeometryBuilder;
use crate::lyon_compat::ShapeBundle;
use crate::lyon_compat::shapes;
use crate::rng::RngComponent;

const VIRTUAL_LAYER: u32 = 1 << 31;

static VIRTUAL_LAYER_OBJ: CollisionLayers =
    CollisionLayers::from_bits(VIRTUAL_LAYER, VIRTUAL_LAYER);

pub fn query_only_real() -> SpatialQueryFilter {
    SpatialQueryFilter::from_mask(0xffff_ffff ^ VIRTUAL_LAYER)
}

#[derive(Debug, Message)]
pub enum AddHingeEvent {
    Mouse(Vec2),
    AddCenter(Entity),
}

#[derive(Debug, Message)]
pub enum AddObjectEvent {
    Hinge(AddHingeEvent),
    Fix(Vec2),
    Circle { center: Vec2, radius: f32 },
    Box { pos: Vec2, size: Vec2 },
    Laser(Vec2),
    Polygon { pos: Vec2, points: Vec<Vec2> },
}

const DEFAULT_OBJ_SIZE: f32 = 66.0;

pub fn process_add_object(
    mut events: MessageReader<AddObjectEvent>,
    query: Query<(&GlobalTransform, &RigidBody), Without<MainCamera>>,
    images: Res<AppIcons>,
    mut commands: Commands,
    mut cameras: Query<&mut Transform, With<MainCamera>>,
    palette_config: Res<PaletteConfig>,
    mut z: Local<DepthSorter>,
    mut rng: Query<&mut RngComponent>,
    mut select_mouse: MessageWriter<SelectUnderMouseEvent>,
    sensor: Query<&Sensor>,
    ui_state: Res<UiState>,
    spatial_query: SpatialQuery,
) {
    let palette = &palette_config.current_palette;

    for ev in events.read() {
        use AddObjectEvent::*;
        match *ev {
            Box { pos, size } => {
                commands
                    .spawn(PhysicalObject::rect(size, z.pos(pos)))
                    .insert(ChildOf(ui_state.scene))
                    .insert(
                        ColorComponent(palette.get_color_hsva(&mut *rng.single_mut().unwrap()))
                            .update_from_this(),
                    )
                    .log_components();
            }
            Circle { center, radius } => {
                commands
                    .spawn(PhysicalObject::ball(radius, z.pos(center)))
                    .insert(ChildOf(ui_state.scene))
                    .insert(
                        ColorComponent(palette.get_color_hsva(&mut *rng.single_mut().unwrap()))
                            .update_from_this(),
                    )
                    .log_components();
            }
            Polygon { pos, ref points } => {
                commands
                    .spawn(PhysicalObject::poly(points.clone(), z.pos(pos)))
                    .insert(ChildOf(ui_state.scene))
                    .insert(
                        ColorComponent(palette.get_color_hsva(&mut *rng.single_mut().unwrap()))
                            .update_from_this(),
                    )
                    .log_components();
            }
            Fix(pos) => {
                let (entity1, entity2) = {
                    let mut entities =
                        select::find_under_mouse(&spatial_query, pos, query_only_real(), |ent| {
                            let (transform, _) = query.get(ent).unwrap();
                            transform.translation_vec3a().z
                        });
                    (entities.next(), entities.next())
                };

                if let Some(entity1) = entity1 {
                    if sensor.get(entity1).is_ok() {
                        select_mouse.write(SelectUnderMouseEvent {
                            pos,
                            open_menu: false,
                        });
                        return;
                    }

                    let (transform, _) = query.get(entity1).unwrap();
                    let anchor1 = transform
                        .affine()
                        .inverse()
                        .transform_point3(pos.extend(0.0))
                        .xy();

                    /*if let Some(entity2) = entity2 {
                        let (transform, _) = query.get(entity2).unwrap();
                        let anchor2 = transform
                            .compute_affine()
                            .inverse()
                            .transform_point3(pos.extend(0.0))
                            .xy();
                        commands.entity(entity2).insert(MultibodyJoint::new(
                            entity1,
                            FixedJointBuilder::new()
                                .local_anchor1(anchor1)
                                .local_anchor2(anchor2),
                        ));
                    } else {
                        commands
                            .spawn((
                                ImpulseJoint::new(
                                    entity1,
                                    FixedJointBuilder::new()
                                        .local_anchor1(anchor1)
                                        .local_anchor2(pos),
                                ),
                                RigidBody::Dynamic,
                            ))
                            .insert(ChildOf(ui_state.scene));
                    }*/
                }
            }
            Hinge(ref ev) => {
                let (entity1, anchor1, entity1z, entity2, pos) = match *ev {
                    AddHingeEvent::Mouse(pos) => {
                        let mut entities = select::find_under_mouse(
                            &spatial_query,
                            pos,
                            query_only_real(),
                            |ent| {
                                let (transform, _) = query.get(ent).unwrap();
                                transform.translation_vec3a().z
                            },
                        );
                        let (entity1, entity2) = (entities.next(), entities.next());
                        let Some(entity1) = entity1 else {
                            info!("Add hinge: no entity under mouse");
                            return;
                        };
                        if sensor.get(entity1).is_ok() {
                            info!("Add hinge on sensor; selecting");
                            select_mouse.write(SelectUnderMouseEvent {
                                pos,
                                open_menu: false,
                            });
                            return;
                        }
                        let Ok((transform, _)) = query.get(entity1) else {
                            info!("Can't find transform for entity under mouse");
                            commands.entity(entity1).log_components();
                            continue;
                        };
                        let anchor1 = transform.to_local(pos);
                        (
                            entity1,
                            anchor1,
                            transform.translation_vec3a().z,
                            entity2,
                            pos,
                        )
                    }
                    AddHingeEvent::AddCenter(ent) => {
                        let entity1 = ent;
                        let anchor1 = Vec2::ZERO;
                        let Ok((transform, _)) = query.get(entity1) else {
                            info!("Can't find transform for entity (add center axle)");
                            commands.entity(entity1).log_components();
                            continue;
                        };
                        let pos = transform.translation_vec3a().xy();
                        let entity2 = select::find_under_mouse(
                            &spatial_query,
                            pos,
                            query_only_real().with_excluded_entities([entity1]),
                            |ent| {
                                let (transform, _) = query.get(ent).unwrap();
                                transform.translation_vec3a().z
                            },
                        )
                        .next();
                        (
                            entity1,
                            anchor1,
                            transform.translation_vec3a().z,
                            entity2,
                            pos,
                        )
                    }
                };

                {
                    let hinge_z = z.next();
                    let hinge_delta = hinge_z - entity1z;
                    let hinge_pos = anchor1.extend(hinge_delta);
                    const HINGE_RADIUS: f32 = DEFAULT_OBJ_SIZE / 2.0;
                    let scale = cameras.single_mut().unwrap().scale.x * DEFAULT_OBJ_SIZE;
                    const IMAGE_SCALE: f32 = 1.0 / 256.0;
                    const IMAGE_SCALE_VEC: Vec3 = Vec3::new(IMAGE_SCALE, IMAGE_SCALE, 1.0);
                    // group the three sprites in an entity containing the transform
                    let hinge_real_ent = commands
                        .spawn((
                            ShapeBundle::new(
                                GeometryBuilder::build_as(&shapes::Circle {
                                    radius: 0.5 * 1.1, // make selection display a bit bigger
                                    ..Default::default()
                                }),
                                Transform::from_translation(hinge_pos)
                                    .with_scale(Vec3::new(scale, scale, 1.0)),
                                Visibility::Inherited,
                            ),
                            crate::make_stroke(Color::srgba(0.0, 0.0, 0.0, 0.0), BORDER_THICKNESS),
                            SpriteOnly,
                            Collider::circle(0.5),
                            Sensor,
                            ColorComponent(palette.get_color_hsva_opaque(&mut *rng.single_mut().unwrap()))
                                .update_from_this(),
                            MotorComponent::default(),
                        ))
                        .insert(ChildOf(entity1))
                        .with_children(|builder| {
                            builder
                                .spawn(Transform::from_scale(IMAGE_SCALE_VEC))
                                .with_children(|builder| {
                                    builder.spawn((
                                        Sprite {
                                            image: images.hinge_balls.clone(),
                                            ..Default::default()
                                        },
                                        UpdateFrom::<ColorComponent>::entity(entity1),
                                    ));
                                })
                                .with_children(|builder| {
                                    builder.spawn((
                                        Sprite {
                                            image: images.hinge_background.clone(),
                                            ..Default::default()
                                        },
                                        UpdateFrom::<ColorComponent>::This,
                                    ));
                                })
                                .with_children(|builder| {
                                    let mut sprite = builder.spawn(Sprite {
                                        image: images.hinge_inner.clone(),
                                        color: palette.sky_color,
                                        ..Default::default()
                                    });
                                    if let Some(entity2) = entity2 {
                                        sprite
                                            .insert(UpdateFrom::<ColorComponent>::entity(entity2));
                                    }
                                });
                        })
                        .id();
                    if let Some(entity2) = entity2 {
                        /*let (transform, _) = query.get(entity2).unwrap();
                        let anchor2 = transform
                            .compute_affine()
                            .inverse()
                            .transform_point3(pos.extend(0.0))
                            .xy();
                        info!(
                            "hinge: {:?} {:?} {:?} {:?}",
                            entity1, anchor1, entity2, anchor2
                        );
                        commands.entity(entity2).insert((
                            HingeObject,
                            UpdateFrom::<MotorComponent>::entity(hinge_real_ent),
                            MultibodyJoint::new(
                                entity1,
                                RevoluteJointBuilder::new()
                                    .local_anchor1(anchor1)
                                    .local_anchor2(anchor2),
                            ),
                            ActiveHooks::FILTER_CONTACT_PAIRS,
                        ));*/
                        let (transform, _) = query.get(entity2).unwrap();
                        let anchor2 = transform.to_local(pos);
                        info!(
                            "hinge: {:?} {:?} {:?} {:?}",
                            entity1, anchor1, entity2, anchor2
                        );
                        commands
                            .spawn((
                                HingeObject,
                                JointCollisionDisabled,
                                UpdateFrom::<MotorComponent>::entity(hinge_real_ent),
                                RevoluteJoint::new(entity1, entity2)
                                    .with_local_anchor1(anchor1)
                                    .with_local_anchor2(anchor2),
                            ))
                            .insert(ChildOf(ui_state.scene));
                    } else {
                        let rigid = commands.spawn((RigidBody::Kinematic, Position(pos))).id();
                        commands
                            .spawn((
                                HingeObject,
                                JointCollisionDisabled,
                                UpdateFrom::<MotorComponent>::entity(hinge_real_ent),
                                RevoluteJoint::new(entity1, rigid).with_local_anchor1(anchor1),
                            ))
                            .insert(ChildOf(ui_state.scene));
                    }
                }
            }
            Laser(pos) => {
                let entity =
                    select::find_under_mouse(&spatial_query, pos, query_only_real(), |ent| {
                        query.get(ent).unwrap().0.translation_vec3a().z
                    })
                    .next();

                let scale = cameras.single_mut().unwrap().scale.x * DEFAULT_OBJ_SIZE;
                let laser = commands
                    .spawn((
                        LaserBundle {
                            fade_distance: 10.0,
                        },
                        ColorComponent(palette.get_color_hsva_opaque(&mut *rng.single_mut().unwrap()))
                            .update_from_this(),
                        Collider::rectangle(0.5, 0.25),
                        VIRTUAL_LAYER_OBJ,
                        SizeComponent(scale),
                        Sensor,
                    ))
                    .insert(ChildOf(ui_state.scene))
                    .id();

                let laser_pos = if let Some(entity) = entity {
                    commands.entity(entity).add_child(laser);
                    pos - query.get(entity).unwrap().0.translation_vec3a().xy()
                } else {
                    pos
                };
                commands
                    .entity(laser)
                    .insert((
                        ShapeBundle::new(
                            GeometryBuilder::build_as(&shapes::Rectangle {
                                extents: Vec2::new(1.0, 0.5) * 1.1, // make selection display a bit bigger
                                ..Default::default()
                            }),
                            Transform::from_translation(z.pos(laser_pos)),
                            Visibility::Inherited,
                        ),
                        crate::make_stroke(Color::srgba(0.0, 0.0, 0.0, 0.0), BORDER_THICKNESS),
                        UpdateFrom::<SizeComponent>::This,
                    ))
                    .with_child((
                            Sprite {
                                image: images.laserpen.clone(),
                                ..Default::default()
                            },
                            Transform::from_scale(Vec3::new(1.0 / 256.0, 1.0 / 256.0, 1.0)),
                            UpdateFrom::<ColorComponent>::This,
                        ));
            }
            ref x => unimplemented!("unimplemented tool {:?}", x),
        }
    }
}

#[derive(Default)]
pub struct DepthSorter {
    current_depth: f32,
}

impl DepthSorter {
    fn next(&mut self) -> f32 {
        self.current_depth += 1.0;
        self.current_depth
    }

    fn pos(&mut self, pos: Vec2) -> Vec3 {
        pos.extend(self.next())
    }
}
