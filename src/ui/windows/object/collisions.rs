use crate::ui::images::GuiIcons;
use crate::ui::{InitialPos, Subwindow};
use bevy::hierarchy::Parent;
use bevy::prelude::{Commands, Component, Entity, Query, Res, With};
use bevy_egui::{egui, EguiContexts};
use bevy_xpbd_2d::{math::*, prelude::*};
use crate::systems;

systems!(CollisionsWindow::show);

#[derive(Default, Component)]
pub struct CollisionsWindow;

const GROUP_COUNT: usize = 10;

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
        mut wnds: Query<(Entity, &Parent, &mut InitialPos), With<CollisionsWindow>>,
        mut ents: Query<&mut CollisionLayers>,
        gui_icons: Res<GuiIcons>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut();
        for (id, parent, mut initial_pos) in wnds.iter_mut() {
            let mut groups = ents.get_mut(parent.get()).unwrap();
            egui::Window::new("Collisions")
                .resizable(false)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, _commands| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            // todo: center vertically
                            if ui
                                .add(egui::ImageButton::new(gui_icons.arrow_up, [16.0, 32.0]))
                                .clicked()
                            {
                                let val = groups.groups_bits();
                                let shifted = val >> 1;
                                let new_val = shifted | ((val & 1) << (GROUP_COUNT - 1));
                                *groups = CollisionLayers::from_bits(new_val, new_val);
                            }
                            if ui
                                .add(egui::ImageButton::new(gui_icons.arrow_down, [16.0, 32.0]))
                                .clicked()
                            {
                                let val = groups.groups_bits();
                                let shifted = val << 1;
                                let new_val = shifted
                                    | ((val & (1 << (GROUP_COUNT - 1))) >> (GROUP_COUNT - 1));
                                *groups = CollisionLayers::from_bits(new_val, new_val);
                            }
                        });
                        ui.vertical(|ui| {
                            for i in 0..GROUP_COUNT {
                                let flag = 1 << i;
                                let mut checked = groups.groups_bits() & flag != 0;
                                if ui
                                    .checkbox(
                                        &mut checked,
                                        format!(
                                            "Collision layer {}",
                                            ('A' as u8 + i as u8) as char
                                        ),
                                    )
                                    .changed()
                                {
                                    let new_val = if checked {
                                        groups.groups_bits() | flag
                                    } else {
                                        groups.groups_bits() & !flag
                                    };
                                    *groups = CollisionLayers::from_bits(new_val, new_val);
                                }
                            }
                        });
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Check all").clicked() {
                            *groups = CollisionLayers::all::<CollisionLayer>();
                        }
                        if ui.button("Uncheck all").clicked() {
                            *groups = CollisionLayers::none();
                        }
                    });
                });
        }
    }
}
