use crate::mouse::select;
use crate::mouse::select::SelectUnderMouseEvent;
use crate::objects::hinge::HingeObject;
use crate::objects::laser::LaserBundle;
use crate::objects::phy_obj::PhysicalObject;
use crate::objects::{ColorComponent, MotorComponent, SettingComponent, SizeComponent, SpriteOnly};
use crate::palette::PaletteConfig;
use crate::ui::images::AppIcons;
use crate::update_from::UpdateFrom;
use crate::{BORDER_THICKNESS};
use bevy::hierarchy::BuildChildren;
use bevy::log::info;
use bevy::math::{Vec2, Vec3, Vec3Swizzles};
use bevy::prelude::{Color, Entity, Event, SpatialBundle, Sprite, SpriteBundle};
use bevy::prelude::{
    Commands, EventReader, EventWriter, Local, Query, Res, Transform, With, Without,
};
use bevy_mouse_tracking_plugin::MainCamera;
use bevy_prototype_lyon::geometry::GeometryBuilder;
use bevy_prototype_lyon::prelude::ShapeBundle;
use bevy_prototype_lyon::shapes;
use bevy_rapier2d::dynamics::RigidBody;
use bevy_rapier2d::dynamics::{
    FixedJointBuilder, ImpulseJoint, MultibodyJoint, RevoluteJointBuilder,
};
use bevy_rapier2d::geometry::Sensor;
use bevy_rapier2d::geometry::{ActiveHooks, Collider};
use bevy_rapier2d::pipeline::QueryFilter;
use bevy_rapier2d::plugin::RapierContext;
use bevy_turborand::RngComponent;
use AddObjectEvent::*;
use crate::ui::UiState;

#[derive(Debug, Event)]
pub enum AddHingeEvent {
    Mouse(Vec2),
    AddCenter(Entity),
}

