use crate::config::AppConfig;
use crate::mouse::select;
use crate::mouse::select::{SelectEvent, SelectionMode};
use crate::mouse_tracking::{MainCamera, MousePosWorld};
use crate::objects::spring::{self, SpringEnd, SpringObject, SpringPlacementState};
use crate::objects::{
    ColorComponent,
    axle::{FixObject, JointGeometry},
    body::world_point,
};
use crate::palette::PaletteConfig;
use crate::rng::RngComponent;
use crate::tools::ToolEnum;
use crate::tools::add_object::DepthSorter;
use crate::tools::drag::{DragObject, DragState, DragTarget};
use crate::tools::r#move::MoveState;
use crate::tools::pan::PanState;
use crate::tools::plane::PlanePlacementState;
use crate::tools::polygon::{FREEHAND_SAMPLE_DISTANCE_PX, PolygonPlacementState};
use crate::tools::rotate::RotateState;
use crate::tools::zoom::ZoomState;
use crate::ui::images::AppIcons;
use crate::ui::selection_overlay::{Overlay, OverlayState, PlaneNormalIcon, RotationOriginIcon};
use crate::ui::{PointerToolState, SceneState, Selected};
use crate::{InvTransformPoint, UsedMouseButton};
use avian2d::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy::math::{EulerRot, Vec2, Vec3, Vec3Swizzles};
use bevy::prelude::{
    ChildOf, Commands, Entity, GlobalTransform, Message, MessageReader, MessageWriter, Query, Res,
    ResMut, Sprite, Transform, Visibility, With, Without,
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
            Option<&ColliderOf>,
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
                        expand_groups: true,
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
                                DragTarget {
                                    entity: ent,
                                    grab_local_point,
                                    mouse_pos: curpos,
                                },
                            ))
                            .insert(ChildOf(ent))
                            .id();
                        *ui_button = Some(Drag(Some(DragState { drag_entity })));
                    }
                    (Rotate(None), Some(under)) => {
                        let (global_transform, _, Some(_rot), _body) = query.get(under).unwrap()
                        else {
                            continue;
                        };
                        if !params.selected.contains(under) {
                            continue;
                        }
                        let selected_entities =
                            expand_compounds(&selected_entities, params.fixes.iter());
                        select_expanded(&mut select_mouse, &selected_entities, &params.selected);
                        info!("start rotate {:?}", under);
                        let pivot = rotation_pivot(
                            &selected_entities,
                            &params.rotation_pivots.body_masses,
                            &params.rotation_pivots.body_positions,
                            &params.rotation_pivots.springs,
                            &params.rotation_pivots.joints,
                        )
                        .unwrap_or_else(|| query.get(under).unwrap().0.translation_vec3a().xy());
                        let targets = selected_entities
                            .iter()
                            .filter_map(|entity| {
                                let (global_transform, _pos, rot, _) = query.get(*entity).ok()?;
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
                            .collect::<Vec<_>>();
                        let origin_angle =
                            rotation_origin_initial_angle(selected_entities.len(), &targets);
                        let scale = params.cameras.single_mut().unwrap().scale.x
                            * params.app_config.ui_scale;
                        let overlay_ent = spawn_rotate_draw_object(
                            &mut commands,
                            &params.draw_objects,
                            &params.images,
                            scale,
                            origin_angle,
                        );
                        *ui_button = Some(Rotate(Some(RotateState {
                            current_angle: global_transform.rotation().to_euler(EulerRot::XYZ).2,
                            pivot,
                            targets,
                            overlay_ent,
                            scale,
                        })));
                        for entity in &selected_entities {
                            if let Ok((_, _, _, Some(body))) = query.get(*entity) {
                                commands.entity(body.body).insert(RigidBody::Static);
                            }
                        }
                    }
                    (Rotate(None) | Move(None), None) => {
                        ev_writeback.write(
                            MouseLongOrMoved(Pan(None), clickpos, click_pos_screen, *button).into(),
                        );
                    }
                    (_, Some(under)) if params.selected.contains(under) => {
                        let selected_entities =
                            expand_compounds(&selected_entities, params.fixes.iter());
                        select_expanded(&mut select_mouse, &selected_entities, &params.selected);
                        let (transform, _, _, _) = query.get(under).unwrap();
                        *ui_button = Some(Move(Some(MoveState {
                            primary_delta: transform.translation_vec3a().xy() - curpos,
                            pointer_start: curpos,
                            targets: selected_entities
                                .iter()
                                .filter_map(|entity| {
                                    let (_, pos, _, _) = query.get(*entity).ok()?;
                                    Some((*entity, pos?.0))
                                })
                                .collect(),
                        })));
                        for entity in &selected_entities {
                            if let Ok((_, _, _, Some(body))) = query.get(*entity) {
                                commands.entity(body.body).insert(RigidBody::Static);
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
                    (Gear(None), _) => {
                        *ui_button = Some(Gear(Some(spawn_draw_object(
                            &mut commands,
                            &params.draw_objects,
                        ))));
                    }
                    (Polygon(None), _) => {
                        let overlay_ent = spawn_draw_object(&mut commands, &params.draw_objects);
                        let camera_scale = params.cameras.single().unwrap().scale.x.abs();
                        let point = params.grid.snap_point(curpos, camera_scale);
                        let minimum_distance = if params.grid.enabled && params.grid.snap {
                            f32::EPSILON
                        } else {
                            FREEHAND_SAMPLE_DISTANCE_PX * camera_scale
                        };
                        let mut state = PolygonPlacementState::new(overlay_ent, clickpos);
                        state.push_world_point(point, minimum_distance);
                        params.overlay.draw_ent = Some((
                            overlay_ent,
                            Overlay::Polygon(state.points.clone(), false),
                            state.origin,
                        ));
                        *ui_button = Some(Polygon(Some(state)));
                    }
                    (Plane(None), _) => {
                        let camera = params.cameras.single_mut().unwrap();
                        let scale = camera.scale.x * params.app_config.ui_scale;
                        let color = params
                            .palette
                            .current_palette
                            .get_color_hsva(&mut *params.rng.single_mut().unwrap());
                        let overlay_ent = spawn_draw_object(&mut commands, &params.draw_objects);
                        crate::objects::plane::insert_plane_preview(
                            &mut commands,
                            overlay_ent,
                            clickpos,
                            color,
                        );
                        commands.entity(overlay_ent).with_children(|parent| {
                            parent.spawn((
                                PlaneNormalIcon,
                                Sprite {
                                    image: params.images.force_arrow.clone(),
                                    custom_size: Some(Vec2::splat(
                                        crate::tools::rotate::ROTATE_HELPER_RADIUS * scale,
                                    )),
                                    ..Default::default()
                                },
                                Transform::from_translation(
                                    (Vec2::Y
                                        * crate::tools::rotate::ROTATE_HELPER_RADIUS
                                        * scale
                                        * 0.5)
                                        .extend(crate::FOREGROUND_Z),
                                ),
                            ));
                        });
                        *ui_button = Some(Plane(Some(PlanePlacementState {
                            overlay_ent,
                            scale,
                            color,
                        })));
                    }
                    _ => {}
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
    app_config: Res<'w, AppConfig>,
    grid: Res<'w, crate::grid::GridSettings>,
    palette: Res<'w, PaletteConfig>,
    rng: Query<'w, 's, &'static mut RngComponent>,
    z: ResMut<'w, DepthSorter>,
    draw_objects: Query<'w, 's, Entity, With<crate::DrawObject>>,
    overlay: ResMut<'w, OverlayState>,
    rotation_pivots: RotationPivotQueries<'w, 's>,
    fixes: Query<'w, 's, &'static JointGeometry, With<FixObject>>,
}

fn expand_compounds<'a>(
    selected: &[Entity],
    fixes: impl IntoIterator<Item = &'a JointGeometry>,
) -> Vec<Entity> {
    let fixes = fixes.into_iter().collect::<Vec<_>>();
    let mut expanded = selected.to_vec();
    loop {
        let len = expanded.len();
        for joint in &fixes {
            if joint
                .geoms
                .iter()
                .flatten()
                .any(|entity| expanded.contains(entity))
            {
                for entity in joint.geoms.iter().flatten() {
                    if !expanded.contains(entity) {
                        expanded.push(*entity);
                    }
                }
            }
        }
        if expanded.len() == len {
            return expanded;
        }
    }
}

fn select_expanded(
    events: &mut MessageWriter<SelectEvent>,
    expanded: &[Entity],
    selected: &Query<Entity, With<Selected>>,
) {
    if expanded.len() != selected.iter().count() {
        events.write(SelectEvent {
            entities: expanded.to_vec(),
            mode: SelectionMode::Replace,
            open_menu: false,
            expand_groups: false,
        });
    }
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
    springs: Query<'w, 's, (Entity, &'static SpringObject)>,
    joints: Query<'w, 's, (&'static JointGeometry, &'static GlobalTransform)>,
}

fn rotation_pivot(
    selected: &[Entity],
    body_masses: &Query<(&Position, &Rotation, &ColliderMassProperties)>,
    body_positions: &Query<(&Position, &Rotation)>,
    springs: &Query<(Entity, &SpringObject)>,
    joints: &Query<(&JointGeometry, &GlobalTransform)>,
) -> Option<Vec2> {
    let external = external_attachment_pivots(selected, body_positions, springs, joints);
    if external.len() == 1 {
        return external.first().copied();
    }
    center_of_mass(selected, body_masses).or_else(|| {
        let positions = selected
            .iter()
            .filter_map(|entity| body_positions.get(*entity).ok().map(|(pos, _)| pos.0))
            .collect::<Vec<_>>();
        (!positions.is_empty())
            .then(|| positions.iter().copied().sum::<Vec2>() / positions.len() as f32)
    })
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
        weighted += world_point((position.0, *rotation), mass.center_of_mass) * mass.mass;
    }
    (mass_sum > 0.0).then_some(weighted / mass_sum)
}

fn external_attachment_pivots(
    selected: &[Entity],
    body_positions: &Query<(&Position, &Rotation)>,
    springs: &Query<(Entity, &SpringObject)>,
    joints: &Query<(&JointGeometry, &GlobalTransform)>,
) -> Vec<Vec2> {
    let mut pivots = Vec::new();

    for (joint, transform) in joints.iter() {
        let selected_ends = joint
            .geoms
            .map(|geom| geom.is_some_and(|entity| selected.contains(&entity)));
        if selected_ends[0] != selected_ends[1] {
            push_unique_pivot(&mut pivots, transform.translation_vec3a().xy());
        }
    }

    for (spring_entity, spring) in springs.iter() {
        push_spring_pivot(
            selected,
            spring_entity,
            spring.end_a,
            spring.end_b,
            body_positions,
            &mut pivots,
        );
        push_spring_pivot(
            selected,
            spring_entity,
            spring.end_b,
            spring.end_a,
            body_positions,
            &mut pivots,
        );
    }

    pivots
}

fn push_spring_pivot(
    selected: &[Entity],
    spring_entity: Entity,
    selected_end: SpringEnd,
    other_end: SpringEnd,
    body_positions: &Query<(&Position, &Rotation)>,
    pivots: &mut Vec<Vec2>,
) {
    let Some(pivot_end) =
        spring_boundary_pivot_end(selected, spring_entity, selected_end, other_end)
    else {
        return;
    };
    if let Some(pivot) = pivot_end.world_pos(body_positions) {
        push_unique_pivot(pivots, pivot);
    }
}

fn spring_boundary_pivot_end(
    selected: &[Entity],
    spring_entity: Entity,
    selected_end: SpringEnd,
    other_end: SpringEnd,
) -> Option<SpringEnd> {
    let SpringEnd::Body { entity, .. } = selected_end else {
        return None;
    };
    if !selected.contains(&entity) {
        return None;
    }

    if !selected.contains(&spring_entity) {
        return Some(selected_end);
    }

    if let SpringEnd::Body { entity: other, .. } = other_end
        && selected.contains(&other)
    {
        return None;
    }
    Some(other_end)
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

const ROTATION_ORIGIN_SCREEN_SIZE: f32 = 32.0;

fn rotation_origin_initial_angle(
    selected_entity_count: usize,
    targets: &[crate::tools::rotate::RotateTarget],
) -> f32 {
    if selected_entity_count == 1 && targets.len() == 1 {
        targets[0].original_angle
    } else {
        0.0
    }
}

fn spawn_rotate_draw_object(
    commands: &mut Commands,
    draw_objects: &Query<Entity, With<crate::DrawObject>>,
    images: &AppIcons,
    camera_scale: f32,
    origin_angle: f32,
) -> Entity {
    let draw_entity = spawn_draw_object(commands, draw_objects);
    commands.entity(draw_entity).with_child((
        Sprite {
            image: images.rotate_origo.clone(),
            custom_size: Some(Vec2::splat(ROTATION_ORIGIN_SCREEN_SIZE * camera_scale)),
            ..Default::default()
        },
        Transform::from_translation(Vec3::Z * 0.01),
        Visibility::Inherited,
        RotationOriginIcon { origin_angle },
    ));
    draw_entity
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(index: u32) -> Entity {
        Entity::from_raw_u32(index).unwrap()
    }

    fn body_end(entity: Entity, local_anchor: Vec2) -> SpringEnd {
        SpringEnd::Body {
            entity,
            local_anchor,
        }
    }

    #[test]
    fn mouse_gestures_expand_logical_compounds_but_not_the_whole_sky() {
        let (a, b, c, sky_a, sky_b) = (entity(1), entity(2), entity(3), entity(4), entity(5));
        let joint = |geoms| JointGeometry {
            geoms,
            positions: [Vec2::ZERO; 2],
        };
        let fixes = [
            joint([Some(a), Some(b)]),
            joint([Some(b), Some(c)]),
            joint([Some(sky_a), None]),
            joint([Some(sky_b), None]),
        ];

        assert_eq!(expand_compounds(&[a], &fixes), vec![a, b, c]);
        assert_eq!(expand_compounds(&[sky_a], &fixes), vec![sky_a]);
    }

    #[test]
    fn rotation_origin_uses_object_angle_only_for_a_single_selection() {
        let object_angle = 42.0_f32.to_radians();
        let target = crate::tools::rotate::RotateTarget {
            entity: entity(1),
            original_pos: Vec2::ZERO,
            original_angle: object_angle,
        };

        assert_eq!(rotation_origin_initial_angle(1, &[target]), object_angle);
        assert_eq!(rotation_origin_initial_angle(2, &[target]), 0.0);
    }

    #[test]
    fn unselected_spring_pivots_at_its_end_on_the_selected_body() {
        let body = entity(1);
        let spring = entity(2);
        let body_anchor = Vec2::new(3.0, 4.0);

        let pivot = spring_boundary_pivot_end(
            &[body],
            spring,
            body_end(body, body_anchor),
            SpringEnd::sky(Vec2::new(20.0, 10.0)),
        );

        assert!(matches!(
            pivot,
            Some(SpringEnd::Body {
                entity,
                local_anchor,
            }) if entity == body && local_anchor == body_anchor
        ));
    }

    #[test]
    fn selected_spring_extends_the_pivot_to_its_sky_end() {
        let body = entity(1);
        let spring = entity(2);
        let sky_anchor = Vec2::new(20.0, 10.0);

        let pivot = spring_boundary_pivot_end(
            &[body, spring],
            spring,
            body_end(body, Vec2::new(3.0, 4.0)),
            SpringEnd::sky(sky_anchor),
        );

        assert!(matches!(
            pivot,
            Some(SpringEnd::Sky { world_anchor }) if world_anchor == sky_anchor
        ));
    }

    #[test]
    fn spring_chain_pivot_moves_across_each_selected_spring() {
        let body_a = entity(1);
        let spring_a_b = entity(2);
        let body_b = entity(3);
        let spring_b_sky = entity(4);
        let body_b_anchor = Vec2::new(5.0, 6.0);
        let sky_anchor = Vec2::new(30.0, 40.0);

        let selected_through_body_b = [body_a, spring_a_b, body_b];
        let internal_link = spring_boundary_pivot_end(
            &selected_through_body_b,
            spring_a_b,
            body_end(body_a, Vec2::X),
            body_end(body_b, -Vec2::X),
        );
        let boundary = spring_boundary_pivot_end(
            &selected_through_body_b,
            spring_b_sky,
            body_end(body_b, body_b_anchor),
            SpringEnd::sky(sky_anchor),
        );

        assert!(internal_link.is_none());
        assert!(matches!(
            boundary,
            Some(SpringEnd::Body {
                entity,
                local_anchor,
            }) if entity == body_b && local_anchor == body_b_anchor
        ));

        let selected_through_sky = [body_a, spring_a_b, body_b, spring_b_sky];
        let boundary = spring_boundary_pivot_end(
            &selected_through_sky,
            spring_b_sky,
            body_end(body_b, body_b_anchor),
            SpringEnd::sky(sky_anchor),
        );

        assert!(matches!(
            boundary,
            Some(SpringEnd::Sky { world_anchor }) if world_anchor == sky_anchor
        ));
    }
}
