use crate::lyon_compat::ScreenSpaceShapeMaterial;
use crate::mouse::select::{SelectEvent, SelectionGroup, SelectionMode};
use crate::mouse_tracking::MainCamera;
use crate::objects::kind::{ObjectKind, ObjectKinds};
use crate::tools::ToolIcons;
use crate::tools::add_object::DepthSorter;
use crate::ui::images::GuiIcons;
use crate::ui::menu_item::MenuItem;
use crate::ui::{InitialPos, Selected, Subwindow, WindowSelectionTarget, bool_checkbox};
use avian2d::prelude::*;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};
use egui::{Popup, RectAlign, SetOpenCommand};
use std::collections::{HashMap, HashSet};

#[derive(Default, Component)]
pub struct SelectionWindow;

pub fn add_systems(app: &mut App) {
    app.init_resource::<CameraFollow>()
        .add_message::<SelectionAction>()
        .add_systems(EguiPrimaryContextPass, SelectionWindow::show)
        .add_systems(PreUpdate, process_selection_actions)
        .add_systems(
            PostUpdate,
            update_camera_follow.before(TransformSystems::Propagate),
        );
}

#[derive(Resource, Default, Debug)]
struct CameraFollow(Option<CameraFollowTarget>);

#[derive(Clone, Copy, Debug)]
struct CameraFollowTarget {
    entity: Entity,
    track_rotation: bool,
    last_position: Option<Vec2>,
}

impl CameraFollow {
    fn set_target(&mut self, entity: Entity) {
        if self.0.is_none_or(|target| target.entity != entity) {
            self.0 = Some(CameraFollowTarget {
                entity,
                track_rotation: false,
                last_position: None,
            });
        }
    }

    fn clear(&mut self) {
        self.0 = None;
    }
}

#[derive(Message)]
enum SelectionAction {
    Invert,
    DeselectAll,
    Group { targets: Vec<Entity> },
    Ungroup { targets: Vec<Entity> },
    Move {
        targets: Vec<Entity>,
        to: ZOrder,
    },
}

#[derive(Clone, Copy)]
enum ZOrder {
    Back,
    Front,
}

type SelectableObjectFilter = (With<Collider>, Without<ColliderDisabled>);

