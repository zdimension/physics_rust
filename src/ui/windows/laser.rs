use bevy::prelude::*;
use bevy_egui::{egui, EguiContext};
use bevy_egui::egui::Style;
use crate::LaserBundle;
use crate::ui::{BevyIdThing, InitialPos, Subwindow};

#[derive(Default, Component)]
pub struct LaserWindow;

impl LaserWindow {
    pub fn show(
        mut wnds: Query<(Entity, &Parent, &mut InitialPos), With<LaserWindow>>,
        mut ents: Query<&mut LaserBundle>,
        mut egui_ctx: ResMut<EguiContext>,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut();
        for (id, parent, mut initial_pos) in wnds.iter_mut() {
            let mut groups = ents.get_mut(parent.get()).unwrap();
            egui::Window::new("Laser pens")
                .resizable(false)
                .id_bevy(id)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, _commands| {
                    ui.group(|ui| {
                        ui.add(egui::Slider::new(&mut groups.fade_distance, 1.0..=1000.0)
                            .logarithmic(true)
                            .suffix("m")
                            .text("Fade distance")
                        );
                    });
                });
        }
    }
}
