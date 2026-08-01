use crate::egui_systems;
use crate::objects::attraction::{Attraction, AttractionFalloff};
use crate::objects::laser::LaserSettings;
use crate::objects::plane::PlaneObject;
use crate::objects::phy_obj::{FrictionModel, RefractiveIndex};
use crate::ui::images::GuiIcons;
use crate::ui::{
    InitialPos, Subwindow, WindowSelectionTarget, component_slider, edit_components, image_radio,
    shared_value, window_matching_entities,
};
use avian2d::prelude::*;
use bevy::prelude::{ChildOf, Commands, Component, Entity, Query, Res, With, Without};
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

fn normalize_simple_friction(friction: &mut Friction) {
    friction.dynamic_coefficient = friction.static_coefficient;
}

fn set_simple_friction(friction: &mut Friction, value: f32) {
    friction.static_coefficient = value;
    friction.dynamic_coefficient = value;
}

fn density_for_mass(collider: &Collider, mass: f32) -> ColliderDensity {
    let unit_density_mass = ColliderMassProperties::from_shape(collider, 1.0).mass;
    ColliderDensity(mass.max(0.0) / unit_density_mass)
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
        friction_models: Query<&FrictionModel>,
        densities: Query<&ColliderDensity, Without<PlaneObject>>,
        colliders: Query<&Collider, Without<PlaneObject>>,
        mass_properties: Query<&ColliderMassProperties, Without<PlaneObject>>,
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
            let shared_friction_model = shared_value(targets.iter().filter_map(|entity| {
                friction_models.get(*entity).ok().copied()
            }));
            let density_targets = targets
                .iter()
                .copied()
                .filter(|entity| densities.contains(*entity))
                .collect::<Vec<_>>();
            let attraction_targets = targets
                .iter()
                .copied()
                .filter(|entity| attractions.contains(*entity))
                .collect::<Vec<_>>();
            let shared_falloff = shared_value(attraction_targets.iter().filter_map(|entity| {
                attractions
                    .get(*entity)
                    .ok()
                    .map(|attraction| attraction.falloff)
            }));

            egui::Window::new("Material")
                .auto_sized()
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, commands| {
                    if !density_targets.is_empty() {
                        component_slider(
                            ui,
                            commands,
                            &density_targets,
                            &densities,
                            |density| density.0,
                            |density, value| density.0 = value.max(0.0),
                            0.001..=100.0,
                            |slider| {
                                slider
                                    .logarithmic(true)
                                                                        .suffix(" kg/m²")
                                    .text("Density:")
                            },
                        );
                    }

                    if targets.len() == 1 {
                        let entity = targets[0];
                        if let (Ok(collider), Ok(properties)) =
                            (colliders.get(entity), mass_properties.get(entity))
                        {
                            let mut mass = properties.mass;
                            if ui
                                .add(
                                    egui::Slider::new(&mut mass, 0.001..=1_000.0)
                                        .logarithmic(true)
                                                                                .suffix(" kg")
                                        .text("Mass:")
                                        .custom(),
                                )
                                .changed()
                            {
                                commands
                                    .entity(entity)
                                    .insert(density_for_mass(collider, mass));
                            }
                        }
                    }

                    ui.horizontal(|ui| {
                        ui.label("Friction model:");
                        if image_radio(
                            ui,
                            &gui_icons,
                            shared_friction_model == Some(FrictionModel::Simple),
                            "Simple",
                        ) {
                            edit_components(
                                commands,
                                &targets,
                                &friction_models,
                                |model| *model = FrictionModel::Simple,
                            );
                            edit_components(
                                commands,
                                &targets,
                                &frictions,
                                normalize_simple_friction,
                            );
                        }
                        if image_radio(
                            ui,
                            &gui_icons,
                            shared_friction_model == Some(FrictionModel::Advanced),
                            "Advanced",
                        ) {
                            edit_components(
                                commands,
                                &targets,
                                &friction_models,
                                |model| *model = FrictionModel::Advanced,
                            );
                        }
                    });

                    if shared_friction_model == Some(FrictionModel::Simple) {
                        component_slider(
                            ui,
                            commands,
                            &targets,
                            &frictions,
                            |friction| friction.static_coefficient,
                            set_simple_friction,
                            0.0..=2.0,
                            |slider| slider.text("Friction:"),
                        );
                    } else {
                        component_slider(
                            ui,
                            commands,
                            &targets,
                            &frictions,
                            |friction| friction.static_coefficient,
                            |friction, value| friction.static_coefficient = value,
                            0.0..=2.0,
                            |slider| slider.text("Static friction:"),
                        );
                        component_slider(
                            ui,
                            commands,
                            &targets,
                            &frictions,
                            |friction| friction.dynamic_coefficient,
                            |friction, value| friction.dynamic_coefficient = value,
                            0.0..=2.0,
                            |slider| slider.text("Dynamic friction:"),
                        );
                    }
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

    #[test]
    fn friction_model_defaults_to_simple_and_keeps_coefficients_together() {
        assert_eq!(FrictionModel::default(), FrictionModel::Simple);

        let mut friction = Friction::default();
        friction.static_coefficient = 0.8;
        friction.dynamic_coefficient = 0.2;
        normalize_simple_friction(&mut friction);
        assert_eq!(friction.dynamic_coefficient, 0.8);

        set_simple_friction(&mut friction, 1.25);
        assert_eq!(friction.static_coefficient, 1.25);
        assert_eq!(friction.dynamic_coefficient, 1.25);
    }

    #[test]
    fn setting_mass_converts_it_to_collider_density() {
        let collider = Collider::rectangle(2.0, 3.0);

        let density = density_for_mass(&collider, 12.0);

        assert!((density.0 - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn negative_mass_is_clamped_to_zero() {
        let collider = Collider::rectangle(2.0, 3.0);

        assert_eq!(density_for_mass(&collider, -12.0), ColliderDensity::ZERO);
    }
}
