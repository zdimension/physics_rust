use crate::egui_systems;
use crate::objects::laser::LaserSettings;
use crate::ui::{
    InitialPos, Subwindow, WindowSelectionTarget, component_slider_mut,
    window_matching_entities,
};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

egui_systems!(LaserWindow::show);

#[derive(Default, Component)]
pub struct LaserWindow;

impl LaserWindow {
    pub fn show(
        mut wnds: Query<
            (
                Entity,
                Option<&ChildOf>,
                Option<&WindowSelectionTarget>,
                &mut InitialPos,
            ),
            With<LaserWindow>,
        >,
        mut ents: Query<&mut LaserSettings>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut().expect("primary egui context");
        for (id, parent, target, mut initial_pos) in wnds.iter_mut() {
            let targets = window_matching_entities(target, parent, &ents);
            if targets.is_empty() {
                commands.entity(id).despawn();
                continue;
            }
            egui::Window::new("Laser pens")
                .resizable(false)
                .default_size(egui::Vec2::ZERO)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, _commands| {
                    component_slider_mut(
                        ui,
                        &targets,
                        &mut ents,
                        |settings| settings.fade_distance,
                        |settings, value| settings.fade_distance = value,
                        1.0..=1000.0,
                        |slider| slider.logarithmic(true).suffix("m").text("Fade distance :"),
                    );
                    component_slider_mut(
                        ui,
                        &targets,
                        &mut ents,
                        |settings| settings.size,
                        |settings, value| settings.size = value,
                        0.01..=5.0,
                        |slider| slider.logarithmic(true).suffix("m").text("Size :"),
                    );
                });
        }
    }
}
