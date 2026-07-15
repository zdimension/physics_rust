use crate::ui::images::GuiIcons;
use crate::ui::{InitialPos, Subwindow};
use bevy::prelude::ChildOf;
use bevy::prelude::{Commands, Component, Entity, Query, Res, With};
use bevy_egui::{egui, EguiContexts};
use egui::load::SizedTexture;
use avian2d::{math::*, prelude::*};
use crate::egui_systems;

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
        mut wnds: Query<(Entity, &ChildOf, &mut InitialPos), With<CollisionsWindow>>,
        ents: Query<&CollisionLayers>,
        gui_icons: Res<GuiIcons>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut().expect("primary egui context");
        for (id, parent, mut initial_pos) in wnds.iter_mut() {
            let mut groups = *ents.get(parent.parent()).unwrap();
            let mut changed = false;
            egui::Window::new("Collisions")
                .resizable(false)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, _commands| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            // todo: center vertically
                            if ui
                                .add(egui::ImageButton::new(SizedTexture::new(gui_icons.arrow_up, [16.0, 32.0])))
                                .clicked()
                            {
                                let val = groups.memberships.0;
                                let shifted = val >> 1;
                                let new_val = shifted | ((val & 1) << (GROUP_COUNT - 1));
                                groups = CollisionLayers::from_bits(new_val, new_val);
                                changed = true;
                            }
                            if ui
                                .add(egui::ImageButton::new(SizedTexture::new(gui_icons.arrow_down, [16.0, 32.0])))
                                .clicked()
                            {
                                let val = groups.memberships.0;
                                let shifted = val << 1;
                                let new_val = shifted
                                    | ((val & (1 << (GROUP_COUNT - 1))) >> (GROUP_COUNT - 1));
                                groups = CollisionLayers::from_bits(new_val, new_val);
                                changed = true;
                            }
                        });
                        ui.vertical(|ui| {
                            for i in 0..GROUP_COUNT {
                                let flag = 1 << i;
                                let mut checked = groups.memberships.0 & flag != 0;
                                if ui
                                    .checkbox(
                                        &mut checked,
                                        format!(
                                            "Collision layer {}",
                                            (b'A' + i as u8) as char
                                        ),
                                    )
                                    .changed()
                                {
                                    let new_val = if checked {
                                        groups.memberships.0 | flag
                                    } else {
                                        groups.memberships.0 & !flag
                                    };
                                    groups = CollisionLayers::from_bits(new_val, new_val);
                                    changed = true;
                                }
                            }
                        });
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Check all").clicked() {
                            groups = CollisionLayers::ALL;
                            changed = true;
                        }
                        if ui.button("Uncheck all").clicked() {
                            groups = CollisionLayers::NONE;
                            changed = true;
                        }
                    });
                });
            if changed {
                commands.entity(parent.parent()).insert(groups);
            }
        }
    }
}
