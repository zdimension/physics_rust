use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::egui_systems;
use crate::objects::tracer::TracerObject;
use crate::objects::SizeComponent;
use crate::ui::{InitialPos, Subwindow};

egui_systems!(TracerWindow::show);

#[derive(Default, Component)]
pub struct TracerWindow;

impl TracerWindow {
    pub fn show(
        mut wnds: Query<(Entity, &ChildOf, &mut InitialPos), With<TracerWindow>>,
        mut ents: Query<(&mut TracerObject, &mut SizeComponent)>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut().expect("primary egui context");
        for (id, parent, mut initial_pos) in wnds.iter_mut() {
            let Ok((mut tracer, mut size)) = ents.get_mut(parent.parent()) else {
                commands.entity(id).despawn();
                continue;
            };
            egui::Window::new("Tracer")
                .resizable(false)
                .default_size(egui::Vec2::ZERO)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, _commands| {
                    ui.add(
                        egui::Slider::new(&mut size.0, 0.01..=5.0)
                            .logarithmic(true)
                            .suffix("m")
                            .text("Diameter :")
                            .custom(),
                    );
                    ui.add(
                        egui::Slider::new(&mut tracer.fade_time, 0.05..=60.0)
                            .logarithmic(true)
                            .suffix("s")
                            .text("Fade time :")
                            .custom(),
                    );
                });
        }
    }
}
