use crate::egui_systems;
use crate::tools::add_object::DepthSorter;
use crate::ui::menu_item::MenuItem;
use crate::ui::{InitialPos, Subwindow, WindowSelectionTarget, window_target_entities};
use avian2d::prelude::*;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

#[derive(Default, Component)]
pub struct SelectionWindow;

egui_systems!(SelectionWindow::show);

#[derive(Clone, Copy)]
enum ZOrderAction {
    Back,
    Front,
}

type SelectableObjectFilter = With<Collider>;

impl SelectionWindow {
    pub fn show(
        mut wnds: Query<
            (
                Entity,
                Option<&ChildOf>,
                Option<&WindowSelectionTarget>,
                &mut InitialPos,
            ),
            With<SelectionWindow>,
        >,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
        mut objects: ParamSet<(
            Query<(Entity, &GlobalTransform), SelectableObjectFilter>,
            Query<(&mut Transform, Option<&ChildOf>)>,
            Query<&GlobalTransform>,
        )>,
        mut depth: ResMut<DepthSorter>,
    ) {
        let ctx = egui_ctx.ctx_mut().expect("primary egui context");
        for (id, parent, target, mut initial_pos) in wnds.iter_mut() {
            let selected = window_target_entities(target, parent);
            egui::Window::new("Selection")
                .default_size(egui::Vec2::ZERO)
                .resizable(false)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, _commands| {
                    if ui
                        .add(MenuItem::button(None, "Move selected to back"))
                        .clicked()
                    {
                        move_selected_z(&selected, ZOrderAction::Back, &mut objects, &mut depth);
                    }
                    if ui
                        .add(MenuItem::button(None, "Move selected to front"))
                        .clicked()
                    {
                        move_selected_z(&selected, ZOrderAction::Front, &mut objects, &mut depth);
                    }
                });
        }
    }
}

fn move_selected_z(
    selected: &[Entity],
    action: ZOrderAction,
    objects: &mut ParamSet<(
        Query<(Entity, &GlobalTransform), SelectableObjectFilter>,
        Query<(&mut Transform, Option<&ChildOf>)>,
        Query<&GlobalTransform>,
    )>,
    depth: &mut DepthSorter,
) {
    let target_z = {
        let objects = objects.p0();
        let other_zs = objects
            .iter()
            .filter_map(|(entity, transform)| {
                (!selected.contains(&entity)).then_some(transform.translation().z)
            });

        match action {
            ZOrderAction::Back => other_zs.reduce(f32::min).map(|z| z - 1.0),
            ZOrderAction::Front => other_zs.reduce(f32::max).map(|z| z + 1.0),
        }
    };

    let Some(target_z) = target_z else {
        return;
    };

    let Some(first_selected) = selected.first().copied() else {
        return;
    };

    let parent_entity = {
        let transforms = objects.p1();
        let Ok((_, parent)) = transforms.get(first_selected) else {
            return;
        };
        parent.map(ChildOf::parent)
    };

    let parent_z = parent_entity
        .and_then(|parent| {
            objects
                .p2()
                .get(parent)
                .ok()
                .map(|transform| transform.translation().z)
        })
        .unwrap_or(0.0);

    {
        let mut transforms = objects.p1();
        for (offset, entity) in selected.iter().copied().enumerate() {
            let Ok((mut transform, _)) = transforms.get_mut(entity) else {
                continue;
            };
            let offset = offset as f32;
            transform.translation.z = match action {
                ZOrderAction::Back => target_z - parent_z - offset,
                ZOrderAction::Front => target_z - parent_z + offset,
            };
        }
    }

    if matches!(action, ZOrderAction::Front) {
        depth.include(target_z);
    }
}
