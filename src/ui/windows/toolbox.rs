use crate::egui_systems;
use crate::mouse::select::SelectionConfig;
use crate::tools::drag::DragConfig;
use crate::tools::{ToolEnum, ToolIcons};
use crate::ui::icon_button::IconButton;
use crate::ui::images::GuiIcons;
use crate::ui::separator_custom::SeparatorCustom;
use crate::ui::{RemoveTemporaryWindowsEvent, ToolboxState, WindowExt, bool_checkbox};
use crate::update_changed;
use bevy::prelude::{MessageWriter, Res, ResMut};
use bevy_egui::egui::{Align2, Frame, Margin};
use bevy_egui::{EguiContexts, egui};

pub fn draw_toolbox(
    mut egui_ctx: EguiContexts,
    mut toolbox_state: ResMut<ToolboxState>,
    tool_icons: Res<ToolIcons>,
    gui_icons: Res<GuiIcons>,
    mut clear_tmp: MessageWriter<RemoveTemporaryWindowsEvent>,
    mut drag_config: ResMut<DragConfig>,
    mut selection_config: ResMut<SelectionConfig>,
) {
    let ctx = egui_ctx.ctx_mut().expect("primary egui context");
    let toolbox = egui::Window::new("Tools")
        .anchor(Align2::LEFT_BOTTOM, [1.0, -1.0])
        .title_bar(false)
        .resizable(false)
        .default_size(egui::Vec2::ZERO)
        .frame(Frame {
            inner_margin: Margin::same(3),
            ..Frame::window(ctx.global_style().as_ref())
        })
        .show_translucent(ctx, |ui| {
            ui.vertical(|ui| {
                ui.style_mut().spacing.item_spacing = egui::Vec2::new(1.0, 1.0);
                let toolbox_state = &mut *toolbox_state;
                for (i, category) in toolbox_state.toolbox.iter().enumerate() {
                    if i > 0 {
                        ui.add(SeparatorCustom::default().horizontal());
                    }
                    for chunk in category.chunks(2) {
                        ui.horizontal(|ui| {
                            for def in chunk {
                                if ui
                                    .add(
                                        IconButton::new(def.egui_icon(&tool_icons), 24.0)
                                            .dim_if_unselected(true)
                                            .selected(toolbox_state.toolbox_selected.is_same(def)),
                                    )
                                    .clicked()
                                {
                                    toolbox_state.toolbox_selected = def.clone();
                                    clear_tmp.write(RemoveTemporaryWindowsEvent);
                                }
                            }
                        });
                    }
                }
            });
        })
        .expect("Toolbox must be visible");

    egui::Window::new("Tool settings")
        .anchor(
            Align2::LEFT_BOTTOM,
            [toolbox.response.rect.width() + 2.0, -1.0],
        )
        .title_bar(false)
        .resizable(false)
        .default_size(egui::Vec2::ZERO)
        .frame(Frame {
            inner_margin: Margin::same(3),
            ..Frame::window(ctx.global_style().as_ref())
        })
        .show_translucent(ctx, |ui| {
            ui.vertical(|ui| {
                ui.style_mut().spacing.item_spacing = egui::Vec2::new(1.0, 1.0);
                use ToolEnum::*;
                'settings: {
                    match toolbox_state.toolbox_selected {
                        Drag(_) => {
                            bool_checkbox(
                                ui,
                                &gui_icons,
                                &mut drag_config.drag_center_of_mass,
                                "Drag center of mass",
                            );
                            update_changed!(ui, drag_config.strength, 1000.0..=1e8, |slider| {
                                slider.text("Drag strength:").logarithmic(true).custom()
                            });
                            update_changed!(
                                ui,
                                drag_config.max_force,
                                1.0..=f32::INFINITY,
                                |slider| {
                                    slider
                                        .text("Max force:")
                                        .logarithmic(true)
                                        .largest_finite(1e6)
                                        .custom()
                                }
                            );
                        }
                        Box(_) => {
                            bool_checkbox(
                                ui,
                                &gui_icons,
                                &mut selection_config.select_by_encircling,
                                "Select by encircling",
                            );
                        }
                        _ => {
                            break 'settings;
                        }
                    }
                    ui.add(SeparatorCustom::default().horizontal());
                }

                ui.label(toolbox_state.toolbox_selected.name());
            });
        });
}

egui_systems!(draw_toolbox);
