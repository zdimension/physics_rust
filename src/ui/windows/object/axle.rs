use crate::objects::MotorComponent;
use crate::ui::{InitialPos, Subwindow};
use crate::egui_systems;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

egui_systems!(AxleWindow::show);

const DEFAULT_BREAK_LIMIT: f32 = 10.0;

#[derive(Default, Component)]
pub struct AxleWindow;

impl AxleWindow {
    pub fn show(
        mut wnds: Query<(Entity, &ChildOf, &mut InitialPos), With<AxleWindow>>,
        mut ents: Query<&mut MotorComponent>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut().expect("primary egui context");
        for (id, parent, mut initial_pos) in wnds.iter_mut() {
            let mut motor = ents.get_mut(parent.parent()).unwrap();
            egui::Window::new("Axle")
                .resizable(false)
                .default_size(egui::Vec2::ZERO)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, _commands| {
                    ui.checkbox(&mut motor.enabled, "Motor");
                    if motor.enabled {
                        ui.checkbox(&mut motor.reversed, "Reversed");
                        ui.add(
                            egui::Slider::new(&mut motor.vel, 0.0..=450.0)
                                .logarithmic(true)
                                .suffix("rpm")
                                .smallest_positive(0.1)
                                .text("Motor speed:")
                                .custom(),
                        );
                        ui.add(
                            egui::Slider::new(&mut motor.torque, 0.1..=50000.0)
                                .logarithmic(true)
                                .suffix("Nm")
                                .text("Motor torque:")
                                .custom(),
                        );
                    }

                    ui.add(
                        egui::Slider::new(&mut motor.break_limit, 0.0..=f32::INFINITY)
                            .logarithmic(true)
                            .suffix("Ns")
                            .smallest_positive(0.01)
                            .largest_finite(1000.0)
                            .text("Break limit:")
                            .custom(),
                    );
                });
        }
    }
}
