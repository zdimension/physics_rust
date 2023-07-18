use crate::palette::{PaletteConfig, PaletteList};
use crate::{ systems};
use bevy::prelude::*;
use bevy_egui::egui::Align2;
use bevy_egui::{egui, EguiContexts};

use crate::ui::icon_button::IconButton;
use crate::ui::images::GuiIcons;
use crate::ui::{InitialPos, Subwindow, UiState};

systems!(draw_scene_actions, NewSceneWindow::show);

pub fn draw_scene_actions(
    mut egui_ctx: EguiContexts,
    gui_icons: Res<GuiIcons>,
    mut commands: Commands,
    ns_window: Query<Entity, With<NewSceneWindow>>
) {
    egui::Window::new("Scene actions")
        .anchor(Align2::LEFT_TOP, [1.0, 36.0])
        .title_bar(false)
        .resizable(false)
        .default_size(egui::Vec2::ZERO)
        .show(&mut egui_ctx.ctx_mut().clone(), |ui| {
            ui.vertical(|ui| {
                let btn = ui.add(IconButton::new(gui_icons.new, 32.0));
                let ns = ns_window.get_single();
                if btn.clicked() {
                    match ns {
                        Ok(ent) => { commands.entity(ent).despawn_recursive(); }
                        Err(_) => { commands.spawn((NewSceneWindow, InitialPos::initial(btn.rect.right_top()))); }
                    }
                }
                if ui.add(IconButton::new(gui_icons.save, 32.0)).clicked() {}
                if ui.add(IconButton::new(gui_icons.open, 32.0)).clicked() {}
            });
        });
}

#[derive(Default, Component)]
pub struct NewSceneWindow;

impl NewSceneWindow {
    pub fn show(
        mut wnds: Query<(Entity, &mut InitialPos), With<NewSceneWindow>>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
        mut palette_config: ResMut<PaletteConfig>,
        assets: Res<Assets<PaletteList>>,
        ui_state: Res<UiState>,
    ) {
        let ctx = egui_ctx.ctx_mut();
        for (id, mut initial_pos) in wnds.iter_mut() {
            egui::Window::new("New scene")
                .resizable(false)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, commands| {
                    ui.style_mut().spacing.item_spacing = egui::Vec2::new(3.0, 3.0);
                    ui.vertical(|ui| {
                        for (name, palette) in assets
                            .get(&palette_config.palettes)
                            .unwrap()
                            .0
                            .iter()
                            .chain(std::iter::once((&"Default".into(), &Default::default())))
                        {
                            if ui.button(name).clicked() {
                                palette_config.current_palette = *palette;
                                commands.entity(ui_state.scene).despawn_descendants();
                                commands.entity(id).despawn();
                            }
                        }
                    });
                });
        }
    }
}
