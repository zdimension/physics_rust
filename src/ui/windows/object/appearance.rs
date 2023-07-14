use crate::objects::ColorComponent;
use crate::ui::{BevyIdThing, InitialPos, Subwindow};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use crate::systems;

systems!(AppearanceWindow::show);

#[derive(Default, Component)]
pub struct AppearanceWindow;

impl AppearanceWindow {
    pub fn show(
        mut wnds: Query<(Entity, &Parent, &mut InitialPos), With<AppearanceWindow>>,
        mut ents: Query<&mut ColorComponent>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut();
        for (id, parent, mut initial_pos) in wnds.iter_mut() {
            let mut color = ents.get_mut(parent.get()).unwrap();
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
                        color.0 = hsva;
                    }
                });
        }
    }
}
