use crate::objects::phy_obj::RefractiveIndex;
use crate::ui::{BevyIdThing, InitialPos, Subwindow};
use bevy::prelude::{Commands, Component, Entity, Parent, Query, ResMut, With};
use bevy_egui::{egui, EguiContext};
use bevy_rapier2d::prelude::{Friction, Restitution};

#[derive(Default, Component)]
pub struct MaterialWindow;

impl MaterialWindow {
    pub fn show(
        mut wnds: Query<(Entity, &Parent, &mut InitialPos), With<MaterialWindow>>,
        mut ents: Query<(&mut Restitution, &mut RefractiveIndex, &mut Friction)>,
        mut egui_ctx: ResMut<EguiContext>,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut();
        for (id, parent, mut initial_pos) in wnds.iter_mut() {
            let (mut restitution, mut refractive, mut friction) =
                ents.get_mut(parent.get()).unwrap();
            egui::Window::new("Material")
                .resizable(false)
                .default_size(egui::Vec2::ZERO)
                .id_bevy(id)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, _commands| {
                    ui.add(
                        egui::Slider::new(&mut friction.coefficient, 0.0..=2.0)
                            .text("Friction :")
                            .custom(),
                    );

                    ui.add(
                        egui::Slider::new(&mut restitution.coefficient, 0.0..=1.0)
                            .text("Restitution :")
                            .custom(),
                    );

                    ui.add(
                        egui::Slider::new(&mut refractive.0, 1.0..=f32::INFINITY)
                            .logarithmic(true)
                            .largest_finite(100.0)
                            .text("Refractive index :")
                            .custom(),
                    );
                });
        }
    }
}
