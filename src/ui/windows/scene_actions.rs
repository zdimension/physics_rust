use crate::palette::{PaletteConfig, PaletteList};
use crate::Despawn;
use bevy::prelude::*;
use bevy_egui::egui::Align2;
use bevy_egui::{egui, EguiContexts};

use crate::ui::icon_button::IconButton;
use crate::ui::images::GuiIcons;
use crate::ui::{BevyIdThing, InitialPos, RemoveTemporaryWindowsEvent, Subwindow, UiState};

pub fn draw_scene_actions(
    mut egui_ctx: EguiContexts,
    _ui_state: ResMut<UiState>,
    gui_icons: Res<GuiIcons>,
    _clear_tmp: EventWriter<RemoveTemporaryWindowsEvent>,
    mut commands: Commands,
) {
    egui::Window::new("Scene actions")
        .anchor(Align2::LEFT_TOP, [0.0, 64.0])
        .title_bar(false)
        .resizable(false)
        .default_size(egui::Vec2::ZERO)
        .show(&mut egui_ctx.ctx_mut().clone(), |ui| {
            ui.vertical(|ui| {
                let btn = ui.add(IconButton::new(gui_icons.new, 32.0));
                if btn.clicked() {
                    commands.spawn((NewSceneWindow, InitialPos::initial(btn.rect.right_top())));
                    info!("new");
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
                .id_bevy(id)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, commands| {
                    ui.vertical(|ui| {
                        for (name, palette) in assets
                            .get(&palette_config.palettes)
                            .unwrap()
                            .0
                            .iter()
                            .chain(std::iter::once((&"Default".into(), &Default::default())))
                        {
                            if ui.button(name).clicked() {
                                palette_config.current_palette = palette.clone();
                                commands.entity(ui_state.scene).insert(Despawn::Descendants);
                                commands.entity(id).despawn();
                            }
                        }
                    });
                });
        }
    }
}
