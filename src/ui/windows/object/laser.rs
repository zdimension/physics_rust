use crate::objects::laser::LaserBundle;
use crate::objects::SizeComponent;
use crate::ui::{InitialPos, Subwindow};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use crate::systems;

systems!(LaserWindow::show);

#[derive(Default, Component)]
pub struct LaserWindow;

impl LaserWindow {
    pub fn show(
        mut wnds: Query<(Entity, &Parent, &mut InitialPos), With<LaserWindow>>,
        mut ents: Query<(&mut LaserBundle, &mut SizeComponent)>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut();
        for (id, parent, mut initial_pos) in wnds.iter_mut() {
            let (mut laser, mut size) = ents.get_mut(parent.get()).unwrap();
            egui::Window::new("Laser pens")
                .resizable(false)
                .default_size(egui::Vec2::ZERO)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, _commands| {
                    ui.add(
                        egui::Slider::new(&mut laser.fade_distance, 1.0..=1000.0)
                            .logarithmic(true)
                            .suffix("m")
                            .text("Fade distance :")
                            .custom(),
                    );
                    ui.add(
                        egui::Slider::new(&mut size.0, 0.01..=5.0)
                            .logarithmic(true)
                            .suffix("m")
                            .text("Size :")
                            .custom(),
                    );
                });
        }
    }
}
