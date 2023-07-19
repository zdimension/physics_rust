use crate::objects::phy_obj::RefractiveIndex;
use crate::ui::{InitialPos, Subwindow};
use bevy::prelude::{Commands, Component, Entity, Parent, Query, With};
use bevy_egui::{egui, EguiContexts};
use bevy_xpbd_2d::{math::*, prelude::*};
use crate::{add_slider, systems, update_changed};
use crate::UpdateStatus::Changed;

systems!(MaterialWindow::show);

#[derive(Default, Component)]
pub struct MaterialWindow;

impl MaterialWindow {
    pub fn show(
        mut wnds: Query<(Entity, &Parent, &mut InitialPos), With<MaterialWindow>>,
        mut ents: Query<(&mut Restitution, &mut RefractiveIndex, &mut Friction)>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut();
        for (id, parent, mut initial_pos) in wnds.iter_mut() {
            let (mut restitution, mut refractive, mut friction) =
                ents.get_mut(parent.get()).unwrap();
            egui::Window::new("Material")
                .resizable(false)
                .default_size(egui::Vec2::ZERO)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, _commands| {
                    update_changed!(ui, friction.static_coefficient, 0.0..=2.0, |slider| {
                        slider.text("Static friction :").custom()
                    });

                    update_changed!(ui, friction.dynamic_coefficient, 0.0..=2.0, |slider| {
                        slider.text("Dynamic friction :").custom()
                    });

                    update_changed!(ui, restitution.coefficient, 0.0..=1.0, |slider| {
                        slider.text("Restitution :").custom()
                    });

                   update_changed!(ui, refractive.0, 1.0..=f32::INFINITY, |slider| {
                        slider
                            .logarithmic(true)
                            .largest_finite(100.0)
                       .text("Refractive index :")
                       .custom()
                    });
                });
        }
    }
}