impl SelectionWindow {
    fn show(
        mut wnds: Query<
            (Entity, &WindowSelectionTarget, &mut InitialPos),
            With<SelectionWindow>,
        >,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
        mut actions: MessageWriter<SelectionAction>,
        mut select_events: MessageWriter<SelectEvent>,
        physical_objects: Query<(), With<RigidBody>>,
        selection_groups: Query<&SelectionGroup>,
        object_kinds: ObjectKinds,
        tool_icons: Res<ToolIcons>,
        gui_icons: Res<GuiIcons>,
        mut camera_follow: ResMut<CameraFollow>,
    ) {
        let ctx = egui_ctx.ctx_mut().expect("primary egui context");
        for (id, target, mut initial_pos) in wnds.iter_mut() {
            let kind_selections = ObjectKind::ALL
                .into_iter()
                .filter_map(|kind| {
                    let entities = target
                        .iter()
                        .filter(|entity| object_kinds.get(*entity) == Some(kind))
                        .collect::<Vec<_>>();
                    (!entities.is_empty()).then(|| {
                        let title = object_kinds.selection_title(entities.iter().copied());
                        (kind, entities, title)
                    })
                })
                .collect::<Vec<_>>();
            egui::Window::new(target.title_or("Select"))
                .default_size(egui::Vec2::ZERO)
                .resizable(false)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, _commands| {
                    if kind_selections.len() >= 2 {
                        let select_menu =
                            ui.add(MenuItem::menu(None, "Select", gui_icons.arrow_right));
                        Popup::menu(&select_menu)
                            .id(select_menu.id.with("object kinds"))
                            .align(RectAlign::RIGHT_START)
                            .align_alternatives(&[])
                            .layout(egui::Layout::top_down(egui::Align::Min))
                            .open_memory(
                                select_menu
                                    .hovered()
                                    .then_some(SetOpenCommand::Bool(true)),
                            )
                            .show(|ui| {
                                for (kind, entities, title) in &kind_selections {
                                    if ui
                                        .add(MenuItem::button(
                                            Some(object_kind_icon(*kind, &tool_icons)),
                                            format!("Select {}", kind.plural()),
                                        ).shrink_to_fit())
                                        .clicked()
                                    {
                                        select_events.write(SelectEvent {
                                            entities: entities.clone(),
                                            mode: SelectionMode::Replace,
                                            open_menu: false,
                                            expand_groups: false,
                                        });
                                        _commands.entity(id).insert(
                                            WindowSelectionTarget::from_entities(
                                                entities.iter().copied(),
                                            )
                                            .with_title(title.clone()),
                                        );
                                        ui.close();
                                    }
                                }
                            });
                    }
                    if ui
                        .add(MenuItem::button(None, "Invert selection"))
                        .clicked()
                    {
                        actions.write(SelectionAction::Invert);
                    }
                    if ui
                        .add(MenuItem::button(None, "Deselect everything"))
                        .clicked()
                    {
                        actions.write(SelectionAction::DeselectAll);
                    }
                    if ui
                        .add(MenuItem::button(None, "Move selected to back"))
                        .clicked()
                    {
                        actions.write(SelectionAction::Move {
                            targets: target.iter().collect(),
                            to: ZOrder::Back,
                        });
                    }
                    if ui
                        .add(MenuItem::button(None, "Move selected to front"))
                        .clicked()
                    {
                        actions.write(SelectionAction::Move {
                            targets: target.iter().collect(),
                            to: ZOrder::Front,
                        });
                    }

                    let targets = target.iter().collect::<Vec<_>>();
                    let (show_group, show_ungroup) =
                        group_action_visibility(&targets, &selection_groups);
                    if show_group || show_ungroup {
                        ui.separator();
                        let mut group_clicked = false;
                        let mut ungroup_clicked = false;
                        if show_group && show_ungroup {
                            ui.columns(2, |columns| {
                                group_clicked = columns[0]
                                    .add(MenuItem::button(None, "Group"))
                                    .clicked();
                                ungroup_clicked = columns[1]
                                    .add(MenuItem::button(None, "Ungroup"))
                                    .clicked();
                            });
                        } else if show_group {
                            group_clicked =
                                ui.add(MenuItem::button(None, "Group")).clicked();
                        } else {
                            ungroup_clicked =
                                ui.add(MenuItem::button(None, "Ungroup")).clicked();
                        }
                        if group_clicked {
                            actions.write(SelectionAction::Group {
                                targets: targets.clone(),
                            });
                        }
                        if ungroup_clicked {
                            actions.write(SelectionAction::Ungroup {
                                targets: targets.clone(),
                            });
                        }
                    }

                    let physical_target = match target.entities.as_slice() {
                        [entity] if physical_objects.contains(*entity) => Some(*entity),
                        _ => None,
                    };
                    if let Some(entity) = physical_target {
                        ui.separator();

                        let mut followed = camera_follow
                            .0
                            .is_some_and(|target| target.entity == entity);
                        if bool_checkbox(ui, &gui_icons, &mut followed, "Follow") {
                            if followed {
                                camera_follow.set_target(entity);
                            } else {
                                camera_follow.clear();
                            }
                        }

                        let mut track_rotation = followed
                            && camera_follow
                                .0
                                .is_some_and(|target| target.track_rotation);
                        let changed = ui
                            .add_enabled_ui(followed, |ui| {
                                bool_checkbox(
                                    ui,
                                    &gui_icons,
                                    &mut track_rotation,
                                    "Track rotation",
                                )
                            })
                            .inner;
                        if changed
                            && let Some(target) = &mut camera_follow.0
                            && target.entity == entity
                        {
                            target.track_rotation = track_rotation;
                        }
                    }
                });
        }
    }
}

fn object_kind_icon(kind: ObjectKind, icons: &ToolIcons) -> egui::TextureId {
    match kind {
        ObjectKind::Polygon => icons.egui_icon_polygon,
        ObjectKind::Box => icons.egui_icon_box,
        ObjectKind::Circle => icons.egui_icon_circle,
        ObjectKind::Plane => icons.egui_icon_plane,
        ObjectKind::Spring => icons.egui_icon_spring,
        ObjectKind::FixJoint => icons.egui_icon_fixjoint,
        ObjectKind::Axle => icons.egui_icon_hinge,
        ObjectKind::Tracer => icons.egui_icon_tracer,
        ObjectKind::LaserPen => icons.egui_icon_laserpen,
        ObjectKind::Thruster => icons.egui_icon_thruster,
    }
}

fn group_action_visibility(
    targets: &[Entity],
    groups: &Query<&SelectionGroup>,
) -> (bool, bool) {
    let first_group = targets
        .first()
        .and_then(|entity| groups.get(*entity).ok())
        .map(|group| group.0);
    let all_in_same_group = first_group.is_some()
        && targets
            .iter()
            .all(|entity| groups.get(*entity).is_ok_and(|group| Some(group.0) == first_group));
    let any_grouped = targets.iter().any(|entity| groups.contains(*entity));
    (!targets.is_empty() && !all_in_same_group, any_grouped)
}

