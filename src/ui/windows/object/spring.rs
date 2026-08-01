use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::egui_systems;
use crate::objects::spring::SpringObject;
use crate::ui::{
    InitialPos, Subwindow, WindowSelectionTarget, component_slider, max_f32,
    window_matching_entities,
};

egui_systems!(SpringWindow::show);

#[derive(Default, Component)]
pub struct SpringWindow {
    ranges: Option<SpringSliderRanges>,
}

#[derive(Copy, Clone)]
struct SpringSliderRanges {
    spring_constant: f32,
    target_length: f32,
}

impl SpringWindow {
    pub fn show(
        mut wnds: Query<
            (
                Entity,
                Option<&ChildOf>,
                Option<&WindowSelectionTarget>,
                &mut SpringWindow,
                &mut InitialPos,
            ),
            With<SpringWindow>,
        >,
        springs: Query<&SpringObject>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut().expect("primary egui context");
        for (id, parent, target, mut spring_wnd, mut initial_pos) in wnds.iter_mut() {
            let spring_targets = window_matching_entities(target, parent, &springs);
            if spring_targets.is_empty() {
                commands.entity(id).despawn();
                continue;
            }
            let ranges = *spring_wnd.ranges.get_or_insert_with(|| SpringSliderRanges {
                spring_constant: max_f32(&spring_targets, &springs, |spring| {
                    spring.spring_constant
                })
                .unwrap_or(0.0),
                target_length: max_f32(&spring_targets, &springs, |spring| spring.target_length)
                    .unwrap_or(0.0)
                    .max(1.0),
            });

            egui::Window::new("Springs")
                .auto_sized()
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, commands| {
                    component_slider(
                        ui,
                        commands,
                        &spring_targets,
                        &springs,
                        |spring| spring.spring_constant,
                        |spring, value| spring.spring_constant = value,
                        0.0..=(ranges.spring_constant * 100.0).max(10800.0),
                        |slider| {
                            slider
                                .logarithmic(true)
                                .smallest_positive((ranges.spring_constant / 100.0) as f64)
                                .suffix("N/m")
                                .text("Spring constant :")
                        },
                    );
                    component_slider(
                        ui,
                        commands,
                        &spring_targets,
                        &springs,
                        |spring| spring.damping,
                        |spring, value| spring.damping = value,
                        0.0..=2.0,
                        |slider| slider.text("Damping :"),
                    );
                    component_slider(
                        ui,
                        commands,
                        &spring_targets,
                        &springs,
                        |spring| spring.target_length,
                        |spring, value| spring.target_length = value,
                        (ranges.target_length / 10.0)..=(ranges.target_length * 10.0),
                        |slider| slider.logarithmic(true).suffix("m").text("Target length :"),
                    );
                });
        }
    }
}
