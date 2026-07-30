use crate::mouse::select;
use crate::mouse::select::{SelectEvent, SelectionMode};
use crate::mouse_tracking::{MainCamera, MousePosWorld};
use crate::objects::ColorComponent;
use crate::objects::spring::{self, SpringEnd, SpringObject, SpringPlacementState};
use crate::palette::PaletteConfig;
use crate::rng::RngComponent;
use crate::tools::ToolEnum;
use crate::tools::add_object::{AttachmentJoint, DepthSorter};
use crate::tools::drag::{DragObject, DragState, DragTarget};
use crate::tools::r#move::MoveState;
use crate::tools::pan::PanState;
use crate::tools::rotate::RotateState;
use crate::tools::zoom::ZoomState;
use crate::ui::images::AppIcons;
use crate::ui::{PointerToolState, SceneState, Selected};
use crate::{CustomForce, InvTransformPoint, UsedMouseButton};
use avian2d::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy::math::{EulerRot, Vec2, Vec3Swizzles};
use bevy::prelude::{
    ChildOf, Commands, Entity, GlobalTransform, Message, MessageReader, MessageWriter, Query, Res,
    ResMut, Transform, With, Without,
};

#[derive(Message)]
pub struct MouseLongOrMovedWriteback {
    event: MouseLongOrMoved,
}

impl From<MouseLongOrMoved> for MouseLongOrMovedWriteback {
    fn from(event: MouseLongOrMoved) -> Self {
        Self { event }
    }
}

pub fn mouse_long_or_moved_writeback(
    mut read: MessageReader<MouseLongOrMovedWriteback>,
    mut write: MessageWriter<MouseLongOrMoved>,
) {
    for event in read.read() {
        write.write(event.event.clone());
    }
}