fn update_camera_follow(
    mut follow: ResMut<CameraFollow>,
    targets: Query<(&Position, &Rotation), With<RigidBody>>,
    mut cameras: Query<&mut Transform, With<MainCamera>>,
) {
    let Some(target) = &mut follow.0 else {
        return;
    };
    let Ok((position, rotation)) = targets.get(target.entity) else {
        follow.clear();
        return;
    };
    let Ok(mut camera) = cameras.single_mut() else {
        return;
    };

    if let Some(last_position) = target.last_position {
        camera.translation += (position.0 - last_position).extend(0.0);
    }
    target.last_position = Some(position.0);

    if target.track_rotation {
        let next_rotation = Quat::from_rotation_z(rotation.as_radians());
        let rotation_delta = next_rotation * camera.rotation.inverse();
        let offset = Vec2::new(
            camera.translation.x - position.x,
            camera.translation.y - position.y,
        );
        let rotated_offset = rotation_delta * offset.extend(0.0);
        camera.translation.x = position.x + rotated_offset.x;
        camera.translation.y = position.y + rotated_offset.y;
        camera.rotation = next_rotation;
    }
}

fn process_selection_actions(
    mut actions: MessageReader<SelectionAction>,
    selected: Query<Entity, With<Selected>>,
    selectable: Query<(Entity, &GlobalTransform), SelectableObjectFilter>,
    mut transforms: Query<(&mut Transform, Option<&ChildOf>)>,
    globals: Query<&GlobalTransform>,
    mut mesh_materials: Query<&mut MeshMaterial2d<ColorMaterial>>,
    mut shape_materials: Query<&mut MeshMaterial2d<ScreenSpaceShapeMaterial>>,
    selection_groups: Query<(Entity, &SelectionGroup)>,
    mut depth: ResMut<DepthSorter>,
    mut commands: Commands,
) {
    for action in actions.read() {
        match action {
            SelectionAction::Invert => invert_selection(&selected, &selectable, &mut commands),
            SelectionAction::DeselectAll => deselect_everything(&selected, &mut commands),
            SelectionAction::Group { targets } => {
                group_selected(targets, &selection_groups, &mut commands)
            }
            SelectionAction::Ungroup { targets } => {
                ungroup_selected(targets, &selection_groups, &mut commands)
            }
            SelectionAction::Move { targets, to } => move_selected_z(
                targets,
                *to,
                &selectable,
                &mut transforms,
                &globals,
                &mut mesh_materials,
                &mut shape_materials,
                &mut depth,
            ),
        }
    }
}

fn group_selected(
    targets: &[Entity],
    groups: &Query<(Entity, &SelectionGroup)>,
    commands: &mut Commands,
) {
    let Some(&group_id) = targets.first() else {
        return;
    };
    let existing_groups = targets
        .iter()
        .filter_map(|entity| groups.get(*entity).ok().map(|(_, group)| group.0))
        .collect::<HashSet<_>>();
    let mut members = targets.iter().copied().collect::<HashSet<_>>();
    for (entity, group) in groups {
        if existing_groups.contains(&group.0) {
            members.insert(entity);
        }
    }
    for entity in members {
        commands.entity(entity).insert(SelectionGroup(group_id));
    }
}

fn ungroup_selected(
    targets: &[Entity],
    groups: &Query<(Entity, &SelectionGroup)>,
    commands: &mut Commands,
) {
    let removed_groups = targets
        .iter()
        .filter_map(|entity| groups.get(*entity).ok().map(|(_, group)| group.0))
        .collect::<HashSet<_>>();
    for (entity, group) in groups {
        if removed_groups.contains(&group.0) {
            commands.entity(entity).remove::<SelectionGroup>();
        }
    }
}

fn invert_selection(
    selected: &Query<Entity, With<Selected>>,
    selectable: &Query<(Entity, &GlobalTransform), SelectableObjectFilter>,
    commands: &mut Commands,
) {
    for (entity, _) in selectable {
        if selected.contains(entity) {
            commands.entity(entity).remove::<Selected>();
        } else {
            commands.entity(entity).insert(Selected);
        }
    }
}

fn deselect_everything(selected: &Query<Entity, With<Selected>>, commands: &mut Commands) {
    for entity in selected {
        commands.entity(entity).remove::<Selected>();
    }
}

