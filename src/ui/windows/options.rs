use crate::palette::{PaletteConfig, PaletteList};
use crate::{Despawn, systems};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use crate::ui::{BevyIdThing, InitialPos, Subwindow, UiState};systems!(OptionsWindow::show);

#[derive(Default, Component)]
pub struct OptionsWindow;

impl OptionsWindow {
    pub fn show(
        mut wnds: Query<(Entity, &mut InitialPos), With<OptionsWindow>>,
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