pub fn mouse_long_or_moved(
    mut events: MessageReader<MouseLongOrMoved>,
    mut ev_writeback: MessageWriter<MouseLongOrMovedWriteback>,
    query: Query<
        (
            &GlobalTransform,
            Option<&Position>,
            Option<&Rotation>,
            Option<&RigidBody>,
        ),
        Without<MainCamera>,
    >,
    mut commands: Commands,
    mut select_mouse: MessageWriter<SelectEvent>,
    mut params: MouseLongOrMovedParams,
) {
    use crate::UsedMouseButton;
    use crate::tools::ToolEnum::*;
    use avian2d::prelude::*;
    use bevy::log::info;
    use bevy::math::Vec3Swizzles;
    for MouseLongOrMoved(hover_tool, pos, click_pos_screen, button) in events.read() {
        let clickpos = *pos;
        let click_pos_screen = *click_pos_screen;
        let curpos = params.mouse_pos.xy();
        info!("long or moved!");

        let selected_entities = params.selected.iter().collect::<Vec<_>>();

        /*let (ui_button, other_button) = match button {
            UsedMouseButton::Left => (&pointer_state.mouse_left, &pointer_state.mouse_right),
            UsedMouseButton::Right => (&pointer_state.mouse_right, &pointer_state.mouse_left)
        };

        if Some(button) == pointer_state.mouse_button.as_ref() && other_button.is_some() {
            continue;
        }*/
        // todo: is this really needed?

        let scene = params.scene_state.scene;
        let ui_button = match button {
            UsedMouseButton::Left => &mut params.pointer_state.mouse_left,
            UsedMouseButton::Right => &mut params.pointer_state.mouse_right,
        };

        match hover_tool {
            Pan(None) => {
                info!("panning");
                *ui_button = Some(Pan(Some(PanState {
                    orig_camera_pos: params.cameras.single_mut().unwrap().translation.xy(),
                })));
            }
            Zoom(None) => {
                let camera = params.cameras.single_mut().unwrap();
                *ui_button = Some(Zoom(Some(ZoomState {
                    orig_camera_pos: camera.translation.xy(),
                    orig_camera_scale: camera.scale.x,
                    click_pos_screen,
                })));
            }
            _ => {
                let under_mouse =
                    select::collider_under_point(clickpos, &params.selection_colliders);

                let should_select_under_mouse =
                    matches!(hover_tool, Drag(None) | Fix(()) | Axle(()) | Tracer(()))
                        || matches!(hover_tool, Laser(()) | Thruster(()))
                            && under_mouse.is_some_and(|entity| {
                                query
                                    .get(entity)
                                    .is_ok_and(|(_, _, _, body)| body.is_none())
                            })
                        || matches!(hover_tool, Move(None) | Rotate(None))
                            && under_mouse.is_none_or(|entity| !params.selected.contains(entity));
                if should_select_under_mouse {
                    select_mouse.write(SelectEvent {
                        entities: under_mouse.into_iter().collect(),
                        mode: SelectionMode::Replace,
                        open_menu: false,
                    });
                }

                match (hover_tool, under_mouse) {
                    (Spring(None), _) => {
                        let start_body =
                            select::colliders_under_point(clickpos, &params.selection_colliders)
                                .find(|ent| {
                                    query.get(*ent).is_ok_and(|(_, _, _, body)| body.is_some())
                                });
                        let start = if let Some(entity) = start_body {
                            let (transform, _, _, _) = query.get(entity).unwrap();
                            SpringEnd::from_body(entity, transform, clickpos)
                        } else {
                            SpringEnd::sky(clickpos)
                        };
                        let camera = params.cameras.single_mut().unwrap();
                        let unit_size = spring::unit_size_for_camera(&camera);
                        let preview = spring::spawn_spring(
                            &mut commands,
                            scene,
                            &params.images,
                            ColorComponent(
                                params
                                    .palette
                                    .current_palette
                                    .get_color_hsva_opaque(&mut *params.rng.single_mut().unwrap()),
                            ),
                            start,
                            SpringEnd::sky(curpos),
                            unit_size,
                            &mut *params.z,
                            true,
                        );
                        *ui_button = Some(Spring(Some(SpringPlacementState { preview, start })));
                    }
                    (Drag(None), Some(ent)) => {
                        info!("start drag {:?}", ent);
                        let grab_local_point = query.get(ent).unwrap().0.to_local(curpos);
                        let drag_entity = commands
                            .spawn((
                                DragObject,
                                CustomForce::default(),
                                DragTarget {
                                    entity: ent,
                                    grab_local_point,
                                    mouse_pos: curpos,
                                },
                            ))
                            .insert(ChildOf(ent))
                            .id();
                        *ui_button = Some(Drag(Some(DragState {
                            entity: ent,
                            grab_local_point,
                            drag_entity,
                        })));
                    }
                    (Rotate(None), Some(under)) => {
                        let (global_transform, _, Some(_rot), _body) = query.get(under).unwrap()
                        else {
                            continue;
                        };
                        if !params.selected.contains(under) {
                            continue;
                        }
                        info!("start rotate {:?}", under);
                        let pivot = rotation_pivot(
                            &selected_entities,
                            &params.rotation_pivots.body_masses,
                            &params.rotation_pivots.body_positions,
                            &params.rotation_pivots.springs,
                            &params.rotation_pivots.attachment_visuals,
                            &params.rotation_pivots.axle_joints,
                            &params.rotation_pivots.fix_joints,
                        )
                        .unwrap_or_else(|| query.get(under).unwrap().0.translation_vec3a().xy());
                        *ui_button = Some(Rotate(Some(RotateState {
                            current_angle: global_transform.rotation().to_euler(EulerRot::XYZ).2,
                            pivot,
                            targets: selected_entities
                                .iter()
                                .filter_map(|entity| {
                                    let (global_transform, _pos, rot, _) =
                                        query.get(*entity).ok()?;
                                    rot?;
                                    Some(crate::tools::rotate::RotateTarget {
                                        entity: *entity,
                                        original_pos: global_transform.translation_vec3a().xy(),
                                        original_angle: global_transform
                                            .rotation()
                                            .to_euler(EulerRot::XYZ)
                                            .2,
                                    })
                                })
                                .collect(),
                            overlay_ent: spawn_draw_object(&mut commands, &params.draw_objects),
                            scale: params.cameras.single_mut().unwrap().scale.x,
                        })));
                        for entity in &selected_entities {
                            if query
                                .get(*entity)
                                .ok()
                                .and_then(|(_, _, _, body)| body)
                                .is_some()
                            {
                                commands.entity(*entity).insert(RigidBody::Static);
                            }
                        }
                    }
                    (Rotate(None) | Move(None), None) => {
                        ev_writeback.write(
                            MouseLongOrMoved(Pan(None), clickpos, click_pos_screen, *button).into(),
                        );
                    }
                    (_, Some(under)) if params.selected.contains(under) => {
                        let (transform, _, _, _) = query.get(under).unwrap();
                        *ui_button = Some(Move(Some(MoveState {
                            primary_delta: transform.translation_vec3a().xy() - curpos,
                            targets: selected_entities
                                .iter()
                                .filter_map(|entity| {
                                    let (_, pos, _, _) = query.get(*entity).ok()?;
                                    Some((*entity, pos?.0))
                                })
                                .collect(),
                        })));
                        for entity in &selected_entities {
                            if query
                                .get(*entity)
                                .ok()
                                .and_then(|(_, _, _, body)| body)
                                .is_some()
                            {
                                commands.entity(*entity).insert(RigidBody::Static);
                            }
                        }
                    }
                    (Box(None), _) => {
                        *ui_button = Some(Box(Some(spawn_draw_object(
                            &mut commands,
                            &params.draw_objects,
                        ))));
                    }
                    (Circle(None), _) => {
                        *ui_button = Some(Circle(Some(spawn_draw_object(
                            &mut commands,
                            &params.draw_objects,
                        ))));
                    }
                    (tool, _) => {
                        dbg!(tool);
                        //todo!()
                    }
                }
            }
        }
    }
}

