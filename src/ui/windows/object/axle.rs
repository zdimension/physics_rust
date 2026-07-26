use crate::objects::MotorComponent;
use crate::ui::{InitialPos, Subwindow};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use crate::egui_systems;

egui_systems!(AxleWindow::show);

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
                                .text("Motor speed :")
                                .custom(),
                        );
                        ui.add(
                            egui::Slider::new(&mut motor.torque, 0.0..=50000.0)
                                .logarithmic(true)
                                .suffix("Nm")
                                .smallest_positive(0.1)
                                .text("Motor torque :")
                                .custom(),
                        );
                    }
                });
        }
    }
}