#[derive(Debug, Event)]
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
    mut events: EventReader<AddObjectEvent>,
    rapier: Res<RapierContext>,
    mut query: Query<(&mut Transform, &mut RigidBody), Without<MainCamera>>,
    images: Res<AppIcons>,
    mut commands: Commands,
    mut cameras: Query<&mut Transform, With<MainCamera>>,
    palette_config: Res<PaletteConfig>,
    mut z: Local<DepthSorter>,
    mut rng: Query<&mut RngComponent>,
    mut select_mouse: EventWriter<SelectUnderMouseEvent>,
    sensor: Query<&Sensor>,
    ui_state: Res<UiState>
) {
    let palette = &palette_config.current_palette;

    for ev in events.iter() {
        match *ev {
            Box { pos, size } => {
                commands
                    .spawn(PhysicalObject::rect(size, z.pos(pos)))
                    .set_parent(ui_state.scene)
                    .insert(
                        ColorComponent(palette.get_color_hsva(&mut *rng.single_mut()))
                            .update_from_this(),
                    )
                    .log_components();
            }
            Circle { center, radius } => {
                commands
                    .spawn(PhysicalObject::ball(radius, z.pos(center)))
                    .set_parent(ui_state.scene)
                    .insert(
                        ColorComponent(palette.get_color_hsva(&mut *rng.single_mut()))
                            .update_from_this(),
                    )
                    .log_components();
            }
            Polygon { pos, ref points } => {
                commands
                    .spawn(PhysicalObject::poly(points.clone(), z.pos(pos))).set_parent(ui_state.scene)
                    .insert(
                        ColorComponent(palette.get_color_hsva(&mut *rng.single_mut()))
                            .update_from_this(),
                    )
                    .log_components();
            }
            Fix(pos) => {
                let (entity1, entity2) = {
                    let mut entities = select::find_under_mouse(
                        &rapier,
                        pos,
                        QueryFilter::only_dynamic(),
                        |ent| {
                            let (transform, _) = query.get(ent).unwrap();
                            transform.translation.z
                        },
                    );
                    (entities.next(), entities.next())
                };

                if let Some(entity1) = entity1 {
                    if sensor.get(entity1).is_ok() {
                        select_mouse.send(SelectUnderMouseEvent {
                            pos,
                            open_menu: false,
                        });
                        return;
                    }

                    let (transform, _) = query.get_mut(entity1).unwrap();
                    let anchor1 = transform
                        .compute_affine()
                        .inverse()
                        .transform_point3(pos.extend(0.0))
                        .xy();

                    if let Some(entity2) = entity2 {
                        let (transform, _) = query.get_mut(entity2).unwrap();
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
                            .set_parent(ui_state.scene);
                    }
                }
            }
            Hinge(ref ev) => {
                let (entity1, anchor1, entity1z, entity2, pos) = match *ev {
                    AddHingeEvent::Mouse(pos) => {
                        let mut entities = select::find_under_mouse(
                            &rapier,
                            pos,
                            QueryFilter::only_dynamic(),
                            |ent| {
                                let (transform, _) = query.get(ent).unwrap();
                                transform.translation.z
                            },
                        );
                        let (entity1, entity2) = (entities.next(), entities.next());
                        let Some(entity1) = entity1 else {
                            info!("Add hinge: no entity under mouse");
                            return;
                        };
                        if sensor.get(entity1).is_ok() {
                            info!("Add hinge on sensor; selecting");
                            select_mouse.send(SelectUnderMouseEvent {
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
                        let anchor1 = transform
                            .compute_affine()
                            .inverse()
                            .transform_point3(pos.extend(0.0))
                            .xy();
                        (entity1, anchor1, transform.translation.z, entity2, pos)
                    },
                    AddHingeEvent::AddCenter(ent) => {
                        let entity1 = ent;
                        let anchor1 = Vec2::ZERO;
                        let Ok((transform, _)) = query.get(entity1) else {
                            info!("Can't find transform for entity (add center axle)");
                            commands.entity(entity1).log_components();
                            continue;
                        };
                        let pos = transform.translation.xy();
                        let entity2 = select::find_under_mouse(
                            &rapier,
                            pos,
                            QueryFilter::only_dynamic().exclude_collider(entity1),
                            |ent| {
                                let (transform, _) = query.get(ent).unwrap();
                                transform.translation.z
                            },
                        ).next();
                        (entity1, anchor1, transform.translation.z, entity2, pos)
                    }
                };

                {
                    let hinge_z = z.next();
                    let hinge_delta = hinge_z - entity1z;
                    let hinge_pos = anchor1.extend(hinge_delta);
                    const HINGE_RADIUS: f32 = DEFAULT_OBJ_SIZE / 2.0;
                    let scale = cameras.single_mut().scale.x * DEFAULT_OBJ_SIZE;
                    const IMAGE_SCALE: f32 = 1.0 / 256.0;
                    const IMAGE_SCALE_VEC: Vec3 = Vec3::new(IMAGE_SCALE, IMAGE_SCALE, 1.0);
                    // group the three sprites in an entity containing the transform
                    let hinge_real_ent = commands
                        .spawn((
                            ShapeBundle {
                                path: GeometryBuilder::build_as(
                                    &shapes::Circle {
                                        radius: 0.5 * 1.1, // make selection display a bit bigger
                                        ..Default::default()
                                    }),
                                transform: Transform::from_translation(hinge_pos)
                                    .with_scale(Vec3::new(scale, scale, 1.0)),
                                ..Default::default()
                            },
                            crate::make_stroke(
                                Color::rgba(0.0, 0.0, 0.0, 0.0),
                                BORDER_THICKNESS,
                            ),
                            SpriteOnly,
                            Collider::ball(0.5),
                            Sensor,
                            ColorComponent(
                                palette.get_color_hsva_opaque(&mut *rng.single_mut()),
                            )
                                .update_from_this(),
                            MotorComponent::default()
                        ))
                        .set_parent(entity1)
                        .with_children(|builder| {
                            builder
                                .spawn(SpatialBundle::from_transform(Transform::from_scale(
                                    IMAGE_SCALE_VEC,
                                )))
                                .with_children(|builder| {
                                    builder.spawn((
                                        SpriteBundle {
                                            texture: images.hinge_balls.clone(),
                                            sprite: Sprite {
                                                ..Default::default()
                                            },
                                            ..Default::default()
                                        },
                                        UpdateFrom::<ColorComponent>::entity(entity1),
                                    ));
                                })
                                .with_children(|builder| {
                                    builder.spawn((
                                        SpriteBundle {
                                            texture: images.hinge_background.clone(),
                                            sprite: Sprite {
                                                ..Default::default()
                                            },
                                            ..Default::default()
                                        },
                                        UpdateFrom::<ColorComponent>::This,
                                    ));
                                })
                                .with_children(|builder| {
                                    let mut sprite = builder.spawn(SpriteBundle {
                                        texture: images.hinge_inner.clone(),
                                        sprite: Sprite {
                                            color: palette.sky_color,
                                            ..Default::default()
                                        },
                                        ..Default::default()
                                    });
                                    if let Some(entity2) = entity2 {
                                        sprite.insert(UpdateFrom::<ColorComponent>::entity(
                                            entity2,
                                        ));
                                    }
                                });
                        })
                        .id();
                    if let Some(entity2) = entity2 {
                        let (transform, _) = query.get_mut(entity2).unwrap();
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
                        ));
                    } else {
                        commands
                            .spawn((
                                HingeObject,
                                UpdateFrom::<MotorComponent>::entity(hinge_real_ent),
                                ImpulseJoint::new(
                                    entity1,
                                    RevoluteJointBuilder::new()
                                        .local_anchor1(anchor1)
                                        .local_anchor2(pos),
                                ),
                                RigidBody::Dynamic,
                            )).set_parent(ui_state.scene);
                    }
                }
            }
            Laser(pos) => {
                let entity =
                    select::find_under_mouse(&rapier, pos, QueryFilter::only_dynamic(), |ent| {
                        query.get(ent).unwrap().0.translation.z
                    })
                    .next();

                let scale = cameras.single_mut().scale.x * DEFAULT_OBJ_SIZE;
                let laser = commands
                    .spawn((
                        LaserBundle {
                            fade_distance: 10.0,
                        },
                        ColorComponent(palette.get_color_hsva_opaque(&mut *rng.single_mut()))
                            .update_from_this(),
                        Collider::cuboid(0.5, 0.25),
                        SizeComponent(scale),
                        Sensor,
                    ))
                    .set_parent(ui_state.scene)
                    .id();

                let laser_pos = if let Some(entity) = entity {
                    commands.entity(entity).add_child(laser);
                    pos - query.get(entity).unwrap().0.translation.xy()
                } else {
                    pos
                };
                commands
                    .entity(laser)
                    .insert((
                        ShapeBundle {
                            path: GeometryBuilder::build_as(
                                &shapes::Rectangle {
                                    extents: Vec2::new(1.0, 0.5) * 1.1, // make selection display a bit bigger
                                    ..Default::default()
                                }),
                            transform: Transform::from_translation(z.pos(laser_pos)),
                            ..Default::default()
                        },
                        crate::make_stroke(Color::rgba(0.0, 0.0, 0.0, 0.0), BORDER_THICKNESS),
                        UpdateFrom::<SizeComponent>::This,
                    ))
                    .with_children(|builder| {
                        builder.spawn((
                            SpriteBundle {
                                texture: images.laserpen.clone(),
                                transform: Transform::from_scale(Vec3::new(
                                    1.0 / 256.0,
                                    1.0 / 256.0,
                                    1.0,
                                )),
                                ..Default::default()
                            },
                            UpdateFrom::<ColorComponent>::This,
                        ));
                    });
            }
            ref x @ _ => unimplemented!("unimplemented tool {:?}", x),
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
