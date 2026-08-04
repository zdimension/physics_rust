use crate::egui_systems;
use crate::objects::MotorComponent;
use crate::ui::images::GuiIcons;
use crate::ui::{
    InitialPos, Subwindow, TriState, WindowSelectionTarget, component_checkbox, component_slider,
    window_matching_entities, window_title,
};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

egui_systems!(AxleWindow::show);

#[derive(Default, Component)]
pub struct AxleWindow;

impl AxleWindow {
    pub fn show(
        mut wnds: Query<
            (
                Entity,
                Option<&ChildOf>,
                Option<&WindowSelectionTarget>,
                &mut InitialPos,
            ),
            With<AxleWindow>,
        >,
        ents: Query<&MotorComponent>,
        gui_icons: Res<GuiIcons>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut().expect("primary egui context");
        for (id, parent, target, mut initial_pos) in wnds.iter_mut() {
            let targets = window_matching_entities(target, parent, &ents);
            if targets.is_empty() {
                commands.entity(id).despawn();
                continue;
            }

            egui::Window::new(window_title(target, "Axle"))
                .auto_sized()
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, commands| {
                    let enabled = component_checkbox(
                        ui,
                        commands,
                        &gui_icons,
                        &targets,
                        &ents,
                        |motor| motor.enabled,
                        |motor, value| motor.enabled = value,
                        "Motor",
                    )
                    .unwrap();

                    if !matches!(enabled, TriState::Off) {
                        component_checkbox(
                            ui,
                            commands,
                            &gui_icons,
                            &targets,
                            &ents,
                            |motor| motor.reversed,
                            |motor, value| motor.reversed = value,
                            "Reversed",
                        );
                        component_slider(
                            ui,
                            commands,
                            &targets,
                            &ents,
                            |motor| motor.vel,
                            |motor, value| motor.vel = value,
                            0.0..=450.0,
                            |slider| {
                                slider
                                    .logarithmic(true)
                                    .suffix("rpm")
                                    .smallest_positive(0.1)
                                    .text("Motor speed:")
                            },
                        );
                        component_slider(
                            ui,
                            commands,
                            &targets,
                            &ents,
                            |motor| motor.torque,
                            |motor, value| motor.torque = value,
                            0.1..=50000.0,
                            |slider| slider.logarithmic(true).suffix("Nm").text("Motor torque:"),
                        );
                    }

                    component_slider(
                        ui,
                        commands,
                        &targets,
                        &ents,
                        |motor| motor.break_limit,
                        |motor, value| motor.break_limit = value,
                        0.0..=f32::INFINITY,
                        |slider| {
                            slider
                                .logarithmic(true)
                                .suffix("Ns")
                                .smallest_positive(0.01)
                                .largest_finite(1000.0)
                                .text("Break limit:")
                        },
                    );
                });
        }
    }
}
