use crate::egui_systems;
use crate::objects::phy_obj::RefractiveIndex;
use crate::ui::{
    InitialPos, Subwindow, WindowSelectionTarget, component_slider, window_matching_entities,
};
use avian2d::prelude::*;
use bevy::prelude::{ChildOf, Commands, Component, Entity, Query, With};
use bevy_egui::{EguiContexts, egui};

egui_systems!(MaterialWindow::show);

#[derive(Default, Component)]
pub struct MaterialWindow;

impl MaterialWindow {
    pub fn show(
        mut wnds: Query<
            (
                Entity,
                Option<&ChildOf>,
                Option<&WindowSelectionTarget>,
                &mut InitialPos,
            ),
            With<MaterialWindow>,
        >,
        frictions: Query<&Friction>,
        restitutions: Query<&Restitution>,
        refractive_indices: Query<&RefractiveIndex>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut().expect("primary egui context");
        for (id, parent, target, mut initial_pos) in wnds.iter_mut() {
            let targets = window_matching_entities(target, parent, &frictions);
            if targets.is_empty() {
                commands.entity(id).despawn();
                continue;
            }

            egui::Window::new("Material")
                .resizable(false)
                .default_size(egui::Vec2::ZERO)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, commands| {
                    component_slider(
                        ui,
                        commands,
                        &targets,
                        &frictions,
                        |friction| friction.static_coefficient,
                        |friction, value| friction.static_coefficient = value,
                        0.0..=2.0,
                        |slider| slider.text("Static friction :"),
                    );
                    component_slider(
                        ui,
                        commands,
                        &targets,
                        &frictions,
                        |friction| friction.dynamic_coefficient,
                        |friction, value| friction.dynamic_coefficient = value,
                        0.0..=2.0,
                        |slider| slider.text("Dynamic friction :"),
                    );
                    component_slider(
                        ui,
                        commands,
                        &targets,
                        &restitutions,
                        |restitution| restitution.coefficient,
                        |restitution, value| restitution.coefficient = value,
                        0.0..=1.0,
                        |slider| slider.text("Restitution :"),
                    );
                    component_slider(
                        ui,
                        commands,
                        &targets,
                        &refractive_indices,
                        |refractive| refractive.0,
                        |refractive, value| refractive.0 = value,
                        1.0..=f32::INFINITY,
                        |slider| {
                            slider
                                .logarithmic(true)
                                .largest_finite(100.0)
                                .text("Refractive index :")
                        },
                    );
                });
        }
    }
}
