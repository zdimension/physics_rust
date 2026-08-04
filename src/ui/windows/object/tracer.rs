use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::egui_systems;
use crate::objects::tracer::{TracerObject, TracerSettings};
use crate::ui::{
    InitialPos, Subwindow, WindowSelectionTarget, component_slider_mut,
    window_matching_entities, window_title,
};

egui_systems!(TracerWindow::show);

#[derive(Default, Component)]
pub struct TracerWindow;

impl TracerWindow {
    pub fn show(
        mut wnds: Query<
            (
                Entity,
                Option<&ChildOf>,
                Option<&WindowSelectionTarget>,
                &mut InitialPos,
            ),
            With<TracerWindow>,
        >,
        mut settings: Query<&mut TracerSettings>,
        mut tracers: Query<&mut TracerObject>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut().expect("primary egui context");
        for (id, parent, target, mut initial_pos) in wnds.iter_mut() {
            let targets = window_matching_entities(target, parent, &settings);
            if targets.is_empty() {
                commands.entity(id).despawn();
                continue;
            }

            egui::Window::new(window_title(target, "Tracer"))
                .auto_sized()
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, _commands| {
                    component_slider_mut(
                        ui,
                        &targets,
                        &mut settings,
                        |settings| settings.diameter,
                        |settings, value| settings.diameter = value,
                        0.01..=5.0,
                        |slider| slider.logarithmic(true).suffix("m").text("Diameter :"),
                    );
                    component_slider_mut(
                        ui,
                        &targets,
                        &mut settings,
                        |settings| settings.fade_time,
                        |settings, value| settings.fade_time = value,
                        0.05..=60.0,
                        |slider| slider.logarithmic(true).suffix("s").text("Fade time :"),
                    );
                    if ui.button("Clear trail").clicked() {
                        for entity in &targets {
                            if let Ok(mut tracer) = tracers.get_mut(*entity) {
                                tracer.clear_trail();
                            }
                        }
                    }
                });
        }
    }
}
