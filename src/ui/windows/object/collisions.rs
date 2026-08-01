use crate::egui_systems;
use crate::ui::images::GuiIcons;
use crate::ui::{
    InitialPos, Subwindow, WindowSelectionTarget, component_checkbox, edit_components,
    window_matching_entities,
};
use avian2d::prelude::*;
use bevy::prelude::{ChildOf, Commands, Component, Entity, Query, Res, With};
use bevy_egui::{EguiContexts, egui};

egui_systems!(CollisionsWindow::show);

#[derive(Default, Component)]
pub struct CollisionsWindow;

const GROUP_COUNT: usize = 10;

#[derive(Default)]
pub struct CollisionLayer(pub u32);

impl PhysicsLayer for CollisionLayer {
    fn to_bits(&self) -> u32 {
        self.0
    }

    fn all_bits() -> u32 {
        u32::MAX
    }
}

impl CollisionsWindow {
    pub fn show(
        mut wnds: Query<
            (
                Entity,
                Option<&ChildOf>,
                Option<&WindowSelectionTarget>,
                &mut InitialPos,
            ),
            With<CollisionsWindow>,
        >,
        ents: Query<&CollisionLayers>,
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

            egui::Window::new("Collisions").auto_sized().subwindow(
                id,
                ctx,
                &mut initial_pos,
                &mut commands,
                |ui, commands| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            if ui
                                .add(egui::Button::image(egui::load::SizedTexture::new(
                                    gui_icons.arrow_up,
                                    [16.0, 32.0],
                                )))
                                .clicked()
                            {
                                edit_components(commands, &targets, &ents, |groups| {
                                    let value = groups.memberships.0;
                                    let shifted = value >> 1;
                                    let new_value = shifted | ((value & 1) << (GROUP_COUNT - 1));
                                    *groups = CollisionLayers::from_bits(new_value, new_value);
                                });
                            }
                            if ui
                                .add(egui::Button::image(egui::load::SizedTexture::new(
                                    gui_icons.arrow_down,
                                    [16.0, 32.0],
                                )))
                                .clicked()
                            {
                                edit_components(commands, &targets, &ents, |groups| {
                                    let value = groups.memberships.0;
                                    let shifted = value << 1;
                                    let new_value = shifted
                                        | ((value & (1 << (GROUP_COUNT - 1))) >> (GROUP_COUNT - 1));
                                    *groups = CollisionLayers::from_bits(new_value, new_value);
                                });
                            }
                        });
                        ui.vertical(|ui| {
                            for i in 0..GROUP_COUNT {
                                let flag = 1 << i;
                                component_checkbox(
                                    ui,
                                    commands,
                                    &gui_icons,
                                    &targets,
                                    &ents,
                                    |groups| groups.memberships.0 & flag != 0,
                                    |groups, checked| {
                                        let new_value = if checked {
                                            groups.memberships.0 | flag
                                        } else {
                                            groups.memberships.0 & !flag
                                        };
                                        *groups = CollisionLayers::from_bits(new_value, new_value);
                                    },
                                    format!("Collision layer {}", (b'A' + i as u8) as char),
                                );
                            }
                        });
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Check all").clicked() {
                            edit_components(commands, &targets, &ents, |groups| {
                                *groups = CollisionLayers::ALL;
                            });
                        }
                        if ui.button("Uncheck all").clicked() {
                            edit_components(commands, &targets, &ents, |groups| {
                                *groups = CollisionLayers::NONE;
                            });
                        }
                    });
                },
            );
        }
    }
}
