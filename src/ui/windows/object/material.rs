use crate::egui_systems;
use crate::objects::attraction::{Attraction, AttractionFalloff};
use crate::objects::laser::LaserSettings;
use crate::objects::phy_obj::RefractiveIndex;
use crate::ui::images::GuiIcons;
use crate::ui::{
    InitialPos, Subwindow, WindowSelectionTarget, component_slider, edit_components, image_radio,
    window_matching_entities,
};
use avian2d::prelude::*;
use bevy::prelude::{ChildOf, Commands, Component, Entity, Query, Res, With};
use bevy_egui::{EguiContexts, egui};

egui_systems!(MaterialWindow::show);

#[derive(Default, Component)]
#[require(MaterialWindowState)]
pub struct MaterialWindow;

#[derive(Default, Component)]
pub(crate) struct MaterialWindowState {
    show_refractive_index: Option<bool>,
}

impl MaterialWindowState {
    fn show_refractive_index(&mut self, lasers_exist: bool) -> bool {
        *self.show_refractive_index.get_or_insert(lasers_exist)
    }
}

impl MaterialWindow {
    pub fn show(
        mut wnds: Query<
            (
                Entity,
                Option<&ChildOf>,
                Option<&WindowSelectionTarget>,
                &mut InitialPos,
                &mut MaterialWindowState,
            ),
            With<MaterialWindow>,
        >,
        frictions: Query<&Friction>,
        restitutions: Query<&Restitution>,
        refractive_indices: Query<&RefractiveIndex>,
        attractions: Query<&Attraction>,
        lasers: Query<(), With<LaserSettings>>,
        gui_icons: Res<GuiIcons>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut().expect("primary egui context");
        for (id, parent, target, mut initial_pos, mut state) in wnds.iter_mut() {
            let targets = window_matching_entities(target, parent, &frictions);
            if targets.is_empty() {
                commands.entity(id).despawn();
                continue;
            }
            let show_refractive_index = state.show_refractive_index(!lasers.is_empty());
            let attraction_targets = targets
                .iter()
                .copied()
                .filter(|entity| attractions.contains(*entity))
                .collect::<Vec<_>>();
            let shared_falloff = attraction_targets
                .first()
                .and_then(|entity| attractions.get(*entity).ok())
                .map(|attraction| attraction.falloff)
                .filter(|falloff| {
                    attraction_targets.iter().all(|entity| {
                        attractions
                            .get(*entity)
                            .is_ok_and(|attraction| attraction.falloff == *falloff)
                    })
                });

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
                    if show_refractive_index {
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
                    }

                    if !attraction_targets.is_empty() {
                        let units = match shared_falloff {
                            Some(AttractionFalloff::Linear) => " N·m/kg²",
                            Some(AttractionFalloff::Quadratic) => " N·m²/kg²",
                            None => "",
                        };
                        component_slider(
                            ui,
                            commands,
                            &attraction_targets,
                            &attractions,
                            |attraction| attraction.strength,
                            |attraction, value| attraction.strength = value,
                            0.0..=1.0e12,
                            |slider| {
                                slider
                                    .logarithmic(true)
                                    .smallest_positive(1.0e-12)
                                    .suffix(units)
                                    .text("Attraction:")
                            },
                        );

                        ui.horizontal(|ui| {
                            ui.label("Falloff:");
                            if image_radio(
                                ui,
                                &gui_icons,
                                shared_falloff == Some(AttractionFalloff::Linear),
                                "Linear",
                            ) {
                                edit_components(
                                    commands,
                                    &attraction_targets,
                                    &attractions,
                                    |attraction| {
                                        attraction.falloff = AttractionFalloff::Linear
                                    },
                                );
                            }
                            if image_radio(
                                ui,
                                &gui_icons,
                                shared_falloff == Some(AttractionFalloff::Quadratic),
                                "Quadratic",
                            ) {
                                edit_components(
                                    commands,
                                    &attraction_targets,
                                    &attractions,
                                    |attraction| {
                                        attraction.falloff = AttractionFalloff::Quadratic
                                    },
                                );
                            }
                        });
                    }
                });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refractive_index_visibility_is_decided_only_when_window_first_opens() {
        let mut opened_without_lasers = MaterialWindowState::default();
        assert!(!opened_without_lasers.show_refractive_index(false));
        assert!(!opened_without_lasers.show_refractive_index(true));

        let mut opened_with_lasers = MaterialWindowState::default();
        assert!(opened_with_lasers.show_refractive_index(true));
        assert!(opened_with_lasers.show_refractive_index(false));
    }
}