fn move_selected_z(
    selected: &[Entity],
    action: ZOrder,
    selectable: &Query<(Entity, &GlobalTransform), SelectableObjectFilter>,
    transforms: &mut Query<(&mut Transform, Option<&ChildOf>)>,
    globals: &Query<&GlobalTransform>,
    mesh_materials: &mut Query<&mut MeshMaterial2d<ColorMaterial>>,
    shape_materials: &mut Query<&mut MeshMaterial2d<ScreenSpaceShapeMaterial>>,
    depth: &mut DepthSorter,
) {
    let selected = selected.iter().copied().collect::<HashSet<_>>();
    let mut selected_order = Vec::new();
    let mut other_order = Vec::new();
    for (entity, transform) in selectable {
        let entry = (entity, transform.translation().z);
        if selected.contains(&entity) {
            selected_order.push(entry);
        } else {
            other_order.push(entry);
        }
    }

    if selected_order.is_empty() || other_order.is_empty() {
        return;
    }

    selected_order.sort_by(|(_, a), (_, b)| a.total_cmp(b));
    other_order.sort_by(|(_, a), (_, b)| a.total_cmp(b));
    let ordered = match action {
        ZOrder::Back => selected_order.into_iter().chain(other_order),
        ZOrder::Front => other_order.into_iter().chain(selected_order),
    };
    let ordered = ordered.map(|(entity, _)| entity).collect::<Vec<_>>();
    let target_zs = ordered
        .iter()
        .enumerate()
        .map(|(index, entity)| (*entity, index as f32 + 1.0))
        .collect::<HashMap<_, _>>();

    let updates = ordered
        .iter()
        .filter_map(|entity| {
            let (_, parent) = transforms.get(*entity).ok()?;
            let parent = parent.map(ChildOf::parent);
            let (parent_z, parent_scale_z) = parent
                .and_then(|parent| globals.get(parent).ok().map(|global| (parent, global)))
                .map_or((0.0, 1.0), |(parent, global)| {
                    (
                        target_zs
                            .get(&parent)
                            .copied()
                            .unwrap_or(global.translation().z),
                        global.compute_transform().scale.z,
                    )
                });
            let target_z = target_zs[entity];
            Some((*entity, (target_z - parent_z) / parent_scale_z))
        })
        .collect::<Vec<_>>();

    for (entity, local_z) in updates {
        if let Ok((mut transform, _)) = transforms.get_mut(entity) {
            transform.translation.z = local_z;
        }
    }

    // Bevy 0.19 retains Material2d sort keys when only GlobalTransform changes.
    // Requeue 2D meshes so their transparent-phase order uses the new Z values.
    for mut material in mesh_materials.iter_mut() {
        material.set_changed();
    }
    for mut material in shape_materials.iter_mut() {
        material.set_changed();
    }

    depth.include(ordered.len() as f32);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::transform::TransformPlugin;

    fn selectable(transform: Transform) -> impl Bundle {
        (transform, Collider::circle(1.0))
    }

    fn global_z(app: &App, entity: Entity) -> f32 {
        app.world()
            .entity(entity)
            .get::<GlobalTransform>()
            .unwrap()
            .translation()
            .z
    }

    fn spawn_selectable(app: &mut App, parent: Entity, local_z: f32) -> Entity {
        app.world_mut()
            .spawn((
                selectable(Transform::from_xyz(0.0, 0.0, local_z)),
                ChildOf(parent),
            ))
            .id()
    }

    #[test]
    fn grouping_merges_existing_groups_and_ungrouping_dissolves_them() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<SelectionAction>()
            .init_resource::<DepthSorter>()
            .add_systems(Update, process_selection_actions);

        let old_group = app.world_mut().spawn_empty().id();
        let first = app
            .world_mut()
            .spawn(SelectionGroup(old_group))
            .id();
        let second = app
            .world_mut()
            .spawn(SelectionGroup(old_group))
            .id();
        let third = app.world_mut().spawn_empty().id();

        app.world_mut().write_message(SelectionAction::Group {
            targets: vec![second, third],
        });
        app.update();

        for entity in [first, second, third] {
            assert_eq!(
                app.world().entity(entity).get::<SelectionGroup>(),
                Some(&SelectionGroup(second))
            );
        }

        app.world_mut().write_message(SelectionAction::Ungroup {
            targets: vec![third],
        });
        app.update();

        for entity in [first, second, third] {
            assert!(!app.world().entity(entity).contains::<SelectionGroup>());
        }
    }

    #[test]
    fn ordering_actions_reorder_child_hierarchies_and_boxes() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TransformPlugin))
            .init_resource::<DepthSorter>()
            .init_schedule(EguiPrimaryContextPass);
        add_systems(&mut app);

        let scene = app.world_mut().spawn(Transform::default()).id();
        let first_box = spawn_selectable(&mut app, scene, 1.0);
        let spring = spawn_selectable(&mut app, scene, 2.0);
        let handle_a = spawn_selectable(&mut app, spring, 1.0);
        let handle_b = spawn_selectable(&mut app, spring, 2.0);
        let second_box = spawn_selectable(&mut app, scene, 5.0);

        app.update();
        app.world_mut().write_message(SelectionAction::Move {
            targets: vec![spring, handle_a, handle_b],
            to: ZOrder::Back,
        });
        app.update();

        assert!(global_z(&app, spring) < global_z(&app, handle_a));
        assert!(global_z(&app, handle_a) < global_z(&app, handle_b));
        assert!(global_z(&app, handle_b) < global_z(&app, first_box));
        assert!(global_z(&app, first_box) < global_z(&app, second_box));

        app.world_mut().write_message(SelectionAction::Move {
            targets: vec![first_box],
            to: ZOrder::Front,
        });
        app.update();

        assert!(global_z(&app, first_box) > global_z(&app, spring));
        assert!(global_z(&app, first_box) > global_z(&app, handle_a));
        assert!(global_z(&app, first_box) > global_z(&app, handle_b));
        assert!(global_z(&app, first_box) > global_z(&app, second_box));

        app.world_mut().write_message(SelectionAction::Move {
            targets: vec![second_box],
            to: ZOrder::Back,
        });
        app.update();

        assert!(global_z(&app, second_box) < global_z(&app, first_box));
        assert!(global_z(&app, second_box) < global_z(&app, spring));
    }

    #[test]
    fn camera_follow_preserves_manual_pan_while_tracking_body_pose() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TransformPlugin));
        app.init_resource::<CameraFollow>()
            .add_systems(Update, update_camera_follow);

        let body = app
            .world_mut()
            .spawn((
                RigidBody::Dynamic,
                Position(Vec2::new(2.0, 3.0)),
                Rotation::radians(0.25),
            ))
            .id();
        let camera = app
            .world_mut()
            .spawn((MainCamera, Transform::from_xyz(10.0, 20.0, 30.0)))
            .id();
        app.world_mut().resource_mut::<CameraFollow>().0 = Some(CameraFollowTarget {
            entity: body,
            track_rotation: true,
            last_position: None,
        });

        let screen_position_before_tracking = {
            let camera = app.world().entity(camera).get::<Transform>().unwrap();
            point_in_camera_space(camera, Vec2::new(2.0, 3.0))
        };
        app.update();
        let camera_after_tracking = app.world().entity(camera).get::<Transform>().unwrap();
        assert_vec2_close(
            point_in_camera_space(camera_after_tracking, Vec2::new(2.0, 3.0)),
            screen_position_before_tracking,
        );

        app.world_mut()
            .entity_mut(camera)
            .get_mut::<Transform>()
            .unwrap()
            .translation
            .x += 4.0;
        let screen_position_after_pan = {
            let camera = app.world().entity(camera).get::<Transform>().unwrap();
            point_in_camera_space(camera, Vec2::new(2.0, 3.0))
        };
        app.world_mut()
            .entity_mut(body)
            .get_mut::<Position>()
            .unwrap()
            .0 = Vec2::new(5.0, 1.0);
        *app.world_mut()
            .entity_mut(body)
            .get_mut::<Rotation>()
            .unwrap() = Rotation::radians(0.75);
        app.update();

        let camera = app.world().entity(camera).get::<Transform>().unwrap();
        assert_eq!(camera.translation.z, 30.0);
        assert_vec2_close(
            point_in_camera_space(camera, Vec2::new(5.0, 1.0)),
            screen_position_after_pan,
        );
        assert!((camera.rotation.to_euler(EulerRot::XYZ).2 - 0.75).abs() < 1.0e-6);
    }

    fn point_in_camera_space(camera: &Transform, point: Vec2) -> Vec2 {
        let offset = Vec3::new(
            point.x - camera.translation.x,
            point.y - camera.translation.y,
            0.0,
        );
        let local = camera.rotation.inverse() * offset;
        Vec2::new(local.x, local.y)
    }

    fn assert_vec2_close(actual: Vec2, expected: Vec2) {
        assert!(
            actual.distance(expected) < 1.0e-5,
            "expected {expected:?}, got {actual:?}"
        );
    }
}
