use crate::tools::ToolIcons;
use crate::ui::icon_button::IconButton;
use crate::ui::separator_custom::SeparatorCustom;
use crate::ui::{RemoveTemporaryWindowsEvent, UiState};
use bevy::prelude::{MessageWriter, Res, ResMut};
use bevy_egui::egui::{Align2, Frame, Margin};
use bevy_egui::{egui, EguiContexts};
use crate::systems;

pub fn draw_toolbox(
    mut egui_ctx: EguiContexts,
    mut ui_state: ResMut<UiState>,
    tool_icons: Res<ToolIcons>,
    mut clear_tmp: MessageWriter<RemoveTemporaryWindowsEvent>,
) {
    let ctx = egui_ctx.ctx_mut().expect("primary egui context");
    egui::Window::new("Tools")
        .anchor(Align2::LEFT_BOTTOM, [1.0, -1.0])
        .title_bar(false)
        .resizable(false)
        .default_size(egui::Vec2::ZERO)
        .frame(Frame {
            inner_margin: Margin::same(3),
            ..Frame::window(ctx.style().as_ref())
        })
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.style_mut().spacing.item_spacing = egui::Vec2::new(1.0, 1.0);
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
                                            def.egui_icon(&tool_icons),
                                            24.0,
                                        )
                                            .dim_if_unselected(true)
                                        .selected(ui_state.toolbox_selected.is_same(def)),
                                    )
                                    .clicked()
                                {
                                    ui_state.toolbox_selected = *def;
                                    clear_tmp.write(RemoveTemporaryWindowsEvent);
                                }
                            }
                        });
                    }
                }
            });
        });
}

systems!(draw_toolbox);