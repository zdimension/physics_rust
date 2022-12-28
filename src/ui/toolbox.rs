use crate::ui::icon_button::IconButton;
use crate::ui::separator_custom::SeparatorCustom;
use crate::ui::RemoveTemporaryWindowsEvent;
use crate::{ToolIcons, UiState};
use bevy::prelude::{EventWriter, Res, ResMut};
use bevy_egui::egui::Align2;
use bevy_egui::{egui, EguiContext};

pub fn draw_toolbox(
    mut egui_ctx: ResMut<EguiContext>,
    mut ui_state: ResMut<UiState>,
    tool_icons: Res<ToolIcons>,
    mut clear_tmp: EventWriter<RemoveTemporaryWindowsEvent>,
) {
    egui::Window::new("Tools")
        .anchor(Align2::LEFT_BOTTOM, [0.0, 0.0])
        .title_bar(false)
        .resizable(false)
        .default_size(egui::Vec2::ZERO)
        .show(egui_ctx.clone().ctx_mut(), |ui| {
            ui.vertical(|ui| {
                let ui_state = &mut *ui_state;
                for (i, category) in ui_state.toolbox.iter().enumerate() {
                    if i > 0 {
                        ui.add(SeparatorCustom::default().horizontal());
                    }
                    for chunk in category.chunks(2) {
                        ui.horizontal(|ui| {
                            for def in chunk {
                                if ui
                                    .add(
                                        IconButton::new(
                                            egui_ctx.add_image(def.icon(&tool_icons)),
                                            24.0,
                                        )
                                        .selected(ui_state.toolbox_selected.is_same(def)),
                                    )
                                    .clicked()
                                {
                                    ui_state.toolbox_selected = *def;
                                    clear_tmp.send(RemoveTemporaryWindowsEvent);
                                }
                            }
                        });
                    }
                }
            });
        });
}