use crate::ui::images::GuiIcons;
use crate::ui::{BevyIdThing, InitialPos, Subwindow};
use bevy::hierarchy::Parent;
use bevy::prelude::{Commands, Component, Entity, Query, Res, ResMut, With};
use bevy_egui::{egui, EguiContext};
use bevy_rapier2d::geometry::{CollisionGroups, Group};

#[derive(Default, Component)]
pub struct CollisionsWindow;

const GROUP_COUNT: usize = 10;

impl CollisionsWindow {
    pub fn show(
        mut wnds: Query<(Entity, &Parent, &mut InitialPos), With<CollisionsWindow>>,
        mut ents: Query<&mut CollisionGroups>,
        gui_icons: Res<GuiIcons>,
        mut egui_ctx: ResMut<EguiContext>,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut();
        for (id, parent, mut initial_pos) in wnds.iter_mut() {
            let mut groups = ents.get_mut(parent.get()).unwrap();
            egui::Window::new("Collisions")
                .resizable(false)
                .id_bevy(id)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, _commands| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            // todo: center vertically
                            if ui
                                .add(egui::ImageButton::new(gui_icons.arrow_up, [16.0, 32.0]))
                                .clicked()
                            {
                                let val = groups.memberships.bits();
                                let shifted = val >> 1;
                                let new_val = shifted | ((val & 1) << (GROUP_COUNT - 1));
                                groups.memberships = Group::from_bits_truncate(new_val);
                                groups.filters = groups.memberships;
                            }
                            if ui
                                .add(egui::ImageButton::new(gui_icons.arrow_down, [16.0, 32.0]))
                                .clicked()
                            {
                                let val = groups.memberships.bits();
                                let shifted = val << 1;
                                let new_val = shifted
                                    | ((val & (1 << (GROUP_COUNT - 1))) >> (GROUP_COUNT - 1));
                                groups.memberships = Group::from_bits_truncate(new_val);
                                groups.filters = groups.memberships;
                            }
                        });
                        ui.vertical(|ui| {
                            for i in 0..GROUP_COUNT {
                                let flag = Group::from_bits_truncate(1 << i);
                                let mut checked = groups.memberships.contains(flag);
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
                                    groups.memberships.set(flag, checked);
                                    groups.filters = groups.memberships;
                                }
                            }
                        });
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Check all").clicked() {
                            groups.memberships = Group::from_bits_truncate((1 << GROUP_COUNT) - 1);
                            groups.filters = groups.memberships;
                        }
                        if ui.button("Uncheck all").clicked() {
                            groups.memberships = Group::empty();
                            groups.filters = groups.memberships;
                        }
                    });
                });
        }
    }
}