#[derive(SystemParam)]
pub struct MouseLongOrMovedParams<'w, 's> {
    cameras: Query<'w, 's, &'static mut Transform, With<MainCamera>>,
    pointer_state: ResMut<'w, PointerToolState>,
    selected: Query<'w, 's, Entity, With<Selected>>,
    selection_colliders: Query<
        'w,
        's,
        (Entity, &'static Collider, &'static GlobalTransform),
        Without<ColliderDisabled>,
    >,
    scene_state: Res<'w, SceneState>,
    mouse_pos: Res<'w, MousePosWorld>,
    images: Res<'w, AppIcons>,
    palette: Res<'w, PaletteConfig>,
    rng: Query<'w, 's, &'static mut RngComponent>,
    z: ResMut<'w, DepthSorter>,
    draw_objects: Query<'w, 's, Entity, With<crate::DrawObject>>,
    rotation_pivots: RotationPivotQueries<'w, 's>,
}

#[derive(SystemParam)]
pub struct RotationPivotQueries<'w, 's> {
    body_masses: Query<
        'w,
        's,
        (
            &'static Position,
            &'static Rotation,
            &'static ColliderMassProperties,
        ),
    >,
    body_positions: Query<'w, 's, (&'static Position, &'static Rotation)>,
    springs: Query<'w, 's, &'static SpringObject>,
    attachment_visuals: Query<'w, 's, &'static GlobalTransform>,
    axle_joints: Query<'w, 's, (&'static RevoluteJoint, &'static AttachmentJoint)>,
    fix_joints: Query<'w, 's, (&'static FixedJoint, &'static AttachmentJoint)>,
}

fn rotation_pivot(
    selected: &[Entity],
    body_masses: &Query<(&Position, &Rotation, &ColliderMassProperties)>,
    body_positions: &Query<(&Position, &Rotation)>,
    springs: &Query<&SpringObject>,
    attachment_visuals: &Query<&GlobalTransform>,
    axle_joints: &Query<(&RevoluteJoint, &AttachmentJoint)>,
    fix_joints: &Query<(&FixedJoint, &AttachmentJoint)>,
) -> Option<Vec2> {
    let external = external_attachment_pivots(
        selected,
        body_positions,
        springs,
        attachment_visuals,
        axle_joints,
        fix_joints,
    );
    if external.len() == 1 {
        return external.first().copied();
    }
    center_of_mass(selected, body_masses)
}

