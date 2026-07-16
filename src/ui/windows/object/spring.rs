use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::egui_systems;
use crate::objects::spring::{SpringEndHandle, SpringObject};
use crate::ui::{InitialPos, Subwindow};

egui_systems!(SpringWindow::show);

#[derive(Default, Component)]
pub struct SpringWindow;

impl SpringWindow {
    pub fn show(
        mut wnds: Query<(Entity, &ChildOf, &mut InitialPos), With<SpringWindow>>,
        spring_ends: Query<&SpringEndHandle, Without<SpringObject>>,
        mut springs: Query<&mut SpringObject>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut().expect("primary egui context");
        for (id, parent, mut initial_pos) in wnds.iter_mut() {
            let selected_entity = parent.parent();
            let spring_entity = spring_ends
                .get(selected_entity)
                .map_or(selected_entity, |handle| handle.spring);
            let Ok(mut spring) = springs.get_mut(spring_entity) else {
                commands.entity(id).despawn();
                continue;
            };

            egui::Window::new("Springs")
                .resizable(false)
                .default_size(egui::Vec2::ZERO)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, _commands| {
                    let orig_spring_constant = spring.spring_constant;
                    ui.add(
                        egui::Slider::new(&mut spring.spring_constant, 0.0..=(orig_spring_constant * 100.0).max(10800.0))
                            .logarithmic(true)
                            .smallest_positive((orig_spring_constant / 100.0) as f64)
                            .suffix("N/m")
                            .text("Spring constant :")
                            .custom(),
                    );
                    ui.add(
                        egui::Slider::new(&mut spring.damping, 0.0..=2.0)
                            .text("Damping :")
                            .custom(),
                    );
                    let orig_length = if spring.target_length == 0.0 { 1.0 } else { spring.target_length };
                    ui.add(
                        egui::Slider::new(&mut spring.target_length, (orig_length / 10.0)..=(orig_length * 10.0))
                            .logarithmic(true)
                            .suffix("m")
                            .text("Target length :")
                            .custom(),
                    );
                });
        }
    }
}
