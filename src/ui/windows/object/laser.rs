use crate::objects::laser::LaserBundle;
use crate::ui::{BevyIdThing, InitialPos, Subwindow};
use crate::SizeComponent;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContext};

#[derive(Default, Component)]
pub struct LaserWindow;

impl LaserWindow {
    pub fn show(
        mut wnds: Query<(Entity, &Parent, &mut InitialPos), With<LaserWindow>>,
        mut ents: Query<(&mut LaserBundle, &mut SizeComponent)>,
        mut egui_ctx: ResMut<EguiContext>,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut();
        for (id, parent, mut initial_pos) in wnds.iter_mut() {
            let (mut laser, mut size) = ents.get_mut(parent.get()).unwrap();
            egui::Window::new("Laser pens")
                .resizable(false)
                .default_size(egui::Vec2::ZERO)
                .id_bevy(id)
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