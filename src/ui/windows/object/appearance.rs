use crate::egui_systems;
use crate::objects::ColorComponent;
use crate::ui::{InitialPos, Subwindow, WindowSelectionTarget, window_matching_entities};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

egui_systems!(AppearanceWindow::show);

#[derive(Default, Component)]
pub struct AppearanceWindow;

impl AppearanceWindow {
    pub fn show(
        mut wnds: Query<
            (
                Entity,
                Option<&ChildOf>,
                Option<&WindowSelectionTarget>,
                &mut InitialPos,
            ),
            With<AppearanceWindow>,
        >,
        ents: Query<&ColorComponent>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut().expect("primary egui context");
        for (id, parent, target, mut initial_pos) in wnds.iter_mut() {
            let targets = window_matching_entities(target, parent, &ents);
            let Some(color) = targets.iter().find_map(|entity| ents.get(*entity).ok()) else {
                commands.entity(id).despawn();
                continue;
            };
            egui::Window::new("Appearance")
                .resizable(false)
                .default_size(egui::Vec2::ZERO)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, _commands| {
                    let mut hsva = color.0;
                    if egui::color_picker::color_picker_hsva_2d(
                        ui,
                        &mut hsva,
                        egui::color_picker::Alpha::OnlyBlend,
                    ) {
                        for entity in &targets {
                            _commands.entity(*entity).insert(ColorComponent(hsva));
                        }
                    }
                });
        }
    }
}
