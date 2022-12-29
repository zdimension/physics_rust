use bevy::prelude::*;
use bevy::sprite::Anchor::Custom;
use bevy_egui::{egui, EguiContext};
use bevy_egui::egui::{emath, Style};
use crate::LaserBundle;
use crate::ui::{BevyIdThing, InitialPos, Subwindow};
use crate::ui::custom_slider::CustomSlider;

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
                .default_size(egui::Vec2::ZERO)
                .id_bevy(id)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, _commands| {
                    ui.add(egui::Slider::new(&mut groups.fade_distance, 1.0..=1000.0)
                        .logarithmic(true)
                        .suffix("m")
                        .text("Fade distance :")
                        .custom()
                    );
                });
        }
    }
}