fn center_of_mass(
    selected: &[Entity],
    body_masses: &Query<(&Position, &Rotation, &ColliderMassProperties)>,
) -> Option<Vec2> {
    let mut mass_sum = 0.0;
    let mut weighted = Vec2::ZERO;
    for entity in selected {
        let Ok((position, rotation, mass)) = body_masses.get(*entity) else {
            continue;
        };
        mass_sum += mass.mass;
        weighted += (position.0 + *rotation * mass.center_of_mass) * mass.mass;
    }
    (mass_sum > 0.0).then_some(weighted / mass_sum)
}

fn external_attachment_pivots(
    selected: &[Entity],
    body_positions: &Query<(&Position, &Rotation)>,
    springs: &Query<&SpringObject>,
    attachment_visuals: &Query<&GlobalTransform>,
    axle_joints: &Query<(&RevoluteJoint, &AttachmentJoint)>,
    fix_joints: &Query<(&FixedJoint, &AttachmentJoint)>,
) -> Vec<Vec2> {
    let mut pivots = Vec::new();

    for (joint, attachment) in axle_joints.iter() {
        push_joint_pivot(
            selected,
            joint.body1,
            joint.body2,
            attachment,
            attachment_visuals,
            &mut pivots,
        );
    }
    for (joint, attachment) in fix_joints.iter() {
        push_joint_pivot(
            selected,
            joint.body1,
            joint.body2,
            attachment,
            attachment_visuals,
            &mut pivots,
        );
    }

    for spring in springs.iter() {
        push_spring_pivot(
            selected,
            spring.end_a,
            spring.end_b,
            body_positions,
            &mut pivots,
        );
        push_spring_pivot(
            selected,
            spring.end_b,
            spring.end_a,
            body_positions,
            &mut pivots,
        );
    }

    pivots
}

fn push_joint_pivot(
    selected: &[Entity],
    body1: Entity,
    body2: Entity,
    attachment: &AttachmentJoint,
    attachment_visuals: &Query<&GlobalTransform>,
    pivots: &mut Vec<Vec2>,
) {
    if selected.contains(&body1) == selected.contains(&body2) {
        return;
    }
    if let Ok(transform) = attachment_visuals.get(attachment.visual) {
        push_unique_pivot(pivots, transform.translation_vec3a().xy());
    }
}

fn push_spring_pivot(
    selected: &[Entity],
    selected_end: SpringEnd,
    other_end: SpringEnd,
    body_positions: &Query<(&Position, &Rotation)>,
    pivots: &mut Vec<Vec2>,
) {
    let SpringEnd::Body { entity, .. } = selected_end else {
        return;
    };
    if !selected.contains(&entity) {
        return;
    }
    if let SpringEnd::Body { entity: other, .. } = other_end
        && selected.contains(&other)
    {
        return;
    }
    if let Some(pivot) = other_end.world_pos(body_positions) {
        push_unique_pivot(pivots, pivot);
    }
}

fn push_unique_pivot(pivots: &mut Vec<Vec2>, pivot: Vec2) {
    if !pivots
        .iter()
        .any(|existing| existing.distance_squared(pivot) < 1.0e-8)
    {
        pivots.push(pivot);
    }
}

#[derive(Clone, Message)]
pub struct MouseLongOrMoved(pub ToolEnum, pub Vec2, pub Vec2, pub UsedMouseButton);

fn spawn_draw_object(
    commands: &mut Commands,
    draw_objects: &Query<Entity, With<crate::DrawObject>>,
) -> Entity {
    for draw_ent in draw_objects.iter() {
        if let Ok(mut draw_ent) = commands.get_entity(draw_ent) {
            draw_ent.despawn();
        }
    }
    commands.spawn(crate::DrawObject).id()
}
