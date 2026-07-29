use crate::tools::add_object::DepthSorter;
use crate::mouse_tracking::MainCamera;
use crate::ui::images::GuiIcons;
use crate::ui::menu_item::MenuItem;
use crate::ui::{InitialPos, Selected, Subwindow, WindowSelectionTarget, bool_checkbox};
use avian2d::prelude::*;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};
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
        physical_objects: Query<(), With<RigidBody>>,
        gui_icons: Res<GuiIcons>,
        mut camera_follow: ResMut<CameraFollow>,
    ) {
        let ctx = egui_ctx.ctx_mut().expect("primary egui context");
        for (id, target, mut initial_pos) in wnds.iter_mut() {
            egui::Window::new("Select")
                .default_size(egui::Vec2::ZERO)
                .resizable(false)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, _commands| {
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
    mut depth: ResMut<DepthSorter>,
    mut commands: Commands,
) {
    for action in actions.read() {
        match action {
            SelectionAction::Invert => invert_selection(&selected, &selectable, &mut commands),
            SelectionAction::DeselectAll => deselect_everything(&selected, &mut commands),
            SelectionAction::Move { targets, to } => move_selected_z(
                targets,
                *to,
                &selectable,
                &mut transforms,
                &globals,
                &mut mesh_materials,
                &mut depth,
            ),
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
