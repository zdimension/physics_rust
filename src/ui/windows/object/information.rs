use crate::measures::{self, GravityData, GravityEnergy, KineticData, KineticEnergy};
use crate::objects::spring::SpringObject;
use crate::ui::{InitialPos, Subwindow};
use bevy::prelude::ChildOf;
use bevy::prelude::{Commands, Component, Entity, GlobalTransform, Query, Res, Transform, With};
use bevy_egui::egui::Ui;
use bevy_egui::{egui, EguiContexts};
use avian2d::{math::*, prelude::*};
use avian2d::{math::*, prelude::*};
use avian2d::{math::*, prelude::*};
use crate::egui_systems;

egui_systems!(InformationWindow::show);

#[derive(Default, Component)]
pub struct InformationWindow;

impl InformationWindow {
    pub fn show(
        mut wnds: Query<(Entity, &ChildOf, &mut InitialPos), With<InformationWindow>>,
        ents: Query<(
            Option<&Position>,
            Option<&ColliderMassProperties>,
            Option<&LinearVelocity>,
            Option<&AngularVelocity>,
            Option<KineticData>,
            Option<GravityData>,
            Option<&SpringObject>,
        )>,
        body_positions: Query<(&Position, &Rotation)>,
        gravity: Res<Gravity>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut().expect("primary egui context");
        for (id, parent, mut initial_pos) in wnds.iter_mut() {
            let (pos, coll_mass, linvel, angvel, kin, grav, spring) = ents.get(parent.parent()).unwrap();
            egui::Window::new("info").subwindow(
                id,
                ctx,
                &mut initial_pos,
                &mut commands,
                |ui, _commands| {
                    fn line(ui: &mut Ui, label: &'static str, val: String) {
                        ui.label(label);
                        ui.label(val);
                        ui.end_row();
                    }
                    egui::Grid::new("info grid").striped(true).show(ui, |ui| {
                        if let Some(cmp) = coll_mass {
                            line(ui, "Mass", format!("{:.3} kg", cmp.mass));

                            line(
                                ui,
                                "Moment of inertia",
                                format!("{:.3} kg·m²", cmp.angular_inertia),
                            );
                        }

                        if let Some(pos) = pos {
                            line(
                                ui,
                                "Position",
                                format!(
                                    "[x={:.3}, y={:.3}] m",
                                    pos.x, pos.y
                                ),
                            );
                        }

                        if let Some(vel) = linvel {
                            line(
                                ui,
                                "Velocity",
                                format!("[x={:.3}, y={:.3}] m/s", vel.0.x, vel.0.y),
                            );
                        }
                        if let Some(vel) = angvel {
                            line(ui, "Angular velocity", format!("{:.3} rad/s", vel.0));
                        }

                        if let Some(kin) = &kin {
                            let mom = kin.momentum();
                            line(ui, "Momentum", format!("[x={:.3}, y={:.3}] N⋅s", mom.linear.x, mom.linear.y));
                            line(ui, "Angular momentum", format!("{:.3} J⋅s", mom.angular));
                        }
                    });
                    ui.separator();
                    egui::Grid::new("info grid 2").striped(true).show(ui, |ui| {
                        let mut total = 0.0;
                        
                        if let Some(kin) = &kin {
                            let kine = kin.kinetic_energy();
                            line(ui, "Kinetic linear energy", format!("{:.3} J", kine.linear));
                            line(ui, "Kinetic angular energy", format!("{:.3} J", kine.angular));
                            total += kine.linear + kine.angular;
                        }
                        /*if let (Some(lin), Some(col), Some(ang)) = (linvel, coll_mass, angvel) {
                            let kine = measures::kinetic_energy(col, *lin, *ang);
                            line(ui, "Kinetic linear energy", format!("{:.3} J", kine.linear));
                            line(ui, "Kinetic angular energy", format!("{:.3} J", kine.angular));
                            total += kine.linear + kine.angular;
                        }*/

                        if let Some(grav) = grav {
                            let grav = grav.gravity_energy();
                            let pot = grav.energy;
                            line(ui, "Potential energy (gravity)", format!("{:.3} J", pot)); // todo: nonvertical gravity
                            total += pot;
                        }
                        if let Some(energy) = spring.and_then(|spring| spring.potential_energy(&body_positions)) {
                            line(ui, "Potential enery (spring)", format!("{:.3} J", energy));
                            total += energy;
                        }

                        line(ui, "Energy (total)", format!("{:.3} J", total));
                    });
                },
            );
        }
    }
}
